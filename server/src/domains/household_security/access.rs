// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{error::OwnerError, transactions, types::Scope};
use sqlx::PgConnection;

pub(crate) async fn allowed(
    conn: &mut PgConnection,
    actor: Scope,
    capability: &str,
) -> Result<bool, OwnerError> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM users WHERE id=$1 AND status='active' AND deleted_at IS NULL",
    )
    .bind(actor.user)
    .fetch_optional(&mut *conn)
    .await?;
    let role = role.ok_or(OwnerError::InvalidProof)?;
    transactions::session_scope_conn(conn, actor, true).await?;
    let value: Option<bool> = sqlx::query_scalar(
        "SELECT is_granted FROM user_capabilities WHERE user_id=$1 AND capability=$2",
    )
    .bind(actor.user)
    .bind(capability)
    .fetch_optional(conn)
    .await?;
    Ok(role == "owner" || value.unwrap_or(role == "admin"))
}
