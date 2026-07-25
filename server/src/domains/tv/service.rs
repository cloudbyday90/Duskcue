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

use std::sync::{OnceLock, RwLock};
use std::time::Instant;

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use super::error::TvError;
use super::types::*;
use crate::extractors::AuthenticatedUser;
use crate::services::event_bus::{EventBus, ServerEvent};

const DEFAULT_LIMIT: u32 = 30;
const MAX_LIMIT: u32 = 100;
const RESUME_EVENT_DEBOUNCE_SECONDS: i64 = 60;
const DEFAULT_EVENT_DEBOUNCE_SECONDS: i64 = 5;

static TV_SURFACE_EVENT_DEBOUNCE: OnceLock<DashMap<String, DateTime<Utc>>> = OnceLock::new();
static TV_SURFACE_RUNTIME_STATUS: OnceLock<RwLock<TvSurfaceRuntimeStatus>> = OnceLock::new();

#[derive(Debug, Clone, Default)]
struct TvSurfaceRuntimeStatus {
    last_feed_generation: Option<DateTime<Utc>>,
    last_event: Option<TvSurfaceLastEvent>,
    last_resolve_failure: Option<TvResolveFailureStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredTvSurfaceSettings {
    #[serde(default = "default_true")]
    tv_publication_enabled: bool,
    #[serde(default = "default_tv_platforms")]
    enabled_platforms: Vec<TvPlatform>,
    #[serde(default = "default_true")]
    publish_continue_watching: bool,
    #[serde(default = "default_true")]
    publish_next_up: bool,
    #[serde(default = "default_true")]
    publish_new_episodes: bool,
    #[serde(default = "default_true")]
    publish_recommendations: bool,
}

impl Default for StoredTvSurfaceSettings {
    fn default() -> Self {
        Self {
            tv_publication_enabled: true,
            enabled_platforms: default_tv_platforms(),
            publish_continue_watching: true,
            publish_next_up: true,
            publish_new_episodes: true,
            publish_recommendations: true,
        }
    }
}

pub struct TvSurfaceSettingsUpdate {
    pub response: TvSurfaceSettingsResponse,
    pub changed_sections: Vec<TvSurfaceSectionType>,
}

#[derive(Debug, Clone)]
pub struct TvAccessScope {
    pub user_id: Uuid,
    pub profile_id: Uuid,
    pub has_all_library_access: bool,
    pub library_ids: Vec<Uuid>,
    pub profile_scope: crate::domains::profiles::types::ProfileScope,
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
    let profile_scope = crate::domains::profiles::service::load_profile_scope(
        pool,
        user.user_id,
        user.profile_id,
        user.has_all_library_access,
    )
    .await
    .map_err(|_| TvError::AccessDenied)?;
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
        profile_id: user.profile_id,
        has_all_library_access: user.has_all_library_access,
        library_ids,
        profile_scope,
    })
}

const LOOKUP_PLATFORM_CONTENT_SQL: &str = r#"
SELECT mi.id,
       mi.type,
       mi.library_id,
       mi.content_rating
FROM media_items mi
JOIN libraries l ON l.id = mi.library_id
WHERE mi.id = $1 AND l.deleted_at IS NULL
"#;

const RESOLVE_PLATFORM_CONTENT_SQL: &str = r#"
SELECT mi.id,
       mi.type,
       mi.library_id,
       mi.content_rating,
       mi.title,
       mi.overview,
       mi.premiere_date,
       mi.runtime_seconds,
       CASE
           WHEN COALESCE(uid.is_watched, false) THEN 0
           ELSE COALESCE(uid.resume_position_ms, 0)
       END AS resume_position_ms,
       sn.season_number,
       ep.episode_number,
       series_mi.title AS series_title,
       COALESCE(file_stats.file_count, 0) AS file_count,
       best_file.id AS best_media_file_id
FROM media_items mi
JOIN libraries l ON l.id = mi.library_id
LEFT JOIN user_item_data uid
       ON uid.profile_id = $2 AND uid.media_item_id = mi.id
