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

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::services::poster_management::CommunityArtworkEntry;

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ScanAssetDirectoryRequest {
    pub path: Option<PathBuf>,
    pub lock_imported: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ImportCommunityPackRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    pub version: Option<i32>,
    #[validate(length(max = 200))]
    pub author: Option<String>,
    pub pack_root: Option<PathBuf>,
    pub lock_imported: Option<bool>,
    #[validate(length(min = 1))]
    pub artwork: Vec<CommunityArtworkEntry>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SetArtworkLockRequest {
    pub locked: bool,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SelectArtworkRequest {
    pub lock: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PosterImportResponse {
    pub discovered: u64,
    pub matched: u64,
    pub imported: u64,
    pub skipped: u64,
    pub failed: u64,
    pub locked: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtworkStatusResponse {
    pub artwork_id: Uuid,
    pub status: String,
}
