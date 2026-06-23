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
pub enum TraktError {
    #[error("Trakt account not linked")]
    AccountNotLinked,

    #[error("Trakt API rate limited")]
    RateLimited { retry_after_secs: Option<u32> },

    #[error("Trakt token expired — re-link required")]
    TokenExpired,

    #[error("Trakt API unavailable")]
    ServiceUnavailable,

    #[error("Trakt API timeout")]
    Timeout,

    #[error("device code expired")]
    DeviceCodeExpired,

    #[error("device authorization pending")]
    DeviceCodePending,

    #[error("device authorization denied")]
    DeviceCodeDenied,

    #[error("a sync is already in progress for this user")]
    SyncInProgress,

    #[error("Trakt integration not configured — admin must set client_id and client_secret")]
    NotConfigured,

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
