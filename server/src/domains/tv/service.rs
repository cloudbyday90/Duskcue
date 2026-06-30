// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use sqlx::Row;
use uuid::Uuid;

use super::error::TvError;
use super::types::*;
use crate::extractors::AuthenticatedUser;

const DEFAULT_LIMIT: u32 = 30;
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Clone)]
pub struct TvAccessScope {
    pub user_id: Uuid,
    pub has_all_library_access: bool,
    pub library_ids: Vec<Uuid>,
}

impl TvAccessScope {
    fn can_access_library(&self, library_id: Uuid) -> bool {
        self.has_all_library_access || self.library_ids.contains(&library_id)
    }
}

pub async fn load_tv_access_scope(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
) -> Result<TvAccessScope, TvError> {
    let library_ids = if user.has_all_library_access {
        Vec::new()
    } else {
        let rows = sqlx::query(ACCESSIBLE_LIBRARY_IDS_SQL)
            .bind(user.user_id)
            .fetch_all(pool)
            .await?;

        rows.iter()
            .map(|row| row.try_get("library_id"))
            .collect::<Result<Vec<Uuid>, sqlx::Error>>()?
    };

    Ok(TvAccessScope {
        user_id: user.user_id,
        has_all_library_access: user.has_all_library_access,
        library_ids,
    })
}

const LOOKUP_PLATFORM_CONTENT_SQL: &str = r#"
SELECT mi.id,
       mi.type,
       mi.library_id
FROM media_items mi
JOIN libraries l ON l.id = mi.library_id
WHERE mi.id = $1 AND l.deleted_at IS NULL
"#;

const ACCESSIBLE_LIBRARY_IDS_SQL: &str = r#"
SELECT ula.library_id
FROM user_library_access ula
JOIN libraries l ON l.id = ula.library_id
WHERE ula.user_id = $1 AND l.deleted_at IS NULL
"#;

const CONTINUE_WATCHING_SQL: &str = r#"
SELECT mi.id,
       mi.type,
       mi.title,
       mi.overview,
       mi.premiere_date,
       mi.runtime_seconds,
       uid.resume_position_ms,
       uid.last_played_at AS last_engaged_at,
       sn.season_number,
       ep.episode_number,
       series_mi.title AS series_title,
       COALESCE(mf.file_count, 0) AS file_count
FROM user_item_data uid
JOIN media_items mi ON mi.id = uid.media_item_id
JOIN libraries l ON l.id = mi.library_id
LEFT JOIN episodes ep ON ep.id = mi.id
LEFT JOIN seasons sn ON sn.id = ep.season_id
LEFT JOIN media_items series_mi ON series_mi.id = ep.series_id
LEFT JOIN LATERAL (
    SELECT count(*) AS file_count
    FROM media_files mf
    WHERE mf.media_item_id = mi.id AND mf.is_healthy = true
) mf ON true
WHERE uid.user_id = $1
  AND mi.type IN ('movie', 'episode')
  AND l.deleted_at IS NULL
  AND uid.is_watched = false
  AND uid.resume_position_ms >= 60000
  AND uid.last_played_at IS NOT NULL
  AND ($2::bool OR mi.library_id = ANY($3::uuid[]))
  AND EXISTS (
      SELECT 1
      FROM media_files mf_ok
      WHERE mf_ok.media_item_id = mi.id AND mf_ok.is_healthy = true
  )
ORDER BY uid.last_played_at DESC, mi.sort_title ASC
LIMIT $4
"#;

