// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database, fixture, support};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

async fn credential(pool: &sqlx::PgPool, session: Uuid) -> anyhow::Result<String> {
    let token = crate::domains::auth::service::generate_session_token();
    let hash = ring::digest::digest(&ring::digest::SHA256, token.as_bytes())
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    sqlx::query("UPDATE user_sessions SET token_hash=$2 WHERE id=$1")
        .bind(session)
        .bind(hash)
        .execute(pool)
        .await?;
    Ok(token)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires managed isolated Android emulator, PostgreSQL18 and actual FFmpeg media"]
async fn real_android_scope_ownership() -> anyhow::Result<()> {
    database::run(async |db| {
        let mut f = support::Fixture::browser_paced(db).await?;
        let result = async {
            let (item, _) = super::client_media::subtitled(db, &f).await?;
            let profile = Uuid::now_v7();
            sqlx::query("INSERT INTO user_profiles(id,owner_user_id,name) VALUES ($1,$2,'Scope peer')")
                .bind(profile).bind(f.base.actor.user).execute(&db.pool).await?;
            let session = Uuid::now_v7();
            sqlx::query("INSERT INTO user_sessions(id,user_id,active_profile_id,token_hash,expires_at) VALUES ($1,$2,$3,$4,clock_timestamp()+interval '1 hour')")
                .bind(session).bind(f.base.actor.user).bind(profile).bind(format!("pending-{session}")).execute(&db.pool).await?;
            let peer_token = credential(&db.pool, session).await?;
            let other = fixture::Fixture::new(db).await?;
            sqlx::query("UPDATE users SET streaming_policy_id=$2 WHERE id=$1")
                .bind(other.actor.user).bind(f.policy).execute(&db.pool).await?;
            let other_token = credential(&db.pool, other.actor.session).await?;
            let private = f.base.state.bootstrap.data_dir.join("android-scope.json");
            tokio::fs::write(&private, serde_json::to_vec(&json!({
                "suite":"scope","token":f.token,"origin":f.url,"item":item,
                "user":f.base.actor.user,"profile":f.base.actor.profile,"peer_profile":profile,
                "peer_token":peer_token,"other_token":other_token,"other_user":other.actor.user,"other_profile":other.actor.profile
            }))?).await?;
            let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
            let mut command = tokio::process::Command::new("node");
            command.current_dir(repo).arg("scripts/testing-memory/android-runtime.mjs")
                .env("DUSKCUE_VOD_CLIENT_FIXTURE", &private)
                .stdin(std::process::Stdio::null()).kill_on_drop(true).creation_flags(0x08000000);
            let mut child = command.spawn()?;
            let status = tokio::time::timeout(Duration::from_secs(1800), child.wait()).await;
            if status.is_err() { child.start_kill()?; child.wait().await?; }
            anyhow::ensure!(status??.success(), "isolated actual Android scope qualification failed");
            tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    let active: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_reservations WHERE state!='released'").fetch_one(&db.pool).await?;
                    if active==0 { return Ok::<_,anyhow::Error>(()); }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }).await??;
            let (commands, released, history, unsettled): (i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM playback_vod_commands WHERE protocol=2),(SELECT count(*) FROM playback_reservations WHERE state='released'),(SELECT count(*) FROM play_sessions WHERE metadata->>'vod_command_protocol'='2'),(SELECT count(*) FROM playback_start_intents WHERE settled_at IS NULL)").fetch_one(&db.pool).await?;
            anyhow::ensure!(commands==4 && released==4 && history==4 && unsettled==0, "scope viewer/history/Start settlement attribution mismatch");
            let primary: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_reservations WHERE household_id=$1").bind(f.base.actor.user).fetch_one(&db.pool).await?;
            let secondary: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_reservations WHERE household_id=$1").bind(other.actor.user).fetch_one(&db.pool).await?;
            anyhow::ensure!(primary==3 && secondary==1, "scope account attribution mismatch");
            println!("VOD_ANDROID_SCOPE_RESULT real_provider=true real_postgres=true real_native=true viewers=4 histories=4 unsettled=0 resources_released=true");
            Ok(())
        }.await;
        let cleanup = f.close().await;
        result?;
        cleanup
    }).await
}
