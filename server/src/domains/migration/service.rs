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

use uuid::Uuid;

use crate::state::AppState;

use super::error::MigrationError;
use super::types::*;

pub fn validate_platform(value: &str) -> Result<(), MigrationError> {
    if VALID_MIGRATION_PLATFORMS.contains(&value) {
        Ok(())
    } else {
        Err(MigrationError::InvalidSourceConfiguration(format!(
            "invalid platform: {value}"
        )))
    }
}

pub fn validate_status(value: &str) -> Result<(), MigrationError> {
    if VALID_MIGRATION_STATUSES.contains(&value) {
        Ok(())
    } else {
        Err(MigrationError::InvalidSourceConfiguration(format!(
            "invalid status: {value}"
        )))
    }
}

pub async fn create_migration_source(
    _state: &AppState,
    request: CreateMigrationSourceRequest,
) -> Result<MigrationSourceResponse, MigrationError> {
    validate_platform(&request.platform)?;
    Err(MigrationError::NotImplemented("create migration source"))
}

pub async fn list_migration_sources(
    _state: &AppState,
    query: ListMigrationSourcesQuery,
    _page: u32,
    _page_size: u32,
) -> Result<MigrationSourceListResponse, MigrationError> {
    if let Some(platform) = query.platform.as_deref() {
        validate_platform(platform)?;
    }
    if let Some(status) = query.status.as_deref() {
        validate_status(status)?;
    }
    Err(MigrationError::NotImplemented("list migration sources"))
}

pub async fn get_migration_source(
    _state: &AppState,
    _id: Uuid,
) -> Result<MigrationSourceResponse, MigrationError> {
    Err(MigrationError::NotImplemented("get migration source"))
}

pub async fn delete_migration_source(_state: &AppState, _id: Uuid) -> Result<(), MigrationError> {
    Err(MigrationError::NotImplemented("delete migration source"))
}

pub async fn test_connection(
    _state: &AppState,
    _id: Uuid,
) -> Result<MigrationActionResponse, MigrationError> {
    Err(MigrationError::NotImplemented("test migration connection"))
}

pub async fn discover_source(
    _state: &AppState,
    _id: Uuid,
) -> Result<MigrationActionResponse, MigrationError> {
    Err(MigrationError::NotImplemented("discover migration source"))
}

pub async fn save_user_mappings(
    _state: &AppState,
    _id: Uuid,
    request: SaveUserMappingsRequest,
) -> Result<MigrationActionResponse, MigrationError> {
    if request.mappings.is_empty() {
        return Err(MigrationError::NoUserMappings);
    }
    Err(MigrationError::NotImplemented(
        "save migration user mappings",
    ))
}

pub async fn start_migration(
    _state: &AppState,
    _id: Uuid,
    _request: StartMigrationRequest,
) -> Result<MigrationActionResponse, MigrationError> {
    Err(MigrationError::NotImplemented("start migration"))
}

pub async fn get_migration_progress(
    _state: &AppState,
    _id: Uuid,
) -> Result<MigrationProgressResponse, MigrationError> {
    Err(MigrationError::NotImplemented("get migration progress"))
}

pub async fn get_unmatched_report(
    _state: &AppState,
    _id: Uuid,
    _query: UnmatchedReportQuery,
    _page: u32,
    _page_size: u32,
) -> Result<UnmatchedReportResponse, MigrationError> {
    Err(MigrationError::NotImplemented(
        "get migration unmatched report",
    ))
}

pub async fn cancel_migration(
    _state: &AppState,
    _id: Uuid,
) -> Result<MigrationActionResponse, MigrationError> {
    Err(MigrationError::NotImplemented("cancel migration"))
}
