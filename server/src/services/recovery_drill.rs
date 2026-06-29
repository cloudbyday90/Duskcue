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

//! Recovery drill service — restores the latest eligible backup into a
//! disposable PostgreSQL instance and runs read-only structural checks.
//!
//! Owned by [`workers::recovery_drill_runner`] (scheduler adapter). The drill
//! logic lives here so future manual API endpoints can reuse the same code
//! path, mirroring the `services::backup` ↔ `workers::backup_runner` split.
//!
//! See `docs/operations/BACKUP_RECOVERY.md` → "Recovery Drill Runner" for the
//! authoritative design.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use rand::Rng;
use serde::Serialize;
use sqlx::Row;
use tokio::process::Command;
use uuid::Uuid;

use crate::domains::backup::BackupError;
use crate::services::backup::{CommandResult, find_latest_pg_dump, verify_pg_dump_file};
use crate::state::{AppState, BackupConfig};

const DEFAULT_POSTGRES_IMAGE: &str = "postgres:18-alpine";
const DEFAULT_PORT: u16 = 55433;
const DEFAULT_RESTORE_JOBS: u32 = 2;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60 * 30);
const MIN_RESTORE_JOBS: u32 = 1;
const MAX_RESTORE_JOBS: u32 = 4;
const MIN_PORT: u16 = 1024;
const MAX_PORT: u16 = 65535;
const COMPOSE_PROJECT_PREFIX: &str = "duskcue-drill-";
const POSTGRES_USER: &str = "duskcue";
const POSTGRES_DB: &str = "duskcue_drill";
const PG_ISREADY_ATTEMPTS: u32 = 60;
const PG_ISREADY_INTERVAL: Duration = Duration::from_secs(2);
const STDERR_TAIL_BYTES: usize = 512;
const STDERR_TRUNCATE_BYTES: usize = 4096;

const CORE_TABLES: &[&str] = &[
    "libraries",
    "media_items",
    "users",
    "server_config",
    "scheduled_tasks",
];

#[derive(Debug, Clone)]
pub struct DrillOptions {
    pub postgres_image: String,
    pub port: u16,
    pub keep_alive: bool,
    pub source: DrillSource,
    pub dump_path: Option<PathBuf>,
    pub restore_jobs: u32,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrillSource {
    Auto,
    PgDump,
}

#[derive(Debug, Serialize)]
pub struct RecoveryDrillStats {
    pub status: String,
    pub started_at: String,
    pub completed_at: String,
    pub duration_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disposable_postgres: Option<DisposablePostgresReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_source: Option<BackupSourceReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore: Option<RestoreReport>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub structural_checks: Vec<StructuralCheckResult>,
    pub disposal: DisposalReport,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DisposablePostgresReport {
    pub compose_project: String,
    pub image: String,
    pub port: u16,
    pub ready_after_ms: u128,
}

#[derive(Debug, Serialize)]
pub struct BackupSourceReport {
    pub kind: String,
    pub path: String,
    pub size_bytes: u64,
    pub backup_timestamp: Option<String>,
    pub pre_restore_verification: CommandResult,
}

#[derive(Debug, Serialize)]
pub struct RestoreReport {
    pub tool: String,
    pub success: bool,
    pub duration_ms: u128,
    pub stderr_summary: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct StructuralCheckResult {
    pub name: String,
    pub passed: bool,
    pub details: String,
}

#[derive(Debug, Serialize, Default)]
pub struct DisposalReport {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

impl DrillOptions {
    /// Build options from the scheduled-task `config` JSONB plus runtime
    /// `BackupConfig`. Task config takes precedence over runtime config;
    /// both fall back to documented defaults.
    pub fn from_state_and_task(state: &AppState, task_config: &serde_json::Value) -> Self {
        let backup_config = state.runtime_config.load().backup.clone();
        Self::from_config_and_task(&backup_config, task_config)
    }

    /// Pure config extractor — testable without an `AppState`.
    pub fn from_config_and_task(
        _backup_config: &BackupConfig,
        task_config: &serde_json::Value,
    ) -> Self {
        let postgres_image = task_config
            .get("postgres_image")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_POSTGRES_IMAGE.to_string());

        let port = task_config
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|v| u16::try_from(v).unwrap_or(MAX_PORT))
            .unwrap_or(DEFAULT_PORT)
            .clamp(MIN_PORT, MAX_PORT);

