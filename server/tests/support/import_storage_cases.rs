// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{http::Case, import_fixture as fixture};
use duskcue::domains::live_tv::feed::imports::{self, ImportError};
use sqlx::Row;
use uuid::Uuid;

pub async fn run(case: &mut Case) -> anyhow::Result<()> {
    let first = fixture::run(case, &fixture::playlist(), &fixture::guide()).await??;
    let status = fixture::status(case).await?;
    anyhow::ensure!(
        status.snapshot_id == Some(first)
            && status.current_source
            && status.channel_count == 2
            && status.guide_count == 1
            && status.programme_count == 2
            && status.state == "succeeded",
        "paired snapshot metadata incorrect"
    );
    let owner = fixture::owner(case)?;
    anyhow::ensure!(
        owner.start(case.owner.user, 0, 0)?.finish().await == Err(ImportError::Network),
        "closed fixture unexpectedly served a refresh"
    );
    let failed = fixture::status(case).await?;
    anyhow::ensure!(
        failed.snapshot_id == Some(first)
            && failed.current_source
            && failed.imported_at == status.imported_at
            && failed.state == "failed",
        "same-generation refresh failure invalidated last-good authority or timestamp"
    );
    owner.shutdown().await?;
    anyhow::ensure!(
        fixture::count(case, "snapshots").await? == 1
            && fixture::count(case, "channels").await? == 2
            && fixture::count(case, "guides").await? == 1
            && fixture::count(case, "programmes").await? == 2,
        "snapshot counts inconsistent"
    );
    let keys: Vec<Vec<u8>> =
        sqlx::query_scalar("SELECT channel_key FROM live_tv_catalog_channels ORDER BY channel_key")
            .fetch_all(&case.pool)
            .await?;
    let metadata: Vec<String> = sqlx::query_scalar("SELECT metadata FROM live_tv_catalog_channels")
        .fetch_all(&case.pool)
        .await?;
    let streams: Vec<String> = sqlx::query_scalar("SELECT stream FROM live_tv_catalog_channels")
        .fetch_all(&case.pool)
        .await?;
    anyhow::ensure!(
        metadata
            .iter()
            .chain(streams.iter())
            .all(|v| v.starts_with("duskcue-secret:v1:") && !v.contains("private")),
        "derived credentials were plaintext"
    );
    let rotated = String::from_utf8(fixture::playlist())?
        .replace("token=stream-private", "token=rotated-private");
    let second =
        fixture::run(case, rotated.as_bytes(), &fixture::gzip(&fixture::guide())?).await??;
    anyhow::ensure!(
        second != first && fixture::count(case, "snapshots").await? == 1,
        "last-good history grew"
    );
    let rotated_keys: Vec<Vec<u8>> =
        sqlx::query_scalar("SELECT channel_key FROM live_tv_catalog_channels ORDER BY channel_key")
            .fetch_all(&case.pool)
            .await?;
    anyhow::ensure!(
        keys == rotated_keys,
        "credential rotation changed channel identity"
    );
    for bad in [
        b"<tv>".as_slice(),
        b"<!DOCTYPE tv SYSTEM 'file:///private'><tv/>".as_slice(),
        b"<html>error</html>".as_slice(),
    ] {
        anyhow::ensure!(
            fixture::run(case, &fixture::playlist(), bad).await? == Err(ImportError::Format),
            "malformed refresh succeeded"
        );
        let status = fixture::status(case).await?;
        anyhow::ensure!(
            status.snapshot_id == Some(second)
                && !status.current_source
                && status.state == "failed"
                && status.category.as_deref() == Some("format"),
            "failed refresh lost or revived prior generation"
        );
    }
    let good = fixture::run(case, &fixture::playlist(), &fixture::guide()).await??;
    let original: i64 =
        sqlx::query_scalar("SELECT source_generation FROM live_tv_catalog_snapshots WHERE id=$1")
            .bind(good)
            .fetch_one(&case.pool)
            .await?;
    sqlx::query(
        "UPDATE live_tv_catalog_snapshots SET source_generation=source_generation+1 WHERE id=$1",
    )
    .bind(good)
    .execute(&case.pool)
    .await?;
    anyhow::ensure!(
        matches!(
            imports::status(&case.pool, &case.state.encryption_key, case.owner.user).await,
            Err(ImportError::Storage)
        ),
        "manifest accepted relabelled source epoch"
    );
    sqlx::query("UPDATE live_tv_catalog_snapshots SET source_generation=$2 WHERE id=$1")
        .bind(good)
        .bind(original)
        .execute(&case.pool)
        .await?;
    sqlx::query("CREATE FUNCTION import_fixture_refuse() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture refusal'; END $$").execute(&case.pool).await?;
    sqlx::query("CREATE TRIGGER import_fixture_refuse BEFORE INSERT ON live_tv_catalog_programmes FOR EACH ROW EXECUTE FUNCTION import_fixture_refuse()").execute(&case.pool).await?;
    anyhow::ensure!(
        fixture::run(case, &fixture::playlist(), &fixture::guide()).await?
            == Err(ImportError::Storage),
        "publication failure was not refused"
    );
    anyhow::ensure!(
        fixture::status(case).await?.snapshot_id == Some(good)
            && fixture::count(case, "snapshots").await? == 1
            && fixture::count(case, "channels").await? == 2,
        "failed publication left partial data"
    );
    sqlx::query("DROP TRIGGER import_fixture_refuse ON live_tv_catalog_programmes")
        .execute(&case.pool)
        .await?;
    sqlx::query("DROP FUNCTION import_fixture_refuse()")
        .execute(&case.pool)
        .await?;
    let row=sqlx::query("SELECT id,household_user_id,assignment_id,snapshot_id,metadata FROM live_tv_catalog_guides LIMIT 1").fetch_one(&case.pool).await?;
    let wrong=sqlx::query("INSERT INTO live_tv_catalog_channels(id,household_user_id,assignment_id,snapshot_id,channel_key,guide_id,metadata,stream) VALUES ($1,$2,$3,$4,$5,$6,$7,$7)")
        .bind(Uuid::now_v7()).bind(row.get::<Uuid,_>("household_user_id")).bind(row.get::<Uuid,_>("assignment_id")).bind(row.get::<Uuid,_>("snapshot_id")).bind(vec![0u8;32]).bind(Uuid::now_v7()).bind(row.get::<String,_>("metadata")).execute(&case.pool).await;
    anyhow::ensure!(wrong.is_err(), "cross-snapshot guide reference accepted");
    let empty = fixture::run(case, b"#EXTM3U\n", b"<tv/>").await??;
    let status = fixture::status(case).await?;
    anyhow::ensure!(
        status.snapshot_id == Some(empty)
            && status.current_source
            && status.channel_count == 0
            && fixture::count(case, "channels").await? == 0
            && fixture::count(case, "programmes").await? == 0,
        "valid empty lineup did not remove channels"
    );
    let restored = fixture::run(case, &fixture::playlist(), &fixture::guide()).await??;
    let restored_keys: Vec<Vec<u8>> =
        sqlx::query_scalar("SELECT channel_key FROM live_tv_catalog_channels ORDER BY channel_key")
            .fetch_all(&case.pool)
            .await?;
    anyhow::ensure!(
        restored != empty && restored_keys == keys,
        "remove/readd changed channel identity"
    );
    println!("IPTV_IMPORT_STAGE encrypted_atomic_last_good_passed");
    Ok(())
}
