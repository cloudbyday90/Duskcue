// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    assignment,
    http::Case,
    import_fixture as fixture,
    peer::{Behavior, Peer, Security},
};
use axum::http::StatusCode;
use duskcue::domains::live_tv::feed::imports::ImportError;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use uuid::Uuid;

async fn wait(mut predicate: impl AsyncFnMut() -> anyhow::Result<bool>) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if predicate().await? {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    Ok(())
}

pub async fn run(case: &mut Case, url: &str) -> anyhow::Result<()> {
    atomic_and_cancel(case, url).await?;
    latest_attempt(case).await?;
    source_revocation(case).await?;
    worker_cancel(case).await?;
    lost_commit_ack(case, url).await?;
    let assignment: Uuid =
        sqlx::query_scalar("SELECT id FROM live_tv_assignments WHERE household_user_id=$1")
            .bind(case.owner.user)
            .fetch_one(&case.pool)
            .await?;
    sqlx::query("DELETE FROM live_tv_assignments WHERE id=$1")
        .bind(assignment)
        .execute(&case.pool)
        .await?;
    anyhow::ensure!(
        fixture::count(case, "snapshots").await? == 0
            && fixture::count(case, "channels").await? == 0
            && fixture::count(case, "guides").await? == 0
            && fixture::count(case, "programmes").await? == 0,
        "assignment removal did not cascade catalog"
    );
    println!("IPTV_IMPORT_STAGE lifecycle_and_generation_passed");
    Ok(())
}

async fn lost_commit_ack(case: &mut Case, url: &str) -> anyhow::Result<()> {
    for (before_commit, claim_ack) in [(false, false), (true, false), (false, true)] {
        let prior = fixture::status(case).await?.snapshot_id;
        let proxy = super::commit_proxy::Proxy::start(url, before_commit, claim_ack).await?;
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(&proxy.url)
            .await?;
        let (a, b) = fixture::peers(&fixture::playlist(), &fixture::guide()).await?;
        fixture::prepare(case, &a, &b).await?;
        let owner = duskcue::domains::live_tv::feed::imports::ImportOwner::qualification_fixture(
            pool.clone(),
            case.state.encryption_key.clone(),
        )?;
        let result = owner.start(case.owner.user, 0, 0)?.finish().await;
        anyhow::ensure!(
            proxy.dropped() == 1,
            "commit fault was not injected exactly once"
        );
        let status = fixture::status(case).await?;
        if before_commit {
            anyhow::ensure!(
                result == Err(ImportError::Indeterminate)
                    && status.state == "indeterminate"
                    && status.snapshot_id == prior,
                "absent receipt was incorrectly classified as ordinary failure or publication"
            );
        } else {
            anyhow::ensure!(
                status.snapshot_id == Some(result?) && status.state == "succeeded",
                "lost COMMIT acknowledgment was not reconciled against actual committed data"
            );
        }
        owner.shutdown().await?;
        pool.close().await;
        a.finish(true).await?;
        b.finish(true).await?;
        proxy.finish().await?;
    }
    println!("IPTV_IMPORT_STAGE lost_commit_ack_reconciled");
    Ok(())
}

