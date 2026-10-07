use std::path::Path;

use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::PlaybackError;
use super::history::{
    emit_play_event, merge_session_metadata, played_file, upsert_user_item_data_heartbeat,
};
use super::types::SeekResponse;
use crate::services::transcoding::TranscodeManager;

pub(super) async fn seek(
    pool: &PgPool,
    transcode_manager: &TranscodeManager,
    user_id: Uuid,
    session_id: Uuid,
    position_ms: i32,
    data_dir: &Path,
) -> Result<SeekResponse, PlaybackError> {
    if position_ms < 0 {
        return Err(PlaybackError::InvalidSeekPosition(format!(
            "position must be >= 0, got {position_ms}"
        )));
    }

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

    let transcode_session_id = metadata
        .get("transcode_session_id")
        .and_then(|t| t.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    let mut pending = None;
    let (new_stream_url, new_transcode_session_id) = if let Some(ts_id) = transcode_session_id {
        let new_session = transcode_manager
            .seek_session(ts_id, position_ms as i64, data_dir)
            .await?;

        let new_id = new_session.id;
        pending = Some(transcode_manager.pending_session(new_id));
        let url = format!("/api/v1/transcode/{}/manifest.m3u8", new_id);

        let merge = serde_json::json!({
            "transcode_session_id": new_id,
            "current_position_ms": position_ms,
            "current_state": "playing"
        });
        merge_session_metadata(&mut *transaction, session_id, merge).await?;

        (Some(url), Some(new_id))
    } else {
        let merge = serde_json::json!({
            "current_position_ms": position_ms,
            "current_state": "playing"
        });
        merge_session_metadata(&mut *transaction, session_id, merge).await?;
        (None, None)
    };

    if playback_mode == "interactive" {
        let stored_file_id = metadata
            .get("media_file_id")
            .and_then(|file| file.as_str())
            .and_then(|file| Uuid::parse_str(file).ok());
        let media_file_id = played_file(&mut *transaction, media_item_id, stored_file_id)
            .await?
            .map(|(id, _)| id);
        upsert_user_item_data_heartbeat(
            &mut *transaction,
            user_id,
            profile_id,
            media_item_id,
            position_ms,
            media_file_id,
        )
        .await?;
    }

    emit_play_event(
        &mut *transaction,
        session_id,
        user_id,
        "seek",
        Some(position_ms / 1000),
        serde_json::json!({"target_position_ms": position_ms}),
    )
    .await?;

    transaction.commit().await?;
    if let Some(pending) = pending {
        pending.acknowledge();
    }
    Ok(SeekResponse {
        session_id,
        position_ms,
        stream_url: new_stream_url,
        transcode_session_id: new_transcode_session_id,
        playback_mode,
    })
}
