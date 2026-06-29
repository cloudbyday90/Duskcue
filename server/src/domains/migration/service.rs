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

use std::net::IpAddr;
use std::path::Path;

use chrono::Utc;
use serde_json::{Value, json};
use sqlx::Row;
use tokio::net::lookup_host;
use url::Url;
use uuid::Uuid;

use crate::state::AppState;
use crate::state::NetworkMode;

use super::error::MigrationError;
use super::types::*;

const ACTIVE_STATUSES: &[&str] = &["discovering", "matching", "importing"];
const MAX_PLEX_DATABASE_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const API_CONFIG_TIMEOUT_SECONDS: u64 = 10;
const API_CONFIG_MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

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
) -> Result<MigrationActionResponse, MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;

    Ok(MigrationActionResponse {
        migration_source_id: id,
        status: source.status,
        message: "Migration source is registered; source-specific connection checks are handled by later Phase 14 adapters".to_string(),
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
}

pub async fn discover_source(
    state: &AppState,
    id: Uuid,
) -> Result<MigrationActionResponse, MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;

    Ok(MigrationActionResponse {
        migration_source_id: id,
        status: source.status,
        message: "Migration source is ready for discovery; source-specific discovery is implemented in later Phase 14 tasks".to_string(),
    })
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

    let message = if request.dry_run.unwrap_or(false) {
        "Migration dry-run request accepted; preflight and runner behavior are implemented in later Phase 14 tasks"
    } else {
        "Migration start request accepted; runner behavior is implemented in later Phase 14 tasks"
    };

    Ok(MigrationActionResponse {
        migration_source_id: id,
        status: source.status,
        message: message.to_string(),
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
        let status = update_source_status(state, id, "cancelled").await?;
        return Ok(MigrationActionResponse {
            migration_source_id: id,
            status,
            message: "Migration cancellation recorded".to_string(),
        });
    }

    Ok(MigrationActionResponse {
        migration_source_id: id,
        status: source.status,
        message: "Migration is not running; no cancellation was needed".to_string(),
    })
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

async fn update_source_status(
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
