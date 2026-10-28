// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database, support};
use reqwest::Method;
use serde_json::json;
use uuid::Uuid;

pub(super) async fn boundaries(db: &database::Database) -> anyhow::Result<()> {
    support::run(db, async |f| {
        let id = Uuid::now_v7();
        let path = format!("/api/v1/playback-commands/{id}");
        let (status, unknown) = f.json(Method::GET, &path, None).await?;
        anyhow::ensure!(status.is_success() && unknown["status"] == "unknown");
        let (status, settled) = f
            .json(
                Method::POST,
                &format!("{path}/withdraw"),
                Some(json!({"protocol":"duskcue-vod-command/2"})),
            )
            .await?;
        anyhow::ensure!(
            status == 202
                && settled["status"] == "settled"
                && settled["admission_fenced"] == true
                && settled["preparation_complete"] == true
        );
        let (status, _) = f
            .json(
                Method::POST,
                "/api/v1/playback-commands",
                Some(f.start_body(id, false, false)),
            )
            .await?;
        anyhow::ensure!(!status.is_success());
        let reservations: i64 =
            sqlx::query_scalar("SELECT count(*) FROM playback_reservations WHERE id=$1")
                .bind(id)
                .fetch_one(&db.pool)
                .await?;
        anyhow::ensure!(reservations == 0);
        let count: i64 = sqlx::query_scalar(
            "SELECT retained_count FROM playback_receipt_households WHERE household_id=$1",
        )
        .bind(f.base.actor.user)
        .fetch_one(&db.pool)
        .await?;
        anyhow::ensure!(count == 1);
        let early = sqlx::query("DELETE FROM playback_start_intents WHERE id=$1")
            .bind(id)
            .execute(&db.pool)
            .await;
        anyhow::ensure!(early.is_err());
        let regression =
            sqlx::query("UPDATE playback_start_intents SET admission_fenced=false WHERE id=$1")
                .bind(id)
                .execute(&db.pool)
                .await;
        anyhow::ensure!(regression.is_err());
        let id = Uuid::now_v7();
        f.start(id, false, false).await?;
        f.ready(id).await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT retained_count FROM playback_receipt_households WHERE household_id=$1",
        )
        .bind(f.base.actor.user)
        .fetch_one(&db.pool)
        .await?;
        anyhow::ensure!(count == 2, "promotion must not double count");
        let (status, _) = f
            .json(
                Method::POST,
                &format!("/api/v1/playback-commands/{id}/withdraw"),
                Some(json!({"protocol":"duskcue-vod-command/2"})),
            )
            .await?;
        anyhow::ensure!(status == 202);
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let (_, value) = f
                    .json(
                        Method::GET,
                        &format!("/api/v1/playback-commands/{id}/settlement"),
                        None,
                    )
                    .await?;
                if value["status"] == "settled" {
                    anyhow::ensure!(value["parent_released"] == true);
                    break Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        })
        .await??;
        let bearer = f
            .response(
                Method::GET,
                &format!("/api/v1/playback-commands/{id}/settlement"),
                None,
                true,
            )
            .await?;
        anyhow::ensure!(
            bearer.status() == 409,
            "transport changes must not adopt cleanup authority"
        );
        Ok(())
    })
    .await
}

pub(super) async fn retained_inspection(db: &database::Database) -> anyhow::Result<()> {
    use base64::Engine;
    support::run(db, async |f| {
        let id = Uuid::now_v7();
        let mut scope = f.base.actor;
        scope.transport = crate::extractors::CredentialTransport::Cookie;
        scope.binding = Some(
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(f.context.strip_prefix("sc1.").unwrap())?
                .try_into()
                .map_err(|_| anyhow::anyhow!("context shape"))?,
        );
        let intent: super::super::types::StartRequest =
            serde_json::from_value(f.start_body(id, false, false))?;
        let intent = intent.intent()?;
        anyhow::ensure!(
            super::super::settlement::register(&f.base.state, &f.owner, scope, id, intent.hash)
                .await?
        );
        anyhow::ensure!(
            !super::super::settlement::register(&f.base.state, &f.owner, scope, id, intent.hash)
                .await?
        );
        let stop = f.base.state.vod_commands.prepare(id)?;
        let (began, started) = tokio::sync::oneshot::channel();
        let (finish, wait) = std::sync::mpsc::sync_channel(1);
        let output = f.base.state.transcode_manager.output.clone();
        let inspector = output.clone();
        let source = f.source.clone();
        let owner = f.owner.clone();
        let commands = f.base.state.vod_commands.clone();
        let task = tokio::spawn(async move {
            output
                .command(false, move |_| async move {
                    let metadata = crate::domains::playback::inspection::scope(
                        inspector,
                        stop,
                        crate::domains::playback::inspection::run(move || {
                            let metadata = std::fs::metadata(source).ok()?;
                            let _ = began.send(());
                            wait.recv_timeout(std::time::Duration::from_secs(10)).ok()?;
                            Some(metadata.len())
                        }),
                    )
                    .await;
                    metadata.ok_or(crate::domains::playback::PlaybackError::LifecycleCleanup)?;
                    super::super::settlement::complete(&owner, scope, id, true)
                        .await
                        .map_err(|_| crate::domains::playback::PlaybackError::LifecycleCleanup)?;
                    commands
                        .finish_preparation(id)
                        .map_err(|_| crate::domains::playback::PlaybackError::LifecycleCleanup)?;
                    Ok(())
                })
                .await
        });
        started.await?;
        let (status, pending) = f
            .json(
                Method::POST,
                &format!("/api/v1/playback-commands/{id}/withdraw"),
                Some(json!({"protocol":"duskcue-vod-command/2"})),
            )
            .await?;
        let observed = status == 202
            && pending["admission_fenced"] == true
            && pending["preparation_complete"] == false
            && pending["status"] == "pending";
        let root = f.base.state.transcode_manager.output.clone();
        root.close();
        let draining = tokio::spawn(async move { root.drain_commands().await });
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        let retained = !draining.is_finished();
        finish.send(())?;
        task.await??;
        draining.await??;
        anyhow::ensure!(retained, "root must retain started filesystem inspection");
        anyhow::ensure!(
            observed,
            "withdrawal must retain actual inspection completion"
        );
        let (_, value) = f
            .json(
                Method::GET,
                &format!("/api/v1/playback-commands/{id}/settlement"),
                None,
            )
            .await?;
        anyhow::ensure!(value["status"] == "settled");
        Ok(())
    })
    .await
}
