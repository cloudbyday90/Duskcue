pub mod error;
pub mod handlers;
pub mod service;
pub mod types;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::post;

use crate::cache::{NO_STORE_CACHE_CONTROL, cache_control_layer};
use crate::state::AppState;

pub const VALIDATE_CONNECTION_PATH: &str = "/api/v1/live-tv/connections/validate";
pub const CONNECTION_BODY_LIMIT: usize = 16 * 1024;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            VALIDATE_CONNECTION_PATH,
            post(handlers::validate_connection),
        )
        .layer(DefaultBodyLimit::max(CONNECTION_BODY_LIMIT))
        .layer(cache_control_layer(NO_STORE_CACHE_CONTROL))
        .layer(axum::middleware::map_response(handlers::problem_media_type))
        .with_state(state)
}

#[cfg(test)]
mod http_tests;
#[cfg(test)]
mod service_tests;
