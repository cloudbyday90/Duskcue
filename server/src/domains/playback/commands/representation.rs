// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    database, delivery,
    types::{CommandRow, PROTOCOL, ReadyResponse, StatusResponse},
};
use crate::{
    domains::household_security::types::Scope,
    services::playback_admission::{AdmissionError, AdmissionOwner, Request, vod},
    state::AppState,
};
use std::sync::{Arc, atomic::Ordering};
use uuid::Uuid;

pub(super) async fn load(
    state: &AppState,
    owner: &Arc<AdmissionOwner>,
    scope: Scope,
    id: Uuid,
    cleanup: bool,
) -> Result<Option<(CommandRow, Request)>, AdmissionError> {
    let idle = state.runtime_config.load().auth.session_idle_timeout_hours;
    owner
        .sql(cleanup, move |conn| {
            Box::pin(async move {
                let revision = if cleanup {
                    database::cleanup_context(conn, scope, idle).await?;
                    0
                } else {
                    database::context(conn, scope, idle).await?
                };
                let Some(row) = database::read_scoped(conn, id, Some(scope.user)).await? else {
                    vod::missing(conn, id).await?;
                    return Ok(None);
                };
                let request = database::actor(id, &row, scope, revision, cleanup)?;
                Ok(Some((row, request)))
            })
        })
        .await
}

pub(super) async fn status(
    state: &AppState,
    owner: &Arc<AdmissionOwner>,
    scope: Scope,
    id: Uuid,
    cleanup: bool,
    intent: Option<[u8; 32]>,
) -> Result<StatusResponse, AdmissionError> {
    let Some((row, request)) = load(state, owner, scope, id, cleanup).await? else {
        let now = owner
            .sql(cleanup, |conn| {
                Box::pin(async move {
                    Ok(sqlx::query_scalar("SELECT clock_timestamp()")
                        .fetch_one(conn)
                        .await?)
                })
            })
            .await?;
        return Ok(StatusResponse {
            protocol: PROTOCOL,
            request_id: id,
            status: "unknown",
            status_url: format!("/api/v1/playback-commands/{id}"),
            observed_at: now,
            lease_deadline: None,
            stream_url: None,
            ready: None,
        });
    };
    if intent.is_some_and(|hash| row.intent_hash != hash) {
        return Err(AdmissionError::Conflict);
    }
    let mut ready = None;
    let mut stream_url = None;
    let status = if row.reservation_state == "released" {
        if row.state == "failed" {
            "failed"
        } else {
            "released"
        }
    } else if row.instance_id != owner.instance() {
        "uncertain"
    } else if row.reservation_state != "held" || row.deadline <= row.observed_at || cleanup {
        if !row.resources_completed
            && (row.cleanup_uncertain
                || state
                    .vod_commands
                    .get(id)
                    .is_some_and(|viewer| viewer.cleanup_uncertain.load(Ordering::Acquire)))
        {
            "cleanup_uncertain"
        } else {
            "stopping"
        }
    } else if row.state == "ready" {
        let local = state.vod_commands.get(id);
        if local.is_none() {
            "uncertain"
        } else if local.as_ref().is_some_and(|viewer| !viewer.group.active()) {
            "stopping"
        } else if local
            .as_ref()
            .is_some_and(|viewer| !viewer.published.load(Ordering::Acquire))
        {
            "preparing"
        } else {
            let viewer = delivery::authorize(state, owner, request).await?;
            let value: ReadyResponse =
                serde_json::from_value(row.result.ok_or(AdmissionError::Storage)?)
                    .map_err(|_| AdmissionError::Storage)?;
            if Some(value.session_id) != row.history_id
                || viewer.generation != row.generation_id
                || value.media_item_id != viewer.prepared.plan.source.media_item_id
                || value.media_file_id != viewer.prepared.plan.source.details.id
                || value.delivery_mode != viewer.prepared.binding.mode.as_str()
                || value.playback_mode != viewer.prepared.plan.source.playback_mode
                || value.audio_stream_index != viewer.prepared.plan.source.tracks.audio_stream_index
                || value.subtitle_stream_index
                    != viewer.prepared.plan.source.tracks.subtitle_stream_index
            {
                return Err(AdmissionError::Storage);
            }
            stream_url = Some(format!(
                "/api/v1/playback-commands/{id}/{}",
                if viewer.generation.is_some() {
                    "hls/manifest.m3u8"
                } else {
                    "file"
                }
            ));
            ready = Some(value);
            "ready"
        }
    } else if row.state == "preparing" {
        "preparing"
    } else {
        "stopping"
    };
    Ok(StatusResponse {
        protocol: PROTOCOL,
        request_id: id,
        status,
        status_url: format!("/api/v1/playback-commands/{id}"),
        observed_at: row.observed_at,
        lease_deadline: (row.reservation_state == "held").then_some(row.deadline),
        stream_url,
        ready,
    })
}
