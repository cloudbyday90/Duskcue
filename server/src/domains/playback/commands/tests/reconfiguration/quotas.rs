// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{Database, harness::*, support};
use crate::services::playback_admission::vod::revisions::retention;
use reqwest::{Method, StatusCode};
use serde_json::json;
use sqlx::AssertSqlSafe;
use uuid::Uuid;

async fn seed(
    pool: &sqlx::PgPool,
    template: Uuid,
    household: Uuid,
    count: i32,
    roots: i32,
    existing: Option<Uuid>,
) -> anyhow::Result<Vec<Uuid>> {
    let mut tx = pool.begin().await?;
    for table in [
        "playback_vod_controls",
        "playback_vod_outputs",
        "playback_vod_operations",
        "playback_vod_operation_retention",
    ] {
        sqlx::query(AssertSqlSafe(format!(
            "ALTER TABLE {table} DISABLE TRIGGER USER"
        )))
        .execute(&mut *tx)
        .await?;
    }
    let ids =
        sqlx::query_scalar("SELECT qualification_vod_seed($1,$2,$3,$4) FROM generate_series(1,$5)")
            .bind(template)
            .bind(household)
            .bind(count)
            .bind(existing)
            .bind(roots)
            .fetch_all(&mut *tx)
            .await?;
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *tx)
        .await?;
    for table in [
        "playback_vod_controls",
        "playback_vod_outputs",
        "playback_vod_operations",
        "playback_vod_operation_retention",
    ] {
        sqlx::query(AssertSqlSafe(format!(
            "ALTER TABLE {table} ENABLE TRIGGER USER"
        )))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(ids)
}

async fn capacity(pool: &sqlx::PgPool, viewer: Uuid, household: Uuid) -> anyhow::Result<()> {
    let id = Uuid::now_v7();
    let result=sqlx::query("INSERT INTO playback_vod_operations(id,reservation_id,household_id,intent_hash,condition,action,expected_output,candidate_output) SELECT $1,$2,$3,decode(repeat('22',32),'hex'),'\"fixture\"','seek',current_output,$4 FROM playback_vod_controls WHERE reservation_id=$2").bind(id).bind(viewer).bind(household).bind(Uuid::now_v7()).execute(pool).await;
    anyhow::ensure!(
        result
            .as_ref()
            .err()
            .and_then(|e| e.as_database_error())
            .is_some_and(|e| e.code().as_deref() == Some("PBR03")),
        "receipt quota not enforced: {result:?}"
    );
    Ok(())
}

pub(in super::super) async fn full(db: &Database) -> anyhow::Result<()> {
    support::run(db,async |f| {
        probe(db,f).await?;
        let id=Uuid::now_v7();f.start(id,false,false).await?;f.ready(id).await?;
        let _maintenance=f.owner.reserve_fixture_maintenance()?;
        sqlx::raw_sql(AssertSqlSafe(include_str!("seed.sql"))).execute(&db.pool).await?;
        seed(&db.pool,id,f.base.actor.user,256,1,Some(id)).await?;
        let (config,tag)=configuration(f,id).await?;
        let body=change(Uuid::now_v7(),parse(&config["output_revision"])?,json!({"kind":"seek","position_ms":1000}));
        anyhow::ensure!(post(f,id,&tag,&body).await?.0==StatusCode::SERVICE_UNAVAILABLE);
        for roots in [8,8,8,7] {seed(&db.pool,id,f.base.actor.user,256,roots,None).await?;sqlx::query("CHECKPOINT").execute(&db.pool).await?;}
        let household_target=seed(&db.pool,id,f.base.actor.user,0,1,None).await?[0];
        capacity(&db.pool,household_target,f.base.actor.user).await?;
        sqlx::query("SELECT qualification_vod_finish($1)").bind(household_target).execute(&db.pool).await?;
        for _ in 0..15 {
            let household=Uuid::now_v7();
            for _ in 0..4 {
                seed(&db.pool,id,household,256,8,None).await?;
                sqlx::query("CHECKPOINT").execute(&db.pool).await?;
                anyhow::ensure!(f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/renew"),Some(json!({}))).await?.0==StatusCode::OK);
            }
        }
        let household=Uuid::now_v7();
        let global_target=seed(&db.pool,id,household,0,1,None).await?[0];
        capacity(&db.pool,global_target,household).await?;
        sqlx::query("SELECT qualification_vod_finish($1)").bind(global_target).execute(&db.pool).await?;
        let (actual,counter,sum):(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM playback_vod_operations),(SELECT retained_count FROM playback_vod_operation_retention WHERE id),(SELECT sum(retained_operations)::bigint FROM playback_vod_controls)").fetch_one(&db.pool).await?;
        anyhow::ensure!(actual==131072&&counter==actual&&sum==actual);
        let mut tx=db.pool.begin().await?;
        sqlx::query("ALTER TABLE playback_vod_operations DISABLE TRIGGER playback_vod_operation_guard").execute(&mut *tx).await?;
        sqlx::query("WITH candidates AS (SELECT id FROM playback_vod_operations ORDER BY id LIMIT 512) UPDATE playback_vod_operations SET terminal_at=clock_timestamp()-interval '25 hours',cleanup_at=clock_timestamp()-interval '25 hours',prune_after=clock_timestamp()-interval '1 hour' WHERE id IN (SELECT id FROM candidates)").execute(&mut *tx).await?;
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *tx).await?;
        sqlx::query("ALTER TABLE playback_vod_operations ENABLE TRIGGER playback_vod_operation_guard").execute(&mut *tx).await?;
        sqlx::query("UPDATE playback_vod_operation_retention SET cutoff=clock_timestamp()+interval '1 minute',scan_after=NULL,scan_id=NULL WHERE id").execute(&mut *tx).await?;
        tx.commit().await?;
        sqlx::query("ANALYZE playback_vod_operations").execute(&db.pool).await?;
        let sql=format!("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",retention::CANDIDATES);
        let plan:serde_json::Value=sqlx::query_scalar(AssertSqlSafe(sql)).bind(None::<chrono::DateTime<chrono::Utc>>).bind(None::<Uuid>).bind(chrono::Utc::now()).fetch_one(&db.pool).await?;
        anyhow::ensure!(plan.to_string().contains("playback_vod_operation_prune"),"large receipt plan lost eligibility index: {plan}");
        anyhow::ensure!(plan[0]["Execution Time"].as_f64().is_some_and(|n|n<1500.0));
        println!("VOD_OPERATION_PLAN={plan}");
        let mut removed=0;
        for _ in 0..4 {
            let count=f.owner.sql(true,move|conn|Box::pin(async move {retention::prune(conn).await})).await?;anyhow::ensure!(count<=64);removed+=count;
        }
        anyhow::ensure!(removed==256,"bounded maintenance did not advance: {removed}");
        let cursor:bool=sqlx::query_scalar("SELECT scan_after IS NOT NULL AND scan_id IS NOT NULL FROM playback_vod_operation_retention WHERE id").fetch_one(&db.pool).await?;
        anyhow::ensure!(cursor);
        let (actual,counter,sum):(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM playback_vod_operations),(SELECT retained_count FROM playback_vod_operation_retention WHERE id),(SELECT sum(retained_operations)::bigint FROM playback_vod_controls)").fetch_one(&db.pool).await?;
        anyhow::ensure!(actual==131072-i64::try_from(removed)?&&counter==actual&&sum==actual);
        f.stop(id).await?;
        Ok(())
    }).await
}
