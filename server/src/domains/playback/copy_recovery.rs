// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use serde_json::{Value, json};
use sqlx::PgConnection;

use super::PlaybackError;
use super::history::emit_play_event;
use super::recovery::{RecoveryContext, stored_recovery};
use super::types::HeartbeatRequest;
use super::types::RecoveryResponse;
use crate::domains::profiles::service::{
    assert_media_access_on_connection, load_profile_scope_on_connection,
};
use crate::services::transcoding::recovery::{MAX_RECOVERY_ATTEMPTS, RecoveryBudget};
use crate::services::transcoding::{PendingTranscodeSession, plan::StreamAction};

pub(super) fn requested(context: &RecoveryContext<'_>, metadata: &Value) -> bool {
    let Some(id) = super::recovery::transcode_id(metadata) else {
        return false;
    };
    context.request.recovery_requested
        && context.request.copy_fallback.is_some()
        && context.manager.session_failure(id).is_none()
        && super::source_context::copy_fallback(context.manager, metadata).is_some()
}

fn acknowledged(request: &HeartbeatRequest, generation: uuid::Uuid) -> bool {
    request.recovery_requested
        && request.transcode_session_id == Some(generation)
        && request
            .copy_fallback
            .as_ref()
            .is_some_and(|acknowledgement| {
                acknowledgement.generation == generation
                    && acknowledgement.supported
                    && acknowledgement.smooth
            })
}

pub(super) async fn recover(
    connection: &mut PgConnection,
    context: RecoveryContext<'_>,
    metadata: &mut Value,
) -> Result<(Option<PendingTranscodeSession>, Option<RecoveryResponse>), PlaybackError> {
    let Some(id) = super::recovery::transcode_id(metadata) else {
        return Ok((None, None));
    };
    if !acknowledged(context.request, id) {
        return Ok((None, None));
    }
    let Some(session) = context.manager.get_session(&id) else {
        return Ok((None, None));
    };
    if session.stream_plan.video != StreamAction::Copy || session.user_id != context.actor.user_id {
        return Ok((None, None));
    }
    let Some(route) = super::source_context::read(metadata) else {
        return Ok((None, None));
    };
    let Some(candidate) = route.fallback else {
        return Ok((None, None));
    };
    let mut budget: RecoveryBudget = match metadata.get("recovery_budget") {
        Some(value) => serde_json::from_value(value.clone()).unwrap_or(RecoveryBudget {
            exhausted: true,
            ..Default::default()
        }),
        None => RecoveryBudget::default(),
    };
    budget.attempts = budget.attempts.max(session.recovery_attempts);
    if budget.exhausted {
        return Ok((None, stored_recovery(metadata)));
    }
    let scope = load_profile_scope_on_connection(
        connection,
        context.actor.user_id,
        context.actor.profile_id,
        context.actor.has_all_library_access,
    )
    .await
    .map_err(|_| PlaybackError::AccessDenied)?;
    assert_media_access_on_connection(connection, &scope, context.media_item_id)
        .await
        .map_err(|_| PlaybackError::AccessDenied)?;
    let mut status = RecoveryResponse {
        status: "exhausted",
        reason: "source_changed",
        attempts: budget.attempts,
        skipped_ms: 0,
        quality_reduced: false,
    };
    let mut pending = None;
    if budget.attempts >= MAX_RECOVERY_ATTEMPTS {
        budget.exhausted = true;
        status.reason = "budget_exhausted";
    } else if let Some(mut parameters) =
        super::direct_recovery::parameters(connection, &context, metadata, &candidate).await?
    {
        budget.attempts += 1;
        status.attempts = budget.attempts;
        parameters.recovery_attempts = budget.attempts;
        if context.manager.stop_session(id).await.is_err() {
            budget.exhausted = true;
            status.reason = "cleanup_failed";
        } else {
            match context
                .manager
                .start_session(parameters, context.data_dir)
                .await
            {
                Ok(replacement) => {
                    pending = Some(context.manager.pending_session(replacement.id));
                    status.status = "recovered";
                    status.reason = "client_decode";
                    status.quality_reduced = true;
                    metadata["transcode_session_id"] = json!(replacement.id);
                    metadata["stream_offset_ms"] = json!(replacement.source_start_ms);
                    metadata["stream_decision"] = json!("transcode");
                    sqlx::query("UPDATE play_sessions SET stream_decision='transcode' WHERE id=$1 AND user_id=$2")
                        .bind(context.session_id).bind(context.actor.user_id).execute(&mut *connection).await?;
                }
                Err(_) => {
                    budget.exhausted = true;
                    status.reason = "replacement_failed";
                }
            }
        }
    } else {
        budget.exhausted = true;
    }
    if pending.is_none() && context.manager.stop_session(id).await.is_err() {
        status.reason = "cleanup_failed";
    }
    metadata["recovery_budget"] = json!(budget);
    metadata["recovery"] = json!(status);
    emit_play_event(connection, context.session_id, context.actor.user_id, "transcode_change", None,
        json!({"cause":"recovery","status":status.status,"reason":status.reason,"attempts":status.attempts,"skipped_ms":0,"quality_reduced":status.quality_reduced})).await?;
    Ok((pending, Some(status)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn decode_fallback_acknowledges_only_the_current_generation_with_positive_smooth_support() {
        let generation = Uuid::now_v7();
        let mut request: HeartbeatRequest = serde_json::from_value(json!({
            "session_id":Uuid::now_v7(),"transcode_session_id":generation,
            "recovery_requested":true,"copy_fallback":{"generation":generation,"supported":true,"smooth":true}
        })).unwrap();
        assert!(acknowledged(&request, generation));
        assert!(!acknowledged(&request, Uuid::now_v7()));
        request.copy_fallback.as_mut().unwrap().smooth = false;
        assert!(!acknowledged(&request, generation));
        request.copy_fallback.as_mut().unwrap().smooth = true;
        request.copy_fallback.as_mut().unwrap().supported = false;
        assert!(!acknowledged(&request, generation));
        request.copy_fallback.as_mut().unwrap().supported = true;
        request.recovery_requested = false;
        assert!(!acknowledged(&request, generation));
        request.recovery_requested = true;
        request.transcode_session_id = None;
        assert!(!acknowledged(&request, generation));
    }
}
