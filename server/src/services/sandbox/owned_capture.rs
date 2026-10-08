// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::io;
use std::time::Duration;

use tokio_process_tools::visitors::collect::CollectChunks;
use tokio_process_tools::{
    CollectedBytes, CollectionOverflowBehavior, Consumable, Consumer, DEFAULT_OUTPUT_EOF_TIMEOUT,
    NumBytesExt, OutputStream, RawCollectionOptions,
};

pub(super) struct OwnedCapture {
    stdout: Option<Consumer<CollectedBytes>>,
    stderr: Option<Consumer<CollectedBytes>>,
    startup_error: Option<io::Error>,
}

impl OwnedCapture {
    pub(super) fn attach<Stdout: Consumable + OutputStream, Stderr: Consumable + OutputStream>(
        stdout: &Stdout,
        stderr: &Stderr,
    ) -> Self {
        let (stdout, stdout_error) = match collect(stdout) {
            Ok(consumer) => (Some(consumer), None),
            Err(error) => (None, Some(error)),
        };
        let (stderr, stderr_error) = match collect(stderr) {
            Ok(consumer) => (Some(consumer), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            stdout,
            stderr,
            startup_error: stdout_error.or(stderr_error),
        }
    }

    pub(super) fn take_startup_error(&mut self) -> Option<io::Error> {
        self.startup_error.take()
    }

    pub(super) async fn drain_after_exit(mut self) -> io::Result<(Vec<u8>, Vec<u8>)> {
        let finished = tokio::time::timeout(DEFAULT_OUTPUT_EOF_TIMEOUT, async {
            loop {
                match (&self.stdout, &self.stderr) {
                    (Some(stdout), Some(stderr))
                        if stdout.is_finished() && stderr.is_finished() =>
                    {
                        return Ok(());
                    }
                    (None, _) | (_, None) => {
                        return Err(io::Error::other("owned output collector was unavailable"));
                    }
                    _ => tokio::time::sleep(Duration::from_millis(5)).await,
                }
            }
        })
        .await
        .unwrap_or_else(|_| {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "owned output EOF deadline exceeded",
            ))
        });
        if let Err(error) = finished {
            self.abort_after_exit().await;
            return Err(error);
        }
        let stdout = self
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("owned stdout collector was lost"))?;
        let stderr = self
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("owned stderr collector was lost"))?;
        let (stdout, stderr) = tokio::join!(stdout.wait(), stderr.wait());
        Ok((
            stdout
                .map_err(|error| io::Error::other(error.to_string()))?
                .bytes,
            stderr
                .map_err(|error| io::Error::other(error.to_string()))?
                .bytes,
        ))
    }

    pub(super) async fn abort_after_exit(self) {
        tokio::join!(abort(self.stdout), abort(self.stderr));
    }
}

fn collect<Stream: Consumable + OutputStream>(
    stream: &Stream,
) -> io::Result<Consumer<CollectedBytes>> {
    stream
        .consume(CollectChunks::fold(
            CollectedBytes::new(),
            CollectedBytes::collector(RawCollectionOptions::Bounded {
                max_bytes: 64.kilobytes(),
                overflow_behavior: CollectionOverflowBehavior::DropAdditionalData,
            }),
        ))
        .map_err(|error| io::Error::other(error.to_string()))
}

async fn abort(consumer: Option<Consumer<CollectedBytes>>) {
    if let Some(consumer) = consumer {
        consumer.abort().await;
    }
}
