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

pub use error::ProfilesError;

use axum::Router;
use axum::routing::{get, patch, post};

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/profiles",
            get(handlers::list_profiles).post(handlers::create_profile),
        )
        .route(
            "/api/v1/profiles/{id}",
            patch(handlers::update_profile).delete(handlers::delete_profile),
        )
        .route(
            "/api/v1/profiles/{id}/switch",
            post(handlers::switch_profile),
        )
        .route(
            "/api/v1/profiles/parent-unlock",
            post(handlers::parent_unlock),
        )
        .route(
            "/api/v1/ambient-channels",
            get(handlers::list_ambient_channels).post(handlers::create_ambient_channel),
        )
        .route(
            "/api/v1/ambient-channels/{id}",
            patch(handlers::update_ambient_channel).delete(handlers::delete_ambient_channel),
        )
        .route(
            "/api/v1/ambient-channels/{id}/items",
            get(handlers::get_ambient_channel_items).put(handlers::replace_ambient_channel_items),
        )
        .route(
            "/api/v1/ambient-channels/{id}/next",
            post(handlers::next_ambient_channel_item),
        )
        .with_state(state)
}
