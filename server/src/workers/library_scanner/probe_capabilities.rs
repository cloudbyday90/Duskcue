// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::services::source_capabilities::{
    AudioDescriptor, PlaybackDescriptor, ScanIdentity, ScanSnapshot, VideoDescriptor, configuration,
};

use super::{ProbeResult, metadata, model, process};

pub(super) async fn snapshot(path: &Path) -> Option<ScanSnapshot> {
    tokio::time::timeout(std::time::Duration::from_secs(1), ScanSnapshot::read(path))
        .await
        .ok()?
}

async fn selected(
    path: &Path,
    selector: &'static str,
    deadline: tokio::time::Instant,
) -> Option<Vec<model::FfprobeStream>> {
    let remaining = deadline.checked_duration_since(tokio::time::Instant::now())?;
    if remaining.is_zero() {
        return None;
    }
    let output = process::configuration(path, selector, remaining)
        .await
        .ok()?;
    if !output.success {
        return None;
    }
    let output: model::FfprobeOutput = serde_json::from_slice(&output.stdout).ok()?;
    if output.error.is_some() {
        return None;
    }
    let streams = output.streams?;
    (streams.len() <= 32).then_some(streams)
}

pub(super) async fn enrich(path: &Path, result: &mut ProbeResult, before: Option<ScanSnapshot>) {
    let Some(before) = before else {
        return;
    };
    let deadline = tokio::time::Instant::now() + process::ENRICHMENT_DEADLINE;
    let Some(video) = selected(path, "V", deadline).await else {
        return;
    };
    let audio = selected(path, "a", deadline).await.unwrap_or_default();
    let Some(remaining) = deadline.checked_duration_since(tokio::time::Instant::now()) else {
        return;
    };
    let Some(after) = tokio::time::timeout(
        remaining.min(std::time::Duration::from_secs(1)),
        ScanSnapshot::read(path),
    )
    .await
    .ok()
    .flatten() else {
        return;
    };
    if before != after {
        return;
    }
    if let Some(descriptor) = describe(result, before.identity, &video, &audio)
        && let Ok(value) = serde_json::to_value(descriptor)
    {
        result.additional_streams["playback_descriptor"] = value;
    }
}

fn extradata(stream: &model::FfprobeStream) -> Option<Vec<u8>> {
    let length = stream
        .extradata_size
        .filter(|length| (1..=65536).contains(length))?;
    let text = stream.extradata.as_deref()?;
    if text.len() > 4 * 65536 / 3 + 2048 {
        return None;
    }
    let encoded: Vec<_> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if encoded.len() > 4 * 65536 / 3 + 4 {
        return None;
    }
    let bytes = STANDARD.decode(encoded).ok()?;
    (bytes.len() == length).then_some(bytes)
}

fn positive(value: Option<&str>) -> Option<u64> {
    value?.parse().ok().filter(|value| *value > 0)
}

fn colors(stream: &model::FfprobeStream) -> Option<[u8; 4]> {
    let primaries = match stream.color_primaries.as_deref()? {
        "bt709" => 1,
        "bt470m" => 4,
        "bt470bg" => 5,
        "smpte170m" => 6,
        "smpte240m" => 7,
        "film" => 8,
        "bt2020" => 9,
        "smpte428" => 10,
        "smpte431" => 11,
        "smpte432" => 12,
        _ => return None,
    };
    let transfer = match stream.color_transfer.as_deref()? {
        "bt709" => 1,
        "gamma22" => 4,
        "gamma28" => 5,
        "smpte170m" => 6,
        "smpte240m" => 7,
        "linear" => 8,
        "log100" => 9,
        "log316" => 10,
        "iec61966-2-4" => 11,
        "bt1361e" => 12,
        "iec61966-2-1" => 13,
        "bt2020-10" => 14,
        "bt2020-12" => 15,
        "smpte2084" => 16,
        "smpte428" => 17,
        "arib-std-b67" => 18,
        _ => return None,
    };
    let matrix = match stream.color_space.as_deref()? {
        "gbr" => 0,
        "bt709" => 1,
        "fcc" => 4,
        "bt470bg" => 5,
        "smpte170m" => 6,
        "smpte240m" => 7,
        "ycgco" => 8,
        "bt2020nc" => 9,
        "bt2020c" => 10,
        "smpte2085" => 11,
        "chroma-derived-nc" => 12,
        "chroma-derived-c" => 13,
        "ictcp" => 14,
        _ => return None,
    };
    let range = match stream.color_range.as_deref()? {
        "tv" => 0,
        "pc" => 1,
        _ => return None,
    };
    Some([primaries, transfer, matrix, range])
}

