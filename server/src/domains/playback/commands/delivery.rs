// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{output::Output, representation};
use crate::{
    domains::{household_security::types::Scope, playback},
    services::playback_admission::{AdmissionError, AdmissionOwner, Request, vod::revisions},
    state::AppState,
};
use axum::{http::HeaderMap, response::Response};
use std::sync::{Arc, atomic::Ordering};
use uuid::Uuid;

pub(super) async fn authorize(
    state: &AppState,
    owner: &Arc<AdmissionOwner>,
    request: Request,
    expected: Uuid,
) -> Result<Arc<Output>, AdmissionError> {
    let viewer = state
        .vod_commands
        .get(request.id)
        .ok_or(AdmissionError::Indeterminate)?;
    if !viewer.group.active() {
        return Err(AdmissionError::Retired);
    }
    let output = viewer.current()?;
    if output.id != expected {
        return Err(AdmissionError::Conflict);
    }
    if !output.published.load(Ordering::Acquire)
        || !output.group.active()
        || output
            .generation
            .is_some_and(|id| !state.transcode_manager.output_usable(id))
    {
        return Err(AdmissionError::Indeterminate);
    }
    let current = output.clone();
    let authorized = owner
        .active_sql(request, move |conn| {
            Box::pin(async move {
                let control = revisions::control(conn, request.id).await?;
                if control.state != "ready"
                    || control.current_output != Some(expected)
                    || control.pending_operation.is_some()
                {
                    return Err(AdmissionError::Conflict);
                }
                let row = revisions::output(conn, request.id, expected).await?;
                if row.state != "ready"
                    || row.resources_completed
                    || row.generation_id != current.generation
                    || row.delivery_mode != current.prepared.binding.mode.as_str()
                    || row.encodes != current.prepared.binding.encodes
                    || row.configuration != current.configuration()
                {
                    return Err(AdmissionError::Conflict);
                }
                let source = &current.prepared.plan.source;
                let details = playback::media_source::fetch_media_file_details_on_connection(
                    conn,
                    source.details.id,
                    source.media_item_id,
                )
                .await
                .map_err(|_| AdmissionError::Authority)?;
                if !current.prepared.identity.matches_details(&details) {
                    return Err(AdmissionError::Authority);
                }
                Ok(())
            })
        })
        .await;
    if let Err(error) = authorized {
        if matches!(
            error,
            AdmissionError::Authority | AdmissionError::Policy | AdmissionError::Retired
        ) {
            let _ = owner.stop(request.id);
        }
        return Err(error);
    }
    if output.identity(state).await.is_err() {
        let _ = owner.stop(request.id);
        return Err(AdmissionError::Authority);
    }
    Ok(output)
}

async fn selected(
    state: &AppState,
    scope: Scope,
    id: Uuid,
    expected: Uuid,
) -> Result<Arc<Output>, AdmissionError> {
    let owner = state
        .playback_admission
        .get()
        .ok_or(AdmissionError::Closed)?;
    let (row, request) = representation::load(state, owner, scope, id, false)
        .await?
        .ok_or(AdmissionError::Authority)?;
    if row.protocol != 2 || row.state != "ready" || row.instance_id != owner.instance() {
        return Err(AdmissionError::Retired);
    }
    authorize(state, owner, request, expected).await
}

pub(super) async fn file(
    state: &AppState,
    scope: Scope,
    id: Uuid,
    expected: Uuid,
    headers: &HeaderMap,
) -> Result<Response, super::error::CommandError> {
    let output = selected(state, scope, id, expected).await?;
    if output.generation.is_some() {
        return Err(super::error::CommandError::Conflict);
    }
    playback::file_delivery::response_scoped(
        &state.transcode_manager.output,
        std::path::Path::new(&output.prepared.identity.path),
        headers,
        Some(output.group.clone()),
        Some(output.prepared.identity.clone()),
    )
    .await
    .map_err(|_| super::error::CommandError::Unavailable)
}

pub(super) async fn hls(
    state: &AppState,
    scope: Scope,
    id: Uuid,
    expected: Uuid,
    rendition: Option<&str>,
    segment: Option<&str>,
) -> Result<Response, super::error::CommandError> {
    let output = selected(state, scope, id, expected).await?;
    let generation = output
        .generation
        .ok_or(super::error::CommandError::Conflict)?;
    let group = Some((id, output.id, output.group.clone()));
    let result = match (rendition, segment) {
        (None, None) => {
            playback::hls::manifest_scoped(&state.transcode_manager, generation, group).await
        }
        (Some(rendition), None) => {
            playback::hls::playlist_scoped(&state.transcode_manager, generation, rendition, group)
                .await
        }
        (Some(rendition), Some(segment)) => {
            playback::hls::segment_scoped(
                &state.transcode_manager,
                generation,
                rendition,
                segment,
                group,
            )
            .await
        }
        _ => return Err(super::error::CommandError::Invalid),
    };
    result.map_err(|_| super::error::CommandError::Unavailable)
}
