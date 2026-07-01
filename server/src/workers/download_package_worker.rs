// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use chrono::{DateTime, Utc};
use ring::digest::{Context as DigestContext, SHA256};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use tokio::process::Command;
use uuid::Uuid;

use crate::domains::downloads::service::{
    DownloadJobStatusEventPayload, dispatch_download_job_notification,
    publish_download_job_status_event,
};
use crate::state::AppState;

const DOWNLOADS_SUBDIR: &str = "downloads";
const DEFAULT_MAX_JOBS_PER_RUN: i64 = 1;
const DEFAULT_MAX_RETRIES: i32 = 2;
const DEFAULT_STALE_PREPARING_MINUTES: i64 = 120;
const DEFAULT_FAILED_CLEANUP_HOURS: i64 = 24;

#[derive(Debug, Clone, Deserialize)]
struct WorkerConfig {
    #[serde(default = "default_max_jobs_per_run")]
    max_jobs_per_run: i64,
    #[serde(default = "default_max_retries")]
    max_retries: i32,
    #[serde(default = "default_stale_preparing_minutes")]
    stale_preparing_minutes: i64,
    #[serde(default = "default_failed_cleanup_hours")]
    failed_cleanup_hours: i64,
}

#[derive(Debug, Clone)]
struct DownloadJobWork {
    id: Uuid,
    user_id: Uuid,
    user_session_id: Option<Uuid>,
    device_identifier: String,
    library_id: Uuid,
    media_item_id: Uuid,
    media_file_id: Uuid,
    package_format: String,
    package_strategy: String,
    quality_mode: String,
    quality_label: Option<String>,
    selected_audio: Value,
    selected_subtitles: Value,
    selected_artwork: Value,
    bytes_expected: Option<i64>,
    access_policy_snapshot: Value,
    retry_count: i32,
    expires_at: Option<DateTime<Utc>>,
    metadata: Value,
    source_path: PathBuf,
    source_file_hash: Option<String>,
    source_modified_at: Option<DateTime<Utc>>,
    source_container: String,
    source_video_codec: Option<String>,
    source_video_resolution: Option<String>,
    source_video_bitrate: Option<i32>,
    source_audio_codec: Option<String>,
    source_audio_channels: Option<i32>,
    source_audio_language: Option<String>,
    runtime_seconds: i32,
}

#[derive(Debug, Clone)]
struct PackageFile {
    relative_path: String,
    file_role: String,
    content_type: Option<String>,
    byte_size: i64,
    checksum_sha256: String,
    segment_index: Option<i32>,
    track_type: Option<String>,
    is_required: bool,
}

#[derive(Debug, Default)]
struct WorkerStats {
    claimed: u64,
    ready: u64,
    failed: u64,
    retried: u64,
    cancelled: u64,
    cleaned: u64,
}

#[derive(Debug, Clone, Copy)]
struct FailureOutcome {
    retried: bool,
    retry_count: i32,
    progress_percent: f32,
    bytes_prepared: i64,
}

