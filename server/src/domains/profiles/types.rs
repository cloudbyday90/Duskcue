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

pub const PROFILE_TYPES: &[&str] = &["standard", "kids"];
pub const CONTENT_RATINGS: &[&str] = &[
    "TV-Y", "TV-Y7", "G", "TV-G", "PG", "TV-PG", "PG-13", "TV-14", "R", "TV-MA", "NC-17",
];
pub const CHANNEL_AUDIENCES: &[&str] = &["standard", "kids"];

pub struct ProfileRow {
    pub id: Uuid,
    pub owner_user_id: Uuid,
    pub name: String,
    pub avatar: Option<String>,
    pub profile_type: String,
    pub is_default: bool,
    pub max_content_rating: String,
    pub allow_search: bool,
    pub allow_downloads: bool,
    pub allow_external_links: bool,
    pub allow_ambient_channels: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct AmbientChannelRow {
    pub id: Uuid,
    pub owner_user_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub audience: String,
    pub is_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateProfileRequest {
    #[validate(length(min = 1, max = 80))]
    pub name: String,
    #[validate(length(max = 500))]
    pub avatar: Option<String>,
    pub profile_type: Option<String>,
    pub max_content_rating: Option<String>,
    pub library_ids: Option<Vec<Uuid>>,
    pub allow_search: Option<bool>,
    pub allow_downloads: Option<bool>,
    pub allow_external_links: Option<bool>,
    pub allow_ambient_channels: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateProfileRequest {
    #[validate(length(min = 1, max = 80))]
    pub name: Option<String>,
    #[validate(length(max = 500))]
    pub avatar: Option<String>,
    pub max_content_rating: Option<String>,
    pub library_ids: Option<Vec<Uuid>>,
    pub allow_search: Option<bool>,
    pub allow_downloads: Option<bool>,
    pub allow_external_links: Option<bool>,
    pub allow_ambient_channels: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileResponse {
    pub id: Uuid,
    pub name: String,
    pub avatar: Option<String>,
    pub profile_type: String,
    pub is_default: bool,
    pub max_content_rating: String,
    pub library_ids: Vec<Uuid>,
    pub allow_search: bool,
    pub allow_downloads: bool,
    pub allow_external_links: bool,
    pub allow_ambient_channels: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileListResponse {
    pub active_profile_id: Uuid,
    pub remembered_profile_id: Option<Uuid>,
    pub device_can_remember_profile: bool,
    pub items: Vec<ProfileResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SwitchProfileRequest {
    pub remember_on_device: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwitchProfileResponse {
    pub active_profile: ProfileResponse,
    pub remembered_profile_id: Option<Uuid>,
    pub device_can_remember_profile: bool,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateAmbientChannelRequest {
    #[validate(length(min = 1, max = 120))]
    pub name: String,
    #[validate(length(max = 2_000))]
    pub description: Option<String>,
    pub audience: String,
    pub is_enabled: Option<bool>,
    pub media_item_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateAmbientChannelRequest {
    #[validate(length(min = 1, max = 120))]
    pub name: Option<String>,
    #[validate(length(max = 2_000))]
    pub description: Option<String>,
    pub audience: Option<String>,
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReplaceAmbientChannelItemsRequest {
    pub media_item_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AmbientChannelNextRequest {
    pub after_media_item_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmbientChannelResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub audience: String,
    pub is_enabled: bool,
    pub item_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmbientChannelListResponse {
    pub items: Vec<AmbientChannelResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmbientChannelItemsResponse {
    pub channel_id: Uuid,
    pub media_item_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmbientChannelNextResponse {
    pub channel_id: Uuid,
    pub channel_name: String,
    pub media_item_id: Uuid,
    pub playback_mode: String,
}

#[derive(Debug, Clone)]
pub struct ProfileScope {
    pub profile_id: Uuid,
    pub owner_user_id: Uuid,
    pub profile_type: String,
    pub max_content_rating: String,
    pub allow_search: bool,
    pub allow_downloads: bool,
    pub allow_external_links: bool,
    pub allow_ambient_channels: bool,
    pub library_ids: Vec<Uuid>,
    pub user_library_ids: Vec<Uuid>,
    pub has_all_library_access: bool,
}
