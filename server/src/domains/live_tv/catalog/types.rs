// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ChannelsRequest {
    pub limit: Option<u32>,
    #[validate(length(max = 2048))]
    pub cursor: Option<String>,
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ProgrammesRequest {
    pub snapshot_id: Uuid,
    #[validate(length(min = 1, max = 64))]
    pub from: String,
    #[validate(length(min = 1, max = 64))]
    pub to: String,
    pub limit: Option<u32>,
    #[validate(length(max = 2048))]
    pub cursor: Option<String>,
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UnpositionableRequest {
    pub snapshot_id: Uuid,
    pub limit: Option<u32>,
    #[validate(length(max = 2048))]
    pub cursor: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct TextResponse {
    pub language: Option<String>,
    pub text: String,
}

#[derive(Clone, Serialize)]
pub struct RatingResponse {
    pub system: Option<String>,
    pub value: String,
}

#[derive(Clone, Serialize)]
pub struct DeclarationResponse {
    pub display_names: Vec<TextResponse>,
}

#[derive(Clone, Serialize)]
pub struct GuideResponse {
    pub undeclared: bool,
    pub ambiguous: bool,
    pub declarations: Vec<DeclarationResponse>,
}

#[derive(Serialize)]
pub struct ChannelResponse {
    pub id: Uuid,
    pub name: String,
    pub tvg_name: Option<String>,
    pub group: Option<String>,
    pub number: Option<String>,
    pub guide: Option<GuideResponse>,
}

#[derive(Serialize)]
pub struct QualityResponse {
    pub uncertain: bool,
    pub conflict: bool,
    pub overlap: bool,
    pub clump: bool,
}

#[derive(Serialize)]
pub struct ProgrammeResponse {
    pub id: Uuid,
    pub start_raw: Option<String>,
    pub stop_raw: Option<String>,
    pub clump_index_raw: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub stops_at: Option<DateTime<Utc>>,
    pub titles: Vec<TextResponse>,
    pub subtitles: Vec<TextResponse>,
    pub descriptions: Vec<TextResponse>,
    pub categories: Vec<TextResponse>,
    pub ratings: Vec<RatingResponse>,
    pub quality: QualityResponse,
}

#[derive(Serialize)]
pub struct RefreshResponse {
    pub state: &'static str,
    pub category: Option<&'static str>,
    pub attempted_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct ContextResponse {
    pub profile_id: Uuid,
    pub profile_context_revision: String,
    pub snapshot_id: Uuid,
    pub imported_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub channel_count: i32,
    pub latest_refresh: RefreshResponse,
}

#[derive(Serialize)]
pub struct ChannelsResponse {
    pub context: ContextResponse,
    pub items: Vec<ChannelResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Window {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct ProgrammesResponse {
    pub context: ContextResponse,
    pub channel_id: Uuid,
    pub guide: Option<GuideResponse>,
    pub view: &'static str,
    pub(crate) window: Option<Window>,
    pub items: Vec<ProgrammeResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub has_more: bool,
}