        let keep_alive = task_config
            .get("keep_alive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let source = match task_config
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("auto")
        {
            "pg_dump" => DrillSource::PgDump,
            _ => DrillSource::Auto,
        };

        let dump_path = task_config
            .get("dump_path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);

        let restore_jobs = task_config
            .get("restore_jobs")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(DEFAULT_RESTORE_JOBS)
            .clamp(MIN_RESTORE_JOBS, MAX_RESTORE_JOBS);

        let timeout = task_config
            .get("timeout_seconds")
            .and_then(|v| v.as_u64())
            .filter(|v| *v > 0)
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_TIMEOUT);

        DrillOptions {
            postgres_image,
            port,
            keep_alive,
            source,
            dump_path,
            restore_jobs,
            timeout,
        }
    }
}

/// Run the full recovery drill. Returns the populated stats regardless of
/// success/failure so the worker can persist evidence; the caller decides
/// whether to mark the run as failed based on `stats.status`.
pub async fn run_recovery_drill(
    state: &AppState,
    options: &DrillOptions,
) -> Result<RecoveryDrillStats, BackupError> {
    let started_at = Utc::now();
    let start = Instant::now();
    let mut stats = RecoveryDrillStats {
        status: "unavailable".to_string(),
        started_at: started_at.to_rfc3339(),
        completed_at: started_at.to_rfc3339(),
        duration_ms: 0,
        skip_reason: None,
        disposable_postgres: None,
        backup_source: None,
        restore: None,
        structural_checks: Vec::new(),
        disposal: DisposalReport {
            status: "not_started".to_string(),
            stderr: None,
        },
        errors: Vec::new(),
    };

    let backup_config = state.runtime_config.load().backup.clone();

    if !docker_is_available().await {
        stats.status = "unavailable".to_string();
        stats.skip_reason = Some("docker is not available on this host".to_string());
        finalize_stats(&mut stats, start);
        return Ok(stats);
    }

    if !backup_config.pg_dump_enabled {
        stats.status = "skipped".to_string();
        stats.skip_reason = Some("pg_dump backups are disabled in server config".to_string());
        finalize_stats(&mut stats, start);
        return Ok(stats);
    }

    let dump_path = match resolve_dump_path(&backup_config, options).await? {
        Some(path) => path,
        None => {
            stats.status = "skipped".to_string();
            stats.skip_reason =
                Some("no eligible pg_dump files in configured storage path".to_string());
            finalize_stats(&mut stats, start);
            return Ok(stats);
        }
    };

    let pre_restore_verification = match verify_pg_dump_file(&backup_config, &dump_path).await {
        Ok(result) => result,
        Err(err) => {
            stats
                .errors
                .push(format!("pre-restore pg_restore --list failed: {err}"));
            stats.status = "failed".to_string();
            finalize_stats(&mut stats, start);
            return Ok(stats);
        }
    };

    let backup_timestamp = dump_path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(parse_dump_timestamp)
        .map(|dt| dt.to_rfc3339());
    let backup_size = match tokio::fs::metadata(&dump_path).await {
        Ok(metadata) => metadata.len(),
        Err(err) => {
            stats
                .errors
                .push(format!("failed to stat dump file: {err}"));
            stats.status = "failed".to_string();
            finalize_stats(&mut stats, start);
            return Ok(stats);
        }
    };

    stats.backup_source = Some(BackupSourceReport {
        kind: "pg_dump".to_string(),
        path: dump_path.display().to_string(),
        size_bytes: backup_size,
        backup_timestamp,
        pre_restore_verification,
    });

    let run_id = Uuid::now_v7();
    let project = format!("{COMPOSE_PROJECT_PREFIX}{}", run_id.simple());
    let password = generate_password();
    let port = options.port;

    let compose_dir = std::env::temp_dir().join(&project);
    let compose_path =
        match write_compose_file(&compose_dir, &options.postgres_image, port, &password).await {
            Ok(path) => path,
            Err(err) => {
                stats
                    .errors
                    .push(format!("failed to write compose file: {err}"));
                stats.status = "failed".to_string();
                finalize_stats(&mut stats, start);
                return Ok(stats);
            }
        };

    let startup_start = Instant::now();
    let compose_path_str = compose_path.to_string_lossy().into_owned();
    match bring_up_disposable_postgres(&project, &compose_path_str, options.timeout).await {
        Ok(()) => {
            stats.disposable_postgres = Some(DisposablePostgresReport {
                compose_project: project.clone(),
                image: options.postgres_image.clone(),
                port,
                ready_after_ms: startup_start.elapsed().as_millis(),
            });
        }
        Err(err) => {
            stats
                .errors
                .push(format!("failed to start disposable postgres: {err}"));
            stats.status = "failed".to_string();
            stats.disposal = dispose(
                &project,
                &compose_path_str,
                &compose_dir,
                options.keep_alive,
            )
            .await;
            finalize_stats(&mut stats, start);
            return Ok(stats);
        }
    }

    let database_url = build_database_url(port, &password);
    let restore_outcome = restore_pg_dump(&database_url, &dump_path, options).await;
    let restore_report = match restore_outcome {
        Ok(report) => report,
        Err(err) => {
            stats.errors.push(format!("pg_restore failed: {err}"));
            RestoreReport {
                tool: "pg_restore".to_string(),
                success: false,
                duration_ms: 0,
                stderr_summary: String::new(),
            }
        }
    };
    let restore_succeeded = restore_report.success;
    if !restore_succeeded && !restore_report.stderr_summary.is_empty() {
        stats.errors.push(format!(
            "pg_restore stderr: {}",
            restore_report.stderr_summary
        ));
    }
    stats.restore = Some(restore_report);

    if restore_succeeded {
        match run_structural_checks(&database_url).await {
            Ok(checks) => {
                stats.structural_checks = checks.clone();
                if checks.iter().all(|c| c.passed) {
                    stats.status = "passed".to_string();
                } else {
                    stats.status = "failed".to_string();
                    let failed: Vec<&str> = checks
                        .iter()
                        .filter(|c| !c.passed)
                        .map(|c| c.name.as_str())
                        .collect();
                    stats
                        .errors
                        .push(format!("structural checks failed: {}", failed.join(", ")));
                }
            }
            Err(err) => {
                stats.errors.push(format!("structural check error: {err}"));
                stats.status = "failed".to_string();
            }
        }
    } else {
        stats.status = "failed".to_string();
    }

    stats.disposal = dispose(
        &project,
        &compose_path_str,
        &compose_dir,
        options.keep_alive,
    )
    .await;
    if stats.disposal.status == "cleanup_failed"
        && let Some(stderr) = stats.disposal.stderr.as_ref()
    {
        stats
            .errors
            .push(format!("disposal cleanup failed: {stderr}"));
    }

    finalize_stats(&mut stats, start);
    Ok(stats)
}

async fn docker_is_available() -> bool {
    let mut cmd = Command::new("docker");
    cmd.arg("version")
        .arg("--format")
        .arg("{{.Server.Version}}");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    matches!(cmd.status().await, Ok(status) if status.success())
}

async fn resolve_dump_path(
    config: &BackupConfig,
    options: &DrillOptions,
) -> Result<Option<PathBuf>, BackupError> {
    if let Some(explicit) = options.dump_path.as_ref() {
        return validate_dump_under_storage(config, explicit)
            .await
            .map(Some);
    }
    match options.source {
        DrillSource::Auto | DrillSource::PgDump => match find_latest_pg_dump(config).await {
            Ok(path) => Ok(Some(path)),
            Err(BackupError::InvalidConfig(_)) => Ok(None),
            Err(other) => Err(other),
        },
    }
}

async fn validate_dump_under_storage(
    config: &BackupConfig,
    path: &Path,
) -> Result<PathBuf, BackupError> {
    let base = tokio::fs::canonicalize(&config.pg_dump_storage_path).await?;
    let target = tokio::fs::canonicalize(path).await?;
    if !target.starts_with(&base) {
        return Err(BackupError::InvalidConfig(
            "explicit dump_path must be inside configured pg_dump storage path".to_string(),
        ));
    }
    Ok(target)
}

async fn write_compose_file(
    compose_dir: &Path,
    image: &str,
    port: u16,
    password: &str,
) -> Result<PathBuf, BackupError> {
    tokio::fs::create_dir_all(compose_dir).await?;

    let compose = format!(
        r#"services:
  postgres:
    image: {image}
    environment:
      POSTGRES_DB: {db}
      POSTGRES_USER: {user}
      POSTGRES_PASSWORD: {password}
      POSTGRES_INITDB_ARGS: "--data-checksums"
      PGDATA: /var/lib/postgresql/data/pgdata
    ports:
      - "127.0.0.1:{port}:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U \"{user}\" -d \"{db}\""]
      interval: 2s
      timeout: 5s
      retries: 30
      start_period: 5s
    volumes:
      - postgres-data:/var/lib/postgresql/data
    stop_grace_period: 30s

volumes:
  postgres-data:
"#,
        image = image,
        db = POSTGRES_DB,
        user = POSTGRES_USER,
        password = password,
        port = port,
    );