const NEXT_UP_SQL: &str = r#"
WITH latest_watched AS (
    SELECT DISTINCT ON (ep.series_id)
           ep.series_id,
           sn.season_number,
           ep.episode_number,
           uid.last_played_at
    FROM user_item_data uid
    JOIN episodes ep ON ep.id = uid.media_item_id
    JOIN seasons sn ON sn.id = ep.season_id
    JOIN media_items mi ON mi.id = uid.media_item_id
    JOIN libraries l ON l.id = mi.library_id
    WHERE uid.user_id = $1
      AND uid.is_watched = true
      AND l.deleted_at IS NULL
      AND ($2::bool OR mi.library_id = ANY($3::uuid[]))
      AND EXISTS (
          SELECT 1
          FROM media_files mf_ok
          WHERE mf_ok.media_item_id = mi.id AND mf_ok.is_healthy = true
      )
    ORDER BY ep.series_id,
             sn.season_number DESC NULLS LAST,
             ep.episode_number DESC NULLS LAST,
             uid.last_played_at DESC NULLS LAST
),
next_episode AS (
    SELECT lw.series_id,
           lw.last_played_at,
           candidate.id AS media_item_id
    FROM latest_watched lw
    JOIN LATERAL (
        SELECT ep.id
        FROM episodes ep
        JOIN seasons sn ON sn.id = ep.season_id
        JOIN media_items mi ON mi.id = ep.id
        JOIN libraries l ON l.id = mi.library_id
        LEFT JOIN user_item_data uid_next
               ON uid_next.user_id = $1 AND uid_next.media_item_id = ep.id
        WHERE ep.series_id = lw.series_id
          AND l.deleted_at IS NULL
          AND COALESCE(uid_next.is_watched, false) = false
          AND (
              COALESCE(sn.season_number, -1) > COALESCE(lw.season_number, -1)
              OR (
                  COALESCE(sn.season_number, -1) = COALESCE(lw.season_number, -1)
                  AND COALESCE(ep.episode_number, -1) > COALESCE(lw.episode_number, -1)
              )
          )
          AND ($2::bool OR mi.library_id = ANY($3::uuid[]))
          AND EXISTS (
              SELECT 1
              FROM media_files mf_ok
              WHERE mf_ok.media_item_id = mi.id AND mf_ok.is_healthy = true
          )
        ORDER BY sn.season_number ASC NULLS LAST,
                 ep.episode_number ASC NULLS LAST,
                 mi.sort_title ASC
        LIMIT 1
    ) candidate ON true
)
SELECT mi.id,
       mi.type,
       mi.title,
       mi.overview,
       mi.premiere_date,
       mi.runtime_seconds,
       0::int AS resume_position_ms,
       ne.last_played_at AS last_engaged_at,
       sn.season_number,
       ep.episode_number,
       series_mi.title AS series_title,
       COALESCE(mf.file_count, 0) AS file_count
FROM next_episode ne
JOIN media_items mi ON mi.id = ne.media_item_id
JOIN episodes ep ON ep.id = mi.id
JOIN seasons sn ON sn.id = ep.season_id
JOIN media_items series_mi ON series_mi.id = ep.series_id
LEFT JOIN LATERAL (
    SELECT count(*) AS file_count
    FROM media_files mf
    WHERE mf.media_item_id = mi.id AND mf.is_healthy = true
) mf ON true
ORDER BY ne.last_played_at DESC NULLS LAST, series_mi.sort_title ASC
LIMIT $4
"#;

const NEW_EPISODES_SQL: &str = r#"
WITH started_series AS (
    SELECT DISTINCT ep.series_id
    FROM user_item_data uid
    JOIN episodes ep ON ep.id = uid.media_item_id
    WHERE uid.user_id = $1
),
latest_per_series AS (
    SELECT DISTINCT ON (ep.series_id)
           mi.id,
           mi.type,
           mi.title,
           mi.overview,
           mi.premiere_date,
           mi.runtime_seconds,
           0::int AS resume_position_ms,
           mi.created_at AS last_engaged_at,
           sn.season_number,
           ep.episode_number,
           series_mi.title AS series_title,
           COALESCE(mf.file_count, 0) AS file_count
    FROM started_series ss
    JOIN episodes ep ON ep.series_id = ss.series_id
    JOIN seasons sn ON sn.id = ep.season_id
    JOIN media_items mi ON mi.id = ep.id
    JOIN libraries l ON l.id = mi.library_id
    JOIN media_items series_mi ON series_mi.id = ep.series_id
    LEFT JOIN user_item_data uid_seen
           ON uid_seen.user_id = $1 AND uid_seen.media_item_id = mi.id
    LEFT JOIN LATERAL (
        SELECT count(*) AS file_count
        FROM media_files mf
        WHERE mf.media_item_id = mi.id AND mf.is_healthy = true
    ) mf ON true
    WHERE l.deleted_at IS NULL
      AND COALESCE(uid_seen.is_watched, false) = false
      AND ($2::bool OR mi.library_id = ANY($3::uuid[]))
      AND EXISTS (
          SELECT 1
          FROM media_files mf_ok
          WHERE mf_ok.media_item_id = mi.id AND mf_ok.is_healthy = true
      )
    ORDER BY ep.series_id,
             mi.premiere_date DESC NULLS LAST,
             mi.created_at DESC,
             sn.season_number DESC NULLS LAST,
             ep.episode_number DESC NULLS LAST
)
SELECT *
FROM latest_per_series
ORDER BY premiere_date DESC NULLS LAST, last_engaged_at DESC
LIMIT $4
"#;