LEFT JOIN episodes ep ON ep.id = mi.id
LEFT JOIN seasons sn ON sn.id = ep.season_id
LEFT JOIN media_items series_mi ON series_mi.id = ep.series_id
LEFT JOIN LATERAL (
    SELECT count(*) AS file_count
    FROM media_files mf
    WHERE mf.media_item_id = mi.id AND mf.is_healthy = true
) file_stats ON true
LEFT JOIN LATERAL (
    SELECT mf.id
    FROM media_files mf
    WHERE mf.media_item_id = mi.id AND mf.is_healthy = true
    ORDER BY mf.file_size DESC
    LIMIT 1
) best_file ON true
WHERE mi.id = $1
  AND mi.type IN ('movie', 'episode')
  AND l.deleted_at IS NULL
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
       mi.library_id,
       mi.content_rating,
       mi.title,
       mi.overview,
       mi.premiere_date,
       mi.runtime_seconds,
       uid.resume_position_ms,
       uid.last_played_at AS last_engaged_at,
       sn.season_number,
       ep.episode_number,
       ep.series_id,
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
WHERE uid.profile_id = $1
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
    WHERE uid.profile_id = $1
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
               ON uid_next.profile_id = $1 AND uid_next.media_item_id = ep.id
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
       mi.library_id,
       mi.content_rating,
       mi.title,
       mi.overview,
       mi.premiere_date,
       mi.runtime_seconds,
       0::int AS resume_position_ms,
       ne.last_played_at AS last_engaged_at,
       sn.season_number,
       ep.episode_number,
       ep.series_id,
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
    WHERE uid.profile_id = $1
),
latest_per_series AS (
    SELECT DISTINCT ON (ep.series_id)
           mi.id,
           mi.type,
           mi.library_id,
           mi.content_rating,
           mi.title,
           mi.overview,
           mi.premiere_date,
           mi.runtime_seconds,
           0::int AS resume_position_ms,
           mi.created_at AS last_engaged_at,
           sn.season_number,
           ep.episode_number,
           ep.series_id,
           series_mi.title AS series_title,
           COALESCE(mf.file_count, 0) AS file_count
    FROM started_series ss
    JOIN episodes ep ON ep.series_id = ss.series_id
    JOIN seasons sn ON sn.id = ep.season_id
    JOIN media_items mi ON mi.id = ep.id
    JOIN libraries l ON l.id = mi.library_id
    JOIN media_items series_mi ON series_mi.id = ep.series_id
    LEFT JOIN user_item_data uid_seen
           ON uid_seen.profile_id = $1 AND uid_seen.media_item_id = mi.id
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
    WHERE uid.profile_id = $1
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
       mi.library_id,
       mi.content_rating,
       mi.title,
       mi.overview,
       mi.premiere_date,
       mi.runtime_seconds,
       0::int AS resume_position_ms,
       mi.created_at AS last_engaged_at,
       sn.season_number,
       ep.episode_number,
       ep.series_id,
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
       ON uid_seen.profile_id = $1 AND uid_seen.media_item_id = mi.id
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

const DIAGNOSTIC_CANDIDATE_COUNT_SQL: &str = r#"
SELECT COUNT(*) AS candidate_count
FROM media_items mi
WHERE mi.type IN ('movie', 'episode')
"#;

const DIAGNOSTIC_REASON_COUNTS_SQL: &str = r#"
WITH classified AS (
    SELECT CASE
        WHEN l.id IS NULL OR l.deleted_at IS NOT NULL THEN 'library_offline'
        WHEN NOT ($1::bool OR mi.library_id = ANY($2::uuid[])) THEN 'access_revoked'
        WHEN NOT EXISTS (
            SELECT 1
            FROM media_files mf
            WHERE mf.media_item_id = mi.id AND mf.is_healthy = true
        ) THEN 'missing_file'
        WHEN mi.runtime_seconds IS NULL OR NULLIF(BTRIM(mi.title), '') IS NULL THEN 'metadata_incomplete'
        ELSE 'not_selected'
    END AS reason
    FROM media_items mi
    LEFT JOIN libraries l ON l.id = mi.library_id
    WHERE mi.type IN ('movie', 'episode')
      AND NOT (mi.id = ANY($3::uuid[]))
)
SELECT reason, COUNT(*) AS count
FROM classified
GROUP BY reason
ORDER BY reason
"#;

const DIAGNOSTIC_EXCLUSIONS_SQL: &str = r#"
SELECT mi.id,
       CASE
           WHEN l.id IS NULL OR l.deleted_at IS NOT NULL THEN 'library_offline'
           WHEN NOT ($1::bool OR mi.library_id = ANY($2::uuid[])) THEN 'access_revoked'
           WHEN NOT EXISTS (
               SELECT 1
               FROM media_files mf
               WHERE mf.media_item_id = mi.id AND mf.is_healthy = true
           ) THEN 'missing_file'
           WHEN mi.runtime_seconds IS NULL OR NULLIF(BTRIM(mi.title), '') IS NULL THEN 'metadata_incomplete'
           ELSE 'not_selected'
       END AS reason
FROM media_items mi
LEFT JOIN libraries l ON l.id = mi.library_id
WHERE mi.type IN ('movie', 'episode')
  AND NOT (mi.id = ANY($3::uuid[]))
ORDER BY mi.sort_title ASC, mi.id ASC
LIMIT $4
"#;

const ACTIVE_USERS_SQL: &str = r#"
SELECT id
FROM users
WHERE deleted_at IS NULL AND status = 'active'
"#;

const USER_TV_SETTINGS_SQL: &str = r#"
SELECT metadata -> 'tv_surface_settings' AS tv_surface_settings
FROM users
WHERE id = $1 AND deleted_at IS NULL
"#;

const UPDATE_USER_TV_SETTINGS_SQL: &str = r#"
UPDATE users
SET metadata = jsonb_set(
        COALESCE(metadata, '{}'::jsonb),
        '{tv_surface_settings}',
        $2::jsonb,
        true
    ),
    updated_at = now()
WHERE id = $1 AND deleted_at IS NULL
RETURNING metadata -> 'tv_surface_settings' AS tv_surface_settings
"#;

const USERS_WITH_LIBRARY_ACCESS_SQL: &str = r#"
SELECT u.id
FROM users u
WHERE u.deleted_at IS NULL
  AND u.status = 'active'
  AND (
      u.has_all_library_access = true
      OR EXISTS (
          SELECT 1
          FROM user_library_access ula
          WHERE ula.user_id = u.id AND ula.library_id = $1
      )
  )
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
    let started = Instant::now();
    let result = build_tv_surface(pool, user, query).await;
    record_tv_feed_metrics(started, query.platform, result.as_ref().ok());
    result
}

