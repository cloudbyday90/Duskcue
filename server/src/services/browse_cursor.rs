// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use base64::Engine;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::domains::media::MediaError;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedBrowseCursor<K> {
    pub version: u8,
    pub context: String,
    pub id: Uuid,
    pub key: K,
}

pub fn limit(value: Option<u32>) -> Result<u32, MediaError> {
    let value = value.unwrap_or(30);
    if !(1..=100).contains(&value) {
        return Err(MediaError::InvalidBrowseQuery(
            "limit must be between 1 and 100".into(),
        ));
    }
    Ok(value)
}

pub fn decode<K: DeserializeOwned>(
    value: Option<&str>,
    context: &str,
) -> Result<Option<ScopedBrowseCursor<K>>, MediaError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.len() > 8192 {
        return Err(MediaError::InvalidBrowseCursor);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| MediaError::InvalidBrowseCursor)?;
    let cursor: ScopedBrowseCursor<K> =
        serde_json::from_slice(&bytes).map_err(|_| MediaError::InvalidBrowseCursor)?;
    if cursor.version != 1 || cursor.context != context {
        return Err(MediaError::InvalidBrowseCursor);
    }
    Ok(Some(cursor))
}

pub fn encode<K: Serialize>(context: &str, id: Uuid, key: K) -> String {
    base64::engine::general_purpose::STANDARD.encode(
        serde_json::to_vec(&ScopedBrowseCursor {
            version: 1,
            context: context.into(),
            id,
            key,
        })
        .expect("serializable browsing cursor"),
    )
}
