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

use axum::extract::{Query, State};
use axum::Json;
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

pub async fn list_libraries(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Query(query): Query<ListLibrariesQuery>,
) -> Result<Json<LibraryListResponse>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(25).clamp(1, 100);

    let response = service::list_libraries(
        &state.pool,
        page,
        page_size,
        query.media_type.as_deref(),
    )
    .await?;

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
    req.validate().map_err(|e| {
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
            instance: Some("/api/v1/libraries".into()),
        }
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

    Ok(Json(response))
}

pub async fn update_library(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path(library_id): axum::extract::Path<Uuid>,
    Json(req): Json<UpdateLibraryRequest>,
) -> Result<Json<LibraryResponse>, AppError> {
    req.validate().map_err(|e| {
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
            instance: Some(format!("/api/v1/libraries/{}", library_id)),
        }
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
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

pub async fn scan_library(
    State(_state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path(_library_id): axum::extract::Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!("Library scanning — Phase 5 Task 5")
}

pub async fn list_library_items(
    State(_state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path(_library_id): axum::extract::Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!("Library items listing — Phase 5 Task 4 (media domain)")
}
