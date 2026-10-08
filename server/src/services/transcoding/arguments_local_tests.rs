// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;
use std::time::Duration;

use tokio::process::Command;
use tokio_process_tools::{
    CollectionOverflowBehavior, DEFAULT_OUTPUT_EOF_TIMEOUT, GracefulShutdown, NumBytesExt,
    RawCollectionOptions, RawOutputOptions, WaitForCompletionOrTerminateResult,
    WaitForCompletionResult,
};
use uuid::Uuid;

pub(super) fn enabled() -> bool {
    std::env::var("DUSKCUE_TEST_FFMPEG_MODE").as_deref() == Ok("linux-local-managed")
}

pub(super) async fn media(program: &str, args: Vec<String>) -> anyhow::Result<Vec<u8>> {
    Uuid::parse_str(&std::env::var("DUSKCUE_TEST_RESOURCE_ID")?)?;
    let executable = match program {
        "ffmpeg" => "/usr/bin/ffmpeg",
        "ffprobe" => "/usr/bin/ffprobe",
        _ => anyhow::bail!("unsupported local fixture executable"),
    };
    let output = tokio::time::timeout(
        Duration::from_secs(60),
        Command::new(executable)
            .args(args)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await??;
    anyhow::ensure!(
        output.stdout.len() + output.stderr.len() <= 2_000_000,
        "local fixture output exceeded its bound"
    );
    anyhow::ensure!(
        output.status.success(),
        "local fixture source/decode command failed"
    );
    Ok(output.stdout)
}

pub(super) async fn encode(args: &[String], source: &Path, directory: &Path) -> anyhow::Result<()> {
    Uuid::parse_str(&std::env::var("DUSKCUE_TEST_RESOURCE_ID")?)?;
    let spawned =
        super::super::super::launcher::spawn_ffmpeg(args, Uuid::now_v7(), source, directory)?;
    let grace = Duration::from_millis(100);
    let shutdown = GracefulShutdown::builder()
        .unix_sigterm(grace)
        .windows_ctrl_break(grace)
        .build();
    let mut process = spawned.handle.terminate_on_drop(shutdown.clone());
    if let Err(error) = spawned.readiness.wait().await {
        while process.terminate(shutdown.clone()).await.is_err() {
            if matches!(
                process.wait_for_completion(Duration::from_secs(1)).await,
                Ok(WaitForCompletionResult::Completed(_))
            ) {
                break;
            }
        }
        return Err(error.into());
    }
    let outcome = process
        .wait_for_completion(Duration::from_secs(60))
        .or_terminate(shutdown.clone())
        .with_raw_output(
            DEFAULT_OUTPUT_EOF_TIMEOUT,
            RawOutputOptions::symmetric(RawCollectionOptions::Bounded {
                max_bytes: 64.kilobytes(),
                overflow_behavior: CollectionOverflowBehavior::DropAdditionalData,
            }),
        )
        .await;
    match outcome {
        Ok(WaitForCompletionOrTerminateResult::Completed(output)) => {
            anyhow::ensure!(
                output.status.success(),
                "production managed encode failed: {}",
                String::from_utf8_lossy(&output.stderr.bytes)
            );
            Ok(())
        }
        Ok(WaitForCompletionOrTerminateResult::TerminatedAfterTimeout { .. }) => {
            anyhow::bail!("production managed encode exceeded its deadline")
        }
        Err(error) => {
            while process.terminate(shutdown.clone()).await.is_err() {
                if matches!(
                    process.wait_for_completion(Duration::from_secs(1)).await,
                    Ok(WaitForCompletionResult::Completed(_))
                ) {
                    break;
                }
            }
            Err(error.into())
        }
    }
}
