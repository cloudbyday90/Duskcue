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

pub async fn get_download_plan(
    state: &AppState,
    _user: &AuthenticatedUser,
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
    authorize_download_request(state, _user, media_item_id, device_identifier, platform).await?;

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
            _user,
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
    authorize_download_request(
        state,
        user,
        req.media_item_id,
        &req.device_identifier,
        req.client_platform,
    )
    .await?;
    Err(DownloadError::NotImplemented("download job creation"))
}

pub async fn get_download_job(
    state: &AppState,
    user: &AuthenticatedUser,
    id: Uuid,
) -> Result<DownloadJobResponse, DownloadError> {
    ensure_job_owner(&state.pool, user, id).await?;
    Err(DownloadError::NotImplemented("download job status"))
}

pub async fn cancel_download_job(
    state: &AppState,
    user: &AuthenticatedUser,
    id: Uuid,
    _req: CancelDownloadJobRequest,
) -> Result<DownloadActionResponse, DownloadError> {
    ensure_job_owner(&state.pool, user, id).await?;
    Err(DownloadError::NotImplemented("download job cancellation"))
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
) -> Result<DownloadPackageManifestResponse, DownloadError> {
    ensure_package_owner(&state.pool, user, id).await?;
    Err(DownloadError::NotImplemented("download package manifest"))
}

pub async fn create_package_transfer_urls(
    state: &AppState,
    user: &AuthenticatedUser,
    id: Uuid,
    _req: PackageTransferUrlsRequest,
) -> Result<PackageTransferUrlsResponse, DownloadError> {
    ensure_package_owner(&state.pool, user, id).await?;
    Err(DownloadError::NotImplemented(
        "download package transfer URLs",
    ))
}

pub async fn serve_package_file(
    state: &AppState,
    user: &AuthenticatedUser,
    id: Uuid,
    _file_path: String,
) -> Result<(), DownloadError> {
    ensure_package_owner(&state.pool, user, id).await?;
    Err(DownloadError::NotImplemented(
        "download package file serving",
    ))
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
}
