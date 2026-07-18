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

use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::Request;
use axum::http::{Extensions, HeaderMap, HeaderName, HeaderValue, StatusCode, Version, header};
use axum::middleware::Next;
use axum::response::Response;
use governor::{Quota, RateLimiter, clock::DefaultClock, state::keyed::DefaultKeyedStateStore};
use metrics::{counter, histogram};
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{DefaultPredicate, Predicate};
use tower_http::cors::{Any, CorsLayer};
use tower_http::request_id::{MakeRequestId, RequestId, SetRequestIdLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use uuid::Uuid;

use crate::error::AppError;
use crate::state::{AppState, NetworkMode, RateLimitConfig};

pub const REQUEST_ID_HEADER: &str = "x-request-id";

pub struct RateLimitState {
    pub ip_global: Arc<RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>>,
    pub ip_auth: Arc<RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>>,
    pub user_authenticated: Arc<RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>>,
    pub session_streaming: Arc<RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>>,
    pub user_admin: Arc<RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>>,
}

impl RateLimitState {
    pub fn new(config: &RateLimitConfig) -> Self {
        Self {
            ip_global: Arc::new(RateLimiter::keyed(
                Quota::per_minute(nz(config.global_per_minute, 100))
                    .allow_burst(nz(config.global_burst, 50)),
            )),
            ip_auth: Arc::new(RateLimiter::keyed(
                Quota::per_minute(nz(config.auth_per_minute, 10))
                    .allow_burst(nz(config.auth_burst, 5)),
            )),
            user_authenticated: Arc::new(RateLimiter::keyed(
                Quota::per_minute(nz(config.authenticated_per_minute, 300))
                    .allow_burst(nz(config.authenticated_burst, 100)),
            )),
            session_streaming: Arc::new(RateLimiter::keyed(
                Quota::per_minute(nz(config.streaming_per_minute, 600))
                    .allow_burst(nz(config.streaming_burst, 50)),
            )),
            user_admin: Arc::new(RateLimiter::keyed(
                Quota::per_minute(nz(config.admin_per_minute, 1000))
                    .allow_burst(nz(config.admin_burst, 200)),
            )),
        }
    }

    pub fn from_defaults() -> Self {
        Self::new(&RateLimitConfig::default())
    }
}

fn nz(value: u32, fallback: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap_or_else(|| NonZeroU32::new(fallback).unwrap())
}

#[derive(Clone)]
pub struct UuidV7RequestId;

impl MakeRequestId for UuidV7RequestId {
    fn make_request_id<B>(&mut self, _request: &Request<B>) -> Option<RequestId> {
        let id = Uuid::now_v7().to_string();
        HeaderValue::from_str(&id).ok().map(RequestId::new)
    }
}

pub fn extract_client_ip(
    headers: &axum::http::HeaderMap,
    connect_info: Option<&std::net::SocketAddr>,
) -> Option<IpAddr> {
    let trusted_loopback_proxy = connect_info.is_some_and(|peer| peer.ip().is_loopback());
    if trusted_loopback_proxy {
        if let Some(xff) = headers.get("x-forwarded-for")
            && let Ok(val) = xff.to_str()
            && let Some(first) = val.split(',').next()
            && let Ok(ip) = first.trim().parse::<IpAddr>()
        {
            return Some(ip);
        }

        if let Some(xri) = headers.get("x-real-ip")
            && let Ok(val) = xri.to_str()
            && let Ok(ip) = val.parse::<IpAddr>()
        {
            return Some(ip);
        }
    }
    connect_info.map(|ci| ci.ip())
}

pub async fn rate_limit_global(
    axum::extract::State(state): axum::extract::State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let ci = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>();
    let ip = extract_client_ip(request.headers(), ci.map(|c| &c.0))
        .unwrap_or(IpAddr::from([0, 0, 0, 1]));
    match state.rate_limits.ip_global.check_key(&ip) {
        Ok(()) => Ok(next.run(request).await),
        Err(_) => Err(AppError::RateLimited {
            code: "RATE_LIMITED".to_string(),
        }),
    }
}

pub async fn rate_limit_auth(
    axum::extract::State(state): axum::extract::State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let connect_info = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>();
    let ip = extract_client_ip(request.headers(), connect_info.map(|value| &value.0))
        .unwrap_or(IpAddr::from([0, 0, 0, 1]));

    match state.rate_limits.ip_auth.check_key(&ip) {
        Ok(()) => Ok(next.run(request).await),
        Err(_) => Err(AppError::RateLimited {
            code: "RATE_002".to_string(),
        }),
    }
}

pub fn build_set_request_id_layer() -> SetRequestIdLayer<UuidV7RequestId> {
    SetRequestIdLayer::new(HeaderName::from_static(REQUEST_ID_HEADER), UuidV7RequestId)
}

pub fn build_cors_layer(network_mode: &NetworkMode, allowed_origins: &[String]) -> CorsLayer {
    match network_mode {
        NetworkMode::Local => CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
        NetworkMode::Exposed => {
            let origins: Vec<_> = allowed_origins
                .iter()
                .filter_map(|o| o.parse().ok())
                .collect();
            if origins.is_empty() {
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any)
            } else {
                CorsLayer::new()
                    .allow_origin(origins)
                    .allow_methods(Any)
                    .allow_headers(Any)
                    .allow_credentials(true)
            }
        }
    }
}

