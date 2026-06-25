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
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use serde::Deserialize;
use uuid::Uuid;
use validator::Validate;

use crate::error::AppError;
use crate::extractors::AuthenticatedUser;
use crate::services::artwork_delivery;
use crate::services::image_pipeline::{self, ArtworkCategory, EncodeConfig};
use crate::state::AppState;

use super::service;
use super::types::*;

#[derive(Debug, Clone, Deserialize)]
pub struct ListMediaItemsQuery {
    pub library_id: Option<Uuid>,
    pub r#type: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub order: Option<String>,
}

pub async fn list_media_items(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Query(query): Query<ListMediaItemsQuery>,
) -> Result<Json<MediaItemListResponse>, AppError> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let order = query.order.as_deref().unwrap_or("desc");

    if let Some(ref t) = query.r#type {
        service::validate_media_type(t)?;
    }

    let response = service::list_media_items(
        &state.pool,
        query.library_id,
        query.r#type.as_deref(),
        limit,
        query.cursor.as_deref(),
        order,
    )
    .await?;

    Ok(Json(response))
}

pub async fn get_media_item(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    axum::extract::Path(item_id): axum::extract::Path<Uuid>,
) -> Result<Json<MediaItemResponse>, AppError> {
    let response = service::get_media_item(&state.pool, item_id).await?;
    Ok(Json(response))
}

pub async fn update_media_item(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    axum::extract::Path(item_id): axum::extract::Path<Uuid>,
    Json(req): Json<UpdateMediaItemRequest>,
) -> Result<Json<MediaItemResponse>, AppError> {
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
        instance: Some(format!("/api/v1/media-items/{}", item_id)),
    })?;

    let response = service::update_media_item(
        &state.pool,
        service::UpdateMediaItemParams {
            item_id,
            title: req.title,
            sort_title: req.sort_title,
            original_title: req.original_title,
            overview: req.overview,
            premiere_date: req.premiere_date,
            end_date: req.end_date,
            content_rating: req.content_rating,
            runtime_seconds: req.runtime_seconds,
            tmdb_id: req.tmdb_id,
            imdb_id: req.imdb_id,
            tvdb_id: req.tvdb_id,
            trakt_id: req.trakt_id,
            rating_average: req.rating_average,
            rating_vote_count: req.rating_vote_count,
            metadata: req.metadata,
            match_state: req.match_state,
            identification_source: req.identification_source,
        },
    )
    .await?;

    Ok(Json(response))
}

pub async fn delete_media_item(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    axum::extract::Path(item_id): axum::extract::Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    service::delete_media_item(&state.pool, item_id).await?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

pub async fn list_media_files(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    axum::extract::Path(item_id): axum::extract::Path<Uuid>,
) -> Result<Json<Vec<MediaFileResponse>>, AppError> {
    let files = service::list_media_files(&state.pool, item_id).await?;
    Ok(Json(files))
}

pub async fn get_media_file(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    axum::extract::Path((item_id, file_id)): axum::extract::Path<(Uuid, Uuid)>,
) -> Result<Json<MediaFileResponse>, AppError> {
    let file = service::get_media_file(&state.pool, item_id, file_id).await?;
    Ok(Json(file))
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArtworkQuery {
    pub size: Option<String>,
}

pub async fn get_artwork(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path((item_id, artwork_type)): Path<(Uuid, String)>,
    Query(query): Query<ArtworkQuery>,
) -> Result<Response, AppError> {
    let category = ArtworkCategory::from_db_str(&artwork_type)
        .ok_or_else(|| AppError::BadRequest(format!("unknown artwork type: {artwork_type}")))?;

    let variant_label = match &query.size {
        Some(size) => {
            if image_pipeline::resolve_variant(category, size).is_none() {
                return Err(AppError::BadRequest(format!(
                    "invalid size `{size}` for artwork type `{artwork_type}`"
                )));
            }
            size.as_str()
        }
        None => artwork_delivery::default_variant_label(category),
    };

    let config = state.runtime_config.load();
    let encode_config = EncodeConfig {
        lossy_quality: config.metadata.overlay_image_quality as f32,
    };
    drop(config);

    let images_cache_root = state.bootstrap.data_dir.join("cache").join("images");

    let resolved = artwork_delivery::resolve_variant(
        &state.pool,
        item_id,
        category,
        variant_label,
        &images_cache_root,
        &encode_config,
    )
    .await?;

    let etag = format!("\"{}-{}\"", resolved.artwork_id, variant_label);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/webp")
        .header(
            header::CACHE_CONTROL,
            "public, max-age=86400, stale-while-revalidate=604800, immutable",
        )
        .header(header::ETAG, etag)
        .body(Body::from(resolved.bytes))
        .unwrap())
}
