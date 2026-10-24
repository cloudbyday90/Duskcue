use serde_json::{Value, json};

use super::service::validate_connection;
use super::types::{ConnectionValidationRequest, MAX_FEED_URL_BYTES};

fn request(playlist_url: &str, guide_url: &str) -> ConnectionValidationRequest {
    ConnectionValidationRequest {
        playlist_url: playlist_url.into(),
        guide_url: guide_url.into(),
    }
}

fn accepted(playlist: &str, guide: &str) -> Value {
    match validate_connection(request(playlist, guide)) {
        Ok(response) => serde_json::to_value(response).unwrap(),
        Err(error) => panic!("unexpected validation failure: {error}"),
    }
}

#[test]
fn accepts_opaque_dispatcharr_feeds_without_returning_credentials() {
    let response = accepted(
        "http://dispatcharr.internal:9191/reverse-proxy/private-path?token=playlist-secret",
        "https://guide.invalid/other-private-path?key=guide-secret",
    );
    assert_eq!(
        response,
        json!({
            "validation_scope": "configuration_only",
            "network_checked": false,
            "saved": false,
            "playlist": { "transport": "http" },
            "guide": { "transport": "https" }
        })
    );
}

#[test]
fn accepts_local_and_international_hosts_without_resolving_them() {
    for url in [
        "http://192.168.1.50:9191/output/m3u",
        "http://[fd00::1]:9191/output/m3u",
        "http://127.0.0.1:9191/output/m3u",
        "http://localhost:9191/output/m3u",
        "HTTPS://dispatcharr.invalid/feed?token=a%2Fb%3D",
        "https://例え.invalid/guide",
    ] {
        accepted(url, "https://guide.invalid/epg");
    }
}

#[test]
fn rejects_ambiguous_and_non_http_addresses_without_echoing_input() {
    for url in [
        "",
        "file:///private-secret",
        "ftp://secret.invalid/feed",
        "//secret.invalid/feed",
        "http:secret.invalid/feed",
        "http:///secret.invalid/feed",
        "http://secret.invalid:invalid/feed",
        "http://owner:private-secret@dispatcharr.invalid/feed",
        "http://@dispatcharr.invalid/feed",
        "https://dispatcharr.invalid/feed#private-secret",
        "https://dispatcharr.invalid/feed#",
        " https://secret.invalid/feed",
        "https://secret.invalid/a b",
        "https://secret.invalid/\tprivate-secret",
        "https://secret.invalid/\r\nprivate-secret",
        "https://secret.invalid/\0private-secret",
        "https://secret.invalid/\\private-secret",
    ] {
        let error = validate_connection(request(url, "https://guide.invalid/epg"))
            .err()
            .expect("unsafe configuration accepted");
        assert_eq!(error.fields().len(), 1);
        assert_eq!(error.fields()[0].field, "playlist_url");
        let safe_error = format!("{error} {error:?}");
        for secret in ["private-secret", "secret.invalid", "owner:"] {
            assert!(!safe_error.contains(secret));
        }
    }
}

#[test]
fn applies_byte_limit_to_multibyte_urls_and_accepts_exact_limit() {
    let prefix = "https://dispatcharr.invalid/";
    let exact = format!("{prefix}{}", "a".repeat(MAX_FEED_URL_BYTES - prefix.len()));
    accepted(&exact, "http://guide.invalid/feed");
    for oversized in [format!("{exact}x"), format!("{prefix}{}", "é".repeat(3000))] {
        let error = validate_connection(request("https://playlist.invalid/feed", &oversized))
            .err()
            .expect("oversized URL accepted");
        assert_eq!(error.fields()[0].field, "guide_url");
    }
}

#[test]
fn reports_both_invalid_fields_in_stable_order() {
    let error = validate_connection(request(
        "file:///playlist",
        "https://guide.invalid/feed#secret",
    ))
    .err()
    .unwrap();
    assert_eq!(
        error
            .fields()
            .iter()
            .map(|field| field.field.as_str())
            .collect::<Vec<_>>(),
        ["playlist_url", "guide_url"]
    );
}
