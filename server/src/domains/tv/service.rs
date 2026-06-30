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

use chrono::Utc;
use sqlx::Row;
use uuid::Uuid;

use super::error::TvError;
use super::types::*;
use crate::extractors::AuthenticatedUser;

const DEFAULT_LIMIT: u32 = 30;
const MAX_LIMIT: u32 = 100;

const LOOKUP_PLATFORM_CONTENT_SQL: &str = r#"
SELECT mi.id,
       mi.type,
       mi.library_id,
       ($2::bool OR EXISTS (
           SELECT 1
           FROM user_library_access ula
           WHERE ula.user_id = $3 AND ula.library_id = mi.library_id
       )) AS has_access
FROM media_items mi
JOIN libraries l ON l.id = mi.library_id
WHERE mi.id = $1 AND l.deleted_at IS NULL
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
    let row = sqlx::query(LOOKUP_PLATFORM_CONTENT_SQL)
        .bind(parsed.media_item_id)
        .bind(user.has_all_library_access)
        .bind(user.user_id)
        .fetch_optional(pool)
        .await?
        .ok_or(TvError::UnavailableContent)?;

    let actual_media_type: String = row.try_get("type")?;
    validate_platform_content_type(&parsed, &actual_media_type)?;

    let has_access: bool = row.try_get("has_access")?;
    let access_status = if has_access {
        TvContentAccessStatus::Accessible
    } else {
        TvContentAccessStatus::AccessDenied
    };

    Ok(TvPlatformContentLookup {
        platform_content_id: build_platform_content_id(parsed.media_type, parsed.media_item_id),
        media_item_id: parsed.media_item_id,
        media_type: parsed.media_type,
        library_id: row.try_get("library_id")?,
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
}
