// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use crate::services::playback_admission::AdmissionError;
use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

pub(crate) const CANDIDATES: &str = "WITH candidates AS MATERIALIZED (SELECT id,reservation_id,household_id,candidate_output,prune_after FROM playback_vod_operations WHERE prune_after<=$3 AND (prune_after,id)>(COALESCE($1::timestamptz,'-infinity'::timestamptz),COALESCE($2::uuid,'00000000-0000-0000-0000-000000000000'::uuid)) ORDER BY prune_after,id LIMIT 128) SELECT c.id,c.reservation_id,c.household_id,c.prune_after,v.pending_operation IS DISTINCT FROM c.id AND (v.current_output IS DISTINCT FROM c.candidate_output OR r.state='released') FROM candidates c JOIN playback_reservations r ON r.id=c.reservation_id JOIN playback_vod_controls v ON v.reservation_id=r.id ORDER BY c.prune_after,c.id";

pub(crate) async fn prune(conn: &mut PgConnection) -> Result<usize, AdmissionError> {
    let (after,id,now):(Option<DateTime<Utc>>,Option<Uuid>,DateTime<Utc>)=sqlx::query_as("SELECT scan_after,scan_id,clock_timestamp() FROM playback_vod_operation_retention WHERE id").fetch_one(&mut *conn).await?;
    let candidates: Vec<(Uuid, Uuid, Uuid, DateTime<Utc>, bool)> = sqlx::query_as(CANDIDATES)
        .bind(after)
        .bind(id)
        .bind(now)
        .fetch_all(&mut *conn)
        .await?;
    let cursor = candidates.last().map(|c| (c.3, c.0));
    let household = candidates.iter().find(|c| c.4).map(|c| c.2);
    let mut selected = Vec::new();
    if let Some(household) = household {
        super::super::super::policy::fence(conn).await?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id=$1)")
            .bind(household)
            .fetch_one(&mut *conn)
            .await?;
        let lock: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM users WHERE id=$1 FOR NO KEY UPDATE SKIP LOCKED")
                .bind(household)
                .fetch_optional(&mut *conn)
                .await?;
        if !exists || lock.is_some() {
            selected.extend(
                candidates
                    .into_iter()
                    .filter(|c| c.2 == household && c.4)
                    .take(64)
                    .map(|c| (c.0, c.1)),
            );
            selected.sort_by_key(|c| c.1);
            for (_, viewer) in &selected {
                sqlx::query("SELECT id FROM playback_reservations WHERE id=$1 FOR UPDATE")
                    .bind(viewer)
                    .execute(&mut *conn)
                    .await?;
                super::control(conn, *viewer).await?;
            }
        }
    }
    sqlx::query("UPDATE playback_vod_operation_retention SET cutoff=greatest(cutoff,date_trunc('milliseconds',clock_timestamp())-interval '5 minutes'),scan_after=$1,scan_id=$2 WHERE id").bind(cursor.map(|c|c.0)).bind(cursor.map(|c|c.1)).execute(&mut *conn).await?;
    let mut deleted = 0;
    for (id, viewer) in selected {
        let Some(operation) = super::operation(conn, id).await? else {
            continue;
        };
        let control = super::control(conn, viewer).await?;
        let released: bool =
            sqlx::query_scalar("SELECT state='released' FROM playback_reservations WHERE id=$1")
                .bind(viewer)
                .fetch_one(&mut *conn)
                .await?;
        if control.pending_operation == Some(id)
            || (control.current_output == Some(operation.candidate_output) && !released)
        {
            continue;
        }
        if released {
            sqlx::query("UPDATE playback_vod_controls SET current_output=NULL,checkpoint_output=NULL,checkpoint_sequence=0,checkpoint_position=NULL WHERE reservation_id=$1 AND state='retiring' AND current_output=$2").bind(viewer).bind(operation.candidate_output).execute(&mut *conn).await?;
        }
        sqlx::query("DELETE FROM playback_vod_outputs WHERE operation_id=$1 AND reservation_id=$2")
            .bind(id)
            .bind(viewer)
            .execute(&mut *conn)
            .await?;
        deleted += usize::try_from(
            sqlx::query("DELETE FROM playback_vod_operations WHERE id=$1 AND reservation_id=$2")
                .bind(id)
                .bind(viewer)
                .execute(&mut *conn)
                .await?
                .rows_affected(),
        )
        .map_err(|_| AdmissionError::Storage)?;
    }
    Ok(deleted)
}
