// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database, support};
use serde_json::json;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires managed isolated Android emulator, PostgreSQL18 and actual FFmpeg media"]
async fn real_android_player_protocol_two() -> anyhow::Result<()> {
    database::run(async |db| {
        let mut fixture = support::Fixture::browser(db).await?;
        let result = async {
            let (item, _) = super::client_media::clean(db, &fixture).await?;
            let private = fixture.base.state.bootstrap.data_dir.join("android.json");
            tokio::fs::write(&private, serde_json::to_vec(&json!({
                "token":fixture.token,"origin":fixture.url,"item":item,
                "user":fixture.base.actor.user,"profile":fixture.base.actor.profile
            }))?).await?;
            let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
            let mut command = tokio::process::Command::new("node");
            command.current_dir(repo).arg("scripts/testing-memory/android-runtime.mjs")
                .env("DUSKCUE_VOD_CLIENT_FIXTURE", &private)
                .stdin(std::process::Stdio::null()).kill_on_drop(true);
            #[cfg(target_os = "windows")]
            command.creation_flags(0x08000000);
            let mut child = command.spawn()?;
            let status = tokio::time::timeout(Duration::from_secs(1800), child.wait()).await;
            if status.is_err() { child.start_kill()?; child.wait().await?; }
            anyhow::ensure!(status??.success(), "isolated actual Android qualification failed");
            let (commands, released, history, published): (i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM playback_vod_commands WHERE protocol=2),(SELECT count(*) FROM playback_reservations WHERE state='released'),(SELECT count(*) FROM play_sessions WHERE user_id=$1 AND metadata->>'vod_command_protocol'='2'),(SELECT count(*) FROM playback_vod_commands WHERE history_id IS NOT NULL)")
                .bind(fixture.base.actor.user).fetch_one(&db.pool).await?;
            anyhow::ensure!(commands>0 && commands==released && history==published && history>0, "Android viewer/history/cleanup attribution mismatch");
            let unsettled: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_start_intents WHERE settled_at IS NULL").fetch_one(&db.pool).await?;
            anyhow::ensure!(unsettled==0, "Android left unresolved Start ownership");
            let seeks: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_vod_operations WHERE action='seek' AND state='completed'").fetch_one(&db.pool).await?;
            anyhow::ensure!(seeks>0, "Android did not complete its retained seek");
            println!("VOD_ANDROID_RESULT real_postgres=true real_media=true protocol_two=true resources_released=true");
            Ok(())
        }.await;
        let cleanup = fixture.close().await;
        result?;
        cleanup
    }).await
}
