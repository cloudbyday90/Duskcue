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

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use ring::digest::{SHA256, digest};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::extractors::AuthenticatedUser;
use crate::state::{AppState, DownloadsConfig, NetworkMode};

use super::error::DownloadError;
use super::types::*;

#[derive(Clone)]
struct DownloadSourceCandidate {
    id: Uuid,
    updated_at: DateTime<Utc>,
    file_size: i64,
    file_hash: Option<String>,
    container_format: String,
    video_codec: Option<String>,
    video_resolution: Option<String>,
    video_bitrate: Option<i32>,
    audio_codec: Option<String>,
    audio_channels: Option<i32>,
    audio_language: Option<String>,
    audio_bitrate: Option<i32>,
    runtime_seconds: i32,
    additional_streams: Value,
}

pub struct DownloadPackageFileServe {
    pub path: PathBuf,
    pub relative_path: String,
    pub file_role: String,
    pub content_type: Option<String>,
    pub byte_size: i64,
    pub checksum_sha256: String,
    pub segment_index: Option<i32>,
}

pub struct DownloadRangeSpec {
    pub start: u64,
    pub end: u64,
    pub total: u64,
}

impl DownloadRangeSpec {
    pub fn parse(header: Option<&str>, file_size: u64) -> Result<Option<Self>, DownloadError> {
        let header = match header {
            Some(header) => header,
            None => return Ok(None),
        };
        if file_size == 0 {
            return Err(DownloadError::InvalidByteRange(
                "cannot range over an empty package file".into(),
            ));
        }

        let bytes_spec = header
            .strip_prefix("bytes=")
            .ok_or_else(|| DownloadError::InvalidByteRange("expected bytes= prefix".into()))?;
        if bytes_spec.contains(',') {
            return Err(DownloadError::InvalidByteRange(
                "multiple byte ranges are not supported".into(),
            ));
        }

        let (start, end) = if let Some(rest) = bytes_spec.strip_suffix('-') {
            let start: u64 = rest.parse().map_err(|_| {
                DownloadError::InvalidByteRange(format!("invalid start byte: {rest}"))
            })?;
            (start, file_size - 1)
        } else if let Some(rest) = bytes_spec.strip_prefix('-') {
            let suffix_len: u64 = rest.parse().map_err(|_| {
                DownloadError::InvalidByteRange(format!("invalid suffix length: {rest}"))
            })?;
            let start = file_size.saturating_sub(suffix_len);
            (start, file_size - 1)
        } else {
            let parts: Vec<&str> = bytes_spec.split('-').collect();
            if parts.len() != 2 {
                return Err(DownloadError::InvalidByteRange(format!(
                    "invalid range format: {bytes_spec}"
                )));
            }
            let start: u64 = parts[0].parse().map_err(|_| {
                DownloadError::InvalidByteRange(format!("invalid start: {}", parts[0]))
            })?;
            let end: u64 = if parts[1].is_empty() {
                file_size - 1
            } else {
                parts[1].parse().map_err(|_| {
                    DownloadError::InvalidByteRange(format!("invalid end: {}", parts[1]))
                })?
            };
            (start, end)
        };

        if start > end || start >= file_size {
            return Err(DownloadError::InvalidByteRange(format!(
                "range {start}-{end} out of bounds for file size {file_size}"
            )));
        }

        Ok(Some(Self {
            start,
            end: end.min(file_size - 1),
            total: file_size,
        }))
    }

    pub fn content_length(&self) -> u64 {
        self.end - self.start + 1
    }

    pub fn content_range_header(&self) -> String {
        format!("bytes {}-{}/{}", self.start, self.end, self.total)
    }
}

pub async fn get_download_plan(
    state: &AppState,
    user: &AuthenticatedUser,
    media_item_id: Uuid,
    query: DownloadPlanQuery,
) -> Result<DownloadPlanResponse, DownloadError> {
    build_download_plan(state, user, media_item_id, query).await
}

async fn build_download_plan(
    state: &AppState,
    user: &AuthenticatedUser,
    media_item_id: Uuid,
    query: DownloadPlanQuery,
) -> Result<DownloadPlanResponse, DownloadError> {
    let device_identifier = query
        .device_identifier
        .as_deref()
        .ok_or_else(|| DownloadError::InvalidRequest("device_identifier is required".into()))?;
    let platform = query
        .client_platform
        .ok_or_else(|| DownloadError::InvalidRequest("client_platform is required".into()))?;
    authorize_download_request(state, user, media_item_id, device_identifier, platform).await?;

    let config = state.runtime_config.load();
    let downloads = config.downloads.clone();
    let quality = config.quality.clone();
    drop(config);

    let source = select_source_file(&state.pool, media_item_id, query.media_file_id).await?;
    let quality_mode = query.quality_mode.unwrap_or(DownloadQualityMode::Auto);
    let target = resolve_quality_target(
        quality_mode,
        source.video_resolution.as_deref(),
        &downloads.max_quality_resolution,
        quality.fallback_max_bitrate_bps,
    );
    let package_format = choose_package_format(&source, &target);
    let package_strategy = choose_package_strategy(&source, &target, package_format);

    if package_strategy == "transcode" && !downloads.allow_transcoded_downloads {
        record_download_event(
            &state.pool,
            user,
            Some(media_item_id),
            Some(device_identifier),
            "policy_denied",
            Some("transcoded downloads are disabled by server policy"),
        )
        .await?;
        return Err(DownloadError::PolicyDenied(
            "transcoded downloads are disabled by server policy".into(),
        ));
    }

    let estimated_bytes = estimate_package_bytes(&source, &target, &package_strategy);
    let expires_at =
        Some(Utc::now() + Duration::days(i64::from(downloads.default_package_expiry_days)));
    let quality_options =
        quality_options_for_source(&source, &downloads, quality.fallback_max_bitrate_bps);
    let audio_options = extract_audio_options(&source);
    let subtitle_options = extract_subtitle_options(&source);
    let plan_revision = format!(
        "v1:{}:{}:{}",
        source.id,
        source.file_hash.as_deref().unwrap_or("no-file-hash"),
        source.updated_at.timestamp()
    );
    let policy = json!({
        "max_quality_resolution": downloads.max_quality_resolution,
        "allow_transcoded_downloads": downloads.allow_transcoded_downloads,
        "max_bytes_per_user": downloads.max_bytes_per_user,
        "max_bytes_per_device": downloads.max_bytes_per_device,
        "default_package_expiry_days": downloads.default_package_expiry_days
    });

    let hash_seed = PlanHashSeed {
        media_item_id,
        media_file_id: source.id,
        device_identifier,
        package_format,
        package_strategy: &package_strategy,
        quality_mode,
        target_resolution: target.resolution.as_deref(),
        target_bitrate_bps: target.bitrate_bps,
        estimated_bytes,
        plan_revision: &plan_revision,
    };
    let plan_hash = sha256_hex(&serde_json::to_string(&hash_seed).unwrap_or_default());

    Ok(DownloadPlanResponse {
        media_item_id,
        media_file_id: Some(source.id),
        package_format,
        package_strategy,
        quality_mode,
        target_resolution: target.resolution,
        target_bitrate_bps: target.bitrate_bps,
        estimated_bytes,
        estimated_duration_seconds: Some(i64::from(source.runtime_seconds)),
        source_file: Some(source_file_response(&source)),
        quality_options,
        audio_options,
        subtitle_options,
        artwork_included: query.include_artwork.unwrap_or(true),
        storyboards_included: query.include_storyboards.unwrap_or(false),
        expires_at,
        policy,
        plan_revision,
        plan_hash,
    })
}