async fn build_tv_surface(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
    query: &ResolvedTvSurfaceQuery,
) -> Result<TvSurfaceResponse, TvError> {
    let settings = load_user_tv_settings(pool, user.user_id).await?;
    if let Some(reason) = surface_disabled_reason(&settings, query.platform) {
        let response = disabled_surface_response(query, reason);
        record_tv_surface_feed_generation(response.generated_at);
        return Ok(response);
    }

    let access_scope = load_tv_access_scope(pool, user).await?;
    let mut remaining = query.limit as usize;
    let mut sections = Vec::with_capacity(query.sections.len());

    for section_type in &query.sections {
        if !settings.section_enabled(*section_type) {
            sections.push(TvSurfaceSectionResponse {
                section_type: *section_type,
                title: section_title(*section_type).to_string(),
                empty_reason: Some("tv_section_disabled".to_string()),
                items: Vec::new(),
            });
            continue;
        }

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
    record_tv_surface_feed_generation(generated_at);

    Ok(TvSurfaceResponse {
        generated_at,
        platform: query.platform,
        limit: query.limit,
        sections,
    })
}

pub fn default_settings() -> TvSurfaceSettingsResponse {
    settings_response(StoredTvSurfaceSettings::default())
}

pub async fn get_tv_settings(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<TvSurfaceSettingsResponse, TvError> {
    let settings = load_user_tv_settings(pool, user_id).await?;
    Ok(settings_response(settings))
}

pub async fn update_tv_settings(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    req: TvSurfaceSettingsRequest,
) -> Result<TvSurfaceSettingsUpdate, TvError> {
    let current = load_user_tv_settings(pool, user_id).await?;
    let next = merge_tv_settings(current.clone(), req)?;
    let changed_sections = changed_settings_sections(&current, &next);

    let value = serde_json::to_value(&next).unwrap_or_else(|_| serde_json::json!({}));
    let row = sqlx::query(UPDATE_USER_TV_SETTINGS_SQL)
        .bind(user_id)
        .bind(value)
        .fetch_optional(pool)
        .await?
        .ok_or(TvError::AccessDenied)?;
    let saved = stored_settings_from_row(&row);

    Ok(TvSurfaceSettingsUpdate {
        response: settings_response(saved),
        changed_sections,
    })
}

async fn load_user_tv_settings(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<StoredTvSurfaceSettings, TvError> {
    let row = sqlx::query(USER_TV_SETTINGS_SQL)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or(TvError::AccessDenied)?;

    Ok(stored_settings_from_row(&row))
}

fn stored_settings_from_row(row: &sqlx::postgres::PgRow) -> StoredTvSurfaceSettings {
    row.try_get::<Option<serde_json::Value>, _>("tv_surface_settings")
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn merge_tv_settings(
    mut current: StoredTvSurfaceSettings,
    req: TvSurfaceSettingsRequest,
) -> Result<StoredTvSurfaceSettings, TvError> {
    if let Some(enabled) = req.tv_publication_enabled {
        current.tv_publication_enabled = enabled;
    }
    if let Some(platforms) = req.enabled_platforms {
        current.enabled_platforms = parse_settings_platforms(platforms)?;
    }
    if let Some(enabled) = req.publish_continue_watching {
        current.publish_continue_watching = enabled;
    }
    if let Some(enabled) = req.publish_next_up {
        current.publish_next_up = enabled;
    }
    if let Some(enabled) = req.publish_new_episodes {
        current.publish_new_episodes = enabled;
    }
    if let Some(enabled) = req.publish_recommendations {
        current.publish_recommendations = enabled;
    }
    Ok(current)
}

fn parse_settings_platforms(platforms: Vec<String>) -> Result<Vec<TvPlatform>, TvError> {
    let mut parsed = Vec::with_capacity(platforms.len());
    for platform in platforms {
        let platform = parse_platform(platform.trim())?;
        if !parsed.contains(&platform) {
            parsed.push(platform);
        }
    }
    Ok(parsed)
}

fn changed_settings_sections(
    current: &StoredTvSurfaceSettings,
    next: &StoredTvSurfaceSettings,
) -> Vec<TvSurfaceSectionType> {
    if current == next {
        return Vec::new();
    }

    if current.tv_publication_enabled != next.tv_publication_enabled
        || current.enabled_platforms != next.enabled_platforms
    {
        return all_tv_sections();
    }

    let mut sections = Vec::new();
    if current.publish_continue_watching != next.publish_continue_watching {
        sections.push(TvSurfaceSectionType::Continue);
    }
    if current.publish_next_up != next.publish_next_up {
        sections.push(TvSurfaceSectionType::NextUp);
    }
    if current.publish_new_episodes != next.publish_new_episodes {
        sections.push(TvSurfaceSectionType::NewEpisodes);
    }
    if current.publish_recommendations != next.publish_recommendations {
        sections.push(TvSurfaceSectionType::Recommended);
    }
    sections
}

fn settings_response(settings: StoredTvSurfaceSettings) -> TvSurfaceSettingsResponse {
    TvSurfaceSettingsResponse {
        tv_publication_enabled: settings.tv_publication_enabled,
        enabled_platforms: settings.enabled_platforms.clone(),
        publish_continue_watching: settings.publish_continue_watching,
        publish_next_up: settings.publish_next_up,
        publish_new_episodes: settings.publish_new_episodes,
        publish_recommendations: settings.publish_recommendations,
        integration_status: integration_status(&settings),
    }
}

fn integration_status(settings: &StoredTvSurfaceSettings) -> TvSurfaceIntegrationStatus {
    let snapshot = tv_surface_runtime_status()
        .read()
        .map(|status| status.clone())
        .unwrap_or_default();

    TvSurfaceIntegrationStatus {
        publication_enabled: settings.tv_publication_enabled,
        enabled_platforms: settings.enabled_platforms.clone(),
        diagnostics_available: true,
        last_feed_generation: snapshot.last_feed_generation,
        last_event: snapshot.last_event,
        last_resolve_failure: snapshot.last_resolve_failure,
    }
}

fn surface_disabled_reason(
    settings: &StoredTvSurfaceSettings,
    platform: Option<TvPlatform>,
) -> Option<&'static str> {
    if !settings.tv_publication_enabled {
        return Some("tv_publication_disabled");
    }
    if let Some(platform) = platform
        && !settings.enabled_platforms.contains(&platform)
    {
        return Some("tv_platform_disabled");
    }
    None
}

fn disabled_surface_response(
    query: &ResolvedTvSurfaceQuery,
    reason: &'static str,
) -> TvSurfaceResponse {
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
                empty_reason: Some(reason.to_string()),
                items: Vec::new(),
            })
            .collect(),
    }
}

