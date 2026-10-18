// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use sqlx::{PgConnection, Row};
use uuid::Uuid;

use super::error::ProfilesError;
use super::service::is_media_allowed;
use super::types::ProfileScope;

pub async fn load_profile_scope_on_connection(
    connection: &mut PgConnection,
    owner_user_id: Uuid,
    profile_id: Uuid,
    has_all_library_access: bool,
) -> Result<ProfileScope, ProfilesError> {
    let profile = sqlx::query(
        "SELECT profile_type, max_content_rating, allow_search, allow_downloads, \
         allow_external_links, allow_ambient_channels \
         FROM user_profiles WHERE id = $1 AND owner_user_id = $2",
    )
    .bind(profile_id)
    .bind(owner_user_id)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or(ProfilesError::NotFound)?;
    let library_ids = sqlx::query_scalar(
        "SELECT library_id FROM profile_library_access WHERE profile_id = $1 ORDER BY library_id",
    )
    .bind(profile_id)
    .fetch_all(&mut *connection)
    .await?;
    let user_library_ids = if has_all_library_access {
        Vec::new()
    } else {
        sqlx::query_scalar("SELECT library_id FROM user_library_access WHERE user_id = $1")
            .bind(owner_user_id)
            .fetch_all(&mut *connection)
            .await?
    };
    Ok(ProfileScope {
        profile_id,
        owner_user_id,
        profile_type: profile.try_get("profile_type")?,
        max_content_rating: profile.try_get("max_content_rating")?,
        allow_search: profile.try_get("allow_search")?,
        allow_downloads: profile.try_get("allow_downloads")?,
        allow_external_links: profile.try_get("allow_external_links")?,
        allow_ambient_channels: profile.try_get("allow_ambient_channels")?,
        library_ids,
        user_library_ids,
        has_all_library_access,
    })
}

pub async fn assert_media_access_on_connection(
    connection: &mut PgConnection,
    scope: &ProfileScope,
    media_item_id: Uuid,
) -> Result<(), ProfilesError> {
    let row = sqlx::query(
        "SELECT mi.library_id, mi.content_rating FROM media_items mi \
         JOIN libraries l ON l.id = mi.library_id \
         WHERE mi.id = $1 AND l.deleted_at IS NULL",
    )
    .bind(media_item_id)
    .fetch_optional(connection)
    .await?
    .ok_or(ProfilesError::ContentNotAllowed)?;
    let library_id: Uuid = row.try_get("library_id")?;
    let content_rating: Option<String> = row.try_get("content_rating").ok().flatten();
    if is_media_allowed(scope, library_id, content_rating.as_deref()) {
        Ok(())
    } else {
        Err(ProfilesError::ContentNotAllowed)
    }
}