async fn atomic_and_cancel(case: &mut Case, url: &str) -> anyhow::Result<()> {
    let old = fixture::status(case).await?.snapshot_id;
    let lock_pool = PgPoolOptions::new().max_connections(1).connect(url).await?;
    let mut lock = lock_pool.acquire().await?;
    sqlx::query("SELECT pg_advisory_lock(873201)")
        .execute(&mut *lock)
        .await?;
    sqlx::query("CREATE FUNCTION import_fixture_wait() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(873201); RETURN NEW; END $$").execute(&case.pool).await?;
    sqlx::query("CREATE TRIGGER import_fixture_wait BEFORE INSERT ON live_tv_catalog_programmes FOR EACH ROW EXECUTE FUNCTION import_fixture_wait()").execute(&case.pool).await?;
    let (a, b) = fixture::peers(&fixture::playlist(), &fixture::guide()).await?;
    fixture::prepare(case, &a, &b).await?;
    let owner = fixture::owner(case)?;
    let import = owner.start(case.owner.user, 0, 0)?;
    wait(async || {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND NOT granted)",
        )
        .fetch_one(&case.pool)
        .await?)
    })
    .await?;
    anyhow::ensure!(
        fixture::status(case).await?.snapshot_id == old
            && fixture::count(case, "snapshots").await? == 1
            && fixture::count(case, "channels").await? == 2,
        "uncommitted candidate became visible"
    );
    anyhow::ensure!(
        matches!(
            owner.start(case.owner.user, 0, 0),
            Err(ImportError::Capacity)
        ),
        "publication did not retain admission"
    );
    sqlx::query("SELECT pg_advisory_unlock(873201)")
        .execute(&mut *lock)
        .await?;
    let published = import.finish().await?;
    anyhow::ensure!(
        Some(published) != old
            && fixture::status(case).await?.snapshot_id == Some(published)
            && fixture::count(case, "snapshots").await? == 1,
        "atomic replacement failed"
    );
    a.finish(true).await?;
    b.finish(true).await?;
    owner.shutdown().await?;
    sqlx::query("SELECT pg_advisory_lock(873201)")
        .execute(&mut *lock)
        .await?;
    let (a, b) = fixture::peers(&fixture::playlist(), &fixture::guide()).await?;
    fixture::prepare(case, &a, &b).await?;
    let owner = fixture::owner(case)?;
    let import = owner.start(case.owner.user, 0, 0)?;
    wait(async || {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND NOT granted)",
        )
        .fetch_one(&case.pool)
        .await?)
    })
    .await?;
    import.cancel();
    tokio::time::sleep(Duration::from_millis(50)).await;
    anyhow::ensure!(
        !owner.idle()
            && matches!(
                owner.start(case.owner.user, 0, 0),
                Err(ImportError::Capacity)
            ),
        "cancelled database work released admission before rollback"
    );
    sqlx::query("SELECT pg_advisory_unlock(873201)")
        .execute(&mut *lock)
        .await?;
    anyhow::ensure!(
        import.finish().await == Err(ImportError::Cancelled)
            && fixture::status(case).await?.snapshot_id == Some(published)
            && fixture::count(case, "snapshots").await? == 1,
        "cancelled publication was committed or lost last-good data"
    );
    owner.shutdown().await?;
    a.finish(true).await?;
    b.finish(true).await?;
    sqlx::query("DROP TRIGGER import_fixture_wait ON live_tv_catalog_programmes")
        .execute(&case.pool)
        .await?;
    sqlx::query("DROP FUNCTION import_fixture_wait()")
        .execute(&case.pool)
        .await?;
    drop(lock);
    lock_pool.close().await;
    let (a, b) = fixture::peers(&fixture::playlist(), &fixture::guide()).await?;
    fixture::prepare(case, &a, &b).await?;
    let owner = fixture::owner(case)?;
    let import = owner.start(case.owner.user, 0, 0)?;
    import.cancel();
    anyhow::ensure!(
        matches!(import.finish().await, Err(ImportError::Cancelled)),
        "early cancellation was ignored"
    );
    owner.shutdown().await?;
    a.finish(false).await?;
    b.finish(false).await?;
    println!("IPTV_IMPORT_STAGE atomic_visibility_passed");
    Ok(())
}

