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

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use tokio::net::lookup_host;
use tokio::task::JoinSet;
use url::Url;
use uuid::Uuid;

use crate::state::AppState;
use crate::state::NetworkMode;
use crate::workers::migration_runner;

use super::error::MigrationError;
use super::types::*;

const ACTIVE_STATUSES: &[&str] = &["discovering", "matching", "importing"];
pub const MAX_PLEX_DATABASE_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const API_CONFIG_TIMEOUT_SECONDS: u64 = 10;
const API_CONFIG_MAX_RESPONSE_BYTES: u64 = 1024 * 1024;
const API_PAGE_SIZE: i64 = 100;
const API_EXTRACTION_CONCURRENCY: usize = 4;

pub struct PlexUploadTarget {
    pub temp_path: PathBuf,
    final_path: PathBuf,
    original_filename: String,
}

pub fn validate_platform(value: &str) -> Result<(), MigrationError> {
    if VALID_MIGRATION_PLATFORMS.contains(&value) {
        Ok(())
    } else {
        Err(MigrationError::InvalidSourceConfiguration(format!(
            "invalid platform: {value}"
        )))
    }
}

pub fn validate_status(value: &str) -> Result<(), MigrationError> {
    if VALID_MIGRATION_STATUSES.contains(&value) {
        Ok(())
    } else {
        Err(MigrationError::InvalidSourceConfiguration(format!(
            "invalid status: {value}"
        )))
    }
}

pub async fn create_migration_source(
    state: &AppState,
    request: CreateMigrationSourceRequest,
) -> Result<MigrationSourceResponse, MigrationError> {
    validate_platform(&request.platform)?;
    let connection_config =
        sanitize_connection_config(state, &request.platform, request.connection_config).await?;

    let row = sqlx::query(
        r#"
        INSERT INTO migration_sources (platform, name, connection_config)
        VALUES ($1, $2, $3)
        RETURNING id, created_at, platform, name, connection_config, last_run_at, status
        "#,
    )
    .bind(request.platform)
    .bind(request.name)
    .bind(connection_config)
    .fetch_one(&state.pool)
    .await?;

    Ok(row_to_source_response(&row))
}

pub async fn list_migration_sources(
    state: &AppState,
    query: ListMigrationSourcesQuery,
    page: u32,
    page_size: u32,
) -> Result<MigrationSourceListResponse, MigrationError> {
    if let Some(platform) = query.platform.as_deref() {
        validate_platform(platform)?;
    }
    if let Some(status) = query.status.as_deref() {
        validate_status(status)?;
    }

    let mut builder = sqlx::QueryBuilder::new(
        "SELECT id, created_at, platform, name, connection_config, last_run_at, status FROM migration_sources",
    );
    push_source_filters(&mut builder, &query);
    builder.push(" ORDER BY created_at DESC");
    let limit = page_size.max(1) as i64;
    let offset = (page.saturating_sub(1) as i64) * limit;
    builder.push(" LIMIT ").push_bind(limit);
    builder.push(" OFFSET ").push_bind(offset);

    let rows = builder.build().fetch_all(&state.pool).await?;
    let items = rows.iter().map(row_to_source_response).collect();

    let mut count_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM migration_sources");
    push_source_filters(&mut count_builder, &query);
    let total: i64 = count_builder.build().fetch_one(&state.pool).await?.get(0);

    Ok(MigrationSourceListResponse {
        items,
        total,
        page,
        page_size,
        total_pages: ((total as f64) / (page_size as f64)).ceil() as u32,
    })
}

pub async fn get_migration_source(
    state: &AppState,
    id: Uuid,
) -> Result<MigrationSourceResponse, MigrationError> {
    get_source(state, id).await
}

pub async fn delete_migration_source(state: &AppState, id: Uuid) -> Result<(), MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;

    let deleted = sqlx::query("DELETE FROM migration_sources WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?
        .rows_affected();

    if deleted == 0 {
        return Err(MigrationError::NotFound(id));
    }

    Ok(())
}

pub async fn test_connection(
    state: &AppState,
    id: Uuid,
    request: MigrationSourceCredentialRequest,
) -> Result<MigrationActionResponse, MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;

    if matches!(source.platform.as_str(), "jellyfin" | "emby") {
        let client = build_api_migration_client(&source, &request)?;
        let info = client.get_json("/System/Info", &[]).await?;
        let version = info
            .get("Version")
            .and_then(Value::as_str)
            .or_else(|| info.get("version").and_then(Value::as_str))
            .unwrap_or("unknown");
        return Ok(MigrationActionResponse {
            migration_source_id: id,
            status: source.status,
            message: format!(
                "{} connection verified; source version {version}",
                source.platform
            ),
        });
    }

    Ok(MigrationActionResponse {
        migration_source_id: id,
        status: source.status,
        message:
            "Migration source is registered; Plex connection checks are handled by the upload task"
                .to_string(),
    })
}

pub async fn prepare_plex_upload(
    state: &AppState,
    id: Uuid,
    original_filename: &str,
) -> Result<PlexUploadTarget, MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;
    if source.platform != "plex" {
        return Err(MigrationError::InvalidSourceConfiguration(
            "Plex database upload is only valid for Plex migration sources".to_string(),
        ));
    }
    validate_plex_database_filename(original_filename)?;

    let upload_dir = plex_upload_dir(state, id);
    tokio::fs::create_dir_all(&upload_dir)
        .await
        .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;

    let temp_path = upload_dir.join("plex.db.uploading");
    let final_path = upload_dir.join("plex.db");
    let _ = tokio::fs::remove_file(&temp_path).await;

    Ok(PlexUploadTarget {
        temp_path,
        final_path,
        original_filename: original_filename.to_string(),
    })
}

pub async fn complete_plex_upload(
    state: &AppState,
    id: Uuid,
    target: PlexUploadTarget,
    file_size_bytes: u64,
) -> Result<MigrationActionResponse, MigrationError> {
    if let Err(error) = validate_plex_upload_size(file_size_bytes)
        .and_then(|_| ensure_plex_upload_disk_space(state, file_size_bytes))
        .and_then(|_| validate_plex_database_file(&target.temp_path, file_size_bytes))
    {
        let _ = tokio::fs::remove_file(&target.temp_path).await;
        return Err(error);
    }

    tokio::fs::rename(&target.temp_path, &target.final_path)
        .await
        .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;

    let stored_path = target.final_path.to_string_lossy().to_string();
    let config_patch = json!({
        "method": "sqlite_upload",
        "original_filename": target.original_filename,
        "uploaded_at": Utc::now(),
        "file_size_bytes": file_size_bytes,
        "stored_path": stored_path,
        "credential_mode": "none",
        "validation": {
            "max_file_size_bytes": MAX_PLEX_DATABASE_BYTES,
            "requires_sqlite_header": true,
            "required_tables": ["accounts", "metadata_items", "metadata_item_settings"],
            "validated_at": Utc::now(),
        },
    });

    let row = sqlx::query(
        r#"
        UPDATE migration_sources
        SET connection_config = connection_config || $2::jsonb,
            status = 'pending',
            last_run_at = COALESCE(last_run_at, now())
        WHERE id = $1
        RETURNING status
        "#,
    )
    .bind(id)
    .bind(config_patch)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(MigrationError::NotFound(id))?;

    Ok(MigrationActionResponse {
        migration_source_id: id,
        status: row.get("status"),
        message: format!("Plex database uploaded and validated ({file_size_bytes} bytes)"),
    })
}

async fn sanitize_connection_config(
    state: &AppState,
    platform: &str,
    config: Value,
) -> Result<Value, MigrationError> {
    match platform {
        "jellyfin" | "emby" => sanitize_api_connection_config(state, platform, config).await,
        "plex" => sanitize_plex_connection_config(state, config),
        _ => Err(MigrationError::InvalidSourceConfiguration(format!(
            "invalid platform: {platform}"
        ))),
    }
}

async fn sanitize_api_connection_config(
    state: &AppState,
    platform: &str,
    config: Value,
) -> Result<Value, MigrationError> {
    let parsed: ApiMigrationConnectionConfig = serde_json::from_value(config)
        .map_err(|e| MigrationError::InvalidSourceConfiguration(e.to_string()))?;

    let method = parsed.method.as_deref().unwrap_or("api");
    if method != "api" {
        return Err(MigrationError::InvalidSourceConfiguration(
            "Jellyfin and Emby migrations require method = api".to_string(),
        ));
    }

    let base_url = normalize_and_validate_api_base_url(state, &parsed.base_url).await?;
    let (api_key_hash, api_key_prefix) = normalize_api_key_material(parsed)?;

    Ok(json!({
        "method": "api",
        "base_url": base_url,
        "api_key_hash": api_key_hash,
        "api_key_prefix": api_key_prefix,
        "credential_mode": "hash_only",
        "auth_header": "X-Emby-Token",
        "ssrf_policy": {
            "redirects": "blocked",
            "timeout_seconds": API_CONFIG_TIMEOUT_SECONDS,
            "max_response_bytes": API_CONFIG_MAX_RESPONSE_BYTES,
            "private_networks": private_network_policy_label(state),
        },
        "source_platform": platform,
    }))
}

fn sanitize_plex_connection_config(
    state: &AppState,
    config: Value,
) -> Result<Value, MigrationError> {
    let parsed: PlexMigrationConnectionConfig = serde_json::from_value(config)
        .map_err(|e| MigrationError::InvalidSourceConfiguration(e.to_string()))?;

    let method = parsed.method.unwrap_or_else(|| "sqlite_upload".to_string());
    if method != "sqlite_upload" {
        return Err(MigrationError::InvalidSourceConfiguration(
            "Plex migrations require method = sqlite_upload".to_string(),
        ));
    }

    let original_filename = parsed
        .original_filename
        .unwrap_or_else(|| "com.plexapp.plugins.library.db".to_string());
    validate_plex_database_filename(&original_filename)?;

    let file_size_bytes = parsed.file_size_bytes.unwrap_or(0);
    if file_size_bytes > 0 {
        validate_plex_upload_size(file_size_bytes)?;
        ensure_plex_upload_disk_space(state, file_size_bytes)?;
    }

    Ok(json!({
        "method": "sqlite_upload",
        "original_filename": original_filename,
        "uploaded_at": parsed.uploaded_at.unwrap_or_else(Utc::now),
        "file_size_bytes": file_size_bytes,
        "credential_mode": "none",
        "validation": {
            "max_file_size_bytes": MAX_PLEX_DATABASE_BYTES,
            "requires_sqlite_header": true,
            "required_tables": ["accounts", "metadata_items", "metadata_item_settings"],
        },
    }))
}

