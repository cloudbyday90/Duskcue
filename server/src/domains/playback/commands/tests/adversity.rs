// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database::Database, support};
use reqwest::{Method, StatusCode};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

pub(super) async fn concurrency(db: &Database) -> anyhow::Result<()> {
    support::run(db,async|f| {
        sqlx::query("UPDATE users SET max_streams=1 WHERE id=$1").bind(f.base.actor.user).execute(&f.base.state.pool).await?;
        let ids=[Uuid::now_v7(),Uuid::now_v7(),Uuid::now_v7()];
        let (a,b,c)=tokio::join!(f.json(Method::POST,"/api/v1/playback-commands",Some(f.start_body(ids[0],false,false))),f.json(Method::POST,"/api/v1/playback-commands",Some(f.start_body(ids[1],false,false))),f.json(Method::POST,"/api/v1/playback-commands",Some(f.start_body(ids[2],false,false))));
        let responses=[a?,b?,c?];
        anyhow::ensure!(responses.iter().filter(|(status,_)|*status==StatusCode::ACCEPTED).count()==1);
        anyhow::ensure!(responses.iter().all(|(status,_)|matches!(*status,StatusCode::ACCEPTED|StatusCode::SERVICE_UNAVAILABLE)));
        let id=ids[responses.iter().position(|(status,_)|*status==StatusCode::ACCEPTED).unwrap()];
        f.ready(id).await?;f.stop(id).await?;
        let duplicate=Uuid::now_v7();
        let (a,b)=tokio::join!(f.json(Method::POST,"/api/v1/playback-commands",Some(f.start_body(duplicate,false,false))),f.json(Method::POST,"/api/v1/playback-commands",Some(f.start_body(duplicate,false,false))));
        for (status,_) in [a?,b?] {anyhow::ensure!(matches!(status,StatusCode::ACCEPTED|StatusCode::SERVICE_UNAVAILABLE));}
        f.ready(duplicate).await?;
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM playback_reservations WHERE household_id=$1 AND state<>'released'").bind(f.base.actor.user).fetch_one(&f.base.state.pool).await?;
        anyhow::ensure!(count==1);f.stop(duplicate).await?;
        Ok(())
    }).await
}

