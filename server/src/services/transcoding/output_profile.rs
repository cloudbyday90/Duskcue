// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{HwAccelMethod, StartSessionParams, TranscodeRendition, plan::StreamAction};

pub const MAX_WIDTH: u32 = 1920;
pub const MAX_HEIGHT: u32 = 1080;
pub const FRAME_RATE: u32 = 30;
pub const VIDEO_MIN_BITRATE: u32 = 100_000;
pub const VIDEO_MAX_BITRATE: u32 = 10_000_000;
pub const AUDIO_SAMPLE_RATE: u32 = 48_000;
pub const AUDIO_MAX_CHANNELS: u32 = 2;
pub const AUDIO_MIN_BITRATE: u32 = 32_000;
pub const AUDIO_MAX_BITRATE: u32 = 192_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputProfile {
    CompatibleSdr,
}

impl OutputProfile {
    pub fn validate(self, params: &StartSessionParams) -> Result<(), &'static str> {
        if params.output_profile != Some(self)
            || params.stream_plan.video != StreamAction::Encode
            || params.stream_plan.audio != StreamAction::Encode
            || params.target_video_codec.as_deref() != Some("h264")
            || params.target_audio_codec.as_deref() != Some("aac")
            || params.hw_accel_override != Some(HwAccelMethod::Software)
        {
            return Err("compatible SDR output requires software H.264 and AAC encoding");
        }
        let (width, height) = params
            .target_resolution
            .ok_or("compatible SDR output requires an explicit resolution")?;
        if width == 0
            || height == 0
            || width % 2 != 0
            || height % 2 != 0
            || width > MAX_WIDTH
            || height > MAX_HEIGHT
            || width > params.source_video_resolution.0
            || height > params.source_video_resolution.1
        {
            return Err("compatible SDR output resolution exceeds its admitted envelope");
        }
        if !params
            .target_bitrate
            .is_some_and(|bitrate| (VIDEO_MIN_BITRATE..=VIDEO_MAX_BITRATE).contains(&bitrate))
            || !params
                .target_audio_bitrate
                .is_some_and(|bitrate| (AUDIO_MIN_BITRATE..=AUDIO_MAX_BITRATE).contains(&bitrate))
            || !params
                .target_audio_channels
                .is_some_and(|channels| (1..=AUDIO_MAX_CHANNELS).contains(&channels))
        {
            return Err("compatible SDR output requires bounded explicit bitrates and channels");
        }
        Ok(())
    }

    pub(super) fn rendition(
        self,
        params: &StartSessionParams,
    ) -> Result<TranscodeRendition, &'static str> {
        self.validate(params)?;
        let (width, height) = params.target_resolution.unwrap();
        Ok(TranscodeRendition {
            name: "compatible-sdr".into(),
            width,
            height,
            video_bitrate: params.target_bitrate.unwrap(),
            audio_bitrate: params.target_audio_bitrate.unwrap(),
            audio_channels: params.target_audio_channels.unwrap(),
        })
    }

    pub fn output_args(self) -> Vec<String> {
        vec![
            "-level:v".into(),
            "4.0".into(),
            "-r".into(),
            FRAME_RATE.to_string(),
            "-fps_mode:v".into(),
            "cfr".into(),
            "-profile:a".into(),
            "aac_low".into(),
            "-ar".into(),
            AUDIO_SAMPLE_RATE.to_string(),
        ]
    }
}

#[cfg(test)]
#[path = "output_profile_tests.rs"]
mod tests;