async fn normalize_and_validate_api_base_url(
    state: &AppState,
    raw_url: &str,
) -> Result<String, MigrationError> {
    let mut parsed = Url::parse(raw_url.trim()).map_err(|_| {
        MigrationError::InvalidSourceConfiguration("base_url must be a valid URL".to_string())
    })?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(MigrationError::InvalidSourceConfiguration(
            "base_url must use http or https".to_string(),
        ));
    }

    if parsed.host_str().is_none() {
        return Err(MigrationError::InvalidSourceConfiguration(
            "base_url must include a host".to_string(),
        ));
    }

    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(MigrationError::InvalidSourceConfiguration(
            "base_url must not include credentials".to_string(),
        ));
    }

    if parsed.fragment().is_some() {
        return Err(MigrationError::InvalidSourceConfiguration(
            "base_url must not include a fragment".to_string(),
        ));
    }

    parsed.set_query(None);
    let trimmed_path = parsed.path().trim_end_matches('/').to_string();
    parsed.set_path(&trimmed_path);

    validate_api_host_policy(state, &parsed).await?;

    Ok(parsed.to_string().trim_end_matches('/').to_string())
}

async fn validate_api_host_policy(state: &AppState, url: &Url) -> Result<(), MigrationError> {
    let host = url.host_str().ok_or_else(|| {
        MigrationError::InvalidSourceConfiguration("base_url must include a host".to_string())
    })?;
    let port = url.port_or_known_default().ok_or_else(|| {
        MigrationError::InvalidSourceConfiguration("base_url must include a valid port".to_string())
    })?;

    if let Ok(ip) = host.parse::<IpAddr>() {
        validate_resolved_source_ip(state, ip)?;
        return Ok(());
    }

    let resolved = lookup_host((host, port)).await.map_err(|e| {
        MigrationError::InvalidSourceConfiguration(format!("base_url DNS lookup failed: {e}"))
    })?;

    let mut saw_address = false;
    for socket_addr in resolved {
        saw_address = true;
        validate_resolved_source_ip(state, socket_addr.ip())?;
    }

    if !saw_address {
        return Err(MigrationError::InvalidSourceConfiguration(
            "base_url host did not resolve".to_string(),
        ));
    }

    Ok(())
}

fn validate_resolved_source_ip(state: &AppState, ip: IpAddr) -> Result<(), MigrationError> {
    if is_always_blocked_source_ip(ip) {
        return Err(MigrationError::InvalidSourceConfiguration(
            "base_url resolves to an invalid or reserved address".to_string(),
        ));
    }

    if is_metadata_service_ip(ip) {
        return Err(MigrationError::InvalidSourceConfiguration(
            "base_url resolves to a cloud metadata service address".to_string(),
        ));
    }

    let config = state.runtime_config.load();
    if matches!(config.auth.network_mode, NetworkMode::Exposed) && is_blocked_source_ip(ip) {
        return Err(MigrationError::InvalidSourceConfiguration(
            "base_url resolves to a private, loopback, link-local, or reserved address while network_mode is exposed".to_string(),
        ));
    }

    Ok(())
}

fn normalize_api_key_material(
    parsed: ApiMigrationConnectionConfig,
) -> Result<(String, String), MigrationError> {
    if let Some(api_key) = parsed.api_key {
        let trimmed = api_key.trim();
        if trimmed.len() < 8 {
            return Err(MigrationError::InvalidSourceConfiguration(
                "api_key must be at least 8 characters".to_string(),
            ));
        }
        let hash = sha256_hex(trimmed);
        let prefix: String = trimmed.chars().take(4).collect();
        return Ok((format!("sha256:{hash}"), prefix));
    }

    if let (Some(hash), Some(prefix)) = (parsed.api_key_hash, parsed.api_key_prefix) {
        if !hash.starts_with("sha256:") || hash.len() != "sha256:".len() + 64 {
            return Err(MigrationError::InvalidSourceConfiguration(
                "api_key_hash must be a sha256 hash".to_string(),
            ));
        }
        if prefix.is_empty() || prefix.len() > 16 {
            return Err(MigrationError::InvalidSourceConfiguration(
                "api_key_prefix is invalid".to_string(),
            ));
        }
        return Ok((hash, prefix));
    }

    Err(MigrationError::InvalidSourceConfiguration(
        "api_key is required for API-based migration sources".to_string(),
    ))
}

fn validate_plex_database_filename(filename: &str) -> Result<(), MigrationError> {
    if filename != "com.plexapp.plugins.library.db" {
        return Err(MigrationError::InvalidPlexDatabase(
            "expected com.plexapp.plugins.library.db".to_string(),
        ));
    }
    Ok(())
}

fn validate_plex_upload_size(file_size_bytes: u64) -> Result<(), MigrationError> {
    if file_size_bytes > MAX_PLEX_DATABASE_BYTES {
        return Err(MigrationError::PlexDatabaseTooLarge);
    }
    Ok(())
}

fn ensure_plex_upload_disk_space(
    state: &AppState,
    file_size_bytes: u64,
) -> Result<(), MigrationError> {
    let upload_dir = state.bootstrap.data_dir.join("migrations");
    let available =
        available_space_for_path(&upload_dir).ok_or(MigrationError::InsufficientDiskSpace)?;
    if available < file_size_bytes.saturating_mul(2) {
        return Err(MigrationError::InsufficientDiskSpace);
    }
    Ok(())
}

pub fn validate_plex_database_file(
    path: &Path,
    file_size_bytes: u64,
) -> Result<(), MigrationError> {
    validate_plex_upload_size(file_size_bytes)?;
    let mut header = [0u8; 16];
    let mut file = std::fs::File::open(path)
        .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;
    use std::io::Read;
    file.read_exact(&mut header)
        .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;
    if &header != b"SQLite format 3\0" {
        return Err(MigrationError::InvalidPlexDatabase(
            "file is not a SQLite 3 database".to_string(),
        ));
    }

    let conn =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;

    for table in ["accounts", "metadata_items", "metadata_item_settings"] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;
        if count == 0 {
            return Err(MigrationError::InvalidPlexDatabase(format!(
                "required table missing: {table}"
            )));
        }
    }

    Ok(())
}

fn available_space_for_path(path: &Path) -> Option<u64> {
    let resolved = resolve_existing_ancestor(path)?;
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|disk| resolved.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(|disk| disk.available_space())
}

fn plex_upload_dir(state: &AppState, id: Uuid) -> PathBuf {
    state
        .bootstrap
        .data_dir
        .join("migrations")
        .join(id.to_string())
}

fn plex_database_path(
    state: &AppState,
    id: Uuid,
    source: &MigrationSourceResponse,
) -> Result<PathBuf, MigrationError> {
    let configured = source
        .connection_config
        .get("stored_path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| plex_upload_dir(state, id).join("plex.db"));

    let upload_dir = plex_upload_dir(state, id);
    let canonical_dir = std::fs::canonicalize(&upload_dir)
        .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;
    let canonical_file = std::fs::canonicalize(&configured)
        .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;

    if !canonical_file.starts_with(&canonical_dir) {
        return Err(MigrationError::InvalidPlexDatabase(
            "stored Plex database path is outside the migration upload directory".to_string(),
        ));
    }

    Ok(canonical_file)
}

fn resolve_existing_ancestor(path: &Path) -> Option<std::path::PathBuf> {
    let mut current = path.to_path_buf();
    while !current.exists() {
        if !current.pop() {
            return None;
        }
    }
    std::fs::canonicalize(&current).ok().or(Some(current))
}

fn is_blocked_source_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || is_always_blocked_source_ip(ip)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || is_unique_local_v6(&v6)
                || is_link_local_v6(&v6)
                || is_always_blocked_source_ip(ip)
        }
    }
}

fn is_always_blocked_source_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_unspecified() || v4.is_broadcast() || v4.is_documentation() || v4.is_multicast()
        }
        IpAddr::V6(v6) => v6.is_unspecified() || v6.is_multicast(),
    }
}

fn is_metadata_service_ip(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V4(v4) if v4.octets() == [169, 254, 169, 254])
}

fn is_unique_local_v6(v6: &std::net::Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xfe00) == 0xfc00
}

fn is_link_local_v6(v6: &std::net::Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xffc0) == 0xfe80
}

fn private_network_policy_label(state: &AppState) -> &'static str {
    let config = state.runtime_config.load();
    match config.auth.network_mode {
        NetworkMode::Local => "allowed_in_local_mode",
        NetworkMode::Exposed => "blocked_in_exposed_mode",
    }
}

