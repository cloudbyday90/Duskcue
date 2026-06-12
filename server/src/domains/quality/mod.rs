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

pub use error::QualityError;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/device/capabilities",
            post(handlers::report_capabilities).get(handlers::get_capabilities),
        )
        .route(
            "/api/v1/device/capability-tests",
            get(handlers::list_capability_tests),
        )
        .route(
            "/api/v1/device/capability-tests/start",
            post(handlers::start_wizard),
        )
        .route(
            "/api/v1/device/capability-tests/{test_id}/result",
            post(handlers::submit_wizard_result),
        )
        .route(
            "/api/v1/probe/bandwidth",
            get(handlers::get_bandwidth_probe),
        )
        .route(
            "/api/v1/probe/bandwidth/result",
            post(handlers::submit_bandwidth_probe_result),
        )
        .route(
            "/api/v1/playback/telemetry",
            post(handlers::submit_telemetry),
        )
        .route(
            "/api/v1/playback/qoe",
            post(handlers::submit_qoe),
        )
        .route(
            "/api/v1/admin/quality/network",
            get(handlers::admin_network_summary),
        )
        .route(
            "/api/v1/admin/quality/devices",
            get(handlers::admin_device_summary),
        )
        .route(
            "/api/v1/admin/quality/qoe",
            get(handlers::admin_qoe_summary),
        )
        .route(
            "/api/v1/admin/quality/transcodes",
            get(handlers::admin_transcode_breakdown),
        )
        .with_state(state)
}
