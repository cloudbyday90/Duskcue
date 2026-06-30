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

pub use error::SystemError;

use axum::Router;
use axum::routing::{get, post, put};

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/server/config", put(handlers::update_server_config))
        .route(
            "/api/v1/server/config/{group}",
            put(handlers::update_config_group),
        )
        .route(
            "/api/v1/scheduled-tasks",
            get(handlers::list_scheduled_tasks),
        )
        .route(
            "/api/v1/scheduled-tasks/{task_id}",
            get(handlers::get_scheduled_task),
        )
        .route(
            "/api/v1/scheduled-tasks/{task_id}/trigger",
            post(handlers::trigger_scheduled_task),
        )
        .route(
            "/api/v1/scheduled-tasks/{task_id}/cancel",
            post(handlers::cancel_scheduled_task),
        )
        .route(
            "/api/v1/scheduled-tasks/{task_id}/runs",
            get(handlers::list_scheduled_task_runs),
        )
        .route(
            "/api/v1/settings/providers/validate",
            post(handlers::validate_provider_key),
        )
        .with_state(state)
}