const RECOMMENDED_SQL: &str = r#"
WITH recent_items AS (
    SELECT uid.media_item_id
    FROM user_item_data uid
    JOIN media_items mi ON mi.id = uid.media_item_id
    JOIN libraries l ON l.id = mi.library_id
    WHERE uid.user_id = $1
      AND uid.last_played_at IS NOT NULL
      AND mi.type IN ('movie', 'episode')
      AND l.deleted_at IS NULL
      AND ($2::bool OR mi.library_id = ANY($3::uuid[]))
      AND EXISTS (
          SELECT 1
          FROM media_files mf_ok
          WHERE mf_ok.media_item_id = mi.id AND mf_ok.is_healthy = true
      )
    ORDER BY uid.last_played_at DESC
    LIMIT 50
),
preferred_genres AS (
    SELECT mg.genre_id, count(*)::int AS weight
    FROM recent_items ri
    JOIN media_genres mg ON mg.media_item_id = ri.media_item_id
    GROUP BY mg.genre_id
),
preferred_tags AS (
    SELECT mt.tag_id, count(*)::int AS weight
    FROM recent_items ri
    JOIN media_tags mt ON mt.media_item_id = ri.media_item_id
    GROUP BY mt.tag_id
),
preferred_people AS (
    SELECT mc.person_id, count(*)::int AS weight
    FROM recent_items ri
    JOIN media_credits mc ON mc.media_item_id = ri.media_item_id
    WHERE mc.credit_type IN ('cast', 'crew') AND mc."order" <= 5
    GROUP BY mc.person_id
),
collection_scores AS (
    SELECT ci.media_item_id, (count(*)::int * 5) AS score
    FROM collection_items ci
    JOIN collections c ON c.id = ci.collection_id
    WHERE c.is_enabled = true
      AND ci.is_missing = false
    GROUP BY ci.media_item_id
),
candidate_scores AS (
    SELECT mi.id,
           COALESCE(cs.score, 0)
           + COALESCE(genre_score.score, 0)
           + COALESCE(tag_score.score, 0)
           + COALESCE(people_score.score, 0) AS recommendation_score
    FROM media_items mi
    LEFT JOIN collection_scores cs ON cs.media_item_id = mi.id
    LEFT JOIN LATERAL (
        SELECT sum(pg.weight * 3)::int AS score
        FROM media_genres mg
        JOIN preferred_genres pg ON pg.genre_id = mg.genre_id
        WHERE mg.media_item_id = mi.id
    ) genre_score ON true
    LEFT JOIN LATERAL (
        SELECT sum(pt.weight * 2)::int AS score
        FROM media_tags mt
        JOIN preferred_tags pt ON pt.tag_id = mt.tag_id
        WHERE mt.media_item_id = mi.id
    ) tag_score ON true
    LEFT JOIN LATERAL (
        SELECT sum(pp.weight)::int AS score
        FROM media_credits mc
        JOIN preferred_people pp ON pp.person_id = mc.person_id
        WHERE mc.media_item_id = mi.id
          AND mc.credit_type IN ('cast', 'crew')
          AND mc."order" <= 5
    ) people_score ON true
)
SELECT mi.id,
       mi.type,
       mi.title,
       mi.overview,
       mi.premiere_date,
       mi.runtime_seconds,
       0::int AS resume_position_ms,
       mi.created_at AS last_engaged_at,
       sn.season_number,
       ep.episode_number,
       series_mi.title AS series_title,
       COALESCE(mf.file_count, 0) AS file_count,
       COALESCE(cand.recommendation_score, 0) AS recommendation_score
