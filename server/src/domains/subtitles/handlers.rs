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

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use uuid::Uuid;

use crate::error::AppError;
use crate::domains::subtitles::service;
use crate::domains::subtitles::types::*;
use crate::extractors::AuthenticatedUser;
use crate::state::AppState;

pub async fn list_subtitles(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<SubtitleListResponse>, AppError> {
    let result = service::list_subtitles(&state.pool, item_id).await?;
    Ok(Json(result))
}

pub async fn get_subtitle(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path((item_id, subtitle_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<SubtitleFileResponse>, AppError> {
    let result = service::get_subtitle(&state.pool, item_id, subtitle_id).await?;
    Ok(Json(result))
}

pub async fn get_subtitle_content(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path((item_id, subtitle_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<SubtitleContentQuery>,
) -> Result<Response, AppError> {
    let user_offset_ms = get_user_subtitle_offset(&state.pool, user.user_id, item_id).await;

    let (content, content_type) = service::get_subtitle_content(
        &state.pool,
        item_id,
        subtitle_id,
        query.format.as_deref(),
        user_offset_ms,
    )
    .await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(content))
        .unwrap())
}

pub async fn fetch_subtitles(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Json(req): Json<FetchSubtitlesRequest>,
) -> Result<Json<FetchSubtitlesResponse>, AppError> {
    let result = service::fetch_subtitles(&state.pool, item_id, &req).await?;
    Ok(Json(result))
}

pub async fn set_subtitle_offset(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path((item_id, subtitle_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<SetSubtitleOffsetRequest>,
) -> Result<Json<SubtitleOffsetResponse>, AppError> {
    let result = service::set_subtitle_offset(
        &state.pool,
        user.user_id,
        item_id,
        subtitle_id,
        req.offset_ms,
    )
    .await?;
    Ok(Json(result))
}

pub async fn trigger_ocr(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path((item_id, subtitle_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<TriggerOcrRequest>,
) -> Result<Json<SubtitleOcrResult>, AppError> {
    let result =
        service::trigger_ocr(&state.pool, item_id, subtitle_id, req.engine.as_deref()).await?;
    Ok(Json(result))
}

pub async fn get_subtitle_sync_data(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path((item_id, subtitle_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<SubtitleSyncDataResponse>, AppError> {
    let result = service::get_subtitle_sync_data(&state.pool, item_id, subtitle_id).await?;
    Ok(Json(result))
}

pub async fn delete_subtitle(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path((item_id, subtitle_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, AppError> {
    service::delete_subtitle(&state.pool, item_id, subtitle_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn get_user_subtitle_offset(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    media_item_id: Uuid,
) -> Option<i32> {
    let row = sqlx::query(
        "SELECT metadata->>'subtitle_offset_ms' AS offset_ms \
         FROM user_item_data \
         WHERE user_id = $1 AND media_item_id = $2",
    )
    .bind(user_id)
    .bind(media_item_id)
    .fetch_optional(pool)
    .await
    .ok()??;

    use sqlx::Row;
    row.try_get::<Option<String>, _>("offset_ms")
        .ok()
        .flatten()
        .and_then(|s| s.parse::<i32>().ok())
}
