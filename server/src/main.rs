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
use std::time::Duration;

use clap::Parser;
use duskcue::config::{build_bootstrap_config, CliArgs};
use duskcue::router::build_router;
use duskcue::state::{load_runtime_config, AppState};
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use tracing_subscriber::EnvFilter;

static SHUTDOWN_STARTED: AtomicBool = AtomicBool::new(false);

async fn shutdown_signal() {
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

    tracing::info!("Shutting down gracefully...");
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
    let result = sqlx::query(
        "SELECT name, setting FROM pg_settings WHERE name IN ('fsync', 'full_page_writes', 'data_checksums', 'wal_level')"
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
                    }
                    "full_page_writes" if setting != "on" => {
                        tracing::warn!("PostgreSQL full_page_writes is disabled — torn pages may cause corruption after crash. Set full_page_writes=on.");
                    }
                    "data_checksums" if setting != "on" => {
                        tracing::warn!("PostgreSQL data_checksums is disabled — silent corruption will not be detected. Reinitialize with initdb --data-checksums.");
                    }
                    "wal_level" if setting != "replica" && setting != "logical" => {
                        tracing::warn!("PostgreSQL wal_level is '{setting}' — PITR and WAL-G backups will not work. Set wal_level=replica.");
                    }
                    _ => {}
                }
            }
            tracing::info!("PostgreSQL settings validated");
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

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&bootstrap.log_level)),
        )
        .init();

    tracing::info!(
        environment = %bootstrap.environment,
        "Duskcue starting"
    );

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
    tracing::info!("Startup lockfile acquired");

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

    let runtime_config = load_runtime_config(&pool)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "Failed to load runtime configuration");
            eprintln!("Failed to load server configuration: {e}");
            std::process::exit(1);
        });

    let state = AppState::new_with_config(pool, bootstrap, runtime_config);
    tracing::info!("Runtime configuration loaded");

    {
        let config = state.runtime_config.load();
        if config.is_setup_mode() {
            tracing::warn!("Auth setup not complete — server is in setup mode");
            tracing::warn!("Only setup endpoints will be accessible until initial setup is complete");
        }
    }

    tracing::info!("Starting scheduled task runner (not yet implemented)");

    let app = build_router(state.clone()).with_state(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", 48027))
        .await
        .expect("failed to bind to port 48027");

    tracing::info!("Listening on http://0.0.0.0:48027");
    tracing::info!("Duskcue ready");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    tracing::info!("Server stopped");
}
