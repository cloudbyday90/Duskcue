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

pub use error::MigrationError;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};

use crate::state::AppState;

const PLEX_UPLOAD_BODY_LIMIT_BYTES: usize = 10 * 1024 * 1024 * 1024 + 1024 * 1024;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/migrations",
            post(handlers::create_migration_source).get(handlers::list_migration_sources),
        )
        .route(
            "/api/v1/migrations/{id}",
            get(handlers::get_migration_source).delete(handlers::delete_migration_source),
        )
        .route(
            "/api/v1/migrations/{id}/connect",
            post(handlers::test_connection),
        )
        .route(
            "/api/v1/migrations/{id}/discover",
            post(handlers::discover_source),
        )
        .route(
            "/api/v1/migrations/{id}/match",
            post(handlers::match_migration_items),
        )
        .route(
            "/api/v1/migrations/{id}/upload",
            post(handlers::upload_plex_database)
                .layer(DefaultBodyLimit::max(PLEX_UPLOAD_BODY_LIMIT_BYTES)),
        )
        .route(
            "/api/v1/migrations/{id}/map-users",
            get(handlers::get_user_mapping_options).post(handlers::save_user_mappings),
        )
        .route(
            "/api/v1/migrations/{id}/start",
            post(handlers::start_migration),
        )
        .route(
            "/api/v1/migrations/{id}/preflight",
            post(handlers::run_preflight),
        )
        .route(
            "/api/v1/migrations/{id}/progress",
            get(handlers::get_migration_progress),
        )
        .route(
            "/api/v1/migrations/{id}/review",
            get(handlers::get_migration_review),
        )
        .route(
            "/api/v1/migrations/{id}/review.csv",
            get(handlers::export_migration_review_csv),
        )
        .route(
            "/api/v1/migrations/{id}/review/{item_id}",
            post(handlers::resolve_migration_review_item),
        )
        .route(
            "/api/v1/migrations/{id}/unmatched",
            get(handlers::get_unmatched_report),
        )
        .route(
            "/api/v1/migrations/{id}/cancel",
            post(handlers::cancel_migration),
        )
        .with_state(state)
}
