// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database::Database, support};
use crate::{
    domains::playback::{
        service,
        types::{CreateStreamingPolicyRequest, UpdateStreamingPolicyRequest},
    },
    services::playback_admission::{AdmissionOwner, operator},
};
use reqwest::{Method, StatusCode};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

pub(super) async fn policies(db: &Database) -> anyhow::Result<()> {
    support::run(db,async |f| {
        let default:CreateStreamingPolicyRequest=serde_json::from_value(json!({"name":"Strict default","is_default":true,"allow_direct_play":false}))?;
        let strict=service::create_streaming_policy(&db.pool,&default).await?;
        let id=Uuid::now_v7();f.start(id,false,false).await?;f.ready(id).await?;
        let quality:UpdateStreamingPolicyRequest=serde_json::from_value(json!({"max_streams":0,"bandwidth_limit_bps":1}))?;
        service::update_streaming_policy(&db.pool,f.policy,&quality).await?;
        let (code,_)=f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/renew"),Some(json!({}))).await?;
        anyhow::ensure!(code==StatusCode::OK);
        sqlx::raw_sql("CREATE FUNCTION command_test_refuse_delete() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled policy deletion failure'; END; $$; CREATE TRIGGER command_test_refuse_delete BEFORE DELETE ON streaming_policies FOR EACH ROW EXECUTE FUNCTION command_test_refuse_delete();").execute(&db.pool).await?;
        anyhow::ensure!(service::delete_streaming_policy(&db.pool,f.policy).await.is_err());
        let assigned:Option<Uuid>=sqlx::query_scalar("SELECT streaming_policy_id FROM users WHERE id=$1").bind(f.base.actor.user).fetch_one(&db.pool).await?;
        anyhow::ensure!(assigned==Some(f.policy));
        sqlx::raw_sql("DROP TRIGGER command_test_refuse_delete ON streaming_policies; DROP FUNCTION command_test_refuse_delete();").execute(&db.pool).await?;
        service::delete_streaming_policy(&db.pool,f.policy).await?;
        let state:String=sqlx::query_scalar("SELECT state FROM playback_reservations WHERE id=$1").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(state!="held");f.stop(id).await?;
        let refused=f.json(Method::POST,"/api/v1/playback-commands",Some(f.start_body(Uuid::now_v7(),false,false))).await?;
        anyhow::ensure!(refused.0==StatusCode::FORBIDDEN);
        let replacement:CreateStreamingPolicyRequest=serde_json::from_value(json!({"name":"Permissive default","is_default":true,"max_streams":4}))?;
        service::create_streaming_policy(&db.pool,&replacement).await?;
        let next=Uuid::now_v7();f.start(next,false,false).await?;f.ready(next).await?;
        let change:UpdateStreamingPolicyRequest=serde_json::from_value(json!({"is_default":true}))?;
        service::update_streaming_policy(&db.pool,strict.id,&change).await?;
        let state:String=sqlx::query_scalar("SELECT state FROM playback_reservations WHERE id=$1").bind(next).fetch_one(&db.pool).await?;
        anyhow::ensure!(state!="held");f.stop(next).await?;
        Ok(())
    }).await
}

