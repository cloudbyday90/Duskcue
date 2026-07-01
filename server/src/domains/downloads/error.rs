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
pub enum DownloadError {
    #[error("download access denied")]
    AccessDenied,

    #[error("download policy denied: {0}")]
    PolicyDenied(String),

    #[error("download quota exceeded: {0}")]
    QuotaExceeded(String),

    #[error("unsupported media for offline download: {0}")]
    UnsupportedMedia(String),

    #[error("download storage unavailable: {0}")]
    StorageUnavailable(String),

    #[error("download package expired: {0}")]
    PackageExpired(Uuid),

    #[error("download job cancelled: {0}")]
    JobCancelled(Uuid),

    #[error("download package is not ready: {0}")]
    PackageNotReady(Uuid),

    #[error("download checksum mismatch: {0}")]
    ChecksumMismatch(String),

    #[error("stale download client state: {0}")]
    StaleClientState(String),

    #[error("download job not found: {0}")]
    JobNotFound(Uuid),

    #[error("download package not found: {0}")]
    PackageNotFound(Uuid),

    #[error("invalid download platform: {0}")]
    InvalidPlatform(String),

    #[error("invalid download request: {0}")]
    InvalidRequest(String),

    #[error("download feature not implemented yet: {0}")]
    NotImplemented(&'static str),

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
