// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domains::playback::error::PlaybackError;
use crate::domains::playback::types::*;

pub async fn start_playback(
    _user_id: Uuid,
    _media_item_id: Uuid,
    _media_file_id: Option<Uuid>,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn heartbeat(
    _session_id: Uuid,
    _position_ms: Option<i32>,
    _state: Option<&str>,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn stop_playback(_session_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn seek(_session_id: Uuid, _position_ms: i32) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn get_playback_info(_session_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn get_user_item_data(
    _user_id: Uuid,
    _media_item_id: Uuid,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn list_bookmarks(
    _user_id: Uuid,
    _media_item_id: Uuid,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn create_bookmark(
    _user_id: Uuid,
    _media_item_id: Uuid,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn delete_bookmark(
    _user_id: Uuid,
    _bookmark_id: Uuid,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn list_playlists(_user_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn get_playlist(_user_id: Uuid, _playlist_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn create_playlist(_user_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn update_playlist(_user_id: Uuid, _playlist_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn delete_playlist(_user_id: Uuid, _playlist_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn list_playlist_items(
    _user_id: Uuid,
    _playlist_id: Uuid,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn add_playlist_item(_user_id: Uuid, _playlist_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn remove_playlist_item(
    _user_id: Uuid,
    _playlist_id: Uuid,
    _item_id: Uuid,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn stream_file(
    _user_id: Uuid,
    _media_file_id: Uuid,
    _range_header: Option<String>,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn get_transcode_manifest(_session_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn get_transcode_segment(_session_id: Uuid, _rendition: &str, _segment: &str) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn list_streaming_policies(
    pool: &PgPool,
) -> Result<StreamingPolicyListResponse, PlaybackError> {
    let count_row = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM streaming_policies"
    )
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query(
        "SELECT id, created_at, updated_at, name, description, \
         max_streams, max_transcode_streams, bandwidth_limit_bps, \
         allow_direct_play, allow_direct_stream, allow_transcode, \
         max_transcode_resolution, allow_transcode_4k, require_direct_play_4k, \
         allowed_ip_ranges, blocked_ip_ranges, \
         auto_terminate_paused_minutes, is_default, is_system, metadata \
         FROM streaming_policies \
         ORDER BY is_system DESC, name ASC"
    )
    .fetch_all(pool)
    .await?;

    let items: Vec<StreamingPolicyResponse> = rows
        .iter()
        .map(row_to_policy_response)
        .collect();

    Ok(StreamingPolicyListResponse {
        total: count_row,
        items,
    })
}

pub async fn get_streaming_policy(
    pool: &PgPool,
    policy_id: Uuid,
) -> Result<StreamingPolicyResponse, PlaybackError> {
    let row = sqlx::query(
        "SELECT id, created_at, updated_at, name, description, \
         max_streams, max_transcode_streams, bandwidth_limit_bps, \
         allow_direct_play, allow_direct_stream, allow_transcode, \
         max_transcode_resolution, allow_transcode_4k, require_direct_play_4k, \
         allowed_ip_ranges, blocked_ip_ranges, \
         auto_terminate_paused_minutes, is_default, is_system, metadata \
         FROM streaming_policies WHERE id = $1"
    )
    .bind(policy_id)
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or(PlaybackError::PolicyNotFound)?;
    Ok(row_to_policy_response(&row))
}

pub async fn create_streaming_policy(
    pool: &PgPool,
    req: &CreateStreamingPolicyRequest,
) -> Result<StreamingPolicyResponse, PlaybackError> {
    validate_resolution(req.max_transcode_resolution.as_deref())?;
    validate_ip_ranges(req.allowed_ip_ranges.as_deref())?;
    validate_ip_ranges(req.blocked_ip_ranges.as_deref())?;

    let existing = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM streaming_policies WHERE name = $1"
    )
    .bind(&req.name)
    .fetch_one(pool)
    .await?;

    if existing > 0 {
        return Err(PlaybackError::PolicyNameExists(req.name.clone()));
    }

    let allowed_ip_json = ip_ranges_to_jsonb(req.allowed_ip_ranges.as_deref());
    let blocked_ip_json = ip_ranges_to_jsonb(req.blocked_ip_ranges.as_deref());

    let mut tx = pool.begin().await?;

    if req.is_default.unwrap_or(false) {
        sqlx::query("UPDATE streaming_policies SET is_default = false WHERE is_default = true")
            .execute(&mut *tx)
            .await?;
    }

    let row = sqlx::query(
        "INSERT INTO streaming_policies \
         (name, description, max_streams, max_transcode_streams, bandwidth_limit_bps, \
          allow_direct_play, allow_direct_stream, allow_transcode, \
          max_transcode_resolution, allow_transcode_4k, require_direct_play_4k, \
          allowed_ip_ranges, blocked_ip_ranges, \
          auto_terminate_paused_minutes, is_default, is_system, metadata) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, false, '{}') \
         RETURNING id, created_at, updated_at, name, description, \
         max_streams, max_transcode_streams, bandwidth_limit_bps, \
         allow_direct_play, allow_direct_stream, allow_transcode, \
         max_transcode_resolution, allow_transcode_4k, require_direct_play_4k, \
         allowed_ip_ranges, blocked_ip_ranges, \
         auto_terminate_paused_minutes, is_default, is_system, metadata"
    )
    .bind(&req.name)
    .bind(&req.description)
    .bind(req.max_streams)
    .bind(req.max_transcode_streams)
    .bind(req.bandwidth_limit_bps)
    .bind(req.allow_direct_play.unwrap_or(true))
    .bind(req.allow_direct_stream.unwrap_or(true))
    .bind(req.allow_transcode.unwrap_or(true))
    .bind(&req.max_transcode_resolution)
    .bind(req.allow_transcode_4k.unwrap_or(true))
    .bind(req.require_direct_play_4k.unwrap_or(false))
    .bind(&allowed_ip_json)
    .bind(&blocked_ip_json)
    .bind(req.auto_terminate_paused_minutes)
    .bind(req.is_default.unwrap_or(false))
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(row_to_policy_response(&row))
}

pub async fn update_streaming_policy(
    pool: &PgPool,
    policy_id: Uuid,
    req: &UpdateStreamingPolicyRequest,
) -> Result<StreamingPolicyResponse, PlaybackError> {
    validate_resolution(req.max_transcode_resolution.as_deref())?;
    validate_ip_ranges(req.allowed_ip_ranges.as_deref())?;
    validate_ip_ranges(req.blocked_ip_ranges.as_deref())?;

    let _existing = sqlx::query(
        "SELECT id, is_system, is_default FROM streaming_policies WHERE id = $1"
    )
    .bind(policy_id)
    .fetch_optional(pool)
    .await?
    .ok_or(PlaybackError::PolicyNotFound)?;

    if let Some(ref name) = req.name {
        let name_conflict = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM streaming_policies WHERE name = $1 AND id != $2"
        )
        .bind(name)
        .bind(policy_id)
        .fetch_one(pool)
        .await?;

        if name_conflict > 0 {
            return Err(PlaybackError::PolicyNameExists(name.clone()));
        }
    }

    let allowed_ip_json = req.allowed_ip_ranges.as_ref().map(|r| ip_ranges_to_jsonb(Some(r)));
    let blocked_ip_json = req.blocked_ip_ranges.as_ref().map(|r| ip_ranges_to_jsonb(Some(r)));

    let mut tx = pool.begin().await?;

    if req.is_default.unwrap_or(false) {
        sqlx::query("UPDATE streaming_policies SET is_default = false WHERE is_default = true AND id != $1")
            .bind(policy_id)
            .execute(&mut *tx)
            .await?;
    }

    let row = sqlx::query(
        "UPDATE streaming_policies SET \
         name = COALESCE($2, name), \
         description = COALESCE($3, description), \
         max_streams = COALESCE($4, max_streams), \
         max_transcode_streams = COALESCE($5, max_transcode_streams), \
         bandwidth_limit_bps = COALESCE($6, bandwidth_limit_bps), \
         allow_direct_play = COALESCE($7, allow_direct_play), \
         allow_direct_stream = COALESCE($8, allow_direct_stream), \
         allow_transcode = COALESCE($9, allow_transcode), \
         max_transcode_resolution = COALESCE($10, max_transcode_resolution), \
         allow_transcode_4k = COALESCE($11, allow_transcode_4k), \
         require_direct_play_4k = COALESCE($12, require_direct_play_4k), \
         allowed_ip_ranges = COALESCE($13, allowed_ip_ranges), \
         blocked_ip_ranges = COALESCE($14, blocked_ip_ranges), \
         auto_terminate_paused_minutes = COALESCE($15, auto_terminate_paused_minutes), \
         is_default = COALESCE($16, is_default), \
         updated_at = now() \
         WHERE id = $1 \
         RETURNING id, created_at, updated_at, name, description, \
         max_streams, max_transcode_streams, bandwidth_limit_bps, \
         allow_direct_play, allow_direct_stream, allow_transcode, \
         max_transcode_resolution, allow_transcode_4k, require_direct_play_4k, \
         allowed_ip_ranges, blocked_ip_ranges, \
         auto_terminate_paused_minutes, is_default, is_system, metadata"
    )
    .bind(policy_id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(req.max_streams)
    .bind(req.max_transcode_streams)
    .bind(req.bandwidth_limit_bps)
    .bind(req.allow_direct_play)
    .bind(req.allow_direct_stream)
    .bind(req.allow_transcode)
    .bind(&req.max_transcode_resolution)
    .bind(req.allow_transcode_4k)
    .bind(req.require_direct_play_4k)
    .bind(&allowed_ip_json)
    .bind(&blocked_ip_json)
    .bind(req.auto_terminate_paused_minutes)
    .bind(req.is_default)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(row_to_policy_response(&row))
}

pub async fn delete_streaming_policy(
    pool: &PgPool,
    policy_id: Uuid,
) -> Result<(), PlaybackError> {
    let row = sqlx::query(
        "SELECT is_system, is_default FROM streaming_policies WHERE id = $1"
    )
    .bind(policy_id)
    .fetch_optional(pool)
    .await?
    .ok_or(PlaybackError::PolicyNotFound)?;

    let is_system: bool = row.try_get("is_system").unwrap_or(false);
    let is_default: bool = row.try_get("is_default").unwrap_or(false);

    if is_system {
        return Err(PlaybackError::SystemPolicyCannotBeDeleted);
    }

    if is_default {
        let other_default = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM streaming_policies WHERE is_default = true AND id != $1"
        )
        .bind(policy_id)
        .fetch_one(pool)
        .await?;

        if other_default == 0 {
            return Err(PlaybackError::CannotRemoveDefaultPolicy);
        }
    }

    sqlx::query("UPDATE users SET streaming_policy_id = NULL WHERE streaming_policy_id = $1")
        .bind(policy_id)
        .execute(pool)
        .await?;

    let result = sqlx::query("DELETE FROM streaming_policies WHERE id = $1")
        .bind(policy_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(PlaybackError::PolicyNotFound);
    }

    Ok(())
}

pub async fn resolve_streaming_limits(
    pool: &PgPool,
    _user_id: Uuid,
    user_max_streams: Option<i32>,
    user_max_transcode_streams: Option<i32>,
    user_bandwidth_limit_bps: Option<i64>,
    user_streaming_policy_id: Option<Uuid>,
) -> Result<ResolvedStreamingLimitsResponse, PlaybackError> {
    let policy_row = if let Some(policy_id) = user_streaming_policy_id {
        sqlx::query(
            "SELECT id, name, max_streams, max_transcode_streams, bandwidth_limit_bps, \
             allow_direct_play, allow_direct_stream, allow_transcode, \
             max_transcode_resolution, allow_transcode_4k, require_direct_play_4k, \
             allowed_ip_ranges, blocked_ip_ranges, \
             auto_terminate_paused_minutes \
             FROM streaming_policies WHERE id = $1"
        )
        .bind(policy_id)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT id, name, max_streams, max_transcode_streams, bandwidth_limit_bps, \
             allow_direct_play, allow_direct_stream, allow_transcode, \
             max_transcode_resolution, allow_transcode_4k, require_direct_play_4k, \
             allowed_ip_ranges, blocked_ip_ranges, \
             auto_terminate_paused_minutes \
             FROM streaming_policies WHERE is_default = true LIMIT 1"
        )
        .fetch_optional(pool)
        .await?
    };

    let (policy_id, policy_name, p_max_streams, p_max_transcode, p_bandwidth,
         p_allow_dp, p_allow_ds, p_allow_transcode,
         p_max_res, p_allow_4k, p_require_dp_4k,
         p_allowed_ips, p_blocked_ips, p_auto_terminate) = if let Some(ref r) = policy_row {
        (
            Some(r.try_get::<Uuid, _>("id").unwrap_or_default()),
            Some(r.try_get::<String, _>("name").unwrap_or_default()),
            r.try_get::<Option<i32>, _>("max_streams").ok().flatten(),
            r.try_get::<Option<i32>, _>("max_transcode_streams").ok().flatten(),
            r.try_get::<Option<i64>, _>("bandwidth_limit_bps").ok().flatten(),
            r.try_get::<bool, _>("allow_direct_play").unwrap_or(true),
            r.try_get::<bool, _>("allow_direct_stream").unwrap_or(true),
            r.try_get::<bool, _>("allow_transcode").unwrap_or(true),
            r.try_get::<Option<String>, _>("max_transcode_resolution").ok().flatten(),
            r.try_get::<bool, _>("allow_transcode_4k").unwrap_or(true),
            r.try_get::<bool, _>("require_direct_play_4k").unwrap_or(false),
            jsonb_to_string_vec(r.try_get::<serde_json::Value, _>("allowed_ip_ranges").unwrap_or(serde_json::json!([]))),
            jsonb_to_string_vec(r.try_get::<serde_json::Value, _>("blocked_ip_ranges").unwrap_or(serde_json::json!([]))),
            r.try_get::<Option<i32>, _>("auto_terminate_paused_minutes").ok().flatten(),
        )
    } else {
        (None, None, None, None, None, true, true, true, None, true, false, vec![], vec![], None)
    };

    Ok(ResolvedStreamingLimitsResponse {
        policy_id,
        policy_name,
        max_streams: user_max_streams.or(p_max_streams),
        max_transcode_streams: user_max_transcode_streams.or(p_max_transcode),
        bandwidth_limit_bps: user_bandwidth_limit_bps.or(p_bandwidth),
        allow_direct_play: p_allow_dp,
        allow_direct_stream: p_allow_ds,
        allow_transcode: p_allow_transcode,
        max_transcode_resolution: p_max_res,
        allow_transcode_4k: p_allow_4k,
        require_direct_play_4k: p_require_dp_4k,
        allowed_ip_ranges: p_allowed_ips,
        blocked_ip_ranges: p_blocked_ips,
        auto_terminate_paused_minutes: p_auto_terminate,
    })
}

fn row_to_policy_response(row: &sqlx::postgres::PgRow) -> StreamingPolicyResponse {
    use sqlx::Row;
    StreamingPolicyResponse {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        max_streams: row.get("max_streams"),
        max_transcode_streams: row.get("max_transcode_streams"),
        bandwidth_limit_bps: row.get("bandwidth_limit_bps"),
        allow_direct_play: row.get("allow_direct_play"),
        allow_direct_stream: row.get("allow_direct_stream"),
        allow_transcode: row.get("allow_transcode"),
        max_transcode_resolution: row.get("max_transcode_resolution"),
        allow_transcode_4k: row.get("allow_transcode_4k"),
        require_direct_play_4k: row.get("require_direct_play_4k"),
        allowed_ip_ranges: jsonb_to_string_vec(row.get("allowed_ip_ranges")),
        blocked_ip_ranges: jsonb_to_string_vec(row.get("blocked_ip_ranges")),
        auto_terminate_paused_minutes: row.get("auto_terminate_paused_minutes"),
        is_default: row.get("is_default"),
        is_system: row.get("is_system"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn jsonb_to_string_vec(val: serde_json::Value) -> Vec<String> {
    val.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn ip_ranges_to_jsonb(ranges: Option<&[String]>) -> serde_json::Value {
    match ranges {
        Some(r) => serde_json::Value::Array(
            r.iter().map(|s| serde_json::Value::String(s.clone())).collect()
        ),
        None => serde_json::Value::Array(vec![]),
    }
}

fn validate_resolution(resolution: Option<&str>) -> Result<(), PlaybackError> {
    if let Some(res) = resolution
        && !VALID_TRANSCODE_RESOLUTIONS.contains(&res)
    {
        return Err(PlaybackError::InvalidResolution(res.to_string()));
    }
    Ok(())
}

fn validate_ip_ranges(ranges: Option<&[String]>) -> Result<(), PlaybackError> {
    if let Some(ranges) = ranges {
        for range in ranges {
            if !range.contains('/') {
                return Err(PlaybackError::InvalidIpRange(format!(
                    "{}: missing CIDR prefix length",
                    range
                )));
            }
        }
    }
    Ok(())
}
