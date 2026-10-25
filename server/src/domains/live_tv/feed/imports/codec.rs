// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    ImportError,
    limits::{self, Work},
    model::{Candidate, Catalog, SealedRow},
    receipt::Receipt,
};
use crate::services::encryption::{EncryptionKey, context::RecordContext};
use ring::digest::{Context, SHA256};
use serde_json::{Value, json};
use uuid::Uuid;

pub(super) fn manifest(receipt: Receipt, counts: (usize, usize, usize)) -> Value {
    json!({"household":receipt.fence.user,"assignment":receipt.fence.assignment,"generation":receipt.fence.generation,"attempt":receipt.attempt,"sequence":receipt.sequence,"snapshot":receipt.snapshot,"imported_at":receipt.imported_at,"channels":counts.0,"guides":counts.1,"programmes":counts.2})
}

pub(super) fn field(receipt: Receipt, kind: &str, row: &SealedRow, purpose: &str) -> String {
    let mut hash = Context::new(&SHA256);
    hash.update(b"duskcue-catalog-binding\0\x01");
    hash.update(receipt.fence.assignment.as_bytes());
    hash.update(receipt.snapshot.as_bytes());
    hash.update(&receipt.fence.generation.to_be_bytes());
    hash.update(receipt.attempt.as_bytes());
    hash.update(&receipt.sequence.to_be_bytes());
    hash.update(&receipt.imported_at.timestamp_micros().to_be_bytes());
    hash.update(kind.as_bytes());
    hash.update(purpose.as_bytes());
    hash.update(row.guide.unwrap_or(Uuid::nil()).as_bytes());
    if let Some(key) = &row.channel_key {
        hash.update(key);
    }
    for value in [row.start, row.stop] {
        hash.update(
            &value
                .map(|v| v.timestamp_micros())
                .unwrap_or(i64::MIN)
                .to_be_bytes(),
        );
    }
    hash.update(&row.flags.to_be_bytes());
    use base64::Engine;
    format!(
        "v1:{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash.finish().as_ref())
    )
}

pub(super) fn seal(
    key: &EncryptionKey,
    receipt: Receipt,
    kind: &str,
    row: &SealedRow,
    purpose: &str,
    value: &str,
) -> Result<String, ImportError> {
    key.seal_record(
        value,
        RecordContext {
            namespace: "live-tv-catalog",
            owner: receipt.fence.user,
            record: row.id,
            field: &field(receipt, kind, row, purpose),
        },
    )
    .map_err(|_| ImportError::Storage)
}

pub(super) fn open(
    key: &EncryptionKey,
    receipt: Receipt,
    kind: &str,
    row: &SealedRow,
    purpose: &str,
    value: &str,
) -> Result<String, ImportError> {
    key.open_record(
        value,
        RecordContext {
            namespace: "live-tv-catalog",
            owner: receipt.fence.user,
            record: row.id,
            field: &field(receipt, kind, row, purpose),
        },
    )
    .map_err(|_| ImportError::Storage)
}

pub(super) fn empty(id: Uuid) -> SealedRow {
    SealedRow {
        id,
        guide: None,
        channel_key: None,
        start: None,
        stop: None,
        flags: 0,
        metadata: String::new(),
        stream: None,
        logo: None,
    }
}

pub(super) fn encode(
    catalog: Catalog,
    key: &EncryptionKey,
    receipt: Receipt,
    work: &mut Work,
) -> Result<Candidate, ImportError> {
    let counts = (
        catalog.channels.len(),
        catalog.guides.len(),
        catalog.guides.values().map(|g| g.programmes.len()).sum(),
    );
    work.plaintext = 0;
    let manifest = limits::json(&manifest(receipt, counts))?;
    work.payload(manifest.len(), 0)?;
    let mut manifest_row = empty(receipt.snapshot);
    manifest_row.metadata = seal(
        key,
        receipt,
        "snapshot",
        &manifest_row,
        "manifest",
        &manifest,
    )?;
    work.payload(0, manifest_row.metadata.len())?;
    let mut candidate = Candidate {
        snapshot: receipt.snapshot,
        manifest: manifest_row.metadata,
        guides: Vec::new(),
        channels: Vec::new(),
        programmes: Vec::new(),
    };
    for channel in catalog.channels {
        work.check()?;
        let metadata = limits::json(&channel.metadata)?;
        work.payload(
            metadata.len() + channel.stream.len() + channel.logo.as_ref().map_or(0, String::len),
            0,
        )?;
        let mut row = empty(channel.id);
        row.channel_key = Some(channel.key);
        row.guide = channel
            .guide
            .as_ref()
            .and_then(|k| catalog.guides.get(k))
            .map(|g| g.id);
        row.metadata = seal(key, receipt, "channel", &row, "metadata", &metadata)?;
        row.stream = Some(seal(
            key,
            receipt,
            "channel",
            &row,
            "stream",
            &channel.stream,
        )?);
        row.logo = channel
            .logo
            .map(|v| seal(key, receipt, "channel", &row, "logo", &v))
            .transpose()?;
        work.payload(
            0,
            row.metadata.len()
                + row.stream.as_ref().map_or(0, String::len)
                + row.logo.as_ref().map_or(0, String::len),
        )?;
        candidate.channels.push(row);
    }
    for guide in catalog.guides.into_values() {
        work.check()?;
        let metadata = limits::json(&guide.metadata)?;
        work.payload(metadata.len(), 0)?;
        let mut row = empty(guide.id);
        row.metadata = seal(key, receipt, "guide", &row, "metadata", &metadata)?;
        work.payload(0, row.metadata.len())?;
        candidate.guides.push(row);
        for programme in guide.programmes {
            work.check()?;
            let metadata = limits::json(&programme.metadata)?;
            work.payload(metadata.len(), 0)?;
            let mut row = empty(programme.id);
            row.guide = Some(guide.id);
            row.start = programme.start;
            row.stop = programme.stop;
            row.flags = programme.flags;
            row.metadata = seal(key, receipt, "programme", &row, "metadata", &metadata)?;
            work.payload(0, row.metadata.len())?;
            candidate.programmes.push(row);
        }
    }
    Ok(candidate)
}
