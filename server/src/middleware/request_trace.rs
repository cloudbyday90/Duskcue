// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::extract::Request;
use tower_http::trace::{DefaultMakeSpan, MakeSpan};
use tracing::{Level, Span};

pub(crate) fn span(request: &Request) -> Span {
    if request.uri().path().starts_with("/api/v1/live-tv/catalog/") {
        tracing::info_span!("request", method = %request.method(), route = "/api/v1/live-tv/catalog/*", version = ?request.version())
    } else {
        DefaultMakeSpan::new()
            .level(Level::INFO)
            .include_headers(false)
            .make_span(request)
    }
}
