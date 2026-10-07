use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::PlaybackError;
use super::history::{emit_play_event, played_file, upsert_user_item_data_stop};
use super::types::StopPlaybackResponse;
use crate::services::transcoding::TranscodeManager;

const RESULT_KEY: &str = "duskcue_stop_result_v1";

#[derive(Deserialize, Serialize)]
struct StopSnapshot {
    duration_seconds: i32,
    percent_complete: Option<f32>,
    is_watched: bool,
    play_count: i32,
}

impl StopSnapshot {
    fn response(
        self,
        session_id: Uuid,
        media_item_id: Uuid,
        playback_mode: String,
    ) -> StopPlaybackResponse {
        StopPlaybackResponse {
            session_id,
            media_item_id,
            playback_mode,
            duration_seconds: self.duration_seconds,
            percent_complete: self.percent_complete,
            is_watched: self.is_watched,
            play_count: self.play_count,
        }
    }
}

fn transcode_id(metadata: &Value) -> Option<Uuid> {
    metadata
        .get("transcode_session_id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
}

pub(super) async fn stop_playback(
    pool: &PgPool,
    manager: &TranscodeManager,
    user_id: Uuid,
    session_id: Uuid,
    final_position_ms: Option<i32>,
    cancelled_before_start: bool,
) -> Result<StopPlaybackResponse, PlaybackError> {
    if cancelled_before_start {
        return cancel_before_start(pool, manager, user_id, session_id).await;
    }
    let mut transaction = pool.begin().await?;
    let row = sqlx::query(
        "SELECT profile_id, playback_mode, media_item_id, started_at, stopped_at, duration_seconds, percent_complete, metadata \
         FROM play_sessions WHERE id=$1 AND user_id=$2 \
           AND metadata -> 'cancelled_before_start' IS DISTINCT FROM 'true'::jsonb FOR UPDATE",
    ).bind(session_id).bind(user_id).fetch_optional(&mut *transaction).await?
        .ok_or(PlaybackError::SessionNotFound)?;
    let profile_id: Option<Uuid> = row.try_get("profile_id")?;
    let media_item_id: Uuid = row.try_get("media_item_id")?;
    let playback_mode: String = row.try_get("playback_mode")?;
    let metadata: Value = row.try_get("metadata")?;
    let transcode = transcode_id(&metadata);
    let stopped_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("stopped_at")?;
    if stopped_at.is_some() {
        let snapshot = match metadata
            .get(RESULT_KEY)
            .cloned()
            .and_then(|value| serde_json::from_value::<StopSnapshot>(value).ok())
        {
            Some(snapshot) => snapshot,
            None => {
                let percent_complete: Option<f32> = row.try_get("percent_complete")?;
                let play_count = if playback_mode == "interactive"
                    && let Some(profile_id) = profile_id
                {
                    sqlx::query_scalar::<_, i32>("SELECT play_count FROM user_item_data WHERE user_id=$1 AND profile_id=$2 AND media_item_id=$3")
                        .bind(user_id).bind(profile_id).bind(media_item_id).fetch_optional(&mut *transaction).await?.unwrap_or(0)
                } else {
                    0
                };
                StopSnapshot {
                    duration_seconds: row.try_get("duration_seconds")?,
                    percent_complete,
                    is_watched: playback_mode == "interactive"
                        && profile_id.is_some()
                        && percent_complete.is_some_and(|value| value >= 90.0),
                    play_count,
                }
            }
        };
        transaction.commit().await?;
        if let Some(transcode) = transcode {
            manager.stop_session(transcode).await?;
        }
        return Ok(snapshot.response(session_id, media_item_id, playback_mode));
    }
    let stored_file_id = metadata
        .get("media_file_id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    let stored_position = metadata
        .get("current_position_ms")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(0);
    let final_position = final_position_ms.unwrap_or(stored_position);
    let file = played_file(&mut *transaction, media_item_id, stored_file_id).await?;
    let media_file_id = file.map(|(id, _)| id);
    let runtime_seconds = file.and_then(|(_, runtime)| runtime);
    let percent_complete = runtime_seconds
        .filter(|runtime| *runtime > 0)
        .map(|runtime| {
            (100.0 * f64::from(final_position) / (f64::from(runtime) * 1000.0)).min(100.0) as f32
        });
    let is_watched = percent_complete.is_some_and(|percent| percent >= 90.0);
    let started_at: chrono::DateTime<chrono::Utc> = row.try_get("started_at")?;
    let duration_seconds =
        i32::try_from((chrono::Utc::now() - started_at).num_seconds().max(0)).unwrap_or(i32::MAX);
    let play_count = if playback_mode == "interactive"
        && let Some(profile_id) = profile_id
    {
        upsert_user_item_data_stop(
            &mut *transaction,
            user_id,
            profile_id,
            media_item_id,
            is_watched,
            if is_watched { 0 } else { final_position },
            media_file_id,
        )
        .await?
    } else {
        0
    };
    let snapshot = StopSnapshot {
        duration_seconds,
        percent_complete,
        is_watched: playback_mode == "interactive" && profile_id.is_some() && is_watched,
        play_count,
    };
    let merge = json!({ "current_state": "stopped", "current_position_ms": final_position, (RESULT_KEY): snapshot });
    sqlx::query("UPDATE play_sessions SET stopped_at=now(),duration_seconds=$3,percent_complete=$4,metadata=metadata || $5,updated_at=now() WHERE id=$1 AND user_id=$2")
        .bind(session_id).bind(user_id).bind(duration_seconds).bind(percent_complete).bind(merge)
        .execute(&mut *transaction).await?;
    emit_play_event(
        &mut *transaction,
        session_id,
        user_id,
        "stop",
        Some(final_position / 1000),
        json!({"duration_seconds": duration_seconds, "percent_complete": percent_complete}),
    )
    .await?;
    transaction.commit().await?;
    if let Some(transcode) = transcode {
        manager.stop_session(transcode).await?;
    }
    Ok(snapshot.response(session_id, media_item_id, playback_mode))
}

async fn cancel_before_start(
    pool: &PgPool,
    manager: &TranscodeManager,
    user_id: Uuid,
    session_id: Uuid,
) -> Result<StopPlaybackResponse, PlaybackError> {
    let row = sqlx::query(
        "UPDATE play_sessions SET stopped_at=COALESCE(stopped_at,now()),duration_seconds=0,percent_complete=NULL, \
         metadata=metadata || '{\"current_state\":\"stopped\",\"cancelled_before_start\":true}'::jsonb,updated_at=now() \
         WHERE id=$1 AND user_id=$2 AND \
           ((stopped_at IS NULL AND NOT (metadata ? 'last_heartbeat_at') \
             AND COALESCE(metadata ->> 'current_position_ms','0')='0' \
             AND NOT EXISTS (SELECT 1 FROM play_events e WHERE e.play_session_id=play_sessions.id \
               AND e.event_type IN ('heartbeat','seek','pause','resume','buffer_start','buffer_end'))) \
            OR metadata -> 'cancelled_before_start'='true'::jsonb) \
         RETURNING media_item_id,playback_mode,metadata",
    ).bind(session_id).bind(user_id).fetch_optional(pool).await?.ok_or(PlaybackError::SessionNotFound)?;
    let metadata: Value = row.try_get("metadata")?;
    if let Some(transcode) = transcode_id(&metadata) {
        manager.stop_session(transcode).await?;
    }
    Ok(StopPlaybackResponse {
        session_id,
        media_item_id: row.try_get("media_item_id")?,
        playback_mode: row.try_get("playback_mode")?,
        duration_seconds: 0,
        percent_complete: None,
        is_watched: false,
        play_count: 0,
    })
}
