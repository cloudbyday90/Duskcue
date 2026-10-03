use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use validator::Validate;

use super::error::FireError;
use super::types::*;
use crate::domains::tv::{service as tv, types::TvMediaType, types::TvPlatform};
use crate::extractors::AuthenticatedUser;
use crate::state::FireTvIntegrationConfig;

pub fn validate_mapping_request(
    request: &FireCatalogMappingRequest,
    now: DateTime<Utc>,
) -> Result<(), FireError> {
    request
        .validate()
        .map_err(|_| FireError::InvalidRequest("Invalid catalog fields"))?;
    for value in [
        &request.catalog_reference,
        &request.amazon_content_id,
        &request.acceptance_reference,
        &request.distribution_rights_reference,
    ] {
        if value.trim() != value || value.chars().any(char::is_control) || value.trim().is_empty() {
            return Err(FireError::InvalidRequest(
                "Catalog values must be non-empty and contain no surrounding whitespace or control characters",
            ));
        }
    }
    if request.amazon_content_id.starts_with("duskcue:") {
        return Err(FireError::InvalidRequest(
            "A Duskcue platform ID is not an accepted Amazon content ID",
        ));
    }
    if request.is_enabled && request.rights_expires_at <= now {
        return Err(FireError::InvalidRequest(
            "Enabled catalog mappings require current distribution rights",
        ));
    }
    Ok(())
}

pub fn mapping_eligible(row: &FireCatalogMappingRow, catalog: &str, now: DateTime<Utc>) -> bool {
    row.is_enabled
        && row.media_item_id == Some(row.original_media_item_id)
        && row.catalog_reference == catalog
        && row.rights_expires_at > now
        && !row.acceptance_reference.is_empty()
        && !row.distribution_rights_reference.is_empty()
}

pub async fn get_catalog_mapping(
    pool: &PgPool,
    user: &AuthenticatedUser,
    platform_content_id: &str,
    catalog_reference: &str,
) -> Result<FireCatalogMappingResponse, FireError> {
    require_catalog_admin(pool, user).await?;
    let content = tv::lookup_platform_content(pool, user, platform_content_id).await?;
    if content.access_status != crate::domains::tv::types::TvContentAccessStatus::Accessible {
        return Err(FireError::Unavailable);
    }
    let row = sqlx::query_as::<_, FireCatalogMappingRow>(
        "SELECT * FROM fire_catalog_mappings WHERE catalog_reference = $1 AND original_media_item_id = $2",
    )
    .bind(catalog_reference)
    .bind(content.media_item_id)
    .fetch_optional(pool)
    .await?
    .ok_or(FireError::Unavailable)?;
    mapping_response(row)
}

