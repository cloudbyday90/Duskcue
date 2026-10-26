// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use crate::qualification::database;
#[path = "../../../../tests/support/catalog_begin_proxy.rs"]
mod wire;

use super::database::Lease;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;
use tokio::time::{Instant, timeout, timeout_at};

async fn gone(pool: &PgPool, pid: i32) -> anyhow::Result<()> {
    timeout(Duration::from_secs(6), async {
        loop {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1)")
                    .bind(pid)
                    .fetch_one(pool)
                    .await?;
            if !exists {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted owned PostgreSQL18 and executed but unacknowledged BEGIN fault proxy"]
async fn catalog_lease_begin_cancellation() -> anyhow::Result<()> {
    database::run(run).await
}

async fn run(db: &database::Database) -> anyhow::Result<()> {
    let proxy = wire::Proxy::start(&db.url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(2))
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET default_transaction_isolation='repeatable read'")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&proxy.url)
        .await?;
    let outcome = exercise(db, &pool, &proxy).await;
    timeout(Duration::from_secs(8), pool.close()).await?;
    proxy.finish().await?;
    outcome
}

async fn exercise(
    db: &database::Database,
    pool: &PgPool,
    proxy: &wire::Proxy,
) -> anyhow::Result<()> {
    let mut lease = Lease::acquire(pool, Instant::now() + Duration::from_secs(3)).await?;
    lease.begin().await?;
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *lease)
        .await?;
    anyhow::ensure!(
        isolation == "read committed",
        "inherited isolation was not replaced"
    );
    let original: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *lease)
        .await?;
    lease.release().await?;
    let current: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(pool)
        .await?;
    anyhow::ensure!(
        current == original,
        "acknowledged rollback did not permit clean reuse"
    );
    proxy.arm();
    let child_pool = pool.clone();
    let task = tokio::spawn(async move {
        let mut lease =
            Lease::acquire(&child_pool, Instant::now() + Duration::from_secs(3)).await?;
        lease.begin().await?;
        lease.release().await
    });
    let pid = proxy.held().await?;
    let executed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1 AND state='idle in transaction' AND query='BEGIN')").bind(pid).fetch_one(&db.pool).await?;
    anyhow::ensure!(
        executed && pid == original,
        "BEGIN was not executed before withheld acknowledgment"
    );
    task.abort();
    anyhow::ensure!(
        task.await.is_err_and(|e| e.is_cancelled()),
        "caller cancellation not observed"
    );
    gone(&db.pool, pid).await?;
    let replacement: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(pool)
        .await?;
    anyhow::ensure!(replacement != pid, "uncertain connection was reused");
    proxy.arm();
    let child_pool = pool.clone();
    let delayed = tokio::spawn(async move {
        let mut lease =
            Lease::acquire(&child_pool, Instant::now() + Duration::from_secs(3)).await?;
        lease.begin().await?;
        lease.release().await
    });
    let delayed_pid = proxy.held().await?;
    proxy.release();
    timeout(Duration::from_secs(3), delayed).await???;
    let reused: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(pool)
        .await?;
    anyhow::ensure!(
        reused == delayed_pid,
        "delayed acknowledged BEGIN did not release cleanly"
    );
    proxy.arm();
    let child_pool = pool.clone();
    let began = Instant::now();
    let timed = tokio::spawn(async move {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut lease = Lease::acquire(&child_pool, deadline).await?;
        let result = timeout_at(deadline, lease.begin()).await;
        anyhow::ensure!(
            result.is_err(),
            "lost BEGIN acknowledgment did not reach work deadline"
        );
        anyhow::ensure!(
            lease.release().await.is_err(),
            "uncertain release became successful"
        );
        Ok::<_, anyhow::Error>(())
    });
    let timed_pid = proxy.held().await?;
    timeout(Duration::from_secs(6), timed).await???;
    anyhow::ensure!(
        began.elapsed() < Duration::from_secs(6),
        "work/release budgets expanded"
    );
    gone(&db.pool, timed_pid).await?;
    let recovery: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(pool)
        .await?;
    anyhow::ensure!(
        recovery != timed_pid,
        "deadline fallback returned uncertain connection"
    );
    println!(
        "IPTV_CATALOG_LEASE_RESULT {}",
        serde_json::json!({"resource_id":db.resource,"acknowledged_reuse":true,"cancelled_begin_discarded":true,"deadline_begin_discarded":true,"backend_disappearance":true})
    );
    Ok::<_, anyhow::Error>(())
}
