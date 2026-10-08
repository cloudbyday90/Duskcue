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

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::{oneshot, watch};
use tokio_process_tools::{
    DEFAULT_MAX_BUFFERED_CHUNKS, DEFAULT_READ_CHUNK_SIZE, GracefulShutdown, NumBytesExt, Process,
    WaitForCompletionResult,
};

use super::launch::PreparedLaunch;

#[path = "owned_capture.rs"]
mod capture;
use capture::OwnedCapture;

struct Observer {
    cancellation: watch::Sender<u8>,
    armed: bool,
}

impl Observer {
    fn acknowledge(mut self) {
        self.armed = false;
        self.cancellation.send_replace(2);
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        if self.armed {
            self.cancellation.send_replace(1);
        }
    }
}

pub(crate) async fn output(
    command: Command,
    prepared: PreparedLaunch,
    owned_output: PathBuf,
) -> io::Result<std::process::Output> {
    output_inner(command, prepared, owned_output, Duration::ZERO).await
}

async fn output_inner(
    command: Command,
    prepared: PreparedLaunch,
    owned_output: PathBuf,
    bootstrap_observation_delay: Duration,
) -> io::Result<std::process::Output> {
    let handle = Process::new(command)
        .name("storyboard-ffmpeg")
        .stdout_and_stderr(|stream| {
            stream
                .broadcast()
                .reliable_with_backpressure()
                .replay_last_bytes(64.kilobytes())
                .read_chunk_size(DEFAULT_READ_CHUNK_SIZE)
                .max_buffered_chunks(DEFAULT_MAX_BUFFERED_CHUNKS)
        })
        .spawn()
        .map_err(|error| io::Error::other(error.to_string()))?;
    let mut capture = Some(OwnedCapture::attach(handle.stdout(), handle.stderr()));
    let readiness = prepared.readiness();
    let (cancellation, mut cancelled) = watch::channel(0);
    let observer = Observer {
        cancellation,
        armed: true,
    };
    let (completion, result) = oneshot::channel();
    tokio::spawn(async move {
        let grace = Duration::from_secs(
            crate::state::ResourceLimitsConfig::default().ffmpeg_shutdown_grace_secs,
        );
        let shutdown = GracefulShutdown::builder()
            .unix_sigterm(grace)
            .windows_ctrl_break(grace)
            .build();
        let mut process = handle.terminate_on_drop(shutdown.clone());
        let ready = if let Some(error) = capture.as_mut().and_then(OwnedCapture::take_startup_error)
        {
            Err(error)
        } else {
            tokio::select! {
                result = async {
                    if !bootstrap_observation_delay.is_zero() {
                        tokio::time::sleep(bootstrap_observation_delay).await;
                    }
                    readiness.wait().await
                } => result.map(|_| ()),
                _ = cancelled.changed() => Err(io::Error::new(io::ErrorKind::Interrupted, "storyboard observer cancelled")),
            }
        };
        let outcome = if let Err(error) = ready {
            Err(error)
        } else {
            let exited = tokio::select! {
                result = process.wait_for_completion(Duration::from_secs(3600)) => {
                    match result {
                        Ok(WaitForCompletionResult::Completed(status)) => Ok(status),
                        Ok(WaitForCompletionResult::Timeout { .. }) => Err(io::Error::new(io::ErrorKind::TimedOut, "storyboard FFmpeg timed out")),
                        Err(error) => Err(io::Error::other(error.to_string())),
                    }
                },
                _ = cancelled.changed() => Err(io::Error::new(io::ErrorKind::Interrupted, "storyboard observer cancelled")),
            };
            match exited {
                Ok(status) => match capture.take() {
                    Some(capture) => capture.drain_after_exit().await.map(|(stdout, stderr)| {
                        std::process::Output {
                            status,
                            stdout,
                            stderr,
                        }
                    }),
                    None => Err(io::Error::other("owned output capture was lost")),
                },
                Err(error) => Err(error),
            }
        };
        if outcome.is_err() {
            loop {
                if process.terminate(shutdown.clone()).await.is_ok()
                    || matches!(
                        process.wait_for_completion(Duration::from_secs(5)).await,
                        Ok(WaitForCompletionResult::Completed(_))
                    )
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
        if let Some(capture) = capture {
            capture.abort_after_exit().await;
        }
        drop(process);
        let _ = completion.send(outcome);
        if *cancelled.borrow_and_update() == 0 {
            let _ = cancelled.changed().await;
        }
        if *cancelled.borrow() == 1 {
            match tokio::fs::remove_file(owned_output).await {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => tracing::warn!(%error, "abandoned storyboard output cleanup failed"),
            }
        }
    });
    let outcome = result
        .await
        .map_err(|_| io::Error::other("storyboard FFmpeg completion was lost"))?;
    observer.acknowledge();
    outcome
}

#[cfg(test)]
#[path = "bootstrap_tests.rs"]
mod tests;
