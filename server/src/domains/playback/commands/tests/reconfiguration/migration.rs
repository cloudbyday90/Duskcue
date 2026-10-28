// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::Database;
use serde_json::{Value, json};
use uuid::Uuid;

async fn previous(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    let all = sqlx::migrate!("./migrations");
    sqlx::migrate::Migrator::with_migrations(
        all.iter()
            .filter(|m| m.version < 20261027180000)
            .cloned()
            .collect(),
    )
    .run(pool)
    .await?;
    Ok(())
}

async fn insert(
    pool: &sqlx::PgPool,
    state: &str,
    completed: bool,
    generation: bool,
) -> anyhow::Result<Uuid> {
    let instance: Uuid =
        sqlx::query_scalar("INSERT INTO playback_instances DEFAULT VALUES RETURNING id")
            .fetch_one(pool)
            .await?;
    let id = Uuid::now_v7();
    let household = Uuid::now_v7();
    sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind) VALUES ($1,$2,$3,jsonb_build_object('user',$3::uuid,'session',uuidv7(),'profile',uuidv7(),'version',1,'profile_revision',1,'transport',0,'subject',jsonb_build_object('Vod',jsonb_build_object('file',uuidv7(),'item',uuidv7())))::text,'vod')").bind(id).bind(instance).bind(household).execute(pool).await?;
    let history = (state == "ready").then(Uuid::now_v7);
    let stamp = chrono::Utc::now();
    sqlx::query("INSERT INTO playback_vod_commands(reservation_id,intent_hash,policy_hash,source_hash,delivery_mode,encodes,source_4k,state,history_id,history_started_at,generation_id,result,resources_completed,cleanup_uncertain) VALUES ($1,decode(repeat('11',32),'hex'),decode(repeat('22',32),'hex'),decode(repeat('33',32),'hex'),'transcode',true,false,$2,$3,$4,$5,$6,$7,$8)")
        .bind(id).bind(state).bind(history).bind(history.map(|_|stamp)).bind(generation.then(Uuid::now_v7)).bind(history.map(|h|json!({"session_id":h,"position_ms":0}))).bind(completed).bind(state=="failed"&&!completed).execute(pool).await?;
    if completed {
        sqlx::query("UPDATE playback_reservations SET state='retiring' WHERE id=$1")
            .bind(id)
            .execute(pool)
            .await?;
        sqlx::query("UPDATE playback_reservations SET state='released' WHERE id=$1")
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(id)
}

pub(in super::super) async fn backfill(db: &Database) -> anyhow::Result<()> {
    previous(&db.pool).await?;
    let pending = insert(&db.pool, "preparing", false, false).await?;
    insert(&db.pool, "ready", false, true).await?;
    let uncertain = insert(&db.pool, "failed", false, false).await?;
    let done = insert(&db.pool, "failed", true, false).await?;
    let before: Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(c) ORDER BY reservation_id) FROM playback_vod_commands c",
    )
    .fetch_one(&db.pool)
    .await?;
    sqlx::migrate!("./migrations").run(&db.pool).await?;
    let after:Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(c)-'protocol' ORDER BY reservation_id) FROM playback_vod_commands c").fetch_one(&db.pool).await?;
    anyhow::ensure!(before == after);
    let (outputs,controls,operations):(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM playback_vod_outputs WHERE uuid_extract_version(id)=7),(SELECT count(*) FROM playback_vod_controls),(SELECT count(*) FROM playback_vod_operations)").fetch_one(&db.pool).await?;
    anyhow::ensure!(outputs == 4 && controls == 4 && operations == 0);
    let intents:i64=sqlx::query_scalar("SELECT count(*) FROM playback_start_intents i JOIN playback_reservations r ON r.id=i.id JOIN playback_vod_commands c ON c.reservation_id=i.id WHERE i.preparation_complete AND i.promoted AND i.intent_hash=c.intent_hash AND i.session_id=(r.fingerprint::jsonb->>'session')::uuid AND i.credential_version=(r.fingerprint::jsonb->>'version')::bigint AND i.transport=(r.fingerprint::jsonb->>'transport')::smallint AND i.profile_id=(r.fingerprint::jsonb->>'profile')::uuid").fetch_one(&db.pool).await?;
    anyhow::ensure!(intents == 4);
    let accounting: i64 =
        sqlx::query_scalar("SELECT retained_count FROM playback_retention WHERE id")
            .fetch_one(&db.pool)
            .await?;
    anyhow::ensure!(
        accounting == 4,
        "backfill must not count intent/reservation pairs twice"
    );
    for id in [pending, uncertain] {
        let (claim,resources,generation):(bool,bool,Option<Uuid>)=sqlx::query_as("SELECT v.claim_encodes,o.resources_completed,o.generation_id FROM playback_vod_controls v JOIN playback_vod_outputs o ON o.id=v.current_output WHERE v.reservation_id=$1").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(claim && !resources && generation.is_none());
    }
    let claim: bool = sqlx::query_scalar(
        "SELECT claim_encodes FROM playback_vod_controls WHERE reservation_id=$1",
    )
    .bind(done)
    .fetch_one(&db.pool)
    .await?;
    anyhow::ensure!(!claim);
    sqlx::migrate!("./migrations").run(&db.pool).await?;
    anyhow::ensure!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM playback_vod_outputs")
            .fetch_one(&db.pool)
            .await?
            == 4
    );
    Ok(())
}

pub(in super::super) async fn inconsistent(db: &Database) -> anyhow::Result<()> {
    previous(&db.pool).await?;
    let id = insert(&db.pool, "ready", false, false).await?;
    let mut failed = db.pool.acquire().await?;
    failed.close_on_drop();
    anyhow::ensure!(
        sqlx::migrate!("./migrations")
            .run(&mut *failed)
            .await
            .is_err()
    );
    failed.close().await?;
    let absent: bool =
        sqlx::query_scalar("SELECT to_regclass('public.playback_vod_outputs') IS NULL")
            .fetch_one(&db.pool)
            .await?;
    anyhow::ensure!(absent);
    sqlx::query("UPDATE playback_vod_commands SET state='failed' WHERE reservation_id=$1")
        .bind(id)
        .execute(&db.pool)
        .await?;
    sqlx::migrate!("./migrations").run(&db.pool).await?;
    anyhow::ensure!(
        sqlx::query_scalar::<_, bool>(
            "SELECT claim_encodes FROM playback_vod_controls WHERE reservation_id=$1"
        )
        .bind(id)
        .fetch_one(&db.pool)
        .await?
    );
    Ok(())
}
