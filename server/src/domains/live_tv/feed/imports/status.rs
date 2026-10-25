// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::source::Fence;
use super::{ImportError, codec, receipt::Receipt};
use crate::services::encryption::EncryptionKey;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::time::Duration;
use uuid::Uuid;

#[derive(Serialize)]
pub struct ImportStatus {
    pub assignment_id: Uuid,
    pub attempt_id: Uuid,
    pub attempt_sequence: i64,
    pub state: String,
    pub category: Option<String>,
    pub attempted_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub snapshot_id: Option<Uuid>,
    pub imported_at: Option<DateTime<Utc>>,
    pub channel_count: i32,
    pub guide_count: i32,
    pub programme_count: i32,
    pub current_source: bool,
}

pub async fn status(
    pool: &PgPool,
    key: &EncryptionKey,
    user: Uuid,
) -> Result<Option<ImportStatus>, ImportError> {
    let row=tokio::time::timeout(Duration::from_secs(1),sqlx::query("SELECT a.id AS assignment,a.source_generation AS current_generation,a.enabled AND a.playlist_destination IS NOT NULL AND a.guide_destination IS NOT NULL AND u.status='active' AND u.deleted_at IS NULL AS available,i.attempt_id,i.attempt_sequence,i.state,i.category,i.attempted_at,i.finished_at,s.id AS snapshot,s.source_generation,s.attempt_id AS snapshot_attempt,s.attempt_sequence AS snapshot_sequence,s.channel_count,s.guide_count,s.programme_count,s.imported_at,s.manifest FROM live_tv_assignments a JOIN users u ON u.id=a.household_user_id JOIN live_tv_import_state i ON i.assignment_id=a.id LEFT JOIN live_tv_catalog_snapshots s ON s.id=i.current_snapshot_id WHERE a.household_user_id=$1")
        .bind(user).fetch_optional(pool)).await.map_err(|_|ImportError::Deadline)??;
    let Some(row) = row else {
        return Ok(None);
    };
    let snapshot: Option<Uuid> = row.get("snapshot");
    let mut result = ImportStatus {
        assignment_id: row.get("assignment"),
        attempt_id: row.get("attempt_id"),
        attempt_sequence: row.get("attempt_sequence"),
        state: row.get("state"),
        category: row.get("category"),
        attempted_at: row.get("attempted_at"),
        finished_at: row.get("finished_at"),
        snapshot_id: snapshot,
        imported_at: row.get("imported_at"),
        channel_count: 0,
        guide_count: 0,
        programme_count: 0,
        current_source: false,
    };
    if let Some(snapshot) = snapshot {
        result.channel_count = row.get("channel_count");
        result.guide_count = row.get("guide_count");
        result.programme_count = row.get("programme_count");
        let generation = row.get("source_generation");
        let receipt = Receipt {
            fence: Fence {
                user,
                assignment: result.assignment_id,
                generation,
            },
            attempt: row.get("snapshot_attempt"),
            sequence: row.get("snapshot_sequence"),
            snapshot,
            imported_at: result.imported_at.ok_or(ImportError::Storage)?,
        };
        let manifest: String = row.get("manifest");
        let plain = codec::open(
            key,
            receipt,
            "snapshot",
            &codec::empty(snapshot),
            "manifest",
            &manifest,
        )?;
        let value: serde_json::Value =
            serde_json::from_str(&plain).map_err(|_| ImportError::Storage)?;
        if value
            != codec::manifest(
                receipt,
                (
                    result.channel_count as usize,
                    result.guide_count as usize,
                    result.programme_count as usize,
                ),
            )
        {
            return Err(ImportError::Storage);
        }
        result.current_source = row.get::<bool, _>("available")
            && generation == row.get::<i64, _>("current_generation");
    }
    Ok(Some(result))
}
