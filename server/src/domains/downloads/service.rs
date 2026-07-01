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

use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::extractors::AuthenticatedUser;
use crate::state::{AppState, DownloadsConfig, NetworkMode};

use super::error::DownloadError;
use super::types::*;

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
    Err(DownloadError::NotImplemented("download planning"))
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
