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
use axum::extract::{Query, State};
use serde::Deserialize;
use uuid::Uuid;
use validator::Validate;

use crate::error::AppError;
use crate::extractors::{CanManageLibraries, Require};
use crate::state::AppState;

use super::service;
use super::types::*;

#[derive(Debug, Clone, Deserialize)]
pub struct ListLibrariesQuery {
    page: Option<u32>,
    page_size: Option<u32>,
    media_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListLibraryItemsQuery {
    r#type: Option<String>,
    limit: Option<u32>,
    cursor: Option<String>,
    order: Option<String>,
}

pub async fn list_libraries(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Query(query): Query<ListLibrariesQuery>,
) -> Result<Json<LibraryListResponse>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(25).clamp(1, 100);

    let response =
        service::list_libraries(&state.pool, page, page_size, query.media_type.as_deref()).await?;

    Ok(Json(response))
}

pub async fn get_library(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path(library_id): axum::extract::Path<Uuid>,
) -> Result<Json<LibraryResponse>, AppError> {
    let response = service::get_library(&state.pool, library_id).await?;
    Ok(Json(response))
}

pub async fn create_library(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Json(req): Json<CreateLibraryRequest>,
) -> Result<Json<LibraryResponse>, AppError> {
    req.validate().map_err(|e| AppError::Validation {
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
        instance: Some("/api/v1/libraries".into()),
    })?;

    service::validate_media_type(&req.media_type)?;

    let slug = service::generate_slug(&req.name);

    let response = service::create_library(
        &state.pool,
        service::CreateLibraryParams {
            name: req.name,
            slug,
            media_type: req.media_type,
            root_path: req.root_path,
            scan_interval_seconds: req.scan_interval_seconds.unwrap_or(86400),
            metadata_language: req.metadata_language.unwrap_or_else(|| "en".into()),
        },
    )
    .await?;

    let watcher_paths = service::list_library_path_strings(&state.pool, response.id)
        .await
        .ok();
    if let Some(paths) = watcher_paths {
        let path_bufs: Vec<std::path::PathBuf> =
            paths.into_iter().map(std::path::PathBuf::from).collect();
        if let Err(e) = state.fs_watcher.watch_library(response.id, path_bufs) {
            tracing::warn!(library_id = %response.id, error = %e, "Failed to start FS watcher for new library");
        }
    }

    Ok(Json(response))
}

pub async fn update_library(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path(library_id): axum::extract::Path<Uuid>,
    Json(req): Json<UpdateLibraryRequest>,
) -> Result<Json<LibraryResponse>, AppError> {
    req.validate().map_err(|e| AppError::Validation {
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
        instance: Some(format!("/api/v1/libraries/{}", library_id)),
    })?;

    let slug = req.name.as_ref().map(|n| service::generate_slug(n));

    let response = service::update_library(
        &state.pool,
        service::UpdateLibraryParams {
            library_id,
            name: req.name,
            slug,
            root_path: req.root_path,
            scan_enabled: req.scan_enabled,
            scan_interval_seconds: req.scan_interval_seconds,
            metadata_language: req.metadata_language,
            metadata: req.metadata,
        },
    )
    .await?;

    Ok(Json(response))
}

pub async fn delete_library(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path(library_id): axum::extract::Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    service::soft_delete_library(&state.pool, library_id).await?;
    state.fs_watcher.unwatch_library(library_id);
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

pub async fn scan_library(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path(library_id): axum::extract::Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pool = state.pool.clone();
    let enrichment = Some(state.enrichment.clone());
    let result =
        crate::workers::library_scanner::scan_library(&pool, library_id, false, enrichment)
            .await
            .map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!("{}", e)))?;
    Ok(Json(serde_json::to_value(result).unwrap_or_else(
        |_| serde_json::json!({ "status": "scan_completed" }),
    )))
}

pub async fn list_library_items(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path(library_id): axum::extract::Path<Uuid>,
    Query(query): Query<ListLibraryItemsQuery>,
) -> Result<Json<crate::domains::media::types::MediaItemListResponse>, AppError> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let order = query.order.as_deref().unwrap_or("desc");

    if let Some(ref t) = query.r#type {
        crate::domains::media::service::validate_media_type(t)?;
    }

    let response = crate::domains::media::service::list_library_items(
        &state.pool,
        library_id,
        query.r#type.as_deref(),
        limit,
        query.cursor.as_deref(),
        order,
    )
    .await?;

    Ok(Json(response))
}

pub async fn list_library_paths(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path(library_id): axum::extract::Path<Uuid>,
) -> Result<Json<Vec<LibraryPathResponse>>, AppError> {
    let paths = service::list_library_paths(&state.pool, library_id).await?;
    Ok(Json(paths))
}

pub async fn get_library_path(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path((library_id, path_id)): axum::extract::Path<(Uuid, Uuid)>,
) -> Result<Json<LibraryPathResponse>, AppError> {
    let path = service::get_library_path(&state.pool, library_id, path_id).await?;
    Ok(Json(path))
}

pub async fn create_library_path(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path(library_id): axum::extract::Path<Uuid>,
    Json(req): Json<CreateLibraryPathRequest>,
) -> Result<Json<LibraryPathResponse>, AppError> {
    req.validate().map_err(|e| AppError::Validation {
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
        instance: Some(format!("/api/v1/libraries/{}/paths", library_id)),
    })?;

    let response = service::create_library_path(
        &state.pool,
        service::CreateLibraryPathParams {
            library_id,
            path: req.path,
            is_default: req.is_default.unwrap_or(false),
            scan_enabled: req.scan_enabled.unwrap_or(true),
        },
    )
    .await?;

    if response.scan_enabled
        && let Ok(paths) = service::list_library_path_strings(&state.pool, library_id).await
    {
        let path_bufs: Vec<std::path::PathBuf> =
            paths.into_iter().map(std::path::PathBuf::from).collect();
        if let Err(e) = state.fs_watcher.watch_library(library_id, path_bufs) {
            tracing::warn!(library_id = %library_id, error = %e, "Failed to update FS watcher after path creation");
        }
    }

    Ok(Json(response))
}

pub async fn update_library_path(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path((library_id, path_id)): axum::extract::Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateLibraryPathRequest>,
) -> Result<Json<LibraryPathResponse>, AppError> {
    req.validate().map_err(|e| AppError::Validation {
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
        instance: Some(format!(
            "/api/v1/libraries/{}/paths/{}",
            library_id, path_id
        )),
    })?;

    let response = service::update_library_path(
        &state.pool,
        service::UpdateLibraryPathParams {
            library_id,
            path_id,
            path: req.path,
            is_default: req.is_default,
            scan_enabled: req.scan_enabled,
        },
    )
    .await?;

    Ok(Json(response))
}

pub async fn delete_library_path(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path((library_id, path_id)): axum::extract::Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    service::delete_library_path(&state.pool, library_id, path_id).await?;
    if let Ok(paths) = service::list_library_path_strings(&state.pool, library_id).await {
        let path_bufs: Vec<std::path::PathBuf> =
            paths.into_iter().map(std::path::PathBuf::from).collect();
        if path_bufs.is_empty() {
            state.fs_watcher.unwatch_library(library_id);
        } else if let Err(e) = state.fs_watcher.watch_library(library_id, path_bufs) {
            tracing::warn!(library_id = %library_id, error = %e, "Failed to update FS watcher after path deletion");
        }
    }
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}
