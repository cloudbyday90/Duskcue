// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::PlaybackError;
use super::history::emit_play_event;
use super::types::{HeartbeatRequest, HeartbeatResponse, PlaybackActor, RecoveryResponse};
use crate::services::transcoding::recovery::RecoveryBudget;
use crate::services::transcoding::{
    FailureClass, PendingTranscodeSession, TranscodeManager, TranscodeSession,
};

pub(super) struct RecoveryContext<'a> {
    pub manager: &'a TranscodeManager,
    pub actor: PlaybackActor,
    pub session_id: Uuid,
    pub media_item_id: Uuid,
    pub request: &'a HeartbeatRequest,
    pub data_dir: &'a Path,
}

pub(super) fn transcode_id(metadata: &Value) -> Option<Uuid> {
    metadata
        .get("transcode_session_id")
        .and_then(Value::as_str)
        .and_then(|id| Uuid::parse_str(id).ok())
}

pub(super) fn startup_recovery(session: &TranscodeSession) -> Option<RecoveryResponse> {
    (session.recovery_attempts > 0).then_some(RecoveryResponse {
        status: "recovered",
        attempts: session.recovery_attempts,
        reason: "hardware",
        skipped_ms: 0,
        quality_reduced: false,
    })
}

pub(super) fn stored_recovery(metadata: &Value) -> Option<RecoveryResponse> {
    let record = metadata.get("recovery")?;
    let status = match record.get("status")?.as_str()? {
        "recovered" => "recovered",
        "exhausted" => "exhausted",
        _ => return None,
    };
    let reason = match record.get("reason")?.as_str()? {
        "hardware" => "hardware",
        "decode" => "decode",
        "stalled" => "stalled",
        "container" => "container",
        "unknown" => "unknown",
        "budget_exhausted" => "budget_exhausted",
        "replacement_failed" => "replacement_failed",
        "cleanup_failed" => "cleanup_failed",
        "session_lost" => "session_lost",
        _ => return None,
    };
    Some(RecoveryResponse {
        status,
        reason,
        attempts: record
            .get("attempts")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(255) as u8,
        skipped_ms: record
            .get("skipped_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(0),
        quality_reduced: record
            .get("quality_reduced")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub(super) fn response(
    manager: &TranscodeManager,
    session_id: Uuid,
    playback_mode: String,
    metadata: &Value,
    recovery: Option<RecoveryResponse>,
) -> HeartbeatResponse {
    let id = transcode_id(metadata);
    let session = id.and_then(|id| manager.get_session(&id));
    HeartbeatResponse {
        session_id,
        playback_mode,
        position_ms: metadata
            .get("current_position_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, i64::from(i32::MAX)) as i32,
        transcode_session_id: id,
        stream_url: id.map(|id| format!("/api/v1/transcode/{id}/manifest.m3u8")),
        stream_offset_ms: id.map(|_| {
            session
                .as_ref()
                .map(|session| session.source_start_ms)
                .unwrap_or_else(|| {
                    metadata
                        .get("stream_offset_ms")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                })
        }),
        recovery: recovery
            .or_else(|| stored_recovery(metadata))
            .or_else(|| session.as_ref().and_then(startup_recovery)),
    }
}

pub(super) async fn recover(
    connection: &mut PgConnection,
    context: RecoveryContext<'_>,
    metadata: &mut Value,
) -> Result<(Option<PendingTranscodeSession>, Option<RecoveryResponse>), PlaybackError> {
    let RecoveryContext {
        manager,
        actor,
        session_id,
        media_item_id,
        request,
        data_dir,
    } = context;
    let Some(id) = transcode_id(metadata) else {
        return Ok((None, None));
    };
    if request.transcode_session_id != Some(id) {
        return Ok((None, None));
    }
    let mut budget: RecoveryBudget = match metadata.get("recovery_budget") {
        Some(value) => serde_json::from_value(value.clone()).unwrap_or(RecoveryBudget {
            exhausted: true,
            ..Default::default()
        }),
        None => RecoveryBudget::default(),
    };
    if budget.exhausted {
        return Ok((None, stored_recovery(metadata)));
    }
    let failure = manager.session_failure(id);
    if failure.is_none() && manager.get_session(&id).is_some() {
        return Ok((
            None,
            request.recovery_requested.then_some(RecoveryResponse {
                status: "exhausted",
                attempts: budget.attempts,
                reason: "not_eligible",
                skipped_ms: 0,
                quality_reduced: false,
            }),
        ));
    }
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
    let mut status = RecoveryResponse {
        status: "exhausted",
        attempts: budget.attempts,
        reason: "session_lost",
        skipped_ms: 0,
        quality_reduced: false,
    };
    let mut pending = None;
    if let (Some(failure), Some(session)) = (failure, manager.get_session(&id)) {
        let position = metadata
            .get("current_position_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        if let Some(action) = budget.reserve(failure, &session, position) {
            status.attempts = budget.attempts;
            status.reason = match failure {
                FailureClass::Hardware => "hardware",
                FailureClass::Decode => "decode",
                FailureClass::Container => "container",
                FailureClass::Stalled => "stalled",
                FailureClass::Unknown => "unknown",
            };
            match manager
                .recover_session(session, action, budget.attempts, data_dir)
                .await
            {
                Ok(replacement) => {
                    pending = Some(manager.pending_session(replacement.id));
                    status.status = "recovered";
                    status.skipped_ms = action.skipped_ms;
                    status.quality_reduced = action.quality_reduced;
                    metadata["transcode_session_id"] = json!(replacement.id);
                    metadata["current_position_ms"] = json!(action.position_ms);
                    metadata["stream_offset_ms"] = json!(replacement.source_start_ms);
                }
                Err(_) => {
                    budget.exhausted = true;
                    status.reason = "replacement_failed";
                }
            }
        } else {
            status.attempts = budget.attempts;
            status.reason = if budget.attempts
                >= crate::services::transcoding::recovery::MAX_RECOVERY_ATTEMPTS
            {
                "budget_exhausted"
            } else {
                match failure {
                    FailureClass::Hardware => "hardware",
                    FailureClass::Decode => "decode",
                    FailureClass::Container => "container",
                    FailureClass::Stalled => "stalled",
                    FailureClass::Unknown => "unknown",
                }
            };
            if manager.stop_session(id).await.is_err() {
                status.reason = "cleanup_failed";
            }
        }
    } else {
        budget.exhausted = true;
    }
    metadata["recovery_budget"] =
        serde_json::to_value(&budget).map_err(|_| PlaybackError::FfmpegCrashed)?;
    metadata["recovery"] =
        serde_json::to_value(&status).map_err(|_| PlaybackError::FfmpegCrashed)?;
    emit_play_event(connection, session_id, actor.user_id, "transcode_change", None, json!({"cause": "recovery", "status": status.status, "reason": status.reason, "attempts": status.attempts, "skipped_ms": status.skipped_ms, "quality_reduced": status.quality_reduced})).await?;
    Ok((pending, Some(status)))
}
