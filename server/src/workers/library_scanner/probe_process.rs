// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::io;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};

pub(super) const MAX_STDOUT: usize = 4 * 1024 * 1024;
pub(super) const MAX_STDERR: usize = 64 * 1024;

#[derive(Clone, Copy)]
pub(super) struct ProbeProfile {
    pub timeout: Duration,
    pub analysis_us: &'static str,
    pub probe_bytes: &'static str,
}

pub(super) const PROFILES: [ProbeProfile; 2] = [
    ProbeProfile {
        timeout: Duration::from_secs(5),
        analysis_us: "5000000",
        probe_bytes: "5M",
    },
    ProbeProfile {
        timeout: Duration::from_secs(12),
        analysis_us: "15000000",
        probe_bytes: "32M",
    },
];

pub(super) struct AttemptOutput {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub(super) struct AttemptError {
    pub message: String,
    pub retryable: bool,
}

pub(super) fn arguments(profile: ProbeProfile) -> Vec<&'static str> {
    vec![
        "-v",
        "warning",
        "-protocol_whitelist",
        "file",
        "-threads",
        "2",
        "-analyzeduration",
        profile.analysis_us,
        "-probesize",
        profile.probe_bytes,
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
        "-show_chapters",
        "-show_error",
    ]
}

pub(super) async fn run(path: &Path, profile: ProbeProfile) -> Result<AttemptOutput, AttemptError> {
    let child = Command::new("ffprobe")
        .args(arguments(profile))
        .arg("-i")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| AttemptError {
            message: format!("ffprobe spawn failed: {error}"),
            retryable: false,
        })?;
    capture(child, profile.timeout).await
}

async fn capture(mut child: Child, timeout: Duration) -> Result<AttemptOutput, AttemptError> {
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let result = tokio::time::timeout(timeout, async {
        tokio::try_join!(
            bounded_read(stdout, MAX_STDOUT),
            bounded_read(stderr, MAX_STDERR),
            child.wait()
        )
    })
    .await;
    match result {
        Ok(Ok((stdout, stderr, status))) => Ok(AttemptOutput {
            success: status.success(),
            exit_code: status.code(),
            stdout,
            stderr,
        }),
        result => {
            let _ = child.start_kill();
            let reaped = child.wait().await;
            let (message, retryable) = match result {
                Err(_) => ("ffprobe deadline exceeded".into(), true),
                Ok(Err(error)) => (format!("ffprobe output failed: {error}"), false),
                Ok(Ok(_)) => unreachable!(),
            };
            Err(AttemptError {
                message: if reaped.is_err() {
                    format!("{message}; child reap failed")
                } else {
                    message
                },
                retryable: retryable && reaped.is_ok(),
            })
        }
    }
}

pub(super) async fn bounded_read(
    reader: impl AsyncRead + Unpin,
    limit: usize,
) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > limit {
        return Err(io::Error::other("ffprobe output byte limit exceeded"));
    }
    Ok(bytes)
}

pub(super) fn diagnostic(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(2048)])
        .chars()
        .filter(|character| !character.is_control() || *character == '\n')
        .collect()
}

#[cfg(test)]
#[path = "probe_process_tests.rs"]
mod tests;
