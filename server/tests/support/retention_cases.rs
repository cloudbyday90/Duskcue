// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    database::{self, Database},
    fixture::{self, Fixture},
};
use chrono::{DateTime, Utc};
use duskcue::services::{
    encryption::EncryptionKey,
    playback_admission::{AdmissionError as E, AdmissionOwner, MaintenanceReceipt},
};
use sqlx::{AssertSqlSafe, PgPool, Row};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

pub async fn maintain(owner: &Arc<AdmissionOwner>) -> anyhow::Result<MaintenanceReceipt> {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            match owner.maintain().await {
                Err(E::Pending) => tokio::time::sleep(Duration::from_millis(10)).await,
                other => break other,
            }
        }
    })
    .await?
    .map_err(Into::into)
}
pub async fn insert(
    pool: &PgPool,
    id: Uuid,
    instance: Uuid,
    household: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind) VALUES ($1,$2,$3,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod')").bind(id).bind(instance).bind(household).execute(pool).await?;
    Ok(())
}
pub fn code(error: sqlx::Error, expected: &str) {
    assert_eq!(
        error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some(expected)
    );
}

pub async fn migration_and_pruning(db: &Database) -> anyhow::Result<()> {
    database::legacy(&db.pool).await?;
    let instance: Uuid =
        sqlx::query_scalar("INSERT INTO playback_instances DEFAULT VALUES RETURNING id")
            .fetch_one(&db.pool)
            .await?;
    let household = Uuid::now_v7();
    sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind,state,created_at,released_at) SELECT uuidv7(interval '-2 days'),$1,$2,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod','released',clock_timestamp()-interval '2 days',clock_timestamp()-interval '25 hours' FROM generate_series(1,300) n").bind(instance).bind(household).execute(&db.pool).await?;
    let held:Uuid=sqlx::query_scalar("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind,deadline) VALUES (uuidv7(interval '-3 days'),$1,$2,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod',clock_timestamp()-interval '3 days') RETURNING id").bind(instance).bind(household).fetch_one(&db.pool).await?;
    let future:Uuid=sqlx::query_scalar("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind,state,released_at) VALUES (uuidv7(interval '10 years'),$1,$2,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod','released',clock_timestamp()-interval '2 days') RETURNING id").bind(instance).bind(household).fetch_one(&db.pool).await?;
    let recent:Uuid=sqlx::query_scalar("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind,state,released_at) VALUES (uuidv7(interval '-2 days'),$1,$2,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod','released',clock_timestamp()) RETURNING id").bind(instance).bind(household).fetch_one(&db.pool).await?;
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(r) ORDER BY id) FROM playback_reservations r",
    )
    .fetch_one(&db.pool)
    .await?;
    sqlx::query("CREATE TABLE playback_retention (collision boolean)")
        .execute(&db.pool)
        .await?;
    let mut failed_connection = db.pool.acquire().await?;
    failed_connection.close_on_drop();
    assert!(
        sqlx::migrate!("./migrations")
            .run(&mut *failed_connection)
            .await
            .is_err()
    );
    failed_connection.close().await?;
    let absent:bool=sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_name='playback_reservations' AND column_name='prune_after')").fetch_one(&db.pool).await?;
    assert!(absent);
    sqlx::query("DROP TABLE playback_retention")
        .execute(&db.pool)
        .await?;
    let failed: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(r) ORDER BY id) FROM playback_reservations r",
    )
    .fetch_one(&db.pool)
    .await?;
    assert_eq!(before, failed);
    let mut writer = db.pool.begin().await?;
    let retiring:Uuid=sqlx::query_scalar("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind,state) VALUES (uuidv7(interval '-3 days'),$1,$2,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod','retiring') RETURNING id").bind(instance).bind(household).fetch_one(&mut *writer).await?;
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(r) ORDER BY id) FROM playback_reservations r",
    )
    .fetch_one(&mut *writer)
    .await?;
    let pool = db.pool.clone();
    let upgrading = tokio::spawn(async move { database::upgrade(&pool).await });
    tokio::time::timeout(Duration::from_secs(3),async {
        loop {
            let waiting:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock'").fetch_one(&db.pool).await?;
            if waiting>0 {break Ok::<_,sqlx::Error>(());}
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await??;
    writer.commit().await?;
    upgrading.await??;
    let after: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(r)-'prune_after' ORDER BY id) FROM playback_reservations r",
    )
    .fetch_one(&db.pool)
    .await?;
    assert_eq!(before, after);
    assert_eq!(database::counts(&db.pool).await?, 304);
    let mut lock = db.pool.begin().await?;
    sqlx::query("SELECT id FROM playback_retention WHERE id FOR UPDATE")
        .execute(&mut *lock)
        .await?;
    let owner =
        AdmissionOwner::new(db.pool.clone(), Arc::new(EncryptionKey::generate().0), None).await?;
    tokio::time::sleep(Duration::from_millis(650)).await;
    let mut rows = db.pool.begin().await?;
    let ids:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM playback_reservations WHERE state='released' AND prune_after<=clock_timestamp() ORDER BY prune_after,id LIMIT 128 FOR UPDATE").fetch_all(&mut *rows).await?;
    assert_eq!(ids.len(), 128);
    lock.rollback().await?;
    let blocked = maintain(&owner).await?;
    assert_eq!(blocked.deleted, 0);
    assert_eq!(database::counts(&db.pool).await?, 304);
    rows.rollback().await?;
    assert_eq!(maintain(&owner).await?.deleted, 128);
    assert_eq!(maintain(&owner).await?.deleted, 128);
    assert_eq!(maintain(&owner).await?.deleted, 44);
    assert_eq!(maintain(&owner).await?.deleted, 0);
    assert_eq!(database::counts(&db.pool).await?, 4);
    let remaining: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM playback_reservations")
        .fetch_all(&db.pool)
        .await?;
    assert!(
        [held, future, recent, retiring]
            .into_iter()
            .all(|id| remaining.contains(&id))
    );
    code(
        insert(&db.pool, ids[0], instance, household)
            .await
            .unwrap_err(),
        "PBR01",
    );
    fixture::close(&owner).await?;
    Ok(())
}