fn sha256_hex(input: &str) -> String {
    use ring::digest::{SHA256, digest};
    let result = digest(&SHA256, input.as_bytes());
    result.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_material_hashes_raw_key() {
        let config = ApiMigrationConnectionConfig {
            method: Some("api".to_string()),
            base_url: "http://example.test:8096".to_string(),
            api_key: Some("abcdef123456".to_string()),
            api_key_hash: None,
            api_key_prefix: None,
        };

        let (hash, prefix) = normalize_api_key_material(config).unwrap();

        assert!(hash.starts_with("sha256:"));
        assert_eq!(hash.len(), "sha256:".len() + 64);
        assert_eq!(prefix, "abcd");
    }

    #[test]
    fn api_key_material_rejects_short_raw_key() {
        let config = ApiMigrationConnectionConfig {
            method: Some("api".to_string()),
            base_url: "http://example.test:8096".to_string(),
            api_key: Some("short".to_string()),
            api_key_hash: None,
            api_key_prefix: None,
        };

        assert!(matches!(
            normalize_api_key_material(config),
            Err(MigrationError::InvalidSourceConfiguration(_))
        ));
    }

    #[test]
    fn plex_filename_must_match_canonical_database_name() {
        assert!(validate_plex_database_filename("com.plexapp.plugins.library.db").is_ok());
        assert!(matches!(
            validate_plex_database_filename("Library.db"),
            Err(MigrationError::InvalidPlexDatabase(_))
        ));
    }

    #[test]
    fn source_ip_policy_blocks_metadata_and_exposed_private_ranges() {
        assert!(is_metadata_service_ip(
            "169.254.169.254".parse::<IpAddr>().unwrap()
        ));
        assert!(is_blocked_source_ip(
            "192.168.1.10".parse::<IpAddr>().unwrap()
        ));
        assert!(!is_blocked_source_ip("8.8.8.8".parse::<IpAddr>().unwrap()));
        assert!(is_always_blocked_source_ip(
            "0.0.0.0".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn plex_database_file_validator_requires_sqlite_tables() {
        let path =
            std::env::temp_dir().join(format!("duskcue-plex-valid-{}.db", uuid::Uuid::now_v7()));
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute("CREATE TABLE accounts (id INTEGER, name TEXT)", [])
                .unwrap();
            conn.execute("CREATE TABLE metadata_items (id INTEGER, guid TEXT)", [])
                .unwrap();
            conn.execute(
                "CREATE TABLE metadata_item_settings (id INTEGER, guid TEXT)",
                [],
            )
            .unwrap();
        }

        let size = std::fs::metadata(&path).unwrap().len();
        assert!(validate_plex_database_file(&path, size).is_ok());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn plex_database_file_validator_rejects_non_sqlite_file() {
        let path =
            std::env::temp_dir().join(format!("duskcue-plex-invalid-{}.db", uuid::Uuid::now_v7()));
        std::fs::write(&path, b"not a sqlite database").unwrap();

        assert!(matches!(
            validate_plex_database_file(&path, 21),
            Err(MigrationError::InvalidPlexDatabase(_))
        ));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn provider_id_normalization_keeps_standard_ids_and_raw_payload() {
        let normalized = normalize_provider_ids(&json!({
            "Tmdb": "603",
            "Imdb": "tt0133093",
            "Tvdb": "12345",
            "Custom": "abc"
        }));

        assert_eq!(normalized["tmdb"], "603");
        assert_eq!(normalized["imdb"], "tt0133093");
        assert_eq!(normalized["tvdb"], "12345");
        assert_eq!(normalized["raw"]["Custom"], "abc");
    }

    #[test]
    fn source_watch_item_merge_preserves_watched_and_latest_progress() {
        let mapping_id = Uuid::now_v7();
        let mut first = SourceWatchItem {
            migration_user_mapping_id: mapping_id,
            source_item_id: "item-1".to_string(),
            source_item_title: "Example".to_string(),
            source_item_type: "movie".to_string(),
            source_item_year: Some(1999),
            source_provider_ids: json!({}),
            source_is_watched: false,
            source_play_count: 0,
            source_resume_position_ms: 120_000,
            source_last_played_at: parse_source_datetime(Some("2025-01-01T00:00:00Z")),
            source_item_metadata: json!({}),
        };
        let incoming = SourceWatchItem {
            migration_user_mapping_id: mapping_id,
            source_item_id: "item-1".to_string(),
            source_item_title: "Example".to_string(),
            source_item_type: "movie".to_string(),
            source_item_year: Some(1999),
            source_provider_ids: json!({}),
            source_is_watched: true,
            source_play_count: 2,
            source_resume_position_ms: 90_000,
            source_last_played_at: parse_source_datetime(Some("2025-02-01T00:00:00Z")),
            source_item_metadata: json!({}),
        };

        merge_source_watch_item(&mut first, &incoming);

        assert!(first.source_is_watched);
        assert_eq!(first.source_play_count, 2);
        assert_eq!(first.source_resume_position_ms, 0);
        assert_eq!(
            first.source_last_played_at,
            parse_source_datetime(Some("2025-02-01T00:00:00Z"))
        );
    }

    #[test]
    fn plex_guid_parser_extracts_external_provider_ids() {
        assert_eq!(
            parse_plex_provider_guid("com.plexapp.agents.imdb://tt0133093?lang=en"),
            Some(("imdb", "tt0133093".to_string()))
        );
        assert_eq!(
            parse_plex_provider_guid("com.plexapp.agents.themoviedb://603?lang=en"),
            Some(("tmdb", "603".to_string()))
        );
        assert_eq!(
            parse_plex_provider_guid("com.plexapp.agents.thetvdb://78874?lang=en"),
            Some(("tvdb", "78874".to_string()))
        );
        assert_eq!(
            parse_plex_provider_guid("plex://movie/5d776885e6d5c9001dcecb72"),
            None
        );
    }

    #[test]
    fn plex_sqlite_extraction_reads_users_watch_state_and_secondary_guids() {
        let path =
            std::env::temp_dir().join(format!("duskcue-plex-extract-{}.db", uuid::Uuid::now_v7()));
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE accounts (id INTEGER, name TEXT);
                CREATE TABLE metadata_items (
                    id INTEGER,
                    title TEXT,
                    metadata_type INTEGER,
                    year INTEGER,
                    guid TEXT,
                    parent_id INTEGER,
                    "index" INTEGER,
                    parent_index INTEGER
                );
                CREATE TABLE metadata_item_settings (
                    account_id INTEGER,
                    guid TEXT,
                    view_count INTEGER,
                    view_offset INTEGER,
                    last_viewed_at INTEGER
                );
                CREATE TABLE metadata_item_guids (
                    metadata_item_id INTEGER,
                    guid TEXT
                );
                INSERT INTO accounts (id, name) VALUES (1, 'DadPlex');
                INSERT INTO metadata_items (id, title, metadata_type, year, guid, parent_id, "index", parent_index)
                VALUES (10, 'The Matrix', 1, 1999, 'com.plexapp.agents.imdb://tt0133093?lang=en', NULL, NULL, NULL);
                INSERT INTO metadata_item_settings (account_id, guid, view_count, view_offset, last_viewed_at)
                VALUES (1, 'com.plexapp.agents.imdb://tt0133093?lang=en', 1, 12345, 1700000000);
                INSERT INTO metadata_item_guids (metadata_item_id, guid)
                VALUES (10, 'com.plexapp.agents.themoviedb://603?lang=en');
                "#,
            )
            .unwrap();
        }

        let users = extract_plex_users(&path).unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].source_user_id, "1");
        assert_eq!(users[0].source_user_name, "DadPlex");

        let mapping = SourceMapping {
            id: Uuid::now_v7(),
            source_user_id: "1".to_string(),
        };
        let items = extract_plex_watch_items(&path, &[mapping]).unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].source_item_title, "The Matrix");
        assert_eq!(items[0].source_item_type, "movie");
        assert!(items[0].source_is_watched);
        assert_eq!(items[0].source_resume_position_ms, 0);
        assert_eq!(items[0].source_play_count, 1);
        assert_eq!(items[0].source_provider_ids["imdb"], "tt0133093");
        assert_eq!(items[0].source_provider_ids["tmdb"], "603");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn user_mapping_validation_accepts_mapped_and_skipped_users() {
        let request = SaveUserMappingsRequest {
            mappings: vec![
                UserMappingRequest {
                    source_user_id: "source-1".to_string(),
                    source_user_name: "Source One".to_string(),
                    platform_user_id: Some(Uuid::now_v7()),
                    skip: Some(false),
                },
                UserMappingRequest {
                    source_user_id: "source-2".to_string(),
                    source_user_name: "Source Two".to_string(),
                    platform_user_id: None,
                    skip: Some(true),
                },
            ],
        };

        assert!(validate_mapping_conflicts(&request).is_ok());
        assert!(ensure_at_least_one_active_mapping(&request).is_ok());
    }

    #[test]
    fn user_mapping_validation_rejects_all_skipped_users() {
        let request = SaveUserMappingsRequest {
            mappings: vec![UserMappingRequest {
                source_user_id: "source-1".to_string(),
                source_user_name: "Source One".to_string(),
                platform_user_id: None,
                skip: Some(true),
            }],
        };

        assert!(validate_mapping_conflicts(&request).is_ok());
        assert!(matches!(
            ensure_at_least_one_active_mapping(&request),
            Err(MigrationError::NoUserMappings)
        ));
    }

    #[test]
    fn user_mapping_validation_rejects_skip_with_platform_user() {
        let request = SaveUserMappingsRequest {
            mappings: vec![UserMappingRequest {
                source_user_id: "source-1".to_string(),
                source_user_name: "Source One".to_string(),
                platform_user_id: Some(Uuid::now_v7()),
                skip: Some(true),
            }],
        };

        assert!(matches!(
            validate_mapping_conflicts(&request),
            Err(MigrationError::UserMappingConflict(_))
        ));
    }
}

pub async fn discover_source(
    state: &AppState,
    id: Uuid,
    request: MigrationSourceCredentialRequest,
) -> Result<MigrationDiscoveryResponse, MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;

    let result = match source.platform.as_str() {
        "plex" => {
            set_source_status(state, id, "discovering").await?;
            discover_plex_source(state, id, &source).await
        }
        "jellyfin" | "emby" => {
            let client = build_api_migration_client(&source, &request)?;
            set_source_status(state, id, "discovering").await?;
            discover_api_source(state, id, &client).await
        }
        _ => Err(MigrationError::InvalidSourceConfiguration(format!(
            "invalid platform: {}",
            source.platform
        ))),
    };
    let final_status = if result.is_ok() { "pending" } else { "failed" };
    let status = set_source_status(state, id, final_status).await?;

    let mut response = result?;
    response.status = status;
    Ok(response)
}

pub async fn get_user_mapping_options(
    state: &AppState,
    id: Uuid,
) -> Result<MigrationUserMappingOptionsResponse, MigrationError> {
    get_source(state, id).await?;

    let saved_rows = sqlx::query(
        r#"
        SELECT source_user_id, source_user_name, platform_user_id, status
        FROM migration_user_mapping
        WHERE migration_source_id = $1
        ORDER BY source_user_name, source_user_id
        "#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    let platform_rows = sqlx::query(
        r#"
        SELECT DISTINCT ON (u.id)
            u.id,
            u.username,
            u.display_name,
            u.email,
            u.status,
            i.display_name AS invitation_display_name,
            i.email AS invitation_email
        FROM users u
        LEFT JOIN invitations i ON i.user_id = u.id
        WHERE u.deleted_at IS NULL
        ORDER BY u.id, i.created_at DESC
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(MigrationUserMappingOptionsResponse {
        migration_source_id: id,
        saved_mappings: saved_rows.iter().map(row_to_saved_user_mapping).collect(),
        platform_users: platform_rows
            .iter()
            .map(row_to_platform_user_option)
            .collect(),
    })
}

async fn discover_api_source(
    state: &AppState,
    id: Uuid,
    client: &ApiMigrationClient,
) -> Result<MigrationDiscoveryResponse, MigrationError> {
    let source_users = client.fetch_source_users().await?;
    let mappings = load_source_mappings(state, id).await?;

    if mappings.is_empty() {
        return Ok(MigrationDiscoveryResponse {
            migration_source_id: id,
            status: "pending".to_string(),
            users_discovered: source_users.len(),
            users_mapped: 0,
            items_extracted: 0,
            items_inserted: 0,
            items_updated: 0,
            source_users,
            message: "Source users discovered; save user mappings before extracting watch data"
                .to_string(),
        });
    }

    let items = extract_api_watch_items(client, &mappings).await?;
    let (items_inserted, items_updated) = upsert_discovered_items(state, id, &items).await?;

    Ok(MigrationDiscoveryResponse {
        migration_source_id: id,
        status: "pending".to_string(),
        users_discovered: source_users.len(),
        users_mapped: mappings.len(),
        items_extracted: items.len(),
        items_inserted,
        items_updated,
        source_users,
        message: format!(
            "Extracted {} Jellyfin/Emby watch-state item(s)",
            items.len()
        ),
    })
}

async fn discover_plex_source(
    state: &AppState,
    id: Uuid,
    source: &MigrationSourceResponse,
) -> Result<MigrationDiscoveryResponse, MigrationError> {
    let db_path = plex_database_path(state, id, source)?;
    let users_path = db_path.clone();
    let source_users = tokio::task::spawn_blocking(move || extract_plex_users(&users_path))
        .await
        .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))??;
    let mappings = load_source_mappings(state, id).await?;

    if mappings.is_empty() {
        return Ok(MigrationDiscoveryResponse {
            migration_source_id: id,
            status: "pending".to_string(),
            users_discovered: source_users.len(),
            users_mapped: 0,
            items_extracted: 0,
            items_inserted: 0,
            items_updated: 0,
            source_users,
            message: "Plex users discovered; save user mappings before extracting watch data"
                .to_string(),
        });
    }

    let extract_path = db_path.clone();
    let extract_mappings = mappings.clone();
    let items = tokio::task::spawn_blocking(move || {
        extract_plex_watch_items(&extract_path, &extract_mappings)
    })
    .await
    .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))??;
    let (items_inserted, items_updated) = upsert_discovered_items(state, id, &items).await?;

    Ok(MigrationDiscoveryResponse {
        migration_source_id: id,
        status: "pending".to_string(),
        users_discovered: source_users.len(),
        users_mapped: mappings.len(),
        items_extracted: items.len(),
        items_inserted,
        items_updated,
        source_users,
        message: format!("Extracted {} Plex watch-state item(s)", items.len()),
    })
}

