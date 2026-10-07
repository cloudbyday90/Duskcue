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

use std::sync::atomic::{AtomicBool, Ordering};

use super::tests::{child, directory, shutdown};
use super::*;
use crate::services::transcoding::readiness;
use tokio::sync::Semaphore;

struct WorkerFixture {
    workers: Arc<TranscodeWorkers>,
    id: Uuid,
    path: PathBuf,
    semaphore: Arc<Semaphore>,
    cleaned: Arc<AtomicBool>,
}

async fn fixture(code: &str) -> anyhow::Result<WorkerFixture> {
    let path = directory().await?;
    let workers = Arc::new(TranscodeWorkers::default());
    let semaphore = Arc::new(Semaphore::new(1));
    let permit = Arc::clone(&semaphore).acquire_owned().await?;
    let cleaned = Arc::new(AtomicBool::new(false));
    let inspect = Arc::clone(&cleaned);
    let id = Uuid::now_v7();
    let handle = child(code)?;
    workers.start(
        WorkerLaunch {
            session_id: id,
            handle,
            permit,
            directory: path.clone(),
            shutdown: shutdown(),
        },
        |_| Next::Continue,
        move || {
            inspect.store(true, Ordering::Release);
        },
    );
    Ok(WorkerFixture {
        workers,
        id,
        path,
        semaphore,
        cleaned,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn startup_deadline_stops_owned_child_and_removes_missing_asset_cache_before_error()
-> anyhow::Result<()> {
    let WorkerFixture { workers, id, path, semaphore, cleaned } = fixture("process.stdout.write('ready\\n');setInterval(()=>{},1000);setTimeout(()=>process.exit(0),15000);").await?;
    tokio::fs::write(path.join("manifest.m3u8"), "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXT-X-PLAYLIST-TYPE:EVENT\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:2,\nseg_0000.m4s\n").await?;
    let before = tokio::time::Instant::now();
    let result = readiness::confirm(
        &workers,
        id,
        &path,
        &path.join("manifest.m3u8"),
        async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            Ok(())
        },
        before + Duration::from_millis(40),
    )
    .await;
    assert!(before.elapsed() < Duration::from_secs(5));
    assert!(matches!(result, Err(PlaybackError::FfmpegFailed(_))));
    assert!(cleaned.load(Ordering::Acquire));
    assert_eq!(semaphore.available_permits(), 1);
    assert!(!path.exists());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn successful_exit_without_referenced_assets_cleans_before_error() -> anyhow::Result<()> {
    let WorkerFixture {
        workers,
        id,
        path,
        semaphore,
        cleaned,
    } = fixture("process.stdout.write('progress=end\\n');").await?;
    tokio::fs::write(path.join("manifest.m3u8"), "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXT-X-PLAYLIST-TYPE:EVENT\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:2,\nseg_0000.m4s\n").await?;
    let mut execution = workers.execution(id).unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while execution.state() == ExecutionState::Running {
            execution.changed().await;
        }
    })
    .await?;
    assert_eq!(execution.state(), ExecutionState::Succeeded);
    let result = readiness::confirm(
        &workers,
        id,
        &path,
        &path.join("manifest.m3u8"),
        std::future::ready(Ok(())),
        tokio::time::Instant::now() + Duration::from_secs(15),
    )
    .await;
    assert!(matches!(result, Err(PlaybackError::FfmpegFailed(_))));
    assert!(cleaned.load(Ordering::Acquire));
    assert_eq!(semaphore.available_permits(), 1);
    assert!(!path.exists());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropped_startup_observer_keeps_owned_cleanup_awaitable() -> anyhow::Result<()> {
    let WorkerFixture { workers, id, path, semaphore, cleaned } = fixture("process.stdout.write('ready\\n');setInterval(()=>{},1000);setTimeout(()=>process.exit(0),15000);").await?;
    let task_workers = Arc::clone(&workers);
    let task_path = path.clone();
    let entered = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&entered);
    let task = tokio::spawn(async move {
        readiness::confirm(
            &task_workers,
            id,
            &task_path,
            &task_path.join("manifest.m3u8"),
            async move {
                observed.store(true, Ordering::Release);
                std::future::pending::<Result<(), PlaybackError>>().await
            },
            tokio::time::Instant::now() + Duration::from_secs(15),
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while !entered.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    assert_eq!(semaphore.available_permits(), 0);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(5), workers.stop(id)).await??;
    assert!(cleaned.load(Ordering::Acquire));
    assert_eq!(semaphore.available_permits(), 1);
    assert!(!path.exists());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsuccessful_exit_and_late_progress_end_cannot_report_startup_success()
-> anyhow::Result<()> {
    let WorkerFixture {
        workers,
        id,
        path,
        semaphore,
        cleaned,
    } = fixture("process.stdout.write('progress=end\\n');process.exitCode=9;").await?;
    let mut execution = workers.execution(id).unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while execution.state() == ExecutionState::Running {
            execution.changed().await;
        }
    })
    .await?;
    assert_eq!(execution.state(), ExecutionState::Failed);
    let result = readiness::confirm(
        &workers,
        id,
        &path,
        &path.join("manifest.m3u8"),
        std::future::ready(Ok(())),
        tokio::time::Instant::now() + Duration::from_secs(15),
    )
    .await;
    assert!(matches!(result, Err(PlaybackError::FfmpegCrashed)));
    assert!(cleaned.load(Ordering::Acquire));
    assert_eq!(semaphore.available_permits(), 1);
    assert!(!path.exists());
    assert_eq!(execution.state(), ExecutionState::Cancelled);
    Ok(())
}
