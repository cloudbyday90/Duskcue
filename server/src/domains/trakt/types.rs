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

pub struct TraktAccountRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub trakt_username: String,
    pub trakt_user_id: i64,
    pub access_token: String,
    pub refresh_token: String,
    pub token_expires_at: DateTime<Utc>,
    pub token_scope: Option<String>,
    pub last_full_sync_at: Option<DateTime<Utc>>,
    pub sync_enabled: bool,
    pub sync_watched: bool,
    pub sync_watchlist: bool,
    pub sync_collection: bool,
    pub sync_ratings: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct TraktSyncStateRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub media_item_id: Uuid,
    pub trakt_id: i64,
    pub trakt_history_id: Option<i64>,
    pub is_watched: bool,
    pub watched_at: Option<DateTime<Utc>>,
    pub plays: i32,
    pub is_in_watchlist: bool,
    pub watchlist_added_at: Option<DateTime<Utc>>,
    pub is_in_collection: bool,
    pub collected_at: Option<DateTime<Utc>>,
    pub rating: Option<i32>,
    pub rated_at: Option<DateTime<Utc>>,
    pub sync_error: Option<String>,
    pub synced_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct PollDeviceCodeRequest {
    #[validate(length(min = 1, max = 512))]
    pub device_code: String,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateSyncSettingsRequest {
    pub sync_enabled: Option<bool>,
    pub sync_watched: Option<bool>,
    pub sync_watchlist: Option<bool>,
    pub sync_collection: Option<bool>,
    pub sync_ratings: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraktAccountResponse {
    pub linked: bool,
    pub trakt_username: Option<String>,
    pub trakt_user_id: Option<i64>,
    pub token_expires_at: Option<DateTime<Utc>>,
    pub sync_enabled: bool,
    pub sync_watched: bool,
    pub sync_watchlist: bool,
    pub sync_collection: bool,
    pub sync_ratings: bool,
    pub last_full_sync_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_url: String,
    pub verification_url_complete: Option<String>,
    pub expires_in: i64,
    pub interval: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncSettingsResponse {
    pub sync_enabled: bool,
    pub sync_watched: bool,
    pub sync_watchlist: bool,
    pub sync_collection: bool,
    pub sync_ratings: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncTriggerResponse {
    pub queued: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncSummary {
    pub completed: bool,
    pub pulled_watched: i64,
    pub pulled_ratings: i64,
    pub pulled_collection: i64,
    pub pushed_watched: i64,
    pub unmatched: i64,
    pub last_full_sync_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncStatusResponse {
    pub last_full_sync_at: Option<DateTime<Utc>>,
    pub total_items: i64,
    pub watched_count: i64,
    pub watchlist_count: i64,
    pub collection_count: i64,
    pub rated_count: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraktHistoryItem {
    pub media_item_id: Uuid,
    pub trakt_id: i64,
    pub is_watched: bool,
    pub watched_at: Option<DateTime<Utc>>,
    pub plays: i32,
    pub is_in_watchlist: bool,
    pub is_in_collection: bool,
    pub rating: Option<i32>,
    pub rated_at: Option<DateTime<Utc>>,
    pub synced_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraktHistoryResponse {
    pub items: Vec<TraktHistoryItem>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateTraktSettingsRequest {
    #[validate(length(max = 256))]
    pub client_id: Option<String>,
    #[validate(length(max = 512))]
    pub client_secret: Option<String>,
    #[validate(length(max = 512))]
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraktSettingsResponse {
    pub client_id: String,
    pub client_secret_masked: String,
    pub has_client_secret: bool,
    pub redirect_uri: String,
    pub is_configured: bool,
}
