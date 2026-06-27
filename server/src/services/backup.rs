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
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Datelike, Utc};
use serde::Serialize;
use tokio::process::Command;
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::config::BootstrapConfig;
use crate::domains::backup::BackupError;
use crate::state::{AppState, BackupConfig, WalGStorageType};

static BACKUP_OPERATION_LOCK: OnceLock<Arc<Mutex<()>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct CommandResult {
    pub tool: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalGStatusCheck {
    pub enabled: bool,
    pub storage_type: WalGStorageType,
    pub storage_prefix: String,
    pub version: CommandResult,
    pub backup_list: CommandResult,
    pub backup_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PgDumpResult {
    pub path: String,
    pub size_bytes: u64,
    pub dump: CommandResult,
    pub verification: Option<CommandResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerificationResult {
    pub status: String,
    pub wal_g: Option<CommandResult>,
    pub pg_dump: Option<CommandResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduledBackupResult {
    pub status: String,
    pub wal_g: Option<CommandResult>,
    pub pg_dump: Option<PgDumpResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetentionCleanupResult {
    pub status: String,
    pub wal_g: Option<CommandResult>,
    pub pg_dump_deleted: usize,
    pub pg_dump_retained: usize,
    pub pg_dump_unknown_retained: usize,
}

pub async fn check_wal_g_status(state: &AppState) -> Result<WalGStatusCheck, BackupError> {
    let config = state.runtime_config.load().backup.clone();
    ensure_wal_g_enabled(&config)?;

    let version = run_wal_g_command(
        &config,
        &state.bootstrap,
        ["--version"],
        Duration::from_secs(15),
        false,
    )
    .await?;

    let backup_list = run_wal_g_command(
        &config,
        &state.bootstrap,
        ["backup-list", "--json"],
        Duration::from_secs(60),
        true,
    )
    .await?;

    let backup_count = serde_json::from_str::<serde_json::Value>(&backup_list.stdout)
        .ok()
        .and_then(|value| value.as_array().map(Vec::len));

    Ok(WalGStatusCheck {
        enabled: config.wal_g_enabled,
        storage_type: config.wal_g_storage_type.clone(),
        storage_prefix: wal_g_storage_prefix(&config)?,
        version,
        backup_list,
        backup_count,
    })
}

pub async fn run_pg_dump(
    state: &AppState,
    label: Option<&str>,
    verify: bool,
) -> Result<PgDumpResult, BackupError> {
    let _guard = acquire_operation_lock()?;
    let config = state.runtime_config.load().backup.clone();
    run_pg_dump_locked(state, &config, label, verify).await
}

pub async fn run_scheduled_backup(state: &AppState) -> Result<ScheduledBackupResult, BackupError> {
    let _guard = acquire_operation_lock()?;
    let config = state.runtime_config.load().backup.clone();

    if !config.wal_g_enabled && !config.pg_dump_enabled {
        return Err(BackupError::InvalidConfig(
            "WAL-G and pg_dump backups are both disabled".to_string(),
        ));
    }

    let wal_g = if config.wal_g_enabled {
        Some(run_wal_g_base_backup_locked(state, &config).await?)
    } else {
        None
    };

    let pg_dump = if config.pg_dump_enabled {
        Some(run_pg_dump_locked(state, &config, Some("scheduled"), true).await?)
    } else {
        None
    };

    Ok(ScheduledBackupResult {
        status: "completed".to_string(),
        wal_g,
        pg_dump,
    })
}

pub async fn run_retention_cleanup(
    state: &AppState,
) -> Result<RetentionCleanupResult, BackupError> {
    let _guard = acquire_operation_lock()?;
    let config = state.runtime_config.load().backup.clone();

    let wal_g = if config.wal_g_enabled {
        Some(run_wal_g_retention_locked(&config, &state.bootstrap).await?)
    } else {
        None
    };

    let (pg_dump_deleted, pg_dump_retained, pg_dump_unknown_retained) = if config.pg_dump_enabled {
        cleanup_pg_dumps(&config).await?
    } else {
        (0, 0, 0)
    };

    Ok(RetentionCleanupResult {
        status: "completed".to_string(),
        wal_g,
        pg_dump_deleted,
        pg_dump_retained,
        pg_dump_unknown_retained,
    })
}

async fn run_pg_dump_locked(
    state: &AppState,
    config: &BackupConfig,
    label: Option<&str>,
    verify: bool,
) -> Result<PgDumpResult, BackupError> {
    ensure_pg_dump_enabled(config)?;

    let database_url =
        state.bootstrap.database_url.as_deref().ok_or_else(|| {
            BackupError::InvalidConfig("database URL is not configured".to_string())
        })?;

    let dump_dir = PathBuf::from(&config.pg_dump_storage_path);
    tokio::fs::create_dir_all(&dump_dir).await?;

    let path = dump_dir.join(pg_dump_filename(label));
    let mut command = Command::new("pg_dump");
    command
        .arg("--format=custom")
        .arg("--file")
        .arg(&path)
        .arg(database_url);

    let dump = run_command("pg_dump", command, Duration::from_secs(60 * 60 * 2), true).await?;
    let metadata = tokio::fs::metadata(&path).await?;

    let verification = if verify {
        Some(verify_pg_dump_file(config, &path).await?)
    } else {
        None
    };

    Ok(PgDumpResult {
        path: path.display().to_string(),
        size_bytes: metadata.len(),
        dump,
        verification,
    })
}

pub async fn verify_backups(
    state: &AppState,
    verify_wal_g: bool,
    verify_pg_dump: bool,
    pg_dump_path: Option<&str>,
) -> Result<VerificationResult, BackupError> {
    let _guard = acquire_operation_lock()?;
    let config = state.runtime_config.load().backup.clone();
    verify_backups_locked(state, &config, verify_wal_g, verify_pg_dump, pg_dump_path).await
}

async fn verify_backups_locked(
    state: &AppState,
    config: &BackupConfig,
    verify_wal_g: bool,
    verify_pg_dump: bool,
    pg_dump_path: Option<&str>,
) -> Result<VerificationResult, BackupError> {
    if !verify_wal_g && !verify_pg_dump {
        return Err(BackupError::InvalidConfig(
            "at least one verification target must be enabled".to_string(),
        ));
    }

    let wal_g = if verify_wal_g {
        ensure_wal_g_enabled(config)?;
        Some(
            run_wal_g_command(
                config,
                &state.bootstrap,
                ["wal-verify", "integrity"],
                Duration::from_secs(60 * 15),
                true,
            )
            .await?,
        )
    } else {
        None
    };

    let pg_dump = if verify_pg_dump {
        let path = match pg_dump_path {
            Some(path) => validate_existing_dump_path(config, path).await?,
            None => find_latest_pg_dump(config).await?,
        };
        Some(verify_pg_dump_file(config, &path).await?)
    } else {
        None
    };

    Ok(VerificationResult {
        status: "verified".to_string(),
        wal_g,
        pg_dump,
    })
}

async fn run_wal_g_base_backup_locked(
    state: &AppState,
    config: &BackupConfig,
) -> Result<CommandResult, BackupError> {
    ensure_wal_g_enabled(config)?;
    let pgdata = postgres_data_dir(state).await?;

    let mut args = vec!["backup-push".to_string(), pgdata.display().to_string()];
    if config.data_checksums {
        args.push("--verify".to_string());
    }
    run_wal_g_command(
        config,
        &state.bootstrap,
        args,
        Duration::from_secs(60 * 60 * 2),
        true,
    )
    .await
}

async fn run_wal_g_retention_locked(
    config: &BackupConfig,
    bootstrap: &BootstrapConfig,
) -> Result<CommandResult, BackupError> {
    ensure_wal_g_enabled(config)?;
    let retain = config.wal_g_retention_full.max(1).to_string();

    run_wal_g_command(
        config,
        bootstrap,
        ["delete", "retain", retain.as_str(), "--full", "--confirm"],
        Duration::from_secs(60 * 30),
        true,
    )
    .await
}

async fn postgres_data_dir(state: &AppState) -> Result<PathBuf, BackupError> {
    let path = std::env::var_os("PGDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| state.bootstrap.data_dir.join("postgres"));

    if tokio::fs::metadata(&path).await.is_err() {
        return Err(BackupError::InvalidConfig(format!(
            "PostgreSQL data directory does not exist: {}",
            path.display()
        )));
    }

    Ok(path)
}

async fn cleanup_pg_dumps(config: &BackupConfig) -> Result<(usize, usize, usize), BackupError> {
    ensure_pg_dump_enabled(config)?;

    let mut entries = tokio::fs::read_dir(&config.pg_dump_storage_path).await?;
    let mut files = Vec::new();
    let now = Utc::now();

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("dump") {
            continue;
        }

        let modified = entry
            .metadata()
            .await
            .and_then(|metadata| metadata.modified())
            .map(DateTime::<Utc>::from)
            .unwrap_or(now);
        let timestamp = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(parse_dump_timestamp)
            .unwrap_or(modified);
        files.push((path, timestamp));
    }

    let daily_cutoff = now - chrono::Duration::days(config.pg_dump_retention_daily as i64);
    let monthly_cutoff = now - chrono::Duration::days(config.pg_dump_retention_monthly as i64 * 31);
    let mut retained = 0usize;
    let mut unknown_retained = 0usize;
    let mut deleted = 0usize;
    let mut monthly_keep: HashMap<(i32, u32), usize> = HashMap::new();
    let mut delete_flags = vec![false; files.len()];

    for (idx, (path, timestamp)) in files.iter().enumerate() {
        let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
            unknown_retained += 1;
            continue;
        };

        if parse_dump_timestamp(filename).is_none() {
            unknown_retained += 1;
            continue;
        }

        if *timestamp >= daily_cutoff {
            retained += 1;
            continue;
        }

        if *timestamp < monthly_cutoff {
            delete_flags[idx] = true;
            continue;
        }

        let key = (timestamp.year(), timestamp.month());
        match monthly_keep.get(&key).copied() {
            Some(existing_idx) if files[existing_idx].1 < *timestamp => {
                delete_flags[existing_idx] = true;
                monthly_keep.insert(key, idx);
            }
            Some(_) => {
                delete_flags[idx] = true;
            }
            None => {
                monthly_keep.insert(key, idx);
                retained += 1;
            }
        }
    }

    for (idx, (path, _)) in files.iter().enumerate() {
        if delete_flags[idx] {
            tokio::fs::remove_file(path).await?;
            deleted += 1;
        }
    }

    Ok((deleted, retained, unknown_retained))
}

fn parse_dump_timestamp(filename: &str) -> Option<DateTime<Utc>> {
    let stamp = filename
        .strip_suffix(".dump")?
        .rsplit('_')
        .next()
        .filter(|value| value.len() == 16)?;

    chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%dT%H%M%SZ")
        .ok()
        .map(|value| value.and_utc())
}

async fn verify_pg_dump_file(
    config: &BackupConfig,
    path: &Path,
) -> Result<CommandResult, BackupError> {
    ensure_pg_dump_enabled(config)?;

    let mut command = Command::new("pg_restore");
    command.arg("--list").arg(path);

    run_command("pg_restore", command, Duration::from_secs(60 * 15), true).await
}

async fn run_wal_g_command<I, S>(
    config: &BackupConfig,
    bootstrap: &BootstrapConfig,
    args: I,
    timeout: Duration,
    require_success: bool,
) -> Result<CommandResult, BackupError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new("wal-g");
    command.args(args);
    apply_wal_g_env(&mut command, config, bootstrap)?;
    run_command("wal-g", command, timeout, require_success).await
}

async fn run_command(
    tool: &str,
    mut command: Command,
    timeout: Duration,
    require_success: bool,
) -> Result<CommandResult, BackupError> {
    command.kill_on_drop(true);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let start = Instant::now();
    let child = command
        .spawn()
        .map_err(|e| BackupError::CommandUnavailable {
            tool: tool.to_string(),
            reason: e.to_string(),
        })?;

    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| BackupError::CommandTimeout {
            tool: tool.to_string(),
            timeout_seconds: timeout.as_secs(),
        })?
        .map_err(|e| BackupError::CommandUnavailable {
            tool: tool.to_string(),
            reason: e.to_string(),
        })?;

    let result = CommandResult {
        tool: tool.to_string(),
        success: output.status.success(),
        exit_code: output.status.code(),
        duration_ms: start.elapsed().as_millis(),
        stdout: truncate_output(String::from_utf8_lossy(&output.stdout).trim()),
        stderr: truncate_output(String::from_utf8_lossy(&output.stderr).trim()),
    };

    if require_success && !result.success {
        return Err(BackupError::CommandFailed {
            tool: tool.to_string(),
            exit_code: result.exit_code,
            stderr: result.stderr.clone(),
        });
    }

    Ok(result)
}

fn apply_wal_g_env(
    command: &mut Command,
    config: &BackupConfig,
    bootstrap: &BootstrapConfig,
) -> Result<(), BackupError> {
    match config.wal_g_storage_type {
        WalGStorageType::Local => {
            command.env("WALG_FILE_PREFIX", wal_g_storage_prefix(config)?);
        }
        WalGStorageType::S3 => {
            command.env("WALG_S3_PREFIX", wal_g_storage_prefix(config)?);
            if !config.wal_g_s3_endpoint.trim().is_empty() {
                command.env("AWS_ENDPOINT", config.wal_g_s3_endpoint.trim());
            }
            if !config.wal_g_s3_region.trim().is_empty() {
                command.env("AWS_REGION", config.wal_g_s3_region.trim());
            }
        }
    }

    if wal_g_encryption_active(config)
        && let Some(key) = bootstrap.encryption_key.as_deref()
    {
        command.env("WALG_LIBSODIUM_KEY", key);
    }

    Ok(())
}

fn wal_g_storage_prefix(config: &BackupConfig) -> Result<String, BackupError> {
    match config.wal_g_storage_type {
        WalGStorageType::Local => {
            let path = config.wal_g_storage_path.trim();
            if path.is_empty() {
                return Err(BackupError::InvalidConfig(
                    "WAL-G local storage path is empty".to_string(),
                ));
            }
            Ok(path.to_string())
        }
        WalGStorageType::S3 => {
            let bucket = config.wal_g_s3_bucket.trim();
            if bucket.is_empty() {
                return Err(BackupError::InvalidConfig(
                    "WAL-G S3 bucket is not configured".to_string(),
                ));
            }
            let prefix = config.wal_g_s3_prefix.trim().trim_matches('/');
            if prefix.is_empty() {
                Ok(format!("s3://{bucket}"))
            } else {
                Ok(format!("s3://{bucket}/{prefix}"))
            }
        }
    }
}

fn wal_g_encryption_active(config: &BackupConfig) -> bool {
    config.wal_g_encryption_enabled
        || (config.wal_g_encryption_auto_s3
            && matches!(config.wal_g_storage_type, WalGStorageType::S3))
}

fn ensure_wal_g_enabled(config: &BackupConfig) -> Result<(), BackupError> {
    if config.wal_g_enabled {
        Ok(())
    } else {
        Err(BackupError::InvalidConfig(
            "WAL-G backups are disabled".to_string(),
        ))
    }
}

fn ensure_pg_dump_enabled(config: &BackupConfig) -> Result<(), BackupError> {
    if config.pg_dump_enabled {
        Ok(())
    } else {
        Err(BackupError::InvalidConfig(
            "pg_dump backups are disabled".to_string(),
        ))
    }
}

fn pg_dump_filename(label: Option<&str>) -> String {
    let label = label
        .map(sanitize_label)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "manual".to_string());
    format!(
        "duskcue_{label}_{}.dump",
        Utc::now().format("%Y%m%dT%H%M%SZ")
    )
}

fn sanitize_label(label: &str) -> String {
    label
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch == '-' || ch == '_' {
                Some(ch)
            } else {
                None
            }
        })
        .take(64)
        .collect()
}