pub async fn create_download_job(
    state: &AppState,
    user: &AuthenticatedUser,
    req: CreateDownloadJobRequest,
) -> Result<DownloadJobResponse, DownloadError> {
    let plan = build_download_plan(
        state,
        user,
        req.media_item_id,
        DownloadPlanQuery {
            device_identifier: Some(req.device_identifier.clone()),
            client_platform: Some(req.client_platform),
            quality_mode: Some(req.quality_mode),
            media_file_id: req.media_file_id,
            include_storyboards: Some(req.include_storyboards),
            include_artwork: Some(req.include_artwork),
        },
    )
    .await?;

    if req.plan_revision != plan.plan_revision || req.plan_hash != plan.plan_hash {
        return Err(DownloadError::StaleClientState(
            "download plan is stale; refresh the plan and try again".into(),
        ));
    }

    let library_id: Uuid = sqlx::query_scalar("SELECT library_id FROM media_items WHERE id = $1")
        .bind(req.media_item_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| DownloadError::UnsupportedMedia("media item is unavailable".into()))?;

    let selected_subtitles = serde_json::to_value(&req.selected_subtitles)
        .map_err(|_| DownloadError::InvalidRequest("selected_subtitles is invalid".into()))?;
    let selected_artwork = json!({
        "included": req.include_artwork
    });
    let included_storyboards = json!({
        "included": req.include_storyboards
    });
    let metadata = json!({
        "target_resolution": plan.target_resolution,
        "target_bitrate_bps": plan.target_bitrate_bps,
        "estimated_duration_seconds": plan.estimated_duration_seconds,
        "included_storyboards": included_storyboards
    });
    let quality_label = plan
        .quality_options
        .iter()
        .find(|option| option.quality_mode == req.quality_mode)
        .map(|option| option.label.clone());

    let row = sqlx::query(
        "INSERT INTO download_jobs \
         (user_id, user_session_id, device_identifier, device_name, client_platform, \
          client_version, library_id, media_item_id, media_file_id, status, package_format, \
          package_strategy, quality_mode, quality_label, selected_audio, selected_subtitles, \
          selected_artwork, bytes_expected, plan_revision, plan_hash, access_policy_snapshot, \
          expires_at, cleanup_after_at, metadata) \
         VALUES \
         ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'queued', $10, $11, $12, $13, $14, $15, \
          $16, $17, $18, $19, $20, $21, $21 + INTERVAL '7 days', $22) \
         RETURNING id, media_item_id, media_file_id, device_identifier, status, package_format, \
                   quality_mode, progress_percent::REAL AS progress_percent, bytes_expected, \
                   bytes_prepared, failure_reason, expires_at",
    )
    .bind(user.user_id)
    .bind(user.session_id)
    .bind(&req.device_identifier)
    .bind(&req.device_name)
    .bind(client_platform_to_db(req.client_platform))
    .bind(&req.client_version)
    .bind(library_id)
    .bind(req.media_item_id)
    .bind(plan.media_file_id)
    .bind(package_format_to_db(plan.package_format))
    .bind(&plan.package_strategy)
    .bind(quality_mode_to_db(req.quality_mode))
    .bind(quality_label)
    .bind(&req.selected_audio)
    .bind(selected_subtitles)
    .bind(selected_artwork)
    .bind(plan.estimated_bytes)
    .bind(&plan.plan_revision)
    .bind(&plan.plan_hash)
    .bind(&plan.policy)
    .bind(plan.expires_at)
    .bind(metadata)
    .fetch_one(&state.pool)
    .await?;

    record_download_event(
        &state.pool,
        user,
        Some(req.media_item_id),
        Some(&req.device_identifier),
        "job_created",
        None,
    )
    .await?;
    metrics::counter!("download_jobs_queued_total").increment(1);

    job_response_from_row(&row)
}

