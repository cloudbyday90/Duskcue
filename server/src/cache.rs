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

use axum::body::{Body, to_bytes};
use axum::extract::Request;
use axum::http::header::{CACHE_CONTROL, CONTENT_LENGTH, ETAG, IF_NONE_MATCH};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use ring::digest::{SHA256, digest};
use tower_http::set_header::SetResponseHeaderLayer;

pub const MEDIA_METADATA_CACHE_CONTROL: &str = "private, max-age=300, stale-while-revalidate=600";
pub const LIBRARY_CONFIG_CACHE_CONTROL: &str = "private, max-age=60, stale-while-revalidate=300";
pub const TV_SURFACE_CACHE_CONTROL: &str = "private, max-age=60, stale-while-revalidate=300";
pub const ARTWORK_CACHE_CONTROL: &str =
    "public, max-age=86400, stale-while-revalidate=604800, immutable";
pub const NO_STORE_CACHE_CONTROL: &str = "no-store";

const MAX_ETAG_BODY_BYTES: usize = 2 * 1024 * 1024;

pub fn cache_control_layer(value: &'static str) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::if_not_present(CACHE_CONTROL, HeaderValue::from_static(value))
}

pub async fn conditional_etag(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let if_none_match = req
        .headers()
        .get(IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    let mut response = next.run(req).await;

    if !matches!(method, Method::GET | Method::HEAD) || response.status() != StatusCode::OK {
        return response;
    }

    if let Some(etag) = response
        .headers()
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
    {
        if if_none_match
            .as_deref()
            .is_some_and(|value| if_none_match_matches(value, &etag))
        {
            *response.status_mut() = StatusCode::NOT_MODIFIED;
            response.headers_mut().remove(CONTENT_LENGTH);
            *response.body_mut() = Body::empty();
        }

        return response;
    }

    let (mut parts, body) = response.into_parts();
    let bytes = match to_bytes(body, MAX_ETAG_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!(error = %err, "failed to read response body for ETag generation");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let etag = sha256_etag(&bytes);
    let etag_value = match HeaderValue::from_str(&etag) {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(error = %err, "failed to construct ETag header");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    parts.headers.insert(ETAG, etag_value);

    if if_none_match
        .as_deref()
        .is_some_and(|value| if_none_match_matches(value, &etag))
    {
        parts.status = StatusCode::NOT_MODIFIED;
        parts.headers.remove(CONTENT_LENGTH);
        return Response::from_parts(parts, Body::empty());
    }

    Response::from_parts(parts, Body::from(bytes))
}

pub fn sha256_etag(bytes: &[u8]) -> String {
    let hash = digest(&SHA256, bytes);
    let mut etag = String::with_capacity(66);
    etag.push('"');
    for byte in hash.as_ref() {
        push_hex_byte(&mut etag, *byte);
    }
    etag.push('"');
    etag
}

pub fn if_none_match_matches(header_value: &str, etag: &str) -> bool {
    header_value.split(',').any(|candidate| {
        let candidate = candidate.trim();
        candidate == "*"
            || entity_tag_weak_eq(candidate, etag)
            || candidate
                .strip_prefix("W/")
                .is_some_and(|tag| entity_tag_weak_eq(tag, etag))
            || etag
                .strip_prefix("W/")
                .is_some_and(|tag| entity_tag_weak_eq(candidate, tag))
    })
}

fn entity_tag_weak_eq(a: &str, b: &str) -> bool {
    a == b
}

fn push_hex_byte(out: &mut String, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push(HEX[(byte >> 4) as usize] as char);
    out.push(HEX[(byte & 0x0f) as usize] as char);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_etag_uses_quoted_lowercase_hex() {
        assert_eq!(
            sha256_etag(b"abc"),
            "\"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\""
        );
    }

    #[test]
    fn if_none_match_supports_lists_and_wildcard() {
        let etag = "\"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"";

        assert!(if_none_match_matches("*", etag));
        assert!(if_none_match_matches(
            "\"other\", \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"",
            etag
        ));
        assert!(!if_none_match_matches("\"other\"", etag));
    }

    #[test]
    fn if_none_match_uses_weak_comparison() {
        let etag = "\"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"";

        assert!(if_none_match_matches(
            "W/\"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"",
            etag
        ));
    }
}
