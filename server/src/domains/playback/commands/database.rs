// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::types::CommandRow;
use crate::{
    domains::{
        household_security::{context, types::Scope},
        live_tv::catalog::authority,
    },
    services::playback_admission::{AdmissionError, Request, vod},
};
use sqlx::PgConnection;
use uuid::Uuid;

pub(super) async fn context(
    conn: &mut PgConnection,
    scope: Scope,
    idle: Option<i32>,
) -> Result<i64, AdmissionError> {
    if scope.transport == crate::extractors::CredentialTransport::Legacy {
        return Err(AdmissionError::Authority);
    }
    vod::lock(conn, scope.user).await?;
    let active: bool =
        sqlx::query_scalar("SELECT status='active' AND deleted_at IS NULL FROM users WHERE id=$1")
            .bind(scope.user)
            .fetch_one(&mut *conn)
            .await?;
    if !active {
        return Err(AdmissionError::Authority);
    }
    context::check(conn, scope)
        .await
        .map_err(|_| AdmissionError::Conflict)?;
    authority::session(conn, scope, None, idle)
        .await
        .map_err(|_| AdmissionError::Authority)
}

pub(super) async fn read(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<CommandRow>, AdmissionError> {
    read_scoped(conn, id, None).await
}

pub(super) async fn read_scoped(
    conn: &mut PgConnection,
    id: Uuid,
    user: Option<Uuid>,
) -> Result<Option<CommandRow>, AdmissionError> {
    Ok(sqlx::query_as("SELECT r.fingerprint,r.instance_id,r.state AS reservation_state,r.deadline,clock_timestamp() AS observed_at,c.intent_hash,c.state,c.result,c.history_id,c.history_started_at,c.generation_id,c.resources_completed,c.cleanup_uncertain FROM playback_reservations r JOIN playback_vod_commands c ON c.reservation_id=r.id WHERE r.id=$1 AND ($2::uuid IS NULL OR r.household_id=$2) FOR UPDATE OF r,c")
        .bind(id).bind(user).fetch_optional(conn).await?)
}

pub(super) async fn cleanup_context(
    conn: &mut PgConnection,
    scope: Scope,
    idle: Option<i32>,
) -> Result<(), AdmissionError> {
    vod::lock(conn, scope.user).await?;
    let valid:Option<bool>=sqlx::query_scalar("SELECT s.credential_version=$3 AND s.expires_at>clock_timestamp() AND ($4::int IS NULL OR $4<=0 OR s.last_active_at>clock_timestamp()-make_interval(hours=>$4)) AND u.status='active' AND u.deleted_at IS NULL FROM user_sessions s JOIN users u ON u.id=s.user_id WHERE s.id=$1 AND s.user_id=$2 FOR UPDATE OF s")
        .bind(scope.session).bind(scope.user).bind(scope.version).bind(idle).fetch_optional(conn).await?;
    if valid != Some(true) || scope.transport == crate::extractors::CredentialTransport::Legacy {
        return Err(AdmissionError::Authority);
    }
    Ok(())
}

pub(super) fn actor(
    id: Uuid,
    row: &CommandRow,
    scope: Scope,
    revision: i64,
    cleanup: bool,
) -> Result<Request, AdmissionError> {
    let mut request = Request::decode(id, &row.fingerprint)?;
    let original = request.actor;
    if original.user != scope.user
        || original.session != scope.session
        || original.version != scope.version
        || original.transport != scope.transport
        || (!cleanup && (original.profile != scope.profile || request.profile_revision != revision))
    {
        return Err(AdmissionError::Conflict);
    }
    if !cleanup {
        request.actor.binding = scope.binding;
    }
    Ok(request)
}

pub(super) async fn failed(conn: &mut PgConnection, id: Uuid) -> Result<(), AdmissionError> {
    sqlx::query("UPDATE playback_vod_commands SET state='failed' WHERE reservation_id=$1 AND state IN ('preparing','ready')").bind(id).execute(conn).await?;
    Ok(())
}
