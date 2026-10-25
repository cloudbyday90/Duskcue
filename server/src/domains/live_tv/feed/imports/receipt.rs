// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::source::Fence;
use super::ImportError;
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(crate) struct Receipt {
    pub fence: Fence,
    pub attempt: Uuid,
    pub sequence: i64,
    pub snapshot: Uuid,
    pub imported_at: DateTime<Utc>,
    pub initiator: Option<crate::domains::live_tv::refresh::authority::Initiator>,
}

pub(crate) async fn lock(conn: &mut PgConnection, fence: Fence) -> Result<(), ImportError> {
    crate::domains::household_security::transactions::lock_accounts(conn, &[fence.user])
        .await
        .map_err(|_| ImportError::Storage)?;
    lock_source(conn, fence).await
}

pub(crate) async fn lock_source(conn: &mut PgConnection, fence: Fence) -> Result<(), ImportError> {
    let row=sqlx::query("SELECT a.source_generation,a.enabled,u.status='active' AND u.deleted_at IS NULL AS active FROM live_tv_assignments a JOIN users u ON u.id=a.household_user_id WHERE a.id=$1 AND a.household_user_id=$2 FOR UPDATE OF a")
        .bind(fence.assignment).bind(fence.user).fetch_optional(&mut *conn).await?.ok_or(ImportError::Authority)?;
    if row.get::<i64, _>("source_generation") != fence.generation
        || !row.get::<bool, _>("enabled")
        || !row.get::<bool, _>("active")
    {
        return Err(ImportError::Authority);
    }
    Ok(())
}

#[cfg(feature = "playback-qualification")]
pub(crate) async fn claim(pool: &PgPool, fence: Fence) -> Result<Receipt, ImportError> {
    let attempt = Uuid::now_v7();
    let snapshot = Uuid::now_v7();
    let imported_at = DateTime::from_timestamp_micros(Utc::now().timestamp_micros())
        .ok_or(ImportError::Storage)?;
    let result=super::storage::write(pool,async |conn| {
        lock(conn,fence).await?;
        let sequence:i64=sqlx::query_scalar("INSERT INTO live_tv_import_state(household_user_id,assignment_id,attempt_id,attempt_sequence,state) VALUES ($1,$2,$3,1,'running') ON CONFLICT (assignment_id) DO UPDATE SET attempt_id=$3,attempt_sequence=live_tv_import_state.attempt_sequence+1,state='running',category=NULL,attempted_at=clock_timestamp(),finished_at=NULL RETURNING attempt_sequence")
            .bind(fence.user).bind(fence.assignment).bind(attempt).fetch_one(&mut *conn).await?;
        Ok(Receipt {fence,attempt,sequence,snapshot,imported_at,initiator:None})
    }).await;
    if !matches!(result, Err(ImportError::Indeterminate)) {
        return result;
    }
    let recovered = super::storage::write(pool, async |conn| {
        lock(conn, fence).await?;
        let latest: Option<(Uuid, i64)> = sqlx::query_as(
            "SELECT attempt_id,attempt_sequence FROM live_tv_import_state WHERE assignment_id=$1",
        )
        .bind(fence.assignment)
        .fetch_optional(conn)
        .await?;
        Ok(latest)
    })
    .await;
    match recovered {
        Ok(Some((id, sequence))) if id == attempt => Ok(Receipt {
            fence,
            attempt,
            sequence,
            snapshot,
            imported_at,
            initiator: None,
        }),
        Ok(Some(_)) | Err(ImportError::Authority) => Err(ImportError::Authority),
        Ok(None) => Err(ImportError::Indeterminate),
        Err(_) => Err(ImportError::Cleanup),
    }
}

impl Receipt {
    pub async fn current(self, pool: &PgPool) -> Result<(), ImportError> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
        let mut conn = tokio::time::timeout_at(deadline, pool.acquire())
            .await
            .map_err(|_| ImportError::Authority)?
            .map_err(|_| ImportError::Authority)?;
        conn.close_on_drop();
        let setup = tokio::time::timeout_at(
            deadline,
            sqlx::query("SET statement_timeout = '1s'").execute(&mut *conn),
        )
        .await;
        if !matches!(setup, Ok(Ok(_))) {
            conn.close().await.map_err(|_| ImportError::Cleanup)?;
            return Err(ImportError::Authority);
        }
        let query=sqlx::query_scalar::<_,bool>("SELECT a.source_generation=$3 AND a.enabled AND u.status='active' AND u.deleted_at IS NULL AND i.attempt_id=$4 AND i.attempt_sequence=$5 FROM live_tv_assignments a JOIN users u ON u.id=a.household_user_id JOIN live_tv_import_state i ON i.assignment_id=a.id WHERE a.id=$1 AND a.household_user_id=$2")
            .bind(self.fence.assignment).bind(self.fence.user).bind(self.fence.generation).bind(self.attempt).bind(self.sequence).fetch_optional(&mut *conn);
        let result = match tokio::time::timeout_at(deadline, query).await {
            Ok(Ok(Some(true))) => Ok(()),
            _ => Err(ImportError::Authority),
        };
        let result = match (result, self.initiator) {
            (Ok(()), Some(initiator)) => {
                tokio::time::timeout_at(deadline, initiator.current(&mut conn))
                    .await
                    .unwrap_or(Err(ImportError::Authority))
            }
            (result, _) => result,
        };
        conn.close().await.map_err(|_| ImportError::Cleanup)?;
        result
    }

    pub async fn locked(self, conn: &mut PgConnection) -> Result<(), ImportError> {
        if let Some(initiator) = self.initiator {
            initiator.locked(conn, self.fence.user).await?;
            lock_source(conn, self.fence).await?;
        } else {
            lock(conn, self.fence).await?;
        }
        let latest:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM live_tv_import_state WHERE assignment_id=$1 AND attempt_id=$2 AND attempt_sequence=$3)")
            .bind(self.fence.assignment).bind(self.attempt).bind(self.sequence).fetch_one(conn).await?;
        if latest {
            Ok(())
        } else {
            Err(ImportError::Authority)
        }
    }
}
