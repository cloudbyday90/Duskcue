// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database::Database, fixture, support};
use crate::{
    domains::household_security::{context, types::Scope},
    extractors::CredentialTransport,
};
use reqwest::{Method, StatusCode};
use uuid::Uuid;

fn hash(token: &str) -> String {
    ring::digest::digest(&ring::digest::SHA256, token.as_bytes())
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) async fn bearer(db: &Database) -> anyhow::Result<()> {
    support::run(db, async |f| {
        let context = binding(
            &db.pool,
            Scope {
                transport: CredentialTransport::Bearer,
                ..f.base.actor
            },
        )
        .await?;
        let request = |method: Method, path: &str| {
            f.client
                .request(method, format!("{}{path}", f.url))
                .bearer_auth(&f.token)
                .header("x-session-context", &context)
        };
        for hls in [false, true] {
            let id = Uuid::now_v7();
            let path = format!("/api/v1/playback-commands/{id}");
            let response = request(Method::POST, "/api/v1/playback-commands")
                .json(&f.start_body(id, hls, false))
                .send()
                .await?;
            anyhow::ensure!(response.status() == StatusCode::ACCEPTED);
            let ready = tokio::time::timeout(std::time::Duration::from_secs(20), async {
                loop {
                    let response = request(Method::GET, &path).send().await?;
                    anyhow::ensure!(response.status() == StatusCode::OK);
                    let value: serde_json::Value = response.json().await?;
                    if value["status"] == "ready" {
                        return Ok::<_, anyhow::Error>(value);
                    }
                    anyhow::ensure!(value["status"] == "preparing");
                    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                }
            })
            .await??;
            let output = ready["stream_url"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("bearer output missing"))?;
            let response = request(Method::GET, output).send().await?;
            anyhow::ensure!(
                response.status() == StatusCode::OK && !response.bytes().await?.is_empty()
            );
            let wrong = f.response(Method::GET, output, None, false).await?;
            anyhow::ensure!(wrong.status() == StatusCode::CONFLICT);
            let response = request(Method::POST, &format!("{path}/renew"))
                .json(&serde_json::json!({}))
                .send()
                .await?;
            anyhow::ensure!(
                response.status() == StatusCode::OK
                    && response.json::<serde_json::Value>().await?["status"] == "renewed"
            );
            let response = request(Method::POST, &format!("{path}/stop"))
                .json(&serde_json::json!({}))
                .send()
                .await?;
            anyhow::ensure!(response.status() == StatusCode::ACCEPTED);
            tokio::time::timeout(std::time::Duration::from_secs(8), async {
                loop {
                    let response = request(Method::GET, &format!("{path}/retirement"))
                        .send()
                        .await?;
                    anyhow::ensure!(response.status() == StatusCode::OK);
                    let value: serde_json::Value = response.json().await?;
                    anyhow::ensure!(value["stream_url"].is_null());
                    if value["status"] == "released" {
                        return Ok::<_, anyhow::Error>(());
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                }
            })
            .await??;
        }
        Ok(())
    })
    .await
}
async fn binding(pool: &sqlx::PgPool, scope: Scope) -> anyhow::Result<String> {
    let mut conn = pool.acquire().await?;
    Ok(context::read(&mut conn, scope).await?.context_id)
}

pub(super) async fn sessions(db: &Database) -> anyhow::Result<()> {
    support::run(db,async|f| {
        let id=Uuid::now_v7();f.start(id,false,false).await?;f.ready(id).await?;
        let token=crate::domains::auth::service::generate_session_token();let session=Uuid::now_v7();
        sqlx::query("INSERT INTO user_sessions(id,user_id,active_profile_id,token_hash,expires_at) VALUES ($1,$2,$3,$4,clock_timestamp()+interval '1 hour')").bind(session).bind(f.base.actor.user).bind(f.base.actor.profile).bind(hash(&token)).execute(&db.pool).await?;
        let peer=Scope{session,transport:CredentialTransport::Cookie,..f.base.actor};
        let context=binding(&db.pool,peer).await?;
        let response=f.client.get(format!("{}/api/v1/playback-commands/{id}",f.url)).header("cookie",format!("session={token}")).header("x-session-context",context).send().await?;
        anyhow::ensure!(response.status()==StatusCode::CONFLICT);
        let other=fixture::Fixture::new(db).await?;
        sqlx::query("UPDATE users SET streaming_policy_id=$2 WHERE id=$1").bind(other.actor.user).bind(f.policy).execute(&db.pool).await?;
        let token=crate::domains::auth::service::generate_session_token();
        sqlx::query("UPDATE user_sessions SET token_hash=$2 WHERE id=$1").bind(other.actor.session).bind(hash(&token)).execute(&db.pool).await?;
        let context=binding(&db.pool,Scope{transport:CredentialTransport::Cookie,..other.actor}).await?;
        let response=f.client.get(format!("{}/api/v1/playback-commands/{id}",f.url)).header("cookie",format!("session={token}")).header("x-session-context",&context).send().await?;
        anyhow::ensure!(response.status()==StatusCode::OK&&response.json::<serde_json::Value>().await?["status"]=="unknown");
        let response=f.client.post(format!("{}/api/v1/playback-commands",f.url)).header("cookie",format!("session={token}")).header("x-session-context",context).json(&f.start_body(id,false,false)).send().await?;
        anyhow::ensure!(response.status()==StatusCode::CONFLICT);
        f.stop(id).await?;
        Ok(())
    }).await
}

pub(super) async fn profile_cleanup(db: &Database) -> anyhow::Result<()> {
    support::run(db, async |f| {
        let id = Uuid::now_v7();
        f.start(id, false, false).await?;
        f.ready(id).await?;
        let peer = Uuid::now_v7();
        sqlx::query("INSERT INTO user_profiles(id,owner_user_id,name) VALUES ($1,$2,'Other')")
            .bind(peer)
            .bind(f.base.actor.user)
            .execute(&db.pool)
            .await?;
        sqlx::query("UPDATE user_sessions SET active_profile_id=$2 WHERE id=$1")
            .bind(f.base.actor.session)
            .bind(peer)
            .execute(&db.pool)
            .await?;
        sqlx::query("UPDATE user_sessions SET active_profile_id=$2 WHERE id=$1")
            .bind(f.base.actor.session)
            .bind(f.base.actor.profile)
            .execute(&db.pool)
            .await?;
        let current = binding(
            &db.pool,
            Scope {
                transport: CredentialTransport::Cookie,
                ..f.base.actor
            },
        )
        .await?;
        let response = f
            .client
            .get(format!("{}/api/v1/playback-commands/{id}", f.url))
            .header("cookie", format!("session={}", f.token))
            .header("x-session-context", &current)
            .send()
            .await?;
        anyhow::ensure!(response.status() == StatusCode::CONFLICT);
        f.stop(id).await?;
        let next = Uuid::now_v7();
        let response = f
            .client
            .post(format!("{}/api/v1/playback-commands", f.url))
            .header("cookie", format!("session={}", f.token))
            .header("x-session-context", &current)
            .json(&f.start_body(next, false, false))
            .send()
            .await?;
        anyhow::ensure!(response.status() == StatusCode::ACCEPTED);
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let ready: bool = sqlx::query_scalar(
                    "SELECT state='ready' FROM playback_vod_commands WHERE reservation_id=$1",
                )
                .bind(next)
                .fetch_one(&db.pool)
                .await?;
                if ready {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        })
        .await??;
        sqlx::query("DELETE FROM user_profiles WHERE id=$1")
            .bind(f.base.actor.profile)
            .execute(&db.pool)
            .await?;
        f.stop(next).await?;
        let retired = f
            .json(
                Method::GET,
                &format!("/api/v1/playback-commands/{next}/retirement"),
                None,
            )
            .await?;
        anyhow::ensure!(
            retired.0 == StatusCode::OK
                && retired.1["status"] == "released"
                && retired.1.get("stream_url").is_none_or(|v| v.is_null())
        );
        let written: i64 =
            sqlx::query_scalar("SELECT count(*) FROM user_item_data WHERE profile_id=$1")
                .bind(peer)
                .fetch_one(&db.pool)
                .await?;
        anyhow::ensure!(written == 0);
        Ok(())
    })
    .await
}

