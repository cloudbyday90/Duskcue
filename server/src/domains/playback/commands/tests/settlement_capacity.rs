// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database, support};
use reqwest::Method;
use serde_json::json;
use uuid::Uuid;

pub(super) async fn quotas(db: &database::Database) -> anyhow::Result<()> {
    support::run(db,async |f| {
        let id=Uuid::now_v7();f.start(id,false,false).await?;f.ready(id).await?;
        sqlx::query("INSERT INTO playback_start_intents(id,household_id,session_id,credential_version,transport,instance_id,admission_fenced,preparation_complete) SELECT uuidv7(),$1,$2,1,0,$3,true,true FROM generate_series(1,4095)")
            .bind(f.base.actor.user).bind(f.base.actor.session).bind(f.owner.instance()).execute(&db.pool).await?;
        let (status,_)=f.json(Method::POST,&format!("/api/v1/playback-commands/{}/withdraw",Uuid::now_v7()),Some(json!({"protocol":"duskcue-vod-command/2"}))).await?;
        anyhow::ensure!(status==503,"a full receipt budget cannot acknowledge an unseen fence");
        let (status,_)=f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/renew"),Some(json!({}))).await?;
        anyhow::ensure!(status==200);
        let (status,_)=f.json(Method::POST,&format!("/api/v1/playback-commands/{id}/withdraw"),Some(json!({"protocol":"duskcue-vod-command/2"}))).await?;
        anyhow::ensure!(status==202,"existing intent withdrawal must not need receipt capacity");
        let total:i64=sqlx::query_scalar("SELECT retained_count FROM playback_receipt_households WHERE household_id=$1").bind(f.base.actor.user).fetch_one(&db.pool).await?;
        anyhow::ensure!(total==4096);
        Ok(())
    }).await
}

pub(super) async fn no_command_slot(db: &database::Database) -> anyhow::Result<()> {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    support::run(db, async |f| {
        let entered = Arc::new(AtomicUsize::new(0));
        let finish = tokio_util::sync::CancellationToken::new();
        let mut tasks = Vec::with_capacity(128);
        for _ in 0..128 {
            let entered = entered.clone();
            let finish = finish.clone();
            let output = f.base.state.transcode_manager.output.clone();
            tasks.push(tokio::spawn(async move {
                output
                    .command(false, move |_| async move {
                        entered.fetch_add(1, Ordering::SeqCst);
                        finish.cancelled().await;
                        Ok(())
                    })
                    .await
            }));
        }
        let result = async {
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                while entered.load(Ordering::SeqCst) != 128 {
                    tokio::task::yield_now().await;
                }
            })
            .await?;
            anyhow::ensure!(
                f.base
                    .state
                    .transcode_manager
                    .output
                    .command(false, |_| async { Ok(()) })
                    .await
                    .is_err()
            );
            let id = Uuid::now_v7();
            let (status, value) = f
                .json(
                    Method::POST,
                    &format!("/api/v1/playback-commands/{id}/withdraw"),
                    Some(json!({"protocol":"duskcue-vod-command/2"})),
                )
                .await?;
            anyhow::ensure!(
                status == 202 && value["status"] == "settled",
                "withdrawal must use its short SQL owner, not another VOD command slot"
            );
            Ok::<_, anyhow::Error>(())
        }
        .await;
        finish.cancel();
        for task in tasks {
            task.await??;
        }
        result
    })
    .await
}
