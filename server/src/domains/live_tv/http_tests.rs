use axum::Json;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{DefaultBodyLimit, rejection::JsonRejection};
use axum::http::{Request, StatusCode};
use axum::routing::post;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::cache::{NO_STORE_CACHE_CONTROL, cache_control_layer};

use super::handlers::validate_body;
use super::types::ConnectionValidationRequest;
use super::{CONNECTION_BODY_LIMIT, VALIDATE_CONNECTION_PATH};

fn body_contract_router() -> Router {
    Router::new()
        .route(
            VALIDATE_CONNECTION_PATH,
            post(
                |body: Result<Json<ConnectionValidationRequest>, JsonRejection>| async {
                    validate_body(body)
                },
            ),
        )
        .layer(DefaultBodyLimit::max(CONNECTION_BODY_LIMIT))
        .layer(cache_control_layer(NO_STORE_CACHE_CONTROL))
        .layer(axum::middleware::map_response(
            super::handlers::problem_media_type,
        ))
}

async fn response(body: &str, content_type: Option<&str>) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(VALIDATE_CONNECTION_PATH);
    if let Some(content_type) = content_type {
        request = request.header("content-type", content_type);
    }
    let response = body_contract_router()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let status = response.status();
    if status != StatusCode::OK {
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
    }
    let bytes = to_bytes(response.into_body(), 32768).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn redacts_success_and_semantic_field_errors() {
    let body = json!({
        "playlist_url": "https://private-host.invalid/private-path?token=private-query",
        "guide_url": "http://guide.invalid/private-guide?key=private-key"
    });
    let (status, result) = response(&body.to_string(), Some("application/json")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["network_checked"], false);
    assert_eq!(result["saved"], false);
    assert!(!result.to_string().contains("private"));

    let body = json!({
        "playlist_url": "https://owner:private-password@private-host.invalid/feed",
        "guide_url": "file:///private-path"
    });
    let (status, result) = response(&body.to_string(), Some("application/json")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(result["title"], "VALID_001");
    assert_eq!(result["errors"].as_array().unwrap().len(), 2);
    assert!(!result.to_string().contains("private"));
}

#[tokio::test]
async fn redacts_json_rejections_and_preserves_http_status() {
    for (body, content_type, expected) in [
        (
            "{\"private-secret",
            Some("application/json"),
            StatusCode::BAD_REQUEST,
        ),
        (
            "{\"playlist_url\":\"http://playlist.invalid/feed\",\"guide_url\":\"http://guide.invalid/feed\",\"private-secret\":1}",
            Some("application/json"),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "{\"playlist_url\":{\"private-secret\":true},\"guide_url\":\"http://guide.invalid/feed\"}",
            Some("application/json"),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "{}",
            Some("application/json"),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        ("private-secret", None, StatusCode::UNSUPPORTED_MEDIA_TYPE),
        (
            "private-secret",
            Some("text/plain"),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
    ] {
        let (status, result) = response(body, content_type).await;
        assert_eq!(status, expected);
        assert_eq!(result["status"], expected.as_u16());
        if expected == StatusCode::UNPROCESSABLE_ENTITY {
            assert_eq!(result["errors"][0]["field"], "body");
            assert_eq!(result["errors"][0]["code"], "invalid_body");
        }
        assert!(!result.to_string().contains("private-secret"));
    }
}

#[tokio::test]
async fn rejects_body_over_limit_without_returning_its_content() {
    let body = format!(
        "{{\"private-secret\":\"{}\"}}",
        "x".repeat(CONNECTION_BODY_LIMIT)
    );
    let (status, result) = response(&body, Some("application/json")).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(result["status"], 413);
    assert!(!result.to_string().contains("private-secret"));
}
