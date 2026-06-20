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
pub enum SegmentError {
    #[error("media item not found: {media_item_id}")]
    MediaItemNotFound { media_item_id: uuid::Uuid },

    #[error("segment not found: {segment_id}")]
    SegmentNotFound { segment_id: uuid::Uuid },

    #[error("library not found: {library_id}")]
    LibraryNotFound { library_id: uuid::Uuid },

    #[error("invalid segment type: {0}")]
    InvalidSegmentType(String),

    #[error("invalid segment source: {0}")]
    InvalidSegmentSource(String),

    #[error("invalid timestamps: start_ms={start_ms}, end_ms={end_ms}, skip_to_ms={skip_to_ms}")]
    InvalidTimestamps {
        start_ms: i32,
        end_ms: i32,
        skip_to_ms: i32,
    },

    #[error("manual segment already exists for type {segment_type} on this item")]
    ManualSegmentExists { segment_type: String },

    #[error("segment analysis already in progress for library {library_id}")]
    AnalysisAlreadyInProgress { library_id: uuid::Uuid },

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