async fn latest_attempt(case: &mut Case) -> anyhow::Result<()> {
    let a = Peer::new("127.0.0.1".parse()?, Security::Plain, Behavior::BodyStall).await?;
    let (_, b) = fixture::peers(&fixture::playlist(), &fixture::guide()).await?;
    fixture::prepare(case, &a, &b).await?;
    let one = fixture::owner(case)?;
    let two = fixture::owner(case)?;
    let first = one.start(case.owner.user, 0, 0)?;
    a.wait_phase(3).await?;
    let first_attempt = fixture::status(case).await?.attempt_id;
    let second = two.start(case.owner.user, 0, 0)?;
    wait(async || Ok(fixture::status(case).await?.attempt_id != first_attempt)).await?;
    let latest = fixture::status(case).await?.attempt_id;
    second.cancel();
    anyhow::ensure!(
        first.finish().await == Err(ImportError::Authority),
        "older attempt was not fenced"
    );
    let category = match second.finish().await {
        Err(ImportError::Cancelled) => "cancelled",
        Err(ImportError::Network) => "network",
        _ => anyhow::bail!("newer attempt had an unexpected terminal result"),
    };
    anyhow::ensure!(
        fixture::status(case).await?.attempt_id == latest
            && fixture::status(case).await?.category.as_deref() == Some(category),
        "older status overwrote newer attempt"
    );
    one.shutdown().await?;
    two.shutdown().await?;
    a.finish(true).await?;
    b.finish(false).await?;
    Ok(())
}

async fn source_revocation(case: &mut Case) -> anyhow::Result<()> {
    let a = Peer::new("127.0.0.1".parse()?, Security::Plain, Behavior::BodyStall).await?;
    let (_, b) = fixture::peers(&fixture::playlist(), &fixture::guide()).await?;
    fixture::prepare(case, &a, &b).await?;
    let owner = fixture::owner(case)?;
    let import = owner.start(case.owner.user, 0, 0)?;
    a.wait_phase(3).await?;
    assignment::patch(
        case,
        json!({"operation":"revoke_destination","feed":"playlist"}),
    )
    .await?
    .expect(StatusCode::OK)?;
    anyhow::ensure!(
        import.finish().await == Err(ImportError::Authority),
        "revoked operation continued"
    );
    let status = fixture::status(case).await?;
    anyhow::ensure!(
        status.snapshot_id.is_some() && !status.current_source,
        "revocation removed or revived old snapshot"
    );
    anyhow::ensure!(
        owner.start(case.owner.user, 0, 0)?.finish().await == Err(ImportError::Authority),
        "revoked authority admitted networking"
    );
    owner.shutdown().await?;
    a.finish(true).await?;
    b.finish(false).await?;
    anyhow::ensure!(
        matches!(
            owner.start(case.owner.user, 0, 0),
            Err(ImportError::Cancelled)
        ),
        "closed owner admitted work"
    );
    Ok(())
}

async fn worker_cancel(case: &mut Case) -> anyhow::Result<()> {
    let mut guide = String::from("<tv>");
    for i in 0..150_000 {
        guide.push_str(&format!(
            "<programme channel='shared-guide' start='2026'><title>Scale {i}</title></programme>"
        ));
    }
    guide.push_str("</tv>");
    let (a, b) = fixture::peers(&fixture::playlist(), guide.as_bytes()).await?;
    fixture::prepare(case, &a, &b).await?;
    let owner = fixture::owner(case)?;
    let import = owner.start(case.owner.user, 0, 0)?;
    b.wait_phase(4).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    anyhow::ensure!(
        !owner.idle(),
        "scale worker unexpectedly finished before cancellation"
    );
    import.cancel();
    anyhow::ensure!(
        import.finish().await == Err(ImportError::Cancelled),
        "scale worker ignored cooperative cancellation"
    );
    anyhow::ensure!(owner.idle(), "worker did not join before admission release");
    owner.shutdown().await?;
    a.finish(true).await?;
    b.finish(true).await?;
    let (a, b) = fixture::peers(&fixture::playlist(), &fixture::guide()).await?;
    fixture::prepare(case, &a, &b).await?;
    let owner = fixture::owner(case)?;
    let abandoned = owner.start(case.owner.user, 0, 0)?;
    drop(abandoned);
    owner.shutdown().await?;
    anyhow::ensure!(owner.idle(), "abandoned import leaked ownership");
    a.finish(false).await?;
    b.finish(false).await?;
    Ok(())
}
