// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{ownership::Viewer, representation};
use crate::{
    domains::{household_security::types::Scope, playback},
    services::playback_admission::{AdmissionError, AdmissionOwner, Request},
    state::AppState,
};
use axum::{http::HeaderMap, response::Response};
use std::sync::{Arc, atomic::Ordering};
use uuid::Uuid;

pub(super) async fn authorize(
    state: &AppState,
    owner: &Arc<AdmissionOwner>,
    request: Request,
) -> Result<Arc<Viewer>, AdmissionError> {
    let viewer = state
        .vod_commands
        .get(request.id)
        .ok_or(AdmissionError::Indeterminate)?;
    if !viewer.published.load(Ordering::Acquire)
        || !viewer.group.active()
        || viewer
            .generation
            .is_some_and(|id| !state.transcode_manager.output_usable(id))
    {
        let _ = owner.stop(request.id);
        return Err(AdmissionError::Retired);
    }
    let output = viewer.clone();
    let authorized = owner
        .active_sql(request, move |conn| {
            Box::pin(async move {
                let source = &output.prepared.plan.source;
                let details = playback::media_source::fetch_media_file_details_on_connection(
                    conn,
                    source.details.id,
                    source.media_item_id,
                )
                .await
                .map_err(|_| AdmissionError::Authority)?;
                if !output.prepared.identity.matches_details(&details) {
                    return Err(AdmissionError::Authority);
                }
                Ok(())
            })
        })
        .await;
    if let Err(error) = authorized {
        if matches!(
            error,
            AdmissionError::Authority
                | AdmissionError::Policy
                | AdmissionError::Retired
                | AdmissionError::Conflict
        ) {
            let _ = owner.stop(request.id);
        }
        return Err(error);
    }
    let identity = playback::direct::FileIdentity::read_scoped(
        &state.transcode_manager.output,
        &viewer.group,
        &viewer.prepared.plan.source.details,
    )
    .await;
    if identity.as_ref() != Some(&viewer.prepared.identity) {
        let _ = owner.stop(request.id);
        return Err(AdmissionError::Authority);
    }
    Ok(viewer)
}

async fn selected(state: &AppState, scope: Scope, id: Uuid) -> Result<Arc<Viewer>, AdmissionError> {
    let owner = state
        .playback_admission
        .get()
        .ok_or(AdmissionError::Closed)?;
    let (row, request) = representation::load(state, owner, scope, id, false)
        .await?
        .ok_or(AdmissionError::Authority)?;
    if row.state != "ready" || row.instance_id != owner.instance() {
        return Err(AdmissionError::Retired);
    }
    authorize(state, owner, request).await
}

pub(super) async fn file(
    state: &AppState,
    scope: Scope,
    id: Uuid,
    headers: &HeaderMap,
) -> Result<Response, super::error::CommandError> {
    let viewer = selected(state, scope, id).await?;
    if viewer.generation.is_some() {
        return Err(super::error::CommandError::Conflict);
    }
    playback::file_delivery::response_scoped(
        &state.transcode_manager.output,
        std::path::Path::new(&viewer.prepared.identity.path),
        headers,
        Some(viewer.group.clone()),
        Some(viewer.prepared.identity.clone()),
    )
    .await
    .map_err(|_| super::error::CommandError::Unavailable)
}

pub(super) async fn hls(
    state: &AppState,
    scope: Scope,
    id: Uuid,
    rendition: Option<&str>,
    segment: Option<&str>,
) -> Result<Response, super::error::CommandError> {
    let viewer = selected(state, scope, id).await?;
    let generation = viewer
        .generation
        .ok_or(super::error::CommandError::Conflict)?;
    let group = Some((id, viewer.group.clone()));
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
