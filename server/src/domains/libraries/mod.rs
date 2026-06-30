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

pub use error::LibrariesError;

use axum::Router;
use axum::routing::{get, patch, post};

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/libraries",
            get(handlers::list_libraries).post(handlers::create_library),
        )
        .route(
            "/api/v1/libraries/{id}",
            patch(handlers::update_library).delete(handlers::delete_library),
        )
        .route("/api/v1/libraries/{id}/scan", post(handlers::scan_library))
        .route(
            "/api/v1/libraries/{id}/items",
            get(handlers::list_library_items),
        )
        .route(
            "/api/v1/libraries/{id}/paths",
            get(handlers::list_library_paths).post(handlers::create_library_path),
        )
        .route(
            "/api/v1/libraries/{id}/paths/{path_id}",
            get(handlers::get_library_path)
                .patch(handlers::update_library_path)
                .delete(handlers::delete_library_path),
        )
        .with_state(state)
}