pub async fn get_download_job(
    state: &AppState,
    user: &AuthenticatedUser,
    id: Uuid,
) -> Result<DownloadJobResponse, DownloadError> {
    let row = sqlx::query(
        "SELECT id, media_item_id, media_file_id, device_identifier, status, package_format, \
                quality_mode, progress_percent::REAL AS progress_percent, bytes_expected, \
                bytes_prepared, failure_reason, expires_at \
         FROM download_jobs \
         WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await?;

    let Some(row) = row else {
        return Err(DownloadError::JobNotFound(id));
    };

    job_response_from_row(&row)
}

pub async fn cancel_download_job(
    state: &AppState,
    user: &AuthenticatedUser,
    id: Uuid,
    req: CancelDownloadJobRequest,
) -> Result<DownloadActionResponse, DownloadError> {
    ensure_job_owner(&state.pool, user, id).await?;
    let reason = req
        .reason
        .unwrap_or_else(|| "cancelled by user".to_string());
    let row = sqlx::query(
        "UPDATE download_jobs \
         SET status = CASE WHEN status IN ('ready', 'failed', 'cancelled', 'expired', 'revoked') \
                           THEN status ELSE 'cancelled' END, \
             cancellation_requested = true, \
             failure_reason = CASE WHEN status IN ('ready', 'failed', 'cancelled', 'expired', 'revoked') \
                                   THEN failure_reason ELSE $3 END, \
             completed_at = CASE WHEN status IN ('ready', 'failed', 'cancelled', 'expired', 'revoked') \
                                 THEN completed_at ELSE now() END, \
             cleanup_after_at = now(), \
             updated_at = now() \
         WHERE id = $1 AND user_id = $2 \
         RETURNING status",
    )
    .bind(id)
    .bind(user.user_id)
    .bind(&reason)
    .fetch_optional(&state.pool)
    .await?;

    let Some(row) = row else {
        return Err(DownloadError::JobNotFound(id));
    };
    let status: String = row.get("status");

    record_download_event(
        &state.pool,
        user,
        None,
        None,
        "job_cancelled",
        Some(&reason),
    )
    .await?;
    metrics::counter!("download_jobs_cancelled_total").increment(1);

    Ok(DownloadActionResponse {
        ok: true,
        id,
        status,
    })
}

pub async fn list_download_inventory(
    _state: &AppState,
    _user: &AuthenticatedUser,
    _query: DownloadInventoryQuery,
) -> Result<DownloadInventoryResponse, DownloadError> {
    Err(DownloadError::NotImplemented("download inventory"))
}

pub async fn delete_download_package(
    state: &AppState,
    user: &AuthenticatedUser,
    id: Uuid,
    _req: DeleteDownloadPackageRequest,
) -> Result<DownloadActionResponse, DownloadError> {
    ensure_package_owner(&state.pool, user, id).await?;
    Err(DownloadError::NotImplemented("download package deletion"))
}

pub async fn get_package_manifest(
    state: &AppState,
    user: &AuthenticatedUser,
    id: Uuid,
    query: DownloadPackageAccessQuery,
) -> Result<DownloadPackageManifestResponse, DownloadError> {
    let device_identifier = require_device_identifier(query.device_identifier.as_deref())?;
    let row = sqlx::query(
        "SELECT dp.id, dp.download_job_id, dp.user_session_id, dp.device_identifier, \
                dp.media_item_id, dp.media_file_id, dp.status, dp.package_format, \
                dp.manifest_version, dp.total_bytes, \
                dp.package_hash_sha256, dp.selected_audio, dp.selected_subtitles, \
                dp.included_artwork, dp.included_storyboards, dp.sync_metadata, \
                dp.access_policy_snapshot, dp.expires_at, dp.revoked_at, \
                dj.package_strategy, dj.quality_mode, dj.quality_label, \
                mf.file_hash, mf.file_modified_at, mf.container_format, mf.video_codec, \
                mf.video_resolution, mf.video_bitrate, mf.audio_codec, mf.audio_channels, \
                mf.audio_language, mf.runtime_seconds \
         FROM download_packages dp \
         JOIN download_jobs dj ON dj.id = dp.download_job_id \
         LEFT JOIN media_files mf ON mf.id = dp.media_file_id \
         WHERE dp.id = $1 AND dp.user_id = $2",
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await?;

    let Some(row) = row else {
        return Err(DownloadError::PackageNotFound(id));
    };

    revalidate_package_access_from_row(state, user, &row, id, device_identifier).await?;

    let files = load_package_manifest_files(&state.pool, id).await?;
    let source_version = json!({
        "media_file_id": row.try_get::<Option<Uuid>, _>("media_file_id").ok().flatten(),
        "file_hash": row.try_get::<Option<String>, _>("file_hash").ok().flatten(),
        "file_modified_at": row.try_get::<Option<DateTime<Utc>>, _>("file_modified_at").ok().flatten(),
        "container_format": row.try_get::<Option<String>, _>("container_format").ok().flatten(),
        "video_codec": row.try_get::<Option<String>, _>("video_codec").ok().flatten(),
        "video_resolution": row.try_get::<Option<String>, _>("video_resolution").ok().flatten(),
        "video_bitrate": row.try_get::<Option<i32>, _>("video_bitrate").ok().flatten(),
        "audio_codec": row.try_get::<Option<String>, _>("audio_codec").ok().flatten(),
        "audio_channels": row.try_get::<Option<i32>, _>("audio_channels").ok().flatten(),
        "audio_language": row.try_get::<Option<String>, _>("audio_language").ok().flatten(),
        "runtime_seconds": row.try_get::<Option<i32>, _>("runtime_seconds").ok().flatten()
    });
    let selected_quality = json!({
        "quality_mode": row.try_get::<String, _>("quality_mode").unwrap_or_else(|_| "auto".to_string()),
        "quality_label": row.try_get::<Option<String>, _>("quality_label").ok().flatten()
    });

    Ok(DownloadPackageManifestResponse {
        package_id: row.get("id"),
        download_job_id: row.get("download_job_id"),
        schema_version: 1,
        manifest_version: row.get("manifest_version"),
        package_format: package_format_from_db(&row.get::<String, _>("package_format"))?,
        package_strategy: row.get("package_strategy"),
        media_item_id: row.get("media_item_id"),
        media_file_id: row.try_get("media_file_id").ok().flatten(),
        source_version,
        selected_quality,
        total_bytes: row.get("total_bytes"),
        package_hash_sha256: row.try_get("package_hash_sha256").ok().flatten(),
        files,
        selected_audio: row.get("selected_audio"),
        selected_subtitles: json_array_to_vec(row.get("selected_subtitles")),
        included_artwork: row.get("included_artwork"),
        included_storyboards: row.get("included_storyboards"),
        expires_at: row.try_get("expires_at").ok().flatten(),
        sync_metadata: row.get("sync_metadata"),
        access_policy: row.get("access_policy_snapshot"),
    })
}

pub async fn create_package_transfer_urls(
    state: &AppState,
    user: &AuthenticatedUser,
    id: Uuid,
    req: PackageTransferUrlsRequest,
) -> Result<PackageTransferUrlsResponse, DownloadError> {
    let device_identifier = require_device_identifier(Some(&req.device_identifier))?;
    let _package = revalidate_package_access(state, user, id, device_identifier).await?;
    let expires_at = Utc::now() + Duration::minutes(15);
    let mut files = Vec::with_capacity(req.file_paths.len());

    for requested in req.file_paths {
        let relative_path = normalize_package_relative_path(&requested)?;
        let file = load_package_file_record(&state.pool, id, &relative_path)
            .await?
            .ok_or_else(|| {
                DownloadError::InvalidRequest(format!(
                    "package file is not in the manifest: {relative_path}"
                ))
            })?;
        files.push(PackageTransferUrlResponse {
            relative_path: file.relative_path.clone(),
            url: authenticated_package_file_url(id, &file.relative_path, device_identifier),
            method: "GET".to_string(),
            headers: json!({
                "Accept-Ranges": "bytes",
                "X-Duskcue-Checksum-Sha256": file.checksum_sha256,
                "X-Duskcue-Byte-Size": file.byte_size
            }),
        });
    }

    Ok(PackageTransferUrlsResponse {
        package_id: id,
        expires_at,
        files,
    })
}

pub async fn serve_package_file(
    state: &AppState,
    user: &AuthenticatedUser,
    id: Uuid,
    file_path: String,
    query: DownloadPackageAccessQuery,
) -> Result<DownloadPackageFileServe, DownloadError> {
    let device_identifier = require_device_identifier(query.device_identifier.as_deref())?;
    let package = revalidate_package_access(state, user, id, device_identifier).await?;
    let relative_path = normalize_package_relative_path(&file_path)?;
    let file = load_package_file_record(&state.pool, id, &relative_path)
        .await?
        .ok_or_else(|| DownloadError::PackageNotFound(id))?;

    let package_root = state.bootstrap.data_dir.join("downloads");
    let package_dir = package_dir_from_storage_key(&package_root, &package.storage_key)?;
    let package_dir = tokio::fs::canonicalize(&package_dir).await.map_err(|_| {
        DownloadError::StorageUnavailable("download package directory is unavailable".into())
    })?;
    let physical_path = resolve_package_file_path(&package_dir, &file.relative_path)?;
    let physical_path = tokio::fs::canonicalize(&physical_path).await.map_err(|_| {
        DownloadError::StorageUnavailable("download package file is unavailable".into())
    })?;
    if !physical_path.starts_with(&package_dir) {
        return Err(DownloadError::InvalidRequest(
            "package file path is invalid".into(),
        ));
    }
    let metadata = tokio::fs::metadata(&physical_path).await.map_err(|_| {
        DownloadError::StorageUnavailable("download package file is unavailable".into())
    })?;
    if !metadata.is_file() {
        return Err(DownloadError::StorageUnavailable(
            "download package file is unavailable".into(),
        ));
    }
    if metadata.len() as i64 != file.byte_size {
        return Err(DownloadError::ChecksumMismatch(file.relative_path));
    }

    sqlx::query(
        "UPDATE download_packages \
         SET status = CASE WHEN status = 'ready' THEN 'serving' ELSE status END, \
             first_served_at = COALESCE(first_served_at, now()), \
             last_served_at = now(), \
             updated_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .execute(&state.pool)
    .await?;
    record_download_event_for_package(
        &state.pool,
        user,
        Some(package.download_job_id),
        Some(id),
        Some(package.media_item_id),
        Some(device_identifier),
        "package_served",
        Some(&file.relative_path),
    )
    .await?;
    metrics::counter!("download_package_files_served_total", "role" => file.file_role.clone())
        .increment(1);

    Ok(DownloadPackageFileServe {
        path: physical_path,
        relative_path: file.relative_path,
        file_role: file.file_role,
        content_type: file.content_type,
        byte_size: file.byte_size,
        checksum_sha256: file.checksum_sha256,
        segment_index: file.segment_index,
    })
}

pub async fn sync_download_state(
    state: &AppState,
    user: &AuthenticatedUser,
    req: DownloadSyncRequest,
) -> Result<DownloadSyncResponse, DownloadError> {
    for package_state in &req.package_states {
        ensure_package_owner(&state.pool, user, package_state.package_id).await?;
    }
    for playback_event in &req.playback_events {
        ensure_package_owner(&state.pool, user, playback_event.package_id).await?;
    }
    Err(DownloadError::NotImplemented("download reconnect sync"))
}

#[derive(Clone)]
struct QualityTarget {
    resolution: Option<String>,
    bitrate_bps: Option<i64>,
}

#[derive(Serialize)]
struct PlanHashSeed<'a> {
    media_item_id: Uuid,
    media_file_id: Uuid,
    device_identifier: &'a str,
    package_format: DownloadPackageFormat,
    package_strategy: &'a str,
    quality_mode: DownloadQualityMode,
    target_resolution: Option<&'a str>,
    target_bitrate_bps: Option<i64>,
    estimated_bytes: Option<i64>,
    plan_revision: &'a str,
}

async fn select_source_file(
    pool: &sqlx::PgPool,
    media_item_id: Uuid,
    requested_media_file_id: Option<Uuid>,
) -> Result<DownloadSourceCandidate, DownloadError> {
    let item_row = sqlx::query("SELECT type FROM media_items WHERE id = $1")
        .bind(media_item_id)
        .fetch_optional(pool)
        .await?;
    let Some(item_row) = item_row else {
        return Err(DownloadError::UnsupportedMedia(
            "media item is unavailable".into(),
        ));
    };
    let media_type: String = item_row.get("type");
    if media_type != "movie" && media_type != "episode" {
        return Err(DownloadError::UnsupportedMedia(
            "only movies and episodes support offline downloads".into(),
        ));
    }

    let rows = sqlx::query(
        "SELECT id, updated_at, file_size, file_hash, container_format, video_codec, \
                video_resolution, video_bitrate, audio_codec, audio_channels, \
                audio_language, audio_bitrate, runtime_seconds, additional_streams \
         FROM media_files \
         WHERE media_item_id = $1 \
           AND is_healthy = true \
           AND ($2::uuid IS NULL OR id = $2)",
    )
    .bind(media_item_id)
    .bind(requested_media_file_id)
    .fetch_all(pool)
    .await?;

    let mut candidates: Vec<DownloadSourceCandidate> = rows
        .iter()
        .map(|row| DownloadSourceCandidate {
            id: row.get("id"),
            updated_at: row.get("updated_at"),
            file_size: row.get("file_size"),
            file_hash: row.try_get("file_hash").ok().flatten(),
            container_format: row.get("container_format"),
            video_codec: row.try_get("video_codec").ok().flatten(),
            video_resolution: row.try_get("video_resolution").ok().flatten(),
            video_bitrate: row.try_get("video_bitrate").ok().flatten(),
            audio_codec: row.try_get("audio_codec").ok().flatten(),
            audio_channels: row.try_get("audio_channels").ok().flatten(),
            audio_language: row.try_get("audio_language").ok().flatten(),
            audio_bitrate: row.try_get("audio_bitrate").ok().flatten(),
            runtime_seconds: row.get("runtime_seconds"),
            additional_streams: row.get("additional_streams"),
        })
        .collect();

    if candidates.is_empty() {
        return Err(DownloadError::UnsupportedMedia(
            "no healthy media file is available for download".into(),
        ));
    }

    candidates.sort_by_key(|candidate| {
        (
            !is_mobile_direct_mp4(candidate),
            candidate
                .video_resolution
                .as_deref()
                .and_then(resolution_height)
                .unwrap_or(i32::MAX),
            candidate.file_size,
        )
    });

    Ok(candidates.remove(0))
}

fn resolve_quality_target(
    quality_mode: DownloadQualityMode,
    source_resolution: Option<&str>,
    policy_max_resolution: &str,
    fallback_bitrate_bps: i64,
) -> QualityTarget {
    let source_height = source_resolution
        .and_then(resolution_height)
        .unwrap_or(1080);
    let policy_height = resolution_height(policy_max_resolution).unwrap_or(1080);
    let ceiling = source_height.min(policy_height);
    let target_height = match quality_mode {
        DownloadQualityMode::DataSaver => ceiling.min(480),
        DownloadQualityMode::Standard => ceiling.min(720),
        DownloadQualityMode::Auto => ceiling.min(1080),
        DownloadQualityMode::Maximum | DownloadQualityMode::Manual => ceiling,
    };

    let bitrate_bps = match target_height {
        h if h <= 480 => 1_500_000,
        h if h <= 720 => 3_000_000,
        h if h <= 1080 => fallback_bitrate_bps.max(6_000_000),
        _ => 16_000_000,
    };

    QualityTarget {
        resolution: Some(format!("{target_height}p")),
        bitrate_bps: Some(bitrate_bps),
    }
}

fn choose_package_format(
    source: &DownloadSourceCandidate,
    target: &QualityTarget,
) -> DownloadPackageFormat {
    if is_mobile_direct_mp4(source)
        && target.resolution.as_deref().and_then(resolution_height)
            >= source
                .video_resolution
                .as_deref()
                .and_then(resolution_height)
    {
        DownloadPackageFormat::Mp4
    } else {
        DownloadPackageFormat::HlsFmp4
    }
}

fn choose_package_strategy(
    source: &DownloadSourceCandidate,
    target: &QualityTarget,
    package_format: DownloadPackageFormat,
) -> String {
    let source_height = source
        .video_resolution
        .as_deref()
        .and_then(resolution_height);
    let target_height = target.resolution.as_deref().and_then(resolution_height);
    let needs_downscale = source_height.zip(target_height).is_some_and(|(s, t)| s > t);
    let codec = source
        .video_codec
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();

    if needs_downscale || !matches!(codec.as_str(), "h264" | "avc" | "avc1" | "hevc" | "h265") {
        "transcode".to_string()
    } else if matches!(package_format, DownloadPackageFormat::Mp4) {
        "direct_copy".to_string()
    } else {
        "remux".to_string()
    }
}

fn estimate_package_bytes(
    source: &DownloadSourceCandidate,
    target: &QualityTarget,
    package_strategy: &str,
) -> Option<i64> {
    if package_strategy == "direct_copy" || package_strategy == "remux" {
        return Some(source.file_size);
    }

    let bitrate = target.bitrate_bps?;
    let audio_bitrate = i64::from(source.audio_bitrate.unwrap_or(192_000));
    Some(((bitrate + audio_bitrate) * i64::from(source.runtime_seconds)) / 8)
}

fn quality_options_for_source(
    source: &DownloadSourceCandidate,
    downloads: &DownloadsConfig,
    fallback_bitrate_bps: i64,
) -> Vec<DownloadQualityOptionResponse> {
    [
        (DownloadQualityMode::Auto, "Auto"),
        (DownloadQualityMode::DataSaver, "Data Saver"),
        (DownloadQualityMode::Standard, "Standard"),
        (DownloadQualityMode::Maximum, "Maximum"),
    ]
    .into_iter()
    .map(|(quality_mode, label)| {
        let target = resolve_quality_target(
            quality_mode,
            source.video_resolution.as_deref(),
            &downloads.max_quality_resolution,
            fallback_bitrate_bps,
        );
        let format = choose_package_format(source, &target);
        let strategy = choose_package_strategy(source, &target, format);
        DownloadQualityOptionResponse {
            quality_mode,
            label: label.to_string(),
            target_resolution: target.resolution.clone(),
            target_bitrate_bps: target.bitrate_bps,
            estimated_bytes: estimate_package_bytes(source, &target, &strategy),
            requires_transcode: strategy == "transcode",
        }
    })
    .collect()
}

fn extract_audio_options(source: &DownloadSourceCandidate) -> Vec<Value> {
    source
        .additional_streams
        .get("audio")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| {
            vec![json!({
                "codec": source.audio_codec.clone(),
                "channels": source.audio_channels,
                "language": source.audio_language.clone(),
                "bitrate": source.audio_bitrate
            })]
        })
}

fn extract_subtitle_options(source: &DownloadSourceCandidate) -> Vec<Value> {
    source
        .additional_streams
        .get("subtitles")
        .or_else(|| source.additional_streams.get("subtitle"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

struct DownloadPackageAccess {
    download_job_id: Uuid,
    media_item_id: Uuid,
    storage_key: String,
}

struct DownloadPackageFileRecord {
    relative_path: String,
    file_role: String,
    content_type: Option<String>,
    byte_size: i64,
    checksum_sha256: String,
    segment_index: Option<i32>,
}

async fn revalidate_package_access(
    state: &AppState,
    user: &AuthenticatedUser,
    package_id: Uuid,
    device_identifier: &str,
) -> Result<DownloadPackageAccess, DownloadError> {
    let row = sqlx::query(
        "SELECT id, download_job_id, user_session_id, device_identifier, media_item_id, \
                status, storage_key, expires_at, revoked_at \
         FROM download_packages \
         WHERE id = $1 AND user_id = $2",
    )
    .bind(package_id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await?;

    let Some(row) = row else {
        return Err(DownloadError::PackageNotFound(package_id));
    };

    revalidate_package_access_from_row(state, user, &row, package_id, device_identifier).await?;

    Ok(DownloadPackageAccess {
        download_job_id: row.get("download_job_id"),
        media_item_id: row.get("media_item_id"),
        storage_key: row.get("storage_key"),
    })
}

async fn revalidate_package_access_from_row(
    state: &AppState,
    user: &AuthenticatedUser,
    row: &sqlx::postgres::PgRow,
    package_id: Uuid,
    device_identifier: &str,
) -> Result<(), DownloadError> {
    let package_device_identifier: String = row.get("device_identifier");
    if package_device_identifier != device_identifier {
        return Err(DownloadError::AccessDenied);
    }

    let package_session_id: Option<Uuid> = row.try_get("user_session_id").ok().flatten();
    if package_session_id.is_some() && package_session_id != Some(user.session_id) {
        return Err(DownloadError::AccessDenied);
    }

    let status: String = row.get("status");
    match status.as_str() {
        "ready" | "serving" => {}
        "expired" => return Err(DownloadError::PackageExpired(package_id)),
        "revoked" => return Err(DownloadError::AccessDenied),
        _ => return Err(DownloadError::PackageNotReady(package_id)),
    }

    let expires_at: Option<DateTime<Utc>> = row.try_get("expires_at").ok().flatten();
    if expires_at.is_some_and(|expires_at| expires_at <= Utc::now()) {
        sqlx::query(
            "UPDATE download_packages \
             SET status = 'expired', cleanup_after_at = COALESCE(cleanup_after_at, now()), \
                 updated_at = now() \
             WHERE id = $1 AND status IN ('ready', 'serving')",
        )
        .bind(package_id)
        .execute(&state.pool)
        .await?;
        record_download_event_for_package(
            &state.pool,
            user,
            row.try_get("download_job_id").ok(),
            Some(package_id),
            row.try_get("media_item_id").ok(),
            Some(device_identifier),
            "package_expired",
            Some("package expiry reached"),
        )
        .await?;
        return Err(DownloadError::PackageExpired(package_id));
    }

    let revoked_at: Option<DateTime<Utc>> = row.try_get("revoked_at").ok().flatten();
    if revoked_at.is_some() {
        return Err(DownloadError::AccessDenied);
    }

    let config = state.runtime_config.load();
    let downloads = config.downloads.clone();
    let network_mode = config.auth.network_mode.clone();
    drop(config);
    validate_network_policy(&downloads, &network_mode)?;

    let media_item_id: Uuid = row.get("media_item_id");
    resolve_media_access(&state.pool, user, media_item_id).await?;
    enforce_streaming_policy(&state.pool, user, media_item_id, device_identifier).await?;
    Ok(())
}

async fn load_package_file_record(
    pool: &sqlx::PgPool,
    package_id: Uuid,
    relative_path: &str,
) -> Result<Option<DownloadPackageFileRecord>, DownloadError> {
    let row = sqlx::query(
        "SELECT relative_path, file_role, content_type, byte_size, checksum_sha256, segment_index \
         FROM download_package_files \
         WHERE download_package_id = $1 AND relative_path = $2",
    )
    .bind(package_id)
    .bind(relative_path)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| DownloadPackageFileRecord {
        relative_path: row.get("relative_path"),
        file_role: row.get("file_role"),
        content_type: row.try_get("content_type").ok().flatten(),
        byte_size: row.get("byte_size"),
        checksum_sha256: row.get("checksum_sha256"),
        segment_index: row.try_get("segment_index").ok().flatten(),
    }))
}

async fn load_package_manifest_files(
    pool: &sqlx::PgPool,
    package_id: Uuid,
) -> Result<Vec<DownloadPackageFileResponse>, DownloadError> {
    let rows = sqlx::query(
        "SELECT relative_path, file_role, content_type, byte_size, checksum_sha256, \
                segment_index, is_required \
         FROM download_package_files \
         WHERE download_package_id = $1 \
         ORDER BY file_role, segment_index NULLS FIRST, relative_path",
    )
    .bind(package_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| DownloadPackageFileResponse {
            relative_path: row.get("relative_path"),
            file_role: row.get("file_role"),
            content_type: row.try_get("content_type").ok().flatten(),
            byte_size: row.get("byte_size"),
            checksum_sha256: row.get("checksum_sha256"),
            segment_index: row.try_get("segment_index").ok().flatten(),
            is_required: row.get("is_required"),
        })
        .collect())
}

fn package_format_from_db(value: &str) -> Result<DownloadPackageFormat, DownloadError> {
    match value {
        "hls_fmp4" => Ok(DownloadPackageFormat::HlsFmp4),
        "mp4" => Ok(DownloadPackageFormat::Mp4),
        other => Err(DownloadError::InvalidRequest(format!(
            "unknown package format in manifest: {other}"
        ))),
    }
}

fn package_format_to_db(value: DownloadPackageFormat) -> &'static str {
    match value {
        DownloadPackageFormat::HlsFmp4 => "hls_fmp4",
        DownloadPackageFormat::Mp4 => "mp4",
    }
}

fn quality_mode_from_db(value: &str) -> Result<DownloadQualityMode, DownloadError> {
    match value {
        "auto" => Ok(DownloadQualityMode::Auto),
        "data_saver" => Ok(DownloadQualityMode::DataSaver),
        "standard" => Ok(DownloadQualityMode::Standard),
        "maximum" => Ok(DownloadQualityMode::Maximum),
        "manual" => Ok(DownloadQualityMode::Manual),
        other => Err(DownloadError::InvalidRequest(format!(
            "unknown quality mode in job: {other}"
        ))),
    }
}

fn quality_mode_to_db(value: DownloadQualityMode) -> &'static str {
    match value {
        DownloadQualityMode::Auto => "auto",
        DownloadQualityMode::DataSaver => "data_saver",
        DownloadQualityMode::Standard => "standard",
        DownloadQualityMode::Maximum => "maximum",
        DownloadQualityMode::Manual => "manual",
    }
}

