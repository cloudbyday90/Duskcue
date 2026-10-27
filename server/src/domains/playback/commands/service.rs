// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    database,
    error::CommandError,
    preparation, representation,
    types::{RenewalResponse, StartRequest, StatusResponse},
};
use crate::{
    domains::household_security::types::Scope,
    services::playback_admission::{AdmissionError, ReservationState},
    state::AppState,
};
use uuid::Uuid;

pub async fn start(
    state: &AppState,
    scope: Scope,
    body: StartRequest,
) -> Result<StatusResponse, CommandError> {
    if scope.binding.is_none() {
        return Err(CommandError::Conflict);
    }
    let intent = body.intent()?;
    let owner = state
        .playback_admission
        .get()
        .cloned()
        .ok_or(CommandError::Unavailable)?;
    let state = state.clone();
    let output = state.transcode_manager.output.clone();
    let result = output
        .command(false, move |_observer| async move {
            let result = async {
                let id = intent.id;
                let hash = intent.hash;
                if representation::load(&state, &owner, scope, id, false)
                    .await?
                    .is_some()
                {
                    return representation::status(&state, &owner, scope, id, false, Some(hash))
                        .await;
                }
                let prepared = preparation::prepare(&state, &owner, scope, intent).await?;
                let request = prepared.request;
                let binding = prepared.binding;
                let viewer = state.vod_commands.register(prepared)?;
                let resources = viewer.clone();
                let resources_state = state.clone();
                let resources_owner = owner.clone();
                let result = owner
                    .start_vod(request, binding, move |cancel| async move {
                        resources
                            .run(resources_state, resources_owner, cancel)
                            .await
                    })
                    .await;
                match result {
                    Ok(receipt) if receipt.fresh && receipt.state == ReservationState::Held => {
                        return Ok(StatusResponse {
                            protocol: super::types::PROTOCOL,
                            request_id: id,
                            status: "preparing",
                            status_url: format!("/api/v1/playback-commands/{id}"),
                            observed_at: receipt.observed_at,
                            lease_deadline: Some(receipt.deadline),
                            stream_url: None,
                            ready: None,
                        });
                    }
                    Ok(_) => {
                        state.vod_commands.remove(&viewer)?;
                    }
                    Err(AdmissionError::Indeterminate) => {
                        state.vod_commands.remove(&viewer)?;
                        return Err(AdmissionError::Indeterminate);
                    }
                    Err(error) => {
                        state.vod_commands.remove(&viewer)?;
                        return Err(error);
                    }
                }
                representation::status(&state, &owner, scope, id, false, Some(hash)).await
            }
            .await;
            Ok(result)
        })
        .await
        .map_err(|_| CommandError::Unavailable)?;
    result.map_err(CommandError::from)
}

pub async fn status(
    state: &AppState,
    scope: Scope,
    id: Uuid,
) -> Result<StatusResponse, CommandError> {
    let owner = state
        .playback_admission
        .get()
        .ok_or(CommandError::Unavailable)?;
    Ok(representation::status(state, owner, scope, id, false, None).await?)
}

pub async fn renew(
    state: &AppState,
    scope: Scope,
    id: Uuid,
) -> Result<RenewalResponse, CommandError> {
    if scope.binding.is_none() {
        return Err(CommandError::Conflict);
    }
    let owner = state
        .playback_admission
        .get()
        .ok_or(CommandError::Unavailable)?;
    let (_, request) = representation::load(state, owner, scope, id, false)
        .await?
        .ok_or(CommandError::Conflict)?;
    if let Some(viewer) = state.vod_commands.get(id)
        && (!viewer.group.active()
            || (viewer.published.load(std::sync::atomic::Ordering::Acquire)
                && viewer
                    .generation
                    .is_some_and(|generation| !state.transcode_manager.output_usable(generation))))
    {
        owner.stop(id)?;
        return Err(CommandError::Conflict);
    }
    let receipt = owner.renew(request).await?;
    Ok(RenewalResponse {
        protocol: super::types::PROTOCOL,
        request_id: id,
        status: "renewed",
        status_url: format!("/api/v1/playback-commands/{id}"),
        observed_at: receipt.observed_at,
        lease_deadline: receipt.deadline,
    })
}

pub async fn stop(
    state: &AppState,
    scope: Scope,
    id: Uuid,
) -> Result<StatusResponse, CommandError> {
    let owner = state
        .playback_admission
        .get()
        .ok_or(CommandError::Unavailable)?;
    let Some((row, request)) = representation::load(state, owner, scope, id, true).await? else {
        return Err(CommandError::Conflict);
    };
    let mut observed = row.observed_at;
    let result = if row.instance_id == owner.instance() && row.reservation_state != "released" {
        let idle = state.runtime_config.load().auth.session_idle_timeout_hours;
        observed=owner.sql(true,move|conn|Box::pin(async move {
            database::cleanup_context(conn,scope,idle).await?;
            let row=database::read_scoped(conn,id,Some(scope.user)).await?.ok_or(AdmissionError::Conflict)?;
            database::actor(id,&row,scope,0,true)?;
            sqlx::query("UPDATE playback_reservations SET state='retiring' WHERE id=$1 AND state='held'").bind(id).execute(&mut *conn).await?;
            sqlx::query("UPDATE playback_vod_commands SET state='stopping' WHERE reservation_id=$1 AND state IN ('preparing','ready')").bind(id).execute(&mut *conn).await?;
            Ok(sqlx::query_scalar("SELECT clock_timestamp()").fetch_one(conn).await?)
        })).await?;
        owner.stop(request.id)?;
        if let Some(viewer) = state.vod_commands.get(id) {
            viewer.retry.notify_one();
        }
        "stopping"
    } else if row.reservation_state == "released" {
        if row.state == "failed" {
            "failed"
        } else {
            "released"
        }
    } else {
        "uncertain"
    };
    let mut receipt = StatusResponse::receipt(id, result, observed, None);
    receipt.status_url = format!("/api/v1/playback-commands/{id}/retirement");
    Ok(receipt)
}

pub async fn retirement(
    state: &AppState,
    scope: Scope,
    id: Uuid,
) -> Result<StatusResponse, CommandError> {
    let owner = state
        .playback_admission
        .get()
        .ok_or(CommandError::Unavailable)?;
    let Some((row, _)) = representation::load(state, owner, scope, id, true).await? else {
        return Err(CommandError::Conflict);
    };
    if row.instance_id == owner.instance()
        && row.reservation_state == "held"
        && row.deadline > row.observed_at
    {
        return Err(CommandError::Conflict);
    }
    let mut response = representation::status(state, owner, scope, id, true, None).await?;
    response.status_url = format!("/api/v1/playback-commands/{id}/retirement");
    Ok(response)
}
