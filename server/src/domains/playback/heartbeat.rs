// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::PlaybackError;
use super::history::{
    emit_play_event, merge_session_metadata, played_file, upsert_user_item_data_heartbeat,
};
use super::types::HeartbeatResponse;

pub(super) async fn heartbeat(
    pool: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
    position_ms: Option<i32>,
    state: Option<&str>,
    is_paused: Option<bool>,
    is_buffering: Option<bool>,
) -> Result<HeartbeatResponse, PlaybackError> {
    let mut transaction = pool.begin().await?;
    let row = sqlx::query(
        "SELECT id, user_id, profile_id, playback_mode, media_item_id, metadata \
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
    let metadata: serde_json::Value = row.try_get("metadata").unwrap_or(serde_json::json!({}));

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

    let effective_state = if let Some(s) = state {
        s.to_string()
    } else if is_buffering.unwrap_or(false) {
        "buffering".to_string()
    } else if is_paused.unwrap_or(false) {
        "paused".to_string()
    } else {
        "playing".to_string()
    };

    let effective_position = position_ms.unwrap_or_else(|| {
        metadata
            .get("current_position_ms")
            .and_then(|p| p.as_i64())
            .map(|p| p as i32)
            .unwrap_or(0)
    });

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

    let merge = serde_json::json!({
        "current_state": effective_state,
        "current_position_ms": effective_position,
        "last_heartbeat_at": chrono::Utc::now().to_rfc3339()
    });

    merge_session_metadata(&mut *transaction, session_id, merge).await?;

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
    Ok(HeartbeatResponse {
        session_id,
        position_ms: effective_position,
        playback_mode,
    })
}
