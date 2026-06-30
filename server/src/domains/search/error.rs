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
pub enum SearchError {
    #[error("search query is too long")]
    QueryTooLong,

    #[error("invalid media type: {0}")]
    InvalidMediaType(String),

    #[error("invalid year: {0}")]
    InvalidYear(i32),

    #[error("invalid rating threshold: {0}")]
    InvalidRating(f32),

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
