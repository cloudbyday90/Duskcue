// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database, support};
use reqwest::Method;
use serde_json::json;
use uuid::Uuid;

pub(super) async fn pruning(db: &database::Database) -> anyhow::Result<()> {
    support::run(db, async |f| {
        let id = Uuid::now_v7();
        let (_, reply) = f.json(Method::POST, &format!("/api/v1/playback-commands/{id}/withdraw"), Some(json!({"protocol":"duskcue-vod-command/2"}))).await?;
        anyhow::ensure!(reply["status"] == "settled");
        sqlx::query("INSERT INTO playback_start_intents(id,household_id,session_id,credential_version,transport,instance_id,admission_fenced,preparation_complete) SELECT uuidv7(),$1,$2,1,0,$3,true,true FROM generate_series(1,129)")
            .bind(f.base.actor.user).bind(f.base.actor.session).bind(f.owner.instance()).execute(&db.pool).await?;
        let held = Uuid::now_v7();
        let legacy=Uuid::now_v7();
        sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind) VALUES ($1,$2,$3,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod')")
            .bind(legacy).bind(f.owner.instance()).bind(f.base.actor.user).execute(&db.pool).await?;
        sqlx::query("INSERT INTO playback_start_intents(id,household_id,session_id,credential_version,transport,profile_id,profile_revision,intent_hash,instance_id) VALUES ($1,$2,$3,1,0,$4,1,decode(repeat('11',32),'hex'),$5)")
            .bind(held).bind(f.base.actor.user).bind(f.base.actor.session).bind(f.base.actor.profile).bind(f.owner.instance()).execute(&db.pool).await?;
        let mut tx = db.pool.begin().await?;
        sqlx::query("ALTER TABLE playback_start_intents DISABLE TRIGGER playback_start_intent_update").execute(&mut *tx).await?;
        sqlx::query("UPDATE playback_start_intents SET settled_at=clock_timestamp()-interval '25 hours',prune_after=clock_timestamp()-interval '1 hour' WHERE admission_fenced AND preparation_complete").execute(&mut *tx).await?;
        sqlx::query("ALTER TABLE playback_start_intents ENABLE TRIGGER playback_start_intent_update").execute(&mut *tx).await?;
        sqlx::query("UPDATE playback_retention SET cutoff=clock_timestamp()+interval '1 minute' WHERE id").execute(&mut *tx).await?;
        tx.commit().await?;
        let rollback = sqlx::query("UPDATE playback_retention SET cutoff=clock_timestamp()-interval '1 hour' WHERE id").execute(&db.pool).await;
        anyhow::ensure!(rollback.is_err());
        let mut tx = db.pool.begin().await?;
        sqlx::query("SET LOCAL enable_seqscan=off").execute(&mut *tx).await?;
        let plan: serde_json::Value = sqlx::query_scalar("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) SELECT id FROM playback_start_intents WHERE settled_at IS NOT NULL AND prune_after<=$1 ORDER BY prune_after,id LIMIT 64")
            .bind(chrono::Utc::now()).fetch_one(&mut *tx).await?;
        anyhow::ensure!(plan.to_string().contains("playback_start_intent_pruning") && plan.to_string().contains("Index Cond"));
        tx.rollback().await?;
        let first:i32=sqlx::query_scalar("SELECT playback_start_prune(64)").fetch_one(&db.pool).await?;
        let second:i32=sqlx::query_scalar("SELECT playback_start_prune(128)").fetch_one(&db.pool).await?;
        anyhow::ensure!(first==64 && second==66);
        let (actual, accounted):(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM playback_start_intents),(SELECT retained_count FROM playback_retention WHERE id)").fetch_one(&db.pool).await?;
        anyhow::ensure!(actual==1 && accounted==2);
        let (status,_)=f.json(Method::GET,&format!("/api/v1/playback-commands/{legacy}/settlement"),None).await?;
        anyhow::ensure!(status==409,"an old UUID is not proof that an unindexed legacy reservation is settled");
        let (_, reply)=f.json(Method::GET,&format!("/api/v1/playback-commands/{id}/settlement"),None).await?;
        anyhow::ensure!(reply["status"]=="settled" && reply["admission_fenced"]==true);
        let (status,_)=f.json(Method::POST,"/api/v1/playback-commands",Some(f.start_body(id,false,false))).await?;
        anyhow::ensure!(status==410);
        let held_present:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM playback_start_intents WHERE id=$1 AND NOT preparation_complete)").bind(held).fetch_one(&db.pool).await?;
        anyhow::ensure!(held_present,"age and replay floor must never prune uncompleted ownership");
        Ok(())
    }).await
}