pub async fn time_and_guards(db: &Database) -> anyhow::Result<()> {
    database::upgrade(&db.pool).await?;
    let f = Fixture::new(db).await?;
    let owner = f.owner().await?;
    let instance = owner.instance();
    let household = f.actor.user;
    let ids:Vec<Uuid>=sqlx::query_scalar("SELECT uuidv7(shift) FROM unnest(ARRAY[interval '-6 minutes',interval '3 minutes',interval '0 seconds']) shift").fetch_all(&db.pool).await?;
    code(
        insert(&db.pool, ids[0], instance, household)
            .await
            .unwrap_err(),
        "PBR01",
    );
    code(
        insert(&db.pool, ids[1], instance, household)
            .await
            .unwrap_err(),
        "PBR02",
    );
    let mut expired = f.request();
    expired.id = ids[0];
    assert_eq!(
        fixture::start(&owner, expired).await.unwrap_err(),
        E::Expired
    );
    let mut future = f.request();
    future.id = ids[1];
    assert_eq!(
        fixture::start(&owner, future).await.unwrap_err(),
        E::RequestTime
    );
    assert_eq!(database::counts(&db.pool).await?, 0);
    let mut rollback = db.pool.begin().await?;
    sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind) VALUES ($1,$2,$3,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod')").bind(ids[2]).bind(instance).bind(household).execute(&mut *rollback).await?;
    rollback.rollback().await?;
    assert_eq!(database::counts(&db.pool).await?, 0);
    let request = f.request();
    let receipt = fixture::start(&owner, request).await?;
    assert!(receipt.fresh);
    assert!(!fixture::start(&owner, request).await?.fresh);
    assert_eq!(database::counts(&db.pool).await?, 1);
    for statement in [
        "UPDATE playback_reservations SET household_id=uuidv7() WHERE id=$1",
        "UPDATE playback_reservations SET fingerprint='changed' WHERE id=$1",
        "UPDATE playback_reservations SET created_at=clock_timestamp() WHERE id=$1",
        "DELETE FROM playback_reservations WHERE id=$1",
    ] {
        code(
            sqlx::query(statement)
                .bind(request.id)
                .execute(&db.pool)
                .await
                .unwrap_err(),
            "PBR04",
        );
    }
    code(
        sqlx::query("TRUNCATE playback_reservations")
            .execute(&db.pool)
            .await
            .unwrap_err(),
        "0A000",
    );
    code(
        sqlx::query("TRUNCATE playback_reservations CASCADE")
            .execute(&db.pool)
            .await
            .unwrap_err(),
        "PBR04",
    );
    code(
        sqlx::query("UPDATE playback_retention SET cutoff=cutoff-interval '1 millisecond'")
            .execute(&db.pool)
            .await
            .unwrap_err(),
        "PBR04",
    );
    code(
        sqlx::query("DELETE FROM playback_retention")
            .execute(&db.pool)
            .await
            .unwrap_err(),
        "PBR04",
    );
    code(
        sqlx::query("UPDATE playback_retention SET retained_count=0")
            .execute(&db.pool)
            .await
            .unwrap_err(),
        "PBR05",
    );
    code(
        sqlx::query("UPDATE playback_receipt_households SET retained_count=0")
            .execute(&db.pool)
            .await
            .unwrap_err(),
        "PBR05",
    );
    code(sqlx::query("INSERT INTO playback_receipt_households(household_id,retained_count) VALUES (uuidv7(),0)").execute(&db.pool).await.unwrap_err(),"PBR05");
    owner.stop(request.id)?;
    fixture::released(&db.pool, request.id).await?;
    let first: DateTime<Utc> =
        sqlx::query_scalar("SELECT released_at FROM playback_reservations WHERE id=$1")
            .bind(request.id)
            .fetch_one(&db.pool)
            .await?;
    sqlx::query("UPDATE playback_reservations SET state='released',released_at=COALESCE(released_at,clock_timestamp()) WHERE id=$1").bind(request.id).execute(&db.pool).await?;
    let same: DateTime<Utc> =
        sqlx::query_scalar("SELECT released_at FROM playback_reservations WHERE id=$1")
            .bind(request.id)
            .fetch_one(&db.pool)
            .await?;
    assert_eq!(first, same);
    for statement in [
        "UPDATE playback_reservations SET state='held',released_at=NULL WHERE id=$1",
        "UPDATE playback_reservations SET released_at=released_at-interval '2 days' WHERE id=$1",
        "DELETE FROM playback_reservations WHERE id=$1",
    ] {
        code(
            sqlx::query(statement)
                .bind(request.id)
                .execute(&db.pool)
                .await
                .unwrap_err(),
            "PBR04",
        );
    }
    assert_eq!(maintain(&owner).await?.deleted, 0);
    let floor:DateTime<Utc>=sqlx::query_scalar("UPDATE playback_retention SET cutoff=clock_timestamp()+interval '1 hour' WHERE id RETURNING cutoff").fetch_one(&db.pool).await?;
    assert_eq!(
        fixture::start(&owner, f.request()).await.unwrap_err(),
        E::Expired
    );
    assert!(!fixture::start(&owner, request).await?.fresh);
    assert_eq!(maintain(&owner).await?.cutoff, floor);
    let boundary:(i16,i16,i16,i16,i16)=sqlx::query_as("WITH v AS (SELECT id,uuid_extract_timestamp(id) t FROM (SELECT uuidv7() id) n) SELECT playback_receipt_time_error(id,t+interval '5 minutes',t),playback_receipt_time_error(id,t+interval '5 minutes 1 millisecond',t),playback_receipt_time_error(id,t-interval '2 minutes',t-interval '5 minutes'),playback_receipt_time_error(id,t-interval '2 minutes 1 millisecond',t-interval '5 minutes'),playback_receipt_time_error(id,t,t+interval '1 millisecond') FROM v").fetch_one(&db.pool).await?;
    assert_eq!(boundary, (0, 1, 0, 2, 1));
    fixture::close(&owner).await?;
    Ok(())
}

