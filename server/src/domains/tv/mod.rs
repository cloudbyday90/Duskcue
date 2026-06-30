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

pub use error::TvError;

use axum::Router;
use axum::routing::get;

use crate::cache::{TV_SURFACE_CACHE_CONTROL, cache_control_layer, conditional_etag};
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/users/me/tv-surface",
            get(handlers::get_tv_surface)
                .route_layer(cache_control_layer(TV_SURFACE_CACHE_CONTROL))
                .route_layer(axum::middleware::from_fn(conditional_etag)),
        )
        .route(
            "/api/v1/tv/resolve/{platform_content_id}",
            get(handlers::resolve_platform_content),
        )
        .route("/api/v1/tv/settings", get(handlers::get_tv_settings))
        .route("/api/v1/tv/diagnostics", get(handlers::get_tv_diagnostics))
        .with_state(state)
}
