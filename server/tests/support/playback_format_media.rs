// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use duskcue::services::transcoding::TranscodeSession;
use serde_json::{Value, json};

use super::super::super::media as progressive;
use super::process;

pub(super) async fn decode(
    tools: &process::FixtureProcesses,
    root: &Path,
    name: &str,
    session: &TranscodeSession,
    initialization: &[u8],
    fragment: &[u8],
) -> anyhow::Result<Value> {
    let timestamp = progressive::decode_start(
        tools,
        root,
        &progressive::FragmentContext {
            name: name.into(),
            source_seek_ms: Some(5000),
            source_start_ms: 5000,
            stream_plan: session.stream_plan,
        },
        initialization,
        fragment,
    )
    .await?;
    let path = root.join(format!("{name}.mp4"));
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
        .ok_or_else(|| anyhow::anyhow!("format output streams missing"))?;
    let video = streams
        .iter()
        .find(|stream| stream["codec_type"] == "video")
        .ok_or_else(|| anyhow::anyhow!("format output video missing"))?;
    let audio = streams
        .iter()
        .find(|stream| stream["codec_type"] == "audio")
        .ok_or_else(|| anyhow::anyhow!("format output audio missing"))?;
    anyhow::ensure!(
        streams.len() == 2
            && video["codec_name"] == "h264"
            && video["profile"] == "High"
            && video["pix_fmt"] == "yuv420p"
            && video["width"] == 320
            && video["height"] == 180
            && video["r_frame_rate"] == "24/1"
            && audio["codec_name"] == "aac"
            && audio["profile"] == "LC"
            && audio["sample_rate"] == "48000"
            && audio["channels"] == 1
    );
    let video_start = seconds(&video["start_time"])?;
    let audio_start = seconds(&audio["start_time"])?;
    anyhow::ensure!(
        (0.0..=0.25).contains(&video_start)
            && (0.0..=0.25).contains(&audio_start)
            && (video_start - audio_start).abs() <= 0.1
    );
    let luma = process::original(
        tools,
        "ffmpeg",
        &[
            "-v".into(),
            "error".into(),
            "-nostdin".into(),
            "-i".into(),
            path.to_string_lossy().into_owned(),
            "-map".into(),
            "0:v:0".into(),
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
        ],
    )
    .await?;
    anyhow::ensure!(luma.len() == 320 * 180);
    let frame_id = frame_id(&luma)?;
    anyhow::ensure!(
        frame_id == 120,
        "format seek decoded frame {frame_id}, expected source frame120"
    );
    let pcm = process::original(
        tools,
        "ffmpeg",
        &[
            "-v".into(),
            "error".into(),
            "-nostdin".into(),
            "-i".into(),
            path.to_string_lossy().into_owned(),
            "-t".into(),
            "0.75".into(),
            "-map".into(),
            "0:a:0".into(),
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
        ],
    )
    .await?;
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
    let frequency = crossings as f64 * 8000.0 / (2.0 * samples.len() as f64);
    anyhow::ensure!((frequency - 880.0).abs() < 20.0);
    let content_type = avc_content_type(initialization)?;
    let video_bitrate = bitrate(video).unwrap_or(session.target_bitrate.into());
    let audio_bitrate = bitrate(audio)
        .or(session.target_audio_bitrate.map(u64::from))
        .ok_or_else(|| anyhow::anyhow!("format output audio bitrate missing"))?;
    anyhow::ensure!(video_bitrate > 0 && audio_bitrate > 0);
    Ok(
        json!({"actual_media":{"decoded_timestamp":timestamp,"selected_audio_hz":frequency,"video_profile":video["profile"],"video_level":video["level"],"video_frame_rate":video["r_frame_rate"],"audio_sample_rate":audio["sample_rate"],"audio_channels":audio["channels"],"video_start_seconds":video_start,"audio_start_seconds":audio_start,"av_start_delta_seconds":video_start-audio_start,"timestamp_alignment":true,"source_frame_id":frame_id},
        "capability":{"type":"media-source","video":{"contentType":content_type,"width":320,"height":180,"bitrate":video_bitrate,"framerate":24},"audio":{"contentType":"audio/mp4; codecs=\"mp4a.40.2\"","channels":"1","bitrate":audio_bitrate,"samplerate":48000}},
        "bitrate_source":{"video":if bitrate(video).is_some(){"probed"}else{"encoder_maxrate"},"audio":if bitrate(audio).is_some(){"probed"}else{"encoder_bitrate"}}}),
    )
}

fn seconds(value: &Value) -> anyhow::Result<f64> {
    let number = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("format stream timestamp missing"))?
        .parse::<f64>()?;
    anyhow::ensure!(number.is_finite());
    Ok(number)
}
fn bitrate(stream: &Value) -> Option<u64> {
    stream["bit_rate"]
        .as_str()?
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
}

fn avc_content_type(bytes: &[u8]) -> anyhow::Result<String> {
    let positions = bytes
        .windows(4)
        .enumerate()
        .filter_map(|(index, value)| (value == b"avcC").then_some(index))
        .collect::<Vec<_>>();
    anyhow::ensure!(positions.len() == 1);
    let index = positions[0];
    anyhow::ensure!(index >= 4 && index + 8 <= bytes.len() && bytes[index + 4] == 1);
    let size = u32::from_be_bytes(bytes[index - 4..index].try_into()?) as usize;
    anyhow::ensure!(size >= 12 && index - 4 + size <= bytes.len());
    Ok(format!(
        "video/mp4; codecs=\"avc1.{:02X}{:02X}{:02X}\"",
        bytes[index + 5],
        bytes[index + 6],
        bytes[index + 7]
    ))
}

fn frame_id(luma: &[u8]) -> anyhow::Result<u8> {
    anyhow::ensure!(luma.len() == 320 * 180);
    let mut result = 0u8;
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
            "format frame identifier cell is not a stable reference"
        );
        if mean > 125.0 {
            result |= 1 << bit;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_identifiers_distinguish_seek_target_from_start_and_damage() {
        for frame in [0u8, 120, 191] {
            let mut bytes = vec![16u8; 320 * 180];
            for bit in 0..8 {
                if frame & (1 << bit) != 0 {
                    for y in 160..176 {
                        for x in 16 + 32 * bit..48 + 32 * bit {
                            bytes[y * 320 + x] = 235;
                        }
                    }
                }
            }
            assert_eq!(frame_id(&bytes).unwrap(), frame);
        }
        assert!(frame_id(&vec![125; 320 * 180]).is_err());
    }
    #[test]
    fn avc_configuration_uses_actual_profile_compatibility_and_level() {
        let bytes = [0, 0, 0, 12, b'a', b'v', b'c', b'C', 1, 100, 0, 31];
        assert_eq!(
            avc_content_type(&bytes).unwrap(),
            "video/mp4; codecs=\"avc1.64001F\""
        );
        assert!(avc_content_type(&bytes[..11]).is_err());
    }
}
