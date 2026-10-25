// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

mod http;
mod signals;
pub use http::HttpServer;
pub use signals::Signals;

use crate::state::AppState;
use std::time::Duration;
use tokio_util::task::TaskTracker;

#[derive(Debug, thiserror::Error)]
#[error("application shutdown did not complete")]
pub struct ShutdownError;

pub async fn shutdown(
    state: &AppState,
    tracker: &TaskTracker,
    http: &mut HttpServer,
) -> Result<(), ShutdownError> {
    if let Some(owner) = state.live_tv_refresh.get() {
        owner.close();
    }
    http.close();
    tracker.close();
    let drain = async {
        tokio::try_join!(
            http.wait(),
            async {
                tracker.wait().await;
                Ok::<_, ShutdownError>(())
            },
            async {
                if let Some(owner) = state.live_tv_refresh.get() {
                    owner.drain().await.map_err(|_| ShutdownError)?;
                }
                Ok::<_, ShutdownError>(())
            },
        )?;
        Ok::<_, ShutdownError>(())
    };
    tokio::time::timeout(Duration::from_secs(30), drain)
        .await
        .map_err(|_| ShutdownError)??;
    state.fs_watcher.stop();
    tokio::time::timeout(Duration::from_secs(60), state.pool.close())
        .await
        .map_err(|_| ShutdownError)?;
    Ok(())
}
