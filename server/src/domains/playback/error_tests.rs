// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::PlaybackError;
use crate::error::AppError;

#[tokio::test]
async fn unsupported_hdr_is_a_public_typed_refusal_without_internal_details() {
    let response = AppError::Playback(PlaybackError::HdrConversionUnavailable).into_response();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response.headers()["content-type"], "application/json");
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let problem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(problem["type"], "/errors/play_020");
    assert_eq!(problem["title"], "PLAY_020");
    assert_eq!(problem["status"], 422);
    assert_eq!(
        problem["detail"],
        "HDR conversion is not supported on this device yet."
    );
    assert!(problem.get("debug_detail").is_none());
}
