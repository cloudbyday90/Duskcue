// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    authority::Viewing,
    error::CatalogError,
    limits::{self, Budget},
    types::Window,
};
use crate::services::encryption::{EncryptionKey, context::RecordContext};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Position {
    Channels { key: Vec<u8> },
    Timed { start: DateTime<Utc>, id: Uuid },
    Unpositionable { id: Uuid },
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Context {
    version: u8,
    profile: Uuid,
    profile_revision: i64,
    credential_version: i64,
    assignment: Uuid,
    assignment_revision: i64,
    generation: i64,
    snapshot: Uuid,
    limit: u32,
    channel: Option<Uuid>,
    window: Option<Window>,
}

impl Context {
    pub fn new(
        view: &Viewing,
        snapshot: Uuid,
        limit: u32,
        channel: Option<Uuid>,
        window: Option<Window>,
    ) -> Self {
        Self {
            version: 1,
            profile: view.actor.profile,
            profile_revision: view.profile_revision,
            credential_version: view.actor.version,
            assignment: view.fence.assignment,
            assignment_revision: view.assignment_revision,
            generation: view.fence.generation,
            snapshot,
            limit,
            channel,
            window,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    context: Context,
    position: Position,
}

fn binding(view: &Viewing) -> RecordContext<'static> {
    RecordContext {
        namespace: "live-tv-catalog-cursor",
        owner: view.actor.user,
        record: view.actor.session,
        field: "v1",
    }
}

fn valid(position: &Position, context: &Context) -> bool {
    match position {
        Position::Channels { key } => {
            key.len() == 32 && context.channel.is_none() && context.window.is_none()
        }
        Position::Timed { start, id } => {
            id.get_version_num() == 7
                && start.timestamp_subsec_nanos() % 1000 == 0
                && context.channel.is_some()
                && context.window.is_some()
        }
        Position::Unpositionable { id } => {
            id.get_version_num() == 7 && context.channel.is_some() && context.window.is_none()
        }
    }
}

pub(crate) fn decode(
    key: &EncryptionKey,
    view: &Viewing,
    context: &Context,
    value: Option<&str>,
    budget: &mut Budget,
) -> Result<Option<Position>, CatalogError> {
    let Some(value) = value else { return Ok(None) };
    if value.is_empty() || value.len() > limits::CURSOR {
        return Err(CatalogError::Invalid);
    }
    let encoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| CatalogError::Invalid)?;
    if encoded.len() > 1536 {
        return Err(CatalogError::Invalid);
    }
    let encoded = std::str::from_utf8(&encoded).map_err(|_| CatalogError::Invalid)?;
    budget.cipher(encoded.len())?;
    let plain = key
        .open_record(encoded, binding(view))
        .map_err(|_| CatalogError::Invalid)?;
    if plain.len() > limits::CURSOR_PLAIN {
        return Err(CatalogError::Invalid);
    }
    budget.plain(plain.len())?;
    let payload: Envelope = serde_json::from_str(&plain).map_err(|_| CatalogError::Invalid)?;
    if payload.context.version != 1 || !valid(&payload.position, &payload.context) {
        return Err(CatalogError::Invalid);
    }
    if payload.context != *context {
        return Err(CatalogError::Changed);
    }
    Ok(Some(payload.position))
}

pub(crate) fn encode(
    key: &EncryptionKey,
    view: &Viewing,
    context: Context,
    position: Position,
    budget: &mut Budget,
) -> Result<String, CatalogError> {
    if !valid(&position, &context) {
        return Err(CatalogError::Unavailable);
    }
    let plain = serde_json::to_string(&Envelope { context, position })
        .map_err(|_| CatalogError::Unavailable)?;
    if plain.len() > limits::CURSOR_PLAIN {
        return Err(CatalogError::Unavailable);
    }
    budget.plain(plain.len())?;
    let envelope = key
        .seal_record(&plain, binding(view))
        .map_err(|_| CatalogError::Unavailable)?;
    budget.cipher(envelope.len())?;
    let value = URL_SAFE_NO_PAD.encode(envelope.as_bytes());
    if value.len() > limits::CURSOR {
        return Err(CatalogError::Unavailable);
    }
    Ok(value)
}
