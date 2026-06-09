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
use crate::extractors::AuthenticatedUser;
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
            instance: Some(format!("/api/v1/media-items/{}", item_id)),
        }
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
