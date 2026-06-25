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

static CODEC_ALIASES: &[(&str, &[&str])] = &[
    ("h264", &["h264", "avc", "avc1"]),
    ("hevc", &["hevc", "h265", "hvc1"]),
    ("av1", &["av1"]),
    ("vp9", &["vp9", "vp09"]),
    ("mpeg2video", &["mpeg2video", "mpeg2"]),
    ("vc1", &["vc1", "vc-1"]),
    ("vp8", &["vp8"]),
    ("aac", &["aac"]),
    ("ac3", &["ac3", "ac-3"]),
    ("eac3", &["eac3", "e-ac-3", "dd+", "dolby_digital_plus"]),
    ("truehd", &["truehd", "thd", "thd+atmos"]),
    ("dts", &["dts"]),
    ("dts_hd_ma", &["dts-hd_ma", "dts_hd_ma", "dtshd", "dts-hd"]),
    ("flac", &["flac"]),
    ("opus", &["opus"]),
    ("vorbis", &["vorbis"]),
    (
        "pcm",
        &["pcm", "lpcm", "pcm_s16le", "pcm_s24le", "pcm_s32le"],
    ),
    ("alac", &["alac"]),
    ("mp3", &["mp3"]),
];

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
        AudioDecision::Passthrough | AudioDecision::Downmix => None,
        AudioDecision::Transcode => Some(select_target_audio_codec(media, device, config)),
    };

    let target_resolution =
        compute_target_resolution(media, device, network, config, video_decision);

    let target_bitrate_bps = compute_target_bitrate(media, network, config, target_resolution);

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
    if config.quality_mode == QualityMode::Maximum
        && codec_supported(&media.video_codec, &device.video_codecs)
    {
        reasons.push(DecisionReason {
            factor: "quality_mode",
            result: "direct_play",
            detail: "maximum quality mode, codec supported".to_string(),
        });
        return VideoDecision::DirectPlay;
    }

    if !codec_supported(&media.video_codec, &device.video_codecs) {
        reasons.push(DecisionReason {
            factor: "video_codec",
            result: "transcode",
            detail: format!("codec '{}' not in device supported list", media.video_codec),
        });
        return VideoDecision::Transcode;
    }

    if media.video_bit_depth > device.max_video_bit_depth && device.max_video_bit_depth > 0 {
        reasons.push(DecisionReason {
            factor: "bit_depth",
            result: "transcode",
            detail: format!(
                "source {}-bit exceeds device max {}-bit",
                media.video_bit_depth, device.max_video_bit_depth
            ),
        });
        return VideoDecision::Transcode;
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
        return VideoDecision::Transcode;
    }

    let is_hdr = is_hdr_content(&media.video_dynamic_range);
    let device_supports_hdr = device.hdr_formats.iter().any(|f| f != "sdr");

    if is_hdr && !device_supports_hdr {
        reasons.push(DecisionReason {
            factor: "hdr",
            result: "tone_map",
            detail: format!(
                "source '{}' but device has no HDR support",
                media.video_dynamic_range
            ),
        });
        return VideoDecision::ToneMap;
    }

    if let Some(dv_profile) = extract_dv_profile(media)
        && !device.supports_dolby_vision
    {
        if dv_profile == 7 || dv_profile == 8 {
            if device_supports_hdr && device.allow_client_side_dv_fallback {
                reasons.push(DecisionReason {
                    factor: "dolby_vision",
                    result: "direct_play",
                    detail: format!(
                        "DV Profile {} with HDR10 base, client-side fallback allowed",
                        dv_profile
                    ),
                });
                return VideoDecision::DirectPlay;
            } else if device_supports_hdr {
                reasons.push(DecisionReason {
                    factor: "dolby_vision",
                    result: "remux",
                    detail: format!("DV Profile {} stripping to HDR10 base layer", dv_profile),
                });
                return VideoDecision::Remux;
            } else {
                reasons.push(DecisionReason {
                    factor: "dolby_vision",
                    result: "tone_map",
                    detail: format!(
                        "DV Profile {} on SDR device, tone mapping required",
                        dv_profile
                    ),
                });
                return VideoDecision::ToneMap;
            }
        } else if dv_profile == 5 {
            reasons.push(DecisionReason {
                factor: "dolby_vision",
                result: "transcode",
                detail: "DV Profile 5 has no HDR fallback, must transcode".to_string(),
            });
            return VideoDecision::Transcode;
        }
    }

    if !codec_supported_container(&media.video_codec, &media.container_format, device)
        && codec_supported(&media.video_codec, &device.video_codecs)
    {
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

    let effective_bitrate = media.video_bitrate_bps + media.audio_bitrate_bps;
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
        return VideoDecision::Transcode;
    }

    if let Some(manual_res) = config.manual_max_resolution {
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
            return VideoDecision::Transcode;
        }
    }

    reasons.push(DecisionReason {
        factor: "video",
        result: "direct_play",
        detail: "all video checks passed".to_string(),
    });
    VideoDecision::DirectPlay
}