fn job_status_from_db(value: &str) -> Result<DownloadJobStatus, DownloadError> {
    match value {
        "queued" => Ok(DownloadJobStatus::Queued),
        "preparing" => Ok(DownloadJobStatus::Preparing),
        "ready" => Ok(DownloadJobStatus::Ready),
        "failed" => Ok(DownloadJobStatus::Failed),
        "cancelled" => Ok(DownloadJobStatus::Cancelled),
        "expired" => Ok(DownloadJobStatus::Expired),
        "revoked" => Ok(DownloadJobStatus::Revoked),
        other => Err(DownloadError::InvalidRequest(format!(
            "unknown download job status: {other}"
        ))),
    }
}

fn client_platform_to_db(value: DownloadClientPlatform) -> &'static str {
    match value {
        DownloadClientPlatform::Android => "android",
        DownloadClientPlatform::Ios => "ios",
    }
}

fn json_array_to_vec(value: Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}

fn require_device_identifier(value: Option<&str>) -> Result<&str, DownloadError> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DownloadError::InvalidRequest("device_identifier is required".into()))?;
    if value.len() > 128 {
        return Err(DownloadError::InvalidRequest(
            "device_identifier is too long".into(),
        ));
    }
    Ok(value)
}

fn normalize_package_relative_path(value: &str) -> Result<String, DownloadError> {
    let decoded = urlencoding::decode(value)
        .map_err(|_| DownloadError::InvalidRequest("package file path is invalid".into()))?;
    let normalized = decoded.trim().replace('\\', "/");
    if normalized.is_empty()
        || normalized.len() > 512
        || normalized.starts_with('/')
        || normalized.contains('\0')
    {
        return Err(DownloadError::InvalidRequest(
            "package file path is invalid".into(),
        ));
    }

    let mut segments = Vec::new();
    for segment in normalized.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(DownloadError::InvalidRequest(
                "package file path is invalid".into(),
            ));
        }
        segments.push(segment);
    }

    Ok(segments.join("/"))
}

