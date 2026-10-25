// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

mod authority;
pub mod error;
mod handlers;
mod policy;
mod preconditions;
pub mod secrets;
mod service;
mod storage;
mod types;

use crate::{cache::conditional_etag, state::AppState};
use axum::{Router, extract::DefaultBodyLimit, middleware, routing::get};

pub(super) fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/live-tv/households/{household_id}/assignment",
            get(handlers::admin_read)
                .put(handlers::create)
                .patch(handlers::admin_patch)
                .delete(handlers::remove),
        )
        .route(
            "/api/v1/household-management/live-tv",
            get(handlers::owner_read).patch(handlers::owner_patch),
        )
        .route(
            "/api/v1/household-management/live-tv/profiles",
            get(handlers::profiles),
        )
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn(conditional_etag))
        .with_state(state)
}
