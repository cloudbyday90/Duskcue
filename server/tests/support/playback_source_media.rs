// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use duskcue::services::transcoding::TranscodeSession;
use serde_json::{Value, json};

use super::{process, source_http};

pub(super) fn sha256(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn decode(
    tools: &process::FixtureProcesses,
    path: &Path,
    video: &str,
    audio: &str,
    seek: bool,
    absolute: bool,
    frame_identifier: bool,
) -> anyhow::Result<Value> {
    let mut input = vec!["-v".into(), "error".into(), "-nostdin".into()];
    if seek {
        input.extend(["-ss".into(), "5".into()]);
    }
    if absolute {
        input.push("-copyts".into());
    }
    input.extend(["-i".into(), path.to_string_lossy().into_owned()]);
    if absolute {
        input.extend(["-ss".into(), "5".into()]);
    }
    let mut args = input.clone();
    args.extend([
        "-map".into(),
        video.into(),
        "-frames:v".into(),
        "1".into(),
        "-vf".into(),
        "extractplanes=y".into(),
        "-pix_fmt".into(),
        "gray".into(),
        "-threads".into(),
        "1".into(),
        "-f".into(),
        "rawvideo".into(),
        "pipe:1".into(),
    ]);
    let luma = process::original(tools, "ffmpeg", &args).await?;
    anyhow::ensure!(luma.len() == 320 * 180);
    let frame = if frame_identifier {
        Some(frame_id(&luma)?)
    } else {
        None
    };
    if let Some(frame) = frame {
        anyhow::ensure!(
            frame == 120,
            "actual source seek decoded frame {frame}; expected frame120 at5000ms"
        );
    }
    let mut args = input;
    args.extend([
        "-t".into(),
        "0.75".into(),
        "-map".into(),
        audio.into(),
        "-vn".into(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        "8000".into(),
        "-c:a".into(),
        "pcm_s16le".into(),
        "-threads".into(),
        "1".into(),
        "-f".into(),
        "s16le".into(),
        "pipe:1".into(),
    ]);
    let pcm = process::original(tools, "ffmpeg", &args).await?;
    let samples = pcm
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .skip(500)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        samples.len() >= 4000 && samples.iter().any(|sample| sample.unsigned_abs() > 500)
    );
    let crossings = samples
        .windows(2)
        .filter(|pair| (pair[0] < 0) != (pair[1] < 0))
        .count();
    let hz = crossings as f64 * 8000.0 / (2.0 * samples.len() as f64);
    anyhow::ensure!(
        (hz - 880.0).abs() < 20.0,
        "actual selected audio is{hz}Hz, expected880Hz"
    );
    Ok(json!({"source_frame_id":frame,"selected_audio_hz":hz,"decoded_luma_sha256":sha256(&luma)}))
}

pub(super) async fn native(
    tools: &process::FixtureProcesses,
    path: &Path,
) -> anyhow::Result<Value> {
    let mut result = decode(tools, path, "0:0", "0:2", true, false, true).await?;
    result["source_offset_ms"] = json!(5000);
    result["source_read_from_scanned_file"] = json!(true);
    Ok(result)
}

async fn pixels(
    tools: &process::FixtureProcesses,
    path: &Path,
    pixel_format: &str,
    absolute: bool,
) -> anyhow::Result<Vec<u8>> {
    let mut args = vec!["-v".into(), "error".into(), "-nostdin".into()];
    if absolute {
        args.push("-copyts".into());
    } else {
        args.extend(["-ss".into(), "5".into()]);
    }
    args.extend(["-i".into(), path.to_string_lossy().into_owned()]);
    if absolute {
        args.extend(["-ss".into(), "5".into()]);
    }
    args.extend([
        "-map".into(),
        "0:v:0".into(),
        "-frames:v".into(),
        "1".into(),
        "-pix_fmt".into(),
        pixel_format.into(),
        "-threads".into(),
        "1".into(),
        "-f".into(),
        "rawvideo".into(),
        "pipe:1".into(),
    ]);
    let pixels = process::original(tools, "ffmpeg", &args).await?;
    let expected = 320 * 180 * 3 / 2 * if pixel_format == "yuv420p10le" { 2 } else { 1 };
    anyhow::ensure!(pixels.len() == expected);
    Ok(pixels)
}

pub(super) async fn hls(
    tools: &process::FixtureProcesses,
    root: &Path,
    name: &str,
    session: &TranscodeSession,
    init: &[u8],
    fragment: &[u8],
    copied: bool,
    source: &Value,
) -> anyhow::Result<Value> {
    let path = root.join(format!("{name}.mp4"));
    let timestamp = if copied {
        super::super::super::media::complete_mp4(init, true)?;
        super::super::super::media::complete_mp4(fragment, false)?;
        let mut bytes = Vec::with_capacity(init.len() + fragment.len());
        bytes.extend_from_slice(init);
        bytes.extend_from_slice(fragment);
        tokio::fs::write(&path, bytes).await?;
        None
    } else {
        Some(
            super::super::super::media::decode_start(
                tools,
                root,
                &super::super::super::media::FragmentContext {
                    name: name.into(),
                    source_seek_ms: Some(5000),
                    source_start_ms: 5000,
                    stream_plan: session.stream_plan,
                },
                init,
                fragment,
            )
            .await?,
        )
    };
    let probe: Value = serde_json::from_slice(
        &process::original(
            tools,
            "ffprobe",
            &[
                "-v".into(),
                "error".into(),
                "-show_streams".into(),
                "-of".into(),
                "json".into(),
                path.to_string_lossy().into_owned(),
            ],
        )
        .await?,
    )?;
    let streams = probe["streams"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("source output streams missing"))?;
    let video = streams
        .iter()
        .find(|value| value["codec_type"] == "video")
        .ok_or_else(|| anyhow::anyhow!("source output video missing"))?;
    let audio = streams
        .iter()
        .find(|value| value["codec_type"] == "audio")
        .ok_or_else(|| anyhow::anyhow!("source output audio missing"))?;
    let expected = if copied {
        session.source_video_codec.as_str()
    } else {
        "h264"
    };
    let tag = match expected {
        "h264" => "avc1",
        "hevc" => "hev1",
        "av1" => "av01",
        _ => anyhow::bail!("unexpected source output codec"),
    };
    anyhow::ensure!(
        streams.len() == 2
            && video["codec_name"] == expected
            && video["width"] == 320
            && video["height"] == 180
            && video["r_frame_rate"] == "24/1"
            && audio["codec_name"] == "aac"
            && audio["profile"] == "LC"
            && audio["sample_rate"] == "48000"
            && audio["channels"] == 1
    );
    if copied {
        anyhow::ensure!(
            video["codec_tag_string"] == tag
                && matches!(video["pix_fmt"].as_str(), Some("yuv420p" | "yuv420p10le"))
        );
        let source_video = source["probe"]["streams"]
            .as_array()
            .and_then(|streams| {
                streams
                    .iter()
                    .find(|stream| stream["codec_type"] == "video")
            })
            .ok_or_else(|| anyhow::anyhow!("copied reference video metadata missing"))?;
        for key in [
            "profile",
            "level",
            "pix_fmt",
            "color_primaries",
            "color_transfer",
            "color_space",
            "color_range",
        ] {
            anyhow::ensure!(
                video[key] == source_video[key],
                "copied output changed source field{key}"
            );
        }
    }
    let vs = seconds(&video["start_time"])?;
    let aus = seconds(&audio["start_time"])?;
    if copied {
        let origin = session.source_start_ms as f64 / 1000.0;
        anyhow::ensure!(
            origin <= 5.0
                && origin >= 3.0
                && vs >= origin - 0.1
                && vs <= 5.0
                && aus >= origin - 0.1
                && aus <= 5.1,
            "copied source preroll/timestamps outside admitted presentation video={vs},audio={aus},origin={origin}"
        );
    } else {
        anyhow::ensure!(
            (0.0..=0.25).contains(&vs) && (0.0..=0.25).contains(&aus) && (vs - aus).abs() <= 0.1,
            "source output timestamps misaligned video={vs},audio={aus}"
        );
    }
    let mut result = decode(tools, &path, "0:v:0", "0:a:0", false, copied, true).await?;
    if copied {
        let reference = native(tools, &session.source_path).await?;
        anyhow::ensure!(
            result["decoded_luma_sha256"] == reference["decoded_luma_sha256"],
            "copied output differs from actual source frame120"
        );
        let format = video["pix_fmt"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("copied pixel format missing"))?;
        let actual_pixels = pixels(tools, &path, format, true).await?;
        let source_pixels = pixels(tools, &session.source_path, format, false).await?;
        anyhow::ensure!(
            actual_pixels == source_pixels,
            "copied output changed native source pixel values"
        );
        result["native_pixels_sha256"] = json!(sha256(&actual_pixels));
        result["exact_source_pixels"] = json!(true);
    }
    result["decoded_timestamp"] = json!(timestamp);
    result["video_codec"] = video["codec_name"].clone();
    result["video_profile"] = video["profile"].clone();
    result["video_level"] = video["level"].clone();
    result["pixel_format"] = video["pix_fmt"].clone();
    result["video_start_seconds"] = json!(vs);
    result["audio_start_seconds"] = json!(aus);
    result["av_start_delta_seconds"] = json!(vs - aus);
    result["timestamp_alignment"] = json!(true);
    result["timestamp_alignment_basis"] = json!(if copied {
        "decoded_absolute_5s_presentation"
    } else {
        "stream_relative_start"
    });
    result["stream_offset_ms"] = json!(session.source_start_ms);
    result["requested_presentation_ms"] = json!(5000);
    result["preroll_retained"] = json!(copied);
    result["audio_sample_rate"] = audio["sample_rate"].clone();
    result["audio_channels"] = audio["channels"].clone();
    Ok(result)
}

pub(super) async fn recovered(
    tools: &process::FixtureProcesses,
    root: &Path,
    session: &TranscodeSession,
    phase: &str,
    client: &reqwest::Client,
    headers: &reqwest::header::HeaderMap,
) -> anyhow::Result<Value> {
    let (init, fragment) =
        source_http::fragments(client, session.id, &session.rendition_name, headers).await?;
    super::super::super::direct_media::decode_fragment(
        tools,
        root,
        &super::super::super::media::FragmentContext {
            name: phase.into(),
            source_seek_ms: Some(5000),
            source_start_ms: 5000,
            stream_plan: session.stream_plan,
        },
        &init,
        &fragment,
        30,
    )
    .await
}

fn seconds(value: &Value) -> anyhow::Result<f64> {
    let number = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("source timestamp missing"))?
        .parse::<f64>()?;
    anyhow::ensure!(number.is_finite());
    Ok(number)
}

fn frame_id(luma: &[u8]) -> anyhow::Result<u8> {
    anyhow::ensure!(luma.len() == 320 * 180);
    let mut frame = 0;
    for bit in 0..8 {
        let mut sum = 0u32;
        for y in 164..172 {
            for x in 24 + 32 * bit..40 + 32 * bit {
                sum += u32::from(luma[y * 320 + x]);
            }
        }
        let mean = sum as f64 / 128.0;
        anyhow::ensure!(
            mean <= 60.0 || mean >= 190.0,
            "source frame identifier cell is not a stable reference"
        );
        if mean > 125.0 {
            frame |= 1 << bit;
        }
    }
    Ok(frame)
}