#[derive(Clone)]
struct ApiMigrationClient {
    platform: String,
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

#[derive(Debug, Clone)]
struct SourceMapping {
    id: Uuid,
    source_user_id: String,
}

#[derive(Debug, Clone)]
struct SourceWatchItem {
    migration_user_mapping_id: Uuid,
    source_item_id: String,
    source_item_title: String,
    source_item_type: String,
    source_item_year: Option<i32>,
    source_provider_ids: Value,
    source_is_watched: bool,
    source_play_count: i32,
    source_resume_position_ms: i64,
    source_last_played_at: Option<DateTime<Utc>>,
    source_item_metadata: Value,
}

#[derive(Debug, Deserialize)]
struct ApiUserDto {
    #[serde(default, rename = "Id")]
    id: Option<String>,
    #[serde(default, rename = "Name")]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiItemDto {
    #[serde(default, rename = "Id")]
    id: Option<String>,
    #[serde(default, rename = "Name")]
    name: Option<String>,
    #[serde(default, rename = "Type")]
    item_type: Option<String>,
    #[serde(default, rename = "ProductionYear")]
    production_year: Option<i32>,
    #[serde(default, rename = "ProviderIds")]
    provider_ids: Value,
    #[serde(default, rename = "UserData")]
    user_data: Option<ApiUserDataDto>,
    #[serde(default, rename = "SeriesName")]
    series_name: Option<String>,
    #[serde(default, rename = "ParentIndexNumber")]
    season_number: Option<i32>,
    #[serde(default, rename = "IndexNumber")]
    episode_number: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct ApiUserDataDto {
    #[serde(default, rename = "Played")]
    played: Option<bool>,
    #[serde(default, rename = "PlayCount")]
    play_count: Option<i32>,
    #[serde(default, rename = "PlaybackPositionTicks")]
    playback_position_ticks: Option<i64>,
    #[serde(default, rename = "LastPlayedDate")]
    last_played_date: Option<String>,
}

fn build_api_migration_client(
    source: &MigrationSourceResponse,
    request: &MigrationSourceCredentialRequest,
) -> Result<ApiMigrationClient, MigrationError> {
    let base_url = source
        .connection_config
        .get("base_url")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            MigrationError::InvalidSourceConfiguration(
                "API migration source is missing base_url".to_string(),
            )
        })?;

    let api_key = request
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            MigrationError::InvalidSourceConfiguration(
                "api_key must be supplied for Jellyfin/Emby source API calls".to_string(),
            )
        })?;
    verify_supplied_api_key(source, api_key)?;

    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(API_CONFIG_TIMEOUT_SECONDS))
        .no_proxy()
        .build()
        .map_err(|e| MigrationError::SourceUnreachable(e.to_string()))?;

    Ok(ApiMigrationClient {
        platform: source.platform.clone(),
        base_url: base_url.to_string(),
        api_key: api_key.to_string(),
        http,
    })
}

fn verify_supplied_api_key(
    source: &MigrationSourceResponse,
    api_key: &str,
) -> Result<(), MigrationError> {
    let stored_hash = source
        .connection_config
        .get("api_key_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            MigrationError::InvalidSourceConfiguration(
                "stored API credential hash is missing".to_string(),
            )
        })?;

    let computed_hash = format!("sha256:{}", sha256_hex(api_key));
    if computed_hash != stored_hash {
        return Err(MigrationError::InvalidSourceConfiguration(
            "supplied API key does not match the stored credential hash".to_string(),
        ));
    }

    Ok(())
}

impl ApiMigrationClient {
    async fn fetch_source_users(&self) -> Result<Vec<MigrationSourceUserResponse>, MigrationError> {
        let primary_path = if self.platform == "emby" {
            "/Users/Query"
        } else {
            "/Users"
        };
        let fallback_path = if self.platform == "emby" {
            "/Users"
        } else {
            "/Users/Query"
        };

        let value = match self
            .get_json(primary_path, &[("IsDisabled", "false".to_string())])
            .await
        {
            Ok(value) => value,
            Err(_) => {
                self.get_json(fallback_path, &[("IsDisabled", "false".to_string())])
                    .await?
            }
        };

        parse_source_users(value)
    }

    async fn fetch_mapping_items(
        &self,
        mapping: SourceMapping,
    ) -> Result<Vec<SourceWatchItem>, MigrationError> {
        let watched = self.fetch_user_items(&mapping, true).await?;
        let resume = self.fetch_user_items(&mapping, false).await?;
        Ok(merge_source_watch_items(watched.into_iter().chain(resume)))
    }

    async fn fetch_user_items(
        &self,
        mapping: &SourceMapping,
        watched: bool,
    ) -> Result<Vec<SourceWatchItem>, MigrationError> {
        let mut start_index = 0_i64;
        let mut out = Vec::new();
        loop {
            let path = if watched {
                format!(
                    "/Users/{}/Items",
                    urlencoding::encode(&mapping.source_user_id)
                )
            } else {
                format!(
                    "/Users/{}/Items/Resume",
                    urlencoding::encode(&mapping.source_user_id)
                )
            };

            let mut params = vec![
                ("Recursive", "true".to_string()),
                ("IncludeItemTypes", "Movie,Episode".to_string()),
                ("Fields", "ProviderIds,UserData".to_string()),
                ("EnableUserData", "true".to_string()),
                ("EnableImages", "false".to_string()),
                ("StartIndex", start_index.to_string()),
                ("Limit", API_PAGE_SIZE.to_string()),
            ];
            if watched {
                params.push(("IsPlayed", "true".to_string()));
                params.push(("Filters", "IsPlayed".to_string()));
            }

            let value = self.get_json(&path, &params).await?;
            let items = parse_api_items(value)?;
            let returned = items.len() as i64;
            for item in items {
                if let Some(mapped) = source_watch_item_from_api_item(mapping, item, watched) {
                    out.push(mapped);
                }
            }

            if returned < API_PAGE_SIZE {
                break;
            }
            start_index += returned;
        }
        Ok(out)
    }

    async fn get_json(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<Value, MigrationError> {
        let delays = [1_u64, 5, 15];
        let mut last_error = None;

        for attempt in 0..=delays.len() {
            match self.get_json_once(path, params).await {
                Ok(value) => return Ok(value),
                Err(MigrationError::InvalidSourceConfiguration(message)) => {
                    return Err(MigrationError::InvalidSourceConfiguration(message));
                }
                Err(error) => {
                    last_error = Some(error.to_string());
                    if attempt < delays.len() {
                        tokio::time::sleep(Duration::from_secs(delays[attempt])).await;
                    }
                }
            }
        }

        Err(MigrationError::SourceUnreachable(
            last_error.unwrap_or_else(|| "source API request failed".to_string()),
        ))
    }

    async fn get_json_once(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<Value, MigrationError> {
        let mut url = Url::parse(&format!("{}{}", self.base_url.trim_end_matches('/'), path))
            .map_err(|e| MigrationError::InvalidSourceConfiguration(e.to_string()))?;
        {
            let mut query = url.query_pairs_mut();
            for (key, value) in params {
                query.append_pair(key, value);
            }
        }

        let response = self
            .http
            .get(url)
            .header("X-Emby-Token", &self.api_key)
            .send()
            .await
            .map_err(|e| MigrationError::SourceUnreachable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(MigrationError::InvalidSourceConfiguration(
                "source API key was rejected".to_string(),
            ));
        }

        if !response.status().is_success() {
            return Err(MigrationError::SourceUnreachable(format!(
                "source API returned HTTP {}",
                response.status()
            )));
        }

        if let Some(content_length) = response.content_length()
            && content_length > API_CONFIG_MAX_RESPONSE_BYTES
        {
            return Err(MigrationError::SourceUnreachable(
                "source API response exceeds size limit".to_string(),
            ));
        }

        let body = response
            .bytes()
            .await
            .map_err(|e| MigrationError::SourceUnreachable(e.to_string()))?;
        if body.len() as u64 > API_CONFIG_MAX_RESPONSE_BYTES {
            return Err(MigrationError::SourceUnreachable(
                "source API response exceeds size limit".to_string(),
            ));
        }

        serde_json::from_slice(&body).map_err(|e| MigrationError::SourceUnreachable(e.to_string()))
    }
}

fn parse_source_users(value: Value) -> Result<Vec<MigrationSourceUserResponse>, MigrationError> {
    let user_value = if value.is_array() {
        value
    } else {
        value
            .get("Items")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()))
    };

    let users: Vec<ApiUserDto> = serde_json::from_value(user_value)
        .map_err(|e| MigrationError::SourceUnreachable(e.to_string()))?;
    Ok(users
        .into_iter()
        .filter_map(|user| {
            Some(MigrationSourceUserResponse {
                source_user_id: user.id?,
                source_user_name: user.name?,
            })
        })
        .collect())
}

fn parse_api_items(value: Value) -> Result<Vec<ApiItemDto>, MigrationError> {
    let item_value = if value.is_array() {
        value
    } else {
        value
            .get("Items")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()))
    };

    serde_json::from_value(item_value).map_err(|e| MigrationError::SourceUnreachable(e.to_string()))
}

fn extract_plex_users(db_path: &Path) -> Result<Vec<MigrationSourceUserResponse>, MigrationError> {
    let conn = open_plex_database(db_path)?;
    let mut stmt = conn
        .prepare("SELECT id, name FROM accounts WHERE id > 0 ORDER BY name, id")
        .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;
    let rows = stmt
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            let name: String = row.get(1)?;
            Ok(MigrationSourceUserResponse {
                source_user_id: id.to_string(),
                source_user_name: name,
            })
        })
        .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))
}

fn extract_plex_watch_items(
    db_path: &Path,
    mappings: &[SourceMapping],
) -> Result<Vec<SourceWatchItem>, MigrationError> {
    let conn = open_plex_database(db_path)?;
    let mut out = Vec::new();
    for mapping in mappings {
        let account_id = mapping.source_user_id.parse::<i64>().map_err(|_| {
            MigrationError::InvalidPlexDatabase(format!(
                "Plex source user id is not numeric: {}",
                mapping.source_user_id
            ))
        })?;
        out.extend(extract_plex_watch_items_for_account(
            &conn, mapping, account_id,
        )?);
    }
    Ok(merge_source_watch_items(out))
}

