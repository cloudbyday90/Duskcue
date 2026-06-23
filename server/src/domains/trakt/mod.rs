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

pub use error::TraktError;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/trakt/account", get(handlers::get_account).delete(handlers::unlink_account))
        .route("/api/v1/trakt/account/link", post(handlers::start_link))
        .route("/api/v1/trakt/account/poll", post(handlers::poll_link))
        .route("/api/v1/trakt/settings", get(handlers::get_settings).put(handlers::update_settings))
        .route("/api/v1/trakt/sync", post(handlers::trigger_sync))
        .route("/api/v1/trakt/sync/status", get(handlers::get_sync_status))
        .route("/api/v1/trakt/history", get(handlers::list_history))
        .route("/api/v1/trakt/ratings", get(handlers::list_ratings))
        .with_state(state)
}
