// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database, support};
use serde_json::{Value, json};
use std::time::Duration;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires managed isolated Android emulator, PostgreSQL18 and actual FFmpeg media"]
async fn real_android_ambient_ownership() -> anyhow::Result<()> {
    database::run(async |db| {
        let mut f = support::Fixture::browser_paced(db).await?;
        let result = async {
            let (first, _) = super::client_media::subtitled(db, &f).await?;
            let (second, _) = super::client_media::subtitled(db, &f).await?;
            let (interactive, _) = super::client_media::subtitled(db, &f).await?;
            let channel = Uuid::now_v7();
            let next_channel = Uuid::now_v7();
            for (id, items) in [(channel, vec![f.item]), (next_channel, vec![first, second])] {
                sqlx::query("INSERT INTO ambient_channels(id,owner_user_id,name,audience,is_enabled) VALUES ($1,$2,'Native qualification','standard',true)")
                    .bind(id).bind(f.base.actor.user).execute(&db.pool).await?;
                for (position, item) in items.iter().enumerate() {
                    sqlx::query("INSERT INTO ambient_channel_items(channel_id,media_item_id,position) VALUES ($1,$2,$3)")
                        .bind(id).bind(item).bind(i32::try_from(position)?).execute(&db.pool).await?;
                }
            }
            let profile = Uuid::now_v7();
            sqlx::query("INSERT INTO user_profiles(id,owner_user_id,name) VALUES ($1,$2,'Ambient peer')")
                .bind(profile).bind(f.base.actor.user).execute(&db.pool).await?;
            let session = Uuid::now_v7();
            let peer_token = crate::domains::auth::service::generate_session_token();
            let hash = ring::digest::digest(&ring::digest::SHA256, peer_token.as_bytes())
                .as_ref().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
            sqlx::query("INSERT INTO user_sessions(id,user_id,active_profile_id,token_hash,expires_at) VALUES ($1,$2,$3,$4,clock_timestamp()+interval '1 hour')")
                .bind(session).bind(f.base.actor.user).bind(f.base.actor.profile).bind(hash).execute(&db.pool).await?;
            let sentinel = Uuid::now_v7();
            sqlx::query("INSERT INTO user_item_data(id,user_id,profile_id,media_item_id,is_watched,play_count,resume_position_ms) VALUES ($1,$2,$3,$4,true,11,1234)")
                .bind(sentinel).bind(f.base.actor.user).bind(f.base.actor.profile).bind(f.item).execute(&db.pool).await?;
            let before: Value = sqlx::query_scalar("SELECT to_jsonb(d) FROM user_item_data d WHERE id=$1")
                .bind(sentinel).fetch_one(&db.pool).await?;
            let private = f.base.state.bootstrap.data_dir.join("android-ambient.json");
            tokio::fs::write(&private, serde_json::to_vec(&json!({
                "suite":"ambient", "origin":f.url, "token":f.token, "peer_token":peer_token,
                "user":f.base.actor.user, "profile":f.base.actor.profile, "peer_profile":profile,
                "channel":channel, "next_channel":next_channel, "item":interactive
            }))?).await?;
            let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
            let mut command = tokio::process::Command::new("node");
            command.current_dir(repo).arg("scripts/testing-memory/android-runtime.mjs")
                .env("DUSKCUE_VOD_CLIENT_FIXTURE", &private)
                .stdin(std::process::Stdio::null()).kill_on_drop(true).creation_flags(0x08000000);
            let mut child = command.spawn()?;
            let status = tokio::time::timeout(Duration::from_secs(1800), child.wait()).await;
            if status.is_err() { child.start_kill()?; child.wait().await?; }
            anyhow::ensure!(status??.success(), "actual Android ambient companion qualification failed");
            tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    let active: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_reservations WHERE state!='released'").fetch_one(&db.pool).await?;
                    if active == 0 { return Ok::<_, anyhow::Error>(()); }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }).await??;
            let (commands, released, diagnostic, unsettled): (i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM playback_vod_commands WHERE protocol=2),(SELECT count(*) FROM playback_reservations WHERE state='released'),(SELECT count(*) FROM play_sessions WHERE metadata->>'vod_command_protocol'='2'),(SELECT count(*) FROM playback_start_intents WHERE settled_at IS NULL)").fetch_one(&db.pool).await?;
            anyhow::ensure!(commands == 10 && released == 10 && diagnostic == 10 && unsettled == 0, "ambient companion exact viewer/diagnostic/settlement attribution differs");
            let ambient: i64 = sqlx::query_scalar("SELECT count(*) FROM play_sessions WHERE playback_mode='ambient'").fetch_one(&db.pool).await?;
            anyhow::ensure!(ambient == 9, "ambient and interactive viewer attribution differs");
            let after: Value = sqlx::query_scalar("SELECT to_jsonb(d) FROM user_item_data d WHERE id=$1")
                .bind(sentinel).fetch_one(&db.pool).await?;
            anyhow::ensure!(before == after, "ambient changed the personal-history sentinel");
            let personal: i64 = sqlx::query_scalar("SELECT count(*) FROM user_item_data WHERE id!=$1").bind(sentinel).fetch_one(&db.pool).await?;
            let attributed: i64 = sqlx::query_scalar("SELECT count(*) FROM user_item_data WHERE media_item_id=$1 AND profile_id=$2")
                .bind(interactive).bind(f.base.actor.profile).fetch_one(&db.pool).await?;
            anyhow::ensure!(personal == 1 && attributed == 1, "ambient mutated personal history outside the independently attributed interactive viewer");
            let changes: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_vod_operations WHERE action='seek' AND state='completed'").fetch_one(&db.pool).await?;
            anyhow::ensure!(changes == 1, "ambient must perform one exact same-viewer seek");
            println!("VOD_ANDROID_AMBIENT_RESULT real_postgres=true real_native=true viewers=10 ambient=9 personal_interactive=1 sentinel_unchanged=true unsettled=0 resources_released=true");
            Ok(())
        }.await;
        let cleanup = f.close().await;
        result?;
        cleanup
    }).await
}
