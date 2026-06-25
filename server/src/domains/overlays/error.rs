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
pub enum OverlayError {
    #[error("overlay definition not found")]
    NotFound,

    #[error("invalid overlay conditions: {0}")]
    InvalidConditions(String),

    #[error("invalid text template: {0}")]
    InvalidTextTemplate(String),

    #[error("overlay image file not found or unreadable: {0}")]
    ImageFileNotFound(String),

    #[error("overlay application already in progress")]
    ApplicationInProgress,

    #[error("overlay compositing failed: {0}")]
    CompositingFailed(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
