// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database::Database, support::Fixture};
use crate::services::transcoding::{FailureClass, qualification::FrozenEncoder};
use std::{sync::atomic::Ordering, time::Duration};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub(super) async fn watch(
    db: &Database,
    f: &Fixture,
    stopped: CancellationToken,
) -> anyhow::Result<()> {
    let (viewer, output, generation) = tokio::time::timeout(Duration::from_secs(900), async {
        loop {
            anyhow::ensure!(!stopped.is_cancelled(), "Android exited before its worker stage");
            let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM playback_reservations WHERE vod_item_id=$1 AND state='held' LIMIT 2").bind(f.item).fetch_all(&db.pool).await?;
            anyhow::ensure!(ids.len() <= 1, "worker fixture has duplicate viewers");
            if let Some(id) = ids.first() && let Some(local) = f.base.state.vod_commands.get(*id)
                && let Ok(output) = local.current() && output.published.load(Ordering::Acquire)
                && let Some(generation) = output.generation {
                break Ok::<_,anyhow::Error>((*id, output, generation));
            }
            tokio::select! { _ = stopped.cancelled() => anyhow::bail!("Android ended before worker readiness"), _ = tokio::time::sleep(Duration::from_millis(100)) => {} }
        }
    }).await??;
    let manager = &f.base.state.transcode_manager;
    let session = manager
        .get_session(&generation)
        .ok_or_else(|| anyhow::anyhow!("exact paced session missing"))?;
    let encoder = manager.qualification_encoder(generation)?;
    anyhow::ensure!(encoder.source == f.source && encoder.manifest == session.manifest_path);
    let probe = FrozenEncoder::stop(encoder.clone())?;
    anyhow::ensure!(probe.thread_count() > 0);
    drop(probe);
    let frozen = FrozenEncoder::stop(encoder.clone())?;
    let since = tokio::time::Instant::now();
    let mut checkpoint = None;
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            anyhow::ensure!(!stopped.is_cancelled(), "Android ended while its worker was frozen");
            if checkpoint.is_none() && !encoder.gone()? {
                let sample: (Option<Uuid>, i64, Option<i32>) = sqlx::query_as("SELECT checkpoint_output,checkpoint_sequence,checkpoint_position FROM playback_vod_controls WHERE reservation_id=$1").bind(viewer).fetch_one(&db.pool).await?;
                if sample.0 == Some(output.id) && sample.1 > 0 && sample.2.is_some_and(|position| position >= 500) && !encoder.gone()? { checkpoint = Some((sample.1, sample.2.unwrap())); }
            }
            if output.cleaned.load(Ordering::Acquire) { break Ok::<_, anyhow::Error>(()); }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await??;
    anyhow::ensure!(encoder.gone()?, "watchdog did not confirm physical exit");
    anyhow::ensure!(
        *output.failure.lock().unwrap() == Some(FailureClass::Stalled),
        "real worker failure was not eligible Stalled evidence"
    );
    let (sequence, position) = checkpoint.ok_or_else(|| {
        anyhow::anyhow!("native checkpoint was not confirmed before worker retirement")
    })?;
    println!(
        "VOD_ANDROID_WORKER_PROOF {}",
        serde_json::json!({"viewer":viewer,"output":output.id,"generation":generation,"pid":encoder.pid,"creation_time":encoder.created,"threads":frozen.thread_count(),"checkpoint_sequence":sequence,"checkpoint_position":position,"elapsed_ms":since.elapsed().as_millis(),"failure":"stalled","physical_exit":true})
    );
    drop(frozen);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires managed disposable PostgreSQL18 and actual paced Windows FFmpeg"]
async fn windows_owned_encoder_stall() -> anyhow::Result<()> {
    super::database::run(async |db| {
        let mut f = Fixture::browser_paced(db).await?;
        let result = async {
            super::reconfiguration::harness::probe_worker(db, &f).await?;
            let id = Uuid::now_v7();
            f.start(id, true, true).await?;
            f.ready(id).await?;
            let local = f.base.state.vod_commands.get(id).ok_or_else(|| anyhow::anyhow!("guard viewer missing"))?;
            let output = local.current()?;
            let encoder = f.base.state.transcode_manager.qualification_encoder(output.generation.unwrap())?;
            let first = FrozenEncoder::stop(encoder.clone())?;
            let count = first.thread_count();
            drop(first);
            let frozen = FrozenEncoder::stop(encoder.clone())?;
            tokio::time::timeout(Duration::from_secs(45), async {
                while !output.cleaned.load(Ordering::Acquire) { tokio::time::sleep(Duration::from_millis(100)).await; }
            }).await?;
            anyhow::ensure!(encoder.gone()? && *output.failure.lock().unwrap() == Some(FailureClass::Stalled), "guard did not produce actual watchdog exit/cleanup");
            println!("VOD_WINDOWS_GUARD_PROOF {}", serde_json::json!({"pid":encoder.pid,"creation_time":encoder.created,"initial_threads":count,"stalled_threads":frozen.thread_count(),"physical_exit":true,"failure":"stalled"}));
            drop(frozen);
            f.stop(id).await?;
            Ok::<_,anyhow::Error>(())
        }.await;
        let cleanup = f.close().await;
        result?;
        cleanup
    }).await
}
