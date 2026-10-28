// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::error::CommandError;
use crate::domains::playback::types::StartPlaybackRequest;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const BODY_LIMIT: usize = 32768;
pub const PROTOCOL: &str = "duskcue-vod-command/2";
pub const LEGACY_PROTOCOL: &str = "duskcue-vod-command/1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartRequest {
    pub protocol: String,
    pub request_id: Uuid,
    pub selection: serde_json::Value,
}

pub(super) struct Intent {
    pub id: Uuid,
    pub selection: StartPlaybackRequest,
    pub hash: [u8; 32],
}

impl StartRequest {
    pub(super) fn intent(mut self) -> Result<Intent, CommandError> {
        use validator::Validate;
        const FIELDS: &[&str] = &[
            "media_item_id",
            "media_file_id",
            "audio_stream_index",
            "subtitle_stream_index",
            "max_streaming_bitrate",
            "force_transcode",
            "force_hls",
            "direct_recovery_protocol",
            "device_profile",
            "source_capabilities",
            "quality_mode",
            "playback_mode",
            "ambient_channel_id",
            "ambient_channel_updated_at",
        ];
        if self.protocol != PROTOCOL || self.request_id.get_version_num() != 7 {
            return Err(CommandError::Invalid);
        }
        let object = self
            .selection
            .as_object_mut()
            .ok_or(CommandError::Invalid)?;
        if object.keys().any(|key| !FIELDS.contains(&key.as_str())) {
            return Err(CommandError::Invalid);
        }
        for field in FIELDS {
            object.entry(*field).or_insert(serde_json::Value::Null);
        }
        for field in ["force_transcode", "force_hls"] {
            if object[field].is_null() {
                object.insert(field.into(), false.into());
            }
        }
        if object["playback_mode"].is_null() {
            object.insert("playback_mode".into(), "interactive".into());
        }
        if object["direct_recovery_protocol"] == serde_json::json!(0) {
            object.insert("direct_recovery_protocol".into(), serde_json::Value::Null);
        }
        let selection: StartPlaybackRequest =
            serde_json::from_value(self.selection.clone()).map_err(|_| CommandError::Invalid)?;
        selection.validate().map_err(|_| CommandError::Invalid)?;
        if selection.media_item_id.is_none()
            || selection.audio_stream_index.is_some_and(|n| n < 0)
            || selection.subtitle_stream_index.is_some_and(|n| n < 0)
            || selection
                .max_streaming_bitrate
                .is_some_and(|n| n == 0 || n > u64::from(u32::MAX))
            || selection.direct_recovery_protocol.is_some_and(|v| v > 1)
            || selection
                .quality_mode
                .as_deref()
                .is_some_and(|v| !matches!(v, "auto" | "maximum" | "manual"))
            || selection
                .source_capabilities
                .as_ref()
                .is_some_and(|v| v.version != 1)
        {
            return Err(CommandError::Invalid);
        }
        let canonical = canonical(self.selection, 0, &mut 0)?;
        let bytes =
            serde_json::to_vec(&(PROTOCOL, canonical)).map_err(|_| CommandError::Invalid)?;
        if bytes.len() > BODY_LIMIT {
            return Err(CommandError::Invalid);
        }
        let hash = ring::digest::digest(&ring::digest::SHA256, &bytes)
            .as_ref()
            .try_into()
            .map_err(|_| CommandError::Invalid)?;
        Ok(Intent {
            id: self.request_id,
            selection,
            hash,
        })
    }
}

pub(super) fn canonical(
    value: serde_json::Value,
    depth: usize,
    nodes: &mut usize,
) -> Result<serde_json::Value, CommandError> {
    *nodes += 1;
    if depth > 16 || *nodes > 4096 {
        return Err(CommandError::Invalid);
    }
    Ok(match value {
        serde_json::Value::Object(object) => {
            let mut ordered = std::collections::BTreeMap::new();
            for (key, value) in object {
                ordered.insert(key, canonical(value, depth + 1, nodes)?);
            }
            serde_json::to_value(ordered).map_err(|_| CommandError::Invalid)?
        }
        serde_json::Value::Array(values) => serde_json::Value::Array(
            values
                .into_iter()
                .map(|value| canonical(value, depth + 1, nodes))
                .collect::<Result<_, _>>()?,
        ),
        value => value,
    })
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadyResponse {
    #[serde(default)]
    pub output_revision: Option<Uuid>,
    #[serde(default)]
    pub position_ms: Option<i32>,
    #[serde(default)]
    pub recovery: Option<serde_json::Value>,
    pub session_id: Uuid,
    pub media_item_id: Uuid,
    pub media_file_id: Uuid,
    pub delivery_mode: String,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub playback_mode: String,
    pub stream_offset_ms: Option<i64>,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub protocol: &'static str,
    pub request_id: Uuid,
    pub status: &'static str,
    pub status_url: String,
    pub observed_at: DateTime<Utc>,
    pub lease_deadline: Option<DateTime<Utc>>,
    pub stream_url: Option<String>,
    pub ready: Option<ReadyResponse>,
}

impl StatusResponse {
    pub(super) fn receipt(
        id: Uuid,
        status: &'static str,
        observed_at: DateTime<Utc>,
        lease_deadline: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            protocol: PROTOCOL,
            request_id: id,
            status,
            status_url: format!("/api/v1/playback-commands/{id}"),
            observed_at,
            lease_deadline,
            stream_url: None,
            ready: None,
        }
    }
}

#[derive(Serialize)]
pub struct RenewalResponse {
    pub protocol: &'static str,
    pub request_id: Uuid,
    pub status: &'static str,
    pub status_url: String,
    pub observed_at: DateTime<Utc>,
    pub lease_deadline: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub(super) struct CommandRow {
    pub protocol: i16,
    pub fingerprint: String,
    pub instance_id: Uuid,
    pub reservation_state: String,
    pub deadline: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub intent_hash: Vec<u8>,
    pub state: String,
    pub history_id: Option<Uuid>,
    pub history_started_at: Option<DateTime<Utc>>,
    pub resources_completed: bool,
    pub cleanup_uncertain: bool,
}
