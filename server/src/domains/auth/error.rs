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

use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("passkey not found")]
    PasskeyNotFound,

    #[error("invalid passkey signature")]
    InvalidSignature,

    #[error("TOTP verification failed")]
    TotpFailed,

    #[error("account locked until {until}")]
    AccountLocked { until: DateTime<Utc> },

    #[error("session expired")]
    SessionExpired,

    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("insufficient capabilities: requires {required:?}")]
    InsufficientCapabilities { required: Vec<String> },

    #[error("API key invalid or revoked")]
    ApiKeyInvalid,

    #[error("invite code invalid or expired")]
    InviteCodeInvalid,

    #[error("invite code revoked")]
    InviteCodeRevoked,

    #[error("invite code use limit exceeded")]
    InviteCodeUseLimitExceeded,

    #[error("too many failed authentication attempts")]
    RateLimited,

    #[error("device linking code expired")]
    DeviceLinkingExpired,

    #[error("device linking denied by user")]
    DeviceLinkingDenied,

    #[error("device linking authorization pending")]
    DeviceLinkingPending,

    #[error("device linking slow down")]
    DeviceLinkingSlowDown,

    #[error("re-authentication code invalid or expired")]
    ReauthCodeInvalid,

    #[error("too many re-auth code requests")]
    ReauthRateLimited,

    #[error("setup already complete")]
    SetupAlreadyComplete,

    #[error("setup required")]
    SetupRequired,

    #[error("WebAuthn challenge expired")]
    WebauthnChallengeExpired,

    #[error("WebAuthn registration failed: {reason}")]
    WebauthnRegistrationFailed { reason: String },

    #[error("WebAuthn authentication failed: {reason}")]
    WebauthnAuthenticationFailed { reason: String },

    #[error("password does not meet requirements")]
    PasswordTooWeak,

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
