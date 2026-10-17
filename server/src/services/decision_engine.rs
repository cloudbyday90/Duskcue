// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

mod compatibility;

#[cfg(test)]
mod tests;

pub use compatibility::codec_supported;
use compatibility::{container_supported, hdr_supported};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamDecision {
    DirectPlay,
    DirectStream,
    Transcode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoDecision {
    DirectPlay,
    Remux,
    Transcode,
    ToneMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioDecision {
    Passthrough,
    Downmix,
    Transcode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleDecision {
    Passthrough,
    Convert,
    BurnIn,
}

#[derive(Debug, Clone)]
pub struct PlaybackDecision {
    pub overall: StreamDecision,
    pub video: VideoDecision,
    pub audio: AudioDecision,
    pub subtitle: SubtitleDecision,
    pub target_video_codec: Option<String>,
    pub target_audio_codec: Option<String>,
    pub target_resolution: Option<(u32, u32)>,
    pub target_bitrate_bps: Option<u64>,
    pub target_audio_channels: Option<u32>,
    pub requires_tone_mapping: bool,
    pub requires_dv_strip: bool,
    pub subtitle_burn_in_required: bool,
    pub decision_reasons: Vec<DecisionReason>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecisionReason {
    pub factor: &'static str,
    pub result: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct MediaFileInfo {
    pub container_format: String,
    pub video_codec: String,
    pub video_profile: Option<String>,
    pub video_level: Option<f32>,
    pub video_bit_depth: u32,
    pub video_resolution: (u32, u32),
    pub video_bitrate_bps: u64,
    pub video_dynamic_range: String,
    pub video_frame_rate: f64,
    pub audio_codec: String,
    pub audio_channels: u32,
    pub audio_bitrate_bps: u64,
    pub audio_language: Option<String>,
    pub has_embedded_subtitles: bool,
    pub subtitle_format: Option<String>,
    pub additional_streams: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct DeviceCapabilities {
    pub video_codecs: HashSet<String>,
    pub audio_codecs: HashSet<String>,
    pub containers: HashSet<String>,
    pub subtitle_formats: HashSet<String>,
    pub max_resolution: (u32, u32),
    pub max_audio_channels: u32,
    pub hdr_formats: HashSet<String>,
    pub max_bitrate_bps: u64,
    pub supports_dolby_vision: bool,
    pub allow_client_side_dv_fallback: bool,
    pub max_video_bit_depth: u32,
}

#[derive(Debug, Clone)]
pub struct NetworkConditions {
    pub estimated_throughput_bps: Option<u64>,
    pub network_tier: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DecisionEngineConfig {
    pub default_video_codec: String,
    pub default_audio_codec: String,
    pub fallback_max_resolution: (u32, u32),
    pub fallback_max_bitrate_bps: u64,
    pub throughput_safety_factor: f64,
    pub allow_client_side_dv_fallback: bool,
    pub audio_passthrough_enabled: bool,
    pub subtitle_burn_in_policy: String,
    pub quality_mode: QualityMode,
    pub manual_max_resolution: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityMode {
    Auto,
    Maximum,
    Manual,
}

#[allow(dead_code)]
static FMP4_COMPATIBLE_AUDIO: &[&str] = &["aac", "ac3", "eac3", "flac", "opus", "pcm"];

static TEXT_SUBTITLE_FORMATS: &[&str] = &["srt", "webvtt", "ass", "ssa"];
static IMAGE_SUBTITLE_FORMATS: &[&str] = &["pgs", "vobsub", "dvd_subtitle"];

pub fn decide(
    media: &MediaFileInfo,
    device: &DeviceCapabilities,
    network: &NetworkConditions,
    config: &DecisionEngineConfig,
) -> PlaybackDecision {
    let mut reasons = Vec::new();

    let video_decision = evaluate_video(media, device, network, config, &mut reasons);
    let audio_decision = evaluate_audio(media, device, config, &mut reasons);
    let subtitle_decision = evaluate_subtitle(media, device, config, &mut reasons);

    let target_video_codec = match video_decision {
        VideoDecision::DirectPlay | VideoDecision::Remux => None,
        VideoDecision::Transcode | VideoDecision::ToneMap => Some(select_target_video_codec(
            media,
            device,
            config,
            video_decision,
        )),
    };

    let target_audio_codec = match audio_decision {
        AudioDecision::Passthrough => None,
        AudioDecision::Downmix | AudioDecision::Transcode => {
            Some(select_target_audio_codec(media, device, config))
        }
    };

    let target_resolution =
        compute_target_resolution(media, device, network, config, video_decision);

    let target_bitrate_bps =
        compute_target_bitrate(media, device, network, config, target_resolution);

    let target_audio_channels = match audio_decision {
        AudioDecision::Passthrough => None,
        AudioDecision::Downmix | AudioDecision::Transcode => {
            Some(device.max_audio_channels.min(media.audio_channels))
        }
    };

    let requires_tone_mapping = video_decision == VideoDecision::ToneMap;
    let requires_dv_strip = video_decision == VideoDecision::Remux && needs_dv_strip(media, device);
    let subtitle_burn_in_required = subtitle_decision == SubtitleDecision::BurnIn;

    let overall =
        compute_overall_decision(video_decision, audio_decision, subtitle_burn_in_required);

    PlaybackDecision {
        overall,
        video: video_decision,
        audio: audio_decision,
        subtitle: subtitle_decision,
        target_video_codec,
        target_audio_codec,
        target_resolution,
        target_bitrate_bps,
        target_audio_channels,
        requires_tone_mapping,
        requires_dv_strip,
        subtitle_burn_in_required,
        decision_reasons: reasons,
    }
}

fn evaluate_video(
    media: &MediaFileInfo,
    device: &DeviceCapabilities,
    network: &NetworkConditions,
    config: &DecisionEngineConfig,
    reasons: &mut Vec<DecisionReason>,
) -> VideoDecision {
    let mut decision = VideoDecision::DirectPlay;

    if !codec_supported(&media.video_codec, &device.video_codecs) {
        reasons.push(DecisionReason {
            factor: "video_codec",
            result: "transcode",
            detail: format!("codec '{}' not in device supported list", media.video_codec),
        });
        decision = VideoDecision::Transcode;
    }

    if media.video_bit_depth == 0 {
        reasons.push(DecisionReason {
            factor: "bit_depth",
            result: "transcode",
            detail: "source video bit depth is unknown".to_string(),
        });
        decision = VideoDecision::Transcode;
    } else if media.video_bit_depth > device.max_video_bit_depth && device.max_video_bit_depth > 0 {
        reasons.push(DecisionReason {
            factor: "bit_depth",
            result: "transcode",
            detail: format!(
                "source {}-bit exceeds device max {}-bit",
                media.video_bit_depth, device.max_video_bit_depth
            ),
        });
        decision = VideoDecision::Transcode;
    }

    let (src_w, src_h) = media.video_resolution;
    let (dev_w, dev_h) = device.max_resolution;
    if src_w > dev_w || src_h > dev_h {
        reasons.push(DecisionReason {
            factor: "resolution",
            result: "transcode",
            detail: format!(
                "source {}x{} exceeds device max {}x{}",
                src_w, src_h, dev_w, dev_h
            ),
        });
        decision = VideoDecision::Transcode;
    }

    let dynamic_range_decision = evaluate_dynamic_range(media, device, config, reasons);
    if dynamic_range_decision == VideoDecision::ToneMap {
        decision = VideoDecision::ToneMap;
    } else if dynamic_range_decision == VideoDecision::Transcode
        || (decision == VideoDecision::DirectPlay && dynamic_range_decision == VideoDecision::Remux)
    {
        decision = dynamic_range_decision;
    }

    let effective_bitrate = media
        .video_bitrate_bps
        .saturating_add(media.audio_bitrate_bps);
    if device.max_bitrate_bps > 0 && effective_bitrate > device.max_bitrate_bps {
        reasons.push(DecisionReason {
            factor: "device_bitrate",
            result: "transcode",
            detail: format!(
                "source {} bps exceeds device limit {} bps",
                effective_bitrate, device.max_bitrate_bps
            ),
        });
        if decision != VideoDecision::ToneMap {
            decision = VideoDecision::Transcode;
        }
    }

    if let Some(limit) = bitrate_limit(network, config)
        && effective_bitrate > limit
    {
        reasons.push(DecisionReason {
            factor: "bitrate",
            result: "transcode",
            detail: format!(
                "source {} bps exceeds network limit {} bps",
                effective_bitrate, limit
            ),
        });
        if decision != VideoDecision::ToneMap {
            decision = VideoDecision::Transcode;
        }
    }

    if config.quality_mode == QualityMode::Manual
        && let Some(manual_res) = config.manual_max_resolution
    {
        let (max_w, max_h) = manual_res;
        if src_w > max_w || src_h > max_h {
            reasons.push(DecisionReason {
                factor: "quality_mode",
                result: "transcode",
                detail: format!(
                    "manual quality mode limits to {}x{}, source is {}x{}",
                    max_w, max_h, src_w, src_h
                ),
            });
            if decision != VideoDecision::ToneMap {
                decision = VideoDecision::Transcode;
            }
        }
    }

    if matches!(decision, VideoDecision::Transcode | VideoDecision::ToneMap) {
        return decision;
    }

    if !container_supported(&media.container_format, &device.containers) {
        reasons.push(DecisionReason {
            factor: "container",
            result: "remux",
            detail: format!(
                "container '{}' not supported, remuxing video stream copy",
                media.container_format
            ),
        });
        return VideoDecision::Remux;
    }

    if decision == VideoDecision::Remux {
        return decision;
    }

    reasons.push(DecisionReason {
        factor: "video",
        result: "direct_play",
        detail: "all video checks passed".to_string(),
    });
    VideoDecision::DirectPlay
}

fn evaluate_dynamic_range(
    media: &MediaFileInfo,
    device: &DeviceCapabilities,
    config: &DecisionEngineConfig,
    reasons: &mut Vec<DecisionReason>,
) -> VideoDecision {
    let dynamic_range = media.video_dynamic_range.trim().to_ascii_lowercase();
    let dv_profile = extract_dv_profile(media);
    if dv_profile.is_some() || dynamic_range.starts_with("dolby_vision") {
        if device.supports_dolby_vision {
            return VideoDecision::DirectPlay;
        }

        if let Some(profile @ (7 | 8)) = dv_profile
            && let Some(base_range) = dv_base_dynamic_range(media)
            && hdr_supported(base_range, &device.hdr_formats)
        {
            if config.allow_client_side_dv_fallback && device.allow_client_side_dv_fallback {
                reasons.push(DecisionReason {
                    factor: "dolby_vision",
                    result: "direct_play",
                    detail: format!(
                        "DV Profile {} with supported {} base, client-side fallback allowed",
                        profile, base_range
                    ),
                });
                return VideoDecision::DirectPlay;
            }
            reasons.push(DecisionReason {
                factor: "dolby_vision",
                result: "remux",
                detail: format!(
                    "DV Profile {} stripping to supported {} base layer",
                    profile, base_range
                ),
            });
            return VideoDecision::Remux;
        }

        reasons.push(DecisionReason {
            factor: "dolby_vision",
            result: "tone_map",
            detail: "Dolby Vision is unsupported and no supported fallback base is known"
                .to_string(),
        });
        return VideoDecision::ToneMap;
    }

    if dynamic_range != "sdr" && !is_hdr_content(&dynamic_range) {
        reasons.push(DecisionReason {
            factor: "dynamic_range",
            result: "transcode",
            detail: "source video dynamic range is unknown".to_string(),
        });
        return VideoDecision::Transcode;
    }

    if is_hdr_content(&dynamic_range) && !hdr_supported(&dynamic_range, &device.hdr_formats) {
        reasons.push(DecisionReason {
            factor: "hdr",
            result: "tone_map",
            detail: format!("source HDR format '{}' is not supported", dynamic_range),
        });
        return VideoDecision::ToneMap;
    }

    VideoDecision::DirectPlay
}

fn evaluate_audio(
    media: &MediaFileInfo,
    device: &DeviceCapabilities,
    config: &DecisionEngineConfig,
    reasons: &mut Vec<DecisionReason>,
) -> AudioDecision {
    if !config.audio_passthrough_enabled {
        reasons.push(DecisionReason {
            factor: "audio_passthrough",
            result: "transcode",
            detail: "audio passthrough disabled in config".to_string(),
        });
        return AudioDecision::Transcode;
    }

    if !codec_supported(&media.audio_codec, &device.audio_codecs) {
        reasons.push(DecisionReason {
            factor: "audio_codec",
            result: "transcode",
            detail: format!(
                "audio codec '{}' not in device supported list",
                media.audio_codec
            ),
        });
        return AudioDecision::Transcode;
    }

    if media.audio_channels > device.max_audio_channels {
        reasons.push(DecisionReason {
            factor: "audio_channels",
            result: "downmix",
            detail: format!(
                "source {}ch exceeds device max {}ch",
                media.audio_channels, device.max_audio_channels
            ),
        });
        return AudioDecision::Downmix;
    }

    reasons.push(DecisionReason {
        factor: "audio",
        result: "passthrough",
        detail: "audio codec and channels supported".to_string(),
    });
    AudioDecision::Passthrough
}

fn evaluate_subtitle(
    media: &MediaFileInfo,
    device: &DeviceCapabilities,
    config: &DecisionEngineConfig,
    reasons: &mut Vec<DecisionReason>,
) -> SubtitleDecision {
    let sub_fmt = match &media.subtitle_format {
        Some(f) => f,
        None => {
            return SubtitleDecision::Passthrough;
        }
    };

    let fmt_lower = sub_fmt.to_lowercase();

    if TEXT_SUBTITLE_FORMATS.contains(&fmt_lower.as_str()) {
        if (fmt_lower == "ass" || fmt_lower == "ssa")
            && !device.subtitle_formats.contains("ass")
            && !device.subtitle_formats.contains("ssa")
        {
            reasons.push(DecisionReason {
                factor: "subtitle",
                result: "convert",
                detail: "ASS/SSA subtitles not supported, converting to SRT".to_string(),
            });
            return SubtitleDecision::Convert;
        }
        reasons.push(DecisionReason {
            factor: "subtitle",
            result: "passthrough",
            detail: format!("text subtitle '{}' supported", fmt_lower),
        });
        return SubtitleDecision::Passthrough;
    }

    if IMAGE_SUBTITLE_FORMATS.contains(&fmt_lower.as_str()) {
        if device.subtitle_formats.contains(&fmt_lower) {
            reasons.push(DecisionReason {
                factor: "subtitle",
                result: "passthrough",
                detail: format!("image subtitle '{}' natively supported", fmt_lower),
            });
            return SubtitleDecision::Passthrough;
        }

        if config.subtitle_burn_in_policy != "last_resort" {
            reasons.push(DecisionReason {
                factor: "subtitle",
                result: "burn_in",
                detail: format!(
                    "image subtitle '{}' not supported, burn-in policy allows it",
                    fmt_lower
                ),
            });
            return SubtitleDecision::BurnIn;
        }

        reasons.push(DecisionReason {
            factor: "subtitle",
            result: "burn_in",
            detail: format!(
                "image subtitle '{}' not supported and no text alternative available",
                fmt_lower
            ),
        });
        return SubtitleDecision::BurnIn;
    }

    reasons.push(DecisionReason {
        factor: "subtitle",
        result: "passthrough",
        detail: format!("subtitle format '{}' treated as passthrough", fmt_lower),
    });
    SubtitleDecision::Passthrough
}

fn select_target_video_codec(
    media: &MediaFileInfo,
    device: &DeviceCapabilities,
    config: &DecisionEngineConfig,
    decision: VideoDecision,
) -> String {
    let is_hdr = decision == VideoDecision::ToneMap;

    if !is_hdr && codec_supported("hevc", &device.video_codecs) {
        let (_, src_h) = media.video_resolution;
        if src_h > 1080 || media.video_bit_depth >= 10 {
            return "hevc".to_string();
        }
    }

    config.default_video_codec.clone()
}

fn select_target_audio_codec(
    media: &MediaFileInfo,
    device: &DeviceCapabilities,
    config: &DecisionEngineConfig,
) -> String {
    if codec_supported("opus", &device.audio_codecs) {
        return "opus".to_string();
    }

    if media.audio_channels > 2 && codec_supported("eac3", &device.audio_codecs) {
        return "eac3".to_string();
    }

    if media.audio_channels > 2 && codec_supported("ac3", &device.audio_codecs) {
        return "ac3".to_string();
    }

    config.default_audio_codec.clone()
}

fn compute_target_resolution(
    media: &MediaFileInfo,
    device: &DeviceCapabilities,
    network: &NetworkConditions,
    config: &DecisionEngineConfig,
    video_decision: VideoDecision,
) -> Option<(u32, u32)> {
    match video_decision {
        VideoDecision::DirectPlay | VideoDecision::Remux => None,
        VideoDecision::Transcode | VideoDecision::ToneMap => {
            let (src_w, src_h) = media.video_resolution;
            let (dev_w, dev_h) = device.max_resolution;
            let mut target_w = src_w.min(dev_w);
            let mut target_h = src_h.min(dev_h);

            if let Some(limit) = bitrate_limit(network, config) {
                let max_res_for_bitrate = max_resolution_for_bitrate(limit);
                target_w = target_w.min(max_res_for_bitrate.0);
                target_h = target_h.min(max_res_for_bitrate.1);
            }

            if config.quality_mode == QualityMode::Manual
                && let Some(manual_res) = config.manual_max_resolution
            {
                target_w = target_w.min(manual_res.0);
                target_h = target_h.min(manual_res.1);
            }

            Some(normalize_resolution(target_w, target_h))
        }
    }
}

fn compute_target_bitrate(
    media: &MediaFileInfo,
    device: &DeviceCapabilities,
    network: &NetworkConditions,
    config: &DecisionEngineConfig,
    target_resolution: Option<(u32, u32)>,
) -> Option<u64> {
    let limit = match (bitrate_limit(network, config), device.max_bitrate_bps) {
        (Some(network_limit), device_limit) if device_limit > 0 => network_limit.min(device_limit),
        (Some(network_limit), _) => network_limit,
        (None, device_limit) if device_limit > 0 => device_limit,
        _ => return None,
    };

    let res = target_resolution.unwrap_or(media.video_resolution);
    let ladder =
        super::transcoding::TranscodeRendition::smart_ladder(res.0, res.1, Some(limit), None);

    ladder.last().map(|r| (r.video_bitrate as u64).min(limit))
}

fn compute_overall_decision(
    video: VideoDecision,
    audio: AudioDecision,
    subtitle_burn_in: bool,
) -> StreamDecision {
    if video == VideoDecision::DirectPlay
        && audio == AudioDecision::Passthrough
        && !subtitle_burn_in
    {
        StreamDecision::DirectPlay
    } else if video == VideoDecision::Remux
        && audio == AudioDecision::Passthrough
        && !subtitle_burn_in
    {
        StreamDecision::DirectStream
    } else {
        StreamDecision::Transcode
    }
}

fn dv_base_dynamic_range(media: &MediaFileInfo) -> Option<&'static str> {
    match media
        .additional_streams
        .as_ref()
        .and_then(|streams| streams.get("dolby_vision"))
        .and_then(|dv| dv.get("compatibility_mode"))
        .and_then(serde_json::Value::as_str)
    {
        Some("hdr10") => return Some("hdr10"),
        Some("hlg") => return Some("hlg"),
        Some("sdr") => return Some("sdr"),
        _ => {}
    }
    match media
        .video_dynamic_range
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "dolby_vision_p8.1" => Some("hdr10"),
        "dolby_vision_p8.4" => Some("hlg"),
        _ if extract_dv_profile(media) == Some(7) => Some("hdr10"),
        _ => None,
    }
}

fn needs_dv_strip(media: &MediaFileInfo, device: &DeviceCapabilities) -> bool {
    if let Some(dv_profile) = extract_dv_profile(media)
        && (dv_profile == 7 || dv_profile == 8)
        && !device.supports_dolby_vision
    {
        return true;
    }
    false
}

fn is_hdr_content(dynamic_range: &str) -> bool {
    let dr = dynamic_range.trim().to_ascii_lowercase();
    matches!(dr.as_str(), "hdr10" | "hdr10_plus" | "hdr10+" | "hlg")
        || dr.starts_with("dolby_vision")
}

fn extract_dv_profile(media: &MediaFileInfo) -> Option<u32> {
    let streams = media.additional_streams.as_ref()?;
    let dv = streams.get("dolby_vision")?;
    let profile = dv.get("profile")?;
    profile.as_u64().map(|p| p as u32)
}

fn bitrate_limit(network: &NetworkConditions, config: &DecisionEngineConfig) -> Option<u64> {
    if config.quality_mode == QualityMode::Maximum {
        return None;
    }

    network
        .estimated_throughput_bps
        .map(|t| (t as f64 * config.throughput_safety_factor) as u64)
}

fn max_resolution_for_bitrate(bitrate_bps: u64) -> (u32, u32) {
    if bitrate_bps >= 5_000_000 {
        (1920, 1080)
    } else if bitrate_bps >= 2_000_000 {
        (1280, 720)
    } else {
        (854, 480)
    }
}

fn normalize_resolution(w: u32, h: u32) -> (u32, u32) {
    [(3840, 2160), (1920, 1080), (1280, 720), (854, 480)]
        .into_iter()
        .find(|&(width, height)| width <= w && height <= h)
        .unwrap_or((w / 2 * 2, h / 2 * 2))
}

pub fn parse_resolution_string(res: &str) -> (u32, u32) {
    match res.to_lowercase().as_str() {
        "4k" | "2160p" | "uhd" => (3840, 2160),
        "1080p" | "fullhd" | "fhd" => (1920, 1080),
        "720p" | "hd" => (1280, 720),
        "480p" | "sd" => (854, 480),
        _ => (1920, 1080),
    }
}

pub fn parse_quality_mode(mode: &str) -> QualityMode {
    match mode {
        "maximum" => QualityMode::Maximum,
        "manual" => QualityMode::Manual,
        _ => QualityMode::Auto,
    }
}

pub fn parse_json_string_set(value: &serde_json::Value) -> HashSet<String> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_hdr_formats(value: &serde_json::Value) -> HashSet<String> {
    parse_json_string_set(value)
}

pub fn parse_resolution_value(value: &Option<String>) -> (u32, u32) {
    value
        .as_deref()
        .map(parse_resolution_string)
        .unwrap_or((1920, 1080))
}