pub async fn run_download_package_worker(
    state: &AppState,
    task_id: Uuid,
    config: serde_json::Value,
) -> anyhow::Result<()> {
    let config: WorkerConfig = serde_json::from_value(config).unwrap_or_else(|_| WorkerConfig {
        max_jobs_per_run: default_max_jobs_per_run(),
        max_retries: default_max_retries(),
        stale_preparing_minutes: default_stale_preparing_minutes(),
        failed_cleanup_hours: default_failed_cleanup_hours(),
    });
    let package_root = state.bootstrap.data_dir.join(DOWNLOADS_SUBDIR);
    tokio::fs::create_dir_all(&package_root)
        .await
        .with_context(|| format!("failed to create {}", package_root.display()))?;

    recover_stale_preparing(&state.pool, config.stale_preparing_minutes).await?;

    let mut stats = WorkerStats::default();
    for _ in 0..config.max_jobs_per_run.max(1) {
        let Some(job) = claim_next_job(&state.pool).await? else {
            break;
        };
        stats.claimed += 1;
        metrics::counter!("download_jobs_started_total").increment(1);
        record_event(
            &state.pool,
            Some(job.user_id),
            job.user_session_id,
            Some(job.id),
            None,
            Some(job.media_item_id),
            Some(&job.device_identifier),
            "job_started",
            None,
            json!({ "task_id": task_id }),
        )
        .await?;
        publish_worker_status_event(
            state,
            &job,
            "preparing",
            5.0,
            0,
            None,
            None,
            None,
            Some("download package preparation claimed"),
        );

        match process_job(state, &package_root, &job).await {
            Ok(()) => {
                stats.ready += 1;
                metrics::counter!("download_jobs_ready_total").increment(1);
            }
            Err(err) => {
                let failure = truncate_reason(&err.to_string());
                let failure_outcome = fail_or_retry_job(
                    &state.pool,
                    &job,
                    &failure,
                    config.max_retries,
                    config.failed_cleanup_hours,
                )
                .await?;
                publish_worker_status_event(
                    state,
                    &job,
                    if failure_outcome.retried {
                        "queued"
                    } else {
                        "failed"
                    },
                    failure_outcome.progress_percent,
                    failure_outcome.bytes_prepared,
                    None,
                    Some(&failure),
                    Some(failure_outcome.retry_count),
                    Some(if failure_outcome.retried {
                        "download package preparation will retry"
                    } else {
                        "download package preparation failed"
                    }),
                );
                if failure_outcome.retried {
                    stats.retried += 1;
                    metrics::counter!("download_jobs_retried_total").increment(1);
                } else {
                    stats.failed += 1;
                    metrics::counter!("download_jobs_failed_total").increment(1);
                    dispatch_download_job_notification(
                        state,
                        job.user_id,
                        job.id,
                        job.media_item_id,
                        "failed",
                        Some(&failure),
                    )
                    .await;
                }
                let _ = tokio::fs::remove_dir_all(package_root.join(job.id.to_string())).await;
            }
        }
    }

    stats.cleaned += cleanup_due_packages(&state.pool, &package_root).await?;
    metrics::gauge!("download_worker_last_claimed_jobs").set(stats.claimed as f64);
    metrics::gauge!("download_worker_last_cleaned_packages").set(stats.cleaned as f64);

    tracing::info!(
        task_id = %task_id,
        claimed = stats.claimed,
        ready = stats.ready,
        failed = stats.failed,
        retried = stats.retried,
        cancelled = stats.cancelled,
        cleaned = stats.cleaned,
        "Download package worker completed"
    );

    Ok(())
}

async fn process_job(
    state: &AppState,
    package_root: &Path,
    job: &DownloadJobWork,
) -> anyhow::Result<()> {
    if is_cancelled(&state.pool, job.id).await? {
        mark_cancelled(&state.pool, job, "cancelled before package preparation").await?;
        publish_worker_status_event(
            state,
            job,
            "cancelled",
            5.0,
            0,
            None,
            Some("cancelled before package preparation"),
            None,
            Some("download job cancelled"),
        );
        return Ok(());
    }

    let package_dir = package_root.join(job.id.to_string());
    reset_package_dir(package_root, &package_dir).await?;
    update_progress(&state.pool, job.id, 10.0, 0).await?;
    publish_worker_status_event(
        state,
        job,
        "preparing",
        10.0,
        0,
        None,
        None,
        None,
        Some("download package preparation started"),
    );

    match job.package_strategy.as_str() {
        "direct_copy" => prepare_direct_copy(job, &package_dir).await?,
        "remux" => prepare_hls_package(state, job, &package_dir, false).await?,
        "transcode" => prepare_hls_package(state, job, &package_dir, true).await?,
        other => return Err(anyhow!("unsupported download package strategy: {other}")),
    }

    if is_cancelled(&state.pool, job.id).await? {
        mark_cancelled(&state.pool, job, "cancelled after package preparation").await?;
        publish_worker_status_event(
            state,
            job,
            "cancelled",
            85.0,
            package_size(&package_dir).await as i64,
            None,
            Some("cancelled after package preparation"),
            None,
            Some("download job cancelled"),
        );
        let _ = tokio::fs::remove_dir_all(&package_dir).await;
        return Ok(());
    }

    let prepared_bytes = package_size(&package_dir).await as i64;
    update_progress(&state.pool, job.id, 85.0, prepared_bytes).await?;
    publish_worker_status_event(
        state,
        job,
        "preparing",
        85.0,
        prepared_bytes,
        None,
        None,
        None,
        Some("download package staged"),
    );
    write_package_manifest(job, &package_dir).await?;
    let files = collect_package_files(&package_dir)?;
    let total_bytes: i64 = files.iter().map(|file| file.byte_size).sum();
    let package_hash = package_hash(&files);
    let manifest_hash = files
        .iter()
        .find(|file| file.relative_path == "manifest.json")
        .map(|file| file.checksum_sha256.clone());

    let package_id = persist_ready_package(
        &state.pool,
        job,
        &files,
        total_bytes,
        package_hash,
        manifest_hash,
    )
    .await?;

    record_event(
        &state.pool,
        Some(job.user_id),
        job.user_session_id,
        Some(job.id),
        None,
        Some(job.media_item_id),
        Some(&job.device_identifier),
        "job_ready",
        None,
        json!({
            "package_format": job.package_format,
            "package_strategy": job.package_strategy,
            "total_bytes": total_bytes,
            "file_count": files.len()
        }),
    )
    .await?;
    update_progress(&state.pool, job.id, 100.0, total_bytes).await?;
    publish_worker_status_event(
        state,
        job,
        "ready",
        100.0,
        total_bytes,
        Some(package_id),
        None,
        None,
        Some("download package ready"),
    );
    dispatch_download_job_notification(
        state,
        job.user_id,
        job.id,
        job.media_item_id,
        "ready",
        None,
    )
    .await;

    Ok(())
}

