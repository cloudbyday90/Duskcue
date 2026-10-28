// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use sqlx::{PgConnection, Row};
use uuid::Uuid;

use super::PlaybackError;
use super::track_selection::default_audio_stream_index;
use crate::services::decision_engine::{self, MediaFileInfo};

#[derive(Clone)]
pub(super) struct MediaFileDetails {
    pub(super) id: Uuid,
    pub(super) file_size: i64,
    pub(super) updated_at: chrono::DateTime<chrono::Utc>,
    pub(super) file_modified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(super) file_path: String,
    pub(super) container_format: String,
    pub(super) video_codec: Option<String>,
    pub(super) video_resolution: Option<String>,
    pub(super) video_bitrate: Option<i32>,
    pub(super) video_dynamic_range: Option<String>,
    pub(super) video_frame_rate: Option<f64>,
    pub(super) audio_codec: Option<String>,
    pub(super) audio_channels: Option<i32>,
    pub(super) audio_language: Option<String>,
    pub(super) audio_bitrate: Option<i32>,
    #[allow(dead_code)]
    pub(super) runtime_seconds: i32,
    pub(super) additional_streams: serde_json::Value,
}

pub(super) async fn fetch_media_file_details_on_connection(
    connection: &mut PgConnection,
    file_id: Uuid,
    media_item_id: Uuid,
) -> Result<MediaFileDetails, PlaybackError> {
    let row = sqlx::query(
        "SELECT id, file_size, updated_at, file_modified_at, file_path, container_format, video_codec, \
         video_resolution, video_bitrate, video_dynamic_range, \
         audio_codec, audio_channels, audio_language, audio_bitrate, \
         runtime_seconds, additional_streams \
         FROM media_files WHERE id = $1 AND media_item_id = $2 AND is_healthy = true",
    )
    .bind(file_id)
    .bind(media_item_id)
    .fetch_optional(connection)
    .await?
    .ok_or(PlaybackError::FileNotFound)?;

    Ok(row_to_media_file_details(&row))
}

pub(super) async fn select_best_media_file_on_connection(
    connection: &mut PgConnection,
    media_item_id: Uuid,
) -> Result<MediaFileDetails, PlaybackError> {
    let row = sqlx::query(
        "SELECT id, file_size, updated_at, file_modified_at, file_path, container_format, video_codec, \
         video_resolution, video_bitrate, video_dynamic_range, \
         audio_codec, audio_channels, audio_language, audio_bitrate, \
         runtime_seconds, additional_streams \
         FROM media_files WHERE media_item_id = $1 AND is_healthy = true \
         ORDER BY file_size DESC LIMIT 1",
    )
    .bind(media_item_id)
    .fetch_optional(connection)
    .await?
    .ok_or(PlaybackError::FileNotFound)?;

    Ok(row_to_media_file_details(&row))
}

