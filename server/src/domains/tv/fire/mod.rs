// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

pub mod error;
pub mod handlers;
pub mod service;
pub mod types;

use axum::Router;
use axum::routing::get;

use crate::cache::cache_control_layer;
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/tv/fire/catalog/{platform_content_id}",
            get(handlers::get_catalog_mapping).put(handlers::put_catalog_mapping),
        )
        .route(
            "/api/v1/tv/fire/playback/{session_id}/authorization",
            get(handlers::get_playback_authorization),
        )
        .route_layer(cache_control_layer("private, no-store"))
        .with_state(state)
}