pub(super) async fn transcode_cap(db: &Database) -> anyhow::Result<()> {
    support::run(db, async |f| {
        sqlx::query("UPDATE streaming_policies SET max_transcode_streams=1 WHERE id=$1")
            .bind(f.policy)
            .execute(&f.base.state.pool)
            .await?;
        let id = Uuid::now_v7();
        f.start(id, true, true).await?;
        let ready = f.ready(id).await?;
        anyhow::ensure!(ready["ready"]["delivery_mode"] == "transcode");
        let generation: Uuid = sqlx::query_scalar(
            "SELECT generation_id FROM playback_vod_commands WHERE reservation_id=$1 AND encodes",
        )
        .bind(id)
        .fetch_one(&f.base.state.pool)
        .await?;
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if f.base
                    .state
                    .transcode_manager
                    .get_session(&generation)
                    .is_some_and(|s| s.is_complete)
                {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await??;
        let refused = f
            .json(
                Method::POST,
                "/api/v1/playback-commands",
                Some(f.start_body(Uuid::now_v7(), true, true)),
            )
            .await?;
        anyhow::ensure!(refused.0 == StatusCode::SERVICE_UNAVAILABLE);
        sqlx::query("UPDATE streaming_policies SET max_transcode_streams=0 WHERE id=$1")
            .bind(f.policy)
            .execute(&f.base.state.pool)
            .await?;
        let renewed = f
            .json(
                Method::POST,
                &format!("/api/v1/playback-commands/{id}/renew"),
                Some(json!({})),
            )
            .await?;
        anyhow::ensure!(renewed.0 == StatusCode::OK && renewed.1["status"] == "renewed");
        let copy = Uuid::now_v7();
        f.start(copy, true, false).await?;
        f.ready(copy).await?;
        f.stop(copy).await?;
        f.stop(id).await?;
        sqlx::query("UPDATE streaming_policies SET max_transcode_streams=1 WHERE id=$1")
            .bind(f.policy)
            .execute(&f.base.state.pool)
            .await?;
        let audio = Uuid::now_v7();
        let mut body = f.start_body(audio, true, false);
        body["selection"]["device_profile"]["max_audio_channels"] = json!(1);
        let started = f
            .json(Method::POST, "/api/v1/playback-commands", Some(body))
            .await?;
        anyhow::ensure!(started.0 == StatusCode::ACCEPTED);
        f.ready(audio).await?;
        let viewer = f
            .base
            .state
            .vod_commands
            .get(audio)
            .ok_or_else(|| anyhow::anyhow!("audio viewer missing"))?;
        let plan = viewer
            .prepared
            .plan
            .stream_plan
            .ok_or_else(|| anyhow::anyhow!("audio plan missing"))?;
        anyhow::ensure!(
            viewer.prepared.binding.encodes
                && plan.video == crate::services::transcoding::plan::StreamAction::Copy
                && plan.audio == crate::services::transcoding::plan::StreamAction::Encode
        );
        let refused = f
            .json(
                Method::POST,
                "/api/v1/playback-commands",
                Some(f.start_body(Uuid::now_v7(), true, true)),
            )
            .await?;
        anyhow::ensure!(refused.0 == StatusCode::SERVICE_UNAVAILABLE);
        f.stop(audio).await?;
        Ok(())
    })
    .await
}

pub(super) async fn observer_loss(db: &Database) -> anyhow::Result<()> {
    support::run(db, async |f| {
        let unknown = Uuid::now_v7();
        let before = f
            .json(
                Method::GET,
                &format!("/api/v1/playback-commands/{unknown}"),
                None,
            )
            .await?;
        anyhow::ensure!(before.0 == StatusCode::OK && before.1["status"] == "unknown");
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_reservations")
            .fetch_one(&f.base.state.pool)
            .await?;
        anyhow::ensure!(count == 0);
        let response = f
            .response(
                Method::POST,
                "/api/v1/playback-commands",
                Some(f.start_body(unknown, true, false)),
                false,
            )
            .await?;
        anyhow::ensure!(
            response.status() == StatusCode::ACCEPTED
                && response.headers().get("location").is_some()
        );
        drop(response);
        f.ready(unknown).await?;
        let before: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT deadline FROM playback_reservations WHERE id=$1")
                .bind(unknown)
                .fetch_one(&f.base.state.pool)
                .await?;
        for _ in 0..3 {
            let (status, _) = f
                .json(
                    Method::GET,
                    &format!("/api/v1/playback-commands/{unknown}"),
                    None,
                )
                .await?;
            anyhow::ensure!(status == StatusCode::OK);
        }
        let after: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT deadline FROM playback_reservations WHERE id=$1")
                .bind(unknown)
                .fetch_one(&f.base.state.pool)
                .await?;
        anyhow::ensure!(before == after);
        f.stop(unknown).await?;
        Ok(())
    })
    .await
}

pub(super) async fn cleanup_retry(db: &Database) -> anyhow::Result<()> {
    support::run(db,async|f| {
        let id=Uuid::now_v7();f.start(id,false,false).await?;f.ready(id).await?;
        let mut block=f.base.state.pool.begin().await?;
        sqlx::query("LOCK TABLE play_events IN ACCESS EXCLUSIVE MODE").execute(&mut *block).await?;
        let (status,_)=f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/stop"),Some(json!({}))).await?;
        anyhow::ensure!(status==StatusCode::ACCEPTED);
        tokio::time::sleep(Duration::from_millis(900)).await;
        let (state,completed):(String,bool)=sqlx::query_as("SELECT r.state,c.resources_completed FROM playback_reservations r JOIN playback_vod_commands c ON c.reservation_id=r.id WHERE r.id=$1").bind(id).fetch_one(&f.base.state.pool).await?;
        anyhow::ensure!(state=="retiring"&&!completed);
        tokio::time::timeout(Duration::from_secs(4),async {
            loop {
                let uncertain=f.json(Method::GET,&format!("/api/v1/playback-commands/{id}/retirement"),None).await?;
                anyhow::ensure!(uncertain.0==StatusCode::OK);
                if uncertain.1["status"]=="cleanup_uncertain" {return Ok::<_,anyhow::Error>(());}
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }).await??;
        block.rollback().await?;
        f.stop(id).await?;
        Ok(())
    }).await
}

pub(super) async fn bodies(db: &Database) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    support::run(db,async|f| {
        let bad=f.json(Method::GET,"/api/v1/playback-commands/not-a-command",None).await?;
        anyhow::ensure!(bad.0==StatusCode::BAD_REQUEST&&!bad.1.to_string().contains("not-a-command"));
        let simple=f.client.post(format!("{}/api/v1/playback-commands/{}/stop",f.url,Uuid::now_v7())).header("cookie",format!("session={}",f.token)).send().await?;
        anyhow::ensure!(simple.status()==StatusCode::BAD_REQUEST);
        let body=json!({"request_id":Uuid::now_v7(),"selection":{"media_item_id":f.item,"oversize":"x".repeat(32768)}});
        let response=f.json(Method::POST,"/api/v1/playback-commands",Some(body)).await?;
        anyhow::ensure!(response.0==StatusCode::BAD_REQUEST&&response.1.to_string().len()<1024);
        let address=f.url.strip_prefix("http://").unwrap();
        let mut stream=tokio::net::TcpStream::connect(address).await?;
        stream.write_all(format!("POST /api/v1/playback-commands HTTP/1.1\r\nHost: {address}\r\nCookie: session={}\r\nContent-Type: application/json\r\nx-session-context: {}\r\nContent-Length: 1\r\nConnection: close\r\n\r\n",f.token,f.context).as_bytes()).await?;
        let mut bytes=Vec::new();
        tokio::time::timeout(Duration::from_secs(7),stream.take(4096).read_to_end(&mut bytes)).await??;
        anyhow::ensure!(String::from_utf8_lossy(&bytes).starts_with("HTTP/1.1 400"));
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM playback_reservations").fetch_one(&f.base.state.pool).await?;
        anyhow::ensure!(count==0);
        Ok(())
    }).await
}