async fn validate_existing_dump_path(
    config: &BackupConfig,
    path: &str,
) -> Result<PathBuf, BackupError> {
    let base = tokio::fs::canonicalize(&config.pg_dump_storage_path).await?;
    let target = tokio::fs::canonicalize(path).await?;

    if !target.starts_with(&base) {
        return Err(BackupError::InvalidConfig(
            "pg_dump verification path must be inside configured storage path".to_string(),
        ));
    }

    Ok(target)
}

async fn find_latest_pg_dump(config: &BackupConfig) -> Result<PathBuf, BackupError> {
    let mut entries = tokio::fs::read_dir(&config.pg_dump_storage_path).await?;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("dump") {
            continue;
        }
        let metadata = entry.metadata().await?;
        let modified = metadata.modified()?;
        if newest
            .as_ref()
            .is_none_or(|(current, _)| modified > *current)
        {
            newest = Some((modified, path));
        }
    }

    newest.map(|(_, path)| path).ok_or_else(|| {
        BackupError::InvalidConfig("no pg_dump files found in configured storage path".to_string())
    })
}

fn acquire_operation_lock() -> Result<OwnedMutexGuard<()>, BackupError> {
    BACKUP_OPERATION_LOCK
        .get_or_init(|| Arc::new(Mutex::new(())))
        .clone()
        .try_lock_owned()
        .map_err(|_| BackupError::OperationInProgress)
}

