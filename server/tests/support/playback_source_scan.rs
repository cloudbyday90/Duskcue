// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Inventory, ProgressiveFixture, inventory};

#[derive(PartialEq)]
struct File {
    id: Uuid,
    stable: Value,
    updated_at: DateTime<Utc>,
    last_scanned_at: DateTime<Utc>,
}

async fn files(fixture: &ProgressiveFixture<'_>, library: Uuid) -> anyhow::Result<Vec<File>> {
    let rows: Vec<(Uuid, Value, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT f.id,jsonb_build_object('item',f.media_item_id,'file',f.id,'path',f.file_path,'healthy',f.is_healthy,\
         'size',f.file_size,'hash',f.file_hash,'modified_at',f.file_modified_at,'container',f.container_format,\
         'video_codec',f.video_codec,'video_resolution',f.video_resolution,'video_bitrate',f.video_bitrate,\
         'video_dynamic_range',f.video_dynamic_range,'video_frame_rate',f.video_frame_rate,\
         'audio_codec',f.audio_codec,'audio_channels',f.audio_channels,'audio_language',f.audio_language,\
         'audio_bitrate',f.audio_bitrate,'runtime',f.runtime_seconds,'streams',f.additional_streams),\
         f.updated_at,f.last_scanned_at FROM media_files f JOIN media_items i ON i.id=f.media_item_id \
         WHERE i.library_id=$1 ORDER BY f.file_path LIMIT 15",
    ).bind(library).fetch_all(fixture.pool).await?;
    Ok(rows
        .into_iter()
        .map(|(id, stable, updated_at, last_scanned_at)| File {
            id,
            stable,
            updated_at,
            last_scanned_at,
        })
        .collect())
}

fn counts(scan: &duskcue::workers::library_scanner::ScanResult) -> Value {
    json!({"type":scan.scan_type,"discovered":scan.files_discovered,"new":scan.files_new,"modified":scan.files_modified,
        "unchanged":scan.files_unchanged,"deleted":scan.files_deleted,"updated":scan.items_created,"errors":scan.errors.len()})
}

pub(super) async fn run(
    inventory: &Inventory,
    fixture: &ProgressiveFixture<'_>,
) -> anyhow::Result<Value> {
    anyhow::ensure!(super::source_enabled() && inventory.cases.len() == 14);
    let before = files(fixture, inventory.library).await?;
    anyhow::ensure!(before.len() == 14 && before.iter().all(|file| file.stable["healthy"] == true));
    let untouched = duskcue::workers::library_scanner::scan_library(
        fixture.pool,
        inventory.library,
        false,
        None,
    )
    .await?;
    let unchanged = files(fixture, inventory.library).await?;
    let first = counts(&untouched);
    eprintln!("DUSKCUE_SOURCE_QUICK_SCAN={first}");
    anyhow::ensure!(
        untouched.scan_type == "quick"
            && untouched.errors.is_empty()
            && untouched.files_discovered == 14
            && untouched.files_new == 0
            && untouched.files_modified == 0
            && untouched.files_unchanged == 14
            && untouched.files_deleted == 0
            && untouched.items_created == 0
            && before == unchanged,
        "unchanged actual quick scan touched or re-probed its owned file inventory"
    );

    let target = &inventory.cases[0];
    let original_modified: DateTime<Utc> = sqlx::query_scalar(
        "SELECT file_modified_at FROM media_files WHERE id=$1 AND media_item_id=$2",
    )
    .bind(target.file)
    .bind(target.item)
    .fetch_one(fixture.pool)
    .await?;
    let source_hash = inventory::hash(&target.path).await?;
    anyhow::ensure!(source_hash == target.source["sha256"]);
    let mut mutation = fixture.pool.begin().await?;
    let changed = sqlx::query("UPDATE media_files f SET file_modified_at=f.file_modified_at-interval '1 millisecond' FROM media_items i \
        WHERE f.id=$1 AND f.media_item_id=$2 AND i.id=f.media_item_id AND i.library_id=$3")
        .bind(target.file).bind(target.item).bind(inventory.library).execute(&mut *mutation).await?;
    anyhow::ensure!(changed.rows_affected() == 1);
    mutation.commit().await?;
    let checked = async {
        let scanned = duskcue::workers::library_scanner::scan_library(fixture.pool, inventory.library, false, None).await?;
        let after = files(fixture, inventory.library).await?;
        let second = counts(&scanned);
        eprintln!("DUSKCUE_SOURCE_QUICK_SCAN={second}");
        anyhow::ensure!(scanned.scan_type == "quick" && scanned.errors.is_empty() && scanned.files_discovered == 14
            && scanned.files_new == 0 && scanned.files_modified == 1 && scanned.files_unchanged == 13
            && scanned.files_deleted == 0 && scanned.items_created == 1 && after.len() == 14,
            "modified actual quick scan did not re-probe/update exactly one source");
        let mut updates = BTreeSet::new();
        for (old, new) in before.iter().zip(&after) {
            anyhow::ensure!(old.id == new.id && old.stable == new.stable,
                "quick re-probe changed file/item/hash/configuration metadata or failed to restore actual source mtime");
            if old.updated_at != new.updated_at || old.last_scanned_at != new.last_scanned_at { updates.insert(new.id); }
        }
        anyhow::ensure!(updates == BTreeSet::from([target.file]), "quick scan did not update exactly its stale source row");
        for case in &inventory.cases { anyhow::ensure!(inventory::hash(&case.path).await? == case.source["sha256"]); }
        Ok::<_, anyhow::Error>(json!({"passed":true,"actual_scanner":true,"unchanged":first,"stale_mtime":second,
            "stale_mtime_ms":1,"unchanged_files_skipped":14,"reprobed_files":1,"updated_files":1,
            "all_inventory_identities_preserved":true,"descriptor_and_hash_preserved":true,"actual_source_mtime_restored":true,
            "source_sha256_unchanged":true,"sources_unchanged":true}))
    }.await;
    if checked.is_err() {
        sqlx::query("UPDATE media_files SET file_modified_at=$2 WHERE id=$1 AND media_item_id=$3")
            .bind(target.file)
            .bind(original_modified)
            .bind(target.item)
            .execute(fixture.pool)
            .await?;
    }
    checked
}