fn package_dir_from_storage_key(
    package_root: &Path,
    storage_key: &str,
) -> Result<PathBuf, DownloadError> {
    let id = storage_key
        .strip_prefix("downloads/")
        .ok_or_else(|| DownloadError::StorageUnavailable("invalid package storage key".into()))?;
    Uuid::parse_str(id)
        .map_err(|_| DownloadError::StorageUnavailable("invalid package storage key".into()))?;
    let package_dir = package_root.join(id);
    if !package_dir.starts_with(package_root) {
        return Err(DownloadError::StorageUnavailable(
            "invalid package storage key".into(),
        ));
    }
    Ok(package_dir)
}

fn resolve_package_file_path(
    package_dir: &Path,
    relative_path: &str,
) -> Result<PathBuf, DownloadError> {
    let relative_path = normalize_package_relative_path(relative_path)?;
    let mut path = package_dir.to_path_buf();
    for segment in relative_path.split('/') {
        path.push(segment);
    }
    if !path.starts_with(package_dir) {
        return Err(DownloadError::InvalidRequest(
            "package file path is invalid".into(),
        ));
    }
    Ok(path)
}

fn authenticated_package_file_url(
    package_id: Uuid,
    relative_path: &str,
    device_identifier: &str,
) -> String {
    format!(
        "/api/v1/downloads/packages/{package_id}/files/{}?device_identifier={}",
        encode_relative_package_url_path(relative_path),
        urlencoding::encode(device_identifier)
    )
}

