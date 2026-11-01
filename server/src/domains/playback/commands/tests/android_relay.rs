// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database, support};
use serde_json::json;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires managed isolated Android emulator, PostgreSQL18 and actual FFmpeg media"]
async fn real_android_relay_and_formats() -> anyhow::Result<()> {
    database::run(async |db| {
        let mut f = support::Fixture::browser_paced(db).await?;
        let result = async {
            let (item, _) = super::client_media::install(db, &f, "DUSKCUE_TEST_RELAY_SOURCE").await?;
            let (boundary_item, _) = super::client_media::subtitled(db, &f).await?;
            let private = f.base.state.bootstrap.data_dir.join("android-relay.json");
            tokio::fs::write(&private, serde_json::to_vec(&json!({
                "suite":"relay", "token":f.token, "origin":f.url, "item":item, "boundary_item":boundary_item,
                "user":f.base.actor.user, "profile":f.base.actor.profile
            }))?).await?;
            let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
            let mut command = tokio::process::Command::new("node");
            command.current_dir(repo).arg("scripts/testing-memory/android-runtime.mjs")
                .env("DUSKCUE_VOD_CLIENT_FIXTURE", &private)
                .stdin(std::process::Stdio::null()).kill_on_drop(true).creation_flags(0x08000000);
            let mut child = command.spawn()?;
            let status = tokio::time::timeout(Duration::from_secs(1800), child.wait()).await;
            if status.is_err() { child.start_kill()?; child.wait().await?; }
            anyhow::ensure!(status??.success(), "actual Android relay companion qualification failed");
            tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    let active: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_reservations WHERE state!='released'").fetch_one(&db.pool).await?;
                    if active == 0 { return Ok::<_,anyhow::Error>(()); }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }).await??;
            let (commands, released, history, unsettled): (i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM playback_vod_commands WHERE protocol=2),(SELECT count(*) FROM playback_reservations WHERE state='released'),(SELECT count(*) FROM play_sessions WHERE metadata->>'vod_command_protocol'='2'),(SELECT count(*) FROM playback_start_intents WHERE settled_at IS NULL)").fetch_one(&db.pool).await?;
            anyhow::ensure!(commands == 2 && released == 2 && history == 2 && unsettled == 0, "relay companion viewer/history/settlement attribution differs");
            let changes: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_vod_operations").fetch_one(&db.pool).await?;
            anyhow::ensure!(changes == 2, "format viewer must have exactly two explicit quality operations");
            println!("VOD_ANDROID_RELAY_RESULT real_postgres=true real_native=true viewers=2 histories=2 unsettled=0 quality_changes=2 resources_released=true");
            Ok(())
        }.await;
        let cleanup = f.close().await;
        result?;
        cleanup
    }).await
}
