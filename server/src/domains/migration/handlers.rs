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

use crate::error::{AppError, FieldError};
use crate::extractors::{CanManageUsers, Require};
use crate::state::AppState;

use super::service;
use super::types::*;

pub async fn create_migration_source(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Json(req): Json<CreateMigrationSourceRequest>,
) -> Result<Json<MigrationSourceResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, "/api/v1/migrations"))?;
    Ok(Json(service::create_migration_source(&state, req).await?))
}

pub async fn list_migration_sources(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Query(query): Query<ListMigrationSourcesQuery>,
) -> Result<Json<MigrationSourceListResponse>, AppError> {
    let (page, page_size) = validate_page(query.page, query.page_size, "/api/v1/migrations")?;
    Ok(Json(
        service::list_migration_sources(&state, query, page, page_size).await?,
    ))
}

pub async fn get_migration_source(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MigrationSourceResponse>, AppError> {
    Ok(Json(service::get_migration_source(&state, id).await?))
}

pub async fn delete_migration_source(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(), AppError> {
    service::delete_migration_source(&state, id).await?;
    Ok(())
}

pub async fn test_connection(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MigrationActionResponse>, AppError> {
    Ok(Json(service::test_connection(&state, id).await?))
}

pub async fn discover_source(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MigrationActionResponse>, AppError> {
    Ok(Json(service::discover_source(&state, id).await?))
}

pub async fn save_user_mappings(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<SaveUserMappingsRequest>,
) -> Result<Json<MigrationActionResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, "/api/v1/migrations/{id}/map-users"))?;
    Ok(Json(service::save_user_mappings(&state, id, req).await?))
}

pub async fn start_migration(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<StartMigrationRequest>,
) -> Result<Json<MigrationActionResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, "/api/v1/migrations/{id}/start"))?;
    Ok(Json(service::start_migration(&state, id, req).await?))
}

pub async fn run_preflight(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MigrationPreflightResponse>, AppError> {
    Ok(Json(service::run_preflight(&state, id).await?))
}

pub async fn get_migration_progress(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MigrationProgressResponse>, AppError> {
    Ok(Json(service::get_migration_progress(&state, id).await?))
}

pub async fn get_unmatched_report(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<UnmatchedReportQuery>,
) -> Result<Json<UnmatchedReportResponse>, AppError> {
    let (page, page_size) = validate_page(
        query.page,
        query.page_size,
        "/api/v1/migrations/{id}/unmatched",
    )?;
    Ok(Json(
        service::get_unmatched_report(&state, id, query, page, page_size).await?,
    ))
}

pub async fn cancel_migration(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MigrationActionResponse>, AppError> {
    Ok(Json(service::cancel_migration(&state, id).await?))
}

fn validate_page(
    page: Option<u32>,
    page_size: Option<u32>,
    instance: &str,
) -> Result<(u32, u32), AppError> {
    let page = page.unwrap_or(1);
    let page_size = page_size.unwrap_or(25);

    if page == 0 {
        return Err(AppError::Validation {
            errors: vec![FieldError {
                field: "page".to_string(),
                code: "range".to_string(),
                message: "page must be at least 1".to_string(),
            }],
            instance: Some(instance.to_string()),
        });
    }

    if !(1..=100).contains(&page_size) {
        return Err(AppError::Validation {
            errors: vec![FieldError {
                field: "page_size".to_string(),
                code: "range".to_string(),
                message: "page_size must be between 1 and 100".to_string(),
            }],
            instance: Some(instance.to_string()),
        });
    }

    Ok((page, page_size))
}

fn validation_error(errors: validator::ValidationErrors, instance: &str) -> AppError {
    AppError::Validation {
        errors: errors
            .field_errors()
            .iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |error| FieldError {
                    field: field.to_string(),
                    code: error.code.to_string(),
                    message: format!("{} failed validation", field),
                })
            })
            .collect(),
        instance: Some(instance.to_string()),
    }
}
