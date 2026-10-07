// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use sqlx::PgPool;
use uuid::Uuid;

use super::PlaybackError;
use crate::domains::profiles::service as profiles;
use crate::error::AppError;

pub async fn assert_transcode_profile_access(
    pool: &PgPool,
    user_id: Uuid,
    profile_id: Uuid,
    has_all_library_access: bool,
    transcode_session_id: Uuid,
) -> Result<(), AppError> {
    let media_item_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT media_item_id FROM play_sessions \
         WHERE metadata ->> 'transcode_session_id' = $1 \
           AND user_id = $2 AND profile_id = $3 AND stopped_at IS NULL",
    )
    .bind(transcode_session_id.to_string())
    .bind(user_id)
    .bind(profile_id)
    .fetch_optional(pool)
    .await
    .map_err(PlaybackError::from)?
    .ok_or(PlaybackError::SessionNotFound)?;
    let scope =
        profiles::load_profile_scope(pool, user_id, profile_id, has_all_library_access).await?;
    profiles::assert_media_access(pool, &scope, media_item_id).await?;
    Ok(())
}