fn publish_worker_status_event(
    state: &AppState,
    job: &DownloadJobWork,
    status: &str,
    progress_percent: f32,
    bytes_prepared: i64,
    package_id: Option<Uuid>,
    failure_reason: Option<&str>,
    retry_count: Option<i32>,
    reason: Option<&str>,
) {
    publish_download_job_status_event(
        state,
        job.user_id,
        DownloadJobStatusEventPayload {
            job_id: job.id,
            package_id,
            media_item_id: job.media_item_id,
            media_file_id: Some(job.media_file_id),
            device_identifier: job.device_identifier.clone(),
            status: status.to_string(),
            progress_percent,
            bytes_expected: job.bytes_expected,
            bytes_prepared,
            failure_reason: failure_reason.map(str::to_string),
            retry_count,
            reason: reason.map(str::to_string),
            occurred_at: Utc::now(),
        },
    );
}

async fn prepare_direct_copy(job: &DownloadJobWork, package_dir: &Path) -> anyhow::Result<()> {
    let target = package_dir.join("media.mp4");
    tokio::fs::copy(&job.source_path, &target)
        .await
        .with_context(|| {
            format!(
                "failed to copy source media {} to {}",
                job.source_path.display(),
                target.display()
            )
        })?;
    Ok(())
}

async fn prepare_hls_package(
    state: &AppState,
    job: &DownloadJobWork,
    package_dir: &Path,
    transcode: bool,
) -> anyhow::Result<()> {
    let segment_seconds = state
        .runtime_config
        .load()
        .transcoding
        .segment_duration_seconds
        .max(2);
    let manifest_path = package_dir.join("stream.m3u8");
    let segment_path = package_dir.join("seg_%05d.m4s");
    let mut command = Command::new("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-y")
        .arg("-i")
        .arg(&job.source_path)
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("0:a:0?");

    if transcode {
        let target_resolution = job
            .metadata
            .get("target_resolution")
            .and_then(Value::as_str)
            .and_then(resolution_height);
        let bitrate = job
            .metadata
            .get("target_bitrate_bps")
            .and_then(Value::as_i64)
            .unwrap_or(6_000_000)
            .max(500_000);
        command
            .arg("-c:v")
            .arg("libx264")
            .arg("-preset")
            .arg("medium")
            .arg("-b:v")
            .arg(bitrate.to_string())
            .arg("-maxrate")
            .arg(bitrate.to_string())
            .arg("-bufsize")
            .arg((bitrate * 2).to_string())
            .arg("-g")
            .arg((segment_seconds * 24).to_string())
            .arg("-sc_threshold")
            .arg("0")
            .arg("-c:a")
            .arg("aac")
            .arg("-b:a")
            .arg("192k");
        if let Some(height) = target_resolution {
            command.arg("-vf").arg(format!("scale=-2:{height}"));
        }
    } else {
        command.arg("-c:v").arg("copy").arg("-c:a").arg("copy");
    }

    let status = command
        .arg("-f")
        .arg("hls")
        .arg("-hls_time")
        .arg(segment_seconds.to_string())
        .arg("-hls_segment_type")
        .arg("fmp4")
        .arg("-hls_playlist_type")
        .arg("vod")
        .arg("-hls_list_size")
        .arg("0")
        .arg("-hls_segment_filename")
        .arg(&segment_path)
        .arg(&manifest_path)
        .status()
        .await
        .context("failed to run ffmpeg for offline package")?;

    if !status.success() {
        return Err(anyhow!("ffmpeg exited with status {status}"));
    }

    Ok(())
}