    let path = compose_dir.join("docker-compose.yml");
    tokio::fs::write(&path, compose).await?;
    Ok(path)
}

async fn bring_up_disposable_postgres(
    project: &str,
    compose_path: &str,
    timeout: Duration,
) -> Result<(), BackupError> {
    let up_result = run_docker_compose(
        project,
        compose_path,
        &["up", "-d"],
        Duration::from_secs(120),
        true,
    )
    .await;
    if let Err(err) = up_result {
        let _ = cleanup_compose(project, compose_path).await;
        return Err(err);
    }

    if let Err(err) = wait_for_pg_ready(project, compose_path).await {
        let _ = cleanup_compose(project, compose_path).await;
        return Err(err);
    }
    let _ = timeout;
    Ok(())
}

async fn wait_for_pg_ready(project: &str, compose_path: &str) -> Result<(), BackupError> {
    for _ in 0..PG_ISREADY_ATTEMPTS {
        let mut cmd = Command::new("docker");
        cmd.args([
            "compose",
            "-f",
            compose_path,
            "-p",
            project,
            "exec",
            "-T",
            "postgres",
            "pg_isready",
            "-U",
            POSTGRES_USER,
            "-d",
            POSTGRES_DB,
        ]);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Ok(status) = cmd.status().await
            && status.success()
        {
            return Ok(());
        }
        tokio::time::sleep(PG_ISREADY_INTERVAL).await;
    }
    Err(BackupError::CommandTimeout {
        tool: "pg_isready".to_string(),
        timeout_seconds: PG_ISREADY_ATTEMPTS as u64 * PG_ISREADY_INTERVAL.as_secs(),
    })
}

async fn restore_pg_dump(
    database_url: &str,
    dump_path: &Path,
    options: &DrillOptions,
) -> Result<RestoreReport, BackupError> {
    let jobs = options.restore_jobs.to_string();
    let mut command = Command::new("pg_restore");
    command
        .arg("--no-owner")
        .arg("--no-privileges")
        .arg("--role")
        .arg(POSTGRES_USER)
        .arg("--jobs")
        .arg(&jobs)
        .arg("--dbname")
        .arg(database_url)
        .arg(dump_path);
    command.kill_on_drop(true);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let start = Instant::now();
    let child = command
        .spawn()
        .map_err(|e| BackupError::CommandUnavailable {
            tool: "pg_restore".to_string(),
            reason: e.to_string(),
        })?;

    let output = tokio::time::timeout(options.timeout, child.wait_with_output())
        .await
        .map_err(|_| BackupError::CommandTimeout {
            tool: "pg_restore".to_string(),
            timeout_seconds: options.timeout.as_secs(),
        })?
        .map_err(|e| BackupError::CommandUnavailable {
            tool: "pg_restore".to_string(),
            reason: e.to_string(),
        })?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr_summary = tail(&stderr, STDERR_TAIL_BYTES);
    Ok(RestoreReport {
        tool: "pg_restore".to_string(),
        success: output.status.success(),
        duration_ms: start.elapsed().as_millis(),
        stderr_summary,
    })
}

async fn run_structural_checks(
    database_url: &str,
) -> Result<Vec<StructuralCheckResult>, BackupError> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(10))
        .connect(database_url)
        .await?;