pub(super) async fn retirement(db: &Database) -> anyhow::Result<()> {
    support::run(db, async |f| {
        let direct = Uuid::now_v7();
        let hls = Uuid::now_v7();
        f.start(direct, false, false).await?;
        f.ready(direct).await?;
        f.start(hls, true, false).await?;
        f.ready(hls).await?;
        let viewer = f
            .base
            .state
            .vod_commands
            .get(direct)
            .ok_or_else(|| anyhow::anyhow!("missing viewer"))?;
        let response = crate::domains::playback::file_delivery::response_scoped(
            &f.base.state.transcode_manager.output,
            &f.source,
            &Default::default(),
            Some(viewer.group.clone()),
            Some(viewer.prepared.identity.clone()),
        )
        .await?;
        viewer.group.retire();
        tokio::time::timeout(Duration::from_secs(3), viewer.group.drain()).await??;
        anyhow::ensure!(
            axum::body::to_bytes(response.into_body(), 16777216)
                .await
                .is_err()
        );
        f.stop(direct).await?;
        let still = f
            .json(
                Method::GET,
                &format!("/api/v1/playback-commands/{hls}"),
                None,
            )
            .await?;
        anyhow::ensure!(still.1["status"] == "ready");
        let physical = f
            .base
            .state
            .vod_commands
            .get(hls)
            .ok_or_else(|| anyhow::anyhow!("missing HLS viewer"))?
            .current()?
            .generation
            .unwrap();
        f.base
            .state
            .transcode_manager
            .output
            .generation(physical)
            .unwrap()
            .retire();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let renewal = f
            .json(
                Method::POST,
                &format!("/api/v1/playback-commands/{hls}/renew"),
                Some(json!({})),
            )
            .await?;
        anyhow::ensure!(renewal.0 == StatusCode::OK);
        let unavailable = f
            .json(
                Method::GET,
                &format!("/api/v1/playback-commands/{hls}"),
                None,
            )
            .await?;
        anyhow::ensure!(
            unavailable.1["status"] == "unavailable" && unavailable.1["stream_url"].is_null()
        );
        let held: bool =
            sqlx::query_scalar("SELECT state='held' FROM playback_reservations WHERE id=$1")
                .bind(hls)
                .fetch_one(&db.pool)
                .await?;
        anyhow::ensure!(held);
        f.stop(hls).await?;
        for query in [
            "DELETE FROM playback_vod_commands WHERE reservation_id=$1",
            "UPDATE playback_vod_commands SET state='ready' WHERE reservation_id=$1",
        ] {
            anyhow::ensure!(
                sqlx::query(query)
                    .bind(hls)
                    .execute(&db.pool)
                    .await
                    .is_err()
            );
        }
        anyhow::ensure!(
            sqlx::query("TRUNCATE playback_vod_commands")
                .execute(&db.pool)
                .await
                .is_err()
        );
        Ok(())
    })
    .await
}

pub(super) async fn ambient(db: &Database) -> anyhow::Result<()> {
    support::run(db,async |f| {
        let channel=Uuid::now_v7();
        let revision:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("INSERT INTO ambient_channels(id,owner_user_id,name,audience,is_enabled) VALUES ($1,$2,'Qualification','standard',true) RETURNING updated_at").bind(channel).bind(f.base.actor.user).fetch_one(&db.pool).await?;
        sqlx::query("INSERT INTO ambient_channel_items(channel_id,media_item_id,position) VALUES ($1,$2,0)").bind(channel).bind(f.item).execute(&db.pool).await?;
        for mode in 0..3 {
            let id=Uuid::now_v7();let mut body=f.start_body(id,false,false);
            body["selection"]["playback_mode"]=json!("ambient");body["selection"]["ambient_channel_id"]=json!(channel);body["selection"]["ambient_channel_updated_at"]=json!(revision);
            let (code,value)=f.json(Method::POST,"/api/v1/playback-commands",Some(body)).await?;
            anyhow::ensure!(code==StatusCode::ACCEPTED,"ambient refused: {value}");f.ready(id).await?;
            match mode {
                0=>{
                    sqlx::query("UPDATE user_profiles SET allow_ambient_channels=false WHERE id=$1").bind(f.base.actor.profile).execute(&db.pool).await?;
                    sqlx::query("UPDATE user_profiles SET allow_ambient_channels=true WHERE id=$1").bind(f.base.actor.profile).execute(&db.pool).await?;
                },
                1=>{
                    sqlx::query("DELETE FROM ambient_channel_items WHERE channel_id=$1").bind(channel).execute(&db.pool).await?;
                    sqlx::query("INSERT INTO ambient_channel_items(channel_id,media_item_id,position) VALUES ($1,$2,0)").bind(channel).bind(f.item).execute(&db.pool).await?;
                },
                _=>{sqlx::query("DELETE FROM ambient_channels WHERE id=$1").bind(channel).execute(&db.pool).await?;},
            }
            let state:String=sqlx::query_scalar("SELECT state FROM playback_reservations WHERE id=$1").bind(id).fetch_one(&db.pool).await?;
            anyhow::ensure!(state!="held");f.stop(id).await?;
        }
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ambient_channels WHERE id=$1)").bind(channel).fetch_one(&db.pool).await?;
        anyhow::ensure!(!exists);Ok(())
    }).await
}