async fn claim_next_job(pool: &PgPool) -> anyhow::Result<Option<DownloadJobWork>> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "UPDATE download_jobs \
         SET status = 'preparing', started_at = COALESCE(started_at, now()), \
             progress_percent = 5, updated_at = now() \
         WHERE id = ( \
             SELECT id FROM download_jobs \
             WHERE status = 'queued' AND cancellation_requested = false \
             ORDER BY created_at ASC \
             LIMIT 1 \
             FOR UPDATE SKIP LOCKED \
         ) \
         RETURNING id",
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };
    let job_id: Uuid = row.get("id");

    let row = sqlx::query(
        "SELECT dj.id, dj.user_id, dj.user_session_id, dj.device_identifier, dj.library_id, \
                dj.media_item_id, dj.media_file_id, dj.package_format, dj.package_strategy, \
                dj.quality_mode, dj.quality_label, dj.selected_audio, dj.selected_subtitles, \
                dj.selected_artwork, dj.bytes_expected, dj.access_policy_snapshot, dj.retry_count, \
                dj.expires_at, dj.metadata, mf.file_path, mf.file_hash, mf.file_modified_at, \
                mf.container_format, mf.video_codec, mf.video_resolution, mf.video_bitrate, \
                mf.audio_codec, mf.audio_channels, mf.audio_language, mf.runtime_seconds \
         FROM download_jobs dj \
         JOIN media_files mf ON mf.id = dj.media_file_id \
         WHERE dj.id = $1",
    )
    .bind(job_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Some(DownloadJobWork {
        id: row.get("id"),
        user_id: row.get("user_id"),
        user_session_id: row.try_get("user_session_id").ok().flatten(),
        device_identifier: row.get("device_identifier"),
        library_id: row.get("library_id"),
        media_item_id: row.get("media_item_id"),
        media_file_id: row.get("media_file_id"),
        package_format: row.get("package_format"),
        package_strategy: row.get("package_strategy"),
        quality_mode: row.get("quality_mode"),
        quality_label: row.try_get("quality_label").ok().flatten(),
        selected_audio: row.get("selected_audio"),
        selected_subtitles: row.get("selected_subtitles"),
        selected_artwork: row.get("selected_artwork"),
        bytes_expected: row.try_get("bytes_expected").ok().flatten(),
        access_policy_snapshot: row.get("access_policy_snapshot"),
        retry_count: row.get("retry_count"),
        expires_at: row.try_get("expires_at").ok().flatten(),
        metadata: row.get("metadata"),
        source_path: PathBuf::from(row.get::<String, _>("file_path")),
        source_file_hash: row.try_get("file_hash").ok().flatten(),
        source_modified_at: row.try_get("file_modified_at").ok().flatten(),
        source_container: row.get("container_format"),
        source_video_codec: row.try_get("video_codec").ok().flatten(),
        source_video_resolution: row.try_get("video_resolution").ok().flatten(),
        source_video_bitrate: row.try_get("video_bitrate").ok().flatten(),
        source_audio_codec: row.try_get("audio_codec").ok().flatten(),
        source_audio_channels: row.try_get("audio_channels").ok().flatten(),
        source_audio_language: row.try_get("audio_language").ok().flatten(),
        runtime_seconds: row.get("runtime_seconds"),
    }))
}

