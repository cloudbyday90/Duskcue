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

pub use error::CollectionsError;

use axum::Router;
use axum::routing::{get, post, put};

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/collections",
            get(handlers::list_collections).post(handlers::create_collection),
        )
        .route("/api/v1/collections/sync", post(handlers::sync_collections))
        .route(
            "/api/v1/collections/templates",
            get(handlers::list_templates).post(handlers::import_template),
        )
        .route(
            "/api/v1/collections/{id}",
            get(handlers::get_collection)
                .patch(handlers::update_collection)
                .delete(handlers::delete_collection),
        )
        .route(
            "/api/v1/collections/{id}/items",
            get(handlers::list_collection_items).post(handlers::add_collection_items),
        )
        .route(
            "/api/v1/collections/{id}/items/reorder",
            put(handlers::reorder_collection_items),
        )
        .route(
            "/api/v1/collections/{id}/items/{media_item_id}",
            axum::routing::delete(handlers::remove_collection_item),
        )
        .route(
            "/api/v1/collections/{id}/sync",
            post(handlers::sync_collection),
        )
        .with_state(state)
}
