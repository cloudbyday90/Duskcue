use std::io;
use std::path::PathBuf;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::{oneshot, watch};
use tokio_process_tools::{
    CollectionOverflowBehavior, DEFAULT_MAX_BUFFERED_CHUNKS, DEFAULT_OUTPUT_EOF_TIMEOUT,
    DEFAULT_READ_CHUNK_SIZE, GracefulShutdown, NumBytesExt, Process, RawCollectionOptions,
    RawOutputOptions, WaitForCompletionResult,
};

use super::launch::PreparedLaunch;

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
    let handle = Process::new(command)
        .name("storyboard-ffmpeg")
        .stdout_and_stderr(|stream| {
            stream
                .single_subscriber()
                .lossy_without_backpressure()
                .replay_last_bytes(64.kilobytes())
                .read_chunk_size(DEFAULT_READ_CHUNK_SIZE)
                .max_buffered_chunks(DEFAULT_MAX_BUFFERED_CHUNKS)
        })
        .spawn()
        .map_err(|error| io::Error::other(error.to_string()))?;
    let readiness = prepared.readiness();
    let (cancellation, mut cancelled) = watch::channel(0);
    let observer = Observer {
        cancellation,
        armed: true,
    };
    let (completion, result) = oneshot::channel();
    tokio::spawn(async move {
        let shutdown = GracefulShutdown::default();
        let mut process = handle.terminate_on_drop(shutdown.clone());
        let ready = tokio::select! {
            result = readiness.wait() => result.map(|_| ()),
            _ = cancelled.changed() => Err(io::Error::new(io::ErrorKind::Interrupted, "storyboard observer cancelled")),
        };
        let outcome = if let Err(error) = ready {
            Err(error)
        } else {
            let options = RawOutputOptions::symmetric(RawCollectionOptions::Bounded {
                max_bytes: 64.kilobytes(),
                overflow_behavior: CollectionOverflowBehavior::DropAdditionalData,
            });
            tokio::select! {
                result = process.wait_for_completion(Duration::from_secs(3600)).with_raw_output(DEFAULT_OUTPUT_EOF_TIMEOUT, options) => {
                    match result {
                        Ok(WaitForCompletionResult::Completed(output)) => Ok(std::process::Output { status: output.status, stdout: output.stdout.bytes, stderr: output.stderr.bytes }),
                        Ok(WaitForCompletionResult::Timeout { .. }) => Err(io::Error::new(io::ErrorKind::TimedOut, "storyboard FFmpeg timed out")),
                        Err(error) => Err(io::Error::other(error.to_string())),
                    }
                },
                _ = cancelled.changed() => Err(io::Error::new(io::ErrorKind::Interrupted, "storyboard observer cancelled")),
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