async fn persist_ready_package(
    pool: &PgPool,
    job: &DownloadJobWork,
    files: &[PackageFile],
    total_bytes: i64,
    package_hash: String,
    manifest_hash: Option<String>,
) -> anyhow::Result<Uuid> {
    let storage_key = format!("{DOWNLOADS_SUBDIR}/{}", job.id);
    let mut tx = pool.begin().await?;
    let package_row = sqlx::query(
        "INSERT INTO download_packages \
         (download_job_id, user_id, user_session_id, device_identifier, library_id, media_item_id, \
          media_file_id, status, package_format, manifest_version, manifest_relative_path, \
          storage_key, total_bytes, file_count, package_hash_sha256, manifest_hash_sha256, \
          selected_audio, selected_subtitles, included_artwork, included_storyboards, \
          sync_metadata, access_policy_snapshot, expires_at, cleanup_after_at, metadata) \
         VALUES \
         ($1, $2, $3, $4, $5, $6, $7, 'ready', $8, 1, 'manifest.json', $9, $10, $11, $12, $13, \
          $14, $15, $16, $17, '{}', $18, $19, $19 + INTERVAL '7 days', $20) \
         ON CONFLICT (download_job_id) DO UPDATE SET \
             status = 'ready', total_bytes = EXCLUDED.total_bytes, file_count = EXCLUDED.file_count, \
             package_hash_sha256 = EXCLUDED.package_hash_sha256, manifest_hash_sha256 = EXCLUDED.manifest_hash_sha256, \
             updated_at = now(), cleanup_after_at = EXCLUDED.cleanup_after_at, metadata = EXCLUDED.metadata \
         RETURNING id",
    )
    .bind(job.id)
    .bind(job.user_id)
    .bind(job.user_session_id)
    .bind(&job.device_identifier)
    .bind(job.library_id)
    .bind(job.media_item_id)
    .bind(job.media_file_id)
    .bind(&job.package_format)
    .bind(storage_key)
    .bind(total_bytes)
    .bind(files.len() as i32)
    .bind(package_hash)
    .bind(manifest_hash)
    .bind(&job.selected_audio)
    .bind(&job.selected_subtitles)
    .bind(&job.selected_artwork)
    .bind(
        job.metadata
            .get("included_storyboards")
            .cloned()
            .unwrap_or_else(|| json!({ "included": false })),
    )
    .bind(&job.access_policy_snapshot)
    .bind(job.expires_at)
    .bind(json!({
        "source_file_hash": job.source_file_hash,
        "source_modified_at": job.source_modified_at,
        "quality_mode": job.quality_mode,
        "quality_label": job.quality_label
    }))
    .fetch_one(&mut *tx)
    .await?;
    let package_id: Uuid = package_row.get("id");

    sqlx::query("DELETE FROM download_package_files WHERE download_package_id = $1")
        .bind(package_id)
        .execute(&mut *tx)
        .await?;

    for file in files {
        sqlx::query(
            "INSERT INTO download_package_files \
             (download_package_id, relative_path, file_role, content_type, byte_size, \
              checksum_sha256, segment_index, track_type, is_required, metadata) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, '{}')",
        )
        .bind(package_id)
        .bind(&file.relative_path)
        .bind(&file.file_role)
        .bind(&file.content_type)
        .bind(file.byte_size)
        .bind(&file.checksum_sha256)
        .bind(file.segment_index)
        .bind(&file.track_type)
        .bind(file.is_required)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "UPDATE download_jobs \
         SET status = 'ready', progress_percent = 100, bytes_prepared = $2, \
             completed_at = now(), updated_at = now() \
         WHERE id = $1",
    )
    .bind(job.id)
    .bind(total_bytes)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(package_id)
}

