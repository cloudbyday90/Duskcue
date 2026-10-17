// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use serde_json::json;

use super::{ProbeResult, ScannerError};

#[path = "probe_metadata.rs"]
mod metadata;
#[path = "probe_model.rs"]
mod model;
#[path = "probe_process.rs"]
mod process;
#[path = "probe_tracks.rs"]
mod tracks;

pub(super) async fn probe_file(path: &Path) -> Result<ProbeResult, ScannerError> {
    let mut failures = Vec::new();
    for (index, profile) in process::PROFILES.into_iter().enumerate() {
        let output = match process::run(path, profile).await {
            Ok(output) => output,
            Err(error) => {
                failures.push(error.message);
                if error.retryable {
                    continue;
                }
                break;
            }
        };
        let parsed = parse_probe_output_at(&output.stdout, path);
        if output.success {
            match parsed {
                Ok(mut result) if usable_video(&result) => {
                    result.additional_streams["probe"] = json!({
                        "status": "probed", "attempts": index + 1, "decode_verified": false,
                        "has_warnings": !output.stderr.is_empty(),
                    });
                    if !output.stderr.is_empty() {
                        tracing::debug!(attempt = index + 1, diagnostic = %process::diagnostic(&output.stderr), "ffprobe completed with warnings");
                    }
                    return Ok(result);
                }
                Ok(_) => {
                    failures.push("ffprobe returned no complete moving-video inventory".into())
                }
                Err(error) => failures.push(format!("invalid ffprobe JSON: {error}")),
            }
        } else {
            tracing::warn!(attempt = index + 1, exit_code = ?output.exit_code, partial_video = parsed.as_ref().is_ok_and(usable_video), diagnostic = %process::diagnostic(&output.stderr), "ffprobe rejected source; partial metadata was not ingested");
            failures.push(format!(
                "ffprobe unsuccessful exit {:?}; source metadata not admitted",
                output.exit_code
            ));
        }
    }
    Err(ScannerError::ProbeFailed {
        path: path.to_string_lossy().into_owned(),
        error: failures.join("; "),
    })
}

fn usable_video(result: &ProbeResult) -> bool {
    let video = &result.additional_streams["video"];
    video["index"].as_u64().is_some()
        && metadata::known(result.video_codec.as_deref()).is_some()
        && video["width"].as_u64().is_some_and(|width| width > 0)
        && video["height"].as_u64().is_some_and(|height| height > 0)
        && result.container_format != "unknown"
        && result.additional_streams["probe_error"].is_null()
}

fn parse_probe_output_at(bytes: &[u8], path: &Path) -> Result<ProbeResult, serde_json::Error> {
    let probe: model::FfprobeOutput = serde_json::from_slice(bytes)?;
    let streams = probe.streams.unwrap_or_default();
    let video = streams.iter().find(|stream| stream.moving_video());
    let audio: Vec<_> = streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .filter_map(tracks::audio)
        .collect();
    let subtitles: Vec<_> = streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("subtitle"))
        .filter_map(tracks::subtitle)
        .collect();
    let mut additional_streams = json!({});
    let dolby = video.and_then(metadata::dolby_vision);
    if let Some(video) = video {
        additional_streams["video"] = metadata::video_metadata(video);
    }
    if let Some(dolby) = &dolby {
        additional_streams["dolby_vision"] = dolby.clone();
    }
    if let Some(chapters) = probe.chapters.filter(|chapters| !chapters.is_empty()) {
        additional_streams["chapters"] = serde_json::to_value(chapters)?;
    }
    if !audio.is_empty() {
        additional_streams["audio"] = json!(audio);
    }
    if !subtitles.is_empty() {
        additional_streams["subtitles"] = json!(subtitles);
    }
    if let Some(error) = probe.error {
        additional_streams["probe_error"] = error;
    }
    let first_audio = audio.first();
    let runtime_seconds = probe
        .format
        .as_ref()
        .and_then(|format| format.duration.as_deref())
        .and_then(|duration| duration.parse::<f64>().ok())
        .filter(|duration| duration.is_finite() && *duration > 0.0)
        .map(|duration| duration.round().min(i32::MAX as f64) as i32)
        .unwrap_or(0);
    Ok(ProbeResult {
        container_format: metadata::canonical_container(
            probe
                .format
                .as_ref()
                .and_then(|format| format.format_name.as_deref()),
            path,
        ),
        video_codec: video
            .and_then(|video| metadata::known(video.codec_name.as_deref()))
            .map(str::to_owned),
        video_resolution: video
            .and_then(|video| video.width.zip(video.height))
            .filter(|(width, height)| *width > 0 && *height > 0)
            .map(|(width, height)| format!("{width}x{height}")),
        video_bitrate: video
            .and_then(|video| metadata::positive_integer(video.bit_rate.as_deref())),
        video_dynamic_range: video.and_then(|video| metadata::dynamic_range(video, dolby.as_ref())),
        video_frame_rate: additional_streams["video"]["frame_rate"].as_f64(),
        audio_codec: first_audio
            .and_then(|audio| audio["codec"].as_str())
            .map(str::to_owned),
        audio_channels: first_audio
            .and_then(|audio| audio["channels"].as_i64())
            .and_then(|channels| i32::try_from(channels).ok()),
        audio_language: first_audio
            .and_then(|audio| audio["language"].as_str())
            .map(str::to_owned),
        audio_bitrate: first_audio
            .and_then(|audio| audio["bitrate"].as_i64())
            .and_then(|bitrate| i32::try_from(bitrate).ok()),
        runtime_seconds,
        additional_streams,
    })
}

#[cfg(test)]
#[path = "probe_normalization_tests.rs"]
mod normalization_tests;
#[cfg(test)]
#[path = "probe_tests.rs"]
mod tests;
