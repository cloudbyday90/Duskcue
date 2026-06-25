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

pub use error::OverlayError;

use axum::Router;
use axum::routing::{get, post};

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/overlays",
            get(handlers::list_overlays).post(handlers::create_overlay),
        )
        .route("/api/v1/overlays/apply", post(handlers::apply_overlays))
        .route("/api/v1/overlays/preview", post(handlers::preview_overlay))
        .route(
            "/api/v1/overlays/templates",
            get(handlers::list_templates).post(handlers::import_template),
        )
        .route(
            "/api/v1/overlays/{id}",
            get(handlers::get_overlay)
                .patch(handlers::update_overlay)
                .delete(handlers::delete_overlay),
        )
        .with_state(state)
}
