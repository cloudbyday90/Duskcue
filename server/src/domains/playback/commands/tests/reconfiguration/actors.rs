// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{Database, harness::*, support};
use crate::{
    domains::household_security::{context, types::Scope},
    extractors::CredentialTransport,
};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use std::time::Duration;
use uuid::Uuid;

pub(in super::super) async fn bearer(db: &Database) -> anyhow::Result<()> {
    support::run(db,async |f| {
        probe(db,f).await?;
        let context={let mut conn=db.pool.acquire().await?;context::read(&mut conn,Scope {transport:CredentialTransport::Bearer,..f.base.actor}).await?.context_id};
        let request=|method:Method,path:&str|f.client.request(method,format!("{}{path}",f.url)).bearer_auth(&f.token).header("x-session-context",&context);
        let id=Uuid::now_v7();
        anyhow::ensure!(request(Method::POST,"/api/v1/playback-commands").json(&f.start_body(id,false,false)).send().await?.status()==StatusCode::ACCEPTED);
        let ready=tokio::time::timeout(Duration::from_secs(20),async {loop {let response=request(Method::GET,&format!("/api/v1/playback-commands/{id}")).send().await?;anyhow::ensure!(response.status()==StatusCode::OK);let value:Value=response.json().await?;if value["status"]=="ready" {break Ok::<_,anyhow::Error>(value);}tokio::time::sleep(Duration::from_millis(25)).await;}}).await??;
        let response=request(Method::GET,&format!("/api/v1/playback-commands/{id}/configuration")).send().await?;
        anyhow::ensure!(response.status()==StatusCode::OK);let tag=response.headers()["etag"].to_str()?.to_owned();let config:Value=response.json().await?;
        let op=Uuid::now_v7();let body=change(op,parse(&config["output_revision"])?,json!({"kind":"seek","position_ms":2000}));
        anyhow::ensure!(request(Method::POST,&format!("/api/v1/playback-commands/{id}/configuration")).header("if-match",&tag).json(&body).send().await?.status()==StatusCode::ACCEPTED);
        tokio::time::timeout(Duration::from_secs(20),async {loop {let response=request(Method::GET,&format!("/api/v1/playback-commands/{id}/operations/{op}")).send().await?;anyhow::ensure!(response.status()==StatusCode::OK);let value:Value=response.json().await?;if value["status"]=="completed" {break Ok::<_,anyhow::Error>(());}tokio::time::sleep(Duration::from_millis(25)).await;}}).await??;
        let cookie=f.response(Method::GET,&format!("/api/v1/playback-commands/{id}/configuration"),None,false).await?;anyhow::ensure!(cookie.status()==StatusCode::CONFLICT);
        let response=request(Method::GET,&format!("/api/v1/playback-commands/{id}")).send().await?;let value:Value=response.json().await?;
        let output=parse(&value["ready"]["output_revision"])?;anyhow::ensure!(value["ready"]["session_id"]==ready["ready"]["session_id"]);
        let report=json!({"protocol":PROTOCOL,"output_revision":output,"sequence":1,"position_ms":2500,"playback_state":"paused"});
        anyhow::ensure!(request(Method::POST,&format!("/api/v1/playback-commands/{id}/checkpoint")).json(&report).send().await?.status()==StatusCode::OK);
        sqlx::query("DELETE FROM user_profiles WHERE id=$1").bind(f.base.actor.profile).execute(&db.pool).await?;
        let response=request(Method::POST,&format!("/api/v1/playback-commands/{id}/stop")).json(&json!({})).send().await?;anyhow::ensure!(response.status()==StatusCode::ACCEPTED);
        tokio::time::timeout(Duration::from_secs(10),async {loop {let released:bool=sqlx::query_scalar("SELECT state='released' FROM playback_reservations WHERE id=$1").bind(id).fetch_one(&db.pool).await?;if released {break Ok::<_,anyhow::Error>(());}tokio::time::sleep(Duration::from_millis(25)).await;}}).await??;
        let response=request(Method::GET,&format!("/api/v1/playback-commands/{id}/operations/{op}/retirement")).send().await?;
        anyhow::ensure!(response.status()==StatusCode::OK);let receipt:Value=response.json().await?;
        anyhow::ensure!(receipt["cleanup_complete"]==true&&receipt["result"].is_null()&&receipt["output_revision"].is_null());
        Ok(())
    }).await
}