fn encode_relative_package_url_path(relative_path: &str) -> String {
    relative_path
        .split('/')
        .map(urlencoding::encode)
        .collect::<Vec<_>>()
        .join("/")
}

fn job_response_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<DownloadJobResponse, DownloadError> {
    let status: String = row.get("status");
    let package_format: String = row.get("package_format");
    let quality_mode: String = row.get("quality_mode");

    Ok(DownloadJobResponse {
        id: row.get("id"),
        media_item_id: row.get("media_item_id"),
        media_file_id: row.try_get("media_file_id").ok().flatten(),
        device_identifier: row.get("device_identifier"),
        status: job_status_from_db(&status)?,
        package_format: package_format_from_db(&package_format)?,
        quality_mode: quality_mode_from_db(&quality_mode)?,
        progress_percent: row.get("progress_percent"),
        bytes_expected: row.try_get("bytes_expected").ok().flatten(),
        bytes_prepared: row.get("bytes_prepared"),
        failure_reason: row.try_get("failure_reason").ok().flatten(),
        expires_at: row.try_get("expires_at").ok().flatten(),
    })
}

fn source_file_response(source: &DownloadSourceCandidate) -> DownloadSourceFileResponse {
    DownloadSourceFileResponse {
        id: source.id,
        file_size: source.file_size,
        container_format: source.container_format.clone(),
        video_codec: source.video_codec.clone(),
        video_resolution: source.video_resolution.clone(),
        video_bitrate: source.video_bitrate,
        audio_codec: source.audio_codec.clone(),
        audio_channels: source.audio_channels,
        audio_language: source.audio_language.clone(),
        runtime_seconds: source.runtime_seconds,
    }
}

