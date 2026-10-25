// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::ImportError;
use sqlx::{PgConnection, PgPool};
use std::{ops::AsyncFnOnce, time::Duration};
use tokio::time::{Instant, timeout, timeout_at};

pub(super) async fn write<T, F>(pool: &PgPool, work: F) -> Result<T, ImportError>
where
    F: for<'a> AsyncFnOnce(&'a mut PgConnection) -> Result<T, ImportError>,
{
    write_until(pool, Instant::now() + Duration::from_secs(20), work).await
}

pub(super) async fn write_until<T, F>(
    pool: &PgPool,
    deadline: Instant,
    work: F,
) -> Result<T, ImportError>
where
    F: for<'a> AsyncFnOnce(&'a mut PgConnection) -> Result<T, ImportError>,
{
    let deadline = deadline.min(Instant::now() + Duration::from_secs(20));
    let mut conn = timeout_at(deadline, pool.acquire())
        .await
        .map_err(|_| ImportError::Deadline)??;
    conn.close_on_drop();
    let prepared = timeout_at(deadline, async {
        sqlx::query("BEGIN").execute(&mut *conn).await?;
        sqlx::query("SET LOCAL transaction_timeout = '20s'")
            .execute(&mut *conn)
            .await?;
        work(&mut conn).await
    })
    .await;
    let result = match prepared {
        Ok(Ok(value)) => {
            match timeout_at(deadline, sqlx::query("COMMIT").execute(&mut *conn)).await {
                Ok(Ok(_)) => Ok(value),
                _ => Err(ImportError::Indeterminate),
            }
        }
        outcome => {
            let error = match outcome {
                Ok(Err(e)) => e,
                _ => ImportError::Deadline,
            };
            match timeout(
                Duration::from_secs(10),
                sqlx::query("ROLLBACK").execute(&mut *conn),
            )
            .await
            {
                Ok(Ok(_)) => Err(error),
                _ => Err(ImportError::Cleanup),
            }
        }
    };
    if conn.close().await.is_err() && !matches!(result, Err(ImportError::Indeterminate)) {
        return Err(ImportError::Cleanup);
    }
    result
}