    let mut checks = Vec::with_capacity(3);

    match check_schema_migrations(&pool).await {
        Ok((count, passed)) => checks.push(StructuralCheckResult {
            name: "schema_migrations_applied".to_string(),
            passed,
            details: format!("{count} migrations"),
        }),
        Err(err) => checks.push(StructuralCheckResult {
            name: "schema_migrations_applied".to_string(),
            passed: false,
            details: format!("error: {err}"),
        }),
    }

    match check_core_tables(&pool).await {
        Ok((present, missing)) => {
            let passed = missing.is_empty();
            let details = if passed {
                present.join(", ")
            } else {
                format!("missing: {}", missing.join(", "))
            };
            checks.push(StructuralCheckResult {
                name: "core_tables_present".to_string(),
                passed,
                details,
            });
        }
        Err(err) => checks.push(StructuralCheckResult {
            name: "core_tables_present".to_string(),
            passed: false,
            details: format!("error: {err}"),
        }),
    }

    match sample_row_counts(&pool).await {
        Ok(summary) => checks.push(StructuralCheckResult {
            name: "row_count_sample".to_string(),
            passed: true,
            details: summary,
        }),
        Err(err) => checks.push(StructuralCheckResult {
            name: "row_count_sample".to_string(),
            passed: false,
            details: format!("error: {err}"),
        }),
    }