fn is_mobile_direct_mp4(source: &DownloadSourceCandidate) -> bool {
    let container = source.container_format.to_lowercase();
    let video = source
        .video_codec
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();
    let audio = source
        .audio_codec
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();
    container == "mp4"
        && matches!(video.as_str(), "h264" | "avc" | "avc1" | "hevc" | "h265")
        && matches!(audio.as_str(), "aac" | "ac3" | "eac3" | "mp3" | "")
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

fn sha256_hex(input: &str) -> String {
    let result = digest(&SHA256, input.as_bytes());
    result
        .as_ref()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect()
}

async fn authorize_download_request(
    state: &AppState,
    user: &AuthenticatedUser,
    media_item_id: Uuid,
    device_identifier: &str,
    _platform: DownloadClientPlatform,
) -> Result<(), DownloadError> {
    let config = state.runtime_config.load();
    let downloads = config.downloads.clone();
    let network_mode = config.auth.network_mode.clone();
    drop(config);

    if let Err(err) = validate_network_policy(&downloads, &network_mode) {
        if matches!(&err, DownloadError::PolicyDenied(_)) {
            record_download_event(
                &state.pool,
                user,
                Some(media_item_id),
                Some(device_identifier),
                "policy_denied",
                Some(&err.to_string()),
            )
            .await?;
        }
        return Err(err);
    }

    resolve_media_access(&state.pool, user, media_item_id).await?;
    enforce_streaming_policy(&state.pool, user, media_item_id, device_identifier).await?;
    enforce_quota_policy(&state.pool, user, device_identifier, &downloads).await?;
    Ok(())
}

fn validate_network_policy(
    downloads: &DownloadsConfig,
    network_mode: &NetworkMode,
) -> Result<(), DownloadError> {
    if !downloads.enabled {
        return Err(DownloadError::PolicyDenied(
            "offline downloads are disabled by server policy".into(),
        ));
    }

    match network_mode {
        NetworkMode::Local if !downloads.allow_lan_downloads => Err(DownloadError::PolicyDenied(
            "offline downloads are disabled for LAN/local mode".into(),
        )),
        NetworkMode::Exposed if !downloads.allow_remote_downloads => {
            Err(DownloadError::PolicyDenied(
                "offline downloads are disabled for remote/exposed mode".into(),
            ))
        }
        _ => Ok(()),
    }
}

async fn resolve_media_access(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
    media_item_id: Uuid,
) -> Result<(), DownloadError> {
    let row = sqlx::query(
        "SELECT mi.library_id, \
                EXISTS ( \
                    SELECT 1 FROM media_files mf \
                    WHERE mf.media_item_id = mi.id AND mf.is_healthy = true \
                ) AS has_healthy_file \
         FROM media_items mi \
         WHERE mi.id = $1",
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Err(DownloadError::UnsupportedMedia(
            "media item is unavailable".into(),
        ));
    };

    let library_id: Uuid = row.get("library_id");
    let has_healthy_file: bool = row.get("has_healthy_file");

    if !has_healthy_file {
        return Err(DownloadError::UnsupportedMedia(
            "no healthy media file is available for download".into(),
        ));
    }

    if !user.has_all_library_access {
        let has_library_access = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS ( \
                SELECT 1 FROM user_library_access \
                WHERE user_id = $1 AND library_id = $2 \
            )",
        )
        .bind(user.user_id)
        .bind(library_id)
        .fetch_one(pool)
        .await?;

        if !has_library_access {
            return Err(DownloadError::AccessDenied);
        }
    }

    Ok(())
}

async fn enforce_quota_policy(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
    device_identifier: &str,
    downloads: &DownloadsConfig,
) -> Result<(), DownloadError> {
    let active_user_jobs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM download_jobs \
         WHERE user_id = $1 AND status IN ('queued', 'preparing')",
    )
    .bind(user.user_id)
    .fetch_one(pool)
    .await?;
    if active_user_jobs >= i64::from(downloads.max_active_jobs_per_user) {
        record_download_event(
            pool,
            user,
            None,
            Some(device_identifier),
            "quota_denied",
            Some("active download job limit reached for this user"),
        )
        .await?;
        return Err(DownloadError::QuotaExceeded(
            "active download job limit reached for this user".into(),
        ));
    }

    let active_device_jobs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM download_jobs \
         WHERE user_id = $1 AND device_identifier = $2 AND status IN ('queued', 'preparing')",
    )
    .bind(user.user_id)
    .bind(device_identifier)
    .fetch_one(pool)
    .await?;
    if active_device_jobs >= i64::from(downloads.max_active_jobs_per_device) {
        record_download_event(
            pool,
            user,
            None,
            Some(device_identifier),
            "quota_denied",
            Some("active download job limit reached for this device"),
        )
        .await?;
        return Err(DownloadError::QuotaExceeded(
            "active download job limit reached for this device".into(),
        ));
    }

    let retained_user_packages: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM download_packages \
         WHERE user_id = $1 AND status IN ('ready', 'serving')",
    )
    .bind(user.user_id)
    .fetch_one(pool)
    .await?;
    if retained_user_packages >= i64::from(downloads.max_retained_packages_per_user) {
        record_download_event(
            pool,
            user,
            None,
            Some(device_identifier),
            "quota_denied",
            Some("retained package limit reached for this user"),
        )
        .await?;
        return Err(DownloadError::QuotaExceeded(
            "retained package limit reached for this user".into(),
        ));
    }

    let retained_device_packages: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM download_packages \
         WHERE user_id = $1 AND device_identifier = $2 AND status IN ('ready', 'serving')",
    )
    .bind(user.user_id)
    .bind(device_identifier)
    .fetch_one(pool)
    .await?;
    if retained_device_packages >= i64::from(downloads.max_retained_packages_per_device) {
        record_download_event(
            pool,
            user,
            None,
            Some(device_identifier),
            "quota_denied",
            Some("retained package limit reached for this device"),
        )
        .await?;
        return Err(DownloadError::QuotaExceeded(
            "retained package limit reached for this device".into(),
        ));
    }

    let user_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(total_bytes), 0)::BIGINT FROM download_packages \
         WHERE user_id = $1 AND status IN ('ready', 'serving')",
    )
    .bind(user.user_id)
    .fetch_one(pool)
    .await?;
    if user_bytes >= downloads.max_bytes_per_user {
        record_download_event(
            pool,
            user,
            None,
            Some(device_identifier),
            "quota_denied",
            Some("download byte quota reached for this user"),
        )
        .await?;
        return Err(DownloadError::QuotaExceeded(
            "download byte quota reached for this user".into(),
        ));
    }

    let device_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(total_bytes), 0)::BIGINT FROM download_packages \
         WHERE user_id = $1 AND device_identifier = $2 AND status IN ('ready', 'serving')",
    )
    .bind(user.user_id)
    .bind(device_identifier)
    .fetch_one(pool)
    .await?;
    if device_bytes >= downloads.max_bytes_per_device {
        record_download_event(
            pool,
            user,
            None,
            Some(device_identifier),
            "quota_denied",
            Some("download byte quota reached for this device"),
        )
        .await?;
        return Err(DownloadError::QuotaExceeded(
            "download byte quota reached for this device".into(),
        ));
    }

    Ok(())
}

async fn enforce_streaming_policy(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
    media_item_id: Uuid,
    device_identifier: &str,
) -> Result<(), DownloadError> {
    let row = sqlx::query(
        "SELECT streaming_policy_id, max_streams, max_transcode_streams, bandwidth_limit_bps \
         FROM users WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(user.user_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Err(DownloadError::AccessDenied);
    };

    let limits = crate::domains::playback::service::resolve_streaming_limits(
        pool,
        user.user_id,
        row.try_get("max_streams").ok().flatten(),
        row.try_get("max_transcode_streams").ok().flatten(),
        row.try_get("bandwidth_limit_bps").ok().flatten(),
        row.try_get("streaming_policy_id").ok().flatten(),
    )
    .await
    .map_err(|err| match err {
        crate::domains::playback::PlaybackError::PolicyNotFound => {
            DownloadError::PolicyDenied("assigned streaming policy is unavailable".into())
        }
        crate::domains::playback::PlaybackError::Database(err) => DownloadError::Database(err),
        _ => DownloadError::PolicyDenied("streaming policy could not be resolved".into()),
    })?;

    if !limits.allow_direct_play && !limits.allow_direct_stream && !limits.allow_transcode {
        record_download_event(
            pool,
            user,
            Some(media_item_id),
            Some(device_identifier),
            "policy_denied",
            Some("streaming policy disallows playback delivery"),
        )
        .await?;
        return Err(DownloadError::PolicyDenied(
            "streaming policy disallows playback delivery".into(),
        ));
    }

    Ok(())
}

