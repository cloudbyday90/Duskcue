// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{ProbeResult, ScannerError};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FfprobeFormat {
    format_name: Option<String>,
    duration: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FfprobeDisposition {
    forced: Option<i64>,
    hearing_impaired: Option<i64>,
    default: Option<i64>,
    visual_impaired: Option<i64>,
    captions: Option<i64>,
    descriptions: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FfprobeStream {
    index: Option<i64>,
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    bit_rate: Option<String>,
    color_transfer: Option<String>,
    r_frame_rate: Option<String>,
    channels: Option<i32>,
    disposition: Option<FfprobeDisposition>,
    tags: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FfprobeChapter {
    id: Option<i64>,
    start_time: Option<String>,
    end_time: Option<String>,
    tags: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FfprobeOutput {
    format: Option<FfprobeFormat>,
    streams: Option<Vec<FfprobeStream>>,
    chapters: Option<Vec<FfprobeChapter>>,
}

pub(super) async fn probe_file(path: &Path) -> Result<ProbeResult, ScannerError> {
    let output = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "-show_chapters",
        ])
        .arg(path)
        .output()
        .await
        .map_err(|e| ScannerError::ProbeFailed {
            path: path.to_string_lossy().to_string(),
            error: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(ScannerError::ProbeFailed {
            path: path.to_string_lossy().to_string(),
            error: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    parse_probe_output(&output.stdout).map_err(|e| ScannerError::ProbeFailed {
        path: path.to_string_lossy().to_string(),
        error: format!("JSON parse error: {}", e),
    })
}

fn parse_probe_output(bytes: &[u8]) -> Result<ProbeResult, serde_json::Error> {
    let probe: FfprobeOutput = serde_json::from_slice(bytes)?;

    let streams = probe.streams.unwrap_or_default();
    let format = probe.format;

    let mut video_codec = None;
    let mut video_resolution = None;
    let mut video_bitrate = None;
    let mut video_dynamic_range = None;
    let mut video_frame_rate = None;
    let mut audio_codec = None;
    let mut audio_channels = None;
    let mut audio_language = None;
    let mut audio_bitrate = None;
    let mut additional_streams = serde_json::json!({});
    let mut audio_streams: Vec<serde_json::Value> = Vec::new();
    let mut subtitle_streams: Vec<serde_json::Value> = Vec::new();

    for stream in &streams {
        match stream.codec_type.as_deref() {
            Some("video") => {
                video_codec = stream.codec_name.clone();
                if let (Some(w), Some(h)) = (stream.width, stream.height) {
                    video_resolution = Some(format!("{}x{}", w, h));
                }
                video_bitrate = stream.bit_rate.as_ref().and_then(|b| b.parse::<i32>().ok());
                video_dynamic_range = stream.color_transfer.as_ref().map(|ct| match ct.as_str() {
                    "smpte2084" => "hdr10".to_string(),
                    "arib-std-b67" => "hlg".to_string(),
                    _ => "sdr".to_string(),
                });
                video_frame_rate = parse_frame_rate(stream.r_frame_rate.as_deref());
            }
            Some("audio") => {
                let language = stream
                    .tags
                    .as_ref()
                    .and_then(|t| t.get("language").cloned())
                    .unwrap_or_else(|| "und".to_string());
                let title = stream.tags.as_ref().and_then(|t| t.get("title").cloned());
                let bitrate = stream.bit_rate.as_ref().and_then(|b| b.parse::<i32>().ok());
                if audio_codec.is_none() {
                    audio_codec = stream.codec_name.clone();
                    audio_channels = stream.channels;
                    audio_language = Some(language.clone());
                    audio_bitrate = bitrate;
                }
                audio_streams.push(serde_json::json!({
                    "index": stream.index.unwrap_or(0),
                    "codec": stream.codec_name,
                    "channels": stream.channels,
                    "language": language,
                    "title": title,
                    "bitrate": bitrate,
                    "is_default": stream.disposition.as_ref().and_then(|d| disposition_flag(d.default)),
                    "is_audio_description": stream.disposition.as_ref().and_then(|d| disposition_flag(d.visual_impaired)),
                    "disposition": stream.disposition,
                }));
            }
            Some("subtitle") => {
                let lang = stream
                    .tags
                    .as_ref()
                    .and_then(|t| t.get("language").cloned())
                    .unwrap_or_else(|| "und".to_string());
                let title = stream.tags.as_ref().and_then(|t| t.get("title").cloned());
                let title_lower = title.as_ref().map(|t| t.to_lowercase()).unwrap_or_default();
                let disp_forced = stream
                    .disposition
                    .as_ref()
                    .and_then(|d| d.forced)
                    .map(|f| f == 1)
                    .unwrap_or(false);
                let disp_hi = stream
                    .disposition
                    .as_ref()
                    .and_then(|d| d.hearing_impaired)
                    .map(|f| f == 1)
                    .unwrap_or(false);
                let is_forced = disp_forced || title_lower.contains("forced");
                let is_hearing_impaired = disp_hi
                    || title_lower.contains("hearing impaired")
                    || title_lower.contains("sdh")
                    || title_lower.contains("cc");

                subtitle_streams.push(serde_json::json!({
                    "index": stream.index.unwrap_or(0),
                    "codec": stream.codec_name,
                    "language": lang,
                    "title": title,
                    "is_forced": is_forced,
                    "is_hearing_impaired": is_hearing_impaired,
                    "is_default": stream.disposition.as_ref().and_then(|d| disposition_flag(d.default)),
                    "disposition": stream.disposition,
                }));
            }
            _ => {}
        }
    }

    let runtime_seconds = format
        .as_ref()
        .and_then(|f| f.duration.as_ref())
        .and_then(|d| d.parse::<f64>().ok())
        .map(|d| d.round() as i32)
        .unwrap_or(0);

    let container_format = format
        .as_ref()
        .and_then(|f| f.format_name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let chapters: Vec<serde_json::Value> = probe
        .chapters
        .unwrap_or_default()
        .iter()
        .map(|ch| {
            serde_json::json!({
                "id": ch.id,
                "start_time": ch.start_time,
                "end_time": ch.end_time,
                "tags": ch.tags,
            })
        })
        .collect();

    if !chapters.is_empty() {
        additional_streams
            .as_object_mut()
            .unwrap_or(&mut serde_json::Map::new())
            .insert("chapters".to_string(), serde_json::json!(chapters));
    }

    if !audio_streams.is_empty() {
        additional_streams
            .as_object_mut()
            .unwrap_or(&mut serde_json::Map::new())
            .insert("audio".to_string(), serde_json::json!(audio_streams));
    }

    if !subtitle_streams.is_empty() {
        additional_streams
            .as_object_mut()
            .unwrap_or(&mut serde_json::Map::new())
            .insert("subtitles".to_string(), serde_json::json!(subtitle_streams));
    }

    Ok(ProbeResult {
        container_format,
        video_codec,
        video_resolution,
        video_bitrate,
        video_dynamic_range,
        video_frame_rate,
        audio_codec,
        audio_channels,
        audio_language,
        audio_bitrate,
        runtime_seconds: runtime_seconds.max(0),
        additional_streams,
    })
}

fn parse_frame_rate(rate: Option<&str>) -> Option<f64> {
    let rate_str = rate?;
    let parts: Vec<&str> = rate_str.split('/').collect();
    match parts.len() {
        1 => parts[0].parse::<f64>().ok(),
        2 => {
            let num: f64 = parts[0].parse().ok()?;
            let den: f64 = parts[1].parse().ok()?;
            if den == 0.0 { None } else { Some(num / den) }
        }
        _ => None,
    }
}

fn disposition_flag(value: Option<i64>) -> Option<bool> {
    match value {
        Some(0) => Some(false),
        Some(1) => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_probe_output;
    use serde_json::{Value, json};

    fn parse(value: Value) -> super::ProbeResult {
        parse_probe_output(&serde_json::to_vec(&value).unwrap()).unwrap()
    }

    #[test]
    fn preserves_real_stream_indices_languages_and_structured_audio_flags() {
        let probe = parse(json!({
            "format": {"format_name": "matroska,webm", "duration": "1800.4"},
            "streams": [
                {"index": 0, "codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080, "r_frame_rate": "24000/1001"},
                {"index": 2, "codec_type": "audio", "codec_name": "aac", "channels": 2, "bit_rate": "192000", "tags": {"language": "eng", "title": "English"}, "disposition": {"default": 1, "visual_impaired": 0}},
                {"index": 5, "codec_type": "audio", "codec_name": "aac", "channels": 2, "tags": {"language": "eng", "title": "Described"}, "disposition": {"default": 0, "visual_impaired": 1}}
            ]
        }));
        assert_eq!(probe.runtime_seconds, 1800);
        assert_eq!(probe.video_resolution.as_deref(), Some("1920x1080"));
        assert!((probe.video_frame_rate.unwrap() - 23.976).abs() < 0.001);
        assert_eq!(probe.audio_language.as_deref(), Some("eng"));
        assert_eq!(probe.audio_bitrate, Some(192000));
        let tracks = probe.additional_streams["audio"].as_array().unwrap();
        assert_eq!(tracks[0]["index"], 2);
        assert_eq!(tracks[0]["is_default"], true);
        assert_eq!(tracks[0]["is_audio_description"], false);
        assert_eq!(tracks[1]["index"], 5);
        assert_eq!(tracks[1]["is_audio_description"], true);
        assert_eq!(tracks[1]["disposition"]["visual_impaired"], 1);
    }

    #[test]
    fn missing_metadata_stays_unknown_even_for_description_titles() {
        let probe = parse(json!({"streams": [
            {"index": 1, "codec_type": "audio", "codec_name": "aac", "tags": {"title": "English Audio Description"}},
            {"index": 3, "codec_type": "subtitle", "codec_name": "subrip", "tags": {"title": "English SDH"}}
        ]}));
        let audio = &probe.additional_streams["audio"][0];
        assert_eq!(audio["language"], "und");
        assert!(audio["is_default"].is_null());
        assert!(audio["is_audio_description"].is_null());
        assert!(audio["disposition"].is_null());
        let subtitle = &probe.additional_streams["subtitles"][0];
        assert_eq!(subtitle["is_hearing_impaired"], true);
        assert!(subtitle["disposition"].is_null());
    }

    #[test]
    fn structured_subtitle_flags_do_not_inherit_legacy_title_heuristics() {
        let probe = parse(json!({"streams": [
            {"index": 3, "codec_type": "subtitle", "codec_name": "subrip", "tags": {"language": "eng", "title": "SDH forced"}, "disposition": {"default": 1, "forced": 0, "hearing_impaired": 0, "descriptions": 0}},
            {"index": 7, "codec_type": "subtitle", "codec_name": "subrip", "tags": {"language": "eng"}, "disposition": {"default": 0, "forced": 1, "hearing_impaired": 1, "descriptions": 0}}
        ]}));
        let subtitles = probe.additional_streams["subtitles"].as_array().unwrap();
        assert_eq!(subtitles[0]["is_default"], true);
        assert_eq!(subtitles[0]["is_forced"], true);
        assert_eq!(subtitles[0]["is_hearing_impaired"], true);
        assert_eq!(subtitles[0]["disposition"]["forced"], 0);
        assert_eq!(subtitles[0]["disposition"]["hearing_impaired"], 0);
        assert_eq!(subtitles[1]["disposition"]["hearing_impaired"], 1);
    }

    #[test]
    fn invalid_disposition_values_are_not_reported_as_known_flags() {
        let probe = parse(json!({"streams": [
            {"index": 1, "codec_type": "audio", "codec_name": "aac", "disposition": {"default": 2, "visual_impaired": -1}}
        ]}));
        let audio = &probe.additional_streams["audio"][0];
        assert!(audio["is_default"].is_null());
        assert!(audio["is_audio_description"].is_null());
        assert_eq!(audio["disposition"]["default"], 2);
    }

    #[test]
    fn keeps_chapters_and_text_description_dispositions_separate() {
        let probe = parse(json!({
            "streams": [{"index": 4, "codec_type": "subtitle", "codec_name": "webvtt", "disposition": {"descriptions": 1, "visual_impaired": 1, "captions": 0}}],
            "chapters": [{"id": 0, "start_time": "0.000", "end_time": "30.000", "tags": {"title": "Opening"}}]
        }));
        assert_eq!(
            probe.additional_streams["chapters"][0]["tags"]["title"],
            "Opening"
        );
        assert_eq!(
            probe.additional_streams["subtitles"][0]["disposition"]["descriptions"],
            1
        );
        assert!(
            probe.additional_streams["subtitles"][0]
                .get("is_audio_description")
                .is_none()
        );
        assert!(probe.additional_streams.get("audio").is_none());
    }

    #[test]
    fn rejects_malformed_probe_json_without_creating_tracks() {
        assert!(parse_probe_output(b"not json").is_err());
        assert!(
            parse_probe_output(br#"{"streams":[{"index":"one","codec_type":"audio"}]}"#).is_err()
        );
    }
}