fn truncate_output(value: impl AsRef<str>) -> String {
    const MAX_LEN: usize = 4096;
    let value = value.as_ref();
    if value.chars().count() <= MAX_LEN {
        return value.to_string();
    }

    format!("{}...", value.chars().take(MAX_LEN).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_label_removes_path_and_shell_characters() {
        assert_eq!(
            sanitize_label("../Nightly Backup; rm -rf /"),
            "nightlybackuprm-rf"
        );
    }

    #[test]
    fn pg_dump_filename_uses_safe_extension() {
        let name = pg_dump_filename(Some("../Prod Backup"));
        assert!(name.starts_with("duskcue_prodbackup_"));
        assert!(name.ends_with(".dump"));
        assert!(!name.contains('/'));
        assert!(!name.contains('\\'));
    }

    #[test]
    fn parse_dump_timestamp_reads_generated_filename() {
        let timestamp = parse_dump_timestamp("duskcue_scheduled_20260627T030405Z.dump")
            .expect("timestamp should parse");

        assert_eq!(timestamp.to_rfc3339(), "2026-06-27T03:04:05+00:00");
    }

    #[test]
    fn parse_dump_timestamp_rejects_unknown_filename() {
        assert!(parse_dump_timestamp("manual-backup.dump").is_none());
        assert!(parse_dump_timestamp("duskcue_scheduled_20260627.dump").is_none());
    }

    #[test]
    fn wal_g_s3_prefix_omits_empty_suffix() {
        let config = BackupConfig {
            wal_g_storage_type: WalGStorageType::S3,
            wal_g_s3_bucket: "bucket".to_string(),
            wal_g_s3_prefix: String::new(),
            ..BackupConfig::default()
        };

        assert_eq!(wal_g_storage_prefix(&config).unwrap(), "s3://bucket");
    }

    #[test]
    fn wal_g_s3_prefix_trims_slashes() {
        let config = BackupConfig {
            wal_g_storage_type: WalGStorageType::S3,
            wal_g_s3_bucket: "bucket".to_string(),
            wal_g_s3_prefix: "/nested/path/".to_string(),
            ..BackupConfig::default()
        };

        assert_eq!(
            wal_g_storage_prefix(&config).unwrap(),
            "s3://bucket/nested/path"
        );
    }
}
