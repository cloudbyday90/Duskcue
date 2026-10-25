// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use crate::services::encryption::{EncryptionError, EncryptionKey, context::RecordContext};
use uuid::Uuid;

#[derive(Clone, Copy)]
pub enum FeedField {
    Playlist,
    Guide,
}

fn context(owner: Uuid, record: Uuid, field: FeedField) -> RecordContext<'static> {
    RecordContext {
        namespace: "live-tv-assignment",
        owner,
        record,
        field: match field {
            FeedField::Playlist => "playlist",
            FeedField::Guide => "guide",
        },
    }
}

pub fn seal(
    key: &EncryptionKey,
    owner: Uuid,
    record: Uuid,
    field: FeedField,
    url: &str,
) -> Result<String, EncryptionError> {
    key.seal_record(url, context(owner, record, field))
}

pub fn open(
    key: &EncryptionKey,
    owner: Uuid,
    record: Uuid,
    field: FeedField,
    envelope: &str,
) -> Result<String, EncryptionError> {
    key.open_record(envelope, context(owner, record, field))
}
