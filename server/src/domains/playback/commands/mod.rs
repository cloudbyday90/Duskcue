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
mod settlement;
mod starting;
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
    state
        .vod_commands
        .mounted
        .store(true, std::sync::atomic::Ordering::Release);
    Router::new()
        .route("/api/v1/playback-commands", post(handlers::start))
        .route("/api/v1/playback-commands/{id}", get(handlers::status))
        .route(
            "/api/v1/playback-commands/{id}/renew",
            post(handlers::renew),
        )
        .route("/api/v1/playback-commands/{id}/stop", post(handlers::stop))
        .route(
            "/api/v1/playback-commands/{id}/withdraw",
            post(settlement::handlers::withdraw),
        )
        .route(
            "/api/v1/playback-commands/{id}/settlement",
            get(settlement::handlers::read),
        )
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

pub(super) async fn capability(state: &AppState) -> Option<serde_json::Value> {
    if !state
        .vod_commands
        .mounted
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return None;
    }
    let owner = state.playback_admission.get()?;
    owner
        .sql(false, |conn| {
            Box::pin(async move { settlement::compatible(conn).await })
        })
        .await
        .ok()?;
    Some(
        serde_json::json!({"protocol":types::PROTOCOL,"features":["start-settlement","conditional-controls","checkpoints","lease-observation"]}),
    )
}
