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
pub enum StoryboardError {
    #[error("media item not found: {media_item_id}")]
    MediaItemNotFound { media_item_id: uuid::Uuid },

    #[error("media file not found: {media_file_id}")]
    MediaFileNotFound { media_file_id: uuid::Uuid },

    #[error("storyboard not found for media item: {media_item_id}")]
    StoryboardNotFound { media_item_id: uuid::Uuid },

    #[error("library not found: {library_id}")]
    LibraryNotFound { library_id: uuid::Uuid },

    #[error("storyboard generation already in progress for media file {media_file_id}")]
    GenerationAlreadyInProgress { media_file_id: uuid::Uuid },

    #[error("invalid sprite filename: {0}")]
    InvalidSpriteFilename(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
