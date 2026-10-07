use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio_process_tools::{GracefulShutdown, Next};

use super::*;
use crate::services::transcoding::arguments::{
    build_ffmpeg_input_args, build_hls_output_args, build_stream_mapping_args,
};
use crate::services::transcoding::lifecycle::{TranscodeWorkers, WorkerCancellation, WorkerLaunch};

struct Running {
    id: Uuid,
    pid: Option<u32>,
    directory: std::path::PathBuf,
    workers: TranscodeWorkers,
    semaphore: Arc<Semaphore>,
    seen: Arc<AtomicBool>,
    pending: WorkerCancellation,
}

impl Running {
    async fn launch(active: bool) -> anyhow::Result<Self> {
        Uuid::parse_str(&std::env::var("DUSKCUE_TEST_RESOURCE_ID")?)?;
        let source = std::fs::canonicalize(std::env::var("DUSKCUE_TEST_PLAYBACK_SOURCE")?)?;
        anyhow::ensure!(
            source.is_file() && source.file_name().is_some_and(|name| name == "source.mkv"),
            "requires owned audio-first fixture"
        );
        let id = Uuid::now_v7();
        let directory = std::env::current_dir()?
            .join(".cache/tonight-linux-launcher")
            .join(id.to_string());
        tokio::fs::create_dir_all(&directory).await?;
        let mut args = if active {
            vec!["-re".into(), "-stream_loop".into(), "-1".into()]
        } else {
            Vec::new()
        };
        args.extend(build_ffmpeg_input_args(None, &source));
        args.extend(build_stream_mapping_args(Some(2), None));
        args.extend([
            "-c:v".into(),
            "copy".into(),
            "-c:a".into(),
            "copy".into(),
            "-threads".into(),
            "1".into(),
            "-progress".into(),
            "pipe:1".into(),
        ]);
        args.extend(build_hls_output_args(
            2,
            &directory.join("seg_%04d.m4s").to_string_lossy(),
            &directory.join("manifest.m3u8").to_string_lossy(),
        ));
        let semaphore = Arc::new(Semaphore::new(1));
        let permit = Arc::clone(&semaphore).acquire_owned().await?;
        let spawned = spawn_session_ffmpeg(&args, id, &source, &directory).await?;
        let pid = spawned.handle.id();
        let workers = TranscodeWorkers::default();
        let seen = Arc::new(AtomicBool::new(false));
        let inspect = Arc::clone(&seen);
        workers.start(
            WorkerLaunch {
                session_id: id,
                handle: spawned.handle,
                permit,
                directory: directory.clone(),
                shutdown: GracefulShutdown::builder()
                    .unix_sigterm(Duration::from_millis(100))
                    .build(),
            },
            move |line| {
                if line.starts_with("frame=") {
                    inspect.store(true, Ordering::Release);
                }
                Next::Continue
            },
            || {},
        );
        let pending = workers.pending(id).unwrap();
        if let Err(error) = spawned.readiness.wait().await {
            workers.stop(id).await?;
            return Err(error.into());
        }
        Ok(Self {
            id,
            pid,
            directory,
            workers,
            semaphore,
            seen,
            pending,
        })
    }

    async fn stop(mut self) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(5), self.workers.stop(self.id)).await??;
        self.pending.acknowledge();
        anyhow::ensure!(
            self.semaphore.available_permits() == 1,
            "owned permit was not released"
        );
        anyhow::ensure!(!self.directory.exists(), "owned cache was not removed");
        if let Some(pid) = self.pid {
            anyhow::ensure!(
                !std::path::Path::new(&format!("/proc/{pid}")).exists(),
                "owned FFmpeg remains after awaited stop"
            );
        }
        Ok(())
    }
}

#[tokio::test]
#[ignore = "requires a fresh guarded Linux artifact and its mandatory packaged bootstrap"]
async fn actual_production_managed_ffmpeg_writes_a_finite_completed_hls_playlist()
-> anyhow::Result<()> {
    let running = Running::launch(false).await?;
    let proof = async {
        let playlist = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Ok(content) =
                    tokio::fs::read_to_string(running.directory.join("manifest.m3u8")).await
                    && content.contains("#EXT-X-ENDLIST")
                {
                    break content;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await?;
        anyhow::ensure!(
            running.seen.load(Ordering::Acquire),
            "no real FFmpeg progress"
        );
        anyhow::ensure!(
            playlist.contains("#EXT-X-PLAYLIST-TYPE:VOD") && playlist.contains("#EXTINF:"),
            "incomplete production playlist"
        );
        for file in ["init.mp4", "seg_0000.m4s"] {
            anyhow::ensure!(
                tokio::fs::metadata(running.directory.join(file))
                    .await?
                    .len()
                    > 0,
                "empty production asset {file}"
            );
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            while running.semaphore.available_permits() != 1 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let cleanup = running.stop().await;
    proof?;
    cleanup
}

#[tokio::test]
#[ignore = "requires a fresh guarded Linux artifact and its mandatory packaged bootstrap"]
async fn actual_active_managed_ffmpeg_releases_child_before_cache_and_permit() -> anyhow::Result<()>
{
    let running = Running::launch(true).await?;
    let proof = async {
        let pid = running
            .pid
            .ok_or_else(|| anyhow::anyhow!("owned FFmpeg PID unavailable"))?;
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let initialized = tokio::fs::metadata(running.directory.join("init.mp4"))
                    .await
                    .is_ok_and(|metadata| metadata.len() > 0);
                let segmented = tokio::fs::metadata(running.directory.join("seg_0000.m4s"))
                    .await
                    .is_ok_and(|metadata| metadata.len() > 0);
                if running.seen.load(Ordering::Acquire) && initialized && segmented {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await?;
        anyhow::ensure!(
            running.semaphore.available_permits() == 0,
            "active FFmpeg released its permit early"
        );
        anyhow::ensure!(
            std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "owned FFmpeg exited before active cancellation proof"
        );
        anyhow::ensure!(
            !running.directory.join("manifest.m3u8").exists(),
            "infinite VOD fixture unexpectedly published a final playlist"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let cleanup = running.stop().await;
    proof?;
    cleanup
}
