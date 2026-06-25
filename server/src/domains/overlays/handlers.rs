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
pub struct ListOverlaysQuery {
    library_id: Option<Uuid>,
    enabled: Option<bool>,
    page: Option<u32>,
    page_size: Option<u32>,
}

pub async fn list_overlays(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Query(query): Query<ListOverlaysQuery>,
) -> Result<Json<OverlayListResponse>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 200);

    let response = service::list_overlays(
        &state.pool,
        query.library_id,
        query.enabled.unwrap_or(false),
        page,
        page_size,
    )
    .await?;

    Ok(Json(response))
}

pub async fn get_overlay(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path(overlay_id): axum::extract::Path<Uuid>,
) -> Result<Json<OverlayDefinitionResponse>, AppError> {
    let response = service::get_overlay(&state.pool, overlay_id).await?;
    Ok(Json(response))
}

pub async fn create_overlay(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Json(req): Json<CreateOverlayRequest>,
) -> Result<Json<OverlayDefinitionResponse>, AppError> {
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
        instance: Some("/api/v1/overlays".into()),
    })?;

    service::validate_overlay_type(&req.overlay_type)?;
    let applies_to = req.applies_to.as_deref().unwrap_or("poster");
    service::validate_applies_to(applies_to)?;
    if let Some(ref align) = req.horizontal_align {
        service::validate_horizontal_align(align)?;
    }
    if let Some(ref align) = req.vertical_align {
        service::validate_vertical_align(align)?;
    }

    let response = service::create_overlay(&state.pool, req).await?;
    Ok(Json(response))
}

pub async fn update_overlay(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path(overlay_id): axum::extract::Path<Uuid>,
    Json(req): Json<UpdateOverlayRequest>,
) -> Result<Json<OverlayDefinitionResponse>, AppError> {
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
        instance: Some(format!("/api/v1/overlays/{}", overlay_id)),
    })?;

    if let Some(ref align) = req.horizontal_align {
        service::validate_horizontal_align(align)?;
    }
    if let Some(ref align) = req.vertical_align {
        service::validate_vertical_align(align)?;
    }
    if let Some(ref applies_to) = req.applies_to {
        service::validate_applies_to(applies_to)?;
    }

    let response = service::update_overlay(&state.pool, overlay_id, req).await?;
    Ok(Json(response))
}

pub async fn delete_overlay(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    axum::extract::Path(overlay_id): axum::extract::Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    service::delete_overlay(&state.pool, overlay_id).await?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

pub async fn apply_overlays(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Json(req): Json<ApplyOverlaysRequest>,
) -> Result<Json<ApplyOverlaysResponse>, AppError> {
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
        instance: Some("/api/v1/overlays/apply".into()),
    })?;

    let response = service::apply_overlays(&state.pool, req).await?;
    Ok(Json(response))
}

pub async fn preview_overlay(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Json(req): Json<PreviewOverlayRequest>,
) -> Result<Json<PreviewOverlayResponse>, AppError> {
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
        instance: Some("/api/v1/overlays/preview".into()),
    })?;

    let response = service::preview_overlay(&state, req).await?;
    Ok(Json(response))
}

pub async fn list_templates(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
) -> Result<Json<Vec<OverlayTemplateSummary>>, AppError> {
    let response = service::list_templates(&state.pool).await?;
    Ok(Json(response))
}

pub async fn import_template(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Json(import): Json<OverlayTemplateImport>,
) -> Result<Json<OverlayTemplateResponse>, AppError> {
    import.validate().map_err(|e| AppError::Validation {
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
        instance: Some("/api/v1/overlays/templates".into()),
    })?;

    let response = service::import_template(&state.pool, import).await?;
    Ok(Json(response))
}
