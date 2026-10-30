// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

pub(super) mod actors;
pub(super) mod adversity;
pub(super) mod copied;
pub(super) mod faults;
pub(super) mod harness;
#[cfg(target_os = "linux")]
pub(super) mod linux;
mod media;
pub(super) mod migration;
pub(super) mod quotas;
pub(super) mod storage;

use super::{database::Database, support};
use harness::*;
use reqwest::{Method, StatusCode};
use serde_json::json;
use uuid::Uuid;

pub(super) async fn direct(db: &Database) -> anyhow::Result<()> {
    support::run(db,async |f| {
        probe(db,f).await?;
        let id=Uuid::now_v7();f.start(id,false,false).await?;
        let ready=f.ready(id).await?;
        let old_output=parse(&ready["ready"]["output_revision"])?;
        let history=parse(&ready["ready"]["session_id"])?;
        let initial_head:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(c) FROM playback_vod_commands c WHERE reservation_id=$1").bind(id).fetch_one(&db.pool).await?;
        let deadline:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("SELECT deadline FROM playback_reservations WHERE id=$1").bind(id).fetch_one(&db.pool).await?;
        let (_,old_tag)=configuration(f,id).await?;
        let report=json!({"protocol":PROTOCOL,"output_revision":old_output,"sequence":1,"position_ms":1000,"playback_state":"paused"});
        anyhow::ensure!(f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/checkpoint"),Some(report.clone())).await?.0==StatusCode::OK);
        let event_count:i64=sqlx::query_scalar("SELECT count(*) FROM play_events WHERE play_session_id=$1").bind(history).fetch_one(&db.pool).await?;
        anyhow::ensure!(f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/checkpoint"),Some(report)).await?.0==StatusCode::OK);
        let repeated:i64=sqlx::query_scalar("SELECT count(*) FROM play_events WHERE play_session_id=$1").bind(history).fetch_one(&db.pool).await?;
        anyhow::ensure!(event_count==repeated);
        let (config,tag)=configuration(f,id).await?;
        anyhow::ensure!(old_tag!=tag && config["playback_state"]=="paused");
        let operation=Uuid::now_v7();
        let body=change(operation,old_output,json!({"kind":"seek","position_ms":5000}));
        anyhow::ensure!(post(f,id,&old_tag,&body).await?.0==StatusCode::PRECONDITION_FAILED);
        let accepted=post(f,id,&tag,&body).await?;
        anyhow::ensure!(accepted.0==StatusCode::ACCEPTED);
        anyhow::ensure!(completed(f,id,operation).await?["status"]=="completed");
        let next=f.ready(id).await?;
        let new_output=parse(&next["ready"]["output_revision"])?;
        anyhow::ensure!(new_output!=old_output && next["ready"]["session_id"]==history.to_string() && next["ready"]["position_ms"]==5000);
        anyhow::ensure!(!f.response(Method::GET,ready["stream_url"].as_str().unwrap(),None,false).await?.status().is_success());
        anyhow::ensure!(f.response(Method::GET,next["stream_url"].as_str().unwrap(),None,false).await?.status()==StatusCode::OK);
        anyhow::ensure!(post(f,id,&tag,&body).await?.0==StatusCode::ACCEPTED);
        let (count,viewers):(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM playback_vod_operations WHERE reservation_id=$1),(SELECT count(*) FROM playback_reservations WHERE id=$1 AND state='held')").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(count==1 && viewers==1);
        let head:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(c) FROM playback_vod_commands c WHERE reservation_id=$1").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(head==initial_head);
        let position:i32=sqlx::query_scalar("SELECT (metadata->>'current_position_ms')::integer FROM play_sessions WHERE id=$1").bind(history).fetch_one(&db.pool).await?;
        anyhow::ensure!(position==1000);
        let checkpoint=f.json(Method::GET,&format!("/api/v1/playback-commands/{id}/checkpoint"),None).await?;
        anyhow::ensure!(checkpoint.1["sequence"]==0 && checkpoint.1["position_ms"].is_null());
        let stale=json!({"protocol":PROTOCOL,"output_revision":old_output,"sequence":2,"position_ms":5100,"playback_state":"playing"});
        anyhow::ensure!(f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/checkpoint"),Some(stale)).await?.0==StatusCode::CONFLICT);
        let (_,after)=configuration(f,id).await?;
        let report=json!({"protocol":PROTOCOL,"output_revision":new_output,"sequence":1,"position_ms":5100,"playback_state":"paused"});
        anyhow::ensure!(f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/checkpoint"),Some(report)).await?.0==StatusCode::OK);
        anyhow::ensure!(configuration(f,id).await?.1==after);
        let after_deadline:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("SELECT deadline FROM playback_reservations WHERE id=$1").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(after_deadline==deadline);
        f.stop(id).await?;
        let final_position:i32=sqlx::query_scalar("SELECT (metadata->>'current_position_ms')::integer FROM play_sessions WHERE id=$1").bind(history).fetch_one(&db.pool).await?;
        anyhow::ensure!(final_position==5100);
        Ok(())
    }).await
}

pub(super) async fn encoded(db: &Database) -> anyhow::Result<()> {
    support::run(db,async |f| {
        probe(db,f).await?;
        let id=Uuid::now_v7();f.start(id,true,true).await?;
        let initial=f.ready(id).await?;
        let history=initial["ready"]["session_id"].clone();
        sqlx::query("UPDATE streaming_policies SET max_transcode_streams=0,max_streams=0 WHERE id=$1").bind(f.policy).execute(&db.pool).await?;
        let (config,tag)=configuration(f,id).await?;
        let op=Uuid::now_v7();
        let body=change(op,parse(&config["output_revision"])? ,json!({"kind":"seek","position_ms":3000}));
        anyhow::ensure!(post(f,id,&tag,&body).await?.0==StatusCode::ACCEPTED);
        anyhow::ensure!(completed(f,id,op).await?["status"]=="completed");
        let sought=f.ready(id).await?;
        anyhow::ensure!(sought["ready"]["session_id"]==history && sought["ready"]["position_ms"]==3000);
        media::verify(f,id,5000).await?;
        let claim:bool=sqlx::query_scalar("SELECT claim_encodes FROM playback_vod_controls WHERE reservation_id=$1").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(claim);
        let (config,tag)=configuration(f,id).await?;
        let op=Uuid::now_v7();
        let selection=f.start_body(id,false,false)["selection"].clone();
        let body=change(op,parse(&config["output_revision"])? ,json!({"kind":"reconfigure","position_ms":4000,"selection":selection}));
        anyhow::ensure!(post(f,id,&tag,&body).await?.0==StatusCode::ACCEPTED);
        anyhow::ensure!(completed(f,id,op).await?["status"]=="completed");
        let direct=f.ready(id).await?;
        anyhow::ensure!(direct["ready"]["session_id"]==history && direct["ready"]["delivery_mode"]=="direct_play", "fresh direct result: {direct}");
        let claim:bool=sqlx::query_scalar("SELECT claim_encodes FROM playback_vod_controls WHERE reservation_id=$1").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(!claim);
        let (config,tag)=configuration(f,id).await?;
        let refused=change(Uuid::now_v7(),parse(&config["output_revision"])? ,json!({"kind":"reconfigure","position_ms":4000,"selection":f.start_body(id,true,true)["selection"]}));
        anyhow::ensure!(post(f,id,&tag,&refused).await?.0==StatusCode::SERVICE_UNAVAILABLE);
        anyhow::ensure!(f.ready(id).await?["ready"]["output_revision"]==direct["ready"]["output_revision"]);
        f.stop(id).await?;
        Ok(())
    }).await
}

pub(super) async fn conditions(db: &Database) -> anyhow::Result<()> {
    support::run(db,async |f| {
        probe(db,f).await?;
        let id=Uuid::now_v7();f.start(id,false,false).await?;f.ready(id).await?;
        let (config,tag)=configuration(f,id).await?;
        let output=parse(&config["output_revision"])?;
        for (sql,code) in [("SELECT uuidv7(interval '-10 minutes')",StatusCode::GONE),("SELECT uuidv7(interval '3 minutes')",StatusCode::BAD_REQUEST)] {
            let op:Uuid=sqlx::query_scalar(sql).fetch_one(&db.pool).await?;
            anyhow::ensure!(post(f,id,&tag,&change(op,output,json!({"kind":"seek","position_ms":2000}))).await?.0==code);
        }
        let first=Uuid::now_v7();let body=change(first,output,json!({"kind":"seek","position_ms":2000}));
        anyhow::ensure!(post(f,id,&tag,&body).await?.0==StatusCode::ACCEPTED);completed(f,id,first).await?;
        let changed=change(first,output,json!({"kind":"seek","position_ms":3000}));
        anyhow::ensure!(post(f,id,&tag,&changed).await?.0==StatusCode::CONFLICT);
        let (config,tag)=configuration(f,id).await?;
        let wrong_file=change(Uuid::now_v7(),parse(&config["output_revision"])? ,json!({"kind":"reconfigure","position_ms":0,"selection":{"media_item_id":f.item,"media_file_id":Uuid::now_v7()}}));
        anyhow::ensure!(post(f,id,&tag,&wrong_file).await?.0==StatusCode::CONFLICT);
        let outside=change(Uuid::now_v7(),parse(&config["output_revision"])? ,json!({"kind":"seek","position_ms":9000}));
        anyhow::ensure!(post(f,id,&tag,&outside).await?.0==StatusCode::BAD_REQUEST);
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM playback_vod_operations WHERE reservation_id=$1").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(count==1);
        f.stop(id).await?;
        Ok(())
    }).await
}
