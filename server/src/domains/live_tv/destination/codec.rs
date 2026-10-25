// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::Approval;
use crate::{
    domains::live_tv::assignment::{error::AssignmentError, secrets::FeedField},
    services::encryption::{EncryptionKey, context::RecordContext},
};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    version: u8,
    origin: String,
    addresses: Vec<String>,
}

fn context(owner: Uuid, record: Uuid, feed: FeedField) -> RecordContext<'static> {
    RecordContext {
        namespace: "live-tv-destination",
        owner,
        record,
        field: match feed {
            FeedField::Playlist => "playlist",
            FeedField::Guide => "guide",
        },
    }
}

impl Approval {
    pub fn seal(
        &self,
        key: &EncryptionKey,
        owner: Uuid,
        record: Uuid,
        feed: FeedField,
    ) -> Result<String, AssignmentError> {
        let value = serde_json::to_string(&Payload {
            version: 1,
            origin: self.origin.clone(),
            addresses: self.addresses.iter().map(ToString::to_string).collect(),
        })
        .map_err(|_| AssignmentError::Unavailable)?;
        key.seal_record(&value, context(owner, record, feed))
            .map_err(|_| AssignmentError::Unavailable)
    }

    pub fn open(
        key: &EncryptionKey,
        owner: Uuid,
        record: Uuid,
        feed: FeedField,
        envelope: &str,
        url: &Url,
    ) -> Result<Self, AssignmentError> {
        let value = key
            .open_record(envelope, context(owner, record, feed))
            .map_err(|_| AssignmentError::Unavailable)?;
        let payload: Payload =
            serde_json::from_str(&value).map_err(|_| AssignmentError::Unavailable)?;
        if payload.version != 1 {
            return Err(AssignmentError::Unavailable);
        }
        Self::new(&payload.origin, &payload.addresses, url)
            .map_err(|_| AssignmentError::Unavailable)
    }
}
