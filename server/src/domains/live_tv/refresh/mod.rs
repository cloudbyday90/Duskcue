// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

mod admission;
pub(crate) mod authority;
mod database;
pub mod error;
mod handlers;
mod manager;
mod representation;
mod service;
mod types;

pub use manager::RefreshManager;

use crate::state::AppState;
use axum::{Router, extract::DefaultBodyLimit, routing::get};

pub(super) fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/live-tv/manual-refresh-control",
            get(handlers::control).put(handlers::configure),
        )
        .route(
            "/api/v1/live-tv/households/{household_id}/refresh",
            get(handlers::resource).post(handlers::start),
        )
        .route(
            "/api/v1/live-tv/households/{household_id}/refresh/status",
            get(handlers::status),
        )
        .layer(DefaultBodyLimit::max(16 * 1024))
        .with_state(state)
}
