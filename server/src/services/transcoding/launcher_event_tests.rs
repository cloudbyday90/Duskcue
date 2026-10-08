// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use anyhow::Context;
use tokio::io::AsyncReadExt;

use super::*;
use crate::services::transcoding::lifecycle::ExecutionState;

struct Snapshot {
    segments: Vec<String>,
    ended: bool,
}

async fn snapshot(directory: &std::path::Path) -> anyhow::Result<Snapshot> {
    let mut options = tokio::fs::OpenOptions::new();
    options.read(true).custom_flags(libc::O_NOFOLLOW);
    let mut file = options.open(directory.join("manifest.m3u8")).await?;
    let mut bytes = Vec::new();
    (&mut file).take(65537).read_to_end(&mut bytes).await?;
    anyhow::ensure!(
        bytes.len() <= 65536,
        "candidate playlist exceeded its read bound"
    );
    let text = std::str::from_utf8(&bytes)?;
    anyhow::ensure!(
        text.starts_with("#EXTM3U\n") && text.ends_with('\n'),
        "partially published candidate playlist"
    );
    anyhow::ensure!(
        text.contains("#EXT-X-PLAYLIST-TYPE:EVENT\n")
            && text.contains("#EXT-X-MAP:URI=\"init.mp4\""),
        "candidate publication changed expected layout"
    );
    let lines: Vec<_> = text.lines().collect();
    let mut segments = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if let Some(duration) = line.strip_prefix("#EXTINF:") {
            let duration: f64 = duration.split(',').next().unwrap_or_default().parse()?;
            anyhow::ensure!(
                duration.is_finite() && duration > 0.0,
                "invalid candidate segment duration"
            );
            let name = *lines
                .get(index + 1)
                .ok_or_else(|| anyhow::anyhow!("partial candidate segment reference"))?;
            anyhow::ensure!(
                name == format!("seg_{:04}.m4s", segments.len()),
                "candidate advertised an unowned or reordered segment"
            );
            let metadata = tokio::fs::symlink_metadata(directory.join(name)).await?;
            anyhow::ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() > 0,
                "candidate advertised a missing/unowned segment"
            );
            segments.push(name.to_owned());
        }
    }
    anyhow::ensure!(
        !segments.is_empty() && segments.len() <= 8,
        "candidate playlist has an unexpected segment count"
    );
    Ok(Snapshot {
        segments,
        ended: text.contains("#EXT-X-ENDLIST\n"),
    })
}

async fn decode_first_frame(manifest: &std::path::Path) -> anyhow::Result<()> {
    let mut child = tokio::process::Command::new("/usr/bin/ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-analyzeduration",
            "100000",
            "-probesize",
            "32768",
            "-i",
        ])
        .arg(manifest)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-an",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "-",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut output = match child.stdout.take() {
        Some(output) => output,
        None => {
            terminate_decoder(&mut child).await;
            anyhow::bail!("first-frame decoder pipe missing");
        }
    };
    let outcome = tokio::time::timeout(Duration::from_secs(3), async {
        let read = async {
            let mut bytes = Vec::new();
            (&mut output).take(172801).read_to_end(&mut bytes).await?;
            Ok::<_, std::io::Error>(bytes)
        };
        tokio::try_join!(read, child.wait())
    })
    .await;
    match outcome {
        Ok(Ok((bytes, status))) => {
            anyhow::ensure!(
                status.success() && bytes.len() == 320 * 180 * 3,
                "first published fragment did not decode one complete frame"
            )
        }
        failure => {
            terminate_decoder(&mut child).await;
            anyhow::bail!("first fragment decoder failed or exceeded its deadline: {failure:?}");
        }
    }
    Ok(())
}

async fn terminate_decoder(child: &mut tokio::process::Child) {
    loop {
        if child.try_wait().is_ok_and(|status| status.is_some()) {
            return;
        }
        let _ = child.start_kill();
        if matches!(
            tokio::time::timeout(Duration::from_secs(1), child.wait()).await,
            Ok(Ok(_))
        ) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires fresh guarded managed Linux artifact and an owned eight-second source"]
async fn actual_managed_event_publishes_playable_assets_before_encoder_exit() -> anyhow::Result<()>
{
    let running = Running::launch_event_candidate().await?;
    let proof = async {
        let started = running.startup_deadline - crate::services::transcoding::readiness::STARTUP_TIMEOUT;
        let manifest = running.directory.join("manifest.m3u8");
        let mut execution = running.workers.execution(running.id)
            .ok_or_else(|| anyhow::anyhow!("candidate worker ownership was lost"))?;
        crate::services::transcoding::readiness::wait(&running.directory, &manifest, &mut execution, running.startup_deadline).await?;
        let first = snapshot(&running.directory).await?;
        let pid = running.pid.ok_or_else(|| anyhow::anyhow!("candidate PID unavailable"))?;
        anyhow::ensure!(execution.state() == ExecutionState::Running && std::path::Path::new(&format!("/proc/{pid}")).exists(), "candidate was not published before physical exit");
        anyhow::ensure!(running.semaphore.available_permits() == 0 && !first.ended, "candidate first publication released ownership or declared completion");
        decode_first_frame(&manifest).await.context("candidate first playable frame")?;
        anyhow::ensure!(execution.state() == ExecutionState::Running && std::path::Path::new(&format!("/proc/{pid}")).exists(), "candidate decoder only succeeded after encoder exit");
        let first_count = first.segments.len();
        let first_seconds = started.elapsed().as_secs_f64();
        let mut previous = first.segments;
        let mut observations = 1;
        loop {
            anyhow::ensure!(tokio::time::Instant::now() < running.startup_deadline, "candidate natural completion exceeded shared deadline");
            anyhow::ensure!(!matches!(execution.state(), ExecutionState::Failed | ExecutionState::Cancelled), "candidate publisher failed or was cancelled");
            let current = snapshot(&running.directory).await?;
            anyhow::ensure!(current.segments.starts_with(&previous), "candidate publication was not append-only");
            previous = current.segments;
            observations += 1;
            if current.ended && execution.state() == ExecutionState::Succeeded
                && running.semaphore.available_permits() == 1
                && !std::path::Path::new(&format!("/proc/{pid}")).exists() {
                anyhow::ensure!(previous.len() > first_count && observations > 1, "candidate did not grow after startup");
                anyhow::ensure!(running.semaphore.available_permits() == 1 && !std::path::Path::new(&format!("/proc/{pid}")).exists(), "candidate completion released before physical exit");
                return Ok::<_, anyhow::Error>(serde_json::json!({"first_seconds":first_seconds,"first_segments":first_count,"final_segments":previous.len(),"observations":observations,"first_frame_decoded_while_running":true,"ended":true}));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }.await;
    let cleanup = running.stop().await;
    let evidence = proof?;
    cleanup?;
    println!("\nDUSKCUE_EVENT_CANDIDATE={evidence}");
    Ok(())
}
