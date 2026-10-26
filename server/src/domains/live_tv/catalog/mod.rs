// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

mod admission;
pub(crate) mod authority;
pub(crate) mod cursor;
mod database;
pub mod error;
mod handlers;
pub(crate) mod limits;
#[cfg(all(test, feature = "playback-qualification"))]
mod qualification;
mod query;
mod service;
pub(crate) mod types;

use crate::{
    cache::{NO_STORE_CACHE_CONTROL, cache_control_layer},
    state::AppState,
};
pub use admission::CatalogReads;
use axum::{Router, routing::get};

pub(super) fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/live-tv/catalog/channels", get(handlers::channels))
        .route(
            "/api/v1/live-tv/catalog/channels/{channel_id}/programmes",
            get(handlers::programmes),
        )
        .route(
            "/api/v1/live-tv/catalog/channels/{channel_id}/programmes/unpositionable",
            get(handlers::unpositionable),
        )
        .layer(cache_control_layer(NO_STORE_CACHE_CONTROL))
        .layer(axum::middleware::map_response(
            crate::middleware::private_responses::problem_media_type,
        ))
        .with_state(state)
}

#[cfg(test)]
mod tests;
