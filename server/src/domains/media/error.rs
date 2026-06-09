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
pub enum MediaError {
    #[error("media item not found")]
    NotFound,

    #[error("media file not found")]
    FileNotFound,

    #[error("media file is unhealthy: {0}")]
    FileUnhealthy(String),

    #[error("artwork not found")]
    ArtworkNotFound,

    #[error("media item already exists in library")]
    AlreadyExists,

    #[error("storyboard not found (not yet generated)")]
    StoryboardNotFound,

    #[error("invalid media type: {0}")]
    InvalidMediaType(String),

    #[error("invalid match state: {0}")]
    InvalidMatchState(String),

    #[error("invalid identification source: {0}")]
    InvalidIdentificationSource(String),

    #[error("series not found for season/episode")]
    SeriesNotFound,

    #[error("season not found")]
    SeasonNotFound,

    #[error("duplicate season number {0} for series")]
    DuplicateSeasonNumber(i32),

    #[error("duplicate episode number {0} for season")]
    DuplicateEpisodeNumber(i32),

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