pub(super) async fn source_revocation(db: &Database) -> anyhow::Result<()> {
    support::run(db,async|f| {
        let id=Uuid::now_v7();f.start(id,false,false).await?;f.ready(id).await?;
        let original:serde_json::Value=sqlx::query_scalar("SELECT additional_streams FROM media_files WHERE id=$1").bind(f.file).fetch_one(&db.pool).await?;
        sqlx::query("UPDATE media_files SET additional_streams=additional_streams||'{\"revision\":2}'::jsonb WHERE id=$1").bind(f.file).execute(&db.pool).await?;
        sqlx::query("UPDATE media_files SET additional_streams=$2 WHERE id=$1").bind(f.file).bind(original).execute(&db.pool).await?;
        let state:String=sqlx::query_scalar("SELECT state FROM playback_reservations WHERE id=$1").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(state!="held");f.stop(id).await?;
        Ok(())
    }).await
}

pub(super) async fn credential_revocation(db: &Database) -> anyhow::Result<()> {
    support::run(db, async |f| {
        let id = Uuid::now_v7();
        f.start(id, false, false).await?;
        f.ready(id).await?;
        sqlx::query("UPDATE user_sessions SET credential_version=2 WHERE id=$1")
            .bind(f.base.actor.session)
            .execute(&db.pool)
            .await?;
        sqlx::query("UPDATE user_sessions SET credential_version=1 WHERE id=$1")
            .bind(f.base.actor.session)
            .execute(&db.pool)
            .await?;
        let (status, value) = f
            .json(
                Method::GET,
                &format!("/api/v1/playback-commands/{id}"),
                None,
            )
            .await?;
        anyhow::ensure!(status == StatusCode::OK && value["stream_url"].is_null());
        f.stop(id).await?;
        Ok(())
    })
    .await
}

pub(super) async fn kids_local(db: &Database) -> anyhow::Result<()> {
    support::run(db,async|f| {
        sqlx::query("UPDATE user_profiles SET profile_type='kids',max_content_rating='TV-14' WHERE id=$1").bind(f.base.actor.profile).execute(&db.pool).await?;
        sqlx::query("INSERT INTO profile_library_access(profile_id,library_id) SELECT $1,library_id FROM media_items WHERE id=$2").bind(f.base.actor.profile).bind(f.item).execute(&db.pool).await?;
        let id=Uuid::now_v7();f.start(id,false,false).await?;f.ready(id).await?;f.stop(id).await?;
        sqlx::query("UPDATE user_profiles SET max_content_rating='TV-Y' WHERE id=$1").bind(f.base.actor.profile).execute(&db.pool).await?;
        let refused=f.json(Method::POST,"/api/v1/playback-commands",Some(f.start_body(Uuid::now_v7(),false,false))).await?;
        anyhow::ensure!(refused.0==StatusCode::FORBIDDEN);
        Ok(())
    }).await
}
