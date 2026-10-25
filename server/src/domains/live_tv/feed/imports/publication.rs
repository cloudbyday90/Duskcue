// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    ImportError,
    model::{Candidate, SealedRow},
    receipt::{self, Receipt},
    storage,
};
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub(super) async fn publish(
    pool: &PgPool,
    receipt: Receipt,
    candidate: Candidate,
    cancel: &CancellationToken,
    deadline: std::time::Instant,
) -> Result<Uuid, ImportError> {
    let result=storage::write_until(pool,tokio::time::Instant::from_std(deadline),async |conn| {
        receipt.locked(conn).await?;
        if cancel.is_cancelled() {return Err(ImportError::Cancelled);}
        sqlx::query("INSERT INTO live_tv_catalog_snapshots(id,household_user_id,assignment_id,source_generation,attempt_id,attempt_sequence,channel_count,guide_count,programme_count,imported_at,manifest) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
            .bind(candidate.snapshot).bind(receipt.fence.user).bind(receipt.fence.assignment).bind(receipt.fence.generation).bind(receipt.attempt).bind(receipt.sequence)
            .bind(i32::try_from(candidate.channels.len()).map_err(|_|ImportError::Limit)?).bind(i32::try_from(candidate.guides.len()).map_err(|_|ImportError::Limit)?).bind(i32::try_from(candidate.programmes.len()).map_err(|_|ImportError::Limit)?).bind(receipt.imported_at).bind(&candidate.manifest).execute(&mut *conn).await?;
        insert(conn,receipt,"guide",&candidate.guides,cancel).await?;
        insert(conn,receipt,"channel",&candidate.channels,cancel).await?;
        insert(conn,receipt,"programme",&candidate.programmes,cancel).await?;
        sqlx::query("UPDATE live_tv_import_state SET current_snapshot_id=$2,state='succeeded',category=NULL,finished_at=clock_timestamp() WHERE assignment_id=$1 AND attempt_id=$3 AND attempt_sequence=$4")
            .bind(receipt.fence.assignment).bind(candidate.snapshot).bind(receipt.attempt).bind(receipt.sequence).execute(&mut *conn).await?;
        sqlx::query("DELETE FROM live_tv_catalog_snapshots WHERE assignment_id=$1 AND id<>$2")
            .bind(receipt.fence.assignment).bind(candidate.snapshot).execute(&mut *conn).await?;
        sqlx::query("INSERT INTO audit_log(table_name,row_id,operation,new_data,user_id,db_user,application_name) VALUES ('live_tv_import_state',$1,'UPDATE',jsonb_build_object('event','published','attempt_id',$2::uuid,'snapshot_id',$3::uuid,'channels',$4::integer,'guides',$5::integer,'programmes',$6::integer),NULL,current_user,'duskcue-live-tv-import')")
            .bind(receipt.fence.assignment).bind(receipt.attempt).bind(candidate.snapshot).bind(candidate.channels.len() as i32).bind(candidate.guides.len() as i32).bind(candidate.programmes.len() as i32).execute(&mut *conn).await?;
        if cancel.is_cancelled() {return Err(ImportError::Cancelled);}
        Ok(candidate.snapshot)
    }).await;
    if result != Err(ImportError::Indeterminate) {
        return result;
    }
    let reconciled=storage::write(pool,async |conn| {
        receipt::lock(conn,receipt.fence).await?;
        let found:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM live_tv_catalog_snapshots WHERE id=$1 AND assignment_id=$2 AND attempt_id=$3 AND attempt_sequence=$4)")
            .bind(receipt.snapshot).bind(receipt.fence.assignment).bind(receipt.attempt).bind(receipt.sequence).fetch_one(conn).await?;
        Ok(found)
    }).await;
    match reconciled {
        Ok(true) => Ok(receipt.snapshot),
        Err(ImportError::Authority) => Err(ImportError::Authority),
        _ => Err(ImportError::Indeterminate),
    }
}

async fn insert(
    conn: &mut PgConnection,
    receipt: Receipt,
    kind: &str,
    rows: &[SealedRow],
    cancel: &CancellationToken,
) -> Result<(), ImportError> {
    let mut offset = 0;
    while offset < rows.len() {
        if cancel.is_cancelled() {
            return Err(ImportError::Cancelled);
        }
        let mut end = offset;
        let mut bytes = 0;
        while end < rows.len() && end - offset < 250 {
            let row = &rows[end];
            let cost = 1024
                + row.metadata.len()
                + row.stream.as_ref().map_or(0, String::len)
                + row.logo.as_ref().map_or(0, String::len);
            if bytes + cost > 1024 * 1024 {
                break;
            }
            bytes += cost;
            end += 1;
        }
        if end == offset {
            return Err(ImportError::Limit);
        }
        let prefix = match kind {
            "guide" => {
                "INSERT INTO live_tv_catalog_guides(id,household_user_id,assignment_id,snapshot_id,metadata) "
            }
            "channel" => {
                "INSERT INTO live_tv_catalog_channels(id,household_user_id,assignment_id,snapshot_id,metadata,guide_id,channel_key,stream,logo) "
            }
            "programme" => {
                "INSERT INTO live_tv_catalog_programmes(id,household_user_id,assignment_id,snapshot_id,metadata,guide_id,starts_at,stops_at,quality_flags) "
            }
            _ => return Err(ImportError::Storage),
        };
        let mut query = QueryBuilder::<Postgres>::new(prefix);
        query.push_values(&rows[offset..end], |mut bind, row| {
            bind.push_bind(row.id)
                .push_bind(receipt.fence.user)
                .push_bind(receipt.fence.assignment)
                .push_bind(receipt.snapshot)
                .push_bind(&row.metadata);
            if kind == "channel" {
                bind.push_bind(row.guide)
                    .push_bind(&row.channel_key)
                    .push_bind(&row.stream)
                    .push_bind(&row.logo);
            }
            if kind == "programme" {
                bind.push_bind(row.guide)
                    .push_bind(row.start)
                    .push_bind(row.stop)
                    .push_bind(row.flags);
            }
        });
        query.build().persistent(false).execute(&mut *conn).await?;
        offset = end;
    }
    Ok(())
}

pub(super) async fn failed(
    pool: &PgPool,
    receipt: Receipt,
    error: ImportError,
) -> Result<(), ImportError> {
    storage::write(pool,async |conn| {
        crate::domains::household_security::transactions::lock_accounts(conn,&[receipt.fence.user]).await.map_err(|_|ImportError::Storage)?;
        sqlx::query("SELECT id FROM live_tv_assignments WHERE id=$1 FOR UPDATE").bind(receipt.fence.assignment).fetch_optional(&mut *conn).await?;
        sqlx::query("UPDATE live_tv_import_state SET state=CASE WHEN $4='indeterminate' THEN 'indeterminate' ELSE 'failed' END,category=$4,finished_at=clock_timestamp() WHERE assignment_id=$1 AND attempt_id=$2 AND attempt_sequence=$3 AND state<>'succeeded'")
            .bind(receipt.fence.assignment).bind(receipt.attempt).bind(receipt.sequence).bind(error.category()).execute(&mut *conn).await?;
        Ok(())
    }).await
}
