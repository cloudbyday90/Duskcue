// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use serde::{Deserialize, Serialize};
use validator::Validate;

pub const MAX_FEED_URL_BYTES: usize = 4096;

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ConnectionValidationRequest {
    #[validate(length(min = 1, max = 4096))]
    pub playlist_url: String,
    #[validate(length(min = 1, max = 4096))]
    pub guide_url: String,
}

#[derive(Serialize)]
pub struct ConnectionValidationResponse {
    pub validation_scope: ValidationScope,
    pub network_checked: bool,
    pub saved: bool,
    pub playlist: FeedValidationResponse,
    pub guide: FeedValidationResponse,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationScope {
    ConfigurationOnly,
}

#[derive(Serialize)]
pub struct FeedValidationResponse {
    pub transport: FeedTransport,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedTransport {
    Http,
    Https,
}
