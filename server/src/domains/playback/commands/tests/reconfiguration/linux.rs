// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{Database, harness::*, support::Fixture};
use reqwest::{Method, StatusCode};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

#[path = "../../../../../../tests/support/playback_recovery_process.rs"]
mod freezing;
#[path = "../../../../../../tests/support/playback_progressive_process.rs"]
#[allow(dead_code)]
mod process;

pub(in super::super) async fn worker(db: &Database) -> anyhow::Result<()> {
    let mut f = Fixture::paced(db).await?;
    let result=async {
        probe(db,&f).await?;
        let id=Uuid::now_v7();f.start(id,true,true).await?;let initial=f.ready(id).await?;
        let local=f.base.state.vod_commands.get(id).ok_or_else(||anyhow::anyhow!("paced viewer missing"))?;
        let output=local.current()?;
        let session=f.base.state.transcode_manager.get_session(&output.generation.unwrap()).ok_or_else(||anyhow::anyhow!("paced encoder missing"))?;
        let encoder=process::Encoder::observe(&f.source,&session.manifest_path,None)?;
        let frozen=freezing::FrozenEncoder::stop(&encoder)?;
        let checkpoint=json!({"protocol":PROTOCOL,"output_revision":output.id,"sequence":1,"position_ms":1000,"playback_state":"paused"});
        anyhow::ensure!(f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/checkpoint"),Some(checkpoint)).await?.0==StatusCode::OK);
        tokio::time::timeout(Duration::from_secs(42),async {loop {let (config,_)=configuration(&f,id).await?;if config["state"]=="unavailable" && output.cleaned.load(std::sync::atomic::Ordering::Acquire) {break Ok::<_,anyhow::Error>(());}tokio::time::sleep(Duration::from_millis(100)).await;}}).await??;
        encoder.gone()?;drop(frozen);
        anyhow::ensure!(*output.failure.lock().unwrap()==Some(crate::services::transcoding::FailureClass::Stalled));
        anyhow::ensure!(f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/renew"),Some(json!({}))).await?.0==StatusCode::OK);
        let (config,tag)=configuration(&f,id).await?;
        let op=Uuid::now_v7();
        let body=change(op,parse(&config["output_revision"])?,json!({"kind":"recover","checkpoint_sequence":1,"evidence":{"kind":"worker"}}));
        let accepted=post(&f,id,&tag,&body).await?;anyhow::ensure!(accepted.0==StatusCode::ACCEPTED,"worker recovery refused: {accepted:?}");
        let result=completed(&f,id,op).await?;anyhow::ensure!(result["status"]=="completed","worker recovery result: {result}");
        let ready=f.ready(id).await?;
        anyhow::ensure!(ready["ready"]["session_id"]==initial["ready"]["session_id"] && ready["ready"]["position_ms"]==1000 && ready["ready"]["recovery"]["attempts"]==1);
        super::media::verify(&f,id,5000).await?;
        let (config,_)=configuration(&f,id).await?;
        anyhow::ensure!(config["recovery_budget"]["restarted_stall"]==true && config["playback_state"]=="paused");
        f.stop(id).await?;
        Ok::<_,anyhow::Error>(())
    }.await;
    let cleanup = f.close().await;
    result?;
    cleanup
}
