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

#[derive(Debug, Clone, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
    pub status: String,
    pub has_all_library_access: bool,
    pub streaming_policy_id: Option<Uuid>,
    pub max_streams: Option<i32>,
    pub max_transcode_streams: Option<i32>,
    pub bandwidth_limit_bps: Option<i64>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserListResponse {
    pub items: Vec<UserResponse>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocaleOptionResponse {
    pub tag: String,
    pub name: String,
    pub text_direction: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserPreferencesResponse {
    pub locale: String,
    pub available_locales: Vec<LocaleOptionResponse>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateUserPreferencesRequest {
    #[validate(length(min = 1, max = 35))]
    pub locale: String,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateUserRequest {
    #[validate(length(min = 1, max = 200))]
    pub display_name: Option<String>,

    #[validate(email)]
    pub email: Option<String>,

    pub avatar_url: Option<String>,

    pub role: Option<String>,

    pub status: Option<String>,

    pub has_all_library_access: Option<bool>,

    pub streaming_policy_id: Option<Uuid>,

    pub max_streams: Option<i32>,

    pub max_transcode_streams: Option<i32>,

    pub bandwidth_limit_bps: Option<i64>,
}

pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
    pub status: String,
    pub has_all_library_access: bool,
    pub streaming_policy_id: Option<Uuid>,
    pub max_streams: Option<i32>,
    pub max_transcode_streams: Option<i32>,
    pub bandwidth_limit_bps: Option<i64>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub static VALID_ROLES: &[&str] = &["owner", "admin", "member", "guest"];
pub static VALID_STATUSES: &[&str] = &["active", "disabled", "locked", "pending"];
