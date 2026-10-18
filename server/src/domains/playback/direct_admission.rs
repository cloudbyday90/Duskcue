// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::collections::HashSet;

use super::direct::{
    self, AdmissionLimits, DirectContext, FileIdentity, PreparedFallback, ResolvedTracks,
};
use super::media_source::{MediaFileDetails, SelectedTracks, source_resolution};
use super::types::{DirectFallbackAudio, DirectFallbackResponse, DirectFallbackVideo};
use crate::services::decision_engine::{
    self, AudioDecision, DecisionEngineConfig, DeviceCapabilities, MediaFileInfo,
    NetworkConditions, VideoDecision,
};
use crate::services::transcoding::output_profile::*;
use crate::services::transcoding::plan::{StreamAction, StreamPlan};

pub(super) async fn prepare(
    details: &MediaFileDetails,
    media: &MediaFileInfo,
    selected: &SelectedTracks,
    device: &DeviceCapabilities,
    network: &NetworkConditions,
    config: &DecisionEngineConfig,
    requested_bitrate: Option<u64>,
) -> DirectContext {
    let candidate = match (
        FileIdentity::read(details).await,
        ResolvedTracks::from_selected(selected),
        admit(media, device, network, config).and_then(|mut descriptor| {
            if let Some(limit) = requested_bitrate {
                descriptor.video.bitrate = descriptor
                    .video
                    .bitrate
                    .min(limit.min(u64::from(u32::MAX)) as u32);
            }
            direct::descriptor_valid(&descriptor).then_some(descriptor)
        }),
    ) {
        (Some(identity), Some(tracks), Some(descriptor))
            if tracks.subtitle.is_none() && source_resolution(details).is_some() =>
        {
            Some(PreparedFallback {
                identity,
                tracks,
                descriptor,
                limits: AdmissionLimits {
                    max_resolution: (
                        device.max_resolution.0.min(MAX_WIDTH),
                        device.max_resolution.1.min(MAX_HEIGHT),
                    ),
                    max_audio_channels: device.max_audio_channels.min(AUDIO_MAX_CHANNELS),
                    max_bitrate: device.max_bitrate_bps.min(u64::from(VIDEO_MAX_BITRATE)),
                    quality_mode: format!("{:?}", config.quality_mode).to_ascii_lowercase(),
                    requested_bitrate,
                },
            })
        }
        _ => None,
    };
    DirectContext {
        version: 1,
        candidate,
    }
}

fn admit(
    media: &MediaFileInfo,
    device: &DeviceCapabilities,
    network: &NetworkConditions,
    config: &DecisionEngineConfig,
) -> Option<DirectFallbackResponse> {
    if media.video_resolution.0 < 2
        || media.video_resolution.1 < 2
        || media.audio_channels == 0
        || media.video_bit_depth == 0
        || matches!(
            media.video_codec.trim().to_ascii_lowercase().as_str(),
            "" | "unknown" | "n/a"
        )
        || matches!(
            media.audio_codec.trim().to_ascii_lowercase().as_str(),
            "" | "unknown" | "n/a"
        )
    {
        return None;
    }
    let mut device = device.clone();
    device.video_codecs = HashSet::from(["h264".into()]);
    device.audio_codecs = HashSet::from(["aac".into()]);
    device.hdr_formats.clear();
    device.supports_dolby_vision = false;
    device.max_video_bit_depth = 8;
    device.max_resolution = (
        device.max_resolution.0.min(MAX_WIDTH),
        device.max_resolution.1.min(MAX_HEIGHT),
    );
    device.max_audio_channels = device.max_audio_channels.min(AUDIO_MAX_CHANNELS);
    device.max_bitrate_bps = device.max_bitrate_bps.min(u64::from(VIDEO_MAX_BITRATE));
    let mut config = config.clone();
    config.default_video_codec = "h264".into();
    config.default_audio_codec = "aac".into();
    let mut encoding_device = device.clone();
    encoding_device.video_codecs.clear();
    let mut decision = decision_engine::decide(media, &encoding_device, network, &config);
    decision.video = VideoDecision::Transcode;
    decision.audio = AudioDecision::Transcode;
    decision.target_video_codec = Some("h264".into());
    decision.target_audio_codec = Some("aac".into());
    let plan = StreamPlan::compile(&decision, media, true).ok()?;
    if plan.video != StreamAction::Encode || plan.audio != StreamAction::Encode {
        return None;
    }
    let targets = plan
        .output_codecs(&decision, media, &device, "h264", "aac")
        .ok()?;
    if targets != (Some("h264".into()), Some("aac".into())) {
        return None;
    }
    let cap = decision.target_resolution.unwrap_or(media.video_resolution);
    let width = cap
        .0
        .min(device.max_resolution.0)
        .min(media.video_resolution.0)
        / 2
        * 2;
    let height = cap
        .1
        .min(device.max_resolution.1)
        .min(media.video_resolution.1)
        / 2
        * 2;
    let mut bitrate = decision
        .target_bitrate_bps?
        .min(device.max_bitrate_bps)
        .min(u64::from(VIDEO_MAX_BITRATE));
    if media.video_bitrate_bps > 0 {
        bitrate = bitrate.min(media.video_bitrate_bps);
    }
    let channels = media.audio_channels.min(device.max_audio_channels);
    let audio_bitrate = if media.audio_bitrate_bps > 0 {
        u32::try_from(media.audio_bitrate_bps.min(u64::from(AUDIO_MAX_BITRATE))).ok()?
    } else {
        AUDIO_MAX_BITRATE
    };
    let descriptor = DirectFallbackResponse {
        video: DirectFallbackVideo {
            content_type: direct::VIDEO_CONTENT_TYPE.into(),
            width,
            height,
            bitrate: u32::try_from(bitrate).ok()?,
            framerate: FRAME_RATE,
        },
        audio: DirectFallbackAudio {
            content_type: direct::AUDIO_CONTENT_TYPE.into(),
            channels: channels.to_string(),
            bitrate: audio_bitrate,
            samplerate: AUDIO_SAMPLE_RATE,
        },
    };
    direct::descriptor_valid(&descriptor).then_some(descriptor)
}

#[cfg(test)]
#[path = "direct_admission_tests.rs"]
mod tests;