async fn fail_or_retry_job(
    pool: &PgPool,
    job: &DownloadJobWork,
    failure: &str,
    max_retries: i32,
    failed_cleanup_hours: i64,
) -> anyhow::Result<FailureOutcome> {
    let next_retry_count = job.retry_count + 1;
    let retry = next_retry_count <= max_retries;
    let status = if retry { "queued" } else { "failed" };
    let row = sqlx::query(
        "UPDATE download_jobs \
         SET status = $2, failure_reason = $3, retry_count = retry_count + 1, \
             progress_percent = CASE WHEN $2 = 'queued' THEN 0 ELSE progress_percent END, \
             completed_at = CASE WHEN $2 = 'failed' THEN now() ELSE completed_at END, \
             cleanup_after_at = CASE WHEN $2 = 'failed' THEN now() + ($4::TEXT || ' hours')::INTERVAL ELSE cleanup_after_at END, \
             updated_at = now() \
         WHERE id = $1 \
         RETURNING progress_percent::REAL AS progress_percent, bytes_prepared",
    )
    .bind(job.id)
    .bind(status)
    .bind(failure)
    .bind(failed_cleanup_hours)
    .fetch_one(pool)
    .await?;

    record_event(
        pool,
        Some(job.user_id),
        job.user_session_id,
        Some(job.id),
        None,
        Some(job.media_item_id),
        Some(&job.device_identifier),
        "job_failed",
        Some(failure),
        json!({ "retry": retry, "retry_count": next_retry_count }),
    )
    .await?;

    Ok(FailureOutcome {
        retried: retry,
        retry_count: next_retry_count,
        progress_percent: row.get("progress_percent"),
        bytes_prepared: row.get("bytes_prepared"),
    })
}

async fn mark_cancelled(pool: &PgPool, job: &DownloadJobWork, reason: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE download_jobs \
         SET status = 'cancelled', cancellation_requested = true, failure_reason = $2, \
             completed_at = now(), cleanup_after_at = now(), updated_at = now() \
         WHERE id = $1",
    )
    .bind(job.id)
    .bind(reason)
    .execute(pool)
    .await?;
    record_event(
        pool,
        Some(job.user_id),
        job.user_session_id,
        Some(job.id),
        None,
        Some(job.media_item_id),
        Some(&job.device_identifier),
        "job_cancelled",
        Some(reason),
        json!({}),
    )
    .await?;
    metrics::counter!("download_jobs_cancelled_total").increment(1);
    Ok(())
}

async fn recover_stale_preparing(pool: &PgPool, stale_minutes: i64) -> anyhow::Result<()> {
    let result = sqlx::query(
        "UPDATE download_jobs \
         SET status = 'queued', retry_count = retry_count + 1, \
             failure_reason = 'recovered stale preparing job after worker interruption', \
             updated_at = now() \
         WHERE status = 'preparing' \
           AND cancellation_requested = false \
           AND updated_at < now() - ($1::TEXT || ' minutes')::INTERVAL",
    )
    .bind(stale_minutes)
    .execute(pool)
    .await?;
    if result.rows_affected() > 0 {
        metrics::counter!("download_jobs_recovered_total").increment(result.rows_affected());
    }
    Ok(())
}

async fn cleanup_due_packages(pool: &PgPool, package_root: &Path) -> anyhow::Result<u64> {
    let rows = sqlx::query(
        "SELECT id, download_job_id, user_id, user_session_id, device_identifier, media_item_id, storage_key \
         FROM download_packages \
         WHERE status IN ('expired', 'revoked', 'cleanup_pending', 'failed') \
           AND cleanup_after_at IS NOT NULL \
           AND cleanup_after_at <= now() \
         LIMIT 25",
    )
    .fetch_all(pool)
    .await?;

    let mut cleaned = 0u64;
    for row in rows {
        let package_id: Uuid = row.get("id");
        let storage_key: String = row.get("storage_key");
        if let Some(dir) = storage_dir_for_key(package_root, &storage_key) {
            let _ = tokio::fs::remove_dir_all(dir).await;
        }
        sqlx::query(
            "UPDATE download_packages SET status = 'cleaned', updated_at = now() WHERE id = $1",
        )
        .bind(package_id)
        .execute(pool)
        .await?;
        record_event(
            pool,
            row.try_get("user_id").ok(),
            row.try_get("user_session_id").ok().flatten(),
            row.try_get("download_job_id").ok(),
            Some(package_id),
            row.try_get("media_item_id").ok(),
            row.try_get::<String, _>("device_identifier")
                .ok()
                .as_deref(),
            "cleanup",
            Some("package cleanup completed"),
            json!({ "storage_key": storage_key }),
        )
        .await?;
        cleaned += 1;
    }
    if cleaned > 0 {
        metrics::counter!("download_packages_cleaned_total").increment(cleaned);
    }
    Ok(cleaned)
}

