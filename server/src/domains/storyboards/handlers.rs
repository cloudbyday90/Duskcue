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
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::domains::storyboards::service;
use crate::domains::storyboards::types::*;
use crate::error::AppError;
use crate::extractors::{AuthenticatedUser, CanManageLibraries, Require};
use crate::state::AppState;

pub async fn get_storyboard(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(query): Query<StoryboardQuery>,
) -> Result<Response, AppError> {
    assert_media_profile_access(&state, &user, item_id).await?;
    let result = service::get_storyboard(&state.pool, item_id, query.media_file_id).await?;
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(result)).into_response())
}

pub async fn get_storyboard_index(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(query): Query<StoryboardQuery>,
) -> Result<Response, AppError> {
    assert_media_profile_access(&state, &user, item_id).await?;
    let cache_dir = state.bootstrap.cache_dir.clone();
    let content =
        service::get_storyboard_index(&state.pool, item_id, query.media_file_id, &cache_dir)
            .await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/vtt; charset=utf-8")
        .header(header::CACHE_CONTROL, "private, no-store")
        .body(Body::from(content))
        .unwrap())
}

pub async fn get_storyboard_sprite(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path((item_id, sprite)): Path<(Uuid, String)>,
    Query(query): Query<StoryboardQuery>,
) -> Result<Response, AppError> {
    assert_media_profile_access(&state, &user, item_id).await?;
    let cache_dir = state.bootstrap.cache_dir.clone();
    let data = service::get_storyboard_sprite(
        &state.pool,
        item_id,
        query.media_file_id,
        &sprite,
        &cache_dir,
    )
    .await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/webp")
        .header(header::CACHE_CONTROL, "private, no-store")
        .body(Body::from(data))
        .unwrap())
}

async fn assert_media_profile_access(
    state: &AppState,
    user: &AuthenticatedUser,
    item_id: Uuid,
) -> Result<(), AppError> {
    let scope = crate::domains::profiles::service::load_profile_scope(
        &state.pool,
        user.user_id,
        user.profile_id,
        user.has_all_library_access,
    )
    .await?;
    crate::domains::profiles::service::assert_media_access(&state.pool, &scope, item_id).await?;
    Ok(())
}

pub async fn generate_library_storyboards(
    State(state): State<AppState>,
    auth: Require<CanManageLibraries>,
    Path(library_id): Path<Uuid>,
) -> Result<Json<GenerateStoryboardsResponse>, AppError> {
    let result =
        service::trigger_library_generation(&state, library_id, Some(auth.user.user_id)).await?;
    Ok(Json(result))
}

pub async fn generate_item_storyboards(
    State(state): State<AppState>,
    auth: Require<CanManageLibraries>,
    Path(item_id): Path<Uuid>,
) -> Result<Json<GenerateStoryboardsResponse>, AppError> {
    let result = service::trigger_item_generation(&state, item_id, Some(auth.user.user_id)).await?;
    Ok(Json(result))
}

pub async fn delete_storyboard(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path(item_id): Path<Uuid>,
) -> Result<Json<DeleteStoryboardResponse>, AppError> {
    let cache_dir = state.bootstrap.cache_dir.clone();
    service::delete_storyboard(&state.pool, item_id, &cache_dir).await?;
    Ok(Json(DeleteStoryboardResponse {
        deleted: true,
        media_item_id: item_id,
    }))
}