fn extract_plex_watch_items_for_account(
    conn: &rusqlite::Connection,
    mapping: &SourceMapping,
    account_id: i64,
) -> Result<Vec<SourceWatchItem>, MigrationError> {
    let has_metadata_item_guids = sqlite_table_exists(conn, "metadata_item_guids")?;
    let has_metadata_item_providers = sqlite_table_exists(conn, "metadata_item_providers")?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT
                mi.id,
                mis.guid,
                COALESCE(mis.view_count, 0),
                COALESCE(mis.view_offset, 0),
                mis.last_viewed_at,
                mi.title,
                mi.metadata_type,
                mi.year,
                mi.guid,
                mi.parent_id,
                parent.title,
                mi."index",
                mi.parent_index
            FROM metadata_item_settings mis
            JOIN metadata_items mi ON mis.guid = mi.guid
            LEFT JOIN metadata_items parent ON mi.parent_id = parent.id
            WHERE mis.account_id = ?1
              AND (COALESCE(mis.view_count, 0) > 0 OR COALESCE(mis.view_offset, 0) > 0)
              AND mi.metadata_type IN (1, 4)
            ORDER BY mi.title, mi.id
            "#,
        )
        .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;

    let rows = stmt
        .query_map([account_id], |row| {
            Ok(PlexWatchRow {
                metadata_item_id: row.get(0)?,
                source_item_id: row.get(1)?,
                view_count: row.get(2)?,
                view_offset: row.get(3)?,
                last_viewed_at: row.get(4)?,
                title: row.get(5)?,
                metadata_type: row.get(6)?,
                year: row.get(7)?,
                primary_guid: row.get(8)?,
                parent_id: row.get(9)?,
                parent_title: row.get(10)?,
                episode_number: row.get(11)?,
                season_number: row.get(12)?,
            })
        })
        .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;

    let mut out = Vec::new();
    for row in rows {
        let row = row.map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;
        let Some(source_item_type) = plex_source_item_type(row.metadata_type) else {
            continue;
        };
        let secondary_guids = load_plex_secondary_guids(
            conn,
            row.metadata_item_id,
            has_metadata_item_guids,
            has_metadata_item_providers,
        );
        let source_is_watched = row.view_count > 0;
        let source_resume_position_ms = if source_is_watched {
            0
        } else {
            row.view_offset.max(0)
        };
        let source_play_count = i32::try_from(row.view_count.max(0)).unwrap_or(i32::MAX);
        let source_provider_ids = normalize_plex_provider_ids(&row.primary_guid, &secondary_guids);

        out.push(SourceWatchItem {
            migration_user_mapping_id: mapping.id,
            source_item_id: row.source_item_id.clone(),
            source_item_title: row.title.clone(),
            source_item_type: source_item_type.to_string(),
            source_item_year: row.year,
            source_provider_ids,
            source_is_watched,
            source_play_count,
            source_resume_position_ms,
            source_last_played_at: row
                .last_viewed_at
                .and_then(|timestamp| DateTime::<Utc>::from_timestamp(timestamp, 0)),
            source_item_metadata: json!({
                "source_item_id": row.source_item_id,
                "metadata_item_id": row.metadata_item_id,
                "metadata_type": row.metadata_type,
                "primary_guid": row.primary_guid,
                "secondary_guids": secondary_guids,
                "parent_id": row.parent_id,
                "series_name": row.parent_title,
                "season_number": row.season_number,
                "episode_number": row.episode_number,
            }),
        });
    }
    Ok(out)
}

#[derive(Debug)]
struct PlexWatchRow {
    metadata_item_id: i64,
    source_item_id: String,
    view_count: i64,
    view_offset: i64,
    last_viewed_at: Option<i64>,
    title: String,
    metadata_type: i64,
    year: Option<i32>,
    primary_guid: String,
    parent_id: Option<i64>,
    parent_title: Option<String>,
    episode_number: Option<i32>,
    season_number: Option<i32>,
}

fn open_plex_database(db_path: &Path) -> Result<rusqlite::Connection, MigrationError> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;
    conn.pragma_update(None, "query_only", true)
        .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;
    Ok(conn)
}

fn sqlite_table_exists(
    conn: &rusqlite::Connection,
    table_name: &str,
) -> Result<bool, MigrationError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table_name],
            |row| row.get(0),
        )
        .map_err(|e| MigrationError::InvalidPlexDatabase(e.to_string()))?;
    Ok(count > 0)
}

fn load_plex_secondary_guids(
    conn: &rusqlite::Connection,
    metadata_item_id: i64,
    has_metadata_item_guids: bool,
    has_metadata_item_providers: bool,
) -> Vec<String> {
    let mut guids = Vec::new();
    if has_metadata_item_guids {
        collect_plex_guid_column(
            conn,
            "SELECT guid FROM metadata_item_guids WHERE metadata_item_id = ?1",
            metadata_item_id,
            &mut guids,
        );
    }
    if has_metadata_item_providers {
        collect_plex_guid_column(
            conn,
            "SELECT guid FROM metadata_item_providers WHERE metadata_item_id = ?1",
            metadata_item_id,
            &mut guids,
        );
        collect_plex_provider_rows(conn, metadata_item_id, &mut guids);
    }
    guids.sort();
    guids.dedup();
    guids
}

fn collect_plex_guid_column(
    conn: &rusqlite::Connection,
    sql: &str,
    metadata_item_id: i64,
    out: &mut Vec<String>,
) {
    let Ok(mut stmt) = conn.prepare(sql) else {
        return;
    };
    let Ok(rows) = stmt.query_map([metadata_item_id], |row| row.get::<_, String>(0)) else {
        return;
    };
    for guid in rows.flatten() {
        if !guid.trim().is_empty() {
            out.push(guid);
        }
    }
}

fn collect_plex_provider_rows(
    conn: &rusqlite::Connection,
    metadata_item_id: i64,
    out: &mut Vec<String>,
) {
    let Ok(mut stmt) = conn.prepare(
        "SELECT provider, provider_id FROM metadata_item_providers WHERE metadata_item_id = ?1",
    ) else {
        return;
    };
    let Ok(rows) = stmt.query_map([metadata_item_id], |row| {
        let provider: String = row.get(0)?;
        let provider_id: String = row.get(1)?;
        Ok((provider, provider_id))
    }) else {
        return;
    };
    for row in rows.flatten() {
        if !row.0.trim().is_empty() && !row.1.trim().is_empty() {
            out.push(format!("{}://{}", row.0.trim(), row.1.trim()));
        }
    }
}

fn plex_source_item_type(metadata_type: i64) -> Option<&'static str> {
    match metadata_type {
        1 => Some("movie"),
        4 => Some("episode"),
        _ => None,
    }
}

fn normalize_plex_provider_ids(primary_guid: &str, secondary_guids: &[String]) -> Value {
    let mut tmdb = None;
    let mut imdb = None;
    let mut tvdb = None;
    for guid in std::iter::once(primary_guid).chain(secondary_guids.iter().map(String::as_str)) {
        if let Some((provider, id)) = parse_plex_provider_guid(guid) {
            match provider {
                "tmdb" if tmdb.is_none() => tmdb = Some(id),
                "imdb" if imdb.is_none() => imdb = Some(id),
                "tvdb" if tvdb.is_none() => tvdb = Some(id),
                _ => {}
            }
        }
    }

    json!({
        "tmdb": tmdb,
        "imdb": imdb,
        "tvdb": tvdb,
        "raw": {
            "primary_guid": primary_guid,
            "secondary_guids": secondary_guids,
        },
    })
}

fn parse_plex_provider_guid(guid: &str) -> Option<(&'static str, String)> {
    let trimmed = guid.trim();
    let (prefix, rest) = trimmed.split_once("://")?;
    let value = rest
        .split(['?', '/', '#'])
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let prefix = prefix.to_ascii_lowercase();
    let provider = if prefix.contains("imdb") {
        "imdb"
    } else if prefix.contains("themoviedb") || prefix.contains("tmdb") {
        "tmdb"
    } else if prefix.contains("thetvdb") || prefix.contains("tvdb") {
        "tvdb"
    } else {
        return None;
    };
    Some((provider, value.to_string()))
}

fn source_watch_item_from_api_item(
    mapping: &SourceMapping,
    item: ApiItemDto,
    from_watched_endpoint: bool,
) -> Option<SourceWatchItem> {
    let source_item_id = item.id?;
    let raw_type = item.item_type?;
    let source_item_type = match raw_type.as_str() {
        "Movie" | "movie" => "movie",
        "Episode" | "episode" => "episode",
        _ => return None,
    }
    .to_string();
    let user_data = item.user_data.unwrap_or(ApiUserDataDto {
        played: None,
        play_count: None,
        playback_position_ticks: None,
        last_played_date: None,
    });
    let source_is_watched = from_watched_endpoint || user_data.played.unwrap_or(false);
    let source_play_count = user_data
        .play_count
        .unwrap_or(i32::from(source_is_watched))
        .max(0);
    let mut source_resume_position_ms = user_data
        .playback_position_ticks
        .unwrap_or(0)
        .saturating_div(10_000)
        .max(0);
    if source_is_watched {
        source_resume_position_ms = 0;
    }

    Some(SourceWatchItem {
        migration_user_mapping_id: mapping.id,
        source_item_id: source_item_id.clone(),
        source_item_title: item.name.unwrap_or(source_item_id.clone()),
        source_item_type,
        source_item_year: item.production_year,
        source_provider_ids: normalize_provider_ids(&item.provider_ids),
        source_is_watched,
        source_play_count,
        source_resume_position_ms,
        source_last_played_at: parse_source_datetime(user_data.last_played_date.as_deref()),
        source_item_metadata: json!({
            "source_item_id": source_item_id,
            "source_type": raw_type,
            "series_name": item.series_name,
            "season_number": item.season_number,
            "episode_number": item.episode_number,
        }),
    })
}

fn merge_source_watch_items<I>(items: I) -> Vec<SourceWatchItem>
where
    I: IntoIterator<Item = SourceWatchItem>,
{
    let mut merged: HashMap<(Uuid, String), SourceWatchItem> = HashMap::new();
    for item in items {
        let key = (item.migration_user_mapping_id, item.source_item_id.clone());
        merged
            .entry(key)
            .and_modify(|existing| merge_source_watch_item(existing, &item))
            .or_insert(item);
    }
    merged.into_values().collect()
}

