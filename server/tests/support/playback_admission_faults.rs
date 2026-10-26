// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    commit_proxy::Proxy,
    database::Database,
    fixture::{self, Fixture},
};
use duskcue::services::playback_admission::{AdmissionError as E, AdmissionOwner};
use sqlx::postgres::PgPoolOptions;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub async fn run(db: &Database) -> anyhow::Result<()> {
    for before in [true, false] {
        let f = Fixture::new(db).await?;
        let proxy =
            Proxy::start(&db.url, b"INSERT INTO playback_reservations", before, false).await?;
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&proxy.url)
            .await?;
        let owner = AdmissionOwner::new(pool.clone(), f.state.encryption_key.clone(), None).await?;
        let request = f.request();
        let started = Arc::new(AtomicBool::new(false));
        let observed = started.clone();
        let result = owner
            .start(request, move |cancel| async move {
                observed.store(true, Ordering::SeqCst);
                cancel.cancelled().await;
                Ok(())
            })
            .await;
        assert_eq!(result.unwrap_err(), E::Indeterminate);
        assert_eq!(proxy.dropped(), 1);
        assert!(!started.load(Ordering::SeqCst));
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM playback_reservations WHERE id=$1")
                .bind(request.id)
                .fetch_one(&db.pool)
                .await?;
        assert_eq!(count, i64::from(!before));
        fixture::close(&owner).await?;
        assert!(!started.load(Ordering::SeqCst));
        pool.close().await;
        proxy.finish().await?;
    }
    let f = Fixture::new(db).await?;
    let proxy = Proxy::start(&db.url, b"never-arm", false, true).await?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&proxy.url)
        .await?;
    assert!(
        AdmissionOwner::new(pool.clone(), f.state.encryption_key.clone(), None)
            .await
            .is_err()
    );
    assert_eq!(proxy.dropped(), 1);
    let one: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&pool).await?;
    assert_eq!(one, 1);
    pool.close().await;
    proxy.finish().await?;

    for release_fault in [false, true] {
        let f = Fixture::new(db).await?;
        let needle = if release_fault {
            b"SET state='released'".as_slice()
        } else {
            b"SET deadline=clock_timestamp()".as_slice()
        };
        let proxy = Proxy::start(&db.url, needle, false, false).await?;
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&proxy.url)
            .await?;
        let owner = AdmissionOwner::new(pool.clone(), f.state.encryption_key.clone(), None).await?;
        let r = f.request();
        fixture::start(&owner, r).await?;
        if release_fault {
            owner.stop(r.id)?;
            owner.close();
            assert_eq!(owner.drain().await.unwrap_err(), E::Cleanup);
            assert_eq!(fixture::state(&db.pool, r.id).await?, "released");
        } else {
            assert_eq!(owner.renew(r).await.unwrap_err(), E::Indeterminate);
            fixture::released(&db.pool, r.id).await?;
            fixture::close(&owner).await?;
        }
        assert_eq!(proxy.dropped(), 1);
        pool.close().await;
        proxy.finish().await?;
    }

    let f = Fixture::new(db).await?;
    let owner = f.owner().await?;
    let r = f.request();
    let mut lock = db.pool.begin().await?;
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(r.actor.user)
        .execute(&mut *lock)
        .await?;
    let running = owner.clone();
    let observer = tokio::spawn(async move { fixture::start(&running, r).await });
    tokio::time::timeout(Duration::from_millis(350),async {
        loop {
            let waiting:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock'").fetch_one(&db.pool).await?;
            if waiting>0 {break Ok::<_,sqlx::Error>(());}
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await??;
    assert_eq!(fixture::start(&owner, r).await.unwrap_err(), E::Pending);
    observer.abort();
    let _ = observer.await;
    lock.rollback().await?;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM playback_reservations WHERE id=$1)",
            )
            .bind(r.id)
            .fetch_one(&db.pool)
            .await?;
            if exists {
                break Ok::<_, sqlx::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    fixture::close(&owner).await?;
    fixture::released(&db.pool, r.id).await?;
    for _ in 0..8 {
        let f = Fixture::new(db).await?;
        let owner = f.owner().await?;
        let r = f.request();
        let concurrent = owner.clone();
        let start = tokio::spawn(async move { fixture::start(&concurrent, r).await });
        tokio::task::yield_now().await;
        owner.close();
        let result = start.await?;
        assert!(result.is_ok() || matches!(result, Err(E::Closed)));
        owner.drain().await?;
        let remaining: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM playback_reservations WHERE instance_id=$1 AND state<>'released'",
        )
        .bind(owner.instance())
        .fetch_one(&db.pool)
        .await?;
        assert_eq!(remaining, 0);
        assert_eq!(
            fixture::start(&owner, f.request()).await.unwrap_err(),
            E::Closed
        );
    }
    Ok(())
}
