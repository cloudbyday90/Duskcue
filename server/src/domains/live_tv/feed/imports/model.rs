// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

pub(super) struct Channel {
    pub id: Uuid,
    pub key: Vec<u8>,
    pub guide: Option<String>,
    pub metadata: Value,
    pub stream: String,
    pub logo: Option<String>,
}

pub(super) struct Programme {
    pub id: Uuid,
    pub start: Option<DateTime<Utc>>,
    pub stop: Option<DateTime<Utc>>,
    pub flags: i32,
    pub metadata: Value,
}

pub(super) struct Guide {
    pub id: Uuid,
    pub metadata: Value,
    pub programmes: Vec<Programme>,
}

pub(super) struct Catalog {
    pub channels: Vec<Channel>,
    pub guides: BTreeMap<String, Guide>,
}

pub(super) struct SealedRow {
    pub id: Uuid,
    pub guide: Option<Uuid>,
    pub channel_key: Option<Vec<u8>>,
    pub start: Option<DateTime<Utc>>,
    pub stop: Option<DateTime<Utc>>,
    pub flags: i32,
    pub metadata: String,
    pub stream: Option<String>,
    pub logo: Option<String>,
}

pub(super) struct Candidate {
    pub snapshot: Uuid,
    pub manifest: String,
    pub guides: Vec<SealedRow>,
    pub channels: Vec<SealedRow>,
    pub programmes: Vec<SealedRow>,
}