pub async fn put_catalog_mapping(
    pool: &PgPool,
    user: &AuthenticatedUser,
    platform_content_id: &str,
    selected_catalog: &str,
    request: FireCatalogMappingRequest,
) -> Result<FireCatalogMappingResponse, FireError> {
    require_catalog_admin(pool, user).await?;
    validate_mapping_request(&request, Utc::now())?;
    if request.catalog_reference != selected_catalog {
        return Err(FireError::InvalidRequest(
            "Select the catalog in server integration configuration before registering a mapping",
        ));
    }
    let content = tv::lookup_platform_content(pool, user, platform_content_id).await?;
    if content.access_status != crate::domains::tv::types::TvContentAccessStatus::Accessible {
        return Err(FireError::Unavailable);
    }
    let media_type = match content.media_type {
        TvMediaType::Movie => "movie",
        TvMediaType::Episode => "episode",
    };
    let mut transaction = pool.begin().await?;
    let created = if request.expected_revision.is_none() {
        sqlx::query(
            "INSERT INTO fire_catalog_mappings (media_item_id, original_media_item_id, media_type, \
             catalog_reference, amazon_content_id, acceptance_reference, distribution_rights_reference, \
             rights_expires_at, is_enabled) VALUES ($1, $1, $2, $3, $4, $5, $6, $7, $8) \
             ON CONFLICT DO NOTHING",
        )
        .bind(content.media_item_id)
        .bind(media_type)
        .bind(&request.catalog_reference)
        .bind(&request.amazon_content_id)
        .bind(&request.acceptance_reference)
        .bind(&request.distribution_rights_reference)
        .bind(request.rights_expires_at)
        .bind(request.is_enabled)
        .execute(&mut *transaction)
        .await?
        .rows_affected() == 1
    } else {
        false
    };
    let current = sqlx::query_as::<_, FireCatalogMappingRow>(
        "SELECT * FROM fire_catalog_mappings WHERE catalog_reference = $1 \
         AND original_media_item_id = $2 FOR UPDATE",
    )
    .bind(&request.catalog_reference)
    .bind(content.media_item_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(FireError::Conflict)?;
    if current.media_item_id != Some(content.media_item_id)
        || current.amazon_content_id != request.amazon_content_id
        || current.media_type != media_type
        || (!created && request.expected_revision != Some(current.revision))
    {
        return Err(FireError::Conflict);
    }
    let row = if created {
        current
    } else {
        let next_revision = current.revision.checked_add(1).ok_or(FireError::Conflict)?;
        sqlx::query_as::<_, FireCatalogMappingRow>(
            "UPDATE fire_catalog_mappings SET acceptance_reference = $2, distribution_rights_reference = $3, \
             rights_expires_at = $4, is_enabled = $5, revision = $6, updated_at = now() WHERE id = $1 RETURNING *",
        )
        .bind(current.id)
        .bind(&request.acceptance_reference)
        .bind(&request.distribution_rights_reference)
        .bind(request.rights_expires_at)
        .bind(request.is_enabled)
        .bind(next_revision)
        .fetch_one(&mut *transaction)
        .await?
    };
    sqlx::query(
        "INSERT INTO fire_catalog_mapping_events (mapping_id, revision, actor_user_id, snapshot) VALUES ($1, $2, $3, $4)",
    )
    .bind(row.id)
    .bind(row.revision)
    .bind(user.user_id)
    .bind(serde_json::json!({
        "is_enabled": row.is_enabled,
        "acceptance_reference": row.acceptance_reference,
        "distribution_rights_reference": row.distribution_rights_reference,
        "rights_expires_at": row.rights_expires_at,
    }))
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    mapping_response(row)
}

pub async fn playback_authorization(
    pool: &PgPool,
    user: &AuthenticatedUser,
    playback_session_id: Uuid,
    config: &FireTvIntegrationConfig,
) -> Result<FirePlaybackAuthorizationResponse, FireError> {
    let denied = FirePlaybackAuthorizationResponse::denied;
    if !config.is_enabled() || user.profile_selection_required {
        return Ok(denied());
    }
    let scope = tv::load_tv_access_scope(pool, user).await?;
    if scope.profile_scope.profile_type != "standard" {
        return Ok(denied());
    }
    let settings = tv::get_tv_settings(pool, user.user_id).await?;
    if !settings.tv_publication_enabled || !settings.enabled_platforms.contains(&TvPlatform::FireTv)
    {
        return Ok(denied());
    }
    let session = sqlx::query(
        "SELECT ps.media_item_id, mi.type FROM play_sessions ps JOIN media_items mi ON mi.id = ps.media_item_id \
         WHERE ps.id = $1 AND ps.user_id = $2 AND ps.profile_id = $3 AND ps.playback_mode = 'interactive' \
         AND ps.stopped_at IS NULL AND mi.type IN ('movie', 'episode')",
    )
    .bind(playback_session_id)
    .bind(user.user_id)
    .bind(user.profile_id)
    .fetch_optional(pool)
    .await?;
    let Some(session) = session else {
        return Ok(denied());
    };
    let media_item_id: Uuid = session.try_get("media_item_id")?;
    let media_type: String = session.try_get("type")?;
    let media_type = if media_type == "movie" {
        TvMediaType::Movie
    } else {
        TvMediaType::Episode
    };
    let content_id = tv::build_platform_content_id(media_type, media_item_id);
    let resolve = match tv::resolve_platform_content(pool, user, &content_id).await {
        Ok(resolve) => resolve,
        Err(crate::domains::tv::error::TvError::Database(error)) => {
            return Err(FireError::Database(error));
        }
        Err(_) => return Ok(denied()),
    };
    if !matches!(
        resolve.availability,
        crate::domains::tv::types::TvAvailabilityState::Playable
            | crate::domains::tv::types::TvAvailabilityState::NeedsTranscode
    ) {
        return Ok(denied());
    }
    let row = sqlx::query_as::<_, FireCatalogMappingRow>(
        "SELECT * FROM fire_catalog_mappings WHERE media_item_id = $1 AND catalog_reference = $2",
    )
    .bind(media_item_id)
    .bind(&config.catalog_reference)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(denied());
    };
    let now = Utc::now();
    let expected_type = match media_type {
        TvMediaType::Movie => "movie",
        TvMediaType::Episode => "episode",
    };
    if row.media_type != expected_type || !mapping_eligible(&row, &config.catalog_reference, now) {
        return Ok(denied());
    }
    let mut random = [0_u8; 32];
    rand::rng().fill_bytes(&mut random);
    let candidate = URL_SAFE_NO_PAD.encode(random);
    sqlx::query("INSERT INTO fire_profile_keys (profile_id, opaque_key) VALUES ($1, $2) ON CONFLICT (profile_id) DO NOTHING")
        .bind(user.profile_id)
        .bind(candidate)
        .execute(pool)
        .await?;
    let opaque_key: String =
        sqlx::query_scalar("SELECT opaque_key FROM fire_profile_keys WHERE profile_id = $1")
            .bind(user.profile_id)
            .fetch_one(pool)
            .await?;
    Ok(FirePlaybackAuthorizationResponse {
        eligible: true,
        catalog_content_id: Some(row.amazon_content_id),
        opaque_profile_key: Some(opaque_key),
        mapping_revision: Some(row.revision),
        expires_at: Some((now + Duration::seconds(60)).min(row.rights_expires_at)),
    })
}

