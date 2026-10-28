// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::{database::Database, support::Fixture};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use std::time::Duration;
use uuid::Uuid;

pub(super) const PROTOCOL: &str = "duskcue-vod-command/2";

pub(super) fn parse(value: &Value) -> anyhow::Result<Uuid> {
    Ok(value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("UUID field required"))?
        .parse()?)
}

pub(super) async fn probe(db: &Database, f: &Fixture) -> anyhow::Result<()> {
    probe_source(db, f, "DUSKCUE_TEST_CONFIGURATION_SOURCE").await
}
pub(super) async fn probe_source(db: &Database, f: &Fixture, name: &str) -> anyhow::Result<()> {
    probe_path(db, f, &std::path::PathBuf::from(std::env::var(name)?)).await
}
pub(super) async fn probe_path(
    db: &Database,
    f: &Fixture,
    source: &std::path::Path,
) -> anyhow::Result<()> {
    let source = source.canonicalize()?;
    let metadata = tokio::fs::symlink_metadata(&source).await?;
    anyhow::ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= 8 * 1024 * 1024
    );
    if tokio::fs::read(&source).await? != tokio::fs::read(&f.source).await? {
        tokio::fs::copy(source, &f.source).await?;
    }
    let metadata = tokio::fs::metadata(&f.source).await?;
    let modified: chrono::DateTime<chrono::Utc> = metadata.modified()?.into();
    sqlx::query("UPDATE media_files SET file_size=$2,file_modified_at=$3,updated_at=clock_timestamp() WHERE id=$1").bind(f.file).bind(i64::try_from(metadata.len())?).bind(modified).execute(&db.pool).await?;
    let probe = crate::workers::library_scanner::probe_qualification_source(&f.source).await?;
    anyhow::ensure!(
        probe.additional_streams["format"]["duration_seconds"]
            .as_f64()
            .is_some_and(|n| (8.0..9.0).contains(&n))
    );
    sqlx::query("UPDATE media_files SET container_format=$2,video_codec=$3,video_resolution=$4,video_bitrate=$5,video_dynamic_range=$6,audio_codec=$7,audio_channels=$8,audio_bitrate=$9,runtime_seconds=$10,additional_streams=$11 WHERE id=$1")
        .bind(f.file).bind(probe.container_format).bind(probe.video_codec).bind(probe.video_resolution).bind(probe.video_bitrate).bind(probe.video_dynamic_range).bind(probe.audio_codec).bind(probe.audio_channels).bind(probe.audio_bitrate).bind(probe.runtime_seconds).bind(probe.additional_streams).execute(&db.pool).await?;
    Ok(())
}

pub(super) fn change(id: Uuid, output: Uuid, action: Value) -> Value {
    json!({"protocol":PROTOCOL,"operation_id":id,"expected_output":output,"action":action})
}

pub(super) async fn configuration(f: &Fixture, id: Uuid) -> anyhow::Result<(Value, String)> {
    let response = f
        .response(
            Method::GET,
            &format!("/api/v1/playback-commands/{id}/configuration"),
            None,
            false,
        )
        .await?;
    anyhow::ensure!(response.status() == StatusCode::OK);
    anyhow::ensure!(
        response.headers()["cache-control"]
            .to_str()?
            .contains("no-store")
    );
    let etag = response.headers()["etag"].to_str()?.to_owned();
    anyhow::ensure!(etag.starts_with("\"vod-config-") && !etag.starts_with("W/"));
    let bytes = response.bytes().await?;
    anyhow::ensure!(bytes.len() <= 32768);
    Ok((serde_json::from_slice(&bytes)?, etag))
}

pub(super) async fn post(
    f: &Fixture,
    id: Uuid,
    etag: &str,
    body: &Value,
) -> anyhow::Result<(StatusCode, Value)> {
    let response = f
        .client
        .post(format!(
            "{}/api/v1/playback-commands/{id}/configuration",
            f.url
        ))
        .header("cookie", format!("session={}", f.token))
        .header("x-session-context", &f.context)
        .header("if-match", etag)
        .json(body)
        .send()
        .await?;
    let code = response.status();
    anyhow::ensure!(
        response.headers()["cache-control"]
            .to_str()?
            .contains("no-store")
    );
    if code == StatusCode::ACCEPTED {
        anyhow::ensure!(response.headers().contains_key("location"));
    }
    let bytes = response.bytes().await?;
    anyhow::ensure!(bytes.len() <= 32768);
    Ok((code, serde_json::from_slice(&bytes)?))
}

pub(super) async fn completed(f: &Fixture, id: Uuid, operation: Uuid) -> anyhow::Result<Value> {
    tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            let (code, value) = f
                .json(
                    Method::GET,
                    &format!("/api/v1/playback-commands/{id}/operations/{operation}"),
                    None,
                )
                .await?;
            anyhow::ensure!(code == StatusCode::OK, "operation status refused: {value}");
            if matches!(
                value["status"].as_str(),
                Some("completed" | "failed" | "cancelled")
            ) {
                return Ok::<_, anyhow::Error>(value);
            }
            anyhow::ensure!(
                matches!(
                    value["status"].as_str(),
                    Some("accepted" | "retiring" | "preparing")
                ),
                "unexpected operation state: {value}"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?
}
