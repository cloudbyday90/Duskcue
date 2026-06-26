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

pub use error::PosterError;

use axum::Router;
use axum::routing::{patch, post};

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/posters/assets/scan",
            post(handlers::scan_asset_directory),
        )
        .route(
            "/api/v1/posters/community/import",
            post(handlers::import_community_pack),
        )
        .route(
            "/api/v1/posters/{id}/lock",
            patch(handlers::set_artwork_lock),
        )
        .route(
            "/api/v1/posters/{id}/select",
            post(handlers::select_artwork),
        )
        .with_state(state)
}
