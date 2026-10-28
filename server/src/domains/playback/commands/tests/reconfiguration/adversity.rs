// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{Database, harness::*, support};
use reqwest::{Method, StatusCode};
use serde_json::json;
use uuid::Uuid;

pub(in super::super) async fn contention(db: &Database) -> anyhow::Result<()> {
    support::run(db, async |f| {
        probe(db, f).await?;
        let id = Uuid::now_v7();
        f.start(id, false, false).await?;
        f.ready(id).await?;
        let (config, tag) = configuration(f, id).await?;
        let a = change(
            Uuid::now_v7(),
            parse(&config["output_revision"])?,
            json!({"kind":"seek","position_ms":1000}),
        );
        let b = change(
            Uuid::now_v7(),
            parse(&config["output_revision"])?,
            json!({"kind":"seek","position_ms":2000}),
        );
        let (a, b) = tokio::join!(post(f, id, &tag, &a), post(f, id, &tag, &b));
        let outcomes = [a?, b?];
        anyhow::ensure!(
            outcomes
                .iter()
                .filter(|(s, _)| *s == StatusCode::ACCEPTED)
                .count()
                == 1,
            "competing outcomes: {outcomes:?}"
        );
        anyhow::ensure!(outcomes.iter().all(|(s, _)| matches!(
            *s,
            StatusCode::ACCEPTED | StatusCode::CONFLICT | StatusCode::PRECONDITION_FAILED
        )));
        let (_, accepted) = outcomes
            .iter()
            .find(|(s, _)| *s == StatusCode::ACCEPTED)
            .unwrap();
        completed(f, id, parse(&accepted["operation_id"])?).await?;
        let (config, tag) = configuration(f, id).await?;
        let op = Uuid::now_v7();
        let body = change(
            op,
            parse(&config["output_revision"])?,
            json!({"kind":"seek","position_ms":3000}),
        );
        let response = f
            .client
            .post(format!(
                "{}/api/v1/playback-commands/{id}/configuration",
                f.url
            ))
            .header("cookie", format!("session={}", f.token))
            .header("x-session-context", &f.context)
            .header("if-match", &tag)
            .json(&body)
            .send()
            .await?;
        anyhow::ensure!(response.status() == StatusCode::ACCEPTED);
        drop(response);
        completed(f, id, op).await?;
        anyhow::ensure!(post(f, id, &tag, &body).await?.0 == StatusCode::ACCEPTED);
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM playback_vod_operations WHERE reservation_id=$1",
        )
        .bind(id)
        .fetch_one(&db.pool)
        .await?;
        anyhow::ensure!(count == 2);
        f.stop(id).await?;
        Ok(())
    })
    .await
}

pub(in super::super) async fn correction(db: &Database) -> anyhow::Result<()> {
    support::run(db, async |f| {
        probe(db, f).await?;
        let id = Uuid::now_v7();
        f.start(id, false, false).await?;
        let initial = f.ready(id).await?;
        let (config, tag) = configuration(f, id).await?;
        let capacity = f.base.state.transcode_manager.reserve_fixture_capacity()?;
        let op = Uuid::now_v7();
        let body = change(op, parse(&config["output_revision"])?, json!({"kind":"reconfigure","position_ms":2000,"selection":f.start_body(id,true,true)["selection"]}));
        let accepted = post(f, id, &tag, &body).await?;
        anyhow::ensure!(accepted.0 == StatusCode::ACCEPTED, "replacement refused: {accepted:?}");
        let failed = completed(f, id, op).await?;
        anyhow::ensure!(failed["status"] == "failed" && failed["cleanup_complete"] == true, "failure not retained: {failed}");
        drop(capacity);
        let (config, tag) = configuration(f, id).await?;
        anyhow::ensure!(config["state"] == "unavailable");
        let claim:bool = sqlx::query_scalar("SELECT claim_encodes FROM playback_vod_controls WHERE reservation_id=$1").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(!claim);
        anyhow::ensure!(!f.response(Method::GET, initial["stream_url"].as_str().unwrap(), None, false).await?.status().is_success());
        anyhow::ensure!(f.json(Method::POST, &format!("/api/v1/playback-commands/{id}/renew"), Some(json!({}))).await?.0 == StatusCode::OK);
        let next = Uuid::now_v7();
        let body = change(next, parse(&config["output_revision"])?, json!({"kind":"seek","position_ms":1000}));
        anyhow::ensure!(post(f, id, &tag, &body).await?.0 == StatusCode::ACCEPTED);
        anyhow::ensure!(completed(f, id, next).await?["status"] == "completed");
        anyhow::ensure!(f.ready(id).await?["ready"]["session_id"] == initial["ready"]["session_id"]);
        f.stop(id).await?;
        Ok(())
    }).await
}