fn evaluate_audio(
    media: &MediaFileInfo,
    device: &DeviceCapabilities,
    config: &DecisionEngineConfig,
    reasons: &mut Vec<DecisionReason>,
) -> AudioDecision {
    if config.quality_mode == QualityMode::Maximum && config.audio_passthrough_enabled {
        reasons.push(DecisionReason {
            factor: "audio",
            result: "passthrough",
            detail: "maximum quality mode with passthrough".to_string(),
        });
        return AudioDecision::Passthrough;
    }

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

            if let Some(manual_res) = config.manual_max_resolution {
                target_w = target_w.min(manual_res.0);
                target_h = target_h.min(manual_res.1);
            }

            let (fb_w, fb_h) = config.fallback_max_resolution;
            target_w = target_w.min(fb_w);
            target_h = target_h.min(fb_h);

            Some(normalize_resolution(target_w, target_h))
        }
    }
}

fn compute_target_bitrate(
    media: &MediaFileInfo,
    network: &NetworkConditions,
    config: &DecisionEngineConfig,
    target_resolution: Option<(u32, u32)>,
) -> Option<u64> {
    let limit = bitrate_limit(network, config)?;

    let res = target_resolution.unwrap_or(media.video_resolution);
    let ladder = super::transcoding::TranscodeRendition::smart_ladder(
        res.0,
        res.1,
        Some((limit as f64 * config.throughput_safety_factor) as u64),
        None,
    );

    ladder.last().map(|r| r.video_bitrate as u64)
}

fn compute_overall_decision(
    video: VideoDecision,
    audio: AudioDecision,
    subtitle_burn_in: bool,
) -> StreamDecision {
    if video == VideoDecision::DirectPlay
        && matches!(audio, AudioDecision::Passthrough | AudioDecision::Downmix)
        && !subtitle_burn_in
    {
        let is_remux = video == VideoDecision::Remux;
        if is_remux {
            return StreamDecision::DirectStream;
        }
        StreamDecision::DirectPlay
    } else if video == VideoDecision::Remux
        && matches!(audio, AudioDecision::Passthrough | AudioDecision::Downmix)
        && !subtitle_burn_in
    {
        StreamDecision::DirectStream
    } else {
        StreamDecision::Transcode
    }
}

pub fn codec_supported(codec: &str, supported: &HashSet<String>) -> bool {
    let codec_lower = codec.to_lowercase();
    if supported.contains(&codec_lower) {
        return true;
    }

    for &(canonical, aliases) in CODEC_ALIASES {
        if codec_lower == canonical || aliases.contains(&codec_lower.as_str()) {
            if supported.contains(canonical) {
                return true;
            }
            for alias in aliases {
                if supported.contains(*alias) {
                    return true;
                }
            }
        }
    }

    false
}

