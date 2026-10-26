// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct ChannelMetadata {
    pub name: String,
    pub tvg_name: Option<String>,
    pub group: Option<String>,
    pub number: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct TextMetadata {
    pub lang: Option<String>,
    pub value: String,
}

#[derive(Deserialize)]
pub(super) struct RatingMetadata {
    pub system: Option<String>,
    pub value: String,
}

#[derive(Deserialize)]
pub(super) struct DeclarationMetadata {
    #[serde(default, rename = "display-name")]
    pub display_names: Vec<TextMetadata>,
}

#[derive(Deserialize)]
pub(super) struct GuideMetadata {
    pub undeclared: bool,
    pub ambiguous: bool,
    pub declarations: Vec<DeclarationMetadata>,
}

#[derive(Deserialize)]
pub(super) struct ProgrammeMetadata {
    pub start: Option<String>,
    pub stop: Option<String>,
    pub clumpidx: Option<String>,
    #[serde(default, rename = "title")]
    pub titles: Vec<TextMetadata>,
    #[serde(default, rename = "sub-title")]
    pub subtitles: Vec<TextMetadata>,
    #[serde(default, rename = "desc")]
    pub descriptions: Vec<TextMetadata>,
    #[serde(default, rename = "category")]
    pub categories: Vec<TextMetadata>,
    #[serde(default)]
    pub ratings: Vec<RatingMetadata>,
}