    pool.close().await;
    Ok(checks)
}

async fn check_schema_migrations(pool: &sqlx::PgPool) -> Result<(i64, bool), sqlx::Error> {
    let table_exists: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name = '_sqlx_migrations'",
    )
    .fetch_optional(pool)
    .await?
    .flatten();

    let Some(count) = table_exists.filter(|c| *c > 0) else {
        return Ok((0, false));
    };

    let applied: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = true")
            .fetch_one(pool)
            .await?;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(pool)
        .await?;

    let _ = count;
    Ok((applied, applied == total && total > 0))
}

async fn check_core_tables(pool: &sqlx::PgPool) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = 'public'
          AND table_type = 'BASE TABLE'
        "#,
    )
    .fetch_all(pool)
    .await?;

    let existing: std::collections::HashSet<String> = rows
        .into_iter()
        .filter_map(|row| row.try_get::<String, _>("table_name").ok())
        .collect();

    let mut present = Vec::new();
    let mut missing = Vec::new();
    for table in CORE_TABLES {
        if existing.contains(*table) {
            present.push(table.to_string());
        } else {
            missing.push(table.to_string());
        }
    }
    present.sort();
    missing.sort();
    Ok((present, missing))
}

async fn sample_row_counts(pool: &sqlx::PgPool) -> Result<String, sqlx::Error> {
    // Static SQL only — sqlx 0.9's SqlSafeStr guard rejects `format!`-built
    // queries. Table names come from the compile-time CORE_TABLES constant,
    // so this query is fully static and audited.
    let counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            (SELECT COUNT(*) FROM "libraries")  AS libraries,
            (SELECT COUNT(*) FROM "media_items") AS media_items,
            (SELECT COUNT(*) FROM "users")       AS users,
            (SELECT COUNT(*) FROM "server_config") AS server_config,
            (SELECT COUNT(*) FROM "scheduled_tasks") AS scheduled_tasks
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(format!(
        "libraries={}, media_items={}, users={}, server_config={}, scheduled_tasks={}",
        counts.0, counts.1, counts.2, counts.3, counts.4
    ))
}

async fn dispose(
    project: &str,
    compose_path: &str,
    compose_dir: &Path,
    keep_alive: bool,
) -> DisposalReport {
    if keep_alive {
        return DisposalReport {
            status: "kept_alive".to_string(),
            stderr: None,
        };
    }
    let cleanup = cleanup_compose(project, compose_path).await;
    let _ = tokio::fs::remove_dir_all(compose_dir).await;
    match cleanup {
        Ok(()) => DisposalReport {
            status: "removed".to_string(),
            stderr: None,
        },
        Err(stderr) => DisposalReport {
            status: "cleanup_failed".to_string(),
            stderr: Some(stderr),
        },
    }
}

