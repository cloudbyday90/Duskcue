// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

mod channels;
mod programmes;
mod projection;
mod snapshot;
mod stored;

#[cfg(all(test, feature = "playback-qualification"))]
mod qualification;

pub(crate) use channels::{channel, channels};
pub(crate) use programmes::{ProgrammeQuery, programmes};
pub(crate) use snapshot::{Snapshot, snapshot};

use super::{codec, model::SealedRow};
use crate::{
    domains::live_tv::catalog::{error::CatalogError, limits::Budget},
    services::encryption::EncryptionKey,
};

fn open(
    key: &EncryptionKey,
    snapshot: &Snapshot,
    kind: &str,
    row: &SealedRow,
    budget: &mut Budget,
) -> Result<String, CatalogError> {
    budget.check()?;
    let plain = codec::open(key, snapshot.receipt, kind, row, "metadata", &row.metadata)?;
    if plain.len() > super::limits::RECORD {
        return Err(CatalogError::Unavailable);
    }
    budget.plain(plain.len())?;
    Ok(plain)
}
