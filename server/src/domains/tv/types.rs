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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvPlatform {
    AndroidTv,
    GoogleTv,
    FireTv,
    Roku,
    Tizen,
    Webos,
    Tvos,
    Xbox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvSurfaceSectionType {
    #[serde(rename = "continue")]
    Continue,
    NextUp,
    NewEpisodes,
    Recommended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvMediaType {
    Movie,
    Episode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvAvailabilityState {
    Playable,
    NeedsTranscode,
    LibraryOffline,
    MissingFile,
    AccessRevoked,
    MetadataIncomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvPlatformIdTarget {
    Canonical,
    RokuFeed,
    AmazonCatalog,
    UrlPath,
    UrlQuery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvContentAccessStatus {
    Accessible,
    AccessDenied,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TvSurfaceQuery {
    pub platform: Option<String>,
    pub limit: Option<u32>,
    pub sections: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedTvSurfaceQuery {
    pub platform: Option<TvPlatform>,
    pub limit: u32,
    pub sections: Vec<TvSurfaceSectionType>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TvSurfaceResponse {
    pub generated_at: DateTime<Utc>,
    pub platform: Option<TvPlatform>,
    pub limit: u32,
    pub sections: Vec<TvSurfaceSectionResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TvSurfaceSectionResponse {
    pub section_type: TvSurfaceSectionType,
    pub title: String,
    pub empty_reason: Option<String>,
    pub items: Vec<TvSurfaceItemResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TvSurfaceItemResponse {
    pub surface_item_id: String,
    pub platform_content_id: String,
    pub media_item_id: Uuid,
    pub media_type: TvMediaType,
    pub section_type: TvSurfaceSectionType,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub duration_ms: Option<i64>,
    pub resume_position_ms: i64,
    pub progress_percent: f64,
    pub last_engaged_at: Option<DateTime<Utc>>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub deep_link: String,
    pub web_url: String,
    pub availability: TvAvailabilityState,
    pub availability_detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TvResolveResponse {
    pub platform_content_id: String,
    pub media_item_id: Uuid,
    pub media_type: TvMediaType,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub duration_ms: Option<i64>,
    pub resume_position_ms: i64,
    pub availability: TvAvailabilityState,
    pub availability_detail: Option<String>,
    pub playback_action: String,
    pub playback_start_path: String,
    pub playback_start: TvPlaybackStartHints,
    pub deep_link: String,
    pub web_url: String,
    pub artwork: TvArtworkHints,
    pub requires_auth: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TvPlaybackStartHints {
    pub method: String,
    pub path: String,
    pub media_item_id: Uuid,
    pub media_file_id: Option<Uuid>,
    pub start_position_ms: i64,
    pub force_transcode: bool,
    pub device_profile_required: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TvArtworkHints {
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub logo_url: Option<String>,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TvSurfaceSettingsResponse {
    pub tv_publication_enabled: bool,
    pub enabled_platforms: Vec<TvPlatform>,
    pub publish_continue_watching: bool,
    pub publish_next_up: bool,
    pub publish_new_episodes: bool,
    pub publish_recommendations: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TvSurfaceSettingsRequest {
    pub tv_publication_enabled: Option<bool>,
    pub enabled_platforms: Option<Vec<String>>,
    pub publish_continue_watching: Option<bool>,
    pub publish_next_up: Option<bool>,
    pub publish_new_episodes: Option<bool>,
    pub publish_recommendations: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TvDiagnosticsResponse {
    pub generated_at: DateTime<Utc>,
    pub platform: Option<TvPlatform>,
    pub candidate_count: u32,
    pub included_count: u32,
    pub section_counts: Vec<TvDiagnosticSectionCount>,
    pub reason_counts: Vec<TvDiagnosticReasonCount>,
    pub excluded: Vec<TvDiagnosticExclusion>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TvDiagnosticSectionCount {
    pub section_type: TvSurfaceSectionType,
    pub item_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TvDiagnosticReasonCount {
    pub reason: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TvDiagnosticExclusion {
    pub media_item_id: Option<Uuid>,
    pub reason: String,
    pub detail: String,
    pub availability: TvAvailabilityState,
}

#[derive(Debug, Clone, Serialize)]
pub struct TvSurfaceChangedEventPayload {
    pub user_id: Uuid,
    pub reason: String,
    pub changed_sections: Vec<TvSurfaceSectionType>,
    pub media_item_id: Option<Uuid>,
    pub series_id: Option<Uuid>,
    pub library_id: Option<Uuid>,
    pub generated_after: DateTime<Utc>,
    pub debounce_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformContentId {
    pub media_type: TvMediaType,
    pub media_item_id: Uuid,
}

#[derive(Debug, Clone, Serialize)]
pub struct TvPlatformContentLookup {
    pub platform_content_id: String,
    pub media_item_id: Uuid,
    pub media_type: TvMediaType,
    pub library_id: Uuid,
    pub access_status: TvContentAccessStatus,
}
