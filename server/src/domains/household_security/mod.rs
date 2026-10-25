// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

mod challenges;
pub mod error;
mod factors;
mod grants;
pub mod handlers;
pub mod operator;
pub mod service;
pub mod transactions;
pub mod types;

use crate::{
    cache::{NO_STORE_CACHE_CONTROL, cache_control_layer},
    state::AppState,
};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{delete, get, post},
};

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/household-management/status", get(handlers::status))
        .route(
            "/api/v1/household-management/password",
            post(handlers::password),
        )
        .route(
            "/api/v1/household-management/passkey/start",
            post(handlers::start),
        )
        .route(
            "/api/v1/household-management/passkey/finish",
            post(handlers::finish),
        )
        .route("/api/v1/household-management/grant", delete(handlers::lock))
        .layer(DefaultBodyLimit::max(65536))
        .layer(cache_control_layer(NO_STORE_CACHE_CONTROL))
        .layer(axum::middleware::map_response(handlers::problem_media_type))
        .with_state(state)
}
