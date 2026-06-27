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

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("invalid backup configuration: {0}")]
    InvalidConfig(String),

    #[error("backup operation already in progress")]
    OperationInProgress,

    #[error("backup command unavailable: {tool}: {reason}")]
    CommandUnavailable { tool: String, reason: String },

    #[error("backup command timed out: {tool} after {timeout_seconds}s")]
    CommandTimeout { tool: String, timeout_seconds: u64 },

    #[error("backup command failed: {tool} exit={exit_code:?}: {stderr}")]
    CommandFailed {
        tool: String,
        exit_code: Option<i32>,
        stderr: String,
    },

    #[error("backup verification failed: {0}")]
    VerificationFailed(String),

    #[error("backup I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
