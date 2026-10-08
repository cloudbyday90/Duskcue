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

use super::*;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio_process_tools::{
    DEFAULT_MAX_BUFFERED_CHUNKS, DEFAULT_READ_CHUNK_SIZE, NumBytesExt, Process,
};

#[test]
fn a_successful_wait_timeout_does_not_confirm_physical_exit() {
    let timeout = tokio_process_tools::WaitForCompletionResult::<()>::Timeout {
        timeout: Duration::from_secs(1),
    };
    assert!(!physically_complete(&timeout));
    assert!(physically_complete(
        &tokio_process_tools::WaitForCompletionResult::Completed(())
    ));
}

pub(super) fn child(code: &str) -> anyhow::Result<FfmpegHandle> {
    let mut command = Command::new("node");
    command
        .args(["--max-old-space-size=16", "-e", code])
        .stdin(std::process::Stdio::null());
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);
    process(command)
}

fn process(command: Command) -> anyhow::Result<FfmpegHandle> {
    #[cfg(target_os = "windows")]
    let command = {
        let mut command = command;
        command.creation_flags(0x08000000);
        command
    };
    Ok(Process::new(command)
        .name("owned-transcode-lifecycle-fixture")
        .stdout_and_stderr(|stream| {
            stream
                .single_subscriber()
                .lossy_without_backpressure()
                .replay_last_bytes(4.kilobytes())
                .read_chunk_size(DEFAULT_READ_CHUNK_SIZE)
                .max_buffered_chunks(DEFAULT_MAX_BUFFERED_CHUNKS)
        })
        .spawn()?)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted guarded tests and verified FFmpeg 8.1.2 on child-only PATH"]
async fn actual_long_running_ffmpeg_is_terminated_before_its_permit_and_cache_are_released()
-> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("DUSKCUE_TEST_RESOURCE_ID").is_ok(),
        "requires guarded resource workflow"
    );
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-re",
            "-f",
            "lavfi",
            "-i",
            "color=size=160x90:rate=24",
            "-t",
            "60",
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-threads",
            "1",
            "-progress",
            "pipe:1",
            "-f",
            "null",
            "-",
        ])
        .stdin(std::process::Stdio::null());
    let handle = process(command)?;
    let path = directory().await?;
    let workers = TranscodeWorkers::default();
    let semaphore = Arc::new(Semaphore::new(1));
    let permit = Arc::clone(&semaphore).acquire_owned().await?;
    let seen = Arc::new(AtomicBool::new(false));
    let inspect = Arc::clone(&seen);
    let id = Uuid::now_v7();
    workers.start(
        WorkerLaunch {
            session_id: id,
            handle,
            permit,
            directory: path.clone(),
            shutdown: shutdown(),
        },
        move |line| {
            if line.starts_with("frame=") {
                inspect.store(true, Ordering::Release);
            }
            Next::Continue
        },
        || {},
    );
    ready(&seen).await?;
    assert_eq!(semaphore.available_permits(), 0);
    let before = std::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(5), workers.stop(id)).await??;
    assert!(before.elapsed() < Duration::from_secs(5));
    assert_eq!(semaphore.available_permits(), 1);
    assert!(!path.exists());
    Ok(())
}

pub(super) async fn directory() -> anyhow::Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("duskcue-worker-fixture-{}", Uuid::now_v7()));
    tokio::fs::create_dir_all(&path).await?;
    Ok(path)
}

async fn ready(seen: &AtomicBool) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !seen.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    Ok(())
}

