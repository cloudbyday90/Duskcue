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

pub mod error;
pub mod handlers;
pub mod service;
pub mod types;

pub use error::AuthError;

use axum::Router;
use axum::routing::{delete, get, post};

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/setup",
            get(handlers::setup_status).post(handlers::setup),
        )
        .route("/api/v1/setup/status", get(handlers::setup_status))
        .route("/api/v1/auth/invite", post(handlers::auth_invite))
        .route("/api/v1/auth/login", post(handlers::auth_login))
        .route("/api/v1/auth/logout", post(handlers::auth_logout))
        .route("/api/v1/auth/logout-all", post(handlers::auth_logout_all))
        .route(
            "/api/v1/auth/webauthn/start",
            post(handlers::webauthn_start),
        )
        .route(
            "/api/v1/auth/webauthn/finish",
            post(handlers::webauthn_finish),
        )
        .route("/api/v1/auth/totp", post(handlers::totp_verify))
        .route("/api/v1/auth/reauth", post(handlers::reauth))
        .route(
            "/api/v1/auth/reauth/request",
            post(handlers::reauth_request),
        )
        .route("/api/v1/device/code", post(handlers::device_code))
        .route("/api/v1/device/token", post(handlers::device_token))
        .route("/api/v1/device/verify", post(handlers::device_verify))
        .route("/api/v1/user/sessions", get(handlers::list_user_sessions))
        .route(
            "/api/v1/user/sessions/{id}",
            delete(handlers::delete_user_session),
        )
        .route(
            "/api/v1/user/sign-out-everywhere",
            post(handlers::sign_out_everywhere),
        )
        .route(
            "/api/v1/user/request-reauth",
            post(handlers::request_reauth),
        )
        .route("/api/v1/user/passkeys", get(handlers::passkey_list))
        .route(
            "/api/v1/user/passkeys/register/start",
            post(handlers::passkey_register_start),
        )
        .route(
            "/api/v1/user/passkeys/register/finish",
            post(handlers::passkey_register_finish),
        )
        .route(
            "/api/v1/user/passkeys/{id}",
            delete(handlers::passkey_delete),
        )
        .route(
            "/api/v1/invitations",
            get(handlers::list_invitations).post(handlers::create_invitation),
        )
        .route(
            "/api/v1/invitations/{id}",
            delete(handlers::revoke_invitation),
        )
        .route(
            "/api/v1/invitations/{id}/resend",
            post(handlers::resend_invitation),
        )
        .route(
            "/api/v1/auth/capabilities",
            get(handlers::list_capabilities),
        )
        .route(
            "/api/v1/users/{id}/capabilities",
            get(handlers::get_user_capabilities).put(handlers::update_user_capabilities),
        )
        .with_state(state)
}
