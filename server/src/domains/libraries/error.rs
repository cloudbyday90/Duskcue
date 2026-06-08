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
pub enum LibrariesError {
    #[error("library not found")]
    NotFound,

    #[error("library name already exists: {0}")]
    NameExists(String),

    #[error("library scan already in progress")]
    ScanInProgress,

    #[error("root path does not exist: {0}")]
    RootPathNotFound(String),

    #[error("cannot delete library with existing media items")]
    CannotDeleteWithMedia,

    #[error("scan already in progress for this library")]
    ScanAlreadyInProgress,

    #[error("filesystem watcher failed to start")]
    FilesystemWatcherFailed,

    #[error(".media-match file is invalid or unreadable")]
    MediaMatchInvalid,

    #[error("NFO file is invalid or contains no usable provider IDs")]
    NfoInvalid,

    #[error("provider ID tag in folder/filename is malformed: {0}")]
    ProviderIdTagMalformed(String),

    #[error("TMDB metadata provider unavailable during enrichment")]
    TmdbUnavailable,

    #[error("TVDB authentication failure")]
    TvdbAuthFailed,

    #[error("metadata provider rate limit exceeded")]
    ProviderRateLimited,

    #[error("metadata provider response validation failure")]
    ProviderResponseInvalid,

    #[error("library path not found")]
    PathNotFound,

    #[error("path already exists for this library: {0}")]
    PathExists(String),

    #[error("cannot delete the default library path")]
    CannotDeleteDefaultPath,

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
