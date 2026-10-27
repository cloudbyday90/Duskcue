// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

mod cleanup_auth;
mod database;
mod delivery;
pub mod error;
pub mod handlers;
mod ownership;
mod preparation;
mod representation;
pub mod service;
pub mod types;
pub(crate) use ownership::Commands;

#[cfg(test)]
mod tests;

use crate::{
    cache::{NO_STORE_CACHE_CONTROL, cache_control_layer},
    state::AppState,
};
use axum::{
    Router,
    routing::{get, post},
};

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/playback-commands", post(handlers::start))
        .route("/api/v1/playback-commands/{id}", get(handlers::status))
        .route(
            "/api/v1/playback-commands/{id}/renew",
            post(handlers::renew),
        )
        .route("/api/v1/playback-commands/{id}/stop", post(handlers::stop))
        .route(
            "/api/v1/playback-commands/{id}/retirement",
            get(handlers::retirement),
        )
        .route("/api/v1/playback-commands/{id}/file", get(handlers::file))
        .route(
            "/api/v1/playback-commands/{id}/hls/manifest.m3u8",
            get(handlers::manifest),
        )
        .route(
            "/api/v1/playback-commands/{id}/hls/{rendition}/index.m3u8",
            get(handlers::playlist),
        )
        .route(
            "/api/v1/playback-commands/{id}/hls/{rendition}/{segment}",
            get(handlers::segment),
        )
        .layer(cache_control_layer(NO_STORE_CACHE_CONTROL))
        .layer(axum::middleware::map_response(
            crate::middleware::private_responses::problem_media_type,
        ))
        .with_state(state)
}
