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

use std::path::Path;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::time::Instant;

use super::lifecycle::{ExecutionMonitor, ExecutionState, TranscodeWorkers};
use crate::domains::playback::PlaybackError;

#[path = "readiness_cache.rs"]
mod cache;
use cache::{OwnedCache, stable};

#[path = "readiness_mp4.rs"]
mod mp4;
#[path = "readiness_playlist.rs"]
mod playlist;

pub(super) const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_PLAYLIST_BYTES: u64 = 1_048_576;

pub(super) async fn wait(
    directory: &Path,
    manifest: &Path,
    monitor: &mut ExecutionMonitor,
    deadline: Instant,
) -> Result<(), PlaybackError> {
    tokio::time::timeout_at(deadline, inspect_until_ready(directory, manifest, monitor))
        .await
        .map_err(|_| failure("first playable HLS startup deadline exceeded"))?
}

pub(super) async fn confirm(
    workers: &TranscodeWorkers,
    session_id: uuid::Uuid,
    directory: &Path,
    manifest: &Path,
    bootstrap: impl std::future::Future<Output = Result<(), PlaybackError>>,
    deadline: Instant,
) -> Result<(), PlaybackError> {
    let mut pending = workers
        .pending(session_id)
        .ok_or_else(|| failure("HLS startup worker ownership was lost"))?;
    let startup = tokio::time::timeout_at(deadline, async {
        let mut execution = workers
            .execution(session_id)
            .ok_or_else(|| failure("HLS startup worker ownership was lost"))?;
        bootstrap.await?;
        wait(directory, manifest, &mut execution, deadline).await
    })
    .await
    .unwrap_or_else(|_| Err(failure("first playable HLS startup deadline exceeded")));
    if let Err(error) = startup {
        workers.stop(session_id).await?;
        return Err(error);
    }
    pending.acknowledge();
    Ok(())
}

async fn inspect_until_ready(
    directory: &Path,
    manifest: &Path,
    monitor: &mut ExecutionMonitor,
) -> Result<(), PlaybackError> {
    let cache = OwnedCache::new(directory, manifest).await?;
    loop {
        let state = monitor.state();
        require_healthy(state)?;
        let playable = probe(&cache, state).await?;
        require_healthy(monitor.state())?;
        if playable {
            return Ok(());
        }
        if state == ExecutionState::Succeeded {
            return Err(failure("FFmpeg completed without playable HLS assets"));
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(100)) => {},
            _ = monitor.changed() => {},
        }
    }
}

async fn probe(cache: &OwnedCache, state: ExecutionState) -> Result<bool, PlaybackError> {
    let Some(mut file) = cache.open("manifest.m3u8").await? else {
        return Ok(false);
    };
    let before = file.metadata().await.map_err(io_error)?;
    if before.len() > MAX_PLAYLIST_BYTES {
        return Err(failure("oversized HLS startup playlist"));
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut file)
        .take(MAX_PLAYLIST_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(io_error)?;
    let after = file.metadata().await.map_err(io_error)?;
    if bytes.len() as u64 > MAX_PLAYLIST_BYTES {
        return Err(failure("oversized HLS startup playlist"));
    }
    if !stable(&before, &after) {
        return Ok(false);
    }
    let content = std::str::from_utf8(&bytes)
        .map_err(|_| failure("invalid HLS startup playlist encoding"))?;
    let Some(references) = playlist::parse(content)? else {
        return Ok(false);
    };
    if references.publication == playlist::Publication::Vod
        && (state != ExecutionState::Succeeded || !references.ended)
    {
        return Ok(false);
    }
    Ok(mp4::complete(
        cache,
        &references.initialization,
        mp4::Asset::Initialization,
    )
    .await?
        && mp4::complete(cache, &references.segment, mp4::Asset::Fragment).await?)
}

fn require_healthy(state: ExecutionState) -> Result<(), PlaybackError> {
    match state {
        ExecutionState::Running | ExecutionState::Succeeded => Ok(()),
        ExecutionState::Failed => Err(PlaybackError::FfmpegCrashed),
        ExecutionState::Cancelled => Err(failure("HLS startup observer cancelled")),
    }
}

fn failure(message: &'static str) -> PlaybackError {
    PlaybackError::FfmpegFailed(message.into())
}
fn io_error(error: std::io::Error) -> PlaybackError {
    PlaybackError::FfmpegFailed(format!("HLS startup cache inspection failed: {error}"))
}

#[cfg(test)]
#[path = "readiness_tests.rs"]
mod tests;
