// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::PlaybackError;
use sqlx::PgPool;
use uuid::Uuid;

pub(super) async fn session(pool: &PgPool, id: Uuid) -> Result<(), PlaybackError> {
    let bound:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM play_sessions WHERE id=$1 AND metadata ? 'vod_command_protocol')").bind(id).fetch_one(pool).await?;
    if bound {
        return Err(PlaybackError::SessionNotFound);
    }
    Ok(())
}