pub async fn quotas(db: &Database) -> anyhow::Result<()> {
    database::legacy(&db.pool).await?;
    let instance: Uuid =
        sqlx::query_scalar("INSERT INTO playback_instances DEFAULT VALUES RETURNING id")
            .fetch_one(&db.pool)
            .await?;
    let household = Uuid::now_v7();
    sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind) SELECT uuidv7(),$1,$2,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod' FROM generate_series(1,4097)").bind(instance).bind(household).execute(&db.pool).await?;
    sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind) SELECT uuidv7(),$1,h.id,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod' FROM (SELECT uuidv7() id FROM generate_series(1,30)) h CROSS JOIN generate_series(1,2048)").bind(instance).execute(&db.pool).await?;
    database::upgrade(&db.pool).await?;
    assert_eq!(database::counts(&db.pool).await?, 65537);
    code(
        insert(&db.pool, Uuid::now_v7(), instance, Uuid::now_v7())
            .await
            .unwrap_err(),
        "PBR03",
    );
    let released:Uuid=sqlx::query_scalar("UPDATE playback_reservations SET state='released',released_at=clock_timestamp() WHERE id=(SELECT id FROM playback_reservations LIMIT 1) RETURNING id").fetch_one(&db.pool).await?;
    let lease:DateTime<Utc>=sqlx::query_scalar("UPDATE playback_reservations SET deadline=clock_timestamp()+interval '60 seconds' WHERE id=(SELECT id FROM playback_reservations WHERE state='held' LIMIT 1) RETURNING deadline").fetch_one(&db.pool).await?;
    assert!(lease > Utc::now());
    let owner =
        AdmissionOwner::new(db.pool.clone(), Arc::new(EncryptionKey::generate().0), None).await?;
    assert_eq!(maintain(&owner).await?.deleted, 0);
    assert_eq!(database::counts(&db.pool).await?, 65537);
    assert!(
        sqlx::query("DELETE FROM playback_reservations WHERE id=$1")
            .bind(released)
            .execute(&db.pool)
            .await
            .is_err()
    );
    sqlx::query("ANALYZE playback_reservations")
        .execute(&db.pool)
        .await?;
    for query in [super::queries::PRUNE, super::queries::PRUNE_SHARED] {
        let mut explain = db.pool.begin().await?;
        let plan: serde_json::Value = sqlx::query(AssertSqlSafe(format!(
            "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
            query
        )))
        .bind(Utc::now())
        .fetch_one(&mut *explain)
        .await?
        .try_get(0)?;
        explain.rollback().await?;
        let plan = plan.to_string();
        assert!(
            plan.contains("playback_receipt_pruning"),
            "prune candidate query must use its partial index: {plan}"
        );
    }
    fixture::close(&owner).await?;
    Ok(())
}

