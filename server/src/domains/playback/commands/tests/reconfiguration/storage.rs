// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{Database, harness::*, support};
use crate::services::playback_admission::vod::revisions::retention;
use serde_json::json;
use sqlx::AssertSqlSafe;
use uuid::Uuid;

async fn refused(pool: &sqlx::PgPool, sql: &str, id: Uuid) -> anyhow::Result<()> {
    let result = sqlx::query(AssertSqlSafe(sql.to_owned()))
        .bind(id)
        .execute(pool)
        .await;
    anyhow::ensure!(
        result
            .as_ref()
            .err()
            .and_then(|e| e.as_database_error())
            .is_some_and(|e| matches!(e.code().as_deref(), Some("PBR04" | "23503" | "23514"))),
        "guard did not refuse: {sql}: {result:?}"
    );
    Ok(())
}

pub(in super::super) async fn guards(db: &Database) -> anyhow::Result<()> {
    support::run(db,async |f| {
        probe(db,f).await?;
        let id=Uuid::now_v7();f.start(id,false,false).await?;f.ready(id).await?;
        let (config,tag)=configuration(f,id).await?;
        let op=Uuid::now_v7();
        anyhow::ensure!(post(f,id,&tag,&change(op,parse(&config["output_revision"])?,json!({"kind":"seek","position_ms":1000}))).await?.0==reqwest::StatusCode::ACCEPTED);
        completed(f,id,op).await?;
        for sql in [
            "UPDATE playback_vod_commands SET protocol=1 WHERE reservation_id=$1",
            "UPDATE playback_vod_controls SET retained_operations=0 WHERE reservation_id=$1",
            "UPDATE playback_vod_controls SET claim_encodes=false,current_output=NULL WHERE reservation_id=$1",
            "DELETE FROM playback_vod_controls WHERE reservation_id=$1",
            "DELETE FROM playback_vod_outputs WHERE reservation_id=$1",
            "DELETE FROM playback_vod_operations WHERE reservation_id=$1",
            "UPDATE playback_vod_operations SET state='accepted' WHERE reservation_id=$1",
            "UPDATE playback_vod_outputs SET source_hash=decode(repeat('00',32),'hex') WHERE reservation_id=$1",
        ] {refused(&db.pool,sql,id).await?;}
        sqlx::query("UPDATE playback_vod_operations SET prune_after=clock_timestamp() WHERE id=$1").bind(op).execute(&db.pool).await?;
        let young:bool=sqlx::query_scalar("SELECT prune_after IS NULL FROM playback_vod_operations WHERE id=$1").bind(op).fetch_one(&db.pool).await?;
        anyhow::ensure!(young);
        for table in ["playback_vod_outputs","playback_vod_controls","playback_vod_operations","playback_vod_operation_retention"] {
            let result=sqlx::query(AssertSqlSafe(format!("TRUNCATE {table} CASCADE"))).execute(&db.pool).await;
            anyhow::ensure!(result.is_err());
        }
        f.stop(id).await?;
        refused(&db.pool,"DELETE FROM playback_reservations WHERE id=$1",id).await?;
        let mut tx=db.pool.begin().await?;
        anyhow::ensure!(retention::prune(&mut tx).await?==0);
        tx.commit().await?;
        let (actual,global,local):(i64,i64,i32)=sqlx::query_as("SELECT (SELECT count(*) FROM playback_vod_operations),(SELECT retained_count FROM playback_vod_operation_retention WHERE id),(SELECT retained_operations FROM playback_vod_controls WHERE reservation_id=$1)").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(actual==1 && global==1 && local==1);
        let mut tx=db.pool.begin().await?;
        sqlx::query("ALTER TABLE playback_vod_operations DISABLE TRIGGER playback_vod_operation_guard").execute(&mut *tx).await?;
        sqlx::query("UPDATE playback_vod_operations SET terminal_at=clock_timestamp()-interval '25 hours',cleanup_at=clock_timestamp()-interval '25 hours',prune_after=clock_timestamp()-interval '1 hour' WHERE id=$1").bind(op).execute(&mut *tx).await?;
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *tx).await?;
        sqlx::query("ALTER TABLE playback_vod_operations ENABLE TRIGGER playback_vod_operation_guard").execute(&mut *tx).await?;
        sqlx::query("UPDATE playback_vod_operation_retention SET cutoff=clock_timestamp()+interval '1 minute' WHERE id").execute(&mut *tx).await?;
        tx.commit().await?;
        let plan:serde_json::Value=sqlx::query_scalar("EXPLAIN (FORMAT JSON) SELECT id FROM playback_vod_operations WHERE prune_after<=clock_timestamp() ORDER BY prune_after,id LIMIT 128").fetch_one(&db.pool).await?;
        anyhow::ensure!(plan.to_string().contains("Limit"));
        let mut tx=db.pool.begin().await?;
        anyhow::ensure!(retention::prune(&mut tx).await?==1);
        tx.commit().await?;
        let (actual,global,local):(i64,i64,i32)=sqlx::query_as("SELECT (SELECT count(*) FROM playback_vod_operations),(SELECT retained_count FROM playback_vod_operation_retention WHERE id),(SELECT retained_operations FROM playback_vod_controls WHERE reservation_id=$1)").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(actual==0 && global==0 && local==0);
        let replay=f.json(reqwest::Method::GET,&format!("/api/v1/playback-commands/{id}/operations/{op}/retirement"),None).await?;
        anyhow::ensure!(replay.0==reqwest::StatusCode::GONE);
        Ok(())
    }).await
}
