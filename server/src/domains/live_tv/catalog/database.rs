// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{error::CatalogError, limits};
use sqlx::{PgConnection, PgPool, Postgres, pool::PoolConnection};
use std::ops::{Deref, DerefMut};
use tokio::time::{Instant, timeout, timeout_at};

pub(crate) struct Lease {
    connection: PoolConnection<Postgres>,
    clean: bool,
}

impl Lease {
    pub async fn acquire(pool: &PgPool, deadline: Instant) -> Result<Self, CatalogError> {
        let connection = timeout_at(deadline, pool.acquire())
            .await
            .map_err(|_| CatalogError::Unavailable)??;
        Ok(Self {
            connection,
            clean: false,
        })
    }

    pub async fn begin(&mut self) -> Result<(), CatalogError> {
        sqlx::query("BEGIN").execute(&mut **self).await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut **self)
            .await?;
        sqlx::query("SET LOCAL lock_timeout = '500ms'")
            .execute(&mut **self)
            .await?;
        sqlx::query("SET LOCAL statement_timeout = '1500ms'")
            .execute(&mut **self)
            .await?;
        sqlx::query("SET LOCAL transaction_timeout = '3s'")
            .execute(&mut **self)
            .await?;
        Ok(())
    }

    pub async fn release(mut self) -> Result<(), CatalogError> {
        timeout(
            limits::RELEASE,
            sqlx::query("ROLLBACK").execute(&mut *self.connection),
        )
        .await
        .map_err(|_| CatalogError::Unavailable)??;
        self.clean = true;
        Ok(())
    }
}

impl Deref for Lease {
    type Target = PgConnection;
    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}
impl DerefMut for Lease {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.connection
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if !self.clean {
            self.connection.close_on_drop();
        }
    }
}
