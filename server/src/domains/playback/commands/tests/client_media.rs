// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database::Database, support::Fixture};
use uuid::Uuid;

pub(super) async fn recovery(db: &Database, f: &Fixture) -> anyhow::Result<(Uuid, Uuid)> {
    install(db, f, "DUSKCUE_TEST_VOD_DAMAGED_COPY").await
}

pub(super) async fn clean(db: &Database, f: &Fixture) -> anyhow::Result<(Uuid, Uuid)> {
    install(db, f, "DUSKCUE_TEST_CONFIGURATION_COPY").await
}

pub(super) async fn subtitled(db: &Database, f: &Fixture) -> anyhow::Result<(Uuid, Uuid)> {
    install(db, f, "DUSKCUE_TEST_CONFIGURATION_SOURCE").await
}

async fn install(db: &Database, f: &Fixture, input: &str) -> anyhow::Result<(Uuid, Uuid)> {
    let item = Uuid::now_v7();
    let file = Uuid::now_v7();
    let source = f.base.state.bootstrap.data_dir.join(format!(
        "fixture-{file}.{}",
        if input == "DUSKCUE_TEST_CONFIGURATION_SOURCE" {
            "mkv"
        } else {
            "mp4"
        }
    ));
    tokio::fs::copy(std::env::var(input)?, &source).await?;
    let metadata = tokio::fs::metadata(&source).await?;
    let modified: chrono::DateTime<chrono::Utc> = metadata.modified()?.into();
    let probe = crate::workers::library_scanner::probe_qualification_source(&source).await?;
    sqlx::query("INSERT INTO media_items(id,library_id,type,title,sort_title,runtime_seconds) SELECT $1,library_id,'movie','Decoder recovery qualification','Decoder recovery qualification',8 FROM media_items WHERE id=$2")
        .bind(item).bind(f.item).execute(&db.pool).await?;
    sqlx::query("INSERT INTO media_files(id,media_item_id,file_path,file_size,file_modified_at,container_format,video_codec,video_resolution,video_bitrate,video_dynamic_range,audio_codec,audio_channels,audio_bitrate,runtime_seconds,additional_streams) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)")
        .bind(file).bind(item).bind(source.to_string_lossy().as_ref()).bind(i64::try_from(metadata.len())?).bind(modified)
        .bind(probe.container_format).bind(probe.video_codec).bind(probe.video_resolution).bind(probe.video_bitrate).bind(probe.video_dynamic_range)
        .bind(probe.audio_codec).bind(probe.audio_channels).bind(probe.audio_bitrate).bind(probe.runtime_seconds).bind(probe.additional_streams).execute(&db.pool).await?;
    Ok((item, file))
}
