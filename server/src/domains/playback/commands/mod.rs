// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

mod cleanup_auth;
mod control;
mod database;
mod delivery;
pub mod error;
mod execution;
pub mod handlers;
mod output;
mod ownership;
mod preparation;
mod publication;
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
        .route(
            "/api/v1/playback-commands/{id}/configuration",
            get(control::handlers::configuration).post(control::handlers::change),
        )
        .route(
            "/api/v1/playback-commands/{id}/checkpoint",
            get(control::handlers::read_checkpoint).post(control::handlers::checkpoint),
        )
        .route(
            "/api/v1/playback-commands/{id}/operations/{operation}",
            get(control::handlers::operation),
        )
        .route(
            "/api/v1/playback-commands/{id}/operations/{operation}/retirement",
            get(control::handlers::retirement),
        )
        .route(
            "/api/v1/playback-commands/{id}/outputs/{output}/file",
            get(handlers::file),
        )
        .route(
            "/api/v1/playback-commands/{id}/outputs/{output}/hls/manifest.m3u8",
            get(handlers::manifest),
        )
        .route(
            "/api/v1/playback-commands/{id}/outputs/{output}/hls/{rendition}/index.m3u8",
            get(handlers::playlist),
        )
        .route(
            "/api/v1/playback-commands/{id}/outputs/{output}/hls/{rendition}/{segment}",
            get(handlers::segment),
        )
        .layer(cache_control_layer(NO_STORE_CACHE_CONTROL))
        .layer(axum::middleware::map_response(
            crate::middleware::private_responses::problem_media_type,
        ))
        .with_state(state)
}