async fn update_progress(
    pool: &PgPool,
    job_id: Uuid,
    progress_percent: f32,
    bytes_prepared: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE download_jobs \
         SET progress_percent = $2, bytes_prepared = GREATEST(bytes_prepared, $3), updated_at = now() \
         WHERE id = $1",
    )
    .bind(job_id)
    .bind(progress_percent)
    .bind(bytes_prepared)
    .execute(pool)
    .await?;
    Ok(())
}

async fn is_cancelled(pool: &PgPool, job_id: Uuid) -> anyhow::Result<bool> {
    let cancelled: bool =
        sqlx::query_scalar("SELECT cancellation_requested FROM download_jobs WHERE id = $1")
            .bind(job_id)
            .fetch_one(pool)
            .await?;
    Ok(cancelled)
}

async fn reset_package_dir(package_root: &Path, package_dir: &Path) -> anyhow::Result<()> {
    if !package_dir.starts_with(package_root) {
        return Err(anyhow!("refusing to write outside download package root"));
    }
    let _ = tokio::fs::remove_dir_all(package_dir).await;
    tokio::fs::create_dir_all(package_dir)
        .await
        .with_context(|| format!("failed to create {}", package_dir.display()))?;
    Ok(())
}

async fn write_package_manifest(job: &DownloadJobWork, package_dir: &Path) -> anyhow::Result<()> {
    let files = collect_package_files(package_dir)?;
    let manifest = json!({
        "schema_version": 1,
        "manifest_version": 1,
        "download_job_id": job.id,
        "media_item_id": job.media_item_id,
        "media_file_id": job.media_file_id,
        "package_format": job.package_format,
        "package_strategy": job.package_strategy,
        "quality_mode": job.quality_mode,
        "quality_label": job.quality_label,
        "source_version": {
            "file_hash": job.source_file_hash,
            "file_modified_at": job.source_modified_at,
            "container_format": job.source_container,
            "video_codec": job.source_video_codec,
            "video_resolution": job.source_video_resolution,
            "video_bitrate": job.source_video_bitrate,
            "audio_codec": job.source_audio_codec,
            "audio_channels": job.source_audio_channels,
            "audio_language": job.source_audio_language,
            "runtime_seconds": job.runtime_seconds
        },
        "bytes_expected": job.bytes_expected,
        "expires_at": job.expires_at,
        "files": files.iter().map(|file| json!({
            "relative_path": file.relative_path,
            "file_role": file.file_role,
            "content_type": file.content_type,
            "byte_size": file.byte_size,
            "checksum_sha256": file.checksum_sha256,
            "segment_index": file.segment_index,
            "is_required": file.is_required
        })).collect::<Vec<_>>(),
        "access_policy": job.access_policy_snapshot
    });
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    tokio::fs::write(package_dir.join("manifest.json"), bytes).await?;
    Ok(())
}

fn collect_package_files(package_dir: &Path) -> anyhow::Result<Vec<PackageFile>> {
    let mut files = Vec::new();
    collect_package_files_inner(package_dir, package_dir, &mut files)?;
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(files)
}

fn collect_package_files_inner(
    root: &Path,
    current: &Path,
    files: &mut Vec<PackageFile>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_package_files_inner(root, &path, files)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let relative_path = path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        files.push(PackageFile {
            file_role: file_role_for_path(&relative_path),
            content_type: content_type_for_path(&relative_path).map(str::to_string),
            byte_size: metadata.len() as i64,
            checksum_sha256: sha256_file_blocking(&path)?,
            segment_index: segment_index_for_path(&relative_path),
            track_type: track_type_for_path(&relative_path).map(str::to_string),
            is_required: true,
            relative_path,
        });
    }
    Ok(())
}

async fn package_size(package_dir: &Path) -> u64 {
    directory_size(package_dir).unwrap_or(0)
}

fn directory_size(path: &Path) -> anyhow::Result<u64> {
    let mut total = 0u64;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            total += directory_size(&entry.path())?;
        } else if metadata.is_file() {
            total += metadata.len();
        }
    }
    Ok(total)
}

