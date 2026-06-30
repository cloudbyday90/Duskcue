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

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domains::playback::error::PlaybackError;
use crate::domains::playback::types::*;
use crate::services::decision_engine::{
    self, DecisionEngineConfig, DeviceCapabilities, MediaFileInfo, NetworkConditions,
    StreamDecision,
};
use crate::services::transcoding::{StartSessionParams, TranscodeManager, TranscodeRendition};
use crate::state::RuntimeConfig;

pub async fn start_playback(
    pool: &PgPool,
    transcode_manager: &TranscodeManager,
    user_id: Uuid,
    _user_role: &str,
    req: &StartPlaybackRequest,
    config: &RuntimeConfig,
    data_dir: &Path,
) -> Result<PlaybackStartResponse, PlaybackError> {
    let media_item_id = req.media_item_id.ok_or(PlaybackError::MediaNotFound)?;

    let item_row =
        sqlx::query("SELECT id, library_id FROM media_items WHERE id = $1 AND deleted_at IS NULL")
            .bind(media_item_id)
            .fetch_optional(pool)
            .await?
            .ok_or(PlaybackError::MediaNotFound)?;

    let library_id: Uuid = item_row.try_get("library_id").unwrap_or_default();

    let media_file_details = if let Some(file_id) = req.media_file_id {
        fetch_media_file_details(pool, file_id).await?
    } else {
        select_best_media_file(pool, media_item_id).await?
    };

    let media_info = build_media_file_info(&media_file_details);

    let device_caps = build_device_capabilities(req.device_profile.as_ref(), config);

    let network = build_network_conditions(pool, user_id, req.max_streaming_bitrate).await;

    let engine_config = build_decision_engine_config(
        config,
        req.quality_mode.as_deref(),
        req.max_streaming_bitrate,
    );

    let mut decision = decision_engine::decide(&media_info, &device_caps, &network, &engine_config);

    if req.force_transcode.unwrap_or(false) {
        decision.overall = StreamDecision::Transcode;
    }

    let stream_decision_str = match decision.overall {
        StreamDecision::DirectPlay => "direct_play",
        StreamDecision::DirectStream => "direct_stream",
        StreamDecision::Transcode => "transcode",
    };

    let stream_url;
    let transcode_session_id;

    match decision.overall {
        StreamDecision::DirectPlay => {
            stream_url = format!("/api/v1/stream/{}", media_file_details.id);
            transcode_session_id = None;
        }
        StreamDecision::DirectStream => {
            let session = transcode_manager
                .start_remux_session(
                    media_file_details.id,
                    user_id,
                    PathBuf::from(&media_file_details.file_path),
                    media_info.video_codec.clone(),
                    media_info.video_resolution,
                    media_info.audio_codec.clone(),
                    data_dir,
                )
                .await?;
            stream_url = format!("/api/v1/transcode/{}/manifest.m3u8", session.id);
            transcode_session_id = Some(session.id);
        }
        StreamDecision::Transcode => {
            let target_v = decision.target_video_codec.clone();
            let target_a = decision.target_audio_codec.clone();
            let target_res = decision.target_resolution;
            let target_bitrate = decision.target_bitrate_bps.map(|b| b as u32);

            let session = transcode_manager
                .start_session(
                    StartSessionParams {
                        media_file_id: media_file_details.id,
                        user_id,
                        source_path: PathBuf::from(&media_file_details.file_path),
                        source_video_codec: media_info.video_codec.clone(),
                        source_video_resolution: media_info.video_resolution,
                        source_audio_codec: media_info.audio_codec.clone(),
                        target_video_codec: target_v,
                        target_audio_codec: target_a,
                        target_resolution: target_res,
                        target_bitrate,
                        seek_position_ms: None,
                    },
                    data_dir,
                )
                .await?;
            stream_url = format!("/api/v1/transcode/{}/manifest.m3u8", session.id);
            transcode_session_id = Some(session.id);
        }
    }

    let play_session_id = create_play_session(
        pool,
        user_id,
        media_item_id,
        library_id,
        stream_decision_str,
        transcode_session_id,
        Some(media_file_details.id),
        req.quality_mode.as_deref(),
        req.max_streaming_bitrate,
    )
    .await?;

    Ok(PlaybackStartResponse {
        session_id: play_session_id,
        stream_decision: stream_decision_str.to_string(),
        stream_url,
        media_item_id,
        media_file_id: Some(media_file_details.id),
        source_video_codec: Some(media_info.video_codec),
        source_audio_codec: Some(media_info.audio_codec),
        target_video_codec: decision.target_video_codec,
        target_audio_codec: decision.target_audio_codec,
        transcode_session_id,
    })
}

struct MediaFileDetails {
    id: Uuid,
    file_path: String,
    container_format: String,
    video_codec: Option<String>,
    video_resolution: Option<String>,
    video_bitrate: Option<i32>,
    video_dynamic_range: Option<String>,
    video_frame_rate: Option<f64>,
    audio_codec: Option<String>,
    audio_channels: Option<i32>,
    audio_language: Option<String>,
    audio_bitrate: Option<i32>,
    #[allow(dead_code)]
    runtime_seconds: i32,
    additional_streams: serde_json::Value,
}

async fn fetch_media_file_details(
    pool: &PgPool,
    file_id: Uuid,
) -> Result<MediaFileDetails, PlaybackError> {
    let row = sqlx::query(
        "SELECT id, file_path, container_format, video_codec, \
         video_resolution, video_bitrate, video_dynamic_range, \
         audio_codec, audio_channels, audio_language, audio_bitrate, \
         runtime_seconds, additional_streams \
         FROM media_files WHERE id = $1 AND is_healthy = true",
    )
    .bind(file_id)
    .fetch_optional(pool)
    .await?
    .ok_or(PlaybackError::FileNotFound)?;

    Ok(row_to_media_file_details(&row))
}

