// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::time::UNIX_EPOCH;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::media_source::{MediaFileDetails, SelectedTracks};
use super::types::DirectFallbackResponse;
use crate::services::transcoding::output_profile::*;

pub(super) const VIDEO_CONTENT_TYPE: &str = "video/mp4; codecs=\"avc1.640028\"";
pub(super) const AUDIO_CONTENT_TYPE: &str = "audio/mp4; codecs=\"mp4a.40.2\"";

pub(super) fn supports_protocol(version: Option<u8>) -> bool {
    version == Some(1)
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct FileIdentity {
    pub file_id: Uuid,
    pub path: String,
    pub bytes: u64,
    pub updated_at: DateTime<Utc>,
    pub modified_ns: String,
    pub probe_hash: String,
}

impl FileIdentity {
    pub(super) async fn read_owned(
        owner: &std::sync::Arc<crate::services::vod_lifecycle::VodLifecycle>,
        details: &MediaFileDetails,
    ) -> Option<Self> {
        let output = owner.clone();
        let details = details.clone();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            owner.command(false, move |stop| async move {
                let _permit = output.inspection_permit(&stop).await?;
                Self::inspect_strict(&details)
                    .await
                    .ok_or(super::PlaybackError::FileNotFound)
            }),
        )
        .await
        .ok()?
        .ok()
    }
    pub(super) async fn read_scoped(
        owner: &std::sync::Arc<crate::services::vod_lifecycle::VodLifecycle>,
        group: &std::sync::Arc<crate::services::vod_lifecycle::Generation>,
        details: &MediaFileDetails,
    ) -> Option<Self> {
        let output = owner.clone();
        let details = details.clone();
        let stop = group.stop.clone();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            group.inspect(async move {
                let _permit = output.inspection_permit(&stop).await?;
                Self::inspect_strict(&details)
                    .await
                    .ok_or(super::PlaybackError::FileNotFound)
            }),
        )
        .await
        .ok()
        .and_then(Result::ok);
        if result.is_none() {
            group.retire();
        }
        result
    }
    async fn inspect_strict(details: &MediaFileDetails) -> Option<Self> {
        let metadata = tokio::fs::symlink_metadata(&details.file_path).await.ok()?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return None;
        }
        Self::inspect(details).await
    }
    pub(super) fn matches_details(&self, details: &MediaFileDetails) -> bool {
        self.file_id == details.id
            && self.path == details.file_path
            && self.bytes == u64::try_from(details.file_size).unwrap_or(u64::MAX)
            && self.updated_at == details.updated_at
            && Self::probe_hash(details).as_deref() == Some(self.probe_hash.as_str())
    }
    pub(super) fn matches_metadata(&self, metadata: &std::fs::Metadata) -> bool {
        metadata.is_file()
            && metadata.len() == self.bytes
            && metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .is_some_and(|value| value.as_nanos().to_string() == self.modified_ns)
    }
    pub(super) async fn read(details: &MediaFileDetails) -> Option<Self> {
        tokio::time::timeout(std::time::Duration::from_secs(2), Self::inspect(details))
            .await
            .ok()
            .flatten()
    }

    async fn inspect(details: &MediaFileDetails) -> Option<Self> {
        if details.file_path.is_empty() || details.file_path.len() > 4096 {
            return None;
        }
        let metadata = tokio::fs::metadata(&details.file_path).await.ok()?;
        if !metadata.is_file()
            || metadata.len() == 0
            || u64::try_from(details.file_size).ok()? != metadata.len()
        {
            return None;
        }
        let probe_hash = Self::probe_hash(details)?;
        Some(Self {
            file_id: details.id,
            path: details.file_path.clone(),
            bytes: metadata.len(),
            updated_at: details.updated_at,
            modified_ns: metadata
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_nanos()
                .to_string(),
            probe_hash,
        })
    }
    fn probe_hash(details: &MediaFileDetails) -> Option<String> {
        let probe = serde_json::to_vec(&json!({
            "container": details.container_format, "video_codec": details.video_codec,
            "resolution": details.video_resolution, "video_bitrate": details.video_bitrate,
            "dynamic_range": details.video_dynamic_range, "audio_codec": details.audio_codec,
            "audio_channels": details.audio_channels, "audio_bitrate": details.audio_bitrate,
            "streams": details.additional_streams,
        }))
        .ok()?;
        if probe.len() > 65_536 {
            return None;
        }
        Some(
            ring::digest::digest(&ring::digest::SHA256, &probe)
                .as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        )
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct ResolvedTracks {
    pub video: i32,
    pub audio: i32,
    pub subtitle: Option<i32>,
    pub subtitle_ordinal: Option<usize>,
}

impl ResolvedTracks {
    pub(super) fn from_selected(selected: &SelectedTracks) -> Option<Self> {
        Some(Self {
            video: selected.video_stream_index.filter(|index| *index >= 0)?,
            audio: selected.audio_stream_index.filter(|index| *index >= 0)?,
            subtitle: selected.subtitle_stream_index,
            subtitle_ordinal: selected.subtitle_stream_ordinal,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct AdmissionLimits {
    pub max_resolution: (u32, u32),
    pub max_audio_channels: u32,
    pub max_bitrate: u64,
    pub quality_mode: String,
    pub requested_bitrate: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct PreparedFallback {
    pub identity: FileIdentity,
    pub tracks: ResolvedTracks,
    pub limits: AdmissionLimits,
    pub descriptor: DirectFallbackResponse,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct DirectContext {
    pub version: u8,
    pub candidate: Option<PreparedFallback>,
}

pub(super) fn attach(metadata: &mut Value, context: &DirectContext, revision: Uuid) {
    metadata["direct_source_revision"] = json!(revision);
    metadata["direct_source_context"] = json!(context);
}

pub(super) fn has_context(metadata: &Value) -> bool {
    metadata.get("direct_source_context").is_some()
}

pub(super) fn context(metadata: &Value) -> Option<DirectContext> {
    let value = metadata.get("direct_source_context")?;
    if serde_json::to_vec(value).ok()?.len() > 16_384 {
        return None;
    }
    let context: DirectContext = serde_json::from_value(value.clone()).ok()?;
    (context.version == 1).then_some(context)
}

pub(super) fn revision(metadata: &Value) -> Option<Uuid> {
    if super::recovery::transcode_id(metadata).is_some() {
        return None;
    }
    metadata
        .get("direct_source_revision")?
        .as_str()?
        .parse()
        .ok()
}

pub(super) fn current_revision(metadata: &Value, requested: Option<Uuid>) -> bool {
    revision(metadata).is_some_and(|revision| requested == Some(revision))
}

pub(super) fn rotate(metadata: &mut Value) -> Option<Uuid> {
    if !has_context(metadata) {
        return None;
    }
    let revision = Uuid::now_v7();
    metadata["direct_source_revision"] = json!(revision);
    Some(revision)
}

pub(super) fn descriptor(metadata: &Value) -> Option<DirectFallbackResponse> {
    revision(metadata)?;
    let descriptor = context(metadata)?.candidate?.descriptor;
    descriptor_valid(&descriptor).then_some(descriptor)
}

pub(super) fn descriptor_valid(descriptor: &DirectFallbackResponse) -> bool {
    let video = &descriptor.video;
    let audio = &descriptor.audio;
    video.content_type == VIDEO_CONTENT_TYPE
        && audio.content_type == AUDIO_CONTENT_TYPE
        && (2..=MAX_WIDTH).contains(&video.width)
        && video.width.is_multiple_of(2)
        && (2..=MAX_HEIGHT).contains(&video.height)
        && video.height.is_multiple_of(2)
        && (VIDEO_MIN_BITRATE..=VIDEO_MAX_BITRATE).contains(&video.bitrate)
        && video.framerate == FRAME_RATE
        && matches!(audio.channels.as_str(), "1" | "2")
        && (AUDIO_MIN_BITRATE..=AUDIO_MAX_BITRATE).contains(&audio.bitrate)
        && audio.samplerate == AUDIO_SAMPLE_RATE
}

#[cfg(test)]
#[path = "direct_tests.rs"]
mod tests;
