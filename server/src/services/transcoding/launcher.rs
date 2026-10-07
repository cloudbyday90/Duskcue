use std::path::Path;

#[cfg(not(target_os = "linux"))]
use tokio::process::Command;
use tokio_process_tools::{
    DEFAULT_MAX_BUFFERED_CHUNKS, DEFAULT_READ_CHUNK_SIZE, NumBytesExt, Process,
};
use uuid::Uuid;

use super::FfmpegHandle;
use crate::domains::playback::error::PlaybackError;

pub(super) struct SpawnedFfmpeg {
    pub(super) handle: FfmpegHandle,
    pub(super) readiness: LaunchReadiness,
}

pub(super) struct LaunchReadiness {
    #[cfg(target_os = "linux")]
    bootstrap: crate::services::sandbox::launch::BootstrapReadiness,
}

pub(super) async fn spawn_session_ffmpeg(
    args: &[String],
    session_id: Uuid,
    source_path: &Path,
    segment_dir: &Path,
) -> Result<SpawnedFfmpeg, PlaybackError> {
    match spawn_ffmpeg(args, session_id, source_path, segment_dir) {
        Ok(spawned) => Ok(spawned),
        Err(error) => {
            match tokio::fs::remove_dir_all(segment_dir).await {
                Ok(()) => {}
                Err(cleanup) if cleanup.kind() == std::io::ErrorKind::NotFound => {}
                Err(cleanup) => {
                    return Err(PlaybackError::FfmpegFailed(format!(
                        "{error}; owned transcode cache cleanup failed: {cleanup}"
                    )));
                }
            }
            Err(error)
        }
    }
}

pub(super) fn spawn_ffmpeg(
    args: &[String],
    session_id: Uuid,
    source_path: &Path,
    segment_dir: &Path,
) -> Result<SpawnedFfmpeg, PlaybackError> {
    #[cfg(target_os = "linux")]
    let mut prepared = crate::services::sandbox::launch::PreparedLaunch::new(
        &crate::services::sandbox::SandboxConfig {
            media_path: source_path,
            transcode_dir: segment_dir,
        },
    )
    .map_err(|error| {
        PlaybackError::FfmpegFailed(format!("managed FFmpeg preparation failed: {error}"))
    })?;
    #[cfg(target_os = "linux")]
    let mut command = prepared.command();
    #[cfg(not(target_os = "linux"))]
    let mut command = {
        let _ = (source_path, segment_dir);
        Command::new("ffmpeg")
    };
    command
        .arg("-nostdin")
        .args(args)
        .stdin(std::process::Stdio::null());
    let handle = Process::new(command)
        .name(format!("transcode-{session_id}"))
        .stdout_and_stderr(|stream| {
            stream
                .single_subscriber()
                .lossy_without_backpressure()
                .replay_last_bytes(64.kilobytes())
                .read_chunk_size(DEFAULT_READ_CHUNK_SIZE)
                .max_buffered_chunks(DEFAULT_MAX_BUFFERED_CHUNKS)
        })
        .spawn()
        .map_err(|error| PlaybackError::FfmpegFailed(format!("FFmpeg spawn failed: {error}")))?;
    Ok(SpawnedFfmpeg {
        handle,
        readiness: LaunchReadiness {
            #[cfg(target_os = "linux")]
            bootstrap: prepared.readiness(),
        },
    })
}

impl LaunchReadiness {
    pub(super) async fn wait(self) -> Result<(), PlaybackError> {
        #[cfg(target_os = "linux")]
        {
            let filesystem = self.bootstrap.wait().await.map_err(|error| {
                PlaybackError::FfmpegFailed(format!("managed FFmpeg bootstrap failed: {error}"))
            })?;
            tracing::info!(
                ?filesystem,
                "managed FFmpeg deny-by-default filter acknowledged"
            );
        }
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "launcher_tests.rs"]
mod tests;