fn codec_supported_container(_codec: &str, container: &str, device: &DeviceCapabilities) -> bool {
    device.containers.contains(&container.to_lowercase()) || device.containers.contains("mkv")
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
    let dr = dynamic_range.to_lowercase();
    dr != "sdr" && !dr.is_empty()
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

fn normalize_resolution(_w: u32, h: u32) -> (u32, u32) {
    if h >= 2160 {
        (3840, 2160)
    } else if h >= 1080 {
        (1920, 1080)
    } else if h >= 720 {
        (1280, 720)
    } else {
        (854, 480)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sdr_media() -> MediaFileInfo {
        MediaFileInfo {
            container_format: "matroska".to_string(),
            video_codec: "h264".to_string(),
            video_profile: Some("High".to_string()),
            video_level: Some(4.1),
            video_bit_depth: 8,
            video_resolution: (1920, 1080),
            video_bitrate_bps: 8_000_000,
            video_dynamic_range: "sdr".to_string(),
            video_frame_rate: 23.976,
            audio_codec: "aac".to_string(),
            audio_channels: 2,
            audio_bitrate_bps: 192_000,
            audio_language: Some("eng".to_string()),
            has_embedded_subtitles: false,
            subtitle_format: None,
            additional_streams: None,
        }
    }

    fn basic_device() -> DeviceCapabilities {
        DeviceCapabilities {
            video_codecs: HashSet::from(["h264".to_string()]),
            audio_codecs: HashSet::from(["aac".to_string()]),
            containers: HashSet::from(["mp4".to_string(), "mkv".to_string()]),
            subtitle_formats: HashSet::from(["srt".to_string(), "webvtt".to_string()]),
            max_resolution: (1920, 1080),
            max_audio_channels: 2,
            hdr_formats: HashSet::new(),
            max_bitrate_bps: 20_000_000,
            supports_dolby_vision: false,
            allow_client_side_dv_fallback: true,
            max_video_bit_depth: 8,
        }
    }

    fn default_config() -> DecisionEngineConfig {
        DecisionEngineConfig {
            default_video_codec: "h264".to_string(),
            default_audio_codec: "aac".to_string(),
            fallback_max_resolution: (3840, 2160),
            fallback_max_bitrate_bps: 6_000_000,
            throughput_safety_factor: 0.8,
            allow_client_side_dv_fallback: true,
            audio_passthrough_enabled: true,
            subtitle_burn_in_policy: "last_resort".to_string(),
            quality_mode: QualityMode::Auto,
            manual_max_resolution: None,
        }
    }

    fn good_network() -> NetworkConditions {
        NetworkConditions {
            estimated_throughput_bps: Some(50_000_000),
            network_tier: Some("excellent".to_string()),
        }
    }

    #[test]
    fn test_direct_play_h264_aac_mkv() {
        let decision = decide(
            &sdr_media(),
            &basic_device(),
            &good_network(),
            &default_config(),
        );
        assert_eq!(decision.overall, StreamDecision::DirectPlay);
        assert_eq!(decision.video, VideoDecision::DirectPlay);
        assert_eq!(decision.audio, AudioDecision::Passthrough);
    }

    #[test]
    fn test_transcode_unsupported_video_codec() {
        let mut media = sdr_media();
        media.video_codec = "hevc".to_string();
        let device = basic_device();
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.overall, StreamDecision::Transcode);
        assert_eq!(decision.video, VideoDecision::Transcode);
        assert_eq!(decision.target_video_codec, Some("h264".to_string()));
    }

    #[test]
    fn test_transcode_resolution_exceeds_device() {
        let mut media = sdr_media();
        media.video_resolution = (3840, 2160);
        let device = basic_device();
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.video, VideoDecision::Transcode);
        assert_eq!(decision.target_resolution, Some((1920, 1080)));
    }

    #[test]
    fn test_tone_map_hdr_to_sdr() {
        let mut media = sdr_media();
        media.video_dynamic_range = "hdr10".to_string();
        media.video_bit_depth = 10;
        media.video_codec = "hevc".to_string();
        let mut device = basic_device();
        device.video_codecs.insert("hevc".to_string());
        device.max_video_bit_depth = 10;
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.video, VideoDecision::ToneMap);
        assert!(decision.requires_tone_mapping);
        assert_eq!(decision.target_video_codec, Some("h264".to_string()));
    }

    #[test]
    fn test_audio_transcode_unsupported_codec() {
        let mut media = sdr_media();
        media.audio_codec = "truehd".to_string();
        media.audio_channels = 8;
        let device = basic_device();
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.audio, AudioDecision::Transcode);
        assert_eq!(decision.target_audio_codec, Some("aac".to_string()));
        assert_eq!(decision.target_audio_channels, Some(2));
    }

    #[test]
    fn test_audio_downmix() {
        let mut media = sdr_media();
        media.audio_channels = 6;
        let mut device = basic_device();
        device.audio_codecs.insert("aac".to_string());
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.audio, AudioDecision::Downmix);
    }

    #[test]
    fn test_subtitle_passthrough_srt() {
        let mut media = sdr_media();
        media.subtitle_format = Some("srt".to_string());
        let device = basic_device();
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.subtitle, SubtitleDecision::Passthrough);
    }

    #[test]
    fn test_subtitle_burn_in_pgs() {
        let mut media = sdr_media();
        media.subtitle_format = Some("pgs".to_string());
        media.video_codec = "h264".to_string();
        let device = basic_device();
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.subtitle, SubtitleDecision::BurnIn);
        assert!(decision.subtitle_burn_in_required);
    }

    #[test]
    fn test_subtitle_convert_ass_to_srt() {
        let mut media = sdr_media();
        media.subtitle_format = Some("ass".to_string());
        let device = basic_device();
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.subtitle, SubtitleDecision::Convert);
    }

    #[test]
    fn test_dv_profile7_client_fallback() {
        let mut media = sdr_media();
        media.video_codec = "hevc".to_string();
        media.video_dynamic_range = "dolby_vision_p7".to_string();
        media.video_bit_depth = 10;
        media.additional_streams = Some(serde_json::json!({
            "dolby_vision": { "profile": 7, "level": 6, "compatibility_mode": "hdr10" }
        }));
        let mut device = basic_device();
        device.video_codecs.insert("hevc".to_string());
        device.max_video_bit_depth = 10;
        device.hdr_formats.insert("hdr10".to_string());
        device.allow_client_side_dv_fallback = true;
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.video, VideoDecision::DirectPlay);
    }

    #[test]
    fn test_dv_profile7_no_fallback_strip() {
        let mut media = sdr_media();
        media.video_codec = "hevc".to_string();
        media.video_dynamic_range = "dolby_vision_p7".to_string();
        media.video_bit_depth = 10;
        media.additional_streams = Some(serde_json::json!({
            "dolby_vision": { "profile": 7, "level": 6 }
        }));
        let mut device = basic_device();
        device.video_codecs.insert("hevc".to_string());
        device.max_video_bit_depth = 10;
        device.hdr_formats.insert("hdr10".to_string());
        device.allow_client_side_dv_fallback = false;
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.video, VideoDecision::Remux);
        assert!(decision.requires_dv_strip);
    }

    #[test]
    fn test_dv_profile5_must_transcode() {
        let mut media = sdr_media();
        media.video_codec = "hevc".to_string();
        media.video_dynamic_range = "dolby_vision_p5".to_string();
        media.video_bit_depth = 10;
        media.additional_streams = Some(serde_json::json!({
            "dolby_vision": { "profile": 5, "level": 6 }
        }));
        let mut device = basic_device();
        device.video_codecs.insert("hevc".to_string());
        device.max_video_bit_depth = 10;
        device.hdr_formats.insert("hdr10".to_string());
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.video, VideoDecision::Transcode);
    }

    #[test]
    fn test_maximum_quality_mode() {
        let mut media = sdr_media();
        media.video_codec = "hevc".to_string();
        let mut device = basic_device();
        device.video_codecs.insert("hevc".to_string());
        let mut config = default_config();
        config.quality_mode = QualityMode::Maximum;
        let decision = decide(&media, &device, &good_network(), &config);
        assert_eq!(decision.overall, StreamDecision::DirectPlay);
        assert_eq!(decision.video, VideoDecision::DirectPlay);
    }

    #[test]
    fn test_bitrate_exceeds_network() {
        let mut media = sdr_media();
        media.video_bitrate_bps = 50_000_000;
        media.audio_bitrate_bps = 5_000_000;
        let device = basic_device();
        let network = NetworkConditions {
            estimated_throughput_bps: Some(20_000_000),
            network_tier: Some("good".to_string()),
        };
        let decision = decide(&media, &device, &network, &default_config());
        assert_eq!(decision.video, VideoDecision::Transcode);
    }

    #[test]
    fn test_codec_alias_matching() {
        let supported: HashSet<String> = HashSet::from(["h264".to_string()]);
        assert!(codec_supported("h264", &supported));
        assert!(codec_supported("avc", &supported));
        assert!(codec_supported("avc1", &supported));
        assert!(!codec_supported("hevc", &supported));
    }

    #[test]
    fn test_remux_container_mismatch() {
        let mut media = sdr_media();
        media.container_format = "matroska".to_string();
        let mut device = basic_device();
        device.containers = HashSet::from(["mp4".to_string()]);
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.video, VideoDecision::Remux);
        assert_eq!(decision.overall, StreamDecision::DirectStream);
    }

    #[test]
    fn test_10bit_exceeds_device() {
        let mut media = sdr_media();
        media.video_bit_depth = 10;
        let device = basic_device();
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.video, VideoDecision::Transcode);
    }

    #[test]
    fn test_parse_resolution_string() {
        assert_eq!(parse_resolution_string("4k"), (3840, 2160));
        assert_eq!(parse_resolution_string("1080p"), (1920, 1080));
        assert_eq!(parse_resolution_string("720p"), (1280, 720));
        assert_eq!(parse_resolution_string("480p"), (854, 480));
        assert_eq!(parse_resolution_string("unknown"), (1920, 1080));
    }

    #[test]
    fn test_normalize_resolution() {
        assert_eq!(normalize_resolution(3840, 2160), (3840, 2160));
        assert_eq!(normalize_resolution(2560, 1440), (1920, 1080));
        assert_eq!(normalize_resolution(1920, 1080), (1920, 1080));
        assert_eq!(normalize_resolution(1280, 720), (1280, 720));
        assert_eq!(normalize_resolution(640, 480), (854, 480));
    }

    #[test]
    fn test_audio_passthrough_disabled() {
        let mut config = default_config();
        config.audio_passthrough_enabled = false;
        let decision = decide(&sdr_media(), &basic_device(), &good_network(), &config);
        assert_eq!(decision.audio, AudioDecision::Transcode);
    }

    #[test]
    fn test_subtitle_pgs_when_supported() {
        let mut media = sdr_media();
        media.subtitle_format = Some("pgs".to_string());
        let mut device = basic_device();
        device.subtitle_formats.insert("pgs".to_string());
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.subtitle, SubtitleDecision::Passthrough);
    }
}