pub(super) fn row_to_media_file_details(row: &sqlx::postgres::PgRow) -> MediaFileDetails {
    MediaFileDetails {
        id: row.get("id"),
        file_size: row.get("file_size"),
        updated_at: row.get("updated_at"),
        file_modified_at: row.try_get("file_modified_at").ok().flatten(),
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

#[derive(Clone)]
pub(super) struct SelectedTracks {
    pub(super) video_stream_index: Option<i32>,
    pub(super) audio_stream_index: Option<i32>,
    pub(super) subtitle_stream_index: Option<i32>,
    pub(super) subtitle_stream_ordinal: Option<usize>,
}

pub(super) fn build_media_file_info(
    details: &MediaFileDetails,
    audio_stream_index: Option<i32>,
    subtitle_stream_index: Option<i32>,
) -> Result<(MediaFileInfo, SelectedTracks), PlaybackError> {
    let audio_stream_index =
        audio_stream_index.or_else(|| default_audio_stream_index(&details.additional_streams));
    let (res_w, res_h) = source_resolution(details).unwrap_or((1920, 1080));

    let bit_depth = extract_video_bit_depth(&details.additional_streams);
    let selected_audio = audio_stream_index
        .map(|index| find_stream(&details.additional_streams, "audio", index, "audio"))
        .transpose()?;
    let selected_subtitle = subtitle_stream_index
        .map(|index| find_stream(&details.additional_streams, "subtitles", index, "subtitle"))
        .transpose()?;
    let audio = selected_audio.map(|(_, stream)| stream);
    let (subtitle_format, has_embedded_subtitles) = selected_subtitle
        .as_ref()
        .map(|(_, stream)| (stream_codec(stream), true))
        .unwrap_or((None, false));

    let frame_rate = details
        .additional_streams
        .get("video")
        .and_then(|video| video.get("frame_rate"))
        .and_then(serde_json::Value::as_f64)
        .filter(|rate| rate.is_finite() && *rate > 0.0)
        .or(details.video_frame_rate)
        .unwrap_or(0.0);

    let media_info = MediaFileInfo {
        container_format: details.container_format.clone(),
        video_codec: details
            .video_codec
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        video_profile: extract_video_profile(&details.additional_streams),
        video_level: extract_video_level(&details.additional_streams),
        video_bit_depth: bit_depth,
        video_resolution: (res_w, res_h),
        video_bitrate_bps: details.video_bitrate.unwrap_or(0) as u64,
        video_dynamic_range: details
            .video_dynamic_range
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        video_frame_rate: frame_rate,
        audio_codec: audio
            .and_then(stream_codec)
            .or_else(|| details.audio_codec.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        audio_channels: audio
            .and_then(|stream| stream.get("channels"))
            .and_then(serde_json::Value::as_u64)
            .map(|channels| channels as u32)
            .unwrap_or_else(|| details.audio_channels.unwrap_or(2) as u32),
        audio_bitrate_bps: audio
            .and_then(|stream| stream.get("bitrate"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_else(|| details.audio_bitrate.unwrap_or(0) as u64),
        audio_language: audio
            .and_then(|stream| stream.get("language"))
            .and_then(serde_json::Value::as_str)
            .map(String::from)
            .or_else(|| details.audio_language.clone()),
        has_embedded_subtitles,
        subtitle_format,
        additional_streams: Some(details.additional_streams.clone()),
    };

    Ok((
        media_info,
        SelectedTracks {
            video_stream_index: details
                .additional_streams
                .get("video")
                .and_then(|video| video.get("index"))
                .and_then(serde_json::Value::as_i64)
                .and_then(|index| i32::try_from(index).ok())
                .filter(|index| *index >= 0),
            audio_stream_index,
            subtitle_stream_index,
            subtitle_stream_ordinal: selected_subtitle.map(|(ordinal, _)| ordinal),
        },
    ))
}

pub(super) fn source_resolution(details: &MediaFileDetails) -> Option<(u32, u32)> {
    let value = details
        .video_resolution
        .as_deref()?
        .trim()
        .to_ascii_lowercase();
    if let Some((width, height)) = value.split_once('x') {
        let width: u32 = width.trim().parse().ok()?;
        let height: u32 = height.trim().parse().ok()?;
        return ((2..=65_535).contains(&width) && (2..=65_535).contains(&height))
            .then_some((width, height));
    }
    matches!(
        value.as_str(),
        "4k" | "2160p" | "uhd" | "1080p" | "fullhd" | "fhd" | "720p" | "hd" | "480p" | "sd"
    )
    .then(|| decision_engine::parse_resolution_string(&value))
}

pub(super) fn find_stream<'a>(
    streams: &'a serde_json::Value,
    category: &str,
    index: i32,
    track_name: &'static str,
) -> Result<(usize, &'a serde_json::Value), PlaybackError> {
    streams
        .get(category)
        .and_then(serde_json::Value::as_array)
        .and_then(|items| {
            items.iter().enumerate().find(|(_, stream)| {
                stream
                    .get("index")
                    .and_then(serde_json::Value::as_i64)
                    .map(|stream_index| stream_index == i64::from(index))
                    .unwrap_or(false)
            })
        })
        .ok_or(PlaybackError::TrackUnavailable(track_name))
}

pub(super) fn stream_codec(stream: &serde_json::Value) -> Option<String> {
    stream
        .get("codec")
        .or_else(|| stream.get("codec_name"))
        .and_then(serde_json::Value::as_str)
        .map(String::from)
}

pub(super) fn extract_video_bit_depth(streams: &serde_json::Value) -> u32 {
    streams
        .get("video")
        .and_then(|v| v.get("bit_depth"))
        .and_then(|b| b.as_u64())
        .map(|b| b as u32)
        .unwrap_or(0)
}

pub(super) fn extract_video_profile(streams: &serde_json::Value) -> Option<String> {
    streams
        .get("video")
        .and_then(|v| v.get("profile"))
        .and_then(|p| p.as_str())
        .map(String::from)
}

pub(super) fn extract_video_level(streams: &serde_json::Value) -> Option<f32> {
    streams
        .get("video")
        .and_then(|v| v.get("level"))
        .and_then(|l| l.as_f64())
        .map(|l| l as f32)
}