pub fn build_security_headers(
    network_mode: &NetworkMode,
) -> Vec<SetResponseHeaderLayer<HeaderValue>> {
    let mut layers = vec![SetResponseHeaderLayer::if_not_present(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    )];

    let csp_value = match network_mode {
        NetworkMode::Local => "default-src 'self' 'unsafe-inline' 'unsafe-eval' blob: data: media:",
        NetworkMode::Exposed => {
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data: blob:; media-src 'self' blob:; object-src 'none'; \
             base-uri 'self'; frame-ancestors 'none'"
        }
    };
    layers.push(SetResponseHeaderLayer::if_not_present(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(csp_value),
    ));

    if matches!(network_mode, NetworkMode::Exposed) {
        layers.push(SetResponseHeaderLayer::if_not_present(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=63072000; includeSubDomains; preload"),
        ));
        layers.push(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ));
        layers.push(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ));
        layers.push(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("geolocation=(), microphone=(), camera=()"),
        ));
    }

    layers
}

pub fn build_compression_layer() -> CompressionLayer<impl Predicate> {
    CompressionLayer::new().compress_when(DefaultPredicate::new().and(
        |_status: StatusCode, _version: Version, headers: &HeaderMap, _extensions: &Extensions| {
            !headers.contains_key(header::ETAG)
        },
    ))
}

pub async fn track_http_metrics(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();

    if path == "/metrics" {
        return next.run(request).await;
    }

    let method = request.method().clone();
    let start = Instant::now();

    let response = next.run(request).await;

    let latency = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    counter!("http_requests_total", "method" => method.to_string(), "status" => status)
        .increment(1);
    histogram!("http_request_duration", "method" => method.to_string()).record(latency);

    response
}

pub async fn metrics_subnet_guard(
    axum::extract::State(state): axum::extract::State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let ci = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>();
    let ip = extract_client_ip(request.headers(), ci.map(|c| &c.0))
        .unwrap_or(IpAddr::from([0, 0, 0, 1]));

    let is_allowed = state
        .metrics_allowed_subnets
        .iter()
        .any(|subnet| subnet.contains(&ip));

    if !is_allowed {
        return Err(AppError::Forbidden(
            "Metrics endpoint access denied".to_string(),
        ));
    }

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, SocketAddr};

    use axum::http::{HeaderMap, HeaderValue};

    use super::extract_client_ip;

    #[test]
    fn direct_peer_cannot_spoof_forwarded_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.10"));
        let peer: SocketAddr = "198.51.100.4:443".parse().unwrap();

        assert_eq!(
            extract_client_ip(&headers, Some(&peer)),
            Some("198.51.100.4".parse::<IpAddr>().unwrap())
        );
    }

    #[test]
    fn loopback_proxy_can_supply_forwarded_ip() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.10, 127.0.0.1"),
        );
        let peer: SocketAddr = "127.0.0.1:48028".parse().unwrap();

        assert_eq!(
            extract_client_ip(&headers, Some(&peer)),
            Some("203.0.113.10".parse::<IpAddr>().unwrap())
        );
    }
}