fn merge_source_watch_item(existing: &mut SourceWatchItem, incoming: &SourceWatchItem) {
    existing.source_is_watched |= incoming.source_is_watched;
    existing.source_play_count = existing.source_play_count.max(incoming.source_play_count);
    if existing.source_is_watched {
        existing.source_resume_position_ms = 0;
    } else {
        existing.source_resume_position_ms = existing
            .source_resume_position_ms
            .max(incoming.source_resume_position_ms);
    }
    existing.source_last_played_at = match (
        existing.source_last_played_at,
        incoming.source_last_played_at,
    ) {
        (Some(existing_date), Some(incoming_date)) => Some(existing_date.max(incoming_date)),
        (Some(existing_date), None) => Some(existing_date),
        (None, Some(incoming_date)) => Some(incoming_date),
        (None, None) => None,
    };
}

fn normalize_provider_ids(provider_ids: &Value) -> Value {
    json!({
        "tmdb": provider_id_value(provider_ids, &["Tmdb", "TMDb", "tmdb"]),
        "imdb": provider_id_value(provider_ids, &["Imdb", "IMDb", "imdb"]),
        "tvdb": provider_id_value(provider_ids, &["Tvdb", "TVDb", "TVDB", "tvdb"]),
        "raw": provider_ids,
    })
}

fn provider_id_value(provider_ids: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| provider_ids.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn parse_source_datetime(value: Option<&str>) -> Option<DateTime<Utc>> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .ok()
}

async fn load_source_mappings(
    state: &AppState,
    id: Uuid,
) -> Result<Vec<SourceMapping>, MigrationError> {
    let rows = sqlx::query(
        r#"
        SELECT id, source_user_id
        FROM migration_user_mapping
        WHERE migration_source_id = $1
          AND status <> 'skipped'
          AND platform_user_id IS NOT NULL
        ORDER BY source_user_name, source_user_id
        "#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| SourceMapping {
            id: row.get("id"),
            source_user_id: row.get("source_user_id"),
        })
        .collect())
}

async fn extract_api_watch_items(
    client: &ApiMigrationClient,
    mappings: &[SourceMapping],
) -> Result<Vec<SourceWatchItem>, MigrationError> {
    let mut out = Vec::new();
    for chunk in mappings.chunks(API_EXTRACTION_CONCURRENCY) {
        let mut tasks = JoinSet::new();
        for mapping in chunk.iter().cloned() {
            let client = client.clone();
            tasks.spawn(async move { client.fetch_mapping_items(mapping).await });
        }
        while let Some(result) = tasks.join_next().await {
            let items = result.map_err(|e| MigrationError::SourceUnreachable(e.to_string()))??;
            out.extend(items);
        }
    }
    Ok(out)
}

async fn upsert_discovered_items(
    state: &AppState,
    migration_source_id: Uuid,
    items: &[SourceWatchItem],
) -> Result<(u64, u64), MigrationError> {
    let mut inserted = 0_u64;
    let mut updated = 0_u64;
    let mut tx = state.pool.begin().await?;

    for item in items {
        let existing: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id
            FROM migration_import_log
            WHERE migration_user_mapping_id = $1
              AND source_item_id = $2
            "#,
        )
        .bind(item.migration_user_mapping_id)
        .bind(&item.source_item_id)
        .fetch_optional(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO migration_import_log (
                migration_source_id,
                migration_user_mapping_id,
                source_item_id,
                source_item_title,
                source_item_type,
                source_item_year,
                source_provider_ids,
                source_is_watched,
                source_play_count,
                source_resume_position_ms,
                source_last_played_at,
                source_item_metadata,
                status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'discovered')
            ON CONFLICT (migration_user_mapping_id, source_item_id)
            DO UPDATE SET
                source_item_title = EXCLUDED.source_item_title,
                source_item_type = EXCLUDED.source_item_type,
                source_item_year = EXCLUDED.source_item_year,
                source_provider_ids = EXCLUDED.source_provider_ids,
                source_is_watched = migration_import_log.source_is_watched OR EXCLUDED.source_is_watched,
                source_play_count = GREATEST(migration_import_log.source_play_count, EXCLUDED.source_play_count),
                source_resume_position_ms = CASE
                    WHEN migration_import_log.source_is_watched OR EXCLUDED.source_is_watched THEN 0
                    ELSE GREATEST(migration_import_log.source_resume_position_ms, EXCLUDED.source_resume_position_ms)
                END,
                source_last_played_at = GREATEST(migration_import_log.source_last_played_at, EXCLUDED.source_last_played_at),
                source_item_metadata = EXCLUDED.source_item_metadata,
                status = CASE
                    WHEN migration_import_log.status IN ('matched', 'unmatched', 'imported', 'skipped') THEN migration_import_log.status
                    ELSE 'discovered'
                END
            "#,
        )
        .bind(migration_source_id)
        .bind(item.migration_user_mapping_id)
        .bind(&item.source_item_id)
        .bind(&item.source_item_title)
        .bind(&item.source_item_type)
        .bind(item.source_item_year)
        .bind(&item.source_provider_ids)
        .bind(item.source_is_watched)
        .bind(item.source_play_count)
        .bind(item.source_resume_position_ms)
        .bind(item.source_last_played_at)
        .bind(&item.source_item_metadata)
        .execute(&mut *tx)
        .await?;

        if existing.is_some() {
            updated += 1;
        } else {
            inserted += 1;
        }
    }

    tx.commit().await?;
    Ok((inserted, updated))
}

pub async fn save_user_mappings(
    state: &AppState,
    id: Uuid,
    request: SaveUserMappingsRequest,
) -> Result<MigrationActionResponse, MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;

    if request.mappings.is_empty() {
        return Err(MigrationError::NoUserMappings);
    }

    validate_mapping_conflicts(&request)?;
    ensure_at_least_one_active_mapping(&request)?;
    validate_platform_users_exist(state, &request).await?;

    let mut tx = state.pool.begin().await?;

    sqlx::query("DELETE FROM migration_user_mapping WHERE migration_source_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    for mapping in request.mappings {
        sqlx::query(
            r#"
            INSERT INTO migration_user_mapping (
                migration_source_id,
                source_user_id,
                source_user_name,
                platform_user_id,
                status
            )
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id)
        .bind(&mapping.source_user_id)
        .bind(&mapping.source_user_name)
        .bind(mapping.platform_user_id)
        .bind(if mapping.skip.unwrap_or(false) {
            "skipped"
        } else {
            "pending"
        })
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(MigrationActionResponse {
        migration_source_id: id,
        status: source.status,
        message: "Migration user mappings saved".to_string(),
    })
}

pub async fn start_migration(
    state: &AppState,
    id: Uuid,
    request: StartMigrationRequest,
) -> Result<MigrationActionResponse, MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;
    ensure_has_mappings(state, id).await?;

    if request.dry_run.unwrap_or(false) {
        let report = run_preflight(state, id).await?;
        return Ok(MigrationActionResponse {
            migration_source_id: id,
            status: report.status,
            message: format!(
                "Migration dry-run preflight completed: {} blocker(s), {} warning(s)",
                report.blockers.len(),
                report.warnings.len()
            ),
        });
    }

    let report = run_preflight(state, id).await?;
    if !report.blockers.is_empty() {
        let blockers = report
            .blockers
            .iter()
            .map(|finding| finding.code.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(MigrationError::InvalidSourceConfiguration(format!(
            "preflight blockers must be resolved before start: {blockers}"
        )));
    }

    ensure_has_watch_data(state, id).await?;
    let status = migration_runner::spawn_migration_runner(state, id).await?;

    Ok(MigrationActionResponse {
        migration_source_id: id,
        status,
        message: "Migration runner started".to_string(),
    })
}

pub async fn run_preflight(
    state: &AppState,
    id: Uuid,
) -> Result<MigrationPreflightResponse, MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;

    let library_readiness = load_library_readiness(state).await?;
    let user_mapping_readiness = load_user_mapping_readiness(state, id).await?;
    let estimated_counts = load_preflight_estimated_counts(state, id).await?;
    let source_readiness = check_source_readiness(&source).await;
    let disk_readiness = check_disk_readiness(state, &source);

    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    let mut checks = Vec::new();

    push_library_findings(
        &library_readiness,
        &mut blockers,
        &mut warnings,
        &mut checks,
    );
    push_mapping_findings(
        &user_mapping_readiness,
        &mut blockers,
        &mut warnings,
        &mut checks,
    );
    push_source_findings(&source_readiness, &mut blockers, &mut warnings, &mut checks);
    push_disk_findings(&disk_readiness, &mut blockers, &mut warnings, &mut checks);
    push_estimate_findings(&estimated_counts, &mut warnings, &mut checks);

    Ok(MigrationPreflightResponse {
        migration_source_id: id,
        platform: source.platform,
        status: source.status,
        is_ready: blockers.is_empty(),
        blockers,
        warnings,
        checks,
        library_readiness,
        user_mapping_readiness,
        source_readiness,
        disk_readiness,
        estimated_counts,
    })
}

