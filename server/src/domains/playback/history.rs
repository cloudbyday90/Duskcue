// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use sqlx::{Executor, Postgres, Row};
use uuid::Uuid;

use super::PlaybackError;

pub(super) async fn played_file<'e, E>(
    executor: E,
    media_item_id: Uuid,
    file_id: Option<Uuid>,
) -> Result<Option<(Uuid, Option<i32>)>, PlaybackError>
where
    E: Executor<'e, Database = Postgres>,
{
    let Some(file_id) = file_id else {
        return Ok(None);
    };
    let row = sqlx::query(
        "SELECT id,runtime_seconds FROM media_files WHERE id=$1 AND media_item_id=$2 FOR KEY SHARE",
    )
    .bind(file_id)
    .bind(media_item_id)
    .fetch_optional(executor)
    .await?;
    row.map(|row| Ok((row.try_get("id")?, row.try_get("runtime_seconds")?)))
        .transpose()
}

pub(super) async fn emit_play_event<'e, E>(
    executor: E,
    session_id: Uuid,
    user_id: Uuid,
    event_type: &str,
    position_seconds: Option<i32>,
    details: serde_json::Value,
) -> Result<(), PlaybackError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        "INSERT INTO play_events (play_session_id, user_id, event_type, position_seconds, details) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(session_id)
    .bind(user_id)
    .bind(event_type)
    .bind(position_seconds)
    .bind(&details)
    .execute(executor)
    .await?;
    Ok(())
}

pub(super) async fn merge_session_metadata<'e, E>(
    executor: E,
    session_id: Uuid,
    merge: serde_json::Value,
) -> Result<(), PlaybackError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        "UPDATE play_sessions SET metadata = metadata || $2, updated_at = now() WHERE id = $1",
    )
    .bind(session_id)
    .bind(&merge)
    .execute(executor)
    .await?;
    Ok(())
}

pub(super) async fn upsert_user_item_data_heartbeat<'e, E>(
    executor: E,
    user_id: Uuid,
    profile_id: Uuid,
    media_item_id: Uuid,
    position_ms: i32,
    media_file_id: Option<Uuid>,
) -> Result<(), PlaybackError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        "INSERT INTO user_item_data (id, user_id, profile_id, media_item_id, resume_position_ms, last_played_media_file_id) \
         VALUES (uuidv7(), $1, $2, $3, $4, $5) \
         ON CONFLICT (profile_id, media_item_id) \
         DO UPDATE SET resume_position_ms = $4, \
                       last_played_media_file_id = COALESCE($5, user_item_data.last_played_media_file_id), \
                       updated_at = now()"
    )
    .bind(user_id)
    .bind(profile_id)
    .bind(media_item_id)
    .bind(position_ms)
    .bind(media_file_id)
    .execute(executor)
    .await?;
    Ok(())
}

pub(super) async fn upsert_user_item_data_stop<'e, E>(
    executor: E,
    user_id: Uuid,
    profile_id: Uuid,
    media_item_id: Uuid,
    is_watched: bool,
    resume_position_ms: i32,
    media_file_id: Option<Uuid>,
) -> Result<i32, PlaybackError>
where
    E: Executor<'e, Database = Postgres>,
{
    let row = sqlx::query(
        "INSERT INTO user_item_data (id, user_id, profile_id, media_item_id, is_watched, play_count, last_played_at, resume_position_ms, last_played_media_file_id) \
         VALUES (uuidv7(), $1, $2, $3, $4, 1, now(), $5, $6) \
         ON CONFLICT (profile_id, media_item_id) \
         DO UPDATE SET play_count = user_item_data.play_count + 1, \
                       last_played_at = now(), \
                       is_watched = user_item_data.is_watched OR $4, \
                       resume_position_ms = $5, \
                       last_played_media_file_id = COALESCE($6, user_item_data.last_played_media_file_id), \
                       updated_at = now() \
         RETURNING play_count"
    )
    .bind(user_id)
    .bind(profile_id)
    .bind(media_item_id)
    .bind(is_watched)
    .bind(resume_position_ms)
    .bind(media_file_id)
    .fetch_one(executor)
    .await?;

    let play_count: i32 = row.try_get("play_count").unwrap_or(1);
    Ok(play_count)
}
