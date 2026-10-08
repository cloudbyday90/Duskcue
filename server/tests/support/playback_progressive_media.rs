// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::{Path, PathBuf};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

use super::process;

pub(super) const SOURCE_SECONDS: i32 = 600;
pub(super) const CACHE_LIMIT: u64 = 128 * 1024 * 1024;

pub(super) struct Source {
    pub(super) path: PathBuf,
    pub(super) bytes: u64,
    pub(super) runtime_seconds: i32,
    pub(super) probed_seconds: f64,
    pub(super) audio: Value,
}

pub(super) async fn source(
    owned: &process::FixtureProcesses,
    original: &Path,
    directory: &Path,
) -> anyhow::Result<Source> {
    tokio::fs::create_dir(directory).await?;
    let path = directory.join("source.mkv");
    process::original(
        owned,
        "ffmpeg",
        &[
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-nostdin".into(),
            "-stream_loop".into(),
            "-1".into(),
            "-i".into(),
            original.to_string_lossy().into_owned(),
            "-map".into(),
            "0".into(),
            "-c".into(),
            "copy".into(),
            "-threads".into(),
            "1".into(),
            "-t".into(),
            SOURCE_SECONDS.to_string(),
            "-fs".into(),
            (32 * 1024 * 1024).to_string(),
            "-n".into(),
            path.to_string_lossy().into_owned(),
        ],
    )
    .await?;
    let metadata = tokio::fs::symlink_metadata(&path).await?;
    anyhow::ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() > 0
            && metadata.len() <= 32 * 1024 * 1024,
        "generated progressive source is not a bounded owned regular file"
    );
    let probe: Value = serde_json::from_slice(
        &process::original(
            owned,
            "ffprobe",
            &[
                "-v".into(),
                "error".into(),
                "-show_streams".into(),
                "-show_format".into(),
                "-of".into(),
                "json".into(),
                path.to_string_lossy().into_owned(),
            ],
        )
        .await?,
    )?;
    let duration = probe["format"]["duration"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("source runtime is unavailable"))?
        .parse::<f64>()?;
    anyhow::ensure!(
        duration.is_finite() && (duration - f64::from(SOURCE_SECONDS)).abs() <= 1.0,
        "actual source runtime differs from the bounded fixture duration"
    );
    let streams = probe["streams"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("source streams unavailable"))?;
    anyhow::ensure!(
        streams.len() == 5
            && streams[0]["codec_type"] == "audio"
            && streams[1]["codec_type"] == "video"
            && streams[2]["codec_type"] == "audio",
        "actual fixture is no longer audio-first with selected audio index two"
    );
    anyhow::ensure!(
        streams[1]["codec_name"] == "h264"
            && streams[1]["width"] == 320
            && streams[1]["height"] == 180
            && streams[2]["codec_name"] == "aac",
        "actual fixture codecs/resolution changed"
    );
    let audio = streams
        .iter()
        .filter(|stream| stream["codec_type"] == "audio")
        .cloned()
        .collect::<Vec<_>>();
    Ok(Source {
        path,
        bytes: metadata.len(),
        runtime_seconds: SOURCE_SECONDS,
        probed_seconds: duration,
        audio: serde_json::json!({ "audio": audio }),
    })
}

pub(super) fn complete_mp4(bytes: &[u8], initialization: bool) -> anyhow::Result<()> {
    anyhow::ensure!(
        !bytes.is_empty() && bytes.len() <= 16 * 1024 * 1024,
        "HTTP MP4 body is empty or oversized"
    );
    let mut offset = 0usize;
    let mut kinds = Vec::new();
    while offset < bytes.len() {
        anyhow::ensure!(
            bytes.len() - offset >= 8 && kinds.len() < 256,
            "incomplete or excessive MP4 boxes"
        );
        let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
        anyhow::ensure!(
            size >= 8 && size <= bytes.len() - offset,
            "HTTP MP4 box is not physically complete"
        );
        kinds.push(&bytes[offset + 4..offset + 8]);
        offset += size;
    }
    let first = if initialization { b"ftyp" } else { b"moof" };
    let second = if initialization { b"moov" } else { b"mdat" };
    anyhow::ensure!(
        kinds.iter().any(|kind| *kind == first) && kinds.iter().any(|kind| *kind == second),
        "HTTP MP4 is missing required initialization/fragment boxes"
    );
    Ok(())
}