async fn cleanup_compose(project: &str, compose_path: &str) -> Result<(), String> {
    let mut cmd = Command::new("docker");
    cmd.args([
        "compose",
        "-f",
        compose_path,
        "-p",
        project,
        "down",
        "-v",
        "--remove-orphans",
    ]);
    cmd.kill_on_drop(true);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("docker compose down spawn failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }
    Ok(())
}

async fn run_docker_compose(
    project: &str,
    compose_path: &str,
    args: &[&str],
    timeout: Duration,
    require_success: bool,
) -> Result<(), BackupError> {
    let mut cmd = Command::new("docker");
    let mut full_args = vec!["compose", "-f", compose_path, "-p", project];
    full_args.extend_from_slice(args);
    cmd.args(&full_args);
    cmd.kill_on_drop(true);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let child = cmd.spawn().map_err(|e| BackupError::CommandUnavailable {
        tool: "docker".to_string(),
        reason: e.to_string(),
    })?;

    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| BackupError::CommandTimeout {
            tool: "docker".to_string(),
            timeout_seconds: timeout.as_secs(),
        })?
        .map_err(|e| BackupError::CommandUnavailable {
            tool: "docker".to_string(),
            reason: e.to_string(),
        })?;

    if require_success && !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BackupError::CommandFailed {
            tool: "docker".to_string(),
            exit_code: output.status.code(),
            stderr: truncate(stderr.trim(), STDERR_TRUNCATE_BYTES),
        });
    }
    Ok(())
}

fn generate_password() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 32] = rng.random();
    hex_encode(&bytes)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
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

fn build_database_url(port: u16, password: &str) -> String {
    format!("postgresql://{POSTGRES_USER}:{password}@127.0.0.1:{port}/{POSTGRES_DB}")
}

fn truncate(value: &str, max_len: usize) -> String {
    if value.chars().count() <= max_len {
        return value.to_string();
    }
    let mut truncated: String = value.chars().take(max_len).collect();
    truncated.push_str("...");
    truncated
}

fn tail(value: &str, max_len: usize) -> String {
    if value.chars().count() <= max_len {
        return value.to_string();
    }
    let skip = value.chars().count() - max_len;
    value.chars().skip(skip).collect()
}

