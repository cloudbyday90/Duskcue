// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{AdmissionError, types::Row};
use sqlx::{PgConnection, PgPool};
use std::time::Duration;
use tokio::time::{Instant, timeout_at};
use uuid::Uuid;

pub(super) async fn transaction<T>(
    pool: &PgPool,
    action: impl AsyncFnOnce(&mut PgConnection) -> Result<T, AdmissionError>,
) -> Result<T, AdmissionError> {
    let end = Instant::now() + Duration::from_secs(3);
    let mut conn = timeout_at(end, pool.acquire())
        .await
        .map_err(|_| AdmissionError::Storage)??;
    conn.close_on_drop();
    let result = timeout_at(end, async {
        sqlx::query("BEGIN ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *conn)
            .await?;
        sqlx::query("SET LOCAL lock_timeout='500ms'")
            .execute(&mut *conn)
            .await?;
        sqlx::query("SET LOCAL statement_timeout='1500ms'")
            .execute(&mut *conn)
            .await?;
        sqlx::query("SET LOCAL transaction_timeout='3s'")
            .execute(&mut *conn)
            .await?;
        action(&mut conn).await
    })
    .await
    .unwrap_or(Err(AdmissionError::Storage));
    match result {
        Ok(value) => {
            timeout_at(end, sqlx::query("COMMIT").execute(&mut *conn))
                .await
                .map_err(|_| AdmissionError::Indeterminate)?
                .map_err(|_| AdmissionError::Indeterminate)?;
            let _ = timeout_at(end, conn.close()).await;
            Ok(value)
        }
        Err(error) => {
            let _ = timeout_at(end, sqlx::query("ROLLBACK").execute(&mut *conn)).await;
            let _ = timeout_at(end, conn.close()).await;
            Err(error)
        }
    }
}

pub(super) async fn instance(
    conn: &mut PgConnection,
    id: Uuid,
    active: bool,
) -> Result<(), AdmissionError> {
    let row: Option<(bool,)> =
        sqlx::query_as("SELECT fenced FROM playback_instances WHERE id=$1 FOR SHARE")
            .bind(id)
            .fetch_optional(conn)
            .await?;
    match row {
        Some((false,)) => Ok(()),
        Some((true,)) if !active => Ok(()),
        _ => Err(AdmissionError::Closed),
    }
}

pub(super) async fn household(conn: &mut PgConnection, id: Uuid) -> Result<(), AdmissionError> {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id=$1 FOR NO KEY UPDATE")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(AdmissionError::Authority)?;
    Ok(())
}

pub(super) async fn row(conn: &mut PgConnection, id: Uuid) -> Result<Option<Row>, AdmissionError> {
    Ok(sqlx::query_as("SELECT id,instance_id,household_id,fingerprint,kind,assignment_id,source_generation,channel_key,state,deadline FROM playback_reservations WHERE id=$1 FOR UPDATE")
        .bind(id).fetch_optional(conn).await?)
}
