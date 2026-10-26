// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::{codec, receipt::Receipt};
use crate::{
    domains::live_tv::{
        catalog::{
            error::CatalogError,
            limits::Budget,
            types::{ContextResponse, RefreshResponse},
        },
        feed::source::Fence,
    },
    services::encryption::EncryptionKey,
};
use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

pub(crate) struct Snapshot {
    pub(super) receipt: Receipt,
    count: i32,
    refresh: RefreshResponse,
}

impl Snapshot {
    pub fn id(&self) -> Uuid {
        self.receipt.snapshot
    }
    pub fn context(self, profile: Uuid, revision: i64) -> ContextResponse {
        ContextResponse {
            profile_id: profile,
            profile_context_revision: revision.to_string(),
            snapshot_id: self.receipt.snapshot,
            imported_at: self.receipt.imported_at,
            observed_at: Utc::now(),
            channel_count: self.count,
            latest_refresh: self.refresh,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SnapshotRow {
    id: Uuid,
    source_generation: i64,
    snapshot_attempt: Uuid,
    snapshot_sequence: i64,
    imported_at: DateTime<Utc>,
    manifest: String,
    channel_count: i32,
    guide_count: i32,
    programme_count: i32,
    state: String,
    category: Option<String>,
    attempted_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
}

pub(crate) async fn snapshot(
    conn: &mut PgConnection,
    key: &EncryptionKey,
    fence: Fence,
    budget: &mut Budget,
) -> Result<Snapshot, CatalogError> {
    let row:SnapshotRow=sqlx::query_as("SELECT s.id,s.source_generation,s.attempt_id AS snapshot_attempt,s.attempt_sequence AS snapshot_sequence,s.imported_at,s.manifest,s.channel_count,s.guide_count,s.programme_count,i.state,i.category,i.attempted_at,i.finished_at FROM live_tv_import_state i JOIN live_tv_catalog_snapshots s ON s.id=i.current_snapshot_id AND s.assignment_id=i.assignment_id AND s.household_user_id=i.household_user_id WHERE i.household_user_id=$1 AND i.assignment_id=$2")
        .bind(fence.user).bind(fence.assignment).fetch_optional(conn).await?.ok_or(CatalogError::Unavailable)?;
    if row.source_generation != fence.generation
        || row.id.get_version_num() != 7
        || row.snapshot_attempt.get_version_num() != 7
        || row.snapshot_sequence <= 0
        || !(0..=10_000).contains(&row.channel_count)
        || !(0..=10_000).contains(&row.guide_count)
        || !(0..=250_000).contains(&row.programme_count)
    {
        return Err(CatalogError::Unavailable);
    }
    budget.cipher(row.manifest.len())?;
    let receipt = Receipt {
        fence,
        attempt: row.snapshot_attempt,
        sequence: row.snapshot_sequence,
        snapshot: row.id,
        imported_at: row.imported_at,
        initiator: None,
    };
    let plain = codec::open(
        key,
        receipt,
        "snapshot",
        &codec::empty(row.id),
        "manifest",
        &row.manifest,
    )?;
    budget.plain(plain.len())?;
    let value: serde_json::Value =
        serde_json::from_str(&plain).map_err(|_| CatalogError::Unavailable)?;
    if value
        != codec::manifest(
            receipt,
            (
                row.channel_count as usize,
                row.guide_count as usize,
                row.programme_count as usize,
            ),
        )
    {
        return Err(CatalogError::Unavailable);
    }
    let state = match row.state.as_str() {
        "running" => "running",
        "succeeded" => "succeeded",
        "failed" => "failed",
        "indeterminate" => "indeterminate",
        _ => return Err(CatalogError::Unavailable),
    };
    let category = match row.category.as_deref() {
        None => None,
        Some("authority") => Some("authority"),
        Some("capacity") => Some("capacity"),
        Some("cancelled") => Some("cancelled"),
        Some("deadline") => Some("deadline"),
        Some("network") => Some("network"),
        Some("format") => Some("format"),
        Some("limit") => Some("limit"),
        Some("storage") => Some("storage"),
        Some("indeterminate") => Some("indeterminate"),
        Some("cleanup") => Some("cleanup"),
        _ => return Err(CatalogError::Unavailable),
    };
    Ok(Snapshot {
        receipt,
        count: row.channel_count,
        refresh: RefreshResponse {
            state,
            category,
            attempted_at: row.attempted_at,
            finished_at: row.finished_at,
        },
    })
}
