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

pub use error::SegmentError;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/items/{item_id}/segments",
            get(handlers::list_segments).post(handlers::create_segment),
        )
        .route(
            "/api/v1/items/{item_id}/segments/{segment_id}",
            axum::routing::put(handlers::update_segment).delete(handlers::delete_segment),
        )
        .route(
            "/api/v1/libraries/{library_id}/analyze-segments",
            post(handlers::analyze_library_segments),
        )
        .with_state(state)
}
