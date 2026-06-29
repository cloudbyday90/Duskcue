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

use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("migration source not found: {0}")]
    NotFound(Uuid),

    #[error("migration already in progress: {0}")]
    AlreadyInProgress(Uuid),

    #[error("source platform unreachable: {0}")]
    SourceUnreachable(String),

    #[error("invalid source configuration: {0}")]
    InvalidSourceConfiguration(String),

    #[error("invalid Plex database file: {0}")]
    InvalidPlexDatabase(String),

    #[error("user mapping conflict: {0}")]
    UserMappingConflict(String),

    #[error("no user mappings provided")]
    NoUserMappings,

    #[error("no watch data found on source platform")]
    NoWatchData,

    #[error("Plex database file too large")]
    PlexDatabaseTooLarge,

    #[error("insufficient disk space for Plex database upload")]
    InsufficientDiskSpace,

    #[error("migration feature not implemented yet: {0}")]
    NotImplemented(&'static str),

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
