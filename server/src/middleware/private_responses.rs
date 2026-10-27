// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::{
    extract::Request,
    http::{HeaderValue, header},
    middleware::Next,
    response::Response,
};

pub(crate) async fn problem_media_type(mut response: Response) -> Response {
    if (response.status().is_client_error() || response.status().is_server_error())
        && response
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|v| v == "application/json")
    {
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
    }
    response
}

pub(crate) async fn response_policy(request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let private = path == "/api/v1/events"
        || path == "/api/v1/auth/session-context"
        || path == "/api/v1/playback-commands"
        || path.starts_with("/api/v1/playback-commands/")
        || path.starts_with("/api/v1/household-management/")
        || path.starts_with("/api/v1/live-tv/");
    let mut response = next.run(request).await;
    if private {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        return problem_media_type(response).await;
    }
    response
}
