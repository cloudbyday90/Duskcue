// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{Output, Source, TransportError, arguments, owner::Status, pump};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::Instant;
use tokio_process_tools::{GracefulShutdown, Process, Stdin};
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    source: Source,
    output: &Output,
    status: &watch::Sender<Status>,
    cancel: CancellationToken,
) -> Result<(), TransportError> {
    let startup = Instant::now() + Duration::from_secs(15);
    let response = source.open(&cancel).await?;
    #[cfg(target_os = "linux")]
    let mut prepared = crate::services::sandbox::launch::PreparedLaunch::pipe(&output.directory)
        .map_err(|_| TransportError::Worker)?;
    #[cfg(target_os = "linux")]
    let mut command = prepared.command();
    #[cfg(not(target_os = "linux"))]
    let mut command = tokio::process::Command::new("ffmpeg");
    command.args(arguments::build(&output.directory));
    let handle = Process::new(command)
        .name("live-transport-ffmpeg")
        .stdout_and_stderr(|s| s.discard())
        .spawn()
        .map_err(|_| TransportError::Worker)?;
    let shutdown = GracefulShutdown::builder()
        .unix_sigterm(Duration::from_secs(2))
        .windows_ctrl_break(Duration::from_secs(2))
        .build();
    let mut process = handle.terminate_on_drop(shutdown.clone());
    let pid = process.id().ok_or(TransportError::Worker)?;
    let stdin = std::mem::replace(process.stdin(), Stdin::Closed);
    let mut input = match stdin {
        Stdin::Open(stdin) => Some(tokio::spawn(pump::run(
            response,
            stdin,
            cancel.child_token(),
            output.writing.clone(),
        ))),
        Stdin::Closed => None,
    };
    let result = async {
        if input.is_none() { return Err(TransportError::Worker); }
        #[cfg(target_os = "linux")]
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(TransportError::Cancelled),
            result = tokio::time::timeout_at(startup, prepared.readiness().wait()) => result.map_err(|_| TransportError::Deadline)?.map_err(|_| TransportError::Worker)?,
        };
        let mut ready = false;
        let mut progress = Instant::now();
        let mut last_segment = None;
        loop {
            let task = input.as_mut().ok_or(TransportError::Worker)?;
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(TransportError::Cancelled),
                result = task => { input = None; return result.map_err(|_| TransportError::Worker)?; },
                result = process.wait_for_completion(Duration::from_secs(60)) => { let _ = result; return Err(TransportError::Worker); },
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if let Some(snapshot) = output.inspect().await? {
                        if last_segment.as_ref() != snapshot.segments.last() { last_segment = snapshot.segments.last().cloned(); progress = Instant::now(); }
                        if !ready {
                            ready = true; output.active.store(true, Ordering::Release); status.send_replace(Status::Ready(pid));
                        }
                    }
                    if (!ready && Instant::now() >= startup) || (ready && progress.elapsed() > Duration::from_secs(5)) { return Err(TransportError::Deadline); }
                }
            }
        }
    }.await;
    output.active.store(false, Ordering::Release);
    cancel.cancel();
    while process.terminate(shutdown.clone()).await.is_err() {
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    if let Some(input) = input {
        let _ = input.await.map_err(|_| TransportError::Worker)?;
    }
    match result {
        Err(TransportError::Cancelled) => Ok(()),
        other => other,
    }
}
