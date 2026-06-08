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

pub struct LibraryRow {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub media_type: String,
    pub root_path: String,
    pub scan_enabled: bool,
    pub scan_interval_seconds: i32,
    pub metadata_language: String,
    pub metadata: serde_json::Value,
    pub last_scan_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LibraryResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub media_type: String,
    pub root_path: String,
    pub scan_enabled: bool,
    pub scan_interval_seconds: i32,
    pub metadata_language: String,
    pub metadata: serde_json::Value,
    pub last_scan_at: Option<DateTime<Utc>>,
    pub item_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LibraryListResponse {
    pub items: Vec<LibraryResponse>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateLibraryRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: String,

    #[validate(length(min = 1, max = 500))]
    pub root_path: String,

    pub media_type: String,

    #[validate(range(min = 60))]
    pub scan_interval_seconds: Option<i32>,

    pub metadata_language: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateLibraryRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: Option<String>,

    pub root_path: Option<String>,

    pub scan_enabled: Option<bool>,

    #[validate(range(min = 60))]
    pub scan_interval_seconds: Option<i32>,

    pub metadata_language: Option<String>,

    pub metadata: Option<serde_json::Value>,
}

pub static VALID_MEDIA_TYPES: &[&str] = &["movies", "tvshows"];
