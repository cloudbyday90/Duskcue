// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{MaintenanceReceipt, queries};
use crate::services::playback_admission::{AdmissionError, database, ledger::Ledger};
use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;
pub(crate) async fn missing(conn: &mut PgConnection, id: Uuid) -> Result<(), AdmissionError> {
    let result: i16 = sqlx::query_scalar("SELECT playback_receipt_time_error($1,clock_timestamp(),cutoff) FROM playback_retention WHERE id")
        .bind(id).fetch_one(conn).await?;
    match result {
        0 => Ok(()),
        1 => Err(AdmissionError::Expired),
        2 => Err(AdmissionError::RequestTime),
        _ => Err(AdmissionError::Storage),
    }
}
impl Ledger {
    pub(crate) async fn prune(&self) -> Result<MaintenanceReceipt, AdmissionError> {
        database::transaction(&self.pool, async |conn| {
            database::instance(conn, self.instance, true).await?;
            let operations=crate::services::playback_admission::vod::revisions::retention::prune(conn).await?;
            let cutoff: DateTime<Utc> = sqlx::query_scalar("UPDATE playback_retention SET cutoff=greatest(cutoff,date_trunc('milliseconds',clock_timestamp())-interval '5 minutes') WHERE id RETURNING cutoff")
                .fetch_one(&mut *conn).await?;
            let now: DateTime<Utc> = sqlx::query_scalar("SELECT date_trunc('milliseconds',clock_timestamp())")
                .fetch_one(&mut *conn).await?;
            let query=if operations==0 {queries::PRUNE}else{queries::PRUNE_SHARED};
            let deleted = sqlx::query_scalar::<_, Uuid>(query).bind(now).fetch_all(&mut *conn).await?;
            Ok(MaintenanceReceipt { deleted: deleted.len()+operations, cutoff })
        }).await
    }
}