pub async fn quota_races(db: &Database) -> anyhow::Result<()> {
    database::legacy(&db.pool).await?;
    let instance: Uuid =
        sqlx::query_scalar("INSERT INTO playback_instances DEFAULT VALUES RETURNING id")
            .fetch_one(&db.pool)
            .await?;
    let household = Uuid::now_v7();
    sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind) SELECT uuidv7(),$1,$2,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod' FROM generate_series(1,4095)").bind(instance).bind(household).execute(&db.pool).await?;
    sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind) SELECT uuidv7(),$1,h.id,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod' FROM (SELECT uuidv7() id FROM generate_series(1,30)) h CROSS JOIN generate_series(1,2048)").bind(instance).execute(&db.pool).await?;
    database::upgrade(&db.pool).await?;
    assert_eq!(database::counts(&db.pool).await?, 65535);
    let (left, right) = tokio::join!(
        insert(&db.pool, Uuid::now_v7(), instance, household),
        insert(&db.pool, Uuid::now_v7(), instance, household)
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    code(
        left.err().or(right.err()).expect("one quota refusal"),
        "PBR03",
    );
    assert_eq!(database::counts(&db.pool).await?, 65536);
    code(
        insert(&db.pool, Uuid::now_v7(), instance, Uuid::now_v7())
            .await
            .unwrap_err(),
        "PBR03",
    );
    Ok(())
}

