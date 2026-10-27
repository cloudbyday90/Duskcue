// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    cases,
    commit_proxy::Proxy,
    database::{self, Database},
    fixture::{self, Fixture},
};
use duskcue::services::playback_admission::{AdmissionError as E, AdmissionOwner};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use uuid::Uuid;

pub async fn faults(db: &Database) -> anyhow::Result<()> {
    database::upgrade(&db.pool).await?;
    let f = Fixture::new(db).await?;
    let owner = f.owner().await?;
    let request = f.request();
    fixture::start(&owner, request).await?;
    let mut lock = db.pool.begin().await?;
    sqlx::query("SELECT id FROM playback_retention WHERE id FOR UPDATE")
        .execute(&mut *lock)
        .await?;
    assert_eq!(
        cases::maintain(&owner)
            .await
            .unwrap_err()
            .downcast_ref::<E>(),
        Some(&E::Storage)
    );
    owner.renew(request).await?;
    owner.authorize_use(request).await?;
    let running = owner.clone();
    let observer = tokio::spawn(async move { running.maintain().await });
    tokio::time::timeout(Duration::from_millis(350),async {
        loop {
            let waiting:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'UPDATE playback_retention%'").fetch_one(&db.pool).await?;
            if waiting>0 {break Ok::<_,sqlx::Error>(());}
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await??;
    observer.abort();
    let _ = observer.await;
    lock.rollback().await?;
    cases::maintain(&owner).await?;
    owner.renew(request).await?;
    fixture::close(&owner).await?;
    fixture::released(&db.pool, request.id).await?;
    database::counts(&db.pool).await?;
    for before in [true, false] {
        let proxy = Proxy::start(
            &db.url,
            b"UPDATE playback_retention SET cutoff=greatest",
            before,
            false,
        )
        .await?;
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&proxy.url)
            .await?;
        let fault = AdmissionOwner::new(pool.clone(), f.state.encryption_key.clone(), None).await?;
        tokio::time::timeout(Duration::from_secs(5), async {
            while proxy.dropped() == 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await?;
        assert_eq!(proxy.dropped(), 1);
        let active = f.request();
        fixture::start(&fault, active).await?;
        fault.renew(active).await?;
        fixture::close(&fault).await?;
        database::counts(&db.pool).await?;
        pool.close().await;
        proxy.finish().await?;
    }
    let before = database::counts(&db.pool).await?;
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
    assert_eq!(database::counts(&db.pool).await?, before);
    let one: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&pool).await?;
    assert_eq!(one, 1);
    pool.close().await;
    proxy.finish().await?;
    Ok(())
}

pub async fn prune_commit_fault(db: &Database, before: bool) -> anyhow::Result<()> {
    database::legacy(&db.pool).await?;
    let instance: Uuid =
        sqlx::query_scalar("INSERT INTO playback_instances DEFAULT VALUES RETURNING id")
            .fetch_one(&db.pool)
            .await?;
    let household = Uuid::now_v7();
    sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind,state,released_at) SELECT uuidv7(interval '-2 days'),$1,$2,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod','released',clock_timestamp()-interval '25 hours' FROM generate_series(1,128)").bind(instance).bind(household).execute(&db.pool).await?;
    database::upgrade(&db.pool).await?;
    let f = Fixture::new(db).await?;
    let proxy = Proxy::start(
        &db.url,
        b"UPDATE playback_retention SET cutoff=greatest",
        before,
        false,
    )
    .await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&proxy.url)
        .await?;
    let owner = AdmissionOwner::new(pool.clone(), f.state.encryption_key.clone(), None).await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        while proxy.dropped() == 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    tokio::time::timeout(Duration::from_secs(3),async {
        loop {
            let busy:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND state<>'idle' AND query='COMMIT'").fetch_one(&db.pool).await?;
            if busy==0 {break Ok::<_,sqlx::Error>(());}
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await??;
    assert_eq!(
        database::counts(&db.pool).await?,
        if before { 128 } else { 0 }
    );
    fixture::close(&owner).await?;
    pool.close().await;
    proxy.finish().await?;
    let healthy = f.owner().await?;
    cases::maintain(&healthy).await?;
    assert_eq!(database::counts(&db.pool).await?, 0);
    fixture::close(&healthy).await?;
    Ok(())
}

pub async fn old_viewer(db: &Database) -> anyhow::Result<()> {
    database::upgrade(&db.pool).await?;
    let f = Fixture::new(db).await?;
    let owner = f.owner().await?;
    let request = f.request();
    fixture::start(&owner, request).await?;
    let began = tokio::time::Instant::now();
    let result = async {
        while began.elapsed() < Duration::from_secs(307) {
            tokio::time::sleep(Duration::from_secs(15)).await;
            owner.renew(request).await?;
            owner.authorize_use(request).await?;
        }
        assert!(!fixture::start(&owner, request).await?.fresh);
        let fresh = f.request();
        fixture::start(&owner, fresh).await?;
        owner.stop(fresh.id)?;
        fixture::released(&db.pool, fresh.id).await?;
        let mut old = f.request();
        old.id = sqlx::query_scalar::<_, Uuid>("SELECT uuidv7(interval '-6 minutes')")
            .fetch_one(&db.pool)
            .await?;
        assert_eq!(fixture::start(&owner, old).await.unwrap_err(), E::Expired);
        owner.renew(request).await?;
        assert_eq!(database::counts(&db.pool).await?, 2);
        Ok::<_, anyhow::Error>(())
    }
    .await;
    fixture::close(&owner).await?;
    fixture::released(&db.pool, request.id).await?;
    result
}
