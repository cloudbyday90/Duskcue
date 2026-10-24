// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncReadExt};

pub async fn tool(program: &str, args: &[String]) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        matches!(program, "ffmpeg" | "ffprobe"),
        "unsupported fixture tool"
    );
    let mut command = tokio::process::Command::new(format!("/usr/bin/{program}"));
    if program == "ffmpeg" {
        command.args(["-filter_threads", "1", "-filter_complex_threads", "1"]);
    }
    let mut child = command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("fixture stdout missing"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("fixture stderr missing"))?;
    let result = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let (bytes, _, status) = tokio::try_join!(bounded(stdout), bounded(stderr), child.wait())?;
        anyhow::ensure!(status.success(), "fixture tool failed");
        Ok::<_, anyhow::Error>(bytes)
    })
    .await;
    if child.try_wait()?.is_none() {
        child.start_kill()?;
        tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await??;
    }
    result?
}

async fn bounded(reader: impl AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(std::io::Error::other("fixture output budget exceeded"));
    }
    Ok(bytes)
}

pub fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

pub async fn source() -> anyhow::Result<Vec<u8>> {
    tool(
        "ffmpeg",
        &args(&[
            "-hide_banner",
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=30",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=997:sample_rate=48000",
            "-t",
            "180",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-threads",
            "2",
            "-b:v",
            "200k",
            "-maxrate",
            "250k",
            "-bufsize",
            "500k",
            "-g",
            "60",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            "64k",
            "-f",
            "mpegts",
            "pipe:1",
        ]),
    )
    .await
}

pub async fn decode(path: &Path) -> anyhow::Result<(usize, f64)> {
    let path = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("fixture path invalid"))?;
    let metadata: serde_json::Value = serde_json::from_slice(
        &tool(
            "ffprobe",
            &args(&["-v", "error", "-show_streams", "-of", "json", path]),
        )
        .await?,
    )?;
    let streams = metadata["streams"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("decoded stream inventory missing"))?;
    anyhow::ensure!(
        streams.iter().any(|s| s["codec_name"] == "h264"
            && s["width"] == 640
            && s["height"] == 360
            && s["pix_fmt"] == "yuv420p"),
        "compatible video inventory differs"
    );
    anyhow::ensure!(
        streams
            .iter()
            .any(|s| s["codec_name"] == "aac" && s["channels"] == 2 && s["sample_rate"] == "48000"),
        "compatible audio inventory differs"
    );
    let video = tool(
        "ffmpeg",
        &args(&[
            "-v",
            "error",
            "-nostdin",
            "-i",
            path,
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-threads",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]),
    )
    .await?;
    anyhow::ensure!(
        video.len() == 640 * 360 * 3 && video.iter().min() != video.iter().max(),
        "decoded video missing or uniform"
    );
    let pcm = tool(
        "ffmpeg",
        &args(&[
            "-v", "error", "-nostdin", "-i", path, "-map", "0:a:0", "-t", "1", "-ac", "1", "-ar",
            "8000", "-f", "s16le", "pipe:1",
        ]),
    )
    .await?;
    let samples: Vec<i16> = pcm
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| i16::from_le_bytes([p[0], p[1]]))
        .collect();
    anyhow::ensure!(samples.len() > 4000, "decoded audio missing");
    let crossings = samples.windows(2).filter(|p| p[0] <= 0 && p[1] > 0).count();
    let frequency = crossings as f64 * 8000.0 / samples.len() as f64;
    anyhow::ensure!(
        (frequency - 997.0).abs() < 20.0 && samples.iter().any(|s| s.unsigned_abs() > 100),
        "decoded audio tone differs"
    );
    Ok((video.len(), frequency))
}

pub async fn workers(root: &Path) -> anyhow::Result<Vec<u32>> {
    let mut entries = tokio::fs::read_dir("/proc").await?;
    let mut ids = Vec::new();
    let needle = root.to_string_lossy();
    while let Some(entry) = entries.next_entry().await? {
        if let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>()
            && let Ok(bytes) = tokio::fs::read(entry.path().join("cmdline")).await
        {
            let text = String::from_utf8_lossy(&bytes);
            if text.contains("pipe:0") && text.contains(needle.as_ref()) {
                ids.push(pid);
            }
        }
    }
    Ok(ids)
}

pub fn pause(pid: u32) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        anyhow::ensure!(
            unsafe { libc::kill(i32::try_from(pid)?, libc::SIGSTOP) } == 0,
            "owned worker could not be paused"
        );
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        anyhow::bail!("managed Linux runtime required")
    }
}
