// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use url::Url;
use validator::Validate;

use crate::error::FieldError;

use super::error::LiveTvError;
use super::types::{
    ConnectionValidationRequest, ConnectionValidationResponse, FeedTransport,
    FeedValidationResponse, MAX_FEED_URL_BYTES, ValidationScope,
};

pub fn validate_connection(
    request: ConnectionValidationRequest,
) -> Result<ConnectionValidationResponse, LiveTvError> {
    if let Err(errors) = request.validate() {
        let errors = errors.field_errors();
        let fields = ["playlist_url", "guide_url"]
            .into_iter()
            .filter(|field| errors.contains_key(*field))
            .map(|field| field_error(field, "URL must contain between 1 and 4096 UTF-8 bytes"))
            .collect();
        return Err(LiveTvError::InvalidConfiguration { fields });
    }

    let playlist = validate_feed_url(&request.playlist_url);
    let guide = validate_feed_url(&request.guide_url);
    let mut fields = Vec::new();
    if let Err(reason) = playlist.as_ref() {
        fields.push(field_error("playlist_url", reason));
    }
    if let Err(reason) = guide.as_ref() {
        fields.push(field_error("guide_url", reason));
    }
    match (playlist, guide) {
        (Ok(playlist), Ok(guide)) => Ok(ConnectionValidationResponse {
            validation_scope: ValidationScope::ConfigurationOnly,
            network_checked: false,
            saved: false,
            playlist: FeedValidationResponse {
                transport: playlist,
            },
            guide: FeedValidationResponse { transport: guide },
        }),
        _ => Err(LiveTvError::InvalidConfiguration { fields }),
    }
}

fn field_error(field: &str, message: &str) -> FieldError {
    FieldError {
        field: field.into(),
        code: "invalid_url".into(),
        message: message.into(),
    }
}

pub(super) fn validate_feed_url(raw: &str) -> Result<FeedTransport, &'static str> {
    if raw.is_empty() || raw.len() > MAX_FEED_URL_BYTES {
        return Err("URL must contain between 1 and 4096 UTF-8 bytes");
    }
    if raw
        .chars()
        .any(|character| character.is_whitespace() || character.is_control() || character == '\\')
    {
        return Err("URL must not contain raw whitespace, controls or backslashes");
    }
    let (scheme, rest) = raw
        .split_once("://")
        .ok_or("URL must be an absolute HTTP or HTTPS address")?;
    let transport = if scheme.eq_ignore_ascii_case("http") {
        FeedTransport::Http
    } else if scheme.eq_ignore_ascii_case("https") {
        FeedTransport::Https
    } else {
        return Err("URL must use HTTP or HTTPS");
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() {
        return Err("URL must include a host");
    }
    if authority.contains('@') {
        return Err("URL userinfo credentials are not supported");
    }
    let parsed = Url::parse(raw).map_err(|_| "URL is invalid")?;
    if parsed.host().is_none() {
        return Err("URL must include a host");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("URL userinfo credentials are not supported");
    }
    if parsed.fragment().is_some() {
        return Err("URL fragments are not supported");
    }
    Ok(transport)
}
