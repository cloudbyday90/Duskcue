// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::commit_proxy::Proxy;
use super::{Database, harness::*, support::Fixture};
use reqwest::{Method, StatusCode};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

pub(in super::super) async fn wire(
    db: &Database,
    needle: &'static [u8],
    before: bool,
    checkpoint: bool,
) -> anyhow::Result<()> {
    let proxy = Proxy::start(&db.url, needle, before, false).await?;
    proxy.resume();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&proxy.url)
        .await?;
    let mut f = Fixture::with_owner_pool(db, Some(pool.clone())).await?;
    let result=async {
        probe(db,&f).await?;
        let id=Uuid::now_v7();f.start(id,false,false).await?;f.ready(id).await?;
        let (config,tag)=configuration(&f,id).await?;
        let output=parse(&config["output_revision"])?;
        proxy.arm();
        let op=Uuid::now_v7();
        let response=if checkpoint {
            f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/checkpoint"),Some(json!({"protocol":PROTOCOL,"output_revision":output,"sequence":1,"position_ms":1000,"playback_state":"paused"}))).await?
        } else {post(&f,id,&tag,&change(op,output,json!({"kind":"seek","position_ms":2000}))).await?};
        anyhow::ensure!(matches!(response.0,StatusCode::ACCEPTED|StatusCode::OK|StatusCode::SERVICE_UNAVAILABLE),"fault response: {response:?}");
        tokio::time::timeout(Duration::from_secs(20),async {while proxy.dropped()==0 {tokio::time::sleep(Duration::from_millis(10)).await;}}).await?;
        proxy.resume();
        if checkpoint {
            let report=json!({"protocol":PROTOCOL,"output_revision":output,"sequence":1,"position_ms":1000,"playback_state":"paused"});
            anyhow::ensure!(f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/checkpoint"),Some(report.clone())).await?.0==StatusCode::OK);
            let events:i64=sqlx::query_scalar("SELECT count(*) FROM play_events WHERE event_type='heartbeat'").fetch_one(&db.pool).await?;
            anyhow::ensure!(f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/checkpoint"),Some(report)).await?.0==StatusCode::OK);
            let after:i64=sqlx::query_scalar("SELECT count(*) FROM play_events WHERE event_type='heartbeat'").fetch_one(&db.pool).await?;
            anyhow::ensure!(events==after);
        } else {
            f.owner.stop(id)?;
            if let Some(viewer)=f.base.state.vod_commands.get(id) {viewer.retry.notify_one();}
        }
        f.stop(id).await?;
        let remaining:i64=sqlx::query_scalar("SELECT count(*) FROM playback_vod_outputs WHERE reservation_id=$1 AND NOT resources_completed").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(remaining==0);
        Ok::<_,anyhow::Error>(())
    }.await;
    proxy.resume();
    let cleanup = f.close().await;
    pool.close().await;
    let transport = proxy.finish().await;
    result?;
    cleanup?;
    transport
}
