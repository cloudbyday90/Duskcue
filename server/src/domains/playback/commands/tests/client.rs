// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database, support};
use serde_json::json;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires owned PostgreSQL18, actual media and browser resource workflow"]
async fn real_shared_player_protocol_two() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("duskcue::domains::playback::commands::control=debug")
        .with_ansi(false)
        .try_init();
    database::run(async |db| {
        let mut f=support::Fixture::browser(db).await?;
        let result=async {
            let source=f.source.with_extension("mp4");
            tokio::fs::copy(std::env::var("DUSKCUE_TEST_CONFIGURATION_COPY")?,&source).await?;
            let metadata=tokio::fs::metadata(&source).await?;
            let modified:chrono::DateTime<chrono::Utc>=metadata.modified()?.into();
            sqlx::query("UPDATE media_files SET file_path=$2,container_format='mp4',file_size=$3,file_modified_at=$4,updated_at=clock_timestamp() WHERE id=$1")
                .bind(f.file).bind(source.to_string_lossy().as_ref()).bind(i64::try_from(metadata.len())?).bind(modified).execute(&db.pool).await?;
            let probe=crate::workers::library_scanner::probe_qualification_source(&source).await?;
            anyhow::ensure!(probe.additional_streams["format"]["duration_seconds"].as_f64().is_some_and(|duration|(8.0..9.0).contains(&duration)));
            sqlx::query("UPDATE media_files SET container_format=$2,video_codec=$3,video_resolution=$4,video_bitrate=$5,video_dynamic_range=$6,audio_codec=$7,audio_channels=$8,audio_bitrate=$9,runtime_seconds=$10,additional_streams=$11 WHERE id=$1")
                .bind(f.file).bind(probe.container_format).bind(probe.video_codec).bind(probe.video_resolution).bind(probe.video_bitrate).bind(probe.video_dynamic_range).bind(probe.audio_codec).bind(probe.audio_channels).bind(probe.audio_bitrate).bind(probe.runtime_seconds).bind(probe.additional_streams).execute(&db.pool).await?;
            sqlx::query("UPDATE media_items SET runtime_seconds=8 WHERE id=$1").bind(f.item).execute(&db.pool).await?;
            let private=f.base.state.bootstrap.data_dir.join("client.json");
            let (recovery_item,recovery_file)=super::client_media::recovery(db,&f).await?;
            let (native_item,native_file)=super::client_media::clean(db,&f).await?;
            let (native_recovery_item,native_recovery_file)=super::client_media::recovery(db,&f).await?;
            tokio::fs::write(&private,serde_json::to_vec(&json!({"token":f.token,"item":f.item,"file":f.file,"user":f.base.actor.user,"profile":f.base.actor.profile,
                "recovery":{"item":recovery_item,"file":recovery_file},"native":{"item":native_item,"file":native_file},"native_recovery":{"item":native_recovery_item,"file":native_recovery_file}}))?).await?;
            let repo=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
            let mut command=tokio::process::Command::new("node");
            command.current_dir(repo.join("clients/web")).args(["node_modules/@playwright/test/cli.js","test","--config","playwright.vod-commands.config.js","--workers=1"])
                .env("DUSKCUE_BACKEND_URL",&f.url).env("DUSKCUE_VOD_CLIENT_FIXTURE",&private).stdin(std::process::Stdio::null()).kill_on_drop(true);
            #[cfg(target_os="windows")]
            command.creation_flags(0x08000000);
            let mut child=command.spawn()?;
            let status=tokio::time::timeout(Duration::from_secs(360),child.wait()).await;
            if status.is_err() {child.start_kill()?;child.wait().await?;}
            anyhow::ensure!(status??.success(),"real protocol-two browser qualification failed");
            if std::env::var_os("DUSKCUE_TEST_VOD_NATIVE_MANIFEST").is_some() {
                let mut command=tokio::process::Command::new("node");
                command.current_dir(repo.join("clients/web")).arg("tests/native/vod-commands.mjs")
                    .env("DUSKCUE_BACKEND_URL",&f.url).env("DUSKCUE_VOD_CLIENT_FIXTURE",&private).stdin(std::process::Stdio::null()).kill_on_drop(true);
                #[cfg(target_os="windows")]
                command.creation_flags(0x08000000);
                let mut child=command.spawn()?;
                let status=tokio::time::timeout(Duration::from_secs(180),child.wait()).await;
                if status.is_err() {child.start_kill()?;child.wait().await?;}
                anyhow::ensure!(status??.success(),"actual Tauri protocol-two qualification failed");
            }
            let (commands,released,history,published):(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM playback_vod_commands WHERE protocol=2),(SELECT count(*) FROM playback_reservations WHERE state='released'),(SELECT count(*) FROM play_sessions WHERE user_id=$1 AND metadata->>'vod_command_protocol'='2'),(SELECT count(*) FROM playback_vod_commands WHERE history_id IS NOT NULL)")
                .bind(f.base.actor.user).fetch_one(&db.pool).await?;
            anyhow::ensure!(commands>=2 && commands==released && history==published && history>0,"real viewer/history/cleanup attribution mismatch");
            let unsettled:i64=sqlx::query_scalar("SELECT count(*) FROM playback_start_intents WHERE settled_at IS NULL").fetch_one(&db.pool).await?;
            anyhow::ensure!(unsettled==0,"browser left unresolved Start ownership");
            let recovered:i64=sqlx::query_scalar("SELECT count(*) FROM playback_vod_operations WHERE action='recover' AND state='completed'").fetch_one(&db.pool).await?;
            anyhow::ensure!(recovered>0,"the real decoder must recover through a retained operation");
            println!("VOD_CLIENT_RESULT real_postgres=true real_media=true protocol_two=true resources_released=true");
            Ok(())
        }.await;
        let cleanup=f.close().await;
        result?;cleanup
    }).await
}