fn package_hash(files: &[PackageFile]) -> String {
    let seed = files
        .iter()
        .map(|file| {
            format!(
                "{}:{}:{}",
                file.relative_path, file.byte_size, file.checksum_sha256
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    sha256_bytes(seed.as_bytes())
}

fn sha256_file_blocking(path: &Path) -> anyhow::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut context = DigestContext::new(&SHA256);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        context.update(&buffer[..read]);
    }
    Ok(hex_digest(context.finish().as_ref()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut context = DigestContext::new(&SHA256);
    context.update(bytes);
    hex_digest(context.finish().as_ref())
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{:02x}", byte)).collect()
}

fn file_role_for_path(path: &str) -> String {
    if path == "manifest.json" {
        "manifest"
    } else if path.ends_with(".m3u8") {
        "manifest"
    } else if path.ends_with(".mp4") && path.starts_with("media.") {
        "mp4"
    } else if path.ends_with(".mp4") {
        "init_segment"
    } else if path.ends_with(".m4s") {
        "media_segment"
    } else if path.ends_with(".vtt") || path.ends_with(".srt") {
        "subtitle"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") || path.ends_with(".png") {
        "artwork"
    } else {
        "metadata"
    }
    .to_string()
}

fn content_type_for_path(path: &str) -> Option<&'static str> {
    if path.ends_with(".json") {
        Some("application/json")
    } else if path.ends_with(".m3u8") {
        Some("application/vnd.apple.mpegurl")
    } else if path.ends_with(".mp4") || path.ends_with(".m4s") {
        Some("video/mp4")
    } else if path.ends_with(".vtt") {
        Some("text/vtt")
    } else if path.ends_with(".srt") {
        Some("application/x-subrip")
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        Some("image/jpeg")
    } else if path.ends_with(".png") {
        Some("image/png")
    } else {
        None
    }
}

fn track_type_for_path(path: &str) -> Option<&'static str> {
    if path.ends_with(".vtt") || path.ends_with(".srt") {
        Some("subtitle")
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") || path.ends_with(".png") {
        Some("image")
    } else if path.ends_with(".json") {
        Some("metadata")
    } else {
        Some("video")
    }
}

fn segment_index_for_path(path: &str) -> Option<i32> {
    let file_name = Path::new(path).file_stem()?.to_string_lossy();
    file_name.strip_prefix("seg_")?.parse::<i32>().ok()
}

fn storage_dir_for_key(package_root: &Path, storage_key: &str) -> Option<PathBuf> {
    let id = storage_key.strip_prefix(&format!("{DOWNLOADS_SUBDIR}/"))?;
    if Uuid::parse_str(id).is_err() {
        return None;
    }
    let dir = package_root.join(id);
    dir.starts_with(package_root).then_some(dir)
}

fn resolution_height(value: &str) -> Option<i32> {
    let value = value.trim().to_lowercase();
    if let Some(height) = value.strip_suffix('p') {
        return height.parse::<i32>().ok();
    }
    if value == "4k" || value == "uhd" {
        return Some(2160);
    }
    if let Some((_, height)) = value.split_once('x') {
        return height.parse::<i32>().ok();
    }
    None
}

async fn record_event(
    pool: &PgPool,
    user_id: Option<Uuid>,
    user_session_id: Option<Uuid>,
    download_job_id: Option<Uuid>,
    download_package_id: Option<Uuid>,
    media_item_id: Option<Uuid>,
    device_identifier: Option<&str>,
    event_type: &str,
    reason: Option<&str>,
    details: Value,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO download_events \
         (user_id, user_session_id, download_job_id, download_package_id, media_item_id, \
          device_identifier, event_type, reason, details) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(user_id)
    .bind(user_session_id)
    .bind(download_job_id)
    .bind(download_package_id)
    .bind(media_item_id)
    .bind(device_identifier)
    .bind(event_type)
    .bind(reason)
    .bind(details)
    .execute(pool)
    .await?;
    Ok(())
}

fn truncate_reason(reason: &str) -> String {
    const MAX_LEN: usize = 512;
    if reason.len() <= MAX_LEN {
        reason.to_string()
    } else {
        reason.chars().take(MAX_LEN).collect()
    }
}

fn default_max_jobs_per_run() -> i64 {
    DEFAULT_MAX_JOBS_PER_RUN
}

fn default_max_retries() -> i32 {
    DEFAULT_MAX_RETRIES
}

fn default_stale_preparing_minutes() -> i64 {
    DEFAULT_STALE_PREPARING_MINUTES
}

fn default_failed_cleanup_hours() -> i64 {
    DEFAULT_FAILED_CLEANUP_HOURS
}
