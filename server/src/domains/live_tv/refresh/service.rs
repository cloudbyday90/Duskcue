// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{authority, database, error::RefreshError, representation, types::*};
use crate::{
    domains::{
        household_security::types::Scope,
        live_tv::{assignment::preconditions::Condition, feed::imports},
    },
    error::AppError,
    state::AppState,
};
use uuid::Uuid;

pub(super) async fn control(state: &AppState, actor: Scope) -> Result<ControlResponse, AppError> {
    database::write(&state.pool, async |conn| {
        authority::authorize(conn, actor, actor.user, true, false).await?;
        authority::control(conn, false).await
    })
    .await
}
pub(super) async fn configure(
    state: &AppState,
    actor: Scope,
    condition: Condition,
    value: ControlRequest,
) -> Result<ControlResponse, AppError> {
    let result = database::write(&state.pool, async |conn| {
        authority::authorize(conn, actor, actor.user, true, true).await?;
        let mut current = authority::control(conn, true).await?;
        condition.check(Some(&representation::etag(&current)?))?;
        if current.enabled != value.enabled {
            current.revision = sqlx::query_scalar("UPDATE live_tv_refresh_control SET enabled=$1,revision=revision+1 WHERE singleton RETURNING revision")
                .bind(value.enabled).fetch_one(&mut *conn).await.map_err(|_| RefreshError::Unavailable)?;
            current.enabled = value.enabled;
            sqlx::query("INSERT INTO audit_log(table_name,row_id,operation,new_data,user_id,db_user,application_name) VALUES ('live_tv_refresh_control',$1,'UPDATE',jsonb_build_object('event','configured','revision',$2::bigint,'enabled',$3::boolean),$4,current_user,'duskcue-live-tv-refresh')")
                .bind(current.id).bind(current.revision).bind(current.enabled).bind(actor.user)
                .execute(conn).await.map_err(|_| RefreshError::Unavailable)?;
        }
        Ok(current)
    }).await;
    if result.as_ref().is_ok_and(|v| !v.enabled)
        && let Some(manager) = state.live_tv_refresh.get()
    {
        manager.cancel_active();
    }
    result
}
pub(super) async fn resource(
    state: &AppState,
    actor: Scope,
    target: Uuid,
) -> Result<ResourceResponse, AppError> {
    database::write(&state.pool, async |conn| {
        authority::authorize(conn, actor, target, false, false).await?;
        representation::resource(conn, target).await
    })
    .await
}
pub(super) async fn status(
    state: &AppState,
    actor: Scope,
    target: Uuid,
    query: StatusQuery,
) -> Result<StatusResponse, AppError> {
    let latest = database::write(&state.pool, async |conn| {
        authority::authorize(conn, actor, target, false, false).await?;
        let _ = representation::resource(conn, target).await?;
        imports::status::load(conn, &state.encryption_key, target)
            .await
            .map_err(|_| RefreshError::Unavailable.into())
    })
    .await?;
    let locally_owned = latest.as_ref().is_some_and(|s| {
        state
            .live_tv_refresh
            .get()
            .is_some_and(|m| m.owns(s.attempt_id))
    });
    let observation = match &latest {
        Some(status) if query.attempt_id.is_some_and(|id| id != status.attempt_id) => "not_current",
        Some(status) if status.state == "running" && !locally_owned => "unfinished_uncertain",
        Some(_) => "current",
        None => "never_imported",
    };
    Ok(StatusResponse {
        latest,
        expected_attempt_id: query.attempt_id,
        observation,
        locally_owned,
        service_available: state.live_tv_refresh.get().is_some_and(|m| m.available()),
    })
}