FROM media_items mi
JOIN candidate_scores cand ON cand.id = mi.id
JOIN libraries l ON l.id = mi.library_id
LEFT JOIN episodes ep ON ep.id = mi.id
LEFT JOIN seasons sn ON sn.id = ep.season_id
LEFT JOIN media_items series_mi ON series_mi.id = ep.series_id
LEFT JOIN user_item_data uid_seen
       ON uid_seen.user_id = $1 AND uid_seen.media_item_id = mi.id
LEFT JOIN LATERAL (
    SELECT count(*) AS file_count
    FROM media_files mf
    WHERE mf.media_item_id = mi.id AND mf.is_healthy = true
) mf ON true
WHERE mi.type IN ('movie', 'episode')
  AND l.deleted_at IS NULL
  AND COALESCE(uid_seen.is_watched, false) = false
  AND ($2::bool OR mi.library_id = ANY($3::uuid[]))
  AND EXISTS (
      SELECT 1
      FROM media_files mf_ok
      WHERE mf_ok.media_item_id = mi.id AND mf_ok.is_healthy = true
  )
ORDER BY COALESCE(cand.recommendation_score, 0) DESC,
         COALESCE(mi.rating_average, 0) DESC,
         mi.premiere_date DESC NULLS LAST,
         mi.sort_title ASC
LIMIT $4
"#;

pub fn resolve_surface_query(query: TvSurfaceQuery) -> Result<ResolvedTvSurfaceQuery, TvError> {
    let platform = query.platform.as_deref().map(parse_platform).transpose()?;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(TvError::InvalidLimit(limit));
    }
    let sections = parse_sections(query.sections.as_deref())?;

    Ok(ResolvedTvSurfaceQuery {
        platform,
        limit,
        sections,
    })
}

pub fn empty_surface_response(query: &ResolvedTvSurfaceQuery) -> TvSurfaceResponse {
    TvSurfaceResponse {
        generated_at: Utc::now(),
        platform: query.platform,
        limit: query.limit,
        sections: query
            .sections
            .iter()
            .copied()
            .map(|section_type| TvSurfaceSectionResponse {
                section_type,
                title: section_title(section_type).to_string(),
                empty_reason: Some("tv_surface_service_not_populated".to_string()),
                items: Vec::new(),
            })
            .collect(),
    }
}

pub async fn get_tv_surface(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
    query: &ResolvedTvSurfaceQuery,
) -> Result<TvSurfaceResponse, TvError> {
    let access_scope = load_tv_access_scope(pool, user).await?;
    let mut remaining = query.limit as usize;
    let mut sections = Vec::with_capacity(query.sections.len());

    for section_type in &query.sections {
        let items = if remaining == 0 {
            Vec::new()
        } else {
            fetch_surface_items(pool, &access_scope, *section_type, remaining).await?
        };

        remaining = remaining.saturating_sub(items.len());
        let empty_reason = if items.is_empty() {
            if remaining == 0 {
                Some("limit_reached".to_string())
            } else {
                Some("no_matching_items".to_string())
            }
        } else {
            None
        };

        sections.push(TvSurfaceSectionResponse {
            section_type: *section_type,
            title: section_title(*section_type).to_string(),
            empty_reason,
            items,
        });
    }

    let generated_at = surface_generated_at(&sections);

    Ok(TvSurfaceResponse {
        generated_at,
        platform: query.platform,
        limit: query.limit,
        sections,
    })
}

pub fn default_settings() -> TvSurfaceSettingsResponse {
    TvSurfaceSettingsResponse {
        tv_publication_enabled: true,
        enabled_platforms: vec![
            TvPlatform::AndroidTv,
            TvPlatform::GoogleTv,
            TvPlatform::FireTv,
            TvPlatform::Roku,
            TvPlatform::Tizen,
            TvPlatform::Webos,
            TvPlatform::Tvos,
            TvPlatform::Xbox,
        ],
        publish_continue_watching: true,
        publish_next_up: true,
        publish_new_episodes: true,
        publish_recommendations: true,
    }
}

