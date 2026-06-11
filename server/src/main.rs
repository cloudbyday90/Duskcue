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

#[cfg(not(target_env = "msvc"))]
use mimalloc::MiMalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use duskcue::config::{build_bootstrap_config, CliArgs};
use duskcue::lockfile::Lockfile;
use duskcue::logging::init_logging;
use duskcue::logging::init_metrics;
use duskcue::router::build_router;
use duskcue::services::encryption;
use duskcue::services::scheduler::{seed_default_tasks, Scheduler};
use duskcue::state::{load_runtime_config, AppState};
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

static SHUTDOWN_STARTED: AtomicBool = AtomicBool::new(false);

async fn wait_for_signal(shutdown: CancellationToken) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl+c");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to listen for SIGTERM")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received SIGINT (Ctrl+C)");
        }
        _ = terminate => {
            tracing::info!("Received SIGTERM");
        }
    }

    if SHUTDOWN_STARTED.swap(true, Ordering::SeqCst) {
        tracing::warn!("Second shutdown signal received, forcing exit");
        std::process::exit(1);
    }

    shutdown.cancel();
}

async fn connect_with_retry(database_url: &str) -> Result<sqlx::PgPool, sqlx::Error> {
    let max_attempts: u32 = 3;
    let retry_interval = Duration::from_secs(5);

    for attempt in 1..=max_attempts {
        match PgPoolOptions::new()
            .max_connections(20)
            .min_connections(2)
            .acquire_timeout(Duration::from_secs(5))
            .max_lifetime(Duration::from_secs(1800))
            .idle_timeout(Duration::from_secs(600))
            .after_connect(|conn: &mut sqlx::PgConnection, _meta| {
                Box::pin(async move {
                    sqlx::query("SET application_name = 'duskcue'")
                        .execute(conn)
                        .await?;
                    Ok(())
                })
            })
            .connect(database_url)
            .await
        {
            Ok(pool) => return Ok(pool),
            Err(e) if attempt < max_attempts => {
                tracing::warn!(
                    attempt,
                    max_attempts,
                    error = %e,
                    "Database connection attempt failed, retrying in {}s",
                    retry_interval.as_secs()
                );
                tokio::time::sleep(retry_interval).await;
            }
            Err(e) => return Err(e),
        }
    }

    unreachable!()
}

async fn validate_pg_settings(pool: &sqlx::PgPool) {
    let mut warnings = 0u32;

    match sqlx::query_scalar::<_, String>("SELECT current_setting('server_version')")
        .fetch_one(pool)
        .await
    {
        Ok(version) => {
            tracing::info!(version = %version, "PostgreSQL server version");
            if let Some(major) = version.split('.').next().and_then(|v| v.parse::<u32>().ok())
                && major < 18
            {
                tracing::warn!(
                    current = major,
                    target = 18,
                    "PostgreSQL version {major} is below target version 18 — features like native uuidv7() may not be available"
                );
                warnings += 1;
            }
        }
        Err(e) => {
            tracing::warn!("Could not determine PostgreSQL version: {e}");
        }
    }

    let result = sqlx::query(
        "SELECT name, setting FROM pg_settings WHERE name IN ('fsync', 'full_page_writes', 'synchronous_commit', 'data_checksums', 'wal_level')"
    )
    .fetch_all(pool)
    .await;

    match result {
        Ok(rows) => {
            for row in &rows {
                let name: &str = row.get("name");
                let setting: &str = row.get("setting");
                match name {
                    "fsync" if setting != "on" => {
                        tracing::warn!("PostgreSQL fsync is disabled — committed transactions may be lost on crash. Set fsync=on in postgresql.conf.");
                        warnings += 1;
                    }
                    "full_page_writes" if setting != "on" => {
                        tracing::warn!("PostgreSQL full_page_writes is disabled — torn pages may cause corruption after crash. Set full_page_writes=on.");
                        warnings += 1;
                    }
                    "synchronous_commit" if setting != "on" => {
                        tracing::warn!("PostgreSQL synchronous_commit is off — acknowledged commits may be lost on crash. Set synchronous_commit=on.");
                        warnings += 1;
                    }
                    "data_checksums" if setting != "on" => {
                        tracing::warn!("PostgreSQL data_checksums is disabled — silent corruption will not be detected. Reinitialize with initdb --data-checksums.");
                        warnings += 1;
                    }
                    "wal_level" if setting != "replica" && setting != "logical" => {
                        tracing::warn!("PostgreSQL wal_level is '{setting}' — PITR and WAL-G backups will not work. Set wal_level=replica.");
                        warnings += 1;
                    }
                    _ => {}
                }
            }
            if warnings == 0 {
                tracing::info!("PostgreSQL settings validated — all checks passed");
            } else {
                tracing::warn!("PostgreSQL settings validated with {warnings} warning(s) — review recommendations above");
            }
        }
        Err(e) => {
            tracing::warn!("Could not validate PostgreSQL settings: {e}");
        }
    }
}

