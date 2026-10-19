// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) mod configuration;
mod identity;
mod validation;

pub use identity::ScanIdentity;
pub(crate) use identity::ScanSnapshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackDescriptor {
    pub version: u8,
    pub scan: ScanIdentity,
    pub moving_video_count: u32,
    pub video: VideoDescriptor,
    pub audio: Vec<AudioDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoDescriptor {
    pub index: i32,
    pub codec: String,
    pub file_codec: Option<String>,
    pub copy_codec: String,
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub frame_rate: f64,
    pub bitrate: Option<u64>,
    pub bitrate_basis: Option<String>,
    pub config_sha256: String,
    pub dynamic_range: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDescriptor {
    pub index: i32,
    pub codec: String,
    pub file_codec: String,
    pub copy_codec: Option<String>,
    pub bitrate: Option<u64>,
    pub sample_rate: u32,
    pub channels: u16,
    pub config_sha256: String,
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl PlaybackDescriptor {
    pub fn from_metadata(metadata: &Value) -> Option<Self> {
        let value = metadata.get("playback_descriptor")?;
        let descriptor: Self = serde_json::from_value(value.clone()).ok()?;
        let video = &descriptor.video;
        if descriptor.version != 1
            || descriptor.scan.size == 0
            || !descriptor.scan.valid()
            || !(1..=32).contains(&descriptor.moving_video_count)
            || video.index < 0
            || !(1..=16384).contains(&video.width)
            || !(1..=16384).contains(&video.height)
            || ![8, 10].contains(&video.bit_depth)
            || !video.frame_rate.is_finite()
            || !(0.001..=1000.0).contains(&video.frame_rate)
            || !valid_hash(&video.config_sha256)
            || !validation::valid_codec(&video.copy_codec, &video.codec)
            || !matches!(
                (
                    video.codec.as_str(),
                    video.copy_codec.split_once('.').map(|pair| pair.0)
                ),
                ("h264", Some("avc1")) | ("hevc", Some("hev1")) | ("av1", Some("av01"))
            )
            || video
                .file_codec
                .as_deref()
                .is_some_and(|value| !validation::valid_codec(value, &video.codec))
            || video.file_codec.as_deref().is_some_and(|value| {
                value.split_once('.').map(|pair| pair.1)
                    != video.copy_codec.split_once('.').map(|pair| pair.1)
            })
            || (video.codec == "av1"
                && video
                    .copy_codec
                    .split('.')
                    .nth(3)
                    .and_then(|value| value.parse::<u8>().ok())
                    != Some(video.bit_depth))
            || !["sdr", "hdr10", "hlg", "unknown"].contains(&video.dynamic_range.as_str())
            || video
                .bitrate
                .is_some_and(|value| value == 0 || value > 100_000_000_000)
            || !matches!(
                (video.bitrate, video.bitrate_basis.as_deref()),
                (None, None) | (Some(_), Some("stream" | "container_average"))
            )
            || descriptor.audio.len() > 32
        {
            return None;
        }
        let mut indices = std::collections::HashSet::from([video.index]);
        for audio in &descriptor.audio {
            if audio.index < 0
                || !indices.insert(audio.index)
                || !valid_hash(&audio.config_sha256)
                || !validation::valid_codec(&audio.file_codec, &audio.codec)
                || !(1..=8).contains(&audio.channels)
                || !(7350..=192000).contains(&audio.sample_rate)
                || audio
                    .bitrate
                    .is_some_and(|value| value == 0 || value > 100_000_000)
                || audio
                    .copy_codec
                    .as_deref()
                    .is_some_and(|value| audio.codec != "aac" || value != "mp4a.40.2")
            {
                return None;
            }
        }
        Some(descriptor)
    }
}

#[cfg(test)]
mod tests;
