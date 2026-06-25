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

pub use error::AnalyticsError;

use axum::Router;
use axum::routing::{get, post};

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/analytics/overview", get(handlers::get_overview))
        .route(
            "/api/v1/analytics/play-history",
            get(handlers::get_play_history),
        )
        .route("/api/v1/analytics/top-media", get(handlers::get_top_media))
        .route("/api/v1/analytics/bandwidth", get(handlers::get_bandwidth))
        .route(
            "/api/v1/analytics/concurrent",
            get(handlers::get_concurrent),
        )
        .route(
            "/api/v1/analytics/trust/scores",
            get(handlers::get_trust_scores),
        )
        .route(
            "/api/v1/analytics/trust/events",
            get(handlers::get_trust_events),
        )
        .route(
            "/api/v1/analytics/trust/events/{event_id}/acknowledge",
            post(handlers::acknowledge_event),
        )
        .route(
            "/api/v1/analytics/geoip/status",
            get(handlers::get_geoip_status),
        )
        .with_state(state)
}
