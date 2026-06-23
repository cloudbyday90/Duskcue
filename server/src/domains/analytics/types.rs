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

pub const VALID_TIME_PRESETS: &[&str] = &["24h", "7d", "30d", "90d", "all"];
pub const VALID_STREAM_DECISIONS: &[&str] = &["direct_play", "direct_stream", "transcode"];
pub const VALID_SEVERITIES: &[&str] = &["low", "medium", "high"];

pub struct PlaySessionRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub media_item_id: Uuid,
    pub library_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub duration_seconds: i32,
    pub ip_address: Option<String>,
    pub location_type: Option<String>,
    pub geo_city: Option<String>,
    pub geo_region: Option<String>,
    pub geo_country: Option<String>,
    pub client_name: String,
    pub client_device: Option<String>,
    pub stream_decision: String,
    pub percent_complete: Option<f64>,
    pub bandwidth_bps: Option<i64>,
}

pub struct TrustEventRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub play_session_id: Option<Uuid>,
    pub rule_type: String,
    pub severity: String,
    pub score_impact: i32,
    pub details: serde_json::Value,
    pub acknowledged: bool,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub struct TrustScoreRow {
    pub user_id: Uuid,
    pub score: i32,
    pub total_violations: i32,
    pub last_violation_at: Option<DateTime<Utc>>,
    pub last_good_session_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnalyticsQuery {
    pub range: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub user_id: Option<Uuid>,
    pub library_id: Option<Uuid>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayHistoryQuery {
    pub range: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub user_id: Option<Uuid>,
    pub library_id: Option<Uuid>,
    pub stream_decision: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TopMediaQuery {
    pub range: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub library_id: Option<Uuid>,
    pub sort_by: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrustEventQuery {
    pub user_id: Option<Uuid>,
    pub severity: Option<String>,
    pub acknowledged: Option<bool>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalyticsOverviewResponse {
    pub total_plays: i64,
    pub unique_users: i64,
    pub total_watch_time_seconds: i64,
    pub concurrent_streams: i64,
    pub direct_play_count: i64,
    pub direct_stream_count: i64,
    pub transcode_count: i64,
    pub range_start: Option<DateTime<Utc>>,
    pub range_end: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaySessionResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub user_display_name: String,
    pub media_item_id: Uuid,
    pub media_title: String,
    pub media_type: String,
    pub library_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub duration_seconds: i32,
    pub location_type: Option<String>,
    pub geo_city: Option<String>,
    pub geo_country: Option<String>,
    pub client_name: String,
    pub client_device: Option<String>,
    pub stream_decision: String,
    pub percent_complete: Option<f64>,
    pub bandwidth_bps: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayHistoryResponse {
    pub items: Vec<PlaySessionResponse>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopMediaItem {
    pub media_item_id: Uuid,
    pub title: String,
    pub media_type: String,
    pub library_id: Uuid,
    pub play_count: i64,
    pub total_watch_time_seconds: i64,
    pub unique_users: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopMediaResponse {
    pub items: Vec<TopMediaItem>,
    pub sort_by: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BandwidthPoint {
    pub timestamp: DateTime<Utc>,
    pub bandwidth_bps: i64,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BandwidthResponse {
    pub points: Vec<BandwidthPoint>,
    pub peak_bandwidth_bps: i64,
    pub average_bandwidth_bps: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConcurrentStreamInfo {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub user_display_name: String,
    pub media_item_id: Uuid,
    pub media_title: String,
    pub started_at: DateTime<Utc>,
    pub stream_decision: String,
    pub client_name: String,
    pub client_device: Option<String>,
    pub bandwidth_bps: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConcurrentStreamsResponse {
    pub count: i64,
    pub streams: Vec<ConcurrentStreamInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrustScoreResponse {
    pub user_id: Uuid,
    pub user_display_name: String,
    pub score: i32,
    pub total_violations: i32,
    pub last_violation_at: Option<DateTime<Utc>>,
    pub last_good_session_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrustScoreListResponse {
    pub items: Vec<TrustScoreResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrustEventResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub user_display_name: String,
    pub play_session_id: Option<Uuid>,
    pub rule_type: String,
    pub severity: String,
    pub score_impact: i32,
    pub details: serde_json::Value,
    pub acknowledged: bool,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrustEventListResponse {
    pub items: Vec<TrustEventResponse>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GeoIpStatusResponse {
    pub enabled: bool,
    pub database_present: bool,
    pub database_path: Option<String>,
    pub database_age_days: Option<i64>,
    pub database_size_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcknowledgeEventResponse {
    pub event_id: Uuid,
    pub acknowledged: bool,
    pub acknowledged_at: DateTime<Utc>,
}