async fn fetch_surface_items(
    pool: &sqlx::PgPool,
    access_scope: &TvAccessScope,
    section_type: TvSurfaceSectionType,
    limit: usize,
) -> Result<Vec<TvSurfaceItemResponse>, TvError> {
    let sql = match section_type {
        TvSurfaceSectionType::Continue => CONTINUE_WATCHING_SQL,
        TvSurfaceSectionType::NextUp => NEXT_UP_SQL,
        TvSurfaceSectionType::NewEpisodes => NEW_EPISODES_SQL,
        TvSurfaceSectionType::Recommended => RECOMMENDED_SQL,
    };

    let rows = sqlx::query(sql)
        .bind(access_scope.user_id)
        .bind(access_scope.has_all_library_access)
        .bind(&access_scope.library_ids)
        .bind(limit as i64)
        .fetch_all(pool)
        .await?;

    rows.iter()
        .map(|row| row_to_surface_item(row, section_type))
        .collect()
}

fn row_to_surface_item(
    row: &sqlx::postgres::PgRow,
    section_type: TvSurfaceSectionType,
) -> Result<TvSurfaceItemResponse, TvError> {
    let media_item_id: Uuid = row.try_get("id")?;
    let media_type_raw: String = row.try_get("type")?;
    let media_type = match media_type_raw.as_str() {
        "movie" => TvMediaType::Movie,
        "episode" => TvMediaType::Episode,
        _ => return Err(TvError::InvalidPlatformContentId(media_type_raw)),
    };
    let title: String = row.try_get("title")?;
    let overview: Option<String> = row.try_get("overview")?;
    let premiere_date: Option<NaiveDate> = row.try_get("premiere_date")?;
    let runtime_seconds: Option<i32> = row.try_get("runtime_seconds")?;
    let resume_position_ms: i32 = row.try_get("resume_position_ms")?;
    let last_engaged_at: Option<DateTime<Utc>> = row.try_get("last_engaged_at")?;
    let season_number: Option<i32> = row.try_get("season_number")?;
    let episode_number: Option<i32> = row.try_get("episode_number")?;
    let series_title: Option<String> = row.try_get("series_title")?;
    let file_count: i64 = row.try_get("file_count")?;

    let platform_content_id = build_platform_content_id(media_type, media_item_id);
    let duration_ms = runtime_seconds.map(|seconds| i64::from(seconds) * 1000);
    let progress_percent = duration_ms
        .filter(|duration| *duration > 0)
        .map(|duration| ((f64::from(resume_position_ms) / duration as f64) * 100.0).min(100.0))
        .unwrap_or(0.0);

    Ok(TvSurfaceItemResponse {
        surface_item_id: format!(
            "tv:{}:{}",
            section_slug(section_type),
            encode_platform_content_id(
                &PlatformContentId {
                    media_type,
                    media_item_id,
                },
                TvPlatformIdTarget::RokuFeed,
            )
        ),
        platform_content_id,
        media_item_id,
        media_type,
        section_type,
        title,
        subtitle: item_subtitle(
            media_type,
            series_title,
            season_number,
            episode_number,
            premiere_date,
        ),
        description: overview,
        season_number,
        episode_number,
        duration_ms,
        resume_position_ms: i64::from(resume_position_ms),
        progress_percent,
        last_engaged_at,
        poster_url: Some(format!("/api/v1/items/{media_item_id}/artwork/poster")),
        backdrop_url: Some(format!("/api/v1/items/{media_item_id}/artwork/backdrop")),
        deep_link: format!(
            "duskcue://play/{}/{}",
            media_type_slug(media_type),
            media_item_id
        ),
        web_url: format!("/media/{media_item_id}"),
        availability: if file_count > 0 {
            TvAvailabilityState::Playable
        } else {
            TvAvailabilityState::MissingFile
        },
    })
}

fn item_subtitle(
    media_type: TvMediaType,
    series_title: Option<String>,
    season_number: Option<i32>,
    episode_number: Option<i32>,
    premiere_date: Option<NaiveDate>,
) -> Option<String> {
    match media_type {
        TvMediaType::Movie => premiere_date.map(|date| date.year().to_string()),
        TvMediaType::Episode => {
            let episode_label = match (season_number, episode_number) {
                (Some(season), Some(episode)) => Some(format!("S{season:02}E{episode:02}")),
                (Some(season), None) => Some(format!("Season {season}")),
                (None, Some(episode)) => Some(format!("Episode {episode}")),
                (None, None) => None,
            };

            match (series_title, episode_label) {
                (Some(series), Some(label)) => Some(format!("{series} {label}")),
                (Some(series), None) => Some(series),
                (None, Some(label)) => Some(label),
                (None, None) => None,
            }
        }
    }
}

