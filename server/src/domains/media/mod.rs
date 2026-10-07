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

pub(crate) mod access;
pub mod browsing;
pub mod error;
pub mod handlers;
pub(crate) mod projection;
pub mod service;
pub mod types;

pub use error::MediaError;

use axum::Router;
use axum::routing::{get, patch};

use crate::cache::{NO_STORE_CACHE_CONTROL, cache_control_layer};
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/media-items",
            get(handlers::list_media_items)
                .route_layer(cache_control_layer(NO_STORE_CACHE_CONTROL)),
        )
        .route(
            "/api/v1/media-items/continue-watching",
            get(handlers::list_continue_watching)
                .route_layer(cache_control_layer(NO_STORE_CACHE_CONTROL)),
        )
        .route(
            "/api/v1/media-items/{id}/seasons",
            get(handlers::list_series_seasons)
                .route_layer(cache_control_layer(NO_STORE_CACHE_CONTROL)),
        )
        .route(
            "/api/v1/media-items/{id}/episodes",
            get(handlers::list_season_episodes)
                .route_layer(cache_control_layer(NO_STORE_CACHE_CONTROL)),
        )
        .route(
            "/api/v1/media-items/{id}",
            patch(handlers::update_media_item).delete(handlers::delete_media_item),
        )
        .route(
            "/api/v1/media-items/{id}/files",
            get(handlers::list_media_files),
        )
        .route(
            "/api/v1/media-items/{id}/files/{file_id}",
            get(handlers::get_media_file),
        )
        .with_state(state)
}
