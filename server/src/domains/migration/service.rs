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
use std::path::Path;
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
const MAX_PLEX_DATABASE_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const API_CONFIG_TIMEOUT_SECONDS: u64 = 10;
const API_CONFIG_MAX_RESPONSE_BYTES: u64 = 1024 * 1024;
const API_PAGE_SIZE: i64 = 100;
const API_EXTRACTION_CONCURRENCY: usize = 4;

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
}

pub async fn discover_source(
    state: &AppState,
    id: Uuid,
    request: MigrationSourceCredentialRequest,
) -> Result<MigrationDiscoveryResponse, MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;

    if !matches!(source.platform.as_str(), "jellyfin" | "emby") {
        return Ok(MigrationDiscoveryResponse {
            migration_source_id: id,
            status: source.status,
            users_discovered: 0,
            users_mapped: 0,
            items_extracted: 0,
            items_inserted: 0,
            items_updated: 0,
            source_users: Vec::new(),
            message: "Plex discovery is implemented by the SQLite upload task".to_string(),
        });
    }

    let client = build_api_migration_client(&source, &request)?;
    set_source_status(state, id, "discovering").await?;

    let result = discover_api_source(state, id, &client).await;
    let final_status = if result.is_ok() { "pending" } else { "failed" };
    let status = set_source_status(state, id, final_status).await?;

    let mut response = result?;
    response.status = status;
    Ok(response)
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
                platform_user_id
            )
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(id)
        .bind(mapping.source_user_id)
        .bind(mapping.source_user_name)
        .bind(mapping.platform_user_id)
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
            COUNT(m.id) FILTER (WHERE u.id IS NOT NULL)::BIGINT AS valid_mappings
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

    Ok(UserMappingReadiness {
        mappings_total,
        valid_mappings,
        invalid_mappings: mappings_total.saturating_sub(valid_mappings),
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
    if readiness.mappings_total == 0 {
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
    if readiness.mappings_total == 1 {
        warnings.push(finding(
            "MIGR_PREFLIGHT_SINGLE_USER_MAPPING",
            "Only one user is mapped; verify skipped source users before importing",
        ));
    }

    checks.push(check(
        "user_mapping_readiness",
        if readiness.mappings_total > 0 && readiness.invalid_mappings == 0 {
            "passed"
        } else {
            "blocked"
        },
        format!(
            "{} valid mappings, {} invalid mappings",
            readiness.valid_mappings, readiness.invalid_mappings
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

        if !platform_user_ids.insert(mapping.platform_user_id) {
            return Err(MigrationError::UserMappingConflict(format!(
                "platform user {} is mapped more than once",
                mapping.platform_user_id
            )));
        }
    }

    Ok(())
}

async fn validate_platform_users_exist(
    state: &AppState,
    request: &SaveUserMappingsRequest,
) -> Result<(), MigrationError> {
    let platform_user_ids: Vec<Uuid> = request
        .mappings
        .iter()
        .map(|m| m.platform_user_id)
        .collect();

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
    let count: i64 =
        sqlx::query("SELECT COUNT(*) FROM migration_user_mapping WHERE migration_source_id = $1")
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
