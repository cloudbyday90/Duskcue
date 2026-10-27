// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::PlaybackError;
use sqlx::{PgConnection, PgPool, Postgres, pool::PoolConnection};
use std::{
    ops::{Deref, DerefMut},
    time::Duration,
};
use tokio::time::{Instant, timeout_at};

pub(super) struct Write {
    conn: PoolConnection<Postgres>,
    end: Instant,
}
impl Deref for Write {
    type Target = PgConnection;
    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}
impl DerefMut for Write {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.conn
    }
}

pub(super) async fn begin(pool: &PgPool) -> Result<Write, PlaybackError> {
    let end = Instant::now() + Duration::from_secs(3);
    let mut conn = timeout_at(end, pool.acquire())
        .await
        .map_err(|_| sqlx::Error::PoolTimedOut)??;
    conn.close_on_drop();
    timeout_at(end, async {
        for statement in [
            "BEGIN ISOLATION LEVEL READ COMMITTED",
            "SET LOCAL lock_timeout='500ms'",
            "SET LOCAL statement_timeout='1500ms'",
            "SET LOCAL transaction_timeout='3s'",
            "LOCK TABLE streaming_policies IN SHARE ROW EXCLUSIVE MODE",
        ] {
            sqlx::query(statement).execute(&mut *conn).await?;
        }
        Ok::<_, sqlx::Error>(())
    })
    .await
    .map_err(|_| sqlx::Error::PoolTimedOut)??;
    Ok(Write { conn, end })
}
impl Write {
    pub(super) async fn commit(mut self) -> Result<(), PlaybackError> {
        timeout_at(self.end, sqlx::query("COMMIT").execute(&mut *self.conn))
            .await
            .map_err(|_| sqlx::Error::PoolTimedOut)??;
        let _ = timeout_at(self.end, self.conn.close()).await;
        Ok(())
    }
}
