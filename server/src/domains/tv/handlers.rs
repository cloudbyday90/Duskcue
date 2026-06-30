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

use crate::error::AppError;
use crate::extractors::AuthenticatedUser;
use crate::state::AppState;

use super::error::TvError;
use super::service;
use super::types::*;

pub async fn get_tv_surface(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Query(query): Query<TvSurfaceQuery>,
) -> Result<Json<TvSurfaceResponse>, AppError> {
    let query = service::resolve_surface_query(query)?;
    Ok(Json(service::empty_surface_response(&query)))
}

pub async fn resolve_platform_content(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path(platform_content_id): Path<String>,
) -> Result<Json<TvResolveResponse>, AppError> {
    let _parsed = service::parse_platform_content_id(&platform_content_id)?;
    Err(TvError::UnavailableContent.into())
}

pub async fn get_tv_settings(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<TvSurfaceSettingsResponse>, AppError> {
    Ok(Json(service::default_settings()))
}

pub async fn get_tv_diagnostics(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Query(query): Query<TvSurfaceQuery>,
) -> Result<Json<TvDiagnosticsResponse>, AppError> {
    let query = service::resolve_surface_query(query)?;
    Ok(Json(service::empty_diagnostics(query.platform)))
}