pub async fn household_quota(db: &Database) -> anyhow::Result<()> {
    database::legacy(&db.pool).await?;
    let f = Fixture::new(db).await?;
    let instance: Uuid =
        sqlx::query_scalar("INSERT INTO playback_instances DEFAULT VALUES RETURNING id")
            .fetch_one(&db.pool)
            .await?;
    let household = f.actor.user;
    sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind,state,released_at) SELECT uuidv7(),$1,$2,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod','released',clock_timestamp() FROM generate_series(1,4095)").bind(instance).bind(household).execute(&db.pool).await?;
    database::upgrade(&db.pool).await?;
    let (left, right) = tokio::join!(
        insert(&db.pool, Uuid::now_v7(), instance, household),
        insert(&db.pool, Uuid::now_v7(), instance, household)
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    code(
        left.err().or(right.err()).expect("one household refusal"),
        "PBR03",
    );
    insert(&db.pool, Uuid::now_v7(), instance, Uuid::now_v7()).await?;
    assert_eq!(database::counts(&db.pool).await?, 4097);
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_receipt_households")
        .fetch_one(&db.pool)
        .await?;
    let duplicate: Uuid = sqlx::query_scalar("SELECT id FROM playback_reservations LIMIT 1")
        .fetch_one(&db.pool)
        .await?;
    code(sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind) VALUES ($1,$2,$3,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod') ON CONFLICT DO NOTHING").bind(duplicate).bind(instance).bind(Uuid::now_v7()).execute(&db.pool).await.unwrap_err(),"PBR04");
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_receipt_households")
        .fetch_one(&db.pool)
        .await?;
    assert_eq!(before, after);
    assert_eq!(database::counts(&db.pool).await?, 4097);
    let owner = f.owner().await?;
    assert_eq!(
        fixture::start(&owner, f.request()).await.unwrap_err(),
        E::ReceiptCapacity
    );
    fixture::close(&owner).await?;
    Ok(())
}

pub async fn household_cleanup_backlog(db: &Database) -> anyhow::Result<()> {
    database::legacy(&db.pool).await?;
    let instance: Uuid =
        sqlx::query_scalar("INSERT INTO playback_instances DEFAULT VALUES RETURNING id")
            .fetch_one(&db.pool)
            .await?;
    sqlx::query("WITH households AS MATERIALIZED (SELECT uuidv7() id FROM generate_series(1,32)) INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind) SELECT uuidv7(),$1,h.id,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod' FROM households h CROSS JOIN generate_series(1,2048)").bind(instance).execute(&db.pool).await?;
    sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind,state,released_at) SELECT uuidv7(interval '-2 days'),$1,uuidv7(),json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod','released',clock_timestamp()-interval '25 hours' FROM generate_series(1,128)").bind(instance).execute(&db.pool).await?;
    database::upgrade(&db.pool).await?;
    assert_eq!(database::counts(&db.pool).await?, 65664);
    sqlx::query("ANALYZE playback_reservations")
        .execute(&db.pool)
        .await?;
    let mut explain = db.pool.begin().await?;
    let plan: serde_json::Value = sqlx::query(AssertSqlSafe(format!(
        "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
        super::queries::PRUNE
    )))
    .bind(Utc::now())
    .fetch_one(&mut *explain)
    .await?
    .try_get(0)?;
    assert_eq!(plan[0]["Plan"]["Actual Rows"].as_f64(), Some(128.0));
    assert!(plan.to_string().contains("playback_receipt_pruning"));
    let metadata: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_receipt_households")
        .fetch_one(&mut *explain)
        .await?;
    assert_eq!(metadata, 32);
    let lookup:serde_json::Value=sqlx::query("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) SELECT 1 FROM ONLY playback_reservations WHERE household_id=$1 LIMIT 1 FOR KEY SHARE").bind(Uuid::now_v7()).fetch_one(&mut *explain).await?.try_get(0)?;
    assert!(
        lookup
            .to_string()
            .contains("playback_receipt_household_rows")
    );
    println!(
        "RETENTION_PLAN {}",
        serde_json::json!({"deleted_rows":plan[0]["Plan"]["Actual Rows"],"shared_hit_blocks":plan[0]["Plan"]["Shared Hit Blocks"],"shared_read_blocks":plan[0]["Plan"]["Shared Read Blocks"],"triggers":plan[0]["Triggers"]})
    );
    explain.rollback().await?;
    assert_eq!(database::counts(&db.pool).await?, 65664);
    let owner =
        AdmissionOwner::new(db.pool.clone(), Arc::new(EncryptionKey::generate().0), None).await?;
    assert!(maintain(&owner).await?.deleted <= 128);
    assert_eq!(database::counts(&db.pool).await?, 65536);
    let metadata: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_receipt_households")
        .fetch_one(&db.pool)
        .await?;
    assert_eq!(metadata, 32);
    fixture::close(&owner).await?;
    Ok(())
}
