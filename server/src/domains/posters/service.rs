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

use uuid::Uuid;

use crate::services::poster_management::{
    self, AssetScanConfig, CommunityPackImport, PosterImportResult,
};
use crate::state::AppState;

use super::error::PosterError;
use super::types::{
    ArtworkStatusResponse, ImportCommunityPackRequest, PosterImportResponse,
    ScanAssetDirectoryRequest, SelectArtworkRequest, SetArtworkLockRequest,
};

pub async fn scan_asset_directory(
    state: &AppState,
    req: ScanAssetDirectoryRequest,
) -> Result<PosterImportResponse, PosterError> {
    let runtime = state.runtime_config.load();
    let path = req
        .path
        .or_else(|| {
            runtime
                .metadata
                .asset_directory
                .as_deref()
                .map(PathBuf::from)
        })
        .ok_or(poster_management::PosterManagementError::AssetDirectoryNotConfigured)?;

    let result = poster_management::scan_asset_directory(
        &state.pool,
        &state.bootstrap.data_dir,
        AssetScanConfig {
            path,
            lock_imported: req.lock_imported.unwrap_or(true),
        },
    )
    .await?;

    Ok(result.into())
}

pub async fn import_community_pack(
    state: &AppState,
    req: ImportCommunityPackRequest,
) -> Result<PosterImportResponse, PosterError> {
    let result = poster_management::import_community_pack(
        &state.pool,
        &state.bootstrap.data_dir,
        CommunityPackImport {
            name: req.name,
            version: req.version,
            author: req.author,
            pack_root: req.pack_root,
            lock_imported: req.lock_imported.unwrap_or(false),
            artwork: req.artwork,
        },
    )
    .await?;

    Ok(result.into())
}

pub async fn set_artwork_lock(
    state: &AppState,
    artwork_id: Uuid,
    req: SetArtworkLockRequest,
) -> Result<ArtworkStatusResponse, PosterError> {
    poster_management::set_artwork_lock(&state.pool, artwork_id, req.locked).await?;
    Ok(ArtworkStatusResponse {
        artwork_id,
        status: if req.locked { "locked" } else { "unlocked" }.to_string(),
    })
}

pub async fn select_artwork(
    state: &AppState,
    artwork_id: Uuid,
    req: SelectArtworkRequest,
) -> Result<ArtworkStatusResponse, PosterError> {
    poster_management::select_artwork(&state.pool, artwork_id, req.lock).await?;
    Ok(ArtworkStatusResponse {
        artwork_id,
        status: "selected".to_string(),
    })
}

impl From<PosterImportResult> for PosterImportResponse {
    fn from(value: PosterImportResult) -> Self {
        Self {
            discovered: value.discovered,
            matched: value.matched,
            imported: value.imported,
            skipped: value.skipped,
            failed: value.failed,
            locked: value.locked,
        }
    }
}
