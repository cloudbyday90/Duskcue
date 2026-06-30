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

use std::sync::OnceLock;
use std::time::Instant;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::HeaderName;
use axum::routing::get;
use serde_json::{Value, json};
use tower_http::request_id::PropagateRequestIdLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::cache::{
    ARTWORK_CACHE_CONTROL, LIBRARY_CONFIG_CACHE_CONTROL, MEDIA_METADATA_CACHE_CONTROL,
    NO_STORE_CACHE_CONTROL, cache_control_layer, conditional_etag,
};
use crate::middleware::{
    REQUEST_ID_HEADER, build_compression_layer, build_cors_layer, build_security_headers,
    build_set_request_id_layer, metrics_subnet_guard, rate_limit_global, track_http_metrics,
};
use crate::state::AppState;

static START_TIME: OnceLock<Instant> = OnceLock::new();

async fn health_check(State(state): State<AppState>) -> Json<Value> {
    let db_status = match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => "connected",
        Err(_) => "disconnected",
    };

    let status = if db_status == "connected" {
        "healthy"
    } else {
        "degraded"
    };

    let uptime = START_TIME.get().map(|t| t.elapsed().as_secs()).unwrap_or(0);

    let hw = state.transcode_manager.get_hw_detection();

    Json(json!({
        "status": status,
        "version": env!("CARGO_PKG_VERSION"),
        "database": db_status,
        "uptime_seconds": uptime,
        "hardware_acceleration": {
            "method": hw.method.as_str(),
            "source": hw.source,
            "nvidia_detected": hw.nvidia_detected,
            "vaapi_available": hw.vaapi_available,
            "qsv_available": hw.qsv_available,
            "amf_available": hw.amf_available,
            "videotoolbox_available": hw.videotoolbox_available,
            "verified_encoders": hw.verified_encoders,
        }
    }))
}

async fn live_check() -> Json<Value> {
    Json(json!({
        "status": "alive",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn metrics_handler(State(state): State<AppState>) -> String {
    state.metrics_handle.render()
}

pub fn build_router(state: AppState) -> Router<AppState> {
    let _ = START_TIME.set(Instant::now());

    let config = state.runtime_config.load();

    let set_request_id = build_set_request_id_layer();
    let propagate_request_id =
        PropagateRequestIdLayer::new(HeaderName::from_static(REQUEST_ID_HEADER));

    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(
            DefaultMakeSpan::new()
                .level(Level::INFO)
                .include_headers(false),
        )
        .on_response(
            DefaultOnResponse::new()
                .level(Level::INFO)
                .latency_unit(tower_http::LatencyUnit::Millis),
        );

    let cors_layer = build_cors_layer(&config.auth.network_mode, &config.security.allowed_origins);

    let compression_layer = build_compression_layer();
    let security_headers = build_security_headers(&config.auth.network_mode);

    drop(config);

    let mut router: Router<AppState> = Router::new()
        .route(
            "/health",
            get(health_check).route_layer(cache_control_layer(NO_STORE_CACHE_CONTROL)),
        )
        .route(
            "/health/live",
            get(live_check).route_layer(cache_control_layer(NO_STORE_CACHE_CONTROL)),
        )
        .route(
            "/health/ready",
            get(health_check).route_layer(cache_control_layer(NO_STORE_CACHE_CONTROL)),
        )
        .route(
            "/metrics",
            get(metrics_handler)
                .route_layer(cache_control_layer(NO_STORE_CACHE_CONTROL))
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    metrics_subnet_guard,
                )),
        )
        .route(
            "/api/v1/media-items/{id}",
            get(crate::domains::media::handlers::get_media_item)
                .route_layer(cache_control_layer(MEDIA_METADATA_CACHE_CONTROL))
                .route_layer(axum::middleware::from_fn(conditional_etag)),
        )
        .route(
            "/api/v1/items/{id}/artwork/{type}",
            get(crate::domains::media::handlers::get_artwork)
                .route_layer(cache_control_layer(ARTWORK_CACHE_CONTROL))
                .route_layer(axum::middleware::from_fn(conditional_etag)),
        )
        .route(
            "/api/v1/libraries/{id}",
            get(crate::domains::libraries::handlers::get_library)
                .route_layer(cache_control_layer(LIBRARY_CONFIG_CACHE_CONTROL))
                .route_layer(axum::middleware::from_fn(conditional_etag)),
        )
        .route(
            "/api/v1/server/config",
            get(crate::domains::system::handlers::get_server_config)
                .route_layer(cache_control_layer(NO_STORE_CACHE_CONTROL))
                .route_layer(axum::middleware::from_fn(conditional_etag)),
        )
        .route(
            "/api/v1/server/config/{group}",
            get(crate::domains::system::handlers::get_config_group)
                .route_layer(cache_control_layer(NO_STORE_CACHE_CONTROL)),
        )
        .route(
            "/api/v1/events",
            get(crate::services::events_handler::events_handler),
        )
        .merge(crate::domains::auth::router(state.clone()))
        .merge(crate::domains::users::router(state.clone()))
        .merge(crate::domains::libraries::router(state.clone()))
        .merge(crate::domains::media::router(state.clone()))
        .merge(crate::domains::notifications::router(state.clone()))
        .merge(crate::domains::system::router(state.clone()))
        .merge(crate::domains::playback::router(state.clone()))
        .merge(crate::domains::quality::router(state.clone()))
        .merge(crate::domains::search::router(state.clone()))
        .merge(crate::domains::subtitles::router(state.clone()))
        .merge(crate::domains::segments::router(state.clone()))
        .merge(crate::domains::storyboards::router(state.clone()))
        .merge(crate::domains::analytics::router(state.clone()))
        .merge(crate::domains::trakt::router(state.clone()))
        .merge(crate::domains::overlays::router(state.clone()))
        .merge(crate::domains::collections::router(state.clone()))
        .merge(crate::domains::posters::router(state.clone()))
        .merge(crate::domains::backup::router(state.clone()))
        .merge(crate::domains::migration::router(state.clone()))
        .merge(crate::domains::tv::router(state.clone()));
    // Phase 13: .merge(crate::domains::system::router())

    router = router
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit_global,
        ))
        .layer(compression_layer);

    for header_layer in security_headers {
        router = router.layer(header_layer);
    }

    router = router
        .layer(cors_layer)
        .layer(axum::middleware::from_fn(track_http_metrics))
        .layer(trace_layer)
        .layer(propagate_request_id)
        .layer(set_request_id);

    router
}
