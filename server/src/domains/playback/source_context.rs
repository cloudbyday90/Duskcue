// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;

use super::PlaybackError;
use super::direct::{FileIdentity, PreparedFallback};
use super::types::PlaybackActor;
use super::types::{CopyFallbackResponse, SourceRouteResponse};
use crate::services::transcoding::{TranscodeManager, plan::StreamAction};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct AcceptedRoute {
    pub version: u8,
    pub candidate_id: String,
    pub source_revision: String,
    pub identity: FileIdentity,
    pub duration_ms: i64,
    pub fallback: Option<PreparedFallback>,
}

pub(super) fn attach(metadata: &mut Value, route: &AcceptedRoute) {
    metadata["source_route_context"] = json!(route);
}

pub(super) fn has_context(metadata: &Value) -> bool {
    metadata
        .get("source_route_context")
        .is_some_and(|value| !value.is_null())
}

pub(super) async fn revalidate(
    connection: &mut PgConnection,
    actor: PlaybackActor,
    media_item_id: uuid::Uuid,
    metadata: &Value,
) -> Result<(), PlaybackError> {
    let route = read(metadata).ok_or(PlaybackError::FileNotFound)?;
    let scope = crate::domains::profiles::service::load_profile_scope_on_connection(
        connection,
        actor.user_id,
        actor.profile_id,
        actor.has_all_library_access,
    )
    .await
    .map_err(|_| PlaybackError::AccessDenied)?;
    crate::domains::profiles::service::assert_media_access_on_connection(
        connection,
        &scope,
        media_item_id,
    )
    .await
    .map_err(|_| PlaybackError::AccessDenied)?;
    let details = super::media_source::fetch_media_file_details_on_connection(
        connection,
        route.identity.file_id,
        media_item_id,
    )
    .await?;
    if FileIdentity::read(&details).await.as_ref() != Some(&route.identity) {
        return Err(PlaybackError::FileNotFound);
    }
    Ok(())
}

pub(super) fn read(metadata: &Value) -> Option<AcceptedRoute> {
    let value = metadata.get("source_route_context")?;
    if serde_json::to_vec(value).ok()?.len() > 16_384 {
        return None;
    }
    let route: AcceptedRoute = serde_json::from_value(value.clone()).ok()?;
    (route.version == 1
        && matches!(
            route.candidate_id.as_str(),
            "native_file" | "hls_video_copy_audio_copy" | "hls_video_copy_aac"
        )
        && valid_revision(&route.source_revision))
    .then_some(route)
    .filter(|route| (1..=i64::from(i32::MAX)).contains(&route.duration_ms))
}

pub(super) fn valid_revision(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn response(metadata: &Value) -> Option<SourceRouteResponse> {
    let route = read(metadata)?;
    Some(SourceRouteResponse {
        candidate_id: route.candidate_id,
        source_revision: route.source_revision,
    })
}

pub(super) fn copy_fallback(
    manager: &TranscodeManager,
    metadata: &Value,
) -> Option<CopyFallbackResponse> {
    let route = read(metadata)?;
    if !route.candidate_id.starts_with("hls_") {
        return None;
    }
    let generation = super::recovery::transcode_id(metadata)?;
    let session = manager.get_session(&generation)?;
    let fallback = route.fallback?;
    if session.stream_plan.video != StreamAction::Copy
        || session.output_profile
            != Some(crate::services::transcoding::OutputProfile::SourceCopySdr)
        || route.identity != fallback.identity
        || session.media_file_id != fallback.identity.file_id
        || session.source_video_stream_index != Some(fallback.tracks.video)
        || session.source_audio_stream_index != Some(fallback.tracks.audio)
        || !super::direct::descriptor_valid(&fallback.descriptor)
    {
        return None;
    }
    Some(CopyFallbackResponse {
        generation,
        descriptor: fallback.descriptor,
    })
}
