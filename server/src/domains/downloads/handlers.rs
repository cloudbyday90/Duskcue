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
use axum::extract::{Path, Query, State};
use uuid::Uuid;
use validator::Validate;

use crate::error::AppError;
use crate::extractors::{AuthenticatedUser, CanDownload, Require};
use crate::state::AppState;

use super::service;
use super::types::*;

pub async fn get_download_plan(
    State(state): State<AppState>,
    auth: Require<CanDownload>,
    Path(media_item_id): Path<Uuid>,
    Query(query): Query<DownloadPlanQuery>,
) -> Result<Json<DownloadPlanResponse>, AppError> {
    query
        .validate()
        .map_err(|e| validation_error(e, format!("/api/v1/downloads/plan/{media_item_id}")))?;
    Ok(Json(
        service::get_download_plan(&state, &auth.user, media_item_id, query).await?,
    ))
}

pub async fn create_download_job(
    State(state): State<AppState>,
    auth: Require<CanDownload>,
    Json(req): Json<CreateDownloadJobRequest>,
) -> Result<Json<DownloadJobResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, "/api/v1/downloads/jobs"))?;
    Ok(Json(
        service::create_download_job(&state, &auth.user, req).await?,
    ))
}

pub async fn get_download_job(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<DownloadJobResponse>, AppError> {
    Ok(Json(service::get_download_job(&state, &user, id).await?))
}

pub async fn cancel_download_job(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CancelDownloadJobRequest>,
) -> Result<Json<DownloadActionResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, format!("/api/v1/downloads/jobs/{id}/cancel")))?;
    Ok(Json(
        service::cancel_download_job(&state, &user, id, req).await?,
    ))
}

pub async fn list_download_inventory(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(query): Query<DownloadInventoryQuery>,
) -> Result<Json<DownloadInventoryResponse>, AppError> {
    query
        .validate()
        .map_err(|e| validation_error(e, "/api/v1/downloads/inventory"))?;
    Ok(Json(
        service::list_download_inventory(&state, &user, query).await?,
    ))
}

pub async fn delete_download_package(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<Uuid>,
    payload: Option<Json<DeleteDownloadPackageRequest>>,
) -> Result<Json<DownloadActionResponse>, AppError> {
    let req = payload.map(|Json(req)| req).unwrap_or_default();
    req.validate()
        .map_err(|e| validation_error(e, format!("/api/v1/downloads/packages/{id}")))?;
    Ok(Json(
        service::delete_download_package(&state, &user, id, req).await?,
    ))
}

pub async fn get_package_manifest(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<DownloadPackageManifestResponse>, AppError> {
    Ok(Json(
        service::get_package_manifest(&state, &user, id).await?,
    ))
}

pub async fn create_package_transfer_urls(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(req): Json<PackageTransferUrlsRequest>,
) -> Result<Json<PackageTransferUrlsResponse>, AppError> {
    req.validate().map_err(|e| {
        validation_error(e, format!("/api/v1/downloads/packages/{id}/transfer-urls"))
    })?;
    Ok(Json(
        service::create_package_transfer_urls(&state, &user, id, req).await?,
    ))
}

pub async fn serve_package_file(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path((id, file_path)): Path<(Uuid, String)>,
) -> Result<(), AppError> {
    service::serve_package_file(&state, &user, id, file_path).await?;
    Ok(())
}

pub async fn sync_download_state(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<DownloadSyncRequest>,
) -> Result<Json<DownloadSyncResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, "/api/v1/downloads/sync"))?;
    Ok(Json(
        service::sync_download_state(&state, &user, req).await?,
    ))
}

fn validation_error(e: validator::ValidationErrors, instance: impl Into<String>) -> AppError {
    AppError::Validation {
        errors: e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |err| crate::error::FieldError {
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
        instance: Some(instance.into()),
    }
}
