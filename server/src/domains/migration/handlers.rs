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
use axum::extract::Multipart;
use axum::extract::{Path, Query, State};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;
use validator::Validate;

use crate::error::{AppError, FieldError};
use crate::extractors::{CanManageUsers, Require};
use crate::state::AppState;

use super::error::MigrationError;
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
    payload: Option<Json<MigrationSourceCredentialRequest>>,
) -> Result<Json<MigrationActionResponse>, AppError> {
    let req = validate_optional_credentials(payload, "/api/v1/migrations/{id}/connect")?;
    Ok(Json(service::test_connection(&state, id, req).await?))
}

pub async fn discover_source(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    payload: Option<Json<MigrationSourceCredentialRequest>>,
) -> Result<Json<MigrationDiscoveryResponse>, AppError> {
    let req = validate_optional_credentials(payload, "/api/v1/migrations/{id}/discover")?;
    Ok(Json(service::discover_source(&state, id, req).await?))
}

pub async fn match_migration_items(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MigrationMatchResponse>, AppError> {
    Ok(Json(service::match_migration_items(&state, id).await?))
}

pub async fn get_user_mapping_options(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MigrationUserMappingOptionsResponse>, AppError> {
    Ok(Json(service::get_user_mapping_options(&state, id).await?))
}

pub async fn upload_plex_database(
    _auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<MigrationActionResponse>, AppError> {
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::from(MigrationError::InvalidPlexDatabase(e.to_string())))?
    {
        if field.name() != Some("file") {
            continue;
        }

        let original_filename = field
            .file_name()
            .unwrap_or("com.plexapp.plugins.library.db")
            .to_string();
        let target = service::prepare_plex_upload(&state, id, &original_filename).await?;
        let mut file = tokio::fs::File::create(&target.temp_path)
            .await
            .map_err(|e| AppError::from(MigrationError::InvalidPlexDatabase(e.to_string())))?;
        let mut file_size_bytes = 0_u64;

        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| AppError::from(MigrationError::InvalidPlexDatabase(e.to_string())))?
        {
            file_size_bytes = file_size_bytes.saturating_add(chunk.len() as u64);
            if file_size_bytes > service::MAX_PLEX_DATABASE_BYTES {
                let _ = tokio::fs::remove_file(&target.temp_path).await;
                return Err(MigrationError::PlexDatabaseTooLarge.into());
            }
            file.write_all(&chunk)
                .await
                .map_err(|e| AppError::from(MigrationError::InvalidPlexDatabase(e.to_string())))?;
        }

        file.flush()
            .await
            .map_err(|e| AppError::from(MigrationError::InvalidPlexDatabase(e.to_string())))?;
        drop(file);

        let response = service::complete_plex_upload(&state, id, target, file_size_bytes).await?;
        return Ok(Json(response));
    }

    Err(AppError::Validation {
        errors: vec![FieldError {
            field: "file".to_string(),
            code: "required".to_string(),
            message: "multipart field file is required".to_string(),
        }],
        instance: Some("/api/v1/migrations/{id}/upload".to_string()),
    })
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

fn validate_optional_credentials(
    payload: Option<Json<MigrationSourceCredentialRequest>>,
    instance: &str,
) -> Result<MigrationSourceCredentialRequest, AppError> {
    let req = payload
        .map(|Json(req)| req)
        .unwrap_or(MigrationSourceCredentialRequest { api_key: None });
    req.validate().map_err(|e| validation_error(e, instance))?;
    Ok(req)
}
