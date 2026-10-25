// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{http::Case, root_cases};
use sqlx::{Connection, PgConnection};
use std::{net::SocketAddr, time::Duration};
use tokio::task::JoinHandle;

pub struct HeldRequest {
    connection: PgConnection,
    task: JoinHandle<anyhow::Result<(u16, serde_json::Value)>>,
}
impl HeldRequest {
    pub async fn start(
        case: &Case,
        url: &str,
        address: SocketAddr,
        path: String,
    ) -> anyhow::Result<Self> {
        let mut connection = PgConnection::connect(url).await?;
        sqlx::query("BEGIN").execute(&mut connection).await?;
        sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
            .bind(case.admin.user)
            .fetch_one(&mut connection)
            .await?;
        let token = case.admin.token.clone();
        let task = tokio::spawn(async move {
            root_cases::request(address, &token, "GET", &path, None, None).await
        });
        let held = Self { connection, task };
        let observed=tokio::time::timeout(Duration::from_secs(3),async {
            loop {
                let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE 'SELECT id FROM users WHERE id=ANY%')").fetch_one(&case.pool).await?;
                if waiting { return Ok::<_,anyhow::Error>(()); }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await;
        match observed {
            Ok(Ok(())) => Ok(held),
            _ => {
                held.finish().await?;
                anyhow::bail!("owned request did not reach the shutdown hold");
            }
        }
    }
    pub async fn finish(mut self) -> anyhow::Result<()> {
        self.task.abort();
        let _result = tokio::time::timeout(Duration::from_secs(3), self.task).await?;
        tokio::time::timeout(
            Duration::from_secs(3),
            sqlx::query("ROLLBACK").execute(&mut self.connection),
        )
        .await??;
        self.connection.close().await?;
        Ok(())
    }
}