fn surface_generated_at(sections: &[TvSurfaceSectionResponse]) -> DateTime<Utc> {
    sections
        .iter()
        .flat_map(|section| section.items.iter())
        .filter_map(|item| item.last_engaged_at.as_ref())
        .max()
        .cloned()
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).expect("valid Unix epoch"))
}

pub fn empty_diagnostics(platform: Option<TvPlatform>) -> TvDiagnosticsResponse {
    TvDiagnosticsResponse {
        generated_at: Utc::now(),
        platform,
        candidate_count: 0,
        included_count: 0,
        excluded: Vec::new(),
    }
}

pub async fn lookup_platform_content(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
    value: &str,
) -> Result<TvPlatformContentLookup, TvError> {
    let parsed = parse_platform_content_id(value)?;
    let access_scope = load_tv_access_scope(pool, user).await?;
    let row = sqlx::query(LOOKUP_PLATFORM_CONTENT_SQL)
        .bind(parsed.media_item_id)
        .fetch_optional(pool)
        .await?
        .ok_or(TvError::UnavailableContent)?;

    let actual_media_type: String = row.try_get("type")?;
    if validate_platform_content_type(&parsed, &actual_media_type).is_err() {
        return Err(TvError::UnavailableContent);
    }

    let library_id: Uuid = row.try_get("library_id")?;
    let has_access = access_scope.can_access_library(library_id);
    let access_status = if has_access {
        TvContentAccessStatus::Accessible
    } else {
        TvContentAccessStatus::AccessDenied
    };

    Ok(TvPlatformContentLookup {
        platform_content_id: build_platform_content_id(parsed.media_type, parsed.media_item_id),
        media_item_id: parsed.media_item_id,
        media_type: parsed.media_type,
        library_id,
        access_status,
    })
}

pub fn build_platform_content_id(media_type: TvMediaType, media_item_id: Uuid) -> String {
    format!("duskcue:{}:{media_item_id}", media_type_slug(media_type))
}

pub fn encode_platform_content_id(id: &PlatformContentId, target: TvPlatformIdTarget) -> String {
    match target {
        TvPlatformIdTarget::Canonical => build_platform_content_id(id.media_type, id.media_item_id),
        TvPlatformIdTarget::RokuFeed | TvPlatformIdTarget::AmazonCatalog => {
            format!(
                "duskcue_{}_{}",
                media_type_slug(id.media_type),
                id.media_item_id.as_simple()
            )
        }
        TvPlatformIdTarget::UrlPath | TvPlatformIdTarget::UrlQuery => {
            urlencoding::encode(&build_platform_content_id(id.media_type, id.media_item_id))
                .into_owned()
        }
    }
}

pub fn decode_platform_content_id(
    value: &str,
    target: TvPlatformIdTarget,
) -> Result<PlatformContentId, TvError> {
    match target {
        TvPlatformIdTarget::Canonical => parse_platform_content_id(value),
        TvPlatformIdTarget::RokuFeed | TvPlatformIdTarget::AmazonCatalog => {
            parse_strict_platform_content_id(value)
        }
        TvPlatformIdTarget::UrlPath | TvPlatformIdTarget::UrlQuery => {
            let decoded = urlencoding::decode(value)
                .map_err(|_| TvError::InvalidPlatformContentId(value.to_string()))?;
            parse_platform_content_id(decoded.as_ref())
        }
    }
}

pub fn parse_platform(value: &str) -> Result<TvPlatform, TvError> {
    match value {
        "android_tv" => Ok(TvPlatform::AndroidTv),
        "google_tv" => Ok(TvPlatform::GoogleTv),
        "fire_tv" => Ok(TvPlatform::FireTv),
        "roku" => Ok(TvPlatform::Roku),
        "tizen" => Ok(TvPlatform::Tizen),
        "webos" => Ok(TvPlatform::Webos),
        "tvos" => Ok(TvPlatform::Tvos),
        "xbox" => Ok(TvPlatform::Xbox),
        other => Err(TvError::InvalidPlatform(other.to_string())),
    }
}

