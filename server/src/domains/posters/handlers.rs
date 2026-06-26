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

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;
use validator::Validate;

use crate::error::{AppError, FieldError};
use crate::extractors::{CanManageLibraries, Require};
use crate::state::AppState;

use super::service;
use super::types::{
    ArtworkStatusResponse, ImportCommunityPackRequest, PosterImportResponse,
    ScanAssetDirectoryRequest, SelectArtworkRequest, SetArtworkLockRequest,
};

pub async fn scan_asset_directory(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Json(req): Json<ScanAssetDirectoryRequest>,
) -> Result<Json<PosterImportResponse>, AppError> {
    validate(&req, "/api/v1/posters/assets/scan")?;
    Ok(Json(service::scan_asset_directory(&state, req).await?))
}

pub async fn import_community_pack(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Json(req): Json<ImportCommunityPackRequest>,
) -> Result<Json<PosterImportResponse>, AppError> {
    validate(&req, "/api/v1/posters/community/import")?;
    Ok(Json(service::import_community_pack(&state, req).await?))
}

pub async fn set_artwork_lock(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path(artwork_id): Path<Uuid>,
    Json(req): Json<SetArtworkLockRequest>,
) -> Result<Json<ArtworkStatusResponse>, AppError> {
    validate(&req, &format!("/api/v1/posters/{artwork_id}/lock"))?;
    Ok(Json(
        service::set_artwork_lock(&state, artwork_id, req).await?,
    ))
}

pub async fn select_artwork(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path(artwork_id): Path<Uuid>,
    Json(req): Json<SelectArtworkRequest>,
) -> Result<Json<ArtworkStatusResponse>, AppError> {
    validate(&req, &format!("/api/v1/posters/{artwork_id}/select"))?;
    Ok(Json(
        service::select_artwork(&state, artwork_id, req).await?,
    ))
}

fn validate<T: Validate>(value: &T, instance: &str) -> Result<(), AppError> {
    value.validate().map_err(|e| AppError::Validation {
        errors: e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |err| FieldError {
                    field: field.to_string(),
                    code: err.code.to_string(),
                    message: err
                        .message
                        .as_ref()
                        .map(|m| m.to_string())
                        .unwrap_or_default(),
                })
            })
            .collect(),
        instance: Some(instance.to_string()),
    })
}
