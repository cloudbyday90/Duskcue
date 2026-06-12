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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

pub static VALID_PROFILE_SOURCES: &[&str] = &["client_report", "capability_wizard", "known_device", "manual"];
pub static VALID_NETWORK_TIERS: &[&str] = &["excellent", "good", "moderate", "slow", "very_slow", "critical"];
pub static VALID_REPORT_TYPES: &[&str] = &["segment", "probe"];
pub static VALID_WIZARD_RESULTS: &[&str] = &["success", "failed", "stuttered"];
pub static VALID_QUALITY_MODES: &[&str] = &["auto", "maximum", "manual"];

pub struct DeviceProfileRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub device_identifier: String,
    pub platform: String,
    pub model: Option<String>,
    pub os_version: Option<String>,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub video_codecs: serde_json::Value,
    pub audio_codecs: serde_json::Value,
    pub subtitle_formats: serde_json::Value,
    pub containers: serde_json::Value,
    pub max_resolution: Option<String>,
    pub max_framerate: Option<i32>,
    pub hdr_support: serde_json::Value,
    pub max_audio_channels: Option<i32>,
    pub spatial_audio: bool,
    pub max_bitrate_bps: Option<i64>,
    pub allow_client_side_dv_fallback: bool,
    pub profile_source: String,
    pub wizard_completed_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

pub struct DeviceCapabilityTestRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub device_profile_id: Uuid,
    pub test_format: String,
    pub test_description: String,
    pub result: String,
    pub actual_codec: Option<String>,
    pub actual_resolution: Option<String>,
    pub actual_bit_depth: Option<i32>,
    pub actual_dynamic_range: Option<String>,
    pub details: serde_json::Value,
}

pub struct ClientNetworkReportRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub report_type: String,
    pub segment_index: Option<i32>,
    pub rung: Option<String>,
    pub payload_bytes: Option<i64>,
    pub download_start_ms: Option<i64>,
    pub download_end_ms: Option<i64>,
    pub throughput_bps: Option<i64>,
    pub buffer_seconds: Option<f32>,
    pub rebuffer_count: Option<i32>,
    pub rebuffer_total_ms: Option<i32>,
    pub estimated_throughput_bps: Option<i64>,
    pub network_tier: Option<String>,
    pub metadata: serde_json::Value,
}

pub struct QoeReportRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub report_interval_seconds: i32,
    pub startup_time_ms: Option<i32>,
    pub rebuffer_ratio: Option<f32>,
    pub average_bitrate_bps: Option<i64>,
    pub switches_per_minute: Option<f32>,
    pub quality_drops: Option<i32>,
    pub current_rung: Option<String>,
    pub current_buffer_seconds: Option<f32>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ReportCapabilitiesRequest {
    #[validate(length(min = 1, max = 255))]
    pub device_identifier: String,
    #[validate(length(min = 1, max = 100))]
    pub platform: String,
    pub model: Option<String>,
    pub os_version: Option<String>,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub video_codecs: Option<serde_json::Value>,
    pub audio_codecs: Option<serde_json::Value>,
    pub subtitle_formats: Option<serde_json::Value>,
    pub containers: Option<serde_json::Value>,
    pub max_resolution: Option<String>,
    pub max_framerate: Option<i32>,
    pub hdr_support: Option<serde_json::Value>,
    pub max_audio_channels: Option<i32>,
    pub spatial_audio: Option<bool>,
    pub max_bitrate_bps: Option<i64>,
    pub allow_client_side_dv_fallback: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct StartWizardRequest {
    #[validate(length(min = 1, max = 255))]
    pub device_identifier: String,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct WizardTestResultRequest {
    #[validate(length(min = 1, max = 100))]
    pub test_format: String,
    #[validate(length(min = 1))]
    pub result: String,
    pub actual_codec: Option<String>,
    pub actual_resolution: Option<String>,
    pub actual_bit_depth: Option<i32>,
    pub actual_dynamic_range: Option<String>,
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SegmentTelemetryRequest {
    #[validate(required)]
    pub session_id: Option<Uuid>,
    pub segment_index: Option<i32>,
    pub rung: Option<String>,
    pub segment_bytes: Option<i64>,
    pub download_start_ms: Option<i64>,
    pub download_end_ms: Option<i64>,
    pub buffer_seconds: Option<f32>,
    pub rebuffer_count: Option<i32>,
    pub rebuffer_total_ms: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct BandwidthProbeResultRequest {
    #[validate(required)]
    pub session_id: Option<Uuid>,
    #[validate(range(min = 1))]
    pub probe_bytes: Option<i64>,
    #[validate(range(min = 1))]
    pub download_ms: Option<i64>,
    pub estimated_throughput_bps: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct QoeReportRequest {
    #[validate(required)]
    pub session_id: Option<Uuid>,
    pub startup_time_ms: Option<i32>,
    pub rebuffer_ratio: Option<f32>,
    pub average_bitrate_bps: Option<i64>,
    pub switches_per_minute: Option<f32>,
    pub quality_drops: Option<i32>,
    pub current_rung: Option<String>,
    pub current_buffer_seconds: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceProfileResponse {
    pub id: Uuid,
    pub device_identifier: String,
    pub platform: String,
    pub model: Option<String>,
    pub os_version: Option<String>,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub video_codecs: serde_json::Value,
    pub audio_codecs: serde_json::Value,
    pub subtitle_formats: serde_json::Value,
    pub containers: serde_json::Value,
    pub max_resolution: Option<String>,
    pub max_framerate: Option<i32>,
    pub hdr_support: serde_json::Value,
    pub max_audio_channels: Option<i32>,
    pub spatial_audio: bool,
    pub max_bitrate_bps: Option<i64>,
    pub allow_client_side_dv_fallback: bool,
    pub profile_source: String,
    pub wizard_completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityTestResponse {
    pub id: Uuid,
    pub test_format: String,
    pub test_description: String,
    pub result: String,
    pub actual_codec: Option<String>,
    pub actual_resolution: Option<String>,
    pub actual_bit_depth: Option<i32>,
    pub actual_dynamic_range: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityTestListResponse {
    pub items: Vec<CapabilityTestResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WizardStartResponse {
    pub profile_id: Uuid,
    pub tests: Vec<CapabilityTestResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkQualitySummary {
    pub user_id: Uuid,
    pub latest_tier: Option<String>,
    pub latest_throughput_bps: Option<i64>,
    pub sample_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceCapabilitySummary {
    pub platform: String,
    pub device_count: i64,
    pub wizard_completion_rate: f64,
    pub top_video_codecs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QoeSummary {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub startup_time_ms: Option<i32>,
    pub rebuffer_ratio: Option<f32>,
    pub average_bitrate_bps: Option<i64>,
    pub switches_per_minute: Option<f32>,
    pub quality_drops: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscodeBreakdown {
    pub direct_play_count: i64,
    pub direct_stream_count: i64,
    pub transcode_count: i64,
    pub total_sessions: i64,
    pub direct_play_percentage: f64,
}