pub(super) async fn expiry(db: &Database) -> anyhow::Result<()> {
    support::run(db, async |f| {
        let id = Uuid::now_v7();
        f.start(id, false, false).await?;
        f.ready(id).await?;
        tokio::time::sleep(Duration::from_secs(61)).await;
        let (code, value) = f
            .json(
                Method::GET,
                &format!("/api/v1/playback-commands/{id}"),
                None,
            )
            .await?;
        anyhow::ensure!(
            code == StatusCode::OK
                && value["status"] != "ready"
                && value.get("stream_url").is_none_or(|v| v.is_null())
        );
        let renewed = f
            .json(
                Method::POST,
                &format!("/api/v1/playback-commands/{id}/renew"),
                Some(json!({})),
            )
            .await?;
        anyhow::ensure!(!renewed.0.is_success());
        let replay = f
            .json(
                Method::POST,
                "/api/v1/playback-commands",
                Some(f.start_body(id, false, false)),
            )
            .await?;
        anyhow::ensure!(replay.1["status"] != "preparing" && replay.1["status"] != "ready");
        f.stop(id).await?;
        Ok(())
    })
    .await
}

pub(super) async fn foreign(db: &Database) -> anyhow::Result<()> {
    support::run(db,async |f| {
        let first=Uuid::now_v7();f.start(first,false,false).await?;f.ready(first).await?;
        let old=AdmissionOwner::new(db.pool.clone(),f.base.state.encryption_key.clone(),None).await?;
        let id=Uuid::now_v7();
        sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind) SELECT $1,$2,household_id,fingerprint,kind FROM playback_reservations WHERE id=$3").bind(id).bind(old.instance()).bind(first).execute(&db.pool).await?;
        sqlx::query("INSERT INTO playback_vod_commands(reservation_id,intent_hash,policy_hash,source_hash,delivery_mode,encodes,source_4k) SELECT $1,intent_hash,policy_hash,source_hash,delivery_mode,encodes,source_4k FROM playback_vod_commands WHERE reservation_id=$2").bind(id).bind(first).execute(&db.pool).await?;
        old.close();old.drain().await?;
        let start=f.json(Method::POST,"/api/v1/playback-commands",Some(f.start_body(id,false,false))).await?;
        anyhow::ensure!(start.0==StatusCode::ACCEPTED&&start.1["status"]=="uncertain"&&f.base.state.vod_commands.get(id).is_none());
        let stopped=f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/stop"),Some(json!({}))).await?;
        anyhow::ensure!(stopped.1["status"]=="uncertain");
        anyhow::ensure!(operator::reconcile(&db.pool,old.instance(),1,true).await.is_err());
        let revision=operator::fence(&db.pool,old.instance(),1).await?;
        anyhow::ensure!(operator::reconcile(&db.pool,old.instance(),revision,false).await.is_err());
        anyhow::ensure!(operator::reconcile(&db.pool,old.instance(),revision,true).await?==1);
        let completed:bool=sqlx::query_scalar("SELECT resources_completed FROM playback_vod_commands WHERE reservation_id=$1").bind(id).fetch_one(&db.pool).await?;
        anyhow::ensure!(completed);f.stop(first).await?;Ok(())
    }).await
}

pub(super) async fn root_shutdown(db: &Database) -> anyhow::Result<()> {
    support::run(db, async |f| {
        let direct = Uuid::now_v7();
        let hls = Uuid::now_v7();
        f.start(direct, false, false).await?;
        f.ready(direct).await?;
        f.start(hls, true, false).await?;
        f.ready(hls).await?;
        let viewer = f
            .base
            .state
            .vod_commands
            .get(direct)
            .ok_or_else(|| anyhow::anyhow!("missing root viewer"))?;
        let response = crate::domains::playback::file_delivery::response_scoped(
            &f.base.state.transcode_manager.output,
            &f.source,
            &Default::default(),
            Some(viewer.group.clone()),
            Some(viewer.prepared.identity.clone()),
        )
        .await?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mut http = crate::services::runtime::HttpServer::start(
            listener,
            axum::Router::new(),
            tokio_util::sync::CancellationToken::new(),
        );
        crate::services::runtime::shutdown(
            &f.base.state,
            &tokio_util::task::TaskTracker::new(),
            &mut http,
        )
        .await?;
        anyhow::ensure!(f.base.state.pool.is_closed() && http.finished());
        anyhow::ensure!(
            axum::body::to_bytes(response.into_body(), 16777216)
                .await
                .is_err()
        );
        let check = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&db.url)
            .await?;
        let active: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM playback_reservations WHERE state<>'released'",
        )
        .fetch_one(&check)
        .await?;
        check.close().await;
        anyhow::ensure!(
            active == 0
                && f.base.state.vod_commands.get(direct).is_none()
                && f.base.state.vod_commands.get(hls).is_none()
        );
        Ok(())
    })
    .await
}