pub(super) async fn instance_recovery(db: &database::Database) -> anyhow::Result<()> {
    use crate::services::playback_admission::operator;
    support::run(db, async |f| {
        let foreign:Uuid=sqlx::query_scalar("INSERT INTO playback_instances(start_intent_protocol) VALUES (2) RETURNING id").fetch_one(&db.pool).await?;
        let id=Uuid::now_v7();
        sqlx::query("INSERT INTO playback_start_intents(id,household_id,session_id,credential_version,transport,profile_id,profile_revision,intent_hash,instance_id) VALUES ($1,$2,$3,1,0,$4,1,decode(repeat('11',32),'hex'),$5)")
            .bind(id).bind(f.base.actor.user).bind(f.base.actor.session).bind(f.base.actor.profile).bind(foreign).execute(&db.pool).await?;
        let (_, pending)=f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/withdraw"),Some(json!({"protocol":"duskcue-vod-command/2"}))).await?;
        anyhow::ensure!(pending["status"]=="uncertain" && pending["admission_fenced"]==true && pending["preparation_complete"]==false);
        let revision=operator::fence(&db.pool,foreign,1).await?;
        anyhow::ensure!(operator::reconcile(&db.pool,foreign,revision,false).await.is_err());
        anyhow::ensure!(operator::reconcile(&db.pool,foreign,revision+1,true).await.is_err());
        operator::reconcile(&db.pool,foreign,revision,true).await?;
        let (_, settled)=f.json(Method::GET,&format!("/api/v1/playback-commands/{id}/settlement"),None).await?;
        anyhow::ensure!(settled["status"]=="settled" && settled["preparation_complete"]==true);
        anyhow::ensure!(sqlx::query("INSERT INTO playback_instances DEFAULT VALUES").execute(&db.pool).await.is_err());
        Ok(())
    }).await
}

pub(super) async fn inconsistent_migration(db: &database::Database) -> anyhow::Result<()> {
    let all = sqlx::migrate!("./migrations");
    sqlx::migrate::Migrator::with_migrations(
        all.iter()
            .filter(|m| m.version < 20261028120000)
            .cloned()
            .collect(),
    )
    .run(&db.pool)
    .await?;
    let instance: Uuid =
        sqlx::query_scalar("INSERT INTO playback_instances DEFAULT VALUES RETURNING id")
            .fetch_one(&db.pool)
            .await?;
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind) VALUES ($1,$2,$3,json_build_object('session',uuidv7(),'profile',uuidv7())::text,'vod')").bind(id).bind(instance).bind(Uuid::now_v7()).execute(&db.pool).await?;
    sqlx::query("INSERT INTO playback_vod_commands(reservation_id,intent_hash,policy_hash,source_hash,delivery_mode,encodes,source_4k,state) VALUES ($1,decode(repeat('11',32),'hex'),decode(repeat('22',32),'hex'),decode(repeat('33',32),'hex'),'direct_play',false,false,'failed')").bind(id).execute(&db.pool).await?;
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(c) FROM playback_vod_commands c WHERE reservation_id=$1",
    )
    .bind(id)
    .fetch_one(&db.pool)
    .await?;
    let mut connection = db.pool.acquire().await?;
    connection.close_on_drop();
    anyhow::ensure!(all.run(&mut *connection).await.is_err());
    connection.close().await?;
    let absent:bool=sqlx::query_scalar("SELECT to_regclass('public.playback_start_intents') IS NULL AND NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_name='playback_instances' AND column_name='start_intent_protocol')").fetch_one(&db.pool).await?;
    let after: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(c) FROM playback_vod_commands c WHERE reservation_id=$1",
    )
    .bind(id)
    .fetch_one(&db.pool)
    .await?;
    anyhow::ensure!(
        absent && before == after,
        "inconsistent actor attribution must roll back the entire upgrade"
    );
    Ok(())
}

pub(super) async fn legacy_compatibility(db: &database::Database) -> anyhow::Result<()> {
    let all = sqlx::migrate!("./migrations");
    sqlx::migrate::Migrator::with_migrations(
        all.iter()
            .filter(|m| m.version < 20261028120000)
            .cloned()
            .collect(),
    )
    .run(&db.pool)
    .await?;
    let old: Uuid =
        sqlx::query_scalar("INSERT INTO playback_instances DEFAULT VALUES RETURNING id")
            .fetch_one(&db.pool)
            .await?;
    all.run(&db.pool).await?;
    let state = db.state();
    let owner = crate::services::playback_admission::AdmissionOwner::attach(&state).await?;
    let _routes = super::super::router(state.clone());
    let result = async {
        anyhow::ensure!(super::super::capability(&state).await.is_none());
        let revision =
            crate::services::playback_admission::operator::fence(&db.pool, old, 1).await?;
        anyhow::ensure!(
            super::super::capability(&state).await.is_none(),
            "fencing alone does not confirm old preparation is gone"
        );
        crate::services::playback_admission::operator::reconcile(&db.pool, old, revision, true)
            .await?;
        anyhow::ensure!(super::super::capability(&state).await.is_some());
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let cleanup = super::fixture::close(&owner).await;
    result?;
    cleanup
}