pub fn parse_sections(value: Option<&str>) -> Result<Vec<TvSurfaceSectionType>, TvError> {
    let Some(value) = value else {
        return Ok(default_sections());
    };
    if value.trim().is_empty() {
        return Ok(default_sections());
    }

    value
        .split(',')
        .map(|part| parse_section(part.trim()))
        .collect()
}

pub fn parse_platform_content_id(value: &str) -> Result<PlatformContentId, TvError> {
    let mut parts = value.split(':');
    let namespace = parts.next();
    let media_type = parts.next();
    let media_item_id = parts.next();

    if namespace != Some("duskcue") || parts.next().is_some() {
        return Err(TvError::InvalidPlatformContentId(value.to_string()));
    }

    let media_type = match media_type {
        Some("movie") => TvMediaType::Movie,
        Some("episode") => TvMediaType::Episode,
        _ => return Err(TvError::InvalidPlatformContentId(value.to_string())),
    };
    let media_item_id = media_item_id
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| TvError::InvalidPlatformContentId(value.to_string()))?;

    Ok(PlatformContentId {
        media_type,
        media_item_id,
    })
}

pub fn validate_platform_content_type(
    parsed: &PlatformContentId,
    actual_media_type: &str,
) -> Result<(), TvError> {
    let actual = match actual_media_type {
        "movie" => TvMediaType::Movie,
        "episode" => TvMediaType::Episode,
        _ => {
            return Err(TvError::InvalidPlatformContentId(
                actual_media_type.to_string(),
            ));
        }
    };

    if parsed.media_type != actual {
        return Err(TvError::InvalidPlatformContentId(
            build_platform_content_id(parsed.media_type, parsed.media_item_id),
        ));
    }

    Ok(())
}

fn parse_strict_platform_content_id(value: &str) -> Result<PlatformContentId, TvError> {
    let (media_type, media_item_id) = if let Some(id) = value.strip_prefix("duskcue_movie_") {
        (TvMediaType::Movie, id)
    } else if let Some(id) = value.strip_prefix("duskcue_episode_") {
        (TvMediaType::Episode, id)
    } else {
        return Err(TvError::InvalidPlatformContentId(value.to_string()));
    };

    let media_item_id = Uuid::parse_str(media_item_id)
        .map_err(|_| TvError::InvalidPlatformContentId(value.to_string()))?;

    Ok(PlatformContentId {
        media_type,
        media_item_id,
    })
}

fn media_type_slug(media_type: TvMediaType) -> &'static str {
    match media_type {
        TvMediaType::Movie => "movie",
        TvMediaType::Episode => "episode",
    }
}

fn parse_section(value: &str) -> Result<TvSurfaceSectionType, TvError> {
    match value {
        "continue" => Ok(TvSurfaceSectionType::Continue),
        "next_up" => Ok(TvSurfaceSectionType::NextUp),
        "new_episodes" => Ok(TvSurfaceSectionType::NewEpisodes),
        "recommended" => Ok(TvSurfaceSectionType::Recommended),
        other => Err(TvError::InvalidSection(other.to_string())),
    }
}

fn default_sections() -> Vec<TvSurfaceSectionType> {
    vec![
        TvSurfaceSectionType::Continue,
        TvSurfaceSectionType::NextUp,
        TvSurfaceSectionType::NewEpisodes,
        TvSurfaceSectionType::Recommended,
    ]
}

fn section_title(section_type: TvSurfaceSectionType) -> &'static str {
    match section_type {
        TvSurfaceSectionType::Continue => "Continue Watching",
        TvSurfaceSectionType::NextUp => "Next Up",
        TvSurfaceSectionType::NewEpisodes => "New Episodes",
        TvSurfaceSectionType::Recommended => "Recommended",
    }
}