impl StoredTvSurfaceSettings {
    fn section_enabled(&self, section_type: TvSurfaceSectionType) -> bool {
        match section_type {
            TvSurfaceSectionType::Continue => self.publish_continue_watching,
            TvSurfaceSectionType::NextUp => self.publish_next_up,
            TvSurfaceSectionType::NewEpisodes => self.publish_new_episodes,
            TvSurfaceSectionType::Recommended => self.publish_recommendations,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_tv_platforms() -> Vec<TvPlatform> {
    vec![
        TvPlatform::AndroidTv,
        TvPlatform::GoogleTv,
        TvPlatform::FireTv,
        TvPlatform::Roku,
        TvPlatform::Tizen,
        TvPlatform::Webos,
        TvPlatform::Tvos,
        TvPlatform::Xbox,
    ]
}

fn all_tv_sections() -> Vec<TvSurfaceSectionType> {
    vec![
        TvSurfaceSectionType::Continue,
        TvSurfaceSectionType::NextUp,
        TvSurfaceSectionType::NewEpisodes,
        TvSurfaceSectionType::Recommended,
    ]
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
        .bind(access_scope.profile_id)
        .bind(access_scope.has_all_library_access)
        .bind(&access_scope.library_ids)
        .bind(limit as i64)
        .fetch_all(pool)
        .await?;

    rows.iter()
        .filter(|row| {
            let library_id: Result<Uuid, sqlx::Error> = row.try_get("library_id");
            let content_rating: Option<String> = row.try_get("content_rating").ok().flatten();
            library_id
                .map(|library_id| {
                    crate::domains::profiles::service::is_media_allowed(
                        &access_scope.profile_scope,
                        library_id,
                        content_rating.as_deref(),
                    )
                })
                .unwrap_or(false)
        })
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
    let series_id: Option<Uuid> = row.try_get("series_id")?;
    let series_title: Option<String> = row.try_get("series_title")?;
    let file_count: i64 = row.try_get("file_count")?;

    let platform_content_id = build_platform_content_id(media_type, media_item_id);
    let duration_ms = runtime_seconds.map(|seconds| i64::from(seconds) * 1000);
    let progress_percent = duration_ms
        .filter(|duration| *duration > 0)
        .map(|duration| ((f64::from(resume_position_ms) / duration as f64) * 100.0).min(100.0))
        .unwrap_or(0.0);
    let availability = availability_for(file_count, &title, runtime_seconds);
    let availability_detail = availability_detail(availability).map(str::to_string);

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
        series_id,
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
        availability,
        availability_detail,
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

pub async fn get_tv_diagnostics(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
    query: &ResolvedTvSurfaceQuery,
) -> Result<TvDiagnosticsResponse, TvError> {
    let access_scope = load_tv_access_scope(pool, user).await?;
    let surface = build_tv_surface(pool, user, query).await?;
    let included_ids = surface
        .sections
        .iter()
        .flat_map(|section| section.items.iter().map(|item| item.media_item_id))
        .collect::<Vec<_>>();
    let section_counts = surface
        .sections
        .iter()
        .map(|section| TvDiagnosticSectionCount {
            section_type: section.section_type,
            item_count: section.items.len() as u32,
        })
        .collect::<Vec<_>>();
    let included_count = included_ids.len() as u32;
    let candidate_count = count_diagnostic_candidates(pool).await?;
    let reason_counts = fetch_diagnostic_reason_counts(pool, &access_scope, &included_ids).await?;
    let excluded = fetch_diagnostic_exclusions(pool, &access_scope, &included_ids, 100).await?;

    for reason in &reason_counts {
        metrics::counter!("tv_surface_excluded_items_total", "reason" => reason.reason.clone())
            .increment(u64::from(reason.count));
    }

    Ok(TvDiagnosticsResponse {
        generated_at: Utc::now(),
        platform: query.platform,
        candidate_count,
        included_count,
        section_counts,
        reason_counts,
        excluded,
    })
}

async fn count_diagnostic_candidates(pool: &sqlx::PgPool) -> Result<u32, TvError> {
    let row = sqlx::query(DIAGNOSTIC_CANDIDATE_COUNT_SQL)
        .fetch_one(pool)
        .await?;
    let count: i64 = row.try_get("candidate_count")?;

    Ok(count.max(0) as u32)
}

async fn fetch_diagnostic_reason_counts(
    pool: &sqlx::PgPool,
    access_scope: &TvAccessScope,
    included_ids: &[Uuid],
) -> Result<Vec<TvDiagnosticReasonCount>, TvError> {
    let rows = sqlx::query(DIAGNOSTIC_REASON_COUNTS_SQL)
        .bind(access_scope.has_all_library_access)
        .bind(&access_scope.library_ids)
        .bind(included_ids)
        .fetch_all(pool)
        .await?;

    rows.iter()
        .map(|row| {
            let reason: String = row.try_get("reason")?;
            let count: i64 = row.try_get("count")?;
            Ok(TvDiagnosticReasonCount {
                reason,
                count: count.max(0) as u32,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(TvError::from)
}

async fn fetch_diagnostic_exclusions(
    pool: &sqlx::PgPool,
    access_scope: &TvAccessScope,
    included_ids: &[Uuid],
    limit: i64,
) -> Result<Vec<TvDiagnosticExclusion>, TvError> {
    let rows = sqlx::query(DIAGNOSTIC_EXCLUSIONS_SQL)
        .bind(access_scope.has_all_library_access)
        .bind(&access_scope.library_ids)
        .bind(included_ids)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    rows.iter()
        .map(|row| {
            let media_item_id: Uuid = row.try_get("id")?;
            let reason: String = row.try_get("reason")?;
            let availability = diagnostic_availability_for_reason(&reason);

            Ok(TvDiagnosticExclusion {
                media_item_id: Some(media_item_id),
                detail: diagnostic_detail_for_reason(&reason).to_string(),
                reason,
                availability,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(TvError::from)
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
    let content_rating: Option<String> = row.try_get("content_rating").ok().flatten();
    let has_access = access_scope.can_access_library(library_id)
        && crate::domains::profiles::service::is_media_allowed(
            &access_scope.profile_scope,
            library_id,
            content_rating.as_deref(),
        );
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

pub async fn resolve_platform_content(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
    value: &str,
) -> Result<TvResolveResponse, TvError> {
    let settings = load_user_tv_settings(pool, user.user_id).await?;
    if !settings.tv_publication_enabled {
        return Err(TvError::UnavailableContent);
    }

    let lookup = lookup_platform_content(pool, user, value).await?;
    if lookup.access_status == TvContentAccessStatus::AccessDenied {
        return Err(TvError::AccessDenied);
    }

    let row = sqlx::query(RESOLVE_PLATFORM_CONTENT_SQL)
        .bind(lookup.media_item_id)
        .bind(user.profile_id)
        .fetch_optional(pool)
        .await?
        .ok_or(TvError::UnavailableContent)?;

    row_to_resolve_response(&row, &lookup)
}

fn row_to_resolve_response(
    row: &sqlx::postgres::PgRow,
    lookup: &TvPlatformContentLookup,
) -> Result<TvResolveResponse, TvError> {
    let media_item_id: Uuid = row.try_get("id")?;
    let media_type_raw: String = row.try_get("type")?;
    validate_platform_content_type(
        &PlatformContentId {
            media_type: lookup.media_type,
            media_item_id,
        },
        &media_type_raw,
    )
    .map_err(|_| TvError::UnavailableContent)?;

    let title: String = row.try_get("title")?;
    let overview: Option<String> = row.try_get("overview")?;
    let premiere_date: Option<NaiveDate> = row.try_get("premiere_date")?;
    let runtime_seconds: Option<i32> = row.try_get("runtime_seconds")?;
    let resume_position_ms: i32 = row.try_get("resume_position_ms")?;
    let season_number: Option<i32> = row.try_get("season_number")?;
    let episode_number: Option<i32> = row.try_get("episode_number")?;
    let series_title: Option<String> = row.try_get("series_title")?;
    let file_count: i64 = row.try_get("file_count")?;
    let best_media_file_id: Option<Uuid> = row.try_get("best_media_file_id")?;
    let availability = availability_for(file_count, &title, runtime_seconds);
    let availability_detail = availability_detail(availability).map(str::to_string);
    let duration_ms = runtime_seconds.map(|seconds| i64::from(seconds) * 1000);
    let playback_start_path = "/api/v1/playback/start".to_string();
    let playback_action = if is_playback_start_available(availability, best_media_file_id) {
        "start_playback"
    } else {
        "unavailable"
    }
    .to_string();

    Ok(TvResolveResponse {
        platform_content_id: lookup.platform_content_id.clone(),
        media_item_id,
        media_type: lookup.media_type,
        title,
        subtitle: item_subtitle(
            lookup.media_type,
            series_title,
            season_number,
            episode_number,
            premiere_date,
        ),
        description: overview,
        duration_ms,
        resume_position_ms: i64::from(resume_position_ms),
        availability,
        availability_detail,
        playback_action,
        playback_start_path: playback_start_path.clone(),
        playback_start: TvPlaybackStartHints {
            method: "POST".to_string(),
            path: playback_start_path,
            media_item_id,
            media_file_id: best_media_file_id,
            start_position_ms: i64::from(resume_position_ms),
            force_transcode: false,
            device_profile_required: false,
        },
        deep_link: format!(
            "duskcue://play/{}/{}",
            media_type_slug(lookup.media_type),
            media_item_id
        ),
        web_url: format!("/media/{media_item_id}"),
        artwork: TvArtworkHints {
            poster_url: Some(format!("/api/v1/items/{media_item_id}/artwork/poster")),
            backdrop_url: Some(format!("/api/v1/items/{media_item_id}/artwork/backdrop")),
            logo_url: Some(format!("/api/v1/items/{media_item_id}/artwork/logo")),
            thumbnail_url: Some(format!("/api/v1/items/{media_item_id}/artwork/thumbnail")),
        },
        requires_auth: true,
    })
}

pub fn build_platform_content_id(media_type: TvMediaType, media_item_id: Uuid) -> String {
    format!("duskcue:{}:{media_item_id}", media_type_slug(media_type))
}

pub fn record_tv_resolve_failure(err: &TvError) {
    metrics::counter!("tv_resolve_failures_total", "reason" => resolve_failure_reason(err))
        .increment(1);
    record_tv_surface_resolve_failure(err);
}

pub fn publish_tv_surface_changed(
    event_bus: &EventBus,
    user_id: Uuid,
    reason: &str,
    changed_sections: Vec<TvSurfaceSectionType>,
    media_item_id: Option<Uuid>,
    series_id: Option<Uuid>,
    library_id: Option<Uuid>,
    debounce_seconds: i64,
) -> bool {
    if changed_sections.is_empty() {
        return false;
    }

    let reason = normalize_surface_change_reason(reason);
    let now = Utc::now();
    let debounce_until = if debounce_seconds > 0 {
        Some(now + Duration::seconds(debounce_seconds))
    } else {
        None
    };

    if should_debounce_surface_event(user_id, reason, media_item_id, library_id, now) {
        return false;
    }

    if let Some(until) = debounce_until {
        let key = surface_event_debounce_key(user_id, reason, media_item_id, library_id);
        surface_event_debounce().insert(key, until);
    }

    let payload = TvSurfaceChangedEventPayload {
        user_id,
        reason: reason.to_string(),
        changed_sections,
        media_item_id,
        series_id,
        library_id,
        generated_after: now,
        debounce_until,
    };

    record_tv_surface_event(&payload);
    let value = serde_json::to_value(payload).unwrap_or_else(|_| serde_json::json!({}));
    let _ = event_bus.publish(user_id, ServerEvent::new("tv_surface_changed", value));
    true
}

pub fn publish_tv_resume_changed(
    event_bus: &EventBus,
    user_id: Uuid,
    media_item_id: Option<Uuid>,
) -> bool {
    publish_tv_surface_changed(
        event_bus,
        user_id,
        "resume_position_changed",
        vec![TvSurfaceSectionType::Continue],
        media_item_id,
        None,
        None,
        RESUME_EVENT_DEBOUNCE_SECONDS,
    )
}

pub async fn publish_tv_surface_changed_for_all_users(
    pool: &sqlx::PgPool,
    event_bus: &EventBus,
    reason: &str,
    changed_sections: Vec<TvSurfaceSectionType>,
    media_item_id: Option<Uuid>,
    library_id: Option<Uuid>,
) -> Result<usize, TvError> {
    let rows = sqlx::query(ACTIVE_USERS_SQL).fetch_all(pool).await?;
    let mut published = 0;
    for row in rows {
        let user_id: Uuid = row.try_get("id")?;
        if publish_tv_surface_changed(
            event_bus,
            user_id,
            reason,
            changed_sections.clone(),
            media_item_id,
            None,
            library_id,
            DEFAULT_EVENT_DEBOUNCE_SECONDS,
        ) {
            published += 1;
        }
    }
    Ok(published)
}

pub async fn publish_tv_surface_changed_for_library(
    pool: &sqlx::PgPool,
    event_bus: &EventBus,
    library_id: Uuid,
    reason: &str,
    changed_sections: Vec<TvSurfaceSectionType>,
) -> Result<usize, TvError> {
    let rows = sqlx::query(USERS_WITH_LIBRARY_ACCESS_SQL)
        .bind(library_id)
        .fetch_all(pool)
        .await?;
    let mut published = 0;
    for row in rows {
        let user_id: Uuid = row.try_get("id")?;
        if publish_tv_surface_changed(
            event_bus,
            user_id,
            reason,
            changed_sections.clone(),
            None,
            None,
            Some(library_id),
            DEFAULT_EVENT_DEBOUNCE_SECONDS,
        ) {
            published += 1;
        }
    }
    Ok(published)
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

fn availability_for(
    healthy_file_count: i64,
    title: &str,
    runtime_seconds: Option<i32>,
) -> TvAvailabilityState {
    if healthy_file_count <= 0 {
        TvAvailabilityState::MissingFile
    } else if runtime_seconds.is_none() || title.trim().is_empty() {
        TvAvailabilityState::MetadataIncomplete
    } else {
        TvAvailabilityState::Playable
    }
}

fn availability_detail(state: TvAvailabilityState) -> Option<&'static str> {
    match state {
        TvAvailabilityState::Playable => None,
        TvAvailabilityState::NeedsTranscode => {
            Some("A compatible transcode may be required for this TV client.")
        }
        TvAvailabilityState::LibraryOffline => Some("The source library is currently unavailable."),
        TvAvailabilityState::MissingFile => {
            Some("No healthy media file is available for playback.")
        }
        TvAvailabilityState::AccessRevoked => {
            Some("The current user no longer has access to this library.")
        }
        TvAvailabilityState::MetadataIncomplete => {
            Some("Required runtime or title metadata is incomplete.")
        }
    }
}

fn is_playback_start_available(
    availability: TvAvailabilityState,
    media_file_id: Option<Uuid>,
) -> bool {
    media_file_id.is_some()
        && matches!(
            availability,
            TvAvailabilityState::Playable
                | TvAvailabilityState::NeedsTranscode
                | TvAvailabilityState::MetadataIncomplete
        )
}

fn diagnostic_availability_for_reason(reason: &str) -> TvAvailabilityState {
    match reason {
        "library_offline" => TvAvailabilityState::LibraryOffline,
        "access_revoked" => TvAvailabilityState::AccessRevoked,
        "missing_file" => TvAvailabilityState::MissingFile,
        "metadata_incomplete" => TvAvailabilityState::MetadataIncomplete,
        _ => TvAvailabilityState::Playable,
    }
}

fn diagnostic_detail_for_reason(reason: &str) -> &'static str {
    match reason {
        "library_offline" => "The item belongs to a library that is unavailable or removed.",
        "access_revoked" => "The diagnosed user does not currently have library access.",
        "missing_file" => "No healthy media file is available for playback.",
        "metadata_incomplete" => "The item is missing required TV-surface metadata.",
        "not_selected" => {
            "The item did not match requested sections or fell outside section limits."
        }
        _ => "The item was not included in the TV surface feed.",
    }
}

fn record_tv_feed_metrics(
    started: Instant,
    platform: Option<TvPlatform>,
    response: Option<&TvSurfaceResponse>,
) {
    let status = if response.is_some() {
        "success"
    } else {
        "error"
    };
    metrics::histogram!(
        "tv_surface_feed_generation_duration_seconds",
        "platform" => platform_metric_label(platform),
        "status" => status
    )
    .record(started.elapsed().as_secs_f64());

    if let Some(response) = response {
        for section in &response.sections {
            metrics::histogram!(
                "tv_surface_section_items",
                "section" => section_metric_label(section.section_type)
            )
            .record(section.items.len() as f64);
        }
    }
}

fn resolve_failure_reason(err: &TvError) -> &'static str {
    match err {
        TvError::InvalidPlatformContentId(_) => "invalid_platform_content_id",
        TvError::UnavailableContent => "unavailable_content",
        TvError::AccessDenied => "access_denied",
        TvError::Database(_) => "database",
        TvError::InvalidPlatform(_) => "invalid_platform",
        TvError::InvalidSection(_) => "invalid_section",
        TvError::InvalidLimit(_) => "invalid_limit",
        TvError::UnsupportedPlatformHint(_) => "unsupported_platform_hint",
        TvError::DiagnosticsUnavailable => "diagnostics_unavailable",
    }
}

fn normalize_surface_change_reason(reason: &str) -> &'static str {
    match reason {
        "playback_started" => "playback_started",
        "resume_position_changed" => "resume_position_changed",
        "playback_paused" => "playback_paused",
        "playback_stopped" => "playback_stopped",
        "playback_completed" => "playback_completed",
        "watch_data_updated" => "watch_data_updated",
        "library_changed" => "library_changed",
        "library_scan_completed" => "library_scan_completed",
        "metadata_changed" => "metadata_changed",
        "artwork_changed" => "artwork_changed",
        "collection_changed" => "collection_changed",
        "access_changed" => "access_changed",
        "settings_changed" => "settings_changed",
        _ => "other",
    }
}

fn surface_event_debounce() -> &'static DashMap<String, DateTime<Utc>> {
    TV_SURFACE_EVENT_DEBOUNCE.get_or_init(DashMap::new)
}

fn tv_surface_runtime_status() -> &'static RwLock<TvSurfaceRuntimeStatus> {
    TV_SURFACE_RUNTIME_STATUS.get_or_init(|| RwLock::new(TvSurfaceRuntimeStatus::default()))
}

fn record_tv_surface_feed_generation(generated_at: DateTime<Utc>) {
    if let Ok(mut status) = tv_surface_runtime_status().write() {
        status.last_feed_generation = Some(generated_at);
    }
}

fn record_tv_surface_event(payload: &TvSurfaceChangedEventPayload) {
    if let Ok(mut status) = tv_surface_runtime_status().write() {
        status.last_event = Some(TvSurfaceLastEvent {
            reason: payload.reason.clone(),
            changed_sections: payload.changed_sections.clone(),
            generated_at: payload.generated_after,
        });
    }
}

fn record_tv_surface_resolve_failure(err: &TvError) {
    if let Ok(mut status) = tv_surface_runtime_status().write() {
        status.last_resolve_failure = Some(TvResolveFailureStatus {
            reason: resolve_failure_reason(err).to_string(),
            generated_at: Utc::now(),
        });
    }
}

fn should_debounce_surface_event(
    user_id: Uuid,
    reason: &str,
    media_item_id: Option<Uuid>,
    library_id: Option<Uuid>,
    now: DateTime<Utc>,
) -> bool {
    let key = surface_event_debounce_key(user_id, reason, media_item_id, library_id);
    surface_event_debounce()
        .get(&key)
        .map(|until| *until > now)
        .unwrap_or(false)
}

fn surface_event_debounce_key(
    user_id: Uuid,
    reason: &str,
    media_item_id: Option<Uuid>,
    library_id: Option<Uuid>,
) -> String {
    format!(
        "{}:{}:{}:{}",
        user_id,
        reason,
        media_item_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "all_media".to_string()),
        library_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "all_libraries".to_string())
    )
}

fn platform_metric_label(platform: Option<TvPlatform>) -> &'static str {
    match platform {
        Some(TvPlatform::AndroidTv) => "android_tv",
        Some(TvPlatform::GoogleTv) => "google_tv",
        Some(TvPlatform::FireTv) => "fire_tv",
        Some(TvPlatform::Roku) => "roku",
        Some(TvPlatform::Tizen) => "tizen",
        Some(TvPlatform::Webos) => "webos",
        Some(TvPlatform::Tvos) => "tvos",
        Some(TvPlatform::Xbox) => "xbox",
        None => "unspecified",
    }
}

fn section_metric_label(section_type: TvSurfaceSectionType) -> &'static str {
    match section_type {
        TvSurfaceSectionType::Continue => "continue",
        TvSurfaceSectionType::NextUp => "next_up",
        TvSurfaceSectionType::NewEpisodes => "new_episodes",
        TvSurfaceSectionType::Recommended => "recommended",
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
            profile_id: Uuid::now_v7(),
            has_all_library_access: false,
            library_ids: vec![allowed],
            profile_scope: crate::domains::profiles::types::ProfileScope {
                profile_id: Uuid::now_v7(),
                owner_user_id: Uuid::now_v7(),
                profile_type: "standard".to_string(),
                max_content_rating: "NC-17".to_string(),
                allow_search: true,
                allow_downloads: true,
                allow_external_links: true,
                allow_ambient_channels: true,
                library_ids: Vec::new(),
                user_library_ids: vec![allowed],
                has_all_library_access: false,
            },
        };
        let unrestricted = TvAccessScope {
            user_id: Uuid::now_v7(),
            profile_id: Uuid::now_v7(),
            has_all_library_access: true,
            library_ids: Vec::new(),
            profile_scope: crate::domains::profiles::types::ProfileScope {
                profile_id: Uuid::now_v7(),
                owner_user_id: Uuid::now_v7(),
                profile_type: "standard".to_string(),
                max_content_rating: "NC-17".to_string(),
                allow_search: true,
                allow_downloads: true,
                allow_external_links: true,
                allow_ambient_channels: true,
                library_ids: Vec::new(),
                user_library_ids: Vec::new(),
                has_all_library_access: true,
            },
        };

        assert!(restricted.can_access_library(allowed));
        assert!(!restricted.can_access_library(denied));
        assert!(unrestricted.can_access_library(denied));
    }

    #[test]
    fn availability_states_are_bounded_and_privacy_safe() {
        assert_eq!(
            availability_for(1, "Movie", Some(7200)),
            TvAvailabilityState::Playable
        );
        assert_eq!(
            availability_for(0, "Movie", Some(7200)),
            TvAvailabilityState::MissingFile
        );
        assert_eq!(
            availability_for(1, "Movie", None),
            TvAvailabilityState::MetadataIncomplete
        );

        let detail = availability_detail(TvAvailabilityState::MissingFile).unwrap();
        assert!(!detail.contains('/'));
        assert!(!detail.contains('\\'));
        assert!(!detail.contains("C:"));
    }

    #[test]
    fn diagnostics_reason_mapping_is_bounded() {
        assert_eq!(
            diagnostic_availability_for_reason("access_revoked"),
            TvAvailabilityState::AccessRevoked
        );
        assert_eq!(
            diagnostic_availability_for_reason("library_offline"),
            TvAvailabilityState::LibraryOffline
        );
        assert_eq!(
            diagnostic_availability_for_reason("not_selected"),
            TvAvailabilityState::Playable
        );

        let detail = diagnostic_detail_for_reason("access_revoked");
        assert!(!detail.contains('/'));
        assert!(!detail.contains('\\'));
    }

    #[test]
    fn tv_metric_labels_are_stable() {
        assert_eq!(
            platform_metric_label(Some(TvPlatform::AndroidTv)),
            "android_tv"
        );
        assert_eq!(platform_metric_label(None), "unspecified");
        assert_eq!(
            section_metric_label(TvSurfaceSectionType::NewEpisodes),
            "new_episodes"
        );
        assert_eq!(
            resolve_failure_reason(&TvError::InvalidPlatformContentId("bad".to_string())),
            "invalid_platform_content_id"
        );
    }

    #[test]
    fn playback_start_requires_available_file() {
        let media_file_id = Uuid::now_v7();

        assert!(is_playback_start_available(
            TvAvailabilityState::Playable,
            Some(media_file_id)
        ));
        assert!(is_playback_start_available(
            TvAvailabilityState::MetadataIncomplete,
            Some(media_file_id)
        ));
        assert!(!is_playback_start_available(
            TvAvailabilityState::MissingFile,
            Some(media_file_id)
        ));
        assert!(!is_playback_start_available(
            TvAvailabilityState::Playable,
            None
        ));
    }

    #[test]
    fn tv_surface_reason_normalization_is_bounded() {
        assert_eq!(
            normalize_surface_change_reason("playback_completed"),
            "playback_completed"
        );
        assert_eq!(
            normalize_surface_change_reason("raw path /tmp/movie"),
            "other"
        );
    }

    #[test]
    fn tv_surface_event_debounce_keys_include_user_reason_and_item() {
        let user_id = Uuid::now_v7();
        let media_item_id = Uuid::now_v7();
        let library_id = Uuid::now_v7();

        let key = surface_event_debounce_key(
            user_id,
            "resume_position_changed",
            Some(media_item_id),
            Some(library_id),
        );

        assert!(key.contains(&user_id.to_string()));
        assert!(key.contains("resume_position_changed"));
        assert!(key.contains(&media_item_id.to_string()));
        assert!(key.contains(&library_id.to_string()));
    }

    #[test]
    fn tv_surface_event_debounce_suppresses_until_deadline() {
        let user_id = Uuid::now_v7();
        let media_item_id = Some(Uuid::now_v7());
        let now = Utc::now();
        let key =
            surface_event_debounce_key(user_id, "resume_position_changed", media_item_id, None);
        surface_event_debounce().insert(key, now + Duration::seconds(30));

        assert!(should_debounce_surface_event(
            user_id,
            "resume_position_changed",
            media_item_id,
            None,
            now
        ));
        assert!(!should_debounce_surface_event(
            user_id,
            "resume_position_changed",
            media_item_id,
            None,
            now + Duration::seconds(31)
        ));
    }

    #[test]
    fn publish_tv_surface_changed_emits_bounded_payload() {
        let bus = EventBus::with_default_limit();
        let user_id = Uuid::now_v7();
        let media_item_id = Uuid::now_v7();
        let mut rx = bus.subscribe(user_id);

        assert!(publish_tv_surface_changed(
            &bus,
            user_id,
            "playback_completed",
            vec![TvSurfaceSectionType::Continue],
            Some(media_item_id),
            None,
            None,
            0,
        ));

        let event = rx.try_recv().expect("tv surface event should be emitted");
        assert_eq!(event.event_type, "tv_surface_changed");
        assert_eq!(event.payload["user_id"], user_id.to_string());
        assert_eq!(event.payload["reason"], "playback_completed");
        assert_eq!(event.payload["changed_sections"][0], "continue");
        assert_eq!(event.payload["media_item_id"], media_item_id.to_string());
        assert!(event.payload["generated_after"].is_string());
        assert!(event.payload["debounce_until"].is_null());
    }

    #[test]
    fn publish_tv_surface_changed_coalesces_duplicate_events() {
        let bus = EventBus::with_default_limit();
        let user_id = Uuid::now_v7();
        let media_item_id = Uuid::now_v7();
        let mut rx = bus.subscribe(user_id);

        assert!(publish_tv_surface_changed(
            &bus,
            user_id,
            "resume_position_changed",
            vec![TvSurfaceSectionType::Continue],
            Some(media_item_id),
            None,
            None,
            60,
        ));
        assert!(!publish_tv_surface_changed(
            &bus,
            user_id,
            "resume_position_changed",
            vec![TvSurfaceSectionType::Continue],
            Some(media_item_id),
            None,
            None,
            60,
        ));

        let event = rx.try_recv().expect("first event should be emitted");
        assert_eq!(event.payload["reason"], "resume_position_changed");
        assert!(event.payload["debounce_until"].is_string());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn publish_tv_surface_changed_skips_empty_section_updates() {
        let bus = EventBus::with_default_limit();
        let user_id = Uuid::now_v7();
        let mut rx = bus.subscribe(user_id);

        assert!(!publish_tv_surface_changed(
            &bus,
            user_id,
            "metadata_changed",
            vec![],
            None,
            None,
            None,
            0,
        ));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn tv_settings_merge_validates_and_deduplicates_platforms() {
        let settings = merge_tv_settings(
            StoredTvSurfaceSettings::default(),
            TvSurfaceSettingsRequest {
                tv_publication_enabled: Some(false),
                enabled_platforms: Some(vec!["roku".into(), "roku".into(), "tvos".into()]),
                publish_continue_watching: None,
                publish_next_up: Some(false),
                publish_new_episodes: None,
                publish_recommendations: None,
            },
        )
        .expect("settings should merge");

        assert!(!settings.tv_publication_enabled);
        assert_eq!(
            settings.enabled_platforms,
            vec![TvPlatform::Roku, TvPlatform::Tvos]
        );
        assert!(!settings.publish_next_up);

        assert!(
            merge_tv_settings(
                StoredTvSurfaceSettings::default(),
                TvSurfaceSettingsRequest {
                    tv_publication_enabled: None,
                    enabled_platforms: Some(vec!["unsupported".into()]),
                    publish_continue_watching: None,
                    publish_next_up: None,
                    publish_new_episodes: None,
                    publish_recommendations: None,
                },
            )
            .is_err()
        );
    }

    #[test]
    fn tv_settings_changed_sections_are_precise() {
        let current = StoredTvSurfaceSettings::default();
        let mut next = current.clone();
        next.publish_continue_watching = false;
        next.publish_recommendations = false;

        assert_eq!(
            changed_settings_sections(&current, &next),
            vec![
                TvSurfaceSectionType::Continue,
                TvSurfaceSectionType::Recommended
            ]
        );

        next.tv_publication_enabled = false;
        assert_eq!(
            changed_settings_sections(&current, &next),
            all_tv_sections()
        );
    }

    #[test]
    fn disabled_tv_surface_response_preserves_requested_sections() {
        let query = ResolvedTvSurfaceQuery {
            platform: Some(TvPlatform::Roku),
            limit: 10,
            sections: vec![TvSurfaceSectionType::Continue, TvSurfaceSectionType::NextUp],
        };

        let response = disabled_surface_response(&query, "tv_platform_disabled");

        assert_eq!(response.platform, Some(TvPlatform::Roku));
        assert_eq!(response.limit, 10);
        assert_eq!(response.sections.len(), 2);
        assert_eq!(
            response.sections[0].empty_reason.as_deref(),
            Some("tv_platform_disabled")
        );
        assert!(
            response
                .sections
                .iter()
                .all(|section| section.items.is_empty())
        );
    }
}