pub(super) async fn decode_start(
    owned: &process::FixtureProcesses,
    directory: &Path,
    name: &str,
    initialization: &[u8],
    segment: &[u8],
) -> anyhow::Result<f64> {
    complete_mp4(initialization, true)?;
    complete_mp4(segment, false)?;
    let path = directory.join(format!("{name}.mp4"));
    let mut bytes = Vec::with_capacity(initialization.len() + segment.len());
    bytes.extend_from_slice(initialization);
    bytes.extend_from_slice(segment);
    tokio::fs::write(&path, bytes).await?;
    let frames: Value = serde_json::from_slice(
        &process::original(
            owned,
            "ffprobe",
            &[
                "-v".into(),
                "error".into(),
                "-select_streams".into(),
                "v:0".into(),
                "-read_intervals".into(),
                "%+0.25".into(),
                "-show_frames".into(),
                "-show_entries".into(),
                "frame=best_effort_timestamp_time".into(),
                "-of".into(),
                "json".into(),
                path.to_string_lossy().into_owned(),
            ],
        )
        .await?,
    )?;
    let timestamp = frames["frames"]
        .as_array()
        .and_then(|frames| frames.first())
        .and_then(|frame| frame["best_effort_timestamp_time"].as_str())
        .ok_or_else(|| anyhow::anyhow!("actual HTTP fragment did not decode a video frame"))?
        .parse::<f64>()?;
    anyhow::ensure!(
        timestamp.is_finite() && (0.0..=0.25).contains(&timestamp),
        "HTTP start/seek fragment is not stream-relative near zero"
    );
    Ok(timestamp)
}

pub(super) fn cache_bytes(directory: &Path) -> anyhow::Result<u64> {
    let root = directory.canonicalize()?;
    let mut pending = vec![root.clone()];
    let mut files = 0usize;
    let mut bytes = 0u64;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            files += 1;
            anyhow::ensure!(
                files <= 4096,
                "progressive cache inventory exceeded its bound"
            );
            let metadata = match entry.path().symlink_metadata() {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            let path = match entry.path().canonicalize() {
                Ok(path) => path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            anyhow::ensure!(
                !metadata.file_type().is_symlink() && path.starts_with(&root),
                "progressive cache escaped its owned root"
            );
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                bytes += metadata.len();
            }
            anyhow::ensure!(
                files <= 4096 && bytes <= CACHE_LIMIT,
                "progressive cache exceeded its bound"
            );
        }
    }
    Ok(bytes)
}

pub(super) fn segments(playlist: &str) -> anyhow::Result<Vec<String>> {
    anyhow::ensure!(
        playlist.starts_with("#EXTM3U\n") && playlist.ends_with('\n') && playlist.len() <= 65536,
        "HTTP media playlist is incomplete or oversized"
    );
    let segments = playlist
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(String::from)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        !segments.is_empty()
            && segments.len() <= 4096
            && playlist
                .lines()
                .filter(|line| line.starts_with("#EXTINF:"))
                .count()
                == segments.len(),
        "HTTP media playlist has no complete segment references"
    );
    Ok(segments)
}

pub(super) async fn playlist(
    app: &Router,
    token: &str,
    id: Uuid,
) -> anyhow::Result<(StatusCode, String)> {
    let (status, bytes) =
        fetch(app, token, &format!("/api/v1/transcode/{id}/manifest.m3u8")).await?;
    anyhow::ensure!(bytes.len() <= 65536, "HTTP playlist exceeded its bound");
    Ok((status, String::from_utf8(bytes)?))
}

pub(super) async fn fetch(
    app: &Router,
    token: &str,
    path: &str,
) -> anyhow::Result<(StatusCode, Vec<u8>)> {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(path)
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())?,
        )
        .await?;
    let status = response.status();
    Ok((
        status,
        to_bytes(response.into_body(), 16 * 1024 * 1024)
            .await?
            .to_vec(),
    ))
}