pub(super) fn shutdown() -> GracefulShutdown {
    GracefulShutdown::builder()
        .unix_sigterm(Duration::from_millis(100))
        .windows_ctrl_break(Duration::from_millis(100))
        .build()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_cancels_a_child_that_closed_stdout_without_exiting_and_releases_its_permit()
-> anyhow::Result<()> {
    let handle = child(
        "process.stdout.write('ready\\n');process.stdout.end();setInterval(()=>{},1000);setTimeout(()=>process.exit(0),15000);",
    )?;
    let path = directory().await?;
    let workers = TranscodeWorkers::default();
    let semaphore = Arc::new(Semaphore::new(1));
    let permit = Arc::clone(&semaphore).acquire_owned().await?;
    let seen = Arc::new(AtomicBool::new(false));
    let inspect = Arc::clone(&seen);
    let id = Uuid::now_v7();
    workers.start(
        WorkerLaunch {
            session_id: id,
            handle,
            permit,
            directory: path.clone(),
            shutdown: shutdown(),
        },
        move |line| {
            if line == "ready" {
                inspect.store(true, Ordering::Release);
            }
            Next::Continue
        },
        || {},
    );
    ready(&seen).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(semaphore.available_permits(), 0);
    tokio::time::timeout(Duration::from_secs(5), workers.stop(id)).await??;
    assert_eq!(semaphore.available_permits(), 1);
    assert!(!path.exists());
    workers.stop(id).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_waits_for_fresh_cleanup_after_failure_without_releasing_a_live_child()
-> anyhow::Result<()> {
    let handle = child(
        "process.stdout.write('ready\\n');setInterval(()=>{},1000);setTimeout(()=>process.exit(0),15000);",
    )?;
    let path = directory().await?;
    let workers = TranscodeWorkers::default();
    let semaphore = Arc::new(Semaphore::new(1));
    let permit = Arc::clone(&semaphore).acquire_owned().await?;
    let seen = Arc::new(AtomicBool::new(false));
    let inspect = Arc::clone(&seen);
    let id = Uuid::now_v7();
    workers.start(
        WorkerLaunch {
            session_id: id,
            handle,
            permit,
            directory: path.clone(),
            shutdown: shutdown(),
        },
        move |line| {
            if line == "ready" {
                inspect.store(true, Ordering::Release);
            }
            Next::Continue
        },
        || {},
    );
    ready(&seen).await?;
    tokio::fs::remove_dir(&path).await?;
    tokio::fs::write(&path, b"owned cleanup failure fixture").await?;
    let error = tokio::time::timeout(Duration::from_secs(5), workers.stop(id))
        .await?
        .unwrap_err();
    assert!(matches!(error, PlaybackError::FfmpegFailed(_)));
    assert_eq!(semaphore.available_permits(), 1);
    assert!(path.is_file());
    tokio::fs::remove_file(&path).await?;
    tokio::fs::create_dir_all(&path).await?;
    let (first, second) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(workers.stop(id), workers.stop(id))
    })
    .await?;
    first?;
    second?;
    assert!(!path.exists());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abandoned_pending_worker_remains_awaitable_until_confirmed_cleanup() -> anyhow::Result<()>
{
    let handle = child(
        "process.stdout.write('ready\\n');setInterval(()=>{},1000);setTimeout(()=>process.exit(0),15000);",
    )?;
    let path = directory().await?;
    let workers = TranscodeWorkers::default();
    let semaphore = Arc::new(Semaphore::new(1));
    let permit = Arc::clone(&semaphore).acquire_owned().await?;
    let seen = Arc::new(AtomicBool::new(false));
    let inspect = Arc::clone(&seen);
    let cleaned = Arc::new(AtomicBool::new(false));
    let cleanup = Arc::clone(&cleaned);
    let id = Uuid::now_v7();
    workers.start(
        WorkerLaunch {
            session_id: id,
            handle,
            permit,
            directory: path.clone(),
            shutdown: shutdown(),
        },
        move |line| {
            if line == "ready" {
                inspect.store(true, Ordering::Release);
            }
            Next::Continue
        },
        move || {
            cleanup.store(true, Ordering::Release);
        },
    );
    ready(&seen).await?;
    let pending = workers.pending(id).unwrap();
    drop(pending);
    tokio::time::timeout(Duration::from_secs(5), workers.stop(id)).await??;
    assert!(cleaned.load(Ordering::Acquire));
    assert_eq!(semaphore.available_permits(), 1);
    assert!(!path.exists());
    Ok(())
}
