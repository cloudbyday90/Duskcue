// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{Database, harness::*, support};
use reqwest::{Method, StatusCode};
use serde_json::json;
use uuid::Uuid;

pub(in super::super) async fn timing_and_fallback(db: &Database) -> anyhow::Result<()> {
    run(
        db,
        &std::path::PathBuf::from(std::env::var("DUSKCUE_TEST_CONFIGURATION_COPY")?),
    )
    .await
}
async fn run(db: &Database, source: &std::path::Path) -> anyhow::Result<()> {
    support::run(db,async |f| {
        probe_path(db,f,source).await?;
        direct(f).await?;
        let id=Uuid::now_v7();
        let mut body=f.start_body(id,true,false);
        body["selection"]["device_profile"]["video_codecs"]=json!(["h264","hevc","av1"]);
        body["selection"]["device_profile"]["max_video_bit_depth"]=json!(10);
        body["selection"]["direct_recovery_protocol"]=json!(1);
        let mut prepare=body["selection"].clone();prepare["capability_protocol"]=json!(1);
        let response=f.response(Method::POST,"/api/v1/playback/prepare",Some(prepare),false).await?;
        anyhow::ensure!(response.status()==StatusCode::OK);
        let prepared:serde_json::Value=response.json().await?;
        anyhow::ensure!(prepared["candidates"].as_array().is_some_and(|c|c.iter().any(|c|c["id"]=="hls_video_copy_audio_copy")),"copy candidate absent: {prepared}");
        body["selection"]["source_capabilities"]=json!({"version":1,"source_revision":prepared["source_revision"],"observations":[{"candidate_id":"hls_video_copy_audio_copy","status":"reported","supported":true,"smooth":true,"delivery_supported":true}]});
        anyhow::ensure!(f.json(Method::POST,"/api/v1/playback-commands",Some(body)).await?.0==StatusCode::ACCEPTED);
        let initial=f.ready(id).await?;
        anyhow::ensure!(initial["ready"]["delivery_mode"]=="direct_stream");
        let (config,tag)=configuration(f,id).await?;
        let op=Uuid::now_v7();
        let body=change(op,parse(&config["output_revision"])?,json!({"kind":"seek","position_ms":5000}));
        anyhow::ensure!(post(f,id,&tag,&body).await?.0==StatusCode::ACCEPTED);
        anyhow::ensure!(completed(f,id,op).await?["status"]=="completed");
        let sought=f.ready(id).await?;
        anyhow::ensure!(sought["ready"]["position_ms"]==5000 && sought["ready"]["stream_offset_ms"].as_i64().is_some_and(|v|v<5000));
        super::media::verify(f,id,5000).await?;
        anyhow::ensure!(!f.response(Method::GET,initial["stream_url"].as_str().unwrap(),None,false).await?.status().is_success());
        let (config,_)=configuration(f,id).await?;
        let output=parse(&config["output_revision"])?;
        let checkpoint=json!({"protocol":PROTOCOL,"output_revision":output,"sequence":1,"position_ms":5000,"playback_state":"paused"});
        anyhow::ensure!(f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/checkpoint"),Some(checkpoint)).await?.0==StatusCode::OK);
        let (config,tag)=configuration(f,id).await?;
        anyhow::ensure!(config["recovery_offer"]["kind"]=="copy");
        let op=Uuid::now_v7();
        let mut body=change(op,output,json!({"kind":"recover","checkpoint_sequence":1,"evidence":{"kind":"hls_media","offer":config["recovery_offer"]["fingerprint"],"supported":true,"smooth":false}}));
        anyhow::ensure!(post(f,id,&tag,&body).await?.0==StatusCode::CONFLICT);
        body["action"]["evidence"]["smooth"]=json!(true);
        anyhow::ensure!(post(f,id,&tag,&body).await?.0==StatusCode::ACCEPTED);
        anyhow::ensure!(completed(f,id,op).await?["status"]=="completed");
        let next=f.ready(id).await?;
        anyhow::ensure!(next["ready"]["session_id"]==initial["ready"]["session_id"] && next["ready"]["recovery"]["attempts"]==1 && next["ready"]["position_ms"]==5000);
        super::media::verify(f,id,5000).await?;
        f.stop(id).await?;
        Ok(())
    }).await
}

async fn direct(f: &super::support::Fixture) -> anyhow::Result<()> {
    let id = Uuid::now_v7();
    let mut body = f.start_body(id, false, false);
    body["selection"]["device_profile"]["video_codecs"] = json!(["h264", "hevc", "av1"]);
    body["selection"]["device_profile"]["max_video_bit_depth"] = json!(10);
    body["selection"]["direct_recovery_protocol"] = json!(1);
    anyhow::ensure!(
        f.json(Method::POST, "/api/v1/playback-commands", Some(body))
            .await?
            .0
            == StatusCode::ACCEPTED
    );
    let ready = f.ready(id).await?;
    anyhow::ensure!(ready["ready"]["delivery_mode"] == "direct_play");
    let (config, _) = configuration(f, id).await?;
    let output = parse(&config["output_revision"])?;
    let checkpoint = json!({"protocol":PROTOCOL,"output_revision":output,"sequence":1,"position_ms":2000,"playback_state":"paused"});
    anyhow::ensure!(
        f.json(
            Method::POST,
            &format!("/api/v1/playback-commands/{id}/checkpoint"),
            Some(checkpoint)
        )
        .await?
        .0 == StatusCode::OK
    );
    let (config, tag) = configuration(f, id).await?;
    anyhow::ensure!(config["recovery_offer"]["kind"] == "direct");
    let op = Uuid::now_v7();
    let body = change(
        op,
        output,
        json!({"kind":"recover","checkpoint_sequence":1,"evidence":{"kind":"native_decode","offer":config["recovery_offer"]["fingerprint"],"supported":true}}),
    );
    let accepted = post(f, id, &tag, &body).await?;
    anyhow::ensure!(
        accepted.0 == StatusCode::ACCEPTED,
        "native source fallback refused: {accepted:?}"
    );
    anyhow::ensure!(completed(f, id, op).await?["status"] == "completed");
    super::media::verify(f, id, 5000).await?;
    f.stop(id).await?;
    Ok(())
}

pub(in super::super) async fn formats() -> anyhow::Result<()> {
    #[derive(serde::Deserialize)]
    struct Variant {
        id: String,
        path: std::path::PathBuf,
        sha256: String,
    }
    let path = std::path::PathBuf::from(std::env::var("DUSKCUE_TEST_CONFIGURATION_CORPUS")?)
        .canonicalize()?;
    let metadata = tokio::fs::symlink_metadata(&path).await?;
    anyhow::ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 8192
    );
    let variants: Vec<Variant> = serde_json::from_slice(&tokio::fs::read(&path).await?)?;
    anyhow::ensure!(variants.len() == 4);
    let mut seen = std::collections::BTreeSet::new();
    for variant in variants {
        anyhow::ensure!(
            ["hevc-sdr-8", "hevc-sdr-10", "av1-sdr-8", "av1-sdr-10"].contains(&variant.id.as_str())
                && seen.insert(variant.id)
        );
        let source = variant.path.canonicalize()?;
        anyhow::ensure!(
            source.parent() == path.parent()
                && tokio::fs::metadata(&source).await?.len() <= 8 * 1024 * 1024
        );
        let bytes = tokio::fs::read(&source).await?;
        let hash: String = ring::digest::digest(&ring::digest::SHA256, &bytes)
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        anyhow::ensure!(hash == variant.sha256);
        super::super::database::run(async |db| run(db, &source).await).await?;
        println!("VOD_CONFIGURATION_CASE qualified_format passed");
    }
    Ok(())
}