fn section_slug(section_type: TvSurfaceSectionType) -> &'static str {
    match section_type {
        TvSurfaceSectionType::Continue => "continue",
        TvSurfaceSectionType::NextUp => "next_up",
        TvSurfaceSectionType::NewEpisodes => "new_episodes",
        TvSurfaceSectionType::Recommended => "recommended",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_surface_query_defaults() {
        let query = resolve_surface_query(TvSurfaceQuery {
            platform: None,
            limit: None,
            sections: None,
        })
        .unwrap();

        assert_eq!(query.platform, None);
        assert_eq!(query.limit, 30);
        assert_eq!(query.sections.len(), 4);
    }

    #[test]
    fn rejects_invalid_limit() {
        let err = resolve_surface_query(TvSurfaceQuery {
            platform: None,
            limit: Some(101),
            sections: None,
        })
        .unwrap_err();

        assert!(matches!(err, TvError::InvalidLimit(101)));
    }

    #[test]
    fn parses_platform_content_id() {
        let id = Uuid::now_v7();
        let parsed = parse_platform_content_id(&format!("duskcue:episode:{id}")).unwrap();

        assert_eq!(parsed.media_type, TvMediaType::Episode);
        assert_eq!(parsed.media_item_id, id);
    }

    #[test]
    fn builds_canonical_platform_content_id_without_paths() {
        let id = Uuid::now_v7();
        let content_id = build_platform_content_id(TvMediaType::Movie, id);

        assert_eq!(content_id, format!("duskcue:movie:{id}"));
        assert!(!content_id.contains('/'));
        assert!(!content_id.contains('\\'));
        assert!(!content_id.contains("C:"));
    }

    #[test]
    fn platform_content_id_is_stable_across_metadata_changes() {
        let id = Uuid::now_v7();
        let before_title_refresh = build_platform_content_id(TvMediaType::Episode, id);
        let after_artwork_refresh = build_platform_content_id(TvMediaType::Episode, id);

        assert_eq!(before_title_refresh, after_artwork_refresh);
    }

    #[test]
    fn encodes_and_decodes_strict_platform_ids() {
        let id = PlatformContentId {
            media_type: TvMediaType::Episode,
            media_item_id: Uuid::now_v7(),
        };

        let roku = encode_platform_content_id(&id, TvPlatformIdTarget::RokuFeed);
        let amazon = encode_platform_content_id(&id, TvPlatformIdTarget::AmazonCatalog);

        assert!(roku.starts_with("duskcue_episode_"));
        assert!(!roku.contains(':'));
        assert!(!roku.contains('-'));
        assert_eq!(
            decode_platform_content_id(&roku, TvPlatformIdTarget::RokuFeed).unwrap(),
            id
        );
        assert_eq!(
            decode_platform_content_id(&amazon, TvPlatformIdTarget::AmazonCatalog).unwrap(),
            id
        );
    }

    #[test]
    fn encodes_and_decodes_url_safe_ids() {
        let id = PlatformContentId {
            media_type: TvMediaType::Movie,
            media_item_id: Uuid::now_v7(),
        };

        let encoded = encode_platform_content_id(&id, TvPlatformIdTarget::UrlPath);

        assert!(encoded.contains("%3A"));
        assert_eq!(
            decode_platform_content_id(&encoded, TvPlatformIdTarget::UrlPath).unwrap(),
            id
        );
    }

    #[test]
    fn rejects_malformed_platform_content_id() {
        let err = parse_platform_content_id("duskcue:episode:path/to/file").unwrap_err();

        assert!(matches!(err, TvError::InvalidPlatformContentId(_)));
    }

    #[test]
    fn rejects_malformed_strict_platform_content_id() {
        let err = decode_platform_content_id(
            "duskcue_episode_path/to/file",
            TvPlatformIdTarget::RokuFeed,
        )
        .unwrap_err();

        assert!(matches!(err, TvError::InvalidPlatformContentId(_)));
    }

    #[test]
    fn rejects_cross_type_platform_content_id() {
        let id = Uuid::now_v7();
        let parsed = parse_platform_content_id(&format!("duskcue:episode:{id}")).unwrap();
        let err = validate_platform_content_type(&parsed, "movie").unwrap_err();

        assert!(matches!(err, TvError::InvalidPlatformContentId(_)));
    }

    #[test]
    fn access_scope_checks_explicit_libraries() {
        let allowed = Uuid::now_v7();
        let denied = Uuid::now_v7();
        let restricted = TvAccessScope {
            user_id: Uuid::now_v7(),
            has_all_library_access: false,
            library_ids: vec![allowed],
        };
        let unrestricted = TvAccessScope {
            user_id: Uuid::now_v7(),
            has_all_library_access: true,
            library_ids: Vec::new(),
        };

        assert!(restricted.can_access_library(allowed));
        assert!(!restricted.can_access_library(denied));
        assert!(unrestricted.can_access_library(denied));
    }
}