fn file_codec(codec: &str, copy_codec: &str, tag: Option<&str>, container: &str) -> Option<String> {
    match (codec, container) {
        ("h264", "mp4" | "mov") => {
            let tag = tag.filter(|tag| ["avc1", "avc3"].contains(tag))?;
            Some(copy_codec.replacen("avc1", tag, 1))
        }
        ("hevc", "mp4" | "mov") => {
            let tag = tag.filter(|tag| ["hvc1", "hev1"].contains(tag))?;
            Some(copy_codec.replacen("hev1", tag, 1))
        }
        ("av1", "mp4" | "mov") if tag == Some("av01") => Some(copy_codec.to_owned()),
        ("h264" | "hevc" | "av1", "mkv") | ("av1", "webm") => Some(copy_codec.to_owned()),
        _ => None,
    }
}

fn describe(
    result: &ProbeResult,
    scan: ScanIdentity,
    videos: &[model::FfprobeStream],
    audio: &[model::FfprobeStream],
) -> Option<PlaybackDescriptor> {
    let moving: Vec<_> = videos
        .iter()
        .filter(|stream| stream.moving_video())
        .collect();
    let stream = moving.iter().find(|stream| {
        stream.stream_index().map(i64::from) == result.additional_streams["video"]["index"].as_i64()
    })?;
    let codec = stream.codec_name.as_deref()?;
    let bytes = extradata(stream)?;
    let normalized = metadata::video_metadata(stream);
    let bit_depth = u8::try_from(normalized["bit_depth"].as_u64()?).ok()?;
    if !["yuv420p", "yuvj420p", "yuv420p10le", "yuv420p10be"].contains(&stream.pix_fmt.as_deref()?)
    {
        return None;
    }
    let color = if codec == "av1" {
        colors(stream)?
    } else {
        [1, 1, 1, 0]
    };
    let config = configuration::video(codec, &bytes, bit_depth, color)?;
    if stream.level != Some(config.level) {
        return None;
    }
    let frame_rate = metadata::frame_rate(stream.avg_frame_rate.as_deref())
        .or_else(|| metadata::frame_rate(stream.r_frame_rate.as_deref()))?;
    let stream_bitrate = positive(stream.bit_rate.as_deref());
    let average_bitrate = result.additional_streams["format"]["duration_seconds"]
        .as_f64()
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|duration| ((scan.size as f64 * 8.0 / duration).ceil()) as u64)
        .filter(|value| *value > 0);
    let bitrate = stream_bitrate.or(average_bitrate);
    let bitrate_basis = stream_bitrate
        .map(|_| "stream")
        .or(average_bitrate.map(|_| "container_average"))
        .map(str::to_owned);
    let audio: Vec<_> = audio
        .iter()
        .filter_map(|stream| {
            let codec = stream.codec_name.as_deref()?;
            let bytes = extradata(stream)?;
            let sample_rate = u32::try_from(positive(stream.sample_rate.as_deref())?).ok()?;
            let channels = u16::try_from(stream.channels?).ok()?;
            let file_codec = configuration::audio(
                codec,
                &bytes,
                stream.profile.as_deref(),
                sample_rate,
                channels,
            )?;
            Some(AudioDescriptor {
                index: stream.stream_index()?,
                codec: codec.to_owned(),
                file_codec: file_codec.to_owned(),
                copy_codec: (codec == "aac").then(|| file_codec.to_owned()),
                bitrate: positive(stream.bit_rate.as_deref()),
                sample_rate,
                channels,
                config_sha256: configuration::sha256(&bytes),
            })
        })
        .collect();
    let descriptor = PlaybackDescriptor {
        version: 1,
        scan,
        moving_video_count: u32::try_from(moving.len()).ok()?,
        video: VideoDescriptor {
            index: stream.stream_index()?,
            codec: codec.to_owned(),
            file_codec: file_codec(
                codec,
                &config.copy_codec,
                stream.codec_tag_string.as_deref(),
                &result.container_format,
            ),
            copy_codec: config.copy_codec,
            width: u32::try_from(stream.width?).ok()?,
            height: u32::try_from(stream.height?).ok()?,
            bit_depth: config.bit_depth,
            frame_rate,
            bitrate,
            bitrate_basis,
            config_sha256: configuration::sha256(&bytes),
            dynamic_range: result
                .video_dynamic_range
                .as_deref()
                .filter(|value| ["sdr", "hdr10", "hlg"].contains(value))
                .unwrap_or("unknown")
                .to_owned(),
        },
        audio,
    };
    PlaybackDescriptor::from_metadata(&serde_json::json!({ "playback_descriptor": descriptor }))
}

#[cfg(test)]
#[path = "probe_capabilities_tests.rs"]
mod tests;