fn finalize_stats(stats: &mut RecoveryDrillStats, start: Instant) {
    let now = Utc::now();
    stats.completed_at = now.to_rfc3339();
    stats.duration_ms = start.elapsed().as_millis();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::BackupConfig;

    #[test]
    fn options_defaults_when_config_empty() {
        let options =
            DrillOptions::from_config_and_task(&BackupConfig::default(), &serde_json::json!({}));
        assert_eq!(options.postgres_image, DEFAULT_POSTGRES_IMAGE);
        assert_eq!(options.port, DEFAULT_PORT);
        assert!(!options.keep_alive);
        assert_eq!(options.source, DrillSource::Auto);
        assert_eq!(options.restore_jobs, DEFAULT_RESTORE_JOBS);
        assert_eq!(options.timeout, DEFAULT_TIMEOUT);
        assert!(options.dump_path.is_none());
    }

    #[test]
    fn options_override_from_task_config() {
        let config = serde_json::json!({
            "postgres_image": "postgres:17-alpine",
            "port": 6000,
            "keep_alive": true,
            "source": "pg_dump",
            "dump_path": "/tmp/explicit.dump",
            "restore_jobs": 4,
            "timeout_seconds": 120
        });
        let options = DrillOptions::from_config_and_task(&BackupConfig::default(), &config);
        assert_eq!(options.postgres_image, "postgres:17-alpine");
        assert_eq!(options.port, 6000);
        assert!(options.keep_alive);
        assert_eq!(options.source, DrillSource::PgDump);
        assert_eq!(
            options.dump_path.as_deref(),
            Some(std::path::Path::new("/tmp/explicit.dump"))
        );
        assert_eq!(options.restore_jobs, 4);
        assert_eq!(options.timeout, Duration::from_secs(120));
    }

    #[test]
    fn options_clamp_port_into_valid_range() {
        let low = DrillOptions::from_config_and_task(
            &BackupConfig::default(),
            &serde_json::json!({"port": 80}),
        );
        assert_eq!(low.port, MIN_PORT);

        // 70000 overflows u16; saturating cast sends it to MAX_PORT, which is
        // then clamped to MAX_PORT (still invalid for ephemeral ports, but the
        // operator gets a clear out-of-range failure when the drill tries to
        // bind). The point of the clamp is to reject obviously-wrong values.
        let high = DrillOptions::from_config_and_task(
            &BackupConfig::default(),
            &serde_json::json!({"port": 70000}),
        );
        assert_eq!(high.port, MAX_PORT);

        let sub_max = DrillOptions::from_config_and_task(
            &BackupConfig::default(),
            &serde_json::json!({"port": 65535}),
        );
        assert_eq!(sub_max.port, MAX_PORT);
    }

    #[test]
    fn options_clamp_restore_jobs_into_valid_range() {
        let high = DrillOptions::from_config_and_task(
            &BackupConfig::default(),
            &serde_json::json!({"restore_jobs": 99}),
        );
        assert_eq!(high.restore_jobs, MAX_RESTORE_JOBS);

        let low = DrillOptions::from_config_and_task(
            &BackupConfig::default(),
            &serde_json::json!({"restore_jobs": 0}),
        );
        assert_eq!(low.restore_jobs, MIN_RESTORE_JOBS);
    }

    #[test]
    fn options_unknown_source_falls_back_to_auto() {
        let options = DrillOptions::from_config_and_task(
            &BackupConfig::default(),
            &serde_json::json!({"source": "unknown"}),
        );
        assert_eq!(options.source, DrillSource::Auto);
    }

    #[test]
    fn options_empty_image_falls_back_to_default() {
        let options = DrillOptions::from_config_and_task(
            &BackupConfig::default(),
            &serde_json::json!({"postgres_image": ""}),
        );
        assert_eq!(options.postgres_image, DEFAULT_POSTGRES_IMAGE);
    }

    #[test]
    fn options_zero_timeout_falls_back_to_default() {
        let options = DrillOptions::from_config_and_task(
            &BackupConfig::default(),
            &serde_json::json!({"timeout_seconds": 0}),
        );
        assert_eq!(options.timeout, DEFAULT_TIMEOUT);
    }

    #[test]
    fn generate_password_is_hex_and_sufficiently_long() {
        let pw = generate_password();
        assert_eq!(pw.len(), 64);
        assert!(pw.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn build_database_url_includes_loopback_and_credentials() {
        let url = build_database_url(55433, "deadbeef");
        assert!(url.starts_with("postgresql://duskcue:deadbeef@127.0.0.1:55433/"));
        assert!(url.ends_with(POSTGRES_DB));
    }

    #[test]
    fn truncate_keeps_short_strings_verbatim() {
        assert_eq!(truncate("abc", 10), "abc");
    }

    #[test]
    fn truncate_appends_marker_when_trimming() {
        let long = "abcdefghij".repeat(10);
        let truncated = truncate(&long, 10);
        assert_eq!(truncated.chars().count(), 13);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn tail_keeps_short_strings_verbatim() {
        assert_eq!(tail("abc", 10), "abc");
    }

    #[test]
    fn tail_returns_last_n_characters() {
        // "abcdefghij" repeated 10 times = 100 chars. Last 5 chars are "fghij".
        let long = "abcdefghij".repeat(10);
        let tailed = tail(&long, 5);
        assert_eq!(tailed.chars().count(), 5);
        assert_eq!(tailed, "fghij");
    }

    #[test]
    fn parse_dump_timestamp_reads_duskcue_filename() {
        let parsed =
            parse_dump_timestamp("duskcue_scheduled_20260627T030000Z.dump").expect("parses");
        assert_eq!(parsed.to_rfc3339(), "2026-06-27T03:00:00+00:00");
    }

    #[test]
    fn parse_dump_timestamp_returns_none_for_unknown_names() {
        assert!(parse_dump_timestamp("manual.dump").is_none());
        assert!(parse_dump_timestamp("duskcue_scheduled_20260627.dump").is_none());
        assert!(parse_dump_timestamp("not-a-timestamp").is_none());
    }

    #[test]
    fn project_name_uses_simple_uuid_suffix() {
        let uuid = Uuid::now_v7();
        let project = format!("{COMPOSE_PROJECT_PREFIX}{}", uuid.simple());
        assert!(project.starts_with(COMPOSE_PROJECT_PREFIX));
        assert!(project.len() > COMPOSE_PROJECT_PREFIX.len());
    }

    #[test]
    fn finalize_stats_populates_duration_and_completion() {
        let mut stats = RecoveryDrillStats {
            status: "passed".to_string(),
            started_at: Utc::now().to_rfc3339(),
            completed_at: String::new(),
            duration_ms: 0,
            skip_reason: None,
            disposable_postgres: None,
            backup_source: None,
            restore: None,
            structural_checks: Vec::new(),
            disposal: DisposalReport {
                status: "removed".to_string(),
                stderr: None,
            },
            errors: Vec::new(),
        };
        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(5));
        finalize_stats(&mut stats, start);
        assert!(stats.duration_ms >= 5);
        assert!(!stats.completed_at.is_empty());
    }

    #[test]
    fn stats_serializes_with_skipped_optional_sections() {
        let stats = RecoveryDrillStats {
            status: "skipped".to_string(),
            started_at: Utc::now().to_rfc3339(),
            completed_at: Utc::now().to_rfc3339(),
            duration_ms: 12,
            skip_reason: Some("no eligible dump".to_string()),
            disposable_postgres: None,
            backup_source: None,
            restore: None,
            structural_checks: Vec::new(),
            disposal: DisposalReport {
                status: "not_started".to_string(),
                stderr: None,
            },
            errors: Vec::new(),
        };
        let value = serde_json::to_value(&stats).expect("serializes");
        assert_eq!(value["status"], "skipped");
        assert_eq!(value["skip_reason"], "no eligible dump");
        assert!(value.get("disposable_postgres").is_none());
        assert!(value.get("backup_source").is_none());
        assert!(value.get("restore").is_none());
        assert!(value.get("structural_checks").is_none());
        assert!(value.get("errors").is_none());
    }

    #[test]
    fn stats_serializes_full_passed_drill() {
        let stats = RecoveryDrillStats {
            status: "passed".to_string(),
            started_at: "2026-06-27T03:00:00+00:00".to_string(),
            completed_at: "2026-06-27T03:02:30+00:00".to_string(),
            duration_ms: 150_000,
            skip_reason: None,
            disposable_postgres: Some(DisposablePostgresReport {
                compose_project: "duskcue-drill-abc".to_string(),
                image: "postgres:18-alpine".to_string(),
                port: 55433,
                ready_after_ms: 5_000,
            }),
            backup_source: Some(BackupSourceReport {
                kind: "pg_dump".to_string(),
                path: "/data/backups/dump/duskcue_scheduled_20260627T030000Z.dump".to_string(),
                size_bytes: 1024,
                backup_timestamp: Some("2026-06-27T03:00:00+00:00".to_string()),
                pre_restore_verification: CommandResult {
                    tool: "pg_restore".to_string(),
                    success: true,
                    exit_code: Some(0),
                    duration_ms: 1_234,
                    stdout: "...".to_string(),
                    stderr: String::new(),
                },
            }),
            restore: Some(RestoreReport {
                tool: "pg_restore".to_string(),
                success: true,
                duration_ms: 90_000,
                stderr_summary: String::new(),
            }),
            structural_checks: vec![StructuralCheckResult {
                name: "schema_migrations_applied".to_string(),
                passed: true,
                details: "15 migrations".to_string(),
            }],
            disposal: DisposalReport {
                status: "removed".to_string(),
                stderr: None,
            },
            errors: Vec::new(),
        };
        let value = serde_json::to_value(&stats).expect("serializes");
        assert_eq!(value["status"], "passed");
        assert_eq!(value["disposable_postgres"]["port"], 55433);
        assert_eq!(
            value["backup_source"]["path"],
            "/data/backups/dump/duskcue_scheduled_20260627T030000Z.dump"
        );
        assert_eq!(value["structural_checks"][0]["passed"], true);
    }

    #[test]
    fn drill_source_equality_distinguishes_variants() {
        assert_eq!(DrillSource::Auto, DrillSource::Auto);
        assert_ne!(DrillSource::Auto, DrillSource::PgDump);
    }

    #[test]
    fn compose_file_contains_loopback_binding_and_random_password() {
        // Validates the compose template format string at the source level —
        // regression guard against accidental binding to 0.0.0.0 or dropping
        // the healthcheck.
        let temp = std::env::temp_dir().join(format!(
            "duskcue-recovery-drill-test-{}",
            Uuid::now_v7().simple()
        ));
        let path = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(write_compose_file(
                &temp,
                "postgres:18-alpine",
                55433,
                "secret",
            ))
            .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("127.0.0.1:55433:5432"));
        assert!(content.contains("POSTGRES_PASSWORD: secret"));
        assert!(content.contains("POSTGRES_INITDB_ARGS: \"--data-checksums\""));
        assert!(content.contains("pg_isready"));
        assert!(content.contains("stop_grace_period: 30s"));
        let _ = std::fs::remove_dir_all(&temp);
    }
}
