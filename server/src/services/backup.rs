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
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use chrono::Utc;
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
    ensure_pg_dump_enabled(&config)?;

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
        Some(verify_pg_dump_file(&config, &path).await?)
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

    if !verify_wal_g && !verify_pg_dump {
        return Err(BackupError::InvalidConfig(
            "at least one verification target must be enabled".to_string(),
        ));
    }

    let wal_g = if verify_wal_g {
        ensure_wal_g_enabled(&config)?;
        Some(
            run_wal_g_command(
                &config,
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
            Some(path) => validate_existing_dump_path(&config, path).await?,
            None => find_latest_pg_dump(&config).await?,
        };
        Some(verify_pg_dump_file(&config, &path).await?)
    } else {
        None
    };

    Ok(VerificationResult {
        status: "verified".to_string(),
        wal_g,
        pg_dump,
    })
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
