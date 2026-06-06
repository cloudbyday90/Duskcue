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

use clap::Parser;
use duskcue::config::{build_bootstrap_config, CliArgs};
use duskcue::router::build_router;
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
        "Duskcue starting (environment: {})",
        bootstrap.environment
    );

    let app = build_router();
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", 48027))
        .await
        .expect("failed to bind to port 48027");

    tracing::info!("Listening on http://0.0.0.0:48027");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    tracing::info!("Server stopped");
}