async fn select_best_media_file(
    pool: &PgPool,
    media_item_id: Uuid,
) -> Result<MediaFileDetails, PlaybackError> {
    let row = sqlx::query(
        "SELECT id, file_path, container_format, video_codec, \
         video_resolution, video_bitrate, video_dynamic_range, \
         audio_codec, audio_channels, audio_language, audio_bitrate, \
         runtime_seconds, additional_streams \
         FROM media_files WHERE media_item_id = $1 AND is_healthy = true \
         ORDER BY file_size DESC LIMIT 1",
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await?
    .ok_or(PlaybackError::FileNotFound)?;

    Ok(row_to_media_file_details(&row))
}

fn row_to_media_file_details(row: &sqlx::postgres::PgRow) -> MediaFileDetails {
    MediaFileDetails {
        id: row.get("id"),
        file_path: row.get("file_path"),
        container_format: row.get("container_format"),
        video_codec: row.try_get("video_codec").ok().flatten(),
        video_resolution: row.try_get("video_resolution").ok().flatten(),
        video_bitrate: row.try_get("video_bitrate").ok().flatten(),
        video_dynamic_range: row.try_get("video_dynamic_range").ok().flatten(),
        video_frame_rate: None,
        audio_codec: row.try_get("audio_codec").ok().flatten(),
        audio_channels: row.try_get("audio_channels").ok().flatten(),
        audio_language: row.try_get("audio_language").ok().flatten(),
        audio_bitrate: row.try_get("audio_bitrate").ok().flatten(),
        runtime_seconds: row.get("runtime_seconds"),
        additional_streams: row
            .try_get("additional_streams")
            .unwrap_or(serde_json::json!({})),
    }
}

fn build_media_file_info(details: &MediaFileDetails) -> MediaFileInfo {
    let (res_w, res_h) = details
        .video_resolution
        .as_deref()
        .map(decision_engine::parse_resolution_string)
        .unwrap_or((1920, 1080));

    let bit_depth = extract_video_bit_depth(&details.additional_streams);

    let (subtitle_format, has_embedded_subtitles) =
        extract_subtitle_info(&details.additional_streams);

    let frame_rate = details.video_frame_rate.unwrap_or(24.0);

    MediaFileInfo {
        container_format: details.container_format.clone(),
        video_codec: details
            .video_codec
            .clone()
            .unwrap_or_else(|| "h264".to_string()),
        video_profile: extract_video_profile(&details.additional_streams),
        video_level: extract_video_level(&details.additional_streams),
        video_bit_depth: bit_depth,
        video_resolution: (res_w, res_h),
        video_bitrate_bps: details.video_bitrate.unwrap_or(0) as u64,
        video_dynamic_range: details
            .video_dynamic_range
            .clone()
            .unwrap_or_else(|| "sdr".to_string()),
        video_frame_rate: frame_rate,
        audio_codec: details
            .audio_codec
            .clone()
            .unwrap_or_else(|| "aac".to_string()),
        audio_channels: details.audio_channels.unwrap_or(2) as u32,
        audio_bitrate_bps: details.audio_bitrate.unwrap_or(0) as u64,
        audio_language: details.audio_language.clone(),
        has_embedded_subtitles,
        subtitle_format,
        additional_streams: Some(details.additional_streams.clone()),
    }
}

fn extract_video_bit_depth(streams: &serde_json::Value) -> u32 {
    streams
        .get("video")
        .and_then(|v| v.get("bit_depth"))
        .and_then(|b| b.as_u64())
        .map(|b| b as u32)
        .unwrap_or(8)
}

fn extract_video_profile(streams: &serde_json::Value) -> Option<String> {
    streams
        .get("video")
        .and_then(|v| v.get("profile"))
        .and_then(|p| p.as_str())
        .map(String::from)
}

fn extract_video_level(streams: &serde_json::Value) -> Option<f32> {
    streams
        .get("video")
        .and_then(|v| v.get("level"))
        .and_then(|l| l.as_f64())
        .map(|l| l as f32)
}

fn extract_subtitle_info(streams: &serde_json::Value) -> (Option<String>, bool) {
    let subs = match streams.get("subtitles").and_then(|s| s.as_array()) {
        Some(arr) if !arr.is_empty() => arr,
        _ => return (None, false),
    };

    let fmt = subs
        .first()
        .and_then(|s| s.get("codec"))
        .and_then(|c| c.as_str())
        .map(String::from);

    (fmt, true)
}

fn build_device_capabilities(
    device_profile: Option<&serde_json::Value>,
    config: &RuntimeConfig,
) -> DeviceCapabilities {
    match device_profile {
        Some(profile) => parse_device_profile(profile, config),
        None => conservative_device_defaults(config),
    }
}

fn parse_device_profile(profile: &serde_json::Value, config: &RuntimeConfig) -> DeviceCapabilities {
    DeviceCapabilities {
        video_codecs: decision_engine::parse_json_string_set(
            profile
                .get("video_codecs")
                .unwrap_or(&serde_json::json!([])),
        ),
        audio_codecs: decision_engine::parse_json_string_set(
            profile
                .get("audio_codecs")
                .unwrap_or(&serde_json::json!([])),
        ),
        containers: decision_engine::parse_json_string_set(
            profile.get("containers").unwrap_or(&serde_json::json!([])),
        ),
        subtitle_formats: decision_engine::parse_json_string_set(
            profile
                .get("subtitle_formats")
                .unwrap_or(&serde_json::json!([])),
        ),
        max_resolution: profile
            .get("max_resolution")
            .and_then(|r| r.as_str())
            .map(decision_engine::parse_resolution_string)
            .unwrap_or((1920, 1080)),
        max_audio_channels: profile
            .get("max_audio_channels")
            .and_then(|c| c.as_u64())
            .map(|c| c as u32)
            .unwrap_or(2),
        hdr_formats: decision_engine::parse_hdr_formats(
            profile.get("hdr_formats").unwrap_or(&serde_json::json!([])),
        ),
        max_bitrate_bps: profile
            .get("max_bitrate_bps")
            .and_then(|b| b.as_u64())
            .unwrap_or(20_000_000),
        supports_dolby_vision: profile
            .get("supports_dolby_vision")
            .and_then(|d| d.as_bool())
            .unwrap_or(false),
        allow_client_side_dv_fallback: profile
            .get("allow_client_side_dv_fallback")
            .and_then(|d| d.as_bool())
            .unwrap_or(config.quality.allow_client_side_dv_fallback),
        max_video_bit_depth: profile
            .get("max_video_bit_depth")
            .and_then(|b| b.as_u64())
            .map(|b| b as u32)
            .unwrap_or(8),
    }
}

fn conservative_device_defaults(config: &RuntimeConfig) -> DeviceCapabilities {
    let mut video_codecs = HashSet::new();
    video_codecs.insert("h264".to_string());

    let mut audio_codecs = HashSet::new();
    audio_codecs.insert("aac".to_string());

    let mut containers = HashSet::new();
    containers.insert("mp4".to_string());
    containers.insert("mkv".to_string());
    containers.insert("matroska".to_string());

    let mut subtitle_formats = HashSet::new();
    subtitle_formats.insert("srt".to_string());
    subtitle_formats.insert("webvtt".to_string());

    DeviceCapabilities {
        video_codecs,
        audio_codecs,
        containers,
        subtitle_formats,
        max_resolution: decision_engine::parse_resolution_string(
            &config.quality.fallback_max_resolution,
        ),
        max_audio_channels: 2,
        hdr_formats: HashSet::new(),
        max_bitrate_bps: config.quality.fallback_max_bitrate_bps as u64,
        supports_dolby_vision: false,
        allow_client_side_dv_fallback: config.quality.allow_client_side_dv_fallback,
        max_video_bit_depth: 8,
    }
}

async fn build_network_conditions(
    pool: &PgPool,
    user_id: Uuid,
    max_streaming_bitrate: Option<u64>,
) -> NetworkConditions {
    if let Some(bitrate) = max_streaming_bitrate {
        return NetworkConditions {
            estimated_throughput_bps: Some(bitrate),
            network_tier: None,
        };
    }

    let row = sqlx::query(
        "SELECT throughput_bps, network_tier FROM client_network_reports \
         WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match row {
        Some(r) => {
            let throughput: Option<i64> = r.try_get("throughput_bps").ok().flatten();
            let tier: Option<String> = r.try_get("network_tier").ok().flatten();
            NetworkConditions {
                estimated_throughput_bps: throughput.map(|t| t as u64),
                network_tier: tier,
            }
        }
        None => NetworkConditions {
            estimated_throughput_bps: None,
            network_tier: None,
        },
    }
}

fn build_decision_engine_config(
    config: &RuntimeConfig,
    quality_mode: Option<&str>,
    max_streaming_bitrate: Option<u64>,
) -> DecisionEngineConfig {
    let mode = quality_mode.unwrap_or(&config.quality.default_quality_mode);
    DecisionEngineConfig {
        default_video_codec: config.transcoding.default_video_codec.clone(),
        default_audio_codec: config.transcoding.default_audio_codec.clone(),
        fallback_max_resolution: decision_engine::parse_resolution_string(
            &config.quality.fallback_max_resolution,
        ),
        fallback_max_bitrate_bps: config.quality.fallback_max_bitrate_bps as u64,
        throughput_safety_factor: config.quality.throughput_safety_factor,
        allow_client_side_dv_fallback: config.quality.allow_client_side_dv_fallback,
        audio_passthrough_enabled: config.quality.audio_passthrough_enabled,
        subtitle_burn_in_policy: config.quality.subtitle_burn_in_policy.clone(),
        quality_mode: decision_engine::parse_quality_mode(mode),
        manual_max_resolution: mode
            .eq_ignore_ascii_case("manual")
            .then(|| max_streaming_bitrate.map(max_resolution_for_manual_bitrate))
            .flatten(),
    }
}

fn max_resolution_for_manual_bitrate(bitrate_bps: u64) -> (u32, u32) {
    if bitrate_bps >= 10_000_000 {
        (3840, 2160)
    } else if bitrate_bps >= 5_000_000 {
        (1920, 1080)
    } else if bitrate_bps >= 2_000_000 {
        (1280, 720)
    } else {
        (854, 480)
    }
}

async fn create_play_session(
    pool: &PgPool,
    user_id: Uuid,
    media_item_id: Uuid,
    library_id: Uuid,
    stream_decision: &str,
    transcode_session_id: Option<Uuid>,
    media_file_id: Option<Uuid>,
    quality_mode: Option<&str>,
    max_streaming_bitrate: Option<u64>,
) -> Result<Uuid, PlaybackError> {
    let session_id = Uuid::now_v7();

    let metadata = serde_json::json!({
        "transcode_session_id": transcode_session_id,
        "media_file_id": media_file_id,
        "current_state": "playing",
        "current_position_ms": 0,
        "quality_mode": quality_mode,
        "max_streaming_bitrate": max_streaming_bitrate,
    });

    sqlx::query(
        "INSERT INTO play_sessions (id, user_id, media_item_id, library_id, \
         started_at, client_name, stream_decision, metadata) \
         VALUES ($1, $2, $3, $4, now(), 'duskcue-web', $5, $6)",
    )
    .bind(session_id)
    .bind(user_id)
    .bind(media_item_id)
    .bind(library_id)
    .bind(stream_decision)
    .bind(&metadata)
    .execute(pool)
    .await?;

    Ok(session_id)
}

pub async fn heartbeat(
    pool: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
    position_ms: Option<i32>,
    state: Option<&str>,
    is_paused: Option<bool>,
    is_buffering: Option<bool>,
) -> Result<HeartbeatResponse, PlaybackError> {
    let row = sqlx::query(
        "SELECT id, user_id, media_item_id, metadata \
         FROM play_sessions \
         WHERE id = $1 AND stopped_at IS NULL",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?
    .ok_or(PlaybackError::SessionNotFound)?;

    let session_user_id: Uuid = row
        .try_get("user_id")
        .map_err(|_| PlaybackError::SessionNotFound)?;
    if session_user_id != user_id {
        return Err(PlaybackError::SessionNotFound);
    }

    let media_item_id: Uuid = row.try_get("media_item_id").unwrap_or_default();
    let metadata: serde_json::Value = row.try_get("metadata").unwrap_or(serde_json::json!({}));

    let prev_state = metadata
        .get("current_state")
        .and_then(|s| s.as_str())
        .unwrap_or("playing")
        .to_string();

    let media_file_id = metadata
        .get("media_file_id")
        .and_then(|f| f.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

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
            pool,
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

    merge_session_metadata(pool, session_id, merge).await?;

    if position_ms.is_some() {
        upsert_user_item_data_heartbeat(
            pool,
            user_id,
            media_item_id,
            effective_position,
            media_file_id,
        )
        .await?;
    }

    emit_play_event(
        pool,
        session_id,
        user_id,
        "heartbeat",
        Some(effective_position / 1000),
        serde_json::json!({"state": effective_state}),
    )
    .await?;

    Ok(HeartbeatResponse {
        session_id,
        position_ms: effective_position,
    })
}

pub async fn stop_playback(
    pool: &PgPool,
    transcode_manager: &TranscodeManager,
    user_id: Uuid,
    session_id: Uuid,
    final_position_ms: Option<i32>,
) -> Result<StopPlaybackResponse, PlaybackError> {
    let row = sqlx::query(
        "SELECT id, user_id, media_item_id, started_at, metadata \
         FROM play_sessions WHERE id = $1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?
    .ok_or(PlaybackError::SessionNotFound)?;

    let session_user_id: Uuid = row
        .try_get("user_id")
        .map_err(|_| PlaybackError::SessionNotFound)?;
    if session_user_id != user_id {
        return Err(PlaybackError::SessionNotFound);
    }

    let media_item_id: Uuid = row.try_get("media_item_id").unwrap_or_default();
    let started_at: chrono::DateTime<chrono::Utc> = row.try_get("started_at").unwrap_or_default();
    let metadata: serde_json::Value = row.try_get("metadata").unwrap_or(serde_json::json!({}));

    let transcode_session_id = metadata
        .get("transcode_session_id")
        .and_then(|t| t.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    let media_file_id = metadata
        .get("media_file_id")
        .and_then(|f| f.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    let stored_position = metadata
        .get("current_position_ms")
        .and_then(|p| p.as_i64())
        .map(|p| p as i32)
        .unwrap_or(0);

    let final_position = final_position_ms.unwrap_or(stored_position);

    if let Some(ts_id) = transcode_session_id {
        let _ = transcode_manager.stop_session(ts_id).await;
    }

    let runtime_seconds: Option<i32> = if let Some(mf_id) = media_file_id {
        sqlx::query_scalar::<_, Option<i32>>(
            "SELECT runtime_seconds FROM media_files WHERE id = $1",
        )
        .bind(mf_id)
        .fetch_optional(pool)
        .await?
        .flatten()
    } else {
        None
    };

    let percent_complete = runtime_seconds
        .filter(|&r| r > 0)
        .map(|r| ((final_position as f64) / (r as f64 * 1000.0) * 100.0).min(100.0) as f32);

    let is_watched = percent_complete.map(|p| p >= 90.0).unwrap_or(false);
    let resume_position = if is_watched { 0 } else { final_position };

    let now = chrono::Utc::now();
    let duration_seconds = (now - started_at).num_seconds().max(0) as i32;

    let stop_merge = serde_json::json!({
        "current_state": "stopped",
        "current_position_ms": final_position
    });

    sqlx::query(
        "UPDATE play_sessions \
         SET stopped_at = now(), \
             duration_seconds = $2, \
             percent_complete = $3, \
             metadata = metadata || $4, \
             updated_at = now() \
         WHERE id = $1",
    )
    .bind(session_id)
    .bind(duration_seconds)
    .bind(percent_complete)
    .bind(&stop_merge)
    .execute(pool)
    .await?;

    emit_play_event(
        pool,
        session_id,
        user_id,
        "stop",
        Some(final_position / 1000),
        serde_json::json!({"duration_seconds": duration_seconds, "percent_complete": percent_complete}),
    )
    .await?;

    let play_count = upsert_user_item_data_stop(
        pool,
        user_id,
        media_item_id,
        is_watched,
        resume_position,
        media_file_id,
    )
    .await?;

    Ok(StopPlaybackResponse {
        session_id,
        media_item_id,
        duration_seconds,
        percent_complete,
        is_watched,
        play_count,
    })
}

pub async fn seek(
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

    let row = sqlx::query(
        "SELECT id, user_id, media_item_id, metadata \
         FROM play_sessions \
         WHERE id = $1 AND stopped_at IS NULL",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?
    .ok_or(PlaybackError::SessionNotFound)?;

    let session_user_id: Uuid = row
        .try_get("user_id")
        .map_err(|_| PlaybackError::SessionNotFound)?;
    if session_user_id != user_id {
        return Err(PlaybackError::SessionNotFound);
    }

    let media_item_id: Uuid = row.try_get("media_item_id").unwrap_or_default();
    let metadata: serde_json::Value = row.try_get("metadata").unwrap_or(serde_json::json!({}));

    let transcode_session_id = metadata
        .get("transcode_session_id")
        .and_then(|t| t.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    let (new_stream_url, new_transcode_session_id) = if let Some(ts_id) = transcode_session_id {
        let new_session = transcode_manager
            .seek_session(ts_id, position_ms as i64, data_dir)
            .await?;

        let new_id = new_session.id;
        let url = format!("/api/v1/transcode/{}/manifest.m3u8", new_id);

        let merge = serde_json::json!({
            "transcode_session_id": new_id,
            "current_position_ms": position_ms,
            "current_state": "playing"
        });
        merge_session_metadata(pool, session_id, merge).await?;

        (Some(url), Some(new_id))
    } else {
        let merge = serde_json::json!({
            "current_position_ms": position_ms,
            "current_state": "playing"
        });
        merge_session_metadata(pool, session_id, merge).await?;
        (None, None)
    };

    upsert_user_item_data_heartbeat(
        pool,
        user_id,
        media_item_id,
        position_ms,
        metadata
            .get("media_file_id")
            .and_then(|f| f.as_str())
            .and_then(|s| Uuid::parse_str(s).ok()),
    )
    .await?;

    emit_play_event(
        pool,
        session_id,
        user_id,
        "seek",
        Some(position_ms / 1000),
        serde_json::json!({"target_position_ms": position_ms}),
    )
    .await?;

    Ok(SeekResponse {
        session_id,
        position_ms,
        stream_url: new_stream_url,
        transcode_session_id: new_transcode_session_id,
    })
}

pub async fn get_playback_info(
    pool: &PgPool,
    transcode_manager: &TranscodeManager,
    user_id: Uuid,
    session_id: Uuid,
) -> Result<PlaybackInfoResponse, PlaybackError> {
    let row = sqlx::query(
        "SELECT id, user_id, media_item_id, stream_decision, started_at, metadata \
         FROM play_sessions WHERE id = $1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?
    .ok_or(PlaybackError::SessionNotFound)?;

    let session_user_id: Uuid = row
        .try_get("user_id")
        .map_err(|_| PlaybackError::SessionNotFound)?;
    if session_user_id != user_id {
        return Err(PlaybackError::SessionNotFound);
    }

    let media_item_id: Uuid = row.try_get("media_item_id").unwrap_or_default();
    let stream_decision: String = row.try_get("stream_decision").unwrap_or_default();
    let started_at: chrono::DateTime<chrono::Utc> = row.try_get("started_at").unwrap_or_default();
    let metadata: serde_json::Value = row.try_get("metadata").unwrap_or(serde_json::json!({}));

    let position_ms = metadata
        .get("current_position_ms")
        .and_then(|p| p.as_i64())
        .map(|p| p as i32)
        .unwrap_or(0);

    let is_paused = metadata
        .get("current_state")
        .and_then(|s| s.as_str())
        .map(|s| s == "paused")
        .unwrap_or(false);

    let transcode_session_id = metadata
        .get("transcode_session_id")
        .and_then(|t| t.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    let media_file_id = metadata
        .get("media_file_id")
        .and_then(|f| f.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    let transcode_progress = transcode_session_id.and_then(|ts_id| {
        transcode_manager
            .get_session(&ts_id)
            .and_then(|s| s.progress_percent())
    });

    let duration_ms: Option<i32> = if let Some(mf_id) = media_file_id {
        sqlx::query_scalar::<_, Option<i32>>(
            "SELECT runtime_seconds FROM media_files WHERE id = $1",
        )
        .bind(mf_id)
        .fetch_optional(pool)
        .await?
        .flatten()
        .map(|s| s * 1000)
    } else {
        None
    };

    Ok(PlaybackInfoResponse {
        session_id,
        media_item_id,
        stream_decision,
        position_ms,
        duration_ms,
        transcode_progress,
        is_paused,
        started_at,
    })
}

pub async fn get_user_item_data(
    pool: &PgPool,
    user_id: Uuid,
    media_item_id: Uuid,
) -> Result<UserItemDataResponse, PlaybackError> {
    let row = sqlx::query(
        "SELECT id, is_watched, play_count, last_played_at, resume_position_ms, \
         is_favorite, user_rating \
         FROM user_item_data WHERE user_id = $1 AND media_item_id = $2",
    )
    .bind(user_id)
    .bind(media_item_id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => Ok(UserItemDataResponse {
            id: r.try_get("id").unwrap_or_default(),
            media_item_id,
            is_watched: r.try_get("is_watched").unwrap_or(false),
            play_count: r.try_get("play_count").unwrap_or(0),
            last_played_at: r.try_get("last_played_at").ok().flatten(),
            resume_position_ms: r.try_get("resume_position_ms").unwrap_or(0),
            is_favorite: r.try_get("is_favorite").unwrap_or(false),
            user_rating: r.try_get("user_rating").ok().flatten(),
        }),
        None => Ok(UserItemDataResponse {
            id: Uuid::nil(),
            media_item_id,
            is_watched: false,
            play_count: 0,
            last_played_at: None,
            resume_position_ms: 0,
            is_favorite: false,
            user_rating: None,
        }),
    }
}

pub async fn update_user_item_data(
    pool: &PgPool,
    user_id: Uuid,
    media_item_id: Uuid,
    req: &UpdateWatchDataRequest,
) -> Result<UserItemDataResponse, PlaybackError> {
    let row = sqlx::query(
        "INSERT INTO user_item_data \
         (id, user_id, media_item_id, is_favorite, user_rating, \
          audio_stream_index, subtitle_stream_index) \
         VALUES (uuidv7(), $1, $2, $3, $4, $5, $6) \
         ON CONFLICT (user_id, media_item_id) \
         DO UPDATE SET \
            is_favorite = COALESCE($3, user_item_data.is_favorite), \
            user_rating = COALESCE($4, user_item_data.user_rating), \
            audio_stream_index = COALESCE($5, user_item_data.audio_stream_index), \
            subtitle_stream_index = COALESCE($6, user_item_data.subtitle_stream_index), \
            updated_at = now() \
         RETURNING id, is_watched, play_count, last_played_at, resume_position_ms, \
                   is_favorite, user_rating",
    )
    .bind(user_id)
    .bind(media_item_id)
    .bind(req.is_favorite)
    .bind(req.user_rating)
    .bind(req.audio_stream_index)
    .bind(req.subtitle_stream_index)
    .fetch_one(pool)
    .await?;

    Ok(UserItemDataResponse {
        id: row.try_get("id").unwrap_or_default(),
        media_item_id,
        is_watched: row.try_get("is_watched").unwrap_or(false),
        play_count: row.try_get("play_count").unwrap_or(0),
        last_played_at: row.try_get("last_played_at").ok().flatten(),
        resume_position_ms: row.try_get("resume_position_ms").unwrap_or(0),
        is_favorite: row.try_get("is_favorite").unwrap_or(false),
        user_rating: row.try_get("user_rating").ok().flatten(),
    })
}

async fn emit_play_event(
    pool: &PgPool,
    session_id: Uuid,
    user_id: Uuid,
    event_type: &str,
    position_seconds: Option<i32>,
    details: serde_json::Value,
) -> Result<(), PlaybackError> {
    sqlx::query(
        "INSERT INTO play_events (play_session_id, user_id, event_type, position_seconds, details) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(session_id)
    .bind(user_id)
    .bind(event_type)
    .bind(position_seconds)
    .bind(&details)
    .execute(pool)
    .await?;
    Ok(())
}

async fn merge_session_metadata(
    pool: &PgPool,
    session_id: Uuid,
    merge: serde_json::Value,
) -> Result<(), PlaybackError> {
    sqlx::query(
        "UPDATE play_sessions SET metadata = metadata || $2, updated_at = now() WHERE id = $1",
    )
    .bind(session_id)
    .bind(&merge)
    .execute(pool)
    .await?;
    Ok(())
}

async fn upsert_user_item_data_heartbeat(
    pool: &PgPool,
    user_id: Uuid,
    media_item_id: Uuid,
    position_ms: i32,
    media_file_id: Option<Uuid>,
) -> Result<(), PlaybackError> {
    sqlx::query(
        "INSERT INTO user_item_data (id, user_id, media_item_id, resume_position_ms, last_played_media_file_id) \
         VALUES (uuidv7(), $1, $2, $3, $4) \
         ON CONFLICT (user_id, media_item_id) \
         DO UPDATE SET resume_position_ms = $3, \
                       last_played_media_file_id = COALESCE($4, user_item_data.last_played_media_file_id), \
                       updated_at = now()"
    )
    .bind(user_id)
    .bind(media_item_id)
    .bind(position_ms)
    .bind(media_file_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn upsert_user_item_data_stop(
    pool: &PgPool,
    user_id: Uuid,
    media_item_id: Uuid,
    is_watched: bool,
    resume_position_ms: i32,
    media_file_id: Option<Uuid>,
) -> Result<i32, PlaybackError> {
    let row = sqlx::query(
        "INSERT INTO user_item_data (id, user_id, media_item_id, is_watched, play_count, last_played_at, resume_position_ms, last_played_media_file_id) \
         VALUES (uuidv7(), $1, $2, $3, 1, now(), $4, $5) \
         ON CONFLICT (user_id, media_item_id) \
         DO UPDATE SET play_count = user_item_data.play_count + 1, \
                       last_played_at = now(), \
                       is_watched = user_item_data.is_watched OR $3, \
                       resume_position_ms = $4, \
                       last_played_media_file_id = COALESCE($5, user_item_data.last_played_media_file_id), \
                       updated_at = now() \
         RETURNING play_count"
    )
    .bind(user_id)
    .bind(media_item_id)
    .bind(is_watched)
    .bind(resume_position_ms)
    .bind(media_file_id)
    .fetch_one(pool)
    .await?;

    let play_count: i32 = row.try_get("play_count").unwrap_or(1);
    Ok(play_count)
}

pub async fn list_bookmarks(
    pool: &PgPool,
    user_id: Uuid,
    media_item_id: Uuid,
) -> Result<BookmarkListResponse, PlaybackError> {
    let rows = sqlx::query(
        "SELECT id, media_item_id, position_ms, label, description, created_at \
         FROM bookmarks WHERE user_id = $1 AND media_item_id = $2 \
         ORDER BY position_ms ASC",
    )
    .bind(user_id)
    .bind(media_item_id)
    .fetch_all(pool)
    .await?;

    let items: Vec<BookmarkResponse> = rows
        .iter()
        .map(|r| BookmarkResponse {
            id: r.try_get("id").unwrap_or_default(),
            media_item_id: r.try_get("media_item_id").unwrap_or(media_item_id),
            position_ms: r.try_get("position_ms").unwrap_or(0),
            label: r.try_get("label").unwrap_or_default(),
            description: r.try_get("description").ok().flatten(),
            created_at: r.try_get("created_at").unwrap_or_default(),
        })
        .collect();

    Ok(BookmarkListResponse { items })
}

pub async fn create_bookmark(
    pool: &PgPool,
    user_id: Uuid,
    media_item_id: Uuid,
    req: &CreateBookmarkRequest,
) -> Result<BookmarkResponse, PlaybackError> {
    let row = sqlx::query(
        "INSERT INTO bookmarks (id, user_id, media_item_id, position_ms, label, description) \
         VALUES (uuidv7(), $1, $2, $3, $4, $5) \
         RETURNING id, media_item_id, position_ms, label, description, created_at",
    )
    .bind(user_id)
    .bind(media_item_id)
    .bind(req.position_ms)
    .bind(&req.label)
    .bind(&req.description)
    .fetch_one(pool)
    .await?;

    Ok(BookmarkResponse {
        id: row.try_get("id").unwrap_or_default(),
        media_item_id: row.try_get("media_item_id").unwrap_or(media_item_id),
        position_ms: row.try_get("position_ms").unwrap_or(0),
        label: row.try_get("label").unwrap_or_default(),
        description: row.try_get("description").ok().flatten(),
        created_at: row.try_get("created_at").unwrap_or_default(),
    })
}

pub async fn delete_bookmark(
    pool: &PgPool,
    user_id: Uuid,
    media_item_id: Uuid,
    bookmark_id: Uuid,
) -> Result<(), PlaybackError> {
    let result = sqlx::query(
        "DELETE FROM bookmarks \
         WHERE id = $1 AND user_id = $2 AND media_item_id = $3",
    )
    .bind(bookmark_id)
    .bind(user_id)
    .bind(media_item_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(PlaybackError::BookmarkNotFound);
    }

    Ok(())
}

pub async fn list_playlists(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<PlaylistListResponse, PlaybackError> {
    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM playlists WHERE user_id = $1 AND deleted_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query(
        "SELECT id, created_at, updated_at, name, description, visibility, \
         is_smart, item_count, total_duration_seconds \
         FROM playlists WHERE user_id = $1 AND deleted_at IS NULL \
         ORDER BY updated_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let items: Vec<PlaylistResponse> = rows.iter().map(row_to_playlist_response).collect();

    Ok(PlaylistListResponse { items, total })
}

pub async fn get_playlist(
    pool: &PgPool,
    user_id: Uuid,
    playlist_id: Uuid,
) -> Result<PlaylistResponse, PlaybackError> {
    let row = sqlx::query(
        "SELECT id, created_at, updated_at, name, description, visibility, \
         is_smart, item_count, total_duration_seconds \
         FROM playlists WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(playlist_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or(PlaybackError::PlaylistNotFound)?;
    Ok(row_to_playlist_response(&row))
}

pub async fn create_playlist(
    pool: &PgPool,
    user_id: Uuid,
    req: &CreatePlaylistRequest,
) -> Result<PlaylistResponse, PlaybackError> {
    let visibility = req.visibility.as_deref().unwrap_or("private");

    if !VALID_PLAYLIST_VISIBILITIES.contains(&visibility) {
        return Err(PlaybackError::InvalidVisibility(visibility.to_string()));
    }

    let row = sqlx::query(
        "INSERT INTO playlists (id, user_id, name, description, visibility) \
         VALUES (uuidv7(), $1, $2, $3, $4) \
         RETURNING id, created_at, updated_at, name, description, visibility, \
                   is_smart, item_count, total_duration_seconds",
    )
    .bind(user_id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(visibility)
    .fetch_one(pool)
    .await?;

    Ok(row_to_playlist_response(&row))
}

pub async fn update_playlist(
    pool: &PgPool,
    user_id: Uuid,
    playlist_id: Uuid,
    req: &UpdatePlaylistRequest,
) -> Result<PlaylistResponse, PlaybackError> {
    if let Some(ref vis) = req.visibility
        && !VALID_PLAYLIST_VISIBILITIES.contains(&vis.as_str())
    {
        return Err(PlaybackError::InvalidVisibility(vis.clone()));
    }

    let row = sqlx::query(
        "UPDATE playlists SET \
         name = COALESCE($3, name), \
         description = COALESCE($4, description), \
         visibility = COALESCE($5, visibility), \
         updated_at = now() \
         WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL \
         RETURNING id, created_at, updated_at, name, description, visibility, \
                   is_smart, item_count, total_duration_seconds",
    )
    .bind(playlist_id)
    .bind(user_id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.visibility)
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or(PlaybackError::PlaylistNotFound)?;
    Ok(row_to_playlist_response(&row))
}

pub async fn delete_playlist(
    pool: &PgPool,
    user_id: Uuid,
    playlist_id: Uuid,
) -> Result<(), PlaybackError> {
    let result = sqlx::query(
        "UPDATE playlists SET deleted_at = now() \
         WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(playlist_id)
    .bind(user_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(PlaybackError::PlaylistNotFound);
    }

    Ok(())
}

pub async fn list_playlist_items(
    pool: &PgPool,
    user_id: Uuid,
    playlist_id: Uuid,
) -> Result<PlaylistItemListResponse, PlaybackError> {
    verify_playlist_ownership(pool, user_id, playlist_id).await?;

    let total =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM playlist_items WHERE playlist_id = $1")
            .bind(playlist_id)
            .fetch_one(pool)
            .await?;

    let rows = sqlx::query(
        "SELECT pi.id, pi.playlist_id, pi.media_item_id, pi.position, pi.created_at, \
         mi.title \
         FROM playlist_items pi \
         JOIN media_items mi ON mi.id = pi.media_item_id \
         WHERE pi.playlist_id = $1 \
         ORDER BY pi.position ASC",
    )
    .bind(playlist_id)
    .fetch_all(pool)
    .await?;

    let items: Vec<PlaylistItemResponse> = rows
        .iter()
        .map(|r| PlaylistItemResponse {
            id: r.try_get("id").unwrap_or_default(),
            playlist_id: r.try_get("playlist_id").unwrap_or(playlist_id),
            media_item_id: r.try_get("media_item_id").unwrap_or_default(),
            position: r.try_get("position").unwrap_or(0),
            title: r
                .try_get("title")
                .unwrap_or_else(|_| "Untitled".to_string()),
            created_at: r.try_get("created_at").unwrap_or_default(),
        })
        .collect();

    Ok(PlaylistItemListResponse { items, total })
}

pub async fn add_playlist_item(
    pool: &PgPool,
    user_id: Uuid,
    playlist_id: Uuid,
    req: &AddPlaylistItemRequest,
) -> Result<PlaylistItemResponse, PlaybackError> {
    verify_playlist_ownership(pool, user_id, playlist_id).await?;

    let media_item_id = req
        .media_item_id
        .ok_or(PlaybackError::PlaylistItemNotFound)?;

    let position = if let Some(pos) = req.position {
        pos
    } else {
        let max_pos: Option<i32> =
            sqlx::query_scalar("SELECT MAX(position) FROM playlist_items WHERE playlist_id = $1")
                .bind(playlist_id)
                .fetch_one(pool)
                .await?;

        max_pos.map(|p| p + 1000).unwrap_or(1000)
    };

    let row = sqlx::query(
        "INSERT INTO playlist_items (id, playlist_id, media_item_id, position) \
         VALUES (uuidv7(), $1, $2, $3) \
         RETURNING id, playlist_id, media_item_id, position, created_at",
    )
    .bind(playlist_id)
    .bind(media_item_id)
    .bind(position)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.is_unique_violation()
        {
            return PlaybackError::PlaylistItemNotFound;
        }
        PlaybackError::from(e)
    })?;

    let title: String = sqlx::query_scalar("SELECT title FROM media_items WHERE id = $1")
        .bind(media_item_id)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|_| "Untitled".to_string());

    update_playlist_counters(pool, playlist_id).await?;

    Ok(PlaylistItemResponse {
        id: row.try_get("id").unwrap_or_default(),
        playlist_id: row.try_get("playlist_id").unwrap_or(playlist_id),
        media_item_id: row.try_get("media_item_id").unwrap_or(media_item_id),
        position: row.try_get("position").unwrap_or(position),
        title,
        created_at: row.try_get("created_at").unwrap_or_default(),
    })
}

pub async fn remove_playlist_item(
    pool: &PgPool,
    user_id: Uuid,
    playlist_id: Uuid,
    media_item_id: Uuid,
) -> Result<(), PlaybackError> {
    verify_playlist_ownership(pool, user_id, playlist_id).await?;

    let result = sqlx::query(
        "DELETE FROM playlist_items \
         WHERE playlist_id = $1 AND media_item_id = $2",
    )
    .bind(playlist_id)
    .bind(media_item_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(PlaybackError::PlaylistItemNotFound);
    }

    update_playlist_counters(pool, playlist_id).await?;

    Ok(())
}

fn row_to_playlist_response(row: &sqlx::postgres::PgRow) -> PlaylistResponse {
    PlaylistResponse {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        visibility: row.get("visibility"),
        is_smart: row.get("is_smart"),
        item_count: row.get("item_count"),
        total_duration_seconds: row.get("total_duration_seconds"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

async fn verify_playlist_ownership(
    pool: &PgPool,
    user_id: Uuid,
    playlist_id: Uuid,
) -> Result<(), PlaybackError> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM playlists \
         WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(playlist_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    if exists == 0 {
        return Err(PlaybackError::PlaylistNotFound);
    }

    Ok(())
}

async fn update_playlist_counters(pool: &PgPool, playlist_id: Uuid) -> Result<(), PlaybackError> {
    sqlx::query(
        "UPDATE playlists SET \
         item_count = (SELECT COUNT(*) FROM playlist_items WHERE playlist_id = $1), \
         updated_at = now() \
         WHERE id = $1",
    )
    .bind(playlist_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_media_file_path(
    pool: &PgPool,
    media_file_id: Uuid,
) -> Result<PathBuf, PlaybackError> {
    let row = sqlx::query(
        "SELECT mf.file_path, mf.is_healthy \
         FROM media_files mf \
         WHERE mf.id = $1",
    )
    .bind(media_file_id)
    .fetch_optional(pool)
    .await?
    .ok_or(PlaybackError::FileNotFound)?;

    let is_healthy: bool = row.try_get("is_healthy").unwrap_or(true);
    if !is_healthy {
        let path: String = row.try_get("file_path").unwrap_or_default();
        return Err(PlaybackError::FileUnhealthy(path));
    }

    let file_path: String = row
        .try_get("file_path")
        .map_err(|_| PlaybackError::FileNotFound)?;
    Ok(PathBuf::from(file_path))
}

pub async fn get_media_file_size(pool: &PgPool, media_file_id: Uuid) -> Result<u64, PlaybackError> {
    let path = get_media_file_path(pool, media_file_id).await?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| PlaybackError::FileNotFound)?;
    Ok(metadata.len())
}

pub struct RangeSpec {
    pub start: u64,
    pub end: u64,
    pub total: u64,
}

impl RangeSpec {
    pub fn parse(header: Option<&str>, file_size: u64) -> Result<Option<Self>, PlaybackError> {
        let header = match header {
            Some(h) => h,
            None => return Ok(None),
        };

        let bytes_spec = header
            .strip_prefix("bytes=")
            .ok_or_else(|| PlaybackError::InvalidByteRange("expected bytes= prefix".into()))?;

        let (start, end) = if let Some(rest) = bytes_spec.strip_suffix('-') {
            let start: u64 = rest.parse().map_err(|_| {
                PlaybackError::InvalidByteRange(format!("invalid start byte: {rest}"))
            })?;
            (start, file_size - 1)
        } else if let Some(rest) = bytes_spec.strip_prefix('-') {
            let suffix_len: u64 = rest.parse().map_err(|_| {
                PlaybackError::InvalidByteRange(format!("invalid suffix length: {rest}"))
            })?;
            let start = file_size.saturating_sub(suffix_len);
            (start, file_size - 1)
        } else {
            let parts: Vec<&str> = bytes_spec.split('-').collect();
            if parts.len() != 2 {
                return Err(PlaybackError::InvalidByteRange(format!(
                    "invalid range format: {bytes_spec}"
                )));
            }
            let start: u64 = parts[0].parse().map_err(|_| {
                PlaybackError::InvalidByteRange(format!("invalid start: {}", parts[0]))
            })?;
            let end: u64 = if parts[1].is_empty() {
                file_size - 1
            } else {
                parts[1].parse().map_err(|_| {
                    PlaybackError::InvalidByteRange(format!("invalid end: {}", parts[1]))
                })?
            };
            (start, end)
        };

        if start > end || start >= file_size {
            return Err(PlaybackError::InvalidByteRange(format!(
                "range {start}-{end} out of bounds for file size {file_size}"
            )));
        }

        let end = end.min(file_size - 1);

        Ok(Some(Self {
            start,
            end,
            total: file_size,
        }))
    }

    pub fn content_length(&self) -> u64 {
        self.end - self.start + 1
    }

    pub fn content_range_header(&self) -> String {
        format!("bytes {}-{}/{}", self.start, self.end, self.total)
    }
}

pub fn guess_content_type(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "mkv" => "video/x-matroska",
        "mp4" | "m4v" => "video/mp4",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        "webm" => "video/webm",
        "ts" => "video/mp2t",
        "mpg" | "mpeg" => "video/mpeg",
        "ogg" | "ogv" => "video/ogg",
        "3gp" => "video/3gpp",
        _ => "video/octet-stream",
    }
}

pub async fn get_transcode_manifest(
    transcode_manager: &TranscodeManager,
    session_id: Uuid,
) -> Result<String, PlaybackError> {
    let session = transcode_manager
        .get_session(&session_id)
        .ok_or(PlaybackError::SessionNotFound)?;

    let manifest_path = &session.manifest_path;
    let content = tokio::fs::read_to_string(manifest_path)
        .await
        .map_err(|_| PlaybackError::SessionNotFound)?;

    Ok(content)
}

pub async fn get_transcode_playlist(
    transcode_manager: &TranscodeManager,
    session_id: Uuid,
    rendition: &str,
) -> Result<String, PlaybackError> {
    let session = transcode_manager
        .get_session(&session_id)
        .ok_or(PlaybackError::SessionNotFound)?;

    let playlist_path = session.segment_dir.join(format!("{rendition}_index.m3u8"));

    if playlist_path.exists() {
        let content = tokio::fs::read_to_string(&playlist_path)
            .await
            .map_err(|_| PlaybackError::SessionNotFound)?;
        return Ok(content);
    }

    let manifest_content = tokio::fs::read_to_string(&session.manifest_path)
        .await
        .map_err(|_| PlaybackError::SessionNotFound)?;

    if is_single_rendition_manifest(&manifest_content) {
        if rendition == session.rendition_name {
            return Ok(manifest_content);
        }
        return Err(PlaybackError::SessionNotFound);
    }

    for line in manifest_content.lines() {
        if line.starts_with('#') {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(dir_rendition) = extract_rendition_from_path(trimmed)
            && dir_rendition == rendition
        {
            let playlist_path = session.segment_dir.join(trimmed);
            let content = tokio::fs::read_to_string(&playlist_path)
                .await
                .map_err(|_| PlaybackError::SessionNotFound)?;
            return Ok(content);
        }
    }

    Err(PlaybackError::SessionNotFound)
}

pub async fn get_transcode_segment(
    transcode_manager: &TranscodeManager,
    session_id: Uuid,
    rendition: &str,
    segment: &str,
) -> Result<Vec<u8>, PlaybackError> {
    let session = transcode_manager
        .get_session(&session_id)
        .ok_or(PlaybackError::SessionNotFound)?;

    validate_segment_filename(segment)?;

    let segment_path = if rendition == session.rendition_name {
        session.segment_dir.join(segment)
    } else {
        let rendition_dir = session.segment_dir.join(rendition);
        if rendition_dir.exists() {
            rendition_dir.join(segment)
        } else {
            session.segment_dir.join(segment)
        }
    };

    let data = tokio::fs::read(&segment_path)
        .await
        .map_err(|_| PlaybackError::SessionNotFound)?;

    Ok(data)
}

fn validate_segment_filename(name: &str) -> Result<(), PlaybackError> {
    if name.is_empty() || name.len() > 64 {
        return Err(PlaybackError::SessionNotFound);
    }
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err(PlaybackError::SessionNotFound);
    }
    if !name.starts_with("seg_") {
        return Err(PlaybackError::SessionNotFound);
    }
    Ok(())
}

fn is_single_rendition_manifest(content: &str) -> bool {
    let mut has_extinf = false;
    for line in content.lines() {
        if line.starts_with("#EXTINF") {
            has_extinf = true;
        }
        if line.starts_with("#EXT-X-STREAM-INF") {
            return false;
        }
    }
    has_extinf
}

fn extract_rendition_from_path(path: &str) -> Option<String> {
    let path = path.trim_end_matches('/');
    let file_name = std::path::Path::new(path)
        .file_stem()?
        .to_str()?
        .to_string();
    if file_name.ends_with("_index") || file_name == "index" {
        let parent = std::path::Path::new(path).parent()?.file_name()?.to_str()?;
        return Some(parent.to_string());
    }
    None
}

pub fn generate_master_manifest(_session_id: Uuid, renditions: &[TranscodeRendition]) -> String {
    let mut lines = vec![
        "#EXTM3U".to_string(),
        "#EXT-X-VERSION:7".to_string(),
        "#EXT-X-INDEPENDENT-SEGMENTS".to_string(),
    ];

    for rendition in renditions {
        let bandwidth = (rendition.video_bitrate + rendition.audio_bitrate) / 1000;
        lines.push(format!(
            "#EXT-X-STREAM-INF:BANDWIDTH={bandwidth},RESOLUTION={width}x{height},CODECS=\"avc1.64001f,mp4a.40.2\"",
            width = rendition.width,
            height = rendition.height,
        ));
        lines.push(format!(
            "/{rendition}/index.m3u8",
            rendition = rendition.name
        ));
    }

    lines.join("\n")
}

pub async fn list_streaming_policies(
    pool: &PgPool,
) -> Result<StreamingPolicyListResponse, PlaybackError> {
    let count_row = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM streaming_policies")
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
         ORDER BY is_system DESC, name ASC",
    )
    .fetch_all(pool)
    .await?;

    let items: Vec<StreamingPolicyResponse> = rows.iter().map(row_to_policy_response).collect();

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
         FROM streaming_policies WHERE id = $1",
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

    let existing =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM streaming_policies WHERE name = $1")
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
         auto_terminate_paused_minutes, is_default, is_system, metadata",
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

    let _existing =
        sqlx::query("SELECT id, is_system, is_default FROM streaming_policies WHERE id = $1")
            .bind(policy_id)
            .fetch_optional(pool)
            .await?
            .ok_or(PlaybackError::PolicyNotFound)?;

    if let Some(ref name) = req.name {
        let name_conflict = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM streaming_policies WHERE name = $1 AND id != $2",
        )
        .bind(name)
        .bind(policy_id)
        .fetch_one(pool)
        .await?;

        if name_conflict > 0 {
            return Err(PlaybackError::PolicyNameExists(name.clone()));
        }
    }

    let allowed_ip_json = req
        .allowed_ip_ranges
        .as_ref()
        .map(|r| ip_ranges_to_jsonb(Some(r)));
    let blocked_ip_json = req
        .blocked_ip_ranges
        .as_ref()
        .map(|r| ip_ranges_to_jsonb(Some(r)));

    let mut tx = pool.begin().await?;

    if req.is_default.unwrap_or(false) {
        sqlx::query(
            "UPDATE streaming_policies SET is_default = false WHERE is_default = true AND id != $1",
        )
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
         auto_terminate_paused_minutes, is_default, is_system, metadata",
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

pub async fn delete_streaming_policy(pool: &PgPool, policy_id: Uuid) -> Result<(), PlaybackError> {
    let row = sqlx::query("SELECT is_system, is_default FROM streaming_policies WHERE id = $1")
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
            "SELECT COUNT(*) FROM streaming_policies WHERE is_default = true AND id != $1",
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
             FROM streaming_policies WHERE id = $1",
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
             FROM streaming_policies WHERE is_default = true LIMIT 1",
        )
        .fetch_optional(pool)
        .await?
    };

    let (
        policy_id,
        policy_name,
        p_max_streams,
        p_max_transcode,
        p_bandwidth,
        p_allow_dp,
        p_allow_ds,
        p_allow_transcode,
        p_max_res,
        p_allow_4k,
        p_require_dp_4k,
        p_allowed_ips,
        p_blocked_ips,
        p_auto_terminate,
    ) = if let Some(ref r) = policy_row {
        (
            Some(r.try_get::<Uuid, _>("id").unwrap_or_default()),
            Some(r.try_get::<String, _>("name").unwrap_or_default()),
            r.try_get::<Option<i32>, _>("max_streams").ok().flatten(),
            r.try_get::<Option<i32>, _>("max_transcode_streams")
                .ok()
                .flatten(),
            r.try_get::<Option<i64>, _>("bandwidth_limit_bps")
                .ok()
                .flatten(),
            r.try_get::<bool, _>("allow_direct_play").unwrap_or(true),
            r.try_get::<bool, _>("allow_direct_stream").unwrap_or(true),
            r.try_get::<bool, _>("allow_transcode").unwrap_or(true),
            r.try_get::<Option<String>, _>("max_transcode_resolution")
                .ok()
                .flatten(),
            r.try_get::<bool, _>("allow_transcode_4k").unwrap_or(true),
            r.try_get::<bool, _>("require_direct_play_4k")
                .unwrap_or(false),
            jsonb_to_string_vec(
                r.try_get::<serde_json::Value, _>("allowed_ip_ranges")
                    .unwrap_or(serde_json::json!([])),
            ),
            jsonb_to_string_vec(
                r.try_get::<serde_json::Value, _>("blocked_ip_ranges")
                    .unwrap_or(serde_json::json!([])),
            ),
            r.try_get::<Option<i32>, _>("auto_terminate_paused_minutes")
                .ok()
                .flatten(),
        )
    } else {
        (
            None,
            None,
            None,
            None,
            None,
            true,
            true,
            true,
            None,
            true,
            false,
            vec![],
            vec![],
            None,
        )
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
            r.iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect(),
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
