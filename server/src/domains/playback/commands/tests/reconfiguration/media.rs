// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::support::Fixture;
use reqwest::Method;
use std::{path::Path, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt};
use uuid::Uuid;

async fn capture(reader: impl AsyncRead + Unpin) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(1_048_577).read_to_end(&mut bytes).await?;
    anyhow::ensure!(bytes.len() <= 1_048_576);
    Ok(bytes)
}

async fn decode(path: &Path, offset: f64, audio: bool, absolute: bool) -> anyhow::Result<Vec<u8>> {
    let mut cmd = tokio::process::Command::new("ffmpeg");
    cmd.args([
        "-v",
        "error",
        "-nostdin",
        "-threads",
        "1",
        "-filter_threads",
        "1",
    ]);
    if absolute {
        cmd.arg("-copyts");
    } else {
        cmd.args(["-ss", &offset.to_string()]);
    }
    cmd.arg("-i").arg(path);
    if absolute {
        cmd.args(["-ss", &offset.to_string()]);
    }
    if audio {
        cmd.args([
            "-t", "0.75", "-map", "0:a:0", "-vn", "-ac", "1", "-ar", "8000", "-f", "s16le",
            "pipe:1",
        ]);
    } else {
        cmd.args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=320:180,format=yuv420p,extractplanes=y",
            "-pix_fmt",
            "gray",
            "-threads",
            "1",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    }
    let mut child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("fixture stdout unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("fixture stderr unavailable"))?;
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let (out, err, status) = tokio::try_join!(capture(stdout), capture(stderr), async {
            Ok::<_, anyhow::Error>(child.wait().await?)
        })?;
        anyhow::ensure!(
            status.success(),
            "independent fixture decoder failed: {}",
            String::from_utf8_lossy(&err)
        );
        Ok::<_, anyhow::Error>(out)
    })
    .await;
    match result {
        Ok(Ok(bytes)) => Ok(bytes),
        other => {
            if child.try_wait()?.is_none() {
                child.start_kill()?;
                tokio::time::timeout(Duration::from_secs(5), child.wait()).await??;
            }
            match other {
                Ok(Err(e)) => Err(e),
                Err(e) => Err(e.into()),
                _ => unreachable!(),
            }
        }
    }
}

async fn fetch(f: &Fixture, path: &str) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        path.starts_with("/api/v1/playback-commands/") && !path.contains(['?', '#', '\\'])
    );
    let mut response = f.response(Method::GET, path, None, false).await?;
    anyhow::ensure!(
        response.status().is_success(),
        "replacement HTTP delivery refused"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(chunk.len() <= 8 * 1024 * 1024 - bytes.len());
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(super) async fn verify(f: &Fixture, id: Uuid, position: i32) -> anyhow::Result<()> {
    verify_audio(f, id, position, 880.0).await
}
pub(super) async fn verify_audio(
    f: &Fixture,
    id: Uuid,
    position: i32,
    hz_expected: f64,
) -> anyhow::Result<()> {
    let viewer = f
        .base
        .state
        .vod_commands
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("viewer missing"))?;
    let output = viewer.current()?;
    let generation = output
        .generation
        .ok_or_else(|| anyhow::anyhow!("HLS revision required"))?;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if f.base
                .state
                .transcode_manager
                .get_session(&generation)
                .is_some_and(|s| s.is_complete)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await?;
    let session = f
        .base
        .state
        .transcode_manager
        .get_session(&generation)
        .ok_or_else(|| anyhow::anyhow!("complete output missing"))?;
    let ready = f.ready(id).await?;
    let mut playlist = String::from_utf8(fetch(f, ready["stream_url"].as_str().unwrap()).await?)?;
    if playlist.contains("#EXT-X-STREAM-INF") {
        let next = playlist
            .lines()
            .find(|line| line.starts_with('/'))
            .ok_or_else(|| anyhow::anyhow!("rendition missing"))?
            .to_owned();
        playlist = String::from_utf8(fetch(f, &next).await?)?;
    }
    let init = playlist
        .lines()
        .find_map(|line| {
            line.strip_prefix("#EXT-X-MAP:URI=\"")
                .and_then(|v| v.strip_suffix('"'))
        })
        .ok_or_else(|| anyhow::anyhow!("initialization missing"))?;
    let mut observed = fetch(f, init).await?;
    for path in playlist
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .take(8)
    {
        let bytes = fetch(f, path).await?;
        anyhow::ensure!(bytes.len() <= 8 * 1024 * 1024 - observed.len());
        observed.extend(bytes);
    }
    let path = f
        .base
        .state
        .bootstrap
        .data_dir
        .join(format!("observed-{}.mp4", output.id));
    tokio::fs::write(&path, observed).await?;
    let absolute =
        session.output_profile == Some(crate::services::transcoding::OutputProfile::SourceCopySdr);
    let relative = f64::from(if absolute {
        position
    } else {
        position - i32::try_from(session.source_start_ms)?
    }) / 1000.0;
    let pixels = decode(&path, relative, false, absolute).await?;
    anyhow::ensure!(pixels.len() == 320 * 180);
    let mut frame = 0_u32;
    for bit in 0..8 {
        let mut total = 0_u32;
        for y in 164..172 {
            for x in 24 + bit * 32..40 + bit * 32 {
                total += u32::from(pixels[y * 320 + x]);
            }
        }
        if total / (8 * 16) > 128 {
            frame |= 1 << bit;
        }
    }
    anyhow::ensure!(
        frame == u32::try_from(position)? * 24 / 1000,
        "decoded source frame {frame} at {position}ms, output origin {}",
        session.source_start_ms
    );
    let pcm = decode(&path, relative, true, absolute).await?;
    let samples = pcm
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| i16::from_le_bytes([v[0], v[1]]))
        .collect::<Vec<_>>();
    anyhow::ensure!(samples.len() > 4000);
    let crossings = samples.windows(2).filter(|v| v[0] <= 0 && v[1] > 0).count();
    let hz =
        f64::from(u32::try_from(crossings)?) * 8000.0 / f64::from(u32::try_from(samples.len())?);
    anyhow::ensure!(
        (hz - hz_expected).abs() < 20.0,
        "selected audio decoded at {hz}Hz"
    );
    Ok(())
}