pub(in super::super) async fn recovery(db: &Database) -> anyhow::Result<()> {
    support::run(db, async |f| {
        probe(db, f).await?;
        let id = Uuid::now_v7();
        let mut body = f.start_body(id, false, false);
        body["selection"]["direct_recovery_protocol"] = json!(1);
        anyhow::ensure!(f.json(Method::POST,"/api/v1/playback-commands",Some(body)).await?.0==StatusCode::ACCEPTED);
        let initial=f.ready(id).await?;
        let (config, _)=configuration(f,id).await?;
        let output=parse(&config["output_revision"])?;
        let checkpoint=json!({"protocol":PROTOCOL,"output_revision":output,"sequence":1,"position_ms":2000,"playback_state":"paused"});
        anyhow::ensure!(f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/checkpoint"),Some(checkpoint)).await?.0==StatusCode::OK);
        let (config, tag)=configuration(f,id).await?;
        anyhow::ensure!(config["recovery_offer"]["kind"]=="direct", "direct offer absent: {config}");
        let healthy=change(Uuid::now_v7(),output,json!({"kind":"recover","checkpoint_sequence":1,"evidence":{"kind":"worker"}}));
        anyhow::ensure!(post(f,id,&tag,&healthy).await?.0==StatusCode::CONFLICT);
        let op=Uuid::now_v7();
        let action=json!({"kind":"recover","checkpoint_sequence":1,"evidence":{"kind":"native_decode","offer":config["recovery_offer"]["fingerprint"],"supported":true}});
        let body=change(op,output,action);
        anyhow::ensure!(post(f,id,&tag,&body).await?.0==StatusCode::ACCEPTED);
        anyhow::ensure!(completed(f,id,op).await?["status"]=="completed");
        let next=f.ready(id).await?;
        super::media::verify(f,id,5000).await?;
        anyhow::ensure!(next["ready"]["session_id"]==initial["ready"]["session_id"] && next["ready"]["position_ms"]==2000 && next["ready"]["recovery"]["attempts"]==1);
        let (config, tag)=configuration(f,id).await?;
        anyhow::ensure!(config["playback_state"]=="paused" && config["recovery_budget"]["attempts"]==1);
        let op=Uuid::now_v7();
        anyhow::ensure!(post(f,id,&tag,&change(op,parse(&config["output_revision"])?,json!({"kind":"seek","position_ms":1000}))).await?.0==StatusCode::ACCEPTED);
        completed(f,id,op).await?;
        anyhow::ensure!(configuration(f,id).await?.0["recovery_budget"]["attempts"]==1);
        for expected in [2,3] {
            let (config,tag)=configuration(f,id).await?;
            let mut selection=f.start_body(id,false,false)["selection"].clone();selection["direct_recovery_protocol"]=json!(1);
            let op=Uuid::now_v7();let body=change(op,parse(&config["output_revision"])?,json!({"kind":"reconfigure","position_ms":1000,"selection":selection}));
            anyhow::ensure!(post(f,id,&tag,&body).await?.0==StatusCode::ACCEPTED);completed(f,id,op).await?;
            let (config,_)=configuration(f,id).await?;let output=parse(&config["output_revision"])?;
            let report=json!({"protocol":PROTOCOL,"output_revision":output,"sequence":1,"position_ms":1000,"playback_state":"paused"});
            anyhow::ensure!(f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/checkpoint"),Some(report)).await?.0==StatusCode::OK);
            let (config,tag)=configuration(f,id).await?;
            let op=Uuid::now_v7();let body=change(op,output,json!({"kind":"recover","checkpoint_sequence":1,"evidence":{"kind":"native_decode","offer":config["recovery_offer"]["fingerprint"],"supported":true}}));
            let accepted=post(f,id,&tag,&body).await?;
            if expected==3 {anyhow::ensure!(accepted.0==StatusCode::CONFLICT);}
            else {
                anyhow::ensure!(accepted.0==StatusCode::ACCEPTED);completed(f,id,op).await?;
                anyhow::ensure!(post(f,id,&tag,&body).await?.0==StatusCode::ACCEPTED);
                let (config,_)=configuration(f,id).await?;anyhow::ensure!(config["recovery_budget"]["attempts"]==2&&config["recovery_budget"]["exhausted"]==true);
            }
        }
        f.stop(id).await?;
        Ok(())
    }).await
}

pub(in super::super) async fn boundaries(db: &Database) -> anyhow::Result<()> {
    support::run(db,async |f| {
        probe(db,f).await?;
        let id=Uuid::now_v7();f.start(id,false,false).await?;let initial=f.ready(id).await?;
        let (config,tag)=configuration(f,id).await?;
        let body=change(Uuid::now_v7(),parse(&config["output_revision"])?,json!({"kind":"seek","position_ms":1000}));
        for (condition,code) in [(None,428),(Some("*"),400),(Some("W/\"vod-config-invalid\""),400)] {
            let mut request=f.client.post(format!("{}/api/v1/playback-commands/{id}/configuration",f.url)).header("cookie",format!("session={}",f.token)).header("x-session-context",&f.context).json(&body);
            if let Some(condition)=condition {request=request.header("if-match",condition);}
            anyhow::ensure!(request.send().await?.status().as_u16()==code);
        }
        let wrong=f.client.post(format!("{}/api/v1/playback-commands/{id}/configuration",f.url)).bearer_auth(&f.token).header("x-session-context",&f.context).header("if-match",&tag).json(&body).send().await?;
        anyhow::ensure!(wrong.status()==StatusCode::CONFLICT);
        let op=parse(&body["operation_id"])?;
        anyhow::ensure!(post(f,id,&tag,&body).await?.0==StatusCode::ACCEPTED);
        f.stop(id).await?;
        let (remaining,rows):(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM playback_vod_outputs WHERE reservation_id=$1 AND NOT resources_completed),(SELECT count(*) FROM playback_vod_operations WHERE reservation_id=$1 AND terminal_at IS NULL)").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(remaining==0 && rows==0);
        let cleanup=f.json(Method::GET,&format!("/api/v1/playback-commands/{id}/operations/{op}/retirement"),None).await?;
        anyhow::ensure!(cleanup.0==StatusCode::OK && cleanup.1["output_revision"].is_null() && cleanup.1["result"].is_null());
        anyhow::ensure!(!f.response(Method::GET,initial["stream_url"].as_str().unwrap(),None,false).await?.status().is_success());
        Ok(())
    }).await
}

pub(in super::super) async fn tracks(db: &Database) -> anyhow::Result<()> {
    support::run(db, async |f| {
        probe(db, f).await?;
        let id = Uuid::now_v7();
        f.start(id, true, true).await?;
        let first = f.ready(id).await?;
        let (config, tag) = configuration(f, id).await?;
        let mut selection = f.start_body(id, true, true)["selection"].clone();
        selection["audio_stream_index"] = json!(0);
        selection["subtitle_stream_index"] = json!(4);
        let op = Uuid::now_v7();
        let body = change(
            op,
            parse(&config["output_revision"])?,
            json!({"kind":"reconfigure","position_ms":3000,"selection":selection}),
        );
        let accepted = post(f, id, &tag, &body).await?;
        anyhow::ensure!(
            accepted.0 == StatusCode::ACCEPTED,
            "track selection refused: {accepted:?}"
        );
        let result = completed(f, id, op).await?;
        anyhow::ensure!(
            result["status"] == "completed",
            "track replacement failed: {result}"
        );
        let ready = f.ready(id).await?;
        anyhow::ensure!(
            ready["ready"]["audio_stream_index"] == 0
                && ready["ready"]["subtitle_stream_index"] == 4
                && ready["ready"]["session_id"] == first["ready"]["session_id"]
        );
        let local = f.base.state.vod_commands.get(id).unwrap();
        let output = local.current()?;
        let session = f
            .base
            .state
            .transcode_manager
            .get_session(&output.generation.unwrap())
            .unwrap();
        anyhow::ensure!(
            session.source_audio_stream_index == Some(0)
                && session.subtitle_stream_index == Some(4)
                && session.subtitle_stream_ordinal == Some(1)
                && session.subtitle_burn_in
        );
        super::media::verify_audio(f, id, 5000, 220.0).await?;
        f.stop(id).await?;
        Ok(())
    })
    .await
}

pub(in super::super) async fn bodies(db: &Database) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    support::run(db,async |f| {
        probe(db,f).await?;let id=Uuid::now_v7();f.start(id,false,false).await?;f.ready(id).await?;
        let (config,tag)=configuration(f,id).await?;
        let request=|suffix:&str|f.client.post(format!("{}/api/v1/playback-commands/{id}/{suffix}",f.url)).header("cookie",format!("session={}",f.token)).header("x-session-context",&f.context).header("if-match",&tag);
        anyhow::ensure!(request("configuration").header("content-type","text/plain").body("{}").send().await?.status()==StatusCode::UNSUPPORTED_MEDIA_TYPE);
        anyhow::ensure!(request("configuration").header("content-type","application/json").body("x".repeat(32769)).send().await?.status()==StatusCode::PAYLOAD_TOO_LARGE);
        anyhow::ensure!(request("checkpoint").header("content-type","application/json").body("x".repeat(1025)).send().await?.status()==StatusCode::PAYLOAD_TOO_LARGE);
        let mut body=change(Uuid::now_v7(),parse(&config["output_revision"])?,json!({"kind":"seek","position_ms":1000}));body["protocol"]=json!("duskcue-vod-command/1");
        anyhow::ensure!(post(f,id,&tag,&body).await?.0==StatusCode::BAD_REQUEST);
        let address=f.url.strip_prefix("http://").unwrap();let mut stream=tokio::net::TcpStream::connect(address).await?;
        stream.write_all(format!("POST /api/v1/playback-commands/{id}/configuration HTTP/1.1\r\nHost: {address}\r\nCookie: session={}\r\nx-session-context: {}\r\nIf-Match: {tag}\r\nContent-Type: application/json\r\nContent-Length: 1\r\nConnection: close\r\n\r\n",f.token,f.context).as_bytes()).await?;
        let mut bytes=Vec::new();tokio::time::timeout(std::time::Duration::from_secs(7),stream.take(4096).read_to_end(&mut bytes)).await??;
        anyhow::ensure!(String::from_utf8_lossy(&bytes).starts_with("HTTP/1.1 400"));
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM playback_vod_operations").fetch_one(&db.pool).await?;anyhow::ensure!(count==0);
        f.stop(id).await?;Ok(())
    }).await
}
