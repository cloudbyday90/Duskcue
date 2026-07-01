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

pub use error::DownloadError;

use axum::Router;
use axum::routing::{delete, get, post};

use crate::cache::{NO_STORE_CACHE_CONTROL, cache_control_layer};
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/downloads/plan/{media_item_id}",
            get(handlers::get_download_plan),
        )
        .route(
            "/api/v1/downloads/jobs",
            post(handlers::create_download_job),
        )
        .route(
            "/api/v1/downloads/jobs/{id}",
            get(handlers::get_download_job),
        )
        .route(
            "/api/v1/downloads/jobs/{id}/cancel",
            post(handlers::cancel_download_job),
        )
        .route(
            "/api/v1/downloads/inventory",
            get(handlers::list_download_inventory),
        )
        .route(
            "/api/v1/downloads/admin/inventory",
            get(handlers::list_admin_download_inventory),
        )
        .route(
            "/api/v1/downloads/packages/{id}",
            delete(handlers::delete_download_package),
        )
        .route(
            "/api/v1/downloads/packages/{id}/renew",
            post(handlers::renew_download_package),
        )
        .route(
            "/api/v1/downloads/packages/{id}/manifest",
            get(handlers::get_package_manifest),
        )
        .route(
            "/api/v1/downloads/packages/{id}/transfer-urls",
            post(handlers::create_package_transfer_urls),
        )
        .route(
            "/api/v1/downloads/packages/{id}/files/{*file_path}",
            get(handlers::serve_package_file),
        )
        .route(
            "/api/v1/downloads/sync",
            post(handlers::sync_download_state),
        )
        .route_layer(cache_control_layer(NO_STORE_CACHE_CONTROL))
        .with_state(state)
}
