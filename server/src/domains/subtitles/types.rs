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

pub static VALID_SUBTITLE_TYPES: &[&str] = &["embedded", "external", "fetched"];
pub static VALID_SUBTITLE_FORMATS: &[&str] =
    &["srt", "ass", "ssa", "vtt", "sup", "sub", "idx", "ttml"];
pub static VALID_OCR_ENGINES: &[&str] = &["paddleocr", "tesseract"];
pub static VALID_SYNC_METHODS: &[&str] = &["voice_activity", "fps_adjust", "manual"];
pub static VALID_DELIVERY_FORMATS: &[&str] = &["srt", "vtt"];
pub static VALID_SUBTITLE_PROVIDERS: &[&str] = &["subdl", "opensubtitles"];
pub static VALID_SUBTITLE_MODES: &[&str] = &["default", "always", "none", "forced_only"];

pub struct SubtitleFileRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub media_item_id: Uuid,
    pub file_path: String,
    pub language: String,
    pub subtitle_type: String,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
    pub source_provider: Option<String>,
}

pub struct SubtitleOcrCacheRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub media_item_id: Uuid,
    pub subtitle_stream_index: i32,
    pub source_hash: String,
    pub ocr_engine: String,
    pub confidence_score: Option<f64>,
    pub srt_content: String,
}

pub struct SubtitleSyncDataRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub media_item_id: Uuid,
    pub subtitle_file_id: Uuid,
    pub sync_method: String,
    pub offset_ms: i32,
    pub confidence: Option<f64>,
    pub fps_source: Option<f64>,
    pub fps_target: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct FetchSubtitlesRequest {
    #[validate(length(min = 2, max = 10))]
    pub language: String,
    pub provider: Option<String>,
    pub is_forced: Option<bool>,
    pub is_hearing_impaired: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SetSubtitleOffsetRequest {
    #[validate(range(min = -300000, max = 300000))]
    pub offset_ms: i32,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct TriggerOcrRequest {
    pub engine: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SubtitleContentQuery {
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubtitleFileResponse {
    pub id: Uuid,
    pub media_item_id: Uuid,
    pub file_path: String,
    pub language: String,
    pub subtitle_type: String,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
    pub source_provider: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubtitleListResponse {
    pub items: Vec<SubtitleFileResponse>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FetchSubtitlesResponse {
    pub fetched: Vec<SubtitleFileResponse>,
    pub provider_used: Option<String>,
    pub no_results: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubtitleOffsetResponse {
    pub subtitle_id: Uuid,
    pub offset_ms: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubtitleOcrResult {
    pub subtitle_file_id: Uuid,
    pub ocr_engine: String,
    pub confidence_score: Option<f64>,
    pub srt_content_length: usize,
    pub below_threshold: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubtitleSyncDataResponse {
    pub subtitle_file_id: Uuid,
    pub sync_method: String,
    pub offset_ms: i32,
    pub confidence: Option<f64>,
    pub fps_source: Option<f64>,
    pub fps_target: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateSubtitleSettingsRequest {
    pub ocr_enabled: bool,
    #[validate(length(min = 1, max = 32))]
    pub ocr_engine: String,
    #[validate(range(min = 0.0, max = 1.0))]
    pub ocr_confidence_threshold: f64,
    pub voice_activity_analysis: bool,
    #[validate(length(min = 1, max = 64))]
    pub voice_activity_schedule: String,
    #[validate(length(min = 1, max = 32))]
    pub default_subtitle_mode: String,
    #[validate(length(min = 2, max = 10))]
    pub default_subtitle_language: String,
    pub auto_fetch_enabled: bool,
    pub auto_fetch_languages: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateSubtitleProviderSettingsRequest {
    #[validate(nested)]
    pub subdl: Option<SubdlProviderUpdate>,
    #[validate(nested)]
    pub opensubtitles: Option<OpensubtitlesProviderUpdate>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SubdlProviderUpdate {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub auto_fetch_enabled: bool,
    pub auto_fetch_languages: Vec<String>,
    pub prefer_hearing_impaired: bool,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct OpensubtitlesProviderUpdate {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub api_token: Option<String>,
    pub auto_fetch_enabled: bool,
    pub auto_fetch_languages: Vec<String>,
    pub prefer_hearing_impaired: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubtitleSettingsResponse {
    pub ocr_enabled: bool,
    pub ocr_engine: String,
    pub ocr_confidence_threshold: f64,
    pub voice_activity_analysis: bool,
    pub voice_activity_schedule: String,
    pub default_subtitle_mode: String,
    pub default_subtitle_language: String,
    pub auto_fetch_enabled: bool,
    pub auto_fetch_languages: Vec<String>,
    pub providers: SubtitleProvidersResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubtitleProvidersResponse {
    pub subdl: SubdlProviderResponse,
    pub opensubtitles: OpensubtitlesProviderResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubdlProviderResponse {
    pub enabled: bool,
    pub api_key_masked: String,
    pub has_api_key: bool,
    pub auto_fetch_enabled: bool,
    pub auto_fetch_languages: Vec<String>,
    pub prefer_hearing_impaired: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpensubtitlesProviderResponse {
    pub enabled: bool,
    pub api_key_masked: String,
    pub has_api_key: bool,
    pub api_token_masked: String,
    pub has_api_token: bool,
    pub auto_fetch_enabled: bool,
    pub auto_fetch_languages: Vec<String>,
    pub prefer_hearing_impaired: bool,
}