pub async fn get_migration_progress(
    state: &AppState,
    id: Uuid,
) -> Result<MigrationProgressResponse, MigrationError> {
    let source = get_source(state, id).await?;

    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*)::INT AS items_discovered,
            COUNT(*) FILTER (WHERE status IN ('matched', 'imported', 'skipped'))::INT AS items_matched,
            COUNT(*) FILTER (WHERE status = 'unmatched')::INT AS items_unmatched,
            COUNT(*) FILTER (WHERE status = 'imported')::INT AS items_imported,
            COUNT(*) FILTER (WHERE status = 'skipped')::INT AS items_skipped,
            COUNT(*) FILTER (WHERE status IN ('imported', 'skipped', 'unmatched', 'error'))::INT AS items_processed
        FROM migration_import_log
        WHERE migration_source_id = $1
        "#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;

    let items_discovered: i32 = row.get("items_discovered");
    let items_processed: i32 = row.get("items_processed");
    let percent_complete = if source.status == "completed" {
        100.0
    } else if items_discovered > 0 {
        ((items_processed as f32) / (items_discovered as f32) * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    Ok(MigrationProgressResponse {
        migration_source_id: id,
        status: source.status,
        percent_complete,
        items_discovered,
        items_matched: row.get("items_matched"),
        items_unmatched: row.get("items_unmatched"),
        items_imported: row.get("items_imported"),
        items_skipped: row.get("items_skipped"),
    })
}

pub async fn get_unmatched_report(
    state: &AppState,
    id: Uuid,
    _query: UnmatchedReportQuery,
    page: u32,
    page_size: u32,
) -> Result<UnmatchedReportResponse, MigrationError> {
    get_source(state, id).await?;

    let limit = page_size.max(1) as i64;
    let offset = (page.saturating_sub(1) as i64) * limit;

    let rows = sqlx::query(
        r#"
        SELECT id, source_item_id, source_item_title, source_item_type,
               source_item_year, source_provider_ids, match_method, status, error_detail
        FROM migration_import_log
        WHERE migration_source_id = $1
          AND (status = 'unmatched' OR match_method = 'unmatched')
        ORDER BY source_item_title, source_item_year NULLS LAST, source_item_id
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let total: i64 = sqlx::query(
        r#"
        SELECT COUNT(*)
        FROM migration_import_log
        WHERE migration_source_id = $1
          AND (status = 'unmatched' OR match_method = 'unmatched')
        "#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?
    .get(0);

    Ok(UnmatchedReportResponse {
        items: rows.iter().map(row_to_unmatched_item).collect(),
        total,
        page,
        page_size,
        total_pages: ((total as f64) / (page_size as f64)).ceil() as u32,
    })
}

pub async fn cancel_migration(
    state: &AppState,
    id: Uuid,
) -> Result<MigrationActionResponse, MigrationError> {
    let source = get_source(state, id).await?;

    if ACTIVE_STATUSES.contains(&source.status.as_str()) {
        let signalled = migration_runner::cancel_migration_runner(state, id);
        let status = update_active_source_status(state, id, "cancelled").await?;
        let message = if signalled {
            "Migration cancellation requested"
        } else {
            "Migration cancellation recorded"
        };
        return Ok(MigrationActionResponse {
            migration_source_id: id,
            status,
            message: message.to_string(),
        });
    }

    Ok(MigrationActionResponse {
        migration_source_id: id,
        status: source.status,
        message: "Migration is not running; no cancellation was needed".to_string(),
    })
}

async fn load_library_readiness(state: &AppState) -> Result<LibraryReadiness, MigrationError> {
    let row = sqlx::query(
        r#"
        SELECT
            COUNT(DISTINCT l.id)::BIGINT AS active_libraries,
            COUNT(DISTINCT l.id) FILTER (WHERE l.last_scan_at IS NOT NULL)::BIGINT AS scanned_libraries,
            COUNT(mi.id) FILTER (WHERE mi.type IN ('movie', 'episode'))::BIGINT AS importable_items,
            COUNT(mi.id) FILTER (
                WHERE mi.type IN ('movie', 'episode')
                  AND (mi.tmdb_id IS NOT NULL OR mi.imdb_id IS NOT NULL OR mi.tvdb_id IS NOT NULL)
            )::BIGINT AS items_with_provider_ids
        FROM libraries l
        LEFT JOIN media_items mi ON mi.library_id = l.id
        WHERE l.deleted_at IS NULL
        "#,
    )
    .fetch_one(&state.pool)
    .await?;

    let importable_items: i64 = row.get("importable_items");
    let items_with_provider_ids: i64 = row.get("items_with_provider_ids");
    let provider_id_coverage_percent = percent(items_with_provider_ids, importable_items);

    Ok(LibraryReadiness {
        active_libraries: row.get("active_libraries"),
        scanned_libraries: row.get("scanned_libraries"),
        importable_items,
        items_with_provider_ids,
        provider_id_coverage_percent,
    })
}

async fn load_user_mapping_readiness(
    state: &AppState,
    id: Uuid,
) -> Result<UserMappingReadiness, MigrationError> {
    let row = sqlx::query(
        r#"
        SELECT
            COUNT(m.id)::BIGINT AS mappings_total,
            COUNT(m.id) FILTER (WHERE m.status <> 'skipped' AND u.id IS NOT NULL)::BIGINT AS valid_mappings,
            COUNT(m.id) FILTER (WHERE m.status = 'skipped')::BIGINT AS skipped_mappings
        FROM migration_user_mapping m
        LEFT JOIN users u ON u.id = m.platform_user_id AND u.deleted_at IS NULL
        WHERE m.migration_source_id = $1
        "#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;

    let mappings_total: i64 = row.get("mappings_total");
    let valid_mappings: i64 = row.get("valid_mappings");
    let skipped_mappings: i64 = row.get("skipped_mappings");

    Ok(UserMappingReadiness {
        mappings_total,
        valid_mappings,
        invalid_mappings: mappings_total.saturating_sub(valid_mappings + skipped_mappings),
        skipped_mappings,
    })
}

async fn load_preflight_estimated_counts(
    state: &AppState,
    id: Uuid,
) -> Result<PreflightEstimatedCounts, MigrationError> {
    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*)::BIGINT AS source_items_discovered,
            COUNT(*) FILTER (WHERE status IN ('discovered', 'matched', 'unmatched', 'imported', 'skipped', 'error'))::BIGINT AS source_items_with_watch_data,
            COUNT(*) FILTER (WHERE status IN ('matched', 'imported', 'skipped'))::BIGINT AS estimated_matches,
            COUNT(*) FILTER (WHERE match_method = 'title_year')::BIGINT AS low_confidence_count,
            COUNT(*) FILTER (WHERE status = 'unmatched' OR match_method = 'unmatched')::BIGINT AS unmatched_count
        FROM migration_import_log
        WHERE migration_source_id = $1
        "#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;

    let source_items_with_watch_data: i64 = row.get("source_items_with_watch_data");
    let estimated_matches: i64 = row.get("estimated_matches");

    Ok(PreflightEstimatedCounts {
        source_items_discovered: row.get("source_items_discovered"),
        source_items_with_watch_data,
        estimated_matches,
        estimated_match_rate_percent: percent(estimated_matches, source_items_with_watch_data),
        low_confidence_count: row.get("low_confidence_count"),
        unmatched_count: row.get("unmatched_count"),
    })
}

async fn check_source_readiness(source: &MigrationSourceResponse) -> SourceReadiness {
    let method = source
        .connection_config
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let credential_mode = source
        .connection_config
        .get("credential_mode")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();

    if source.platform == "plex" {
        let file_size_bytes = source
            .connection_config
            .get("file_size_bytes")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let message = if file_size_bytes > 0 {
            "Plex upload metadata is present; full SQLite validation runs when an uploaded file is available"
        } else {
            "Plex database upload has not been attached yet"
        };
        return SourceReadiness {
            platform: source.platform.clone(),
            method,
            reachable: None,
            credential_mode,
            message: message.to_string(),
        };
    }

    let Some(base_url) = source
        .connection_config
        .get("base_url")
        .and_then(Value::as_str)
    else {
        return SourceReadiness {
            platform: source.platform.clone(),
            method,
            reachable: Some(false),
            credential_mode,
            message: "API source is missing base_url".to_string(),
        };
    };

    match check_api_source_reachability(base_url).await {
        Ok(status) => SourceReadiness {
            platform: source.platform.clone(),
            method,
            reachable: Some(true),
            credential_mode,
            message: format!("Source responded with HTTP {status}"),
        },
        Err(message) => SourceReadiness {
            platform: source.platform.clone(),
            method,
            reachable: Some(false),
            credential_mode,
            message,
        },
    }
}

async fn check_api_source_reachability(base_url: &str) -> Result<reqwest::StatusCode, String> {
    let url = format!("{}/System/Info/Public", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(API_CONFIG_TIMEOUT_SECONDS))
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;

    if let Some(content_length) = response.content_length()
        && content_length > API_CONFIG_MAX_RESPONSE_BYTES
    {
        return Err("Source response exceeds the preflight size limit".to_string());
    }

    let status = response.status();
    if status.is_success() || status.as_u16() == 401 || status.as_u16() == 403 {
        Ok(status)
    } else {
        Err(format!("Source responded with HTTP {status}"))
    }
}

fn check_disk_readiness(state: &AppState, source: &MigrationSourceResponse) -> DiskReadiness {
    let required_bytes = if source.platform == "plex" {
        source
            .connection_config
            .get("file_size_bytes")
            .and_then(Value::as_u64)
            .filter(|size| *size > 0)
            .map(|size| size.saturating_mul(2))
    } else {
        None
    };

    let Some(required_bytes) = required_bytes else {
        return DiskReadiness {
            required_bytes: None,
            available_bytes: None,
            has_headroom: None,
        };
    };

    let upload_dir = state.bootstrap.data_dir.join("migrations");
    let available_bytes = available_space_for_path(&upload_dir);

    DiskReadiness {
        required_bytes: Some(required_bytes),
        available_bytes,
        has_headroom: available_bytes.map(|available| available >= required_bytes),
    }
}

fn push_library_findings(
    readiness: &LibraryReadiness,
    blockers: &mut Vec<PreflightFinding>,
    warnings: &mut Vec<PreflightFinding>,
    checks: &mut Vec<PreflightCheck>,
) {
    if readiness.active_libraries == 0 {
        blockers.push(finding(
            "MIGR_PREFLIGHT_NO_LIBRARIES",
            "No active libraries are available for migration matching",
        ));
    }
    if readiness.scanned_libraries == 0 {
        blockers.push(finding(
            "MIGR_PREFLIGHT_LIBRARIES_NOT_SCANNED",
            "No active libraries have completed a scan",
        ));
    }
    if readiness.importable_items == 0 {
        blockers.push(finding(
            "MIGR_PREFLIGHT_NO_IMPORTABLE_ITEMS",
            "No movie or episode items are available for watch-state matching",
        ));
    } else if readiness.provider_id_coverage_percent < 80.0 {
        warnings.push(finding(
            "MIGR_PREFLIGHT_LOW_PROVIDER_ID_COVERAGE",
            "Provider ID coverage is below 80%; fallback matching may need manual review",
        ));
    }

    checks.push(check(
        "library_provider_id_readiness",
        if readiness.importable_items > 0 && readiness.items_with_provider_ids > 0 {
            "passed"
        } else {
            "blocked"
        },
        format!(
            "{} of {} importable items have provider IDs",
            readiness.items_with_provider_ids, readiness.importable_items
        ),
    ));
}

fn push_mapping_findings(
    readiness: &UserMappingReadiness,
    blockers: &mut Vec<PreflightFinding>,
    warnings: &mut Vec<PreflightFinding>,
    checks: &mut Vec<PreflightCheck>,
) {
    if readiness.valid_mappings == 0 {
        blockers.push(finding(
            "MIGR_PREFLIGHT_NO_USER_MAPPINGS",
            "At least one source user must be mapped before migration can start",
        ));
    }
    if readiness.invalid_mappings > 0 {
        blockers.push(finding(
            "MIGR_PREFLIGHT_INVALID_USER_MAPPINGS",
            "One or more mapped platform users no longer exist",
        ));
    }
    if readiness.valid_mappings == 1 {
        warnings.push(finding(
            "MIGR_PREFLIGHT_SINGLE_USER_MAPPING",
            "Only one user is mapped; verify skipped source users before importing",
        ));
    }

    checks.push(check(
        "user_mapping_readiness",
        if readiness.valid_mappings > 0 && readiness.invalid_mappings == 0 {
            "passed"
        } else {
            "blocked"
        },
        format!(
            "{} valid mappings, {} skipped mappings, {} invalid mappings",
            readiness.valid_mappings, readiness.skipped_mappings, readiness.invalid_mappings
        ),
    ));
}

fn push_source_findings(
    readiness: &SourceReadiness,
    blockers: &mut Vec<PreflightFinding>,
    warnings: &mut Vec<PreflightFinding>,
    checks: &mut Vec<PreflightCheck>,
) {
    match readiness.reachable {
        Some(true) => checks.push(check("source_reachability", "passed", &readiness.message)),
        Some(false) => {
            blockers.push(finding(
                "MIGR_PREFLIGHT_SOURCE_UNREACHABLE",
                &readiness.message,
            ));
            checks.push(check("source_reachability", "blocked", &readiness.message));
        }
        None => {
            warnings.push(finding(
                "MIGR_PREFLIGHT_SOURCE_REACHABILITY_DEFERRED",
                &readiness.message,
            ));
            checks.push(check("source_reachability", "warning", &readiness.message));
        }
    }
}

fn push_disk_findings(
    readiness: &DiskReadiness,
    blockers: &mut Vec<PreflightFinding>,
    warnings: &mut Vec<PreflightFinding>,
    checks: &mut Vec<PreflightCheck>,
) {
    match readiness.has_headroom {
        Some(true) => checks.push(check(
            "disk_headroom",
            "passed",
            "Migration upload disk headroom is available",
        )),
        Some(false) => {
            blockers.push(finding(
                "MIGR_PREFLIGHT_INSUFFICIENT_DISK",
                "Insufficient disk space for the declared Plex database upload",
            ));
            checks.push(check(
                "disk_headroom",
                "blocked",
                "Insufficient disk space for the declared Plex database upload",
            ));
        }
        None if readiness.required_bytes.is_some() => {
            warnings.push(finding(
                "MIGR_PREFLIGHT_DISK_UNKNOWN",
                "Could not determine upload disk headroom",
            ));
            checks.push(check(
                "disk_headroom",
                "warning",
                "Could not determine upload disk headroom",
            ));
        }
        None => checks.push(check(
            "disk_headroom",
            "passed",
            "No source upload disk headroom is required",
        )),
    }
}

fn push_estimate_findings(
    counts: &PreflightEstimatedCounts,
    warnings: &mut Vec<PreflightFinding>,
    checks: &mut Vec<PreflightCheck>,
) {
    if counts.source_items_discovered == 0 {
        warnings.push(finding(
            "MIGR_PREFLIGHT_NO_DISCOVERY_DATA",
            "No source item discovery data exists yet; match-rate estimates will be available after discovery",
        ));
        checks.push(check(
            "match_estimate",
            "warning",
            "No source item discovery data exists yet",
        ));
        return;
    }

    if counts.estimated_match_rate_percent < 80.0 {
        warnings.push(finding(
            "MIGR_PREFLIGHT_LOW_ESTIMATED_MATCH_RATE",
            "Estimated match rate is below 80%; expect manual review",
        ));
    }

    checks.push(check(
        "match_estimate",
        "passed",
        format!(
            "{} of {} source items are currently estimated to match",
            counts.estimated_matches, counts.source_items_with_watch_data
        ),
    ));
}

fn finding(code: &str, message: &str) -> PreflightFinding {
    PreflightFinding {
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn check(name: &str, status: &str, message: impl Into<String>) -> PreflightCheck {
    PreflightCheck {
        name: name.to_string(),
        status: status.to_string(),
        message: message.into(),
    }
}

fn percent(numerator: i64, denominator: i64) -> f32 {
    if denominator <= 0 {
        0.0
    } else {
        ((numerator as f32) / (denominator as f32) * 100.0).clamp(0.0, 100.0)
    }
}

async fn get_source(state: &AppState, id: Uuid) -> Result<MigrationSourceResponse, MigrationError> {
    let row = sqlx::query(
        "SELECT id, created_at, platform, name, connection_config, last_run_at, status FROM migration_sources WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(MigrationError::NotFound(id))?;

    Ok(row_to_source_response(&row))
}

async fn set_source_status(
    state: &AppState,
    id: Uuid,
    status: &str,
) -> Result<String, MigrationError> {
    let row = sqlx::query(
        r#"
        UPDATE migration_sources
        SET status = $2, last_run_at = COALESCE(last_run_at, now())
        WHERE id = $1
        RETURNING status
        "#,
    )
    .bind(id)
    .bind(status)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(MigrationError::NotFound(id))?;

    Ok(row.get("status"))
}

async fn update_active_source_status(
    state: &AppState,
    id: Uuid,
    status: &str,
) -> Result<String, MigrationError> {
    let row = sqlx::query(
        r#"
        UPDATE migration_sources
        SET status = $2, last_run_at = COALESCE(last_run_at, now())
        WHERE id = $1
          AND status IN ('discovering', 'matching', 'importing')
        RETURNING status
        "#,
    )
    .bind(id)
    .bind(status)
    .fetch_optional(&state.pool)
    .await?;

    if let Some(row) = row {
        return Ok(row.get("status"));
    }

    Ok(get_source(state, id).await?.status)
}

fn ensure_not_active(id: Uuid, status: &str) -> Result<(), MigrationError> {
    if ACTIVE_STATUSES.contains(&status) {
        Err(MigrationError::AlreadyInProgress(id))
    } else {
        Ok(())
    }
}

fn push_source_filters(
    builder: &mut sqlx::QueryBuilder<sqlx::Postgres>,
    query: &ListMigrationSourcesQuery,
) {
    let mut where_started = false;

    if let Some(platform) = query.platform.as_deref() {
        builder.push(" WHERE platform = ").push_bind(platform);
        where_started = true;
    }

    if let Some(status) = query.status.as_deref() {
        builder.push(if where_started { " AND" } else { " WHERE" });
        builder.push(" status = ").push_bind(status);
    }
}

fn validate_mapping_conflicts(request: &SaveUserMappingsRequest) -> Result<(), MigrationError> {
    let mut source_user_ids = std::collections::HashSet::new();
    let mut platform_user_ids = std::collections::HashSet::new();

    for mapping in &request.mappings {
        if !source_user_ids.insert(mapping.source_user_id.as_str()) {
            return Err(MigrationError::UserMappingConflict(format!(
                "source user {} is mapped more than once",
                mapping.source_user_id
            )));
        }

        let is_skipped = mapping.skip.unwrap_or(false);
        match (is_skipped, mapping.platform_user_id) {
            (true, Some(_)) => {
                return Err(MigrationError::UserMappingConflict(format!(
                    "source user {} is marked skipped but also has a platform user",
                    mapping.source_user_id
                )));
            }
            (false, None) => {
                return Err(MigrationError::UserMappingConflict(format!(
                    "source user {} must be mapped or explicitly skipped",
                    mapping.source_user_id
                )));
            }
            _ => {}
        }

        if let Some(platform_user_id) = mapping.platform_user_id
            && !platform_user_ids.insert(platform_user_id)
        {
            return Err(MigrationError::UserMappingConflict(format!(
                "platform user {} is mapped more than once",
                platform_user_id
            )));
        }
    }

    Ok(())
}

fn ensure_at_least_one_active_mapping(
    request: &SaveUserMappingsRequest,
) -> Result<(), MigrationError> {
    if request
        .mappings
        .iter()
        .any(|mapping| !mapping.skip.unwrap_or(false) && mapping.platform_user_id.is_some())
    {
        Ok(())
    } else {
        Err(MigrationError::NoUserMappings)
    }
}

async fn validate_platform_users_exist(
    state: &AppState,
    request: &SaveUserMappingsRequest,
) -> Result<(), MigrationError> {
    let platform_user_ids: Vec<Uuid> = request
        .mappings
        .iter()
        .filter_map(|m| m.platform_user_id)
        .collect();

    if platform_user_ids.is_empty() {
        return Ok(());
    }

    let existing_count: i64 =
        sqlx::query("SELECT COUNT(*) FROM users WHERE id = ANY($1) AND deleted_at IS NULL")
            .bind(&platform_user_ids)
            .fetch_one(&state.pool)
            .await?
            .get(0);

    if existing_count != platform_user_ids.len() as i64 {
        return Err(MigrationError::UserMappingConflict(
            "one or more platform users do not exist".to_string(),
        ));
    }

    Ok(())
}

async fn ensure_has_mappings(state: &AppState, id: Uuid) -> Result<(), MigrationError> {
    let count: i64 = sqlx::query(
        r#"
            SELECT COUNT(*)
            FROM migration_user_mapping
            WHERE migration_source_id = $1
              AND status <> 'skipped'
              AND platform_user_id IS NOT NULL
            "#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?
    .get(0);

    if count == 0 {
        return Err(MigrationError::NoUserMappings);
    }

    Ok(())
}

async fn ensure_has_watch_data(state: &AppState, id: Uuid) -> Result<(), MigrationError> {
    let count: i64 =
        sqlx::query("SELECT COUNT(*) FROM migration_import_log WHERE migration_source_id = $1")
            .bind(id)
            .fetch_one(&state.pool)
            .await?
            .get(0);

    if count == 0 {
        return Err(MigrationError::NoWatchData);
    }

    Ok(())
}

fn row_to_source_response(row: &sqlx::postgres::PgRow) -> MigrationSourceResponse {
    MigrationSourceResponse {
        id: row.get("id"),
        created_at: row.get("created_at"),
        platform: row.get("platform"),
        name: row.get("name"),
        connection_config: row.get("connection_config"),
        last_run_at: row.get("last_run_at"),
        status: row.get("status"),
    }
}

fn row_to_saved_user_mapping(row: &sqlx::postgres::PgRow) -> MigrationSavedUserMappingResponse {
    let status: String = row.get("status");
    MigrationSavedUserMappingResponse {
        source_user_id: row.get("source_user_id"),
        source_user_name: row.get("source_user_name"),
        platform_user_id: row.try_get("platform_user_id").ok(),
        is_skipped: status == "skipped",
        status,
    }
}

fn row_to_platform_user_option(row: &sqlx::postgres::PgRow) -> MigrationPlatformUserOptionResponse {
    let username: String = row.get("username");
    let display_name: String = row.get("display_name");
    let invitation_display_name: Option<String> = row.try_get("invitation_display_name").ok();
    let invitation_email: Option<String> = row.try_get("invitation_email").ok();
    let label = match invitation_display_name.as_deref() {
        Some(invite_name) if invite_name != display_name => {
            format!("{display_name} ({username}) - invite: {invite_name}")
        }
        _ => format!("{display_name} ({username})"),
    };

    MigrationPlatformUserOptionResponse {
        platform_user_id: row.get("id"),
        username,
        display_name,
        email: row.try_get("email").ok(),
        status: row.get("status"),
        invitation_display_name,
        invitation_email,
        label,
    }
}

fn row_to_unmatched_item(row: &sqlx::postgres::PgRow) -> UnmatchedItemResponse {
    UnmatchedItemResponse {
        id: row.get("id"),
        source_item_id: row.get("source_item_id"),
        source_item_title: row.get("source_item_title"),
        source_item_type: row.get("source_item_type"),
        source_item_year: row.get("source_item_year"),
        source_provider_ids: row.get("source_provider_ids"),
        match_method: row.get("match_method"),
        status: row.get("status"),
        error_detail: row.get("error_detail"),
    }
}
