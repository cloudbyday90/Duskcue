// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::PlaybackError;
use super::history::{
    emit_play_event, merge_session_metadata, played_file, upsert_user_item_data_heartbeat,
};
use super::recovery::{self, RecoveryContext};
use super::types::{HeartbeatRequest, HeartbeatResponse, PlaybackActor, VALID_PLAYBACK_STATES};
use crate::services::transcoding::TranscodeManager;

pub(super) async fn heartbeat(
    pool: &PgPool,
    manager: &TranscodeManager,
    actor: PlaybackActor,
    request: &HeartbeatRequest,
    data_dir: &std::path::Path,
) -> Result<HeartbeatResponse, PlaybackError> {
    let user_id = actor.user_id;
    let session_id = request.session_id.ok_or(PlaybackError::SessionNotFound)?;
    let position_ms = request.position_ms;
    let state = request
        .state
        .as_deref()
        .filter(|state| VALID_PLAYBACK_STATES.contains(state));
    let is_paused = request.is_paused;
    let is_buffering = request.is_buffering;
    let mut transaction = pool.begin().await?;
    let row = sqlx::query(
        "SELECT id, user_id, profile_id, playback_mode, stream_decision, media_item_id, metadata \
         FROM play_sessions \
         WHERE id = $1 AND user_id = $2 AND stopped_at IS NULL FOR UPDATE",
    )
    .bind(session_id)
    .bind(user_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(PlaybackError::SessionNotFound)?;

    let session_user_id: Uuid = row
        .try_get("user_id")
        .map_err(|_| PlaybackError::SessionNotFound)?;
    if session_user_id != user_id {
        return Err(PlaybackError::SessionNotFound);
    }

    let profile_id: Uuid = row
        .try_get("profile_id")
        .map_err(|_| PlaybackError::SessionNotFound)?;
    let playback_mode: String = row
        .try_get("playback_mode")
        .unwrap_or_else(|_| "interactive".to_string());
    let media_item_id: Uuid = row.try_get("media_item_id").unwrap_or_default();
    let mut metadata: serde_json::Value = row.try_get("metadata").unwrap_or(serde_json::json!({}));
    metadata["stream_decision"] = serde_json::json!(
        row.try_get::<String, _>("stream_decision")
            .unwrap_or_else(|_| "direct_play".into())
    );

    let active_transcode = recovery::transcode_id(&metadata);
    if (active_transcode.is_some() || super::direct::has_context(&metadata))
        && profile_id != actor.profile_id
    {
        return Err(PlaybackError::SessionNotFound);
    }
    if active_transcode.is_none()
        && super::direct::has_context(&metadata)
        && !super::direct::current_revision(&metadata, request.direct_source_revision)
    {
        let response = recovery::response(manager, session_id, playback_mode, &metadata, None);
        transaction.commit().await?;
        return Ok(response);
    }
    if (active_transcode.is_some() || request.transcode_session_id.is_some())
        && request.transcode_session_id != active_transcode
    {
        let response = recovery::response(manager, session_id, playback_mode, &metadata, None);
        transaction.commit().await?;
        return Ok(response);
    }
    if request.recovery_requested
        && request
            .copy_fallback
            .as_ref()
            .is_some_and(|acknowledgement| Some(acknowledgement.generation) != active_transcode)
    {
        let response = recovery::response(manager, session_id, playback_mode, &metadata, None);
        transaction.commit().await?;
        return Ok(response);
    }

    let prev_state = metadata
        .get("current_state")
        .and_then(|s| s.as_str())
        .unwrap_or("playing")
        .to_string();

    let stored_file_id = metadata
        .get("media_file_id")
        .and_then(|f| f.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());
    let media_file_id = played_file(&mut *transaction, media_item_id, stored_file_id)
        .await?
        .map(|(id, _)| id);

    let effective_state = if request.recovery_requested && is_paused == Some(true) {
        "paused".to_string()
    } else if is_buffering.unwrap_or(false) {
        "buffering".to_string()
    } else if let Some(s) = state {
        s.to_string()
    } else if is_paused.unwrap_or(false) {
        "paused".to_string()
    } else {
        "playing".to_string()
    };

    let mut effective_position = position_ms.unwrap_or_else(|| {
        metadata
            .get("current_position_ms")
            .and_then(|p| p.as_i64())
            .map(|p| p as i32)
            .unwrap_or(0)
    });

    metadata["current_position_ms"] = serde_json::json!(effective_position);
    let (pending, recovery_status) = recovery::recover(
        &mut transaction,
        RecoveryContext {
            manager,
            actor,
            session_id,
            media_item_id,
            request,
            data_dir,
        },
        &mut metadata,
    )
    .await?;
    effective_position = metadata
        .get("current_position_ms")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(i64::from(effective_position))
        .clamp(0, i64::from(i32::MAX)) as i32;

    if effective_state != prev_state {
        let (event_type, details) = match (prev_state.as_str(), effective_state.as_str()) {
            ("playing", "paused") => ("pause", serde_json::json!({"reason": "user_paused"})),
            ("paused", "playing") => ("resume", serde_json::json!({})),
            ("playing", "buffering") => ("buffer_start", serde_json::json!({})),
            ("buffering", "playing") => ("buffer_end", serde_json::json!({})),
            ("paused", "buffering") => ("buffer_start", serde_json::json!({"from": "paused"})),
            ("buffering", "paused") => ("pause", serde_json::json!({"from": "buffering"})),
            _ => ("heartbeat", serde_json::json!({})),
        };
        emit_play_event(
            &mut *transaction,
            session_id,
            user_id,
            event_type,
            Some(effective_position / 1000),
            details,
        )
        .await?;
    }

    metadata["current_state"] = serde_json::json!(effective_state);
    metadata["current_position_ms"] = serde_json::json!(effective_position);
    metadata["last_heartbeat_at"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
    merge_session_metadata(&mut *transaction, session_id, metadata.clone()).await?;

    if position_ms.is_some() && playback_mode == "interactive" {
        upsert_user_item_data_heartbeat(
            &mut *transaction,
            user_id,
            profile_id,
            media_item_id,
            effective_position,
            media_file_id,
        )
        .await?;
    }

    emit_play_event(
        &mut *transaction,
        session_id,
        user_id,
        "heartbeat",
        Some(effective_position / 1000),
        serde_json::json!({"state": effective_state}),
    )
    .await?;

    transaction.commit().await?;
    if let Some(pending) = pending {
        pending.acknowledge();
    }
    Ok(recovery::response(
        manager,
        session_id,
        playback_mode,
        &metadata,
        recovery_status,
    ))
}
