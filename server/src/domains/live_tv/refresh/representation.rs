// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{authority, error::RefreshError, types::ResourceResponse};
use crate::error::AppError;
use serde::Serialize;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

pub(super) fn etag(value: &impl Serialize) -> Result<String, AppError> {
    Ok(crate::cache::sha256_etag(
        &serde_json::to_vec(value).map_err(|_| RefreshError::Unavailable)?,
    ))
}
pub(super) async fn resource(
    conn: &mut PgConnection,
    user: Uuid,
) -> Result<ResourceResponse, AppError> {
    let control = authority::control(conn, false).await?;
    let row=sqlx::query("SELECT id,revision,source_generation,enabled,playlist_address_count,guide_address_count FROM live_tv_assignments WHERE household_user_id=$1 FOR UPDATE")
        .bind(user).fetch_optional(&mut *conn).await.map_err(|_|RefreshError::Unavailable)?.ok_or(RefreshError::NotFound)?;
    let latest: Option<(Uuid, i64)> = sqlx::query_as(
        "SELECT attempt_id,attempt_sequence FROM live_tv_import_state WHERE assignment_id=$1",
    )
    .bind(row.get::<Uuid, _>("id"))
    .fetch_optional(conn)
    .await
    .map_err(|_| RefreshError::Unavailable)?;
    Ok(ResourceResponse {
        assignment_id: row.get("id"),
        revision: row.get("revision"),
        source_generation: row.get("source_generation"),
        assignment_enabled: row.get("enabled"),
        playlist_address_count: row.get("playlist_address_count"),
        guide_address_count: row.get("guide_address_count"),
        control,
        latest_attempt_id: latest.map(|v| v.0),
        latest_attempt_sequence: latest.map(|v| v.1),
    })
}