fn mapping_response(row: FireCatalogMappingRow) -> Result<FireCatalogMappingResponse, FireError> {
    let media_type = match row.media_type.as_str() {
        "movie" => TvMediaType::Movie,
        "episode" => TvMediaType::Episode,
        _ => return Err(FireError::Unavailable),
    };
    Ok(FireCatalogMappingResponse {
        mapping_id: row.id,
        platform_content_id: tv::build_platform_content_id(media_type, row.original_media_item_id),
        catalog_reference: row.catalog_reference,
        amazon_content_id: row.amazon_content_id,
        acceptance_reference: row.acceptance_reference,
        distribution_rights_reference: row.distribution_rights_reference,
        rights_expires_at: row.rights_expires_at,
        is_enabled: row.is_enabled,
        revision: row.revision,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

async fn require_catalog_admin(pool: &PgPool, user: &AuthenticatedUser) -> Result<(), FireError> {
    if user.profile_selection_required
        || !user
            .capabilities
            .iter()
            .any(|value| value == "can_manage_server")
    {
        return Err(FireError::Unavailable);
    }
    let scope = tv::load_tv_access_scope(pool, user).await?;
    if scope.profile_scope.profile_type != "standard" {
        return Err(FireError::Unavailable);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> FireCatalogMappingRequest {
        FireCatalogMappingRequest {
            catalog_reference: "fixture-catalog".into(),
            amazon_content_id: "ACCEPTED_MOVIE_1".into(),
            acceptance_reference: "fixture-acceptance".into(),
            distribution_rights_reference: "fixture-rights".into(),
            rights_expires_at: Utc::now() + Duration::hours(1),
            is_enabled: true,
            expected_revision: None,
        }
    }

    #[test]
    fn requires_accepted_identity_and_current_rights() {
        let now = Utc::now();
        let mut request = request();
        assert!(validate_mapping_request(&request, now).is_ok());
        request.amazon_content_id = "duskcue:movie:fixture".into();
        assert!(validate_mapping_request(&request, now).is_err());
        request.amazon_content_id = "ACCEPTED_MOVIE_1".into();
        request.acceptance_reference.clear();
        assert!(validate_mapping_request(&request, now).is_err());
        request.acceptance_reference = "fixture-acceptance".into();
        request.rights_expires_at = now;
        assert!(validate_mapping_request(&request, now).is_err());
        request.is_enabled = false;
        assert!(validate_mapping_request(&request, now).is_ok());
        request.distribution_rights_reference = "fixture\nrights".into();
        assert!(validate_mapping_request(&request, now).is_err());
    }

    #[test]
    fn configuration_never_enables_reporting_implicitly() {
        let mut config = FireTvIntegrationConfig::default();
        assert!(!config.is_enabled());
        config.watch_activity_enabled = true;
        assert!(!config.is_enabled());
        config.catalog_reference = "fixture-catalog".into();
        assert!(!config.is_enabled());
        config.partner_approval_reference = "fixture-partner".into();
        assert!(config.is_enabled());
    }

    #[tokio::test]
    #[ignore = "requires scripts/verify-migrations.ps1 -RunFireRegistryTests and disposable PostgreSQL 18"]
    async fn fire_registry_database_contract() -> anyhow::Result<()> {
        anyhow::ensure!(
            std::env::var("DUSKCUE_FIRE_REGISTRY_TESTS").as_deref() == Ok("disposable"),
            "use the disposable migration verifier"
        );
        let pool = PgPool::connect(&std::env::var("DUSKCUE_DATABASE_URL")?).await?;
        let database: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&pool)
            .await?;
        anyhow::ensure!(
            database.starts_with("duskcue_migration"),
            "registry verification requires a disposable migration database"
        );
        sqlx::query("CREATE TABLE IF NOT EXISTS audit_log_fire_registry_test PARTITION OF audit_log DEFAULT").execute(&pool).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS play_events_fire_registry_test PARTITION OF play_events DEFAULT").execute(&pool).await?;
        let owner = Uuid::now_v7();
        let profile = Uuid::now_v7();
        let kids = Uuid::now_v7();
        let library = Uuid::now_v7();
        let movie = Uuid::now_v7();
        let session = Uuid::now_v7();
        sqlx::query("INSERT INTO users (id, username, display_name) VALUES ($1, $2, 'Fixture')")
            .bind(owner)
            .bind(format!("fire-fixture-{owner}"))
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO user_profiles (id, owner_user_id, name, profile_type) VALUES ($1, $3, 'Standard', 'standard'), ($2, $3, 'Kids', 'kids')")
            .bind(profile).bind(kids).bind(owner).execute(&pool).await?;
        sqlx::query("INSERT INTO libraries (id, name, slug, media_type, root_path) VALUES ($1, 'Fixture', $2, 'movies', 'fixture')")
            .bind(library).bind(format!("fire-{library}")).execute(&pool).await?;
        sqlx::query("INSERT INTO media_items (id, library_id, type, title, sort_title, runtime_seconds, content_rating) VALUES ($1, $2, 'movie', 'Fixture', 'Fixture', 900, 'G')")
            .bind(movie).bind(library).execute(&pool).await?;
        for (language, configuration) in [
            ("en", "english"),
            ("en-GB", "english"),
            ("de_DE", "german"),
            ("ja-JP", "simple"),
            ("unknown", "simple"),
        ] {
            sqlx::query("UPDATE libraries SET metadata_language = $2 WHERE id = $1")
                .bind(library)
                .bind(language)
                .execute(&pool)
                .await?;
            sqlx::query("UPDATE media_items SET title = 'Running horses' WHERE id = $1")
                .bind(movie)
                .execute(&pool)
                .await?;
            let matches: bool = sqlx::query_scalar("SELECT search_vector = setweight(to_tsvector($2::regconfig, 'Running horses'), 'A') FROM media_items WHERE id = $1")
                .bind(movie).bind(format!("pg_catalog.{configuration}")).fetch_one(&pool).await?;
            assert!(matches, "search configuration for {language}");
        }
        sqlx::query("UPDATE libraries SET metadata_language = 'en' WHERE id = $1")
            .bind(library)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO media_files (media_item_id, file_path, file_size, container_format, runtime_seconds) VALUES ($1, 'fixture.mp4', 1, 'mp4', 900)")
            .bind(movie).execute(&pool).await?;
        sqlx::query("INSERT INTO play_sessions (id, user_id, profile_id, media_item_id, library_id, started_at, client_name, stream_decision, playback_mode) VALUES ($1, $2, $3, $4, $5, '2026-06-15', 'fixture', 'direct_play', 'interactive')")
            .bind(session).bind(owner).bind(profile).bind(movie).bind(library).execute(&pool).await?;
        let mut user = AuthenticatedUser {
            user_id: owner,
            session_id: Uuid::now_v7(),
            profile_id: profile,
            profile_selection_required: false,
            device_id: None,
            capabilities: vec!["can_manage_server".into()],
            role: "owner".into(),
            has_all_library_access: true,
            display_name: "Fixture".into(),
        };
        let manager = crate::services::transcoding::TranscodeManager::new(std::sync::Arc::new(
            arc_swap::ArcSwap::from_pointee(crate::state::RuntimeConfig::default()),
        ));
        let abandoned = Uuid::now_v7();
        sqlx::query("INSERT INTO user_item_data (user_id, profile_id, media_item_id, resume_position_ms, play_count) VALUES ($1, $2, $3, 45000, 7)")
            .bind(owner).bind(profile).bind(movie).execute(&pool).await?;
        sqlx::query("INSERT INTO play_sessions (id, user_id, profile_id, media_item_id, library_id, started_at, client_name, stream_decision, playback_mode) VALUES ($1, $2, $3, $4, $5, '2026-06-15', 'fixture', 'direct_play', 'interactive')")
            .bind(abandoned).bind(owner).bind(profile).bind(movie).bind(library).execute(&pool).await?;
        assert!(
            crate::domains::playback::service::stop_playback(
                &pool,
                &manager,
                Uuid::now_v7(),
                abandoned,
                Some(0),
                true
            )
            .await
            .is_err()
        );
        for _ in 0..2 {
            let cancelled = crate::domains::playback::service::stop_playback(
                &pool,
                &manager,
                owner,
                abandoned,
                Some(0),
                true,
            )
            .await?;
            assert_eq!(cancelled.play_count, 0);
            assert!(!cancelled.is_watched);
        }
        let unchanged: (i32, i32) = sqlx::query_as("SELECT resume_position_ms, play_count FROM user_item_data WHERE user_id = $1 AND profile_id = $2 AND media_item_id = $3")
            .bind(owner).bind(profile).bind(movie).fetch_one(&pool).await?;
        assert_eq!(unchanged, (45000, 7));
        assert!(
            crate::domains::playback::service::stop_playback(
                &pool,
                &manager,
                owner,
                abandoned,
                Some(0),
                false
            )
            .await
            .is_err()
        );
        sqlx::query("UPDATE play_sessions SET metadata = '{\"last_heartbeat_at\":\"fixture\"}'::jsonb WHERE id = $1")
            .bind(session).execute(&pool).await?;
        assert!(
            crate::domains::playback::service::stop_playback(
                &pool, &manager, owner, session, None, true
            )
            .await
            .is_err()
        );
        sqlx::query("UPDATE play_sessions SET metadata = '{\"current_position_ms\":1}'::jsonb WHERE id = $1")
            .bind(session).execute(&pool).await?;
        assert!(
            crate::domains::playback::service::stop_playback(
                &pool, &manager, owner, session, None, true
            )
            .await
            .is_err()
        );
        sqlx::query("UPDATE play_sessions SET metadata = '{}'::jsonb WHERE id = $1")
            .bind(session)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO play_events (play_session_id, user_id, event_type, position_seconds) VALUES ($1, $2, 'seek', 0)")
            .bind(session).bind(owner).execute(&pool).await?;
        assert!(
            crate::domains::playback::service::stop_playback(
                &pool, &manager, owner, session, None, true
            )
            .await
            .is_err()
        );
        sqlx::query("DELETE FROM play_events WHERE play_session_id = $1")
            .bind(session)
            .execute(&pool)
            .await?;
        let ordinary = Uuid::now_v7();
        let media_file_id: Uuid =
            sqlx::query_scalar("SELECT id FROM media_files WHERE media_item_id = $1 LIMIT 1")
                .bind(movie)
                .fetch_one(&pool)
                .await?;
        sqlx::query("INSERT INTO play_sessions (id, user_id, profile_id, media_item_id, library_id, started_at, client_name, stream_decision, playback_mode) VALUES ($1, $2, $3, $4, $5, '2026-06-15', 'fixture', 'direct_play', 'interactive')")
            .bind(ordinary).bind(owner).bind(profile).bind(movie).bind(library).execute(&pool).await?;
        sqlx::query("UPDATE play_sessions SET metadata = jsonb_build_object('media_file_id', $2::text) WHERE id = $1")
            .bind(ordinary).bind(media_file_id.to_string()).execute(&pool).await?;
        let stopped = crate::domains::playback::service::stop_playback(
            &pool,
            &manager,
            owner,
            ordinary,
            Some(45000),
            false,
        )
        .await?;
        assert_eq!(stopped.play_count, 8);
        let persisted: (i32, Option<Uuid>) = sqlx::query_as("SELECT resume_position_ms, last_played_media_file_id FROM user_item_data WHERE profile_id = $1 AND media_item_id = $2")
            .bind(profile).bind(movie).fetch_one(&pool).await?;
        assert_eq!(persisted, (45000, Some(media_file_id)));
        assert!(
            crate::domains::playback::service::stop_playback(
                &pool, &manager, owner, ordinary, None, true
            )
            .await
            .is_err()
        );
        let config = FireTvIntegrationConfig {
            watch_activity_enabled: true,
            catalog_reference: "fixture-catalog".into(),
            partner_approval_reference: "fixture-partner".into(),
        };
        let canonical = tv::build_platform_content_id(TvMediaType::Movie, movie);
        let registered = put_catalog_mapping(
            &pool,
            &user,
            &canonical,
            &config.catalog_reference,
            request(),
        )
        .await?;
        assert_eq!(registered.revision, 1);
        let first = playback_authorization(&pool, &user, session, &config).await?;
        assert!(first.eligible);
        assert_eq!(
            first.catalog_content_id.as_deref(),
            Some("ACCEPTED_MOVIE_1")
        );
        assert_eq!(first.opaque_profile_key.as_ref().unwrap().len(), 43);
        let second = playback_authorization(&pool, &user, session, &config).await?;
        assert_eq!(first.opaque_profile_key, second.opaque_profile_key);
        assert!(
            !playback_authorization(&pool, &user, session, &FireTvIntegrationConfig::default())
                .await?
                .eligible
        );
        assert!(
            !playback_authorization(&pool, &user, Uuid::now_v7(), &config)
                .await?
                .eligible
        );
        sqlx::query("UPDATE play_sessions SET stopped_at = now() WHERE id = $1")
            .bind(session)
            .execute(&pool)
            .await?;
        assert!(
            !playback_authorization(&pool, &user, session, &config)
                .await?
                .eligible
        );
        sqlx::query("UPDATE play_sessions SET stopped_at = NULL WHERE id = $1")
            .bind(session)
            .execute(&pool)
            .await?;
        sqlx::query("UPDATE users SET metadata = metadata || '{\"tv_surface_settings\": {\"tv_publication_enabled\": false}}'::jsonb WHERE id = $1").bind(owner).execute(&pool).await?;
        assert!(
            !playback_authorization(&pool, &user, session, &config)
                .await?
                .eligible
        );
        sqlx::query("UPDATE users SET metadata = metadata - 'tv_surface_settings' WHERE id = $1")
            .bind(owner)
            .execute(&pool)
            .await?;
        sqlx::query("UPDATE media_files SET is_healthy = false WHERE media_item_id = $1")
            .bind(movie)
            .execute(&pool)
            .await?;
        assert!(
            !playback_authorization(&pool, &user, session, &config)
                .await?
                .eligible
        );
        sqlx::query("UPDATE media_files SET is_healthy = true WHERE media_item_id = $1")
            .bind(movie)
            .execute(&pool)
            .await?;
        let other_profile = Uuid::now_v7();
        let other_session = Uuid::now_v7();
        sqlx::query("INSERT INTO user_profiles (id, owner_user_id, name) VALUES ($1, $2, 'Other')")
            .bind(other_profile)
            .bind(owner)
            .execute(&pool)
            .await?;
        user.profile_id = other_profile;
        assert!(
            !playback_authorization(&pool, &user, session, &config)
                .await?
                .eligible
        );
        sqlx::query("INSERT INTO play_sessions (id, user_id, profile_id, media_item_id, library_id, started_at, client_name, stream_decision, playback_mode) VALUES ($1, $2, $3, $4, $5, '2026-06-15', 'fixture', 'direct_play', 'interactive')")
            .bind(other_session).bind(owner).bind(other_profile).bind(movie).bind(library).execute(&pool).await?;
        let other = playback_authorization(&pool, &user, other_session, &config).await?;
        assert!(other.eligible);
        assert_ne!(other.opaque_profile_key, first.opaque_profile_key);
        user.profile_id = profile;
        user.has_all_library_access = false;
        assert!(
            !playback_authorization(&pool, &user, session, &config)
                .await?
                .eligible
        );
        user.has_all_library_access = true;
        user.profile_selection_required = true;
        assert!(
            !playback_authorization(&pool, &user, session, &config)
                .await?
                .eligible
        );
        user.profile_selection_required = false;
        user.profile_id = kids;
        assert!(
            !playback_authorization(&pool, &user, session, &config)
                .await?
                .eligible
        );
        assert!(
            put_catalog_mapping(
                &pool,
                &user,
                &canonical,
                &config.catalog_reference,
                request()
            )
            .await
            .is_err()
        );
        user.profile_id = profile;
        user.capabilities.clear();
        assert!(
            put_catalog_mapping(
                &pool,
                &user,
                &canonical,
                &config.catalog_reference,
                request()
            )
            .await
            .is_err()
        );
        user.capabilities.push("can_manage_server".into());
        sqlx::query("UPDATE play_sessions SET playback_mode = 'ambient' WHERE id = $1")
            .bind(session)
            .execute(&pool)
            .await?;
        assert!(
            !playback_authorization(&pool, &user, session, &config)
                .await?
                .eligible
        );
        sqlx::query("UPDATE play_sessions SET playback_mode = 'interactive' WHERE id = $1")
            .bind(session)
            .execute(&pool)
            .await?;
        let mut changed_identity = request();
        changed_identity.amazon_content_id = "DIFFERENT_ID".into();
        changed_identity.expected_revision = Some(1);
        assert!(matches!(
            put_catalog_mapping(
                &pool,
                &user,
                &canonical,
                &config.catalog_reference,
                changed_identity
            )
            .await,
            Err(FireError::Conflict)
        ));
        let mut disabled = request();
        disabled.expected_revision = Some(1);
        disabled.is_enabled = false;
        let mut competing = request();
        competing.expected_revision = Some(1);
        let (left, right) = tokio::join!(
            put_catalog_mapping(
                &pool,
                &user,
                &canonical,
                &config.catalog_reference,
                disabled
            ),
            put_catalog_mapping(
                &pool,
                &user,
                &canonical,
                &config.catalog_reference,
                competing
            ),
        );
        assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
        assert!(matches!(
            left.as_ref().err().or(right.as_ref().err()),
            Some(FireError::Conflict)
        ));
        let mut withdrawal = request();
        withdrawal.expected_revision = Some(2);
        withdrawal.is_enabled = false;
        put_catalog_mapping(
            &pool,
            &user,
            &canonical,
            &config.catalog_reference,
            withdrawal,
        )
        .await?;
        let denied = playback_authorization(&pool, &user, session, &config).await?;
        assert!(!denied.eligible);
        assert!(denied.catalog_content_id.is_none() && denied.opaque_profile_key.is_none());
        let events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM fire_catalog_mapping_events WHERE mapping_id = $1",
        )
        .bind(registered.mapping_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(events, 3);
        let mut expired = request();
        expired.expected_revision = Some(3);
        put_catalog_mapping(&pool, &user, &canonical, &config.catalog_reference, expired).await?;
        sqlx::query("UPDATE fire_catalog_mappings SET rights_expires_at = now() - interval '1 second' WHERE id = $1").bind(registered.mapping_id).execute(&pool).await?;
        assert!(
            !playback_authorization(&pool, &user, session, &config)
                .await?
                .eligible
        );
        sqlx::query("DELETE FROM media_items WHERE id = $1")
            .bind(movie)
            .execute(&pool)
            .await?;
        let tombstone: Option<Uuid> =
            sqlx::query_scalar("SELECT media_item_id FROM fire_catalog_mappings WHERE id = $1")
                .bind(registered.mapping_id)
                .fetch_one(&pool)
                .await?;
        assert!(tombstone.is_none());
        let replacement = Uuid::now_v7();
        sqlx::query("INSERT INTO media_items (id, library_id, type, title, sort_title) VALUES ($1, $2, 'movie', 'Replacement', 'Replacement')").bind(replacement).bind(library).execute(&pool).await?;
        let replacement_id = tv::build_platform_content_id(TvMediaType::Movie, replacement);
        assert!(matches!(
            put_catalog_mapping(
                &pool,
                &user,
                &replacement_id,
                &config.catalog_reference,
                request()
            )
            .await,
            Err(FireError::Conflict)
        ));
        pool.close().await;
        Ok(())
    }
}