#[tokio::main]
async fn main() {
    let cli = CliArgs::parse();

    let bootstrap = build_bootstrap_config(cli).unwrap_or_else(|e| {
        eprintln!("Configuration error: {e}");
        std::process::exit(1);
    });

    let _log_guard = init_logging(&bootstrap.log_level, &bootstrap.data_dir);

    tracing::info!(
        environment = %bootstrap.environment,
        "Duskcue starting"
    );

    let metrics_handle = init_metrics();
    tracing::info!("Prometheus metrics recorder initialized");

    tracing::info!("Initializing encryption key");
    let (encryption_key, _new_key_hex) = encryption::ensure_encryption_key(&bootstrap).unwrap_or_else(|e| {
        tracing::error!(error = %e, "Failed to initialize encryption key");
        eprintln!("Failed to initialize encryption key: {e}");
        std::process::exit(1);
    });

    let database_url = match bootstrap.database_url.as_deref() {
        Some(url) => url.to_string(),
        None => {
            tracing::error!("DUSKCUE_DATABASE_URL is required");
            eprintln!("DUSKCUE_DATABASE_URL is required");
            eprintln!("Set it via --database-url, DUSKCUE_DATABASE_URL env var, or config.toml");
            eprintln!("Example: DUSKCUE_DATABASE_URL=\"postgresql://duskcue:password@localhost:5432/duskcue\"");
            std::process::exit(1);
        }
    };

    tracing::info!("Acquiring startup lockfile");
    let mut lockfile = Lockfile::acquire(&bootstrap.data_dir).unwrap_or_else(|e| {
        tracing::error!(error = %e, "Failed to acquire startup lockfile");
        eprintln!("{e}");
        std::process::exit(1);
    });

    tracing::info!("Connecting to PostgreSQL");
    let pool = connect_with_retry(&database_url).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "Failed to connect to database after retries");
        eprintln!("Failed to connect to database after 3 attempts: {e}");
        std::process::exit(1);
    });
    tracing::info!("Connected to PostgreSQL");

    validate_pg_settings(&pool).await;

    tracing::info!("Running database migrations");
    sqlx::migrate!()
        .run(&pool)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "Failed to run database migrations");
            eprintln!("Migration failed: {e}");
            std::process::exit(1);
        });
    tracing::info!("Database migrations complete");

    let runtime_config = load_runtime_config(&pool, Some(&encryption_key))
        .await
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "Failed to load runtime configuration");
            eprintln!("Failed to load server configuration: {e}");
            std::process::exit(1);
        });

    let state = AppState::new_with_config(pool, bootstrap, runtime_config, metrics_handle, encryption_key);
    tracing::info!("Runtime configuration loaded");

    {
        let config = state.runtime_config.load();
        if config.is_setup_mode() {
            tracing::warn!("Auth setup not complete — server is in setup mode");
            tracing::warn!("Only setup endpoints will be accessible until initial setup is complete");
        }
    }

    tracing::info!("Seeding default scheduled tasks");
    if let Err(e) = seed_default_tasks(&state.pool).await {
        tracing::warn!(error = %e, "Failed to seed default scheduled tasks");
    }

    let app = build_router(state.clone()).with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", 48027))
        .await
        .expect("failed to bind to port 48027");

    tracing::info!("Listening on http://0.0.0.0:48027");
    tracing::info!("Duskcue ready");

    let tracker = TaskTracker::new();
    let shutdown = CancellationToken::new();
    let server_shutdown = shutdown.clone();
    let scheduler_shutdown = shutdown.clone();

    let scheduler = Arc::new(
        Scheduler::new(state.pool.clone())
            .register_executor("library_scan", |pool, task_id, config| {
                let pool = pool.clone();
                async move {
                    let mode = config.get("mode").and_then(|v| v.as_str()).unwrap_or("full");
                    tracing::info!(task_id = %task_id, mode = %mode, "Starting library scan task");

                    let libraries: Result<Vec<uuid::Uuid>, sqlx::Error> = sqlx::query_scalar(
                        "SELECT id FROM libraries WHERE deleted_at IS NULL"
                    )
                    .fetch_all(&pool)
                    .await;

                    match libraries {
                        Ok(ids) => {
                            let mut scanned = 0u64;
                            let mut total_added = 0u64;
                            let mut total_updated = 0u64;
                            let mut total_removed = 0u64;

                            for library_id in ids {
                                match duskcue::workers::library_scanner::scan_library(
                                    &pool, library_id, mode == "quick", None,
                                )
                                .await
                                {
                                    Ok(result) => {
                                        scanned += 1;
                                        total_added += result.items_created;
                                        total_updated += result.files_modified;
                                        total_removed += result.files_deleted;
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            library_id = %library_id,
                                            error = %e,
                                            "Library scan failed"
                                        );
                                    }
                                }
                            }

                            tracing::info!(
                                scanned,
                                total_added,
                                total_updated,
                                total_removed,
                                "Library scan task completed"
                            );
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to fetch libraries for scan");
                        }
                    }
                }
            }),
    );

    tracing::info!("Starting scheduled task runner");
    scheduler.start(&tracker, scheduler_shutdown).await;

    tracing::info!("Starting filesystem watcher");
    if let Err(e) = state.fs_watcher.start(&tracker, shutdown.clone()).await {
        tracing::warn!(error = %e, "Failed to start filesystem watcher — scheduled scans will still work");
    }

    tracker.spawn(async move {
        tokio::select! {
            result = axum::serve(listener, app) => {
                result.expect("server error");
            }
            _ = server_shutdown.cancelled() => {}
        }
    });

    wait_for_signal(shutdown.clone()).await;

    tracing::info!("Phase 1: Signal received — stopping HTTP listener and cancelling tasks");

    tracing::info!("Phase 2: Draining in-flight requests (up to 30s)");
    tracker.close();
    let drain_result = tokio::time::timeout(Duration::from_secs(30), tracker.wait()).await;
    if drain_result.is_err() {
        tracing::warn!("Phase 2: Drain timed out after 30s — some tasks did not complete");
    } else {
        tracing::info!("Phase 2: All tasks completed");
    }

    tracing::info!("Phase 3: Cleanup (up to 90s)");
    state.fs_watcher.stop();
    {
        let pool = state.pool.clone();
        let close_result = tokio::time::timeout(Duration::from_secs(60), async {
            pool.close().await;
        })
        .await;
        if close_result.is_err() {
            tracing::warn!("Phase 3: PG pool close timed out after 60s");
        } else {
            tracing::info!("Phase 3: PG connection pool closed");
        }
    }

    tracing::info!("Removing startup lockfile");
    lockfile.release();

    tracing::info!("Shutdown complete");
}