async fn ensure_job_owner(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
    id: Uuid,
) -> Result<(), DownloadError> {
    let owner_id: Option<Uuid> =
        sqlx::query_scalar("SELECT user_id FROM download_jobs WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;

    match owner_id {
        Some(owner_id) if owner_id == user.user_id => Ok(()),
        _ => Err(DownloadError::JobNotFound(id)),
    }
}

async fn ensure_package_owner(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
    id: Uuid,
) -> Result<(), DownloadError> {
    let owner_id: Option<Uuid> =
        sqlx::query_scalar("SELECT user_id FROM download_packages WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;

    match owner_id {
        Some(owner_id) if owner_id == user.user_id => Ok(()),
        _ => Err(DownloadError::PackageNotFound(id)),
    }
}

async fn record_download_event(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
    media_item_id: Option<Uuid>,
    device_identifier: Option<&str>,
    event_type: &str,
    reason: Option<&str>,
) -> Result<(), DownloadError> {
    sqlx::query(
        "INSERT INTO download_events \
         (user_id, user_session_id, media_item_id, device_identifier, event_type, reason, details) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(user.user_id)
    .bind(user.session_id)
    .bind(media_item_id)
    .bind(device_identifier)
    .bind(event_type)
    .bind(reason)
    .bind(json!({ "source": "downloads_policy" }))
    .execute(pool)
    .await?;

    Ok(())
}

async fn record_download_event_for_package(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
    download_job_id: Option<Uuid>,
    download_package_id: Option<Uuid>,
    media_item_id: Option<Uuid>,
    device_identifier: Option<&str>,
    event_type: &str,
    reason: Option<&str>,
) -> Result<(), DownloadError> {
    sqlx::query(
        "INSERT INTO download_events \
         (user_id, user_session_id, download_job_id, download_package_id, media_item_id, \
          device_identifier, event_type, reason, details) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(user.user_id)
    .bind(user.session_id)
    .bind(download_job_id)
    .bind(download_package_id)
    .bind(media_item_id)
    .bind(device_identifier)
    .bind(event_type)
    .bind(reason)
    .bind(json!({ "source": "downloads_serving" }))
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(overrides: impl FnOnce(&mut DownloadSourceCandidate)) -> DownloadSourceCandidate {
        let mut source = DownloadSourceCandidate {
            id: Uuid::nil(),
            updated_at: Utc::now(),
            file_size: 4_000_000_000,
            file_hash: Some("hash".to_string()),
            container_format: "mp4".to_string(),
            video_codec: Some("h264".to_string()),
            video_resolution: Some("1920x1080".to_string()),
            video_bitrate: Some(6_000_000),
            audio_codec: Some("aac".to_string()),
            audio_channels: Some(2),
            audio_language: Some("en".to_string()),
            audio_bitrate: Some(192_000),
            runtime_seconds: 3600,
            additional_streams: json!({}),
        };
        overrides(&mut source);
        source
    }

    #[test]
    fn resolution_height_parses_common_shapes() {
        assert_eq!(resolution_height("1920x1080"), Some(1080));
        assert_eq!(resolution_height("720p"), Some(720));
        assert_eq!(resolution_height("4K"), Some(2160));
        assert_eq!(resolution_height("unknown"), None);
    }

    #[test]
    fn direct_mobile_mp4_can_use_single_file_package() {
        let source = source(|_| {});
        let target = resolve_quality_target(
            DownloadQualityMode::Maximum,
            source.video_resolution.as_deref(),
            "1080p",
            6_000_000,
        );

        assert!(matches!(
            choose_package_format(&source, &target),
            DownloadPackageFormat::Mp4
        ));
        assert_eq!(
            choose_package_strategy(&source, &target, DownloadPackageFormat::Mp4),
            "direct_copy"
        );
    }

    #[test]
    fn non_mp4_compatible_source_uses_hls_remux_or_transcode() {
        let source = source(|source| {
            source.container_format = "mkv".to_string();
        });
        let target = resolve_quality_target(
            DownloadQualityMode::Maximum,
            source.video_resolution.as_deref(),
            "1080p",
            6_000_000,
        );

        assert!(matches!(
            choose_package_format(&source, &target),
            DownloadPackageFormat::HlsFmp4
        ));
        assert_eq!(
            choose_package_strategy(&source, &target, DownloadPackageFormat::HlsFmp4),
            "remux"
        );
    }

    #[test]
    fn downscaled_data_saver_estimates_transcoded_bytes() {
        let source = source(|_| {});
        let target = resolve_quality_target(
            DownloadQualityMode::DataSaver,
            source.video_resolution.as_deref(),
            "1080p",
            6_000_000,
        );
        let strategy = choose_package_strategy(&source, &target, DownloadPackageFormat::HlsFmp4);

        assert_eq!(strategy, "transcode");
        assert_eq!(target.resolution.as_deref(), Some("480p"));
        assert_eq!(
            estimate_package_bytes(&source, &target, &strategy),
            Some(((1_500_000 + 192_000) * 3600) / 8)
        );
    }

    #[test]
    fn package_format_from_db_accepts_manifest_formats() {
        assert!(matches!(
            package_format_from_db("hls_fmp4").unwrap(),
            DownloadPackageFormat::HlsFmp4
        ));
        assert!(matches!(
            package_format_from_db("mp4").unwrap(),
            DownloadPackageFormat::Mp4
        ));
        assert!(package_format_from_db("zip").is_err());
    }

    #[test]
    fn download_range_spec_parses_prefix_and_suffix_ranges() {
        let range = DownloadRangeSpec::parse(Some("bytes=10-19"), 100)
            .unwrap()
            .unwrap();
        assert_eq!(range.start, 10);
        assert_eq!(range.end, 19);
        assert_eq!(range.content_length(), 10);
        assert_eq!(range.content_range_header(), "bytes 10-19/100");

        let suffix = DownloadRangeSpec::parse(Some("bytes=-25"), 100)
            .unwrap()
            .unwrap();
        assert_eq!(suffix.start, 75);
        assert_eq!(suffix.end, 99);
    }

    #[test]
    fn download_range_spec_rejects_multi_ranges_and_bounds() {
        assert!(matches!(
            DownloadRangeSpec::parse(Some("bytes=0-1,4-5"), 100),
            Err(DownloadError::InvalidByteRange(_))
        ));
        assert!(matches!(
            DownloadRangeSpec::parse(Some("bytes=120-130"), 100),
            Err(DownloadError::InvalidByteRange(_))
        ));
    }

    #[test]
    fn package_relative_paths_reject_traversal() {
        assert_eq!(
            normalize_package_relative_path("subtitles/en.vtt").unwrap(),
            "subtitles/en.vtt"
        );
        assert!(matches!(
            normalize_package_relative_path("../media.mp4"),
            Err(DownloadError::InvalidRequest(_))
        ));
        assert!(matches!(
            normalize_package_relative_path("segments/..%2Fmedia.mp4"),
            Err(DownloadError::InvalidRequest(_))
        ));
        assert!(matches!(
            normalize_package_relative_path("/absolute.mp4"),
            Err(DownloadError::InvalidRequest(_))
        ));
    }
}
