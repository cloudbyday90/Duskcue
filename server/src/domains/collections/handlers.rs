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
use crate::extractors::{CanManageLibraries, Require};
use crate::state::AppState;

use super::service;
use super::types::*;

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

pub async fn list_collections(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Query(query): Query<ListCollectionsQuery>,
) -> Result<Json<CollectionListResponse>, AppError> {
    if let Some(ref collection_type) = query.collection_type {
        service::validate_collection_type(collection_type)?;
    }
    if let Some(ref visibility) = query.visibility {
        service::validate_visibility(visibility)?;
    }

    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 200);
    let response = service::list_collections(&state.pool, query, page, page_size).await?;
    Ok(Json(response))
}

pub async fn get_collection(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path(collection_id): Path<Uuid>,
) -> Result<Json<CollectionResponse>, AppError> {
    let response = service::get_collection(&state.pool, collection_id).await?;
    Ok(Json(response))
}

pub async fn create_collection(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Json(req): Json<CreateCollectionRequest>,
) -> Result<Json<CollectionResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, "/api/v1/collections"))?;
    validate_create_request(&req)?;

    let response = service::create_collection(&state.pool, req).await?;
    Ok(Json(response))
}

pub async fn update_collection(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path(collection_id): Path<Uuid>,
    Json(req): Json<UpdateCollectionRequest>,
) -> Result<Json<CollectionResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, format!("/api/v1/collections/{collection_id}")))?;
    validate_update_request(&req)?;

    let response = service::update_collection(&state.pool, collection_id, req).await?;
    Ok(Json(response))
}

pub async fn delete_collection(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path(collection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    service::delete_collection(&state.pool, collection_id).await?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

pub async fn list_collection_items(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path(collection_id): Path<Uuid>,
    Query(query): Query<ListCollectionItemsQuery>,
) -> Result<Json<CollectionItemsResponse>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 200);
    let response =
        service::list_collection_items(&state.pool, collection_id, query, page, page_size).await?;
    Ok(Json(response))
}

pub async fn add_collection_items(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path(collection_id): Path<Uuid>,
    Json(req): Json<AddCollectionItemsRequest>,
) -> Result<Json<CollectionItemsResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, format!("/api/v1/collections/{collection_id}/items")))?;

    let response = service::add_collection_items(&state.pool, collection_id, req).await?;
    Ok(Json(response))
}

pub async fn reorder_collection_items(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path(collection_id): Path<Uuid>,
    Json(req): Json<ReorderCollectionItemsRequest>,
) -> Result<Json<CollectionItemsResponse>, AppError> {
    req.validate().map_err(|e| {
        validation_error(
            e,
            format!("/api/v1/collections/{collection_id}/items/reorder"),
        )
    })?;

    let response = service::reorder_collection_items(&state.pool, collection_id, req).await?;
    Ok(Json(response))
}

pub async fn remove_collection_item(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path((collection_id, media_item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    service::remove_collection_item(&state.pool, collection_id, media_item_id).await?;
    Ok(Json(serde_json::json!({ "status": "removed" })))
}

pub async fn sync_collections(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Json(req): Json<SyncCollectionsRequest>,
) -> Result<Json<SyncCollectionResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, "/api/v1/collections/sync"))?;

    let response = service::sync_collections(&state.pool, req).await?;
    Ok(Json(response))
}

pub async fn sync_collection(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path(collection_id): Path<Uuid>,
    Json(req): Json<SyncCollectionRequest>,
) -> Result<Json<SyncCollectionResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, format!("/api/v1/collections/{collection_id}/sync")))?;

    let response = service::sync_collection(&state.pool, collection_id, req).await?;
    Ok(Json(response))
}

pub async fn list_templates(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
) -> Result<Json<Vec<CollectionTemplateSummary>>, AppError> {
    let response = service::list_templates(&state.pool).await?;
    Ok(Json(response))
}

pub async fn import_template(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Json(req): Json<ImportCollectionTemplateRequest>,
) -> Result<Json<CollectionTemplateResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, "/api/v1/collections/templates"))?;
    service::validate_template_type(&req.template_type)?;

    let response = service::import_template(&state.pool, req).await?;
    Ok(Json(response))
}

fn validate_create_request(req: &CreateCollectionRequest) -> Result<(), AppError> {
    let collection_type = req.collection_type.as_deref().unwrap_or("static");
    service::validate_collection_type(collection_type)?;
    service::validate_visibility(req.visibility.as_deref().unwrap_or("visible"))?;
    service::validate_sync_mode(req.sync_mode.as_deref().unwrap_or("sync"))?;

    if collection_type == "dynamic" {
        let config = req.dynamic_config.as_ref().ok_or_else(|| {
            super::error::CollectionsError::InvalidDynamicConfig(
                "dynamic_config is required for dynamic collections".into(),
            )
        })?;
        service::validate_dynamic_config(config)?;
    }

    if collection_type == "smart" {
        let filter = req.smart_filter.as_ref().ok_or_else(|| {
            super::error::CollectionsError::InvalidSmartFilter(
                "smart_filter is required for smart collections".into(),
            )
        })?;
        service::validate_smart_filter(filter)?;
    }

    Ok(())
}

fn validate_update_request(req: &UpdateCollectionRequest) -> Result<(), AppError> {
    if let Some(ref collection_type) = req.collection_type {
        service::validate_collection_type(collection_type)?;
    }
    if let Some(ref visibility) = req.visibility {
        service::validate_visibility(visibility)?;
    }
    if let Some(ref sync_mode) = req.sync_mode {
        service::validate_sync_mode(sync_mode)?;
    }
    if let Some(ref config) = req.dynamic_config {
        service::validate_dynamic_config(config)?;
    }
    if let Some(ref filter) = req.smart_filter {
        service::validate_smart_filter(filter)?;
    }

    Ok(())
}
