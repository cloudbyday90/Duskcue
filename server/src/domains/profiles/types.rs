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
    pub parent_pin_hash: Option<String>,
    pub parent_pin_failed_attempts: i16,
    pub parent_pin_locked_until: Option<DateTime<Utc>>,
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
    #[validate(length(min = 4, max = 12))]
    pub parent_pin: Option<String>,
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
    #[validate(length(min = 4, max = 12))]
    pub parent_pin: Option<String>,
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
    pub parent_pin_configured: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileListResponse {
    pub active_profile_id: Uuid,
    pub profile_selection_required: bool,
    pub remembered_profile_id: Option<Uuid>,
    pub device_can_remember_profile: bool,
    pub parent_unlock_required: bool,
    pub parent_unlock_expires_at: Option<DateTime<Utc>>,
    pub items: Vec<ProfileResponse>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleMode {
    None,
    Always,
}

#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ViewingPreferencesRequest {
    pub autoplay_next_episode: bool,
    pub audio_language: Option<String>,
    pub prefer_audio_description: bool,
    pub subtitle_mode: SubtitleMode,
    pub subtitle_language: Option<String>,
    pub prefer_sdh: bool,
}

#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UpdateViewingPreferencesRequest {
    pub expected_profile_id: Uuid,
    #[validate(nested)]
    pub viewing_preferences: ViewingPreferencesRequest,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ViewingPreferencesResponse {
    pub autoplay_next_episode: bool,
    pub audio_language: Option<String>,
    pub prefer_audio_description: bool,
    pub subtitle_mode: SubtitleMode,
    pub subtitle_language: Option<String>,
    pub prefer_sdh: bool,
}

impl Default for ViewingPreferencesResponse {
    fn default() -> Self {
        Self {
            autoplay_next_episode: true,
            audio_language: None,
            prefer_audio_description: false,
            subtitle_mode: SubtitleMode::None,
            subtitle_language: None,
            prefer_sdh: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentViewingPreferencesResponse {
    pub profile_id: Uuid,
    pub has_saved_preferences: bool,
    pub viewing_preferences: ViewingPreferencesResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SwitchProfileRequest {
    pub remember_on_device: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwitchProfileResponse {
    pub active_profile: ProfileResponse,
    pub profile_selection_required: bool,
    pub remembered_profile_id: Option<Uuid>,
    pub device_can_remember_profile: bool,
    pub parent_unlock_required: bool,
    pub parent_unlock_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ParentUnlockRequest {
    #[validate(length(min = 4, max = 12))]
    pub pin: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParentUnlockResponse {
    pub unlocked_until: DateTime<Utc>,
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
    pub channel_updated_at: DateTime<Utc>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn preference_payload() -> serde_json::Value {
        serde_json::json!({
            "expected_profile_id": Uuid::now_v7(),
            "viewing_preferences": {
                "autoplay_next_episode": false,
                "audio_language": null,
                "prefer_audio_description": false,
                "subtitle_mode": "none",
                "subtitle_language": null,
                "prefer_sdh": false
            }
        })
    }

    #[test]
    fn self_service_preferences_reject_parental_fields_at_every_level() {
        let mut outer = preference_payload();
        outer["allow_downloads"] = serde_json::json!(true);
        assert!(serde_json::from_value::<UpdateViewingPreferencesRequest>(outer).is_err());
        let mut nested = preference_payload();
        nested["viewing_preferences"]["max_content_rating"] = serde_json::json!("NC-17");
        assert!(serde_json::from_value::<UpdateViewingPreferencesRequest>(nested).is_err());
    }

    #[test]
    fn preference_save_rejects_wrong_types_unknown_modes_and_missing_booleans() {
        let mut wrong_type = preference_payload();
        wrong_type["viewing_preferences"]["autoplay_next_episode"] = serde_json::json!("false");
        assert!(serde_json::from_value::<UpdateViewingPreferencesRequest>(wrong_type).is_err());
        let mut unsupported_mode = preference_payload();
        unsupported_mode["viewing_preferences"]["subtitle_mode"] = serde_json::json!("forced_only");
        assert!(
            serde_json::from_value::<UpdateViewingPreferencesRequest>(unsupported_mode).is_err()
        );
        let mut missing = preference_payload();
        missing["viewing_preferences"]
            .as_object_mut()
            .unwrap()
            .remove("autoplay_next_episode");
        assert!(serde_json::from_value::<UpdateViewingPreferencesRequest>(missing).is_err());
    }

    #[test]
    fn nullable_language_values_are_distinct_from_stream_indices() {
        let request =
            serde_json::from_value::<UpdateViewingPreferencesRequest>(preference_payload())
                .unwrap();
        assert_eq!(request.viewing_preferences.audio_language, None);
        let mut stream_index = preference_payload();
        stream_index["viewing_preferences"]["audio_language"] = serde_json::json!(1);
        assert!(serde_json::from_value::<UpdateViewingPreferencesRequest>(stream_index).is_err());
    }
}
