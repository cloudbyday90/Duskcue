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

#[derive(Error, Debug)]
pub enum CollectionsError {
    #[error("collection not found")]
    NotFound,

    #[error("collection name already exists in this library")]
    NameAlreadyExists,

    #[error("collection sync already in progress")]
    SyncInProgress,

    #[error("invalid dynamic collection configuration: {0}")]
    InvalidDynamicConfig(String),

    #[error("invalid smart filter syntax: {0}")]
    InvalidSmartFilter(String),

    #[error("external builder source unavailable: {0}")]
    ExternalSourceUnavailable(String),

    #[error("external API rate limit exceeded during collection sync")]
    ExternalRateLimited,

    #[error("collection template not found")]
    TemplateNotFound,

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
