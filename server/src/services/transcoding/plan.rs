// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::collections::HashSet;

use crate::services::decision_engine::{
    AudioDecision, DeviceCapabilities, MediaFileInfo, PlaybackDecision, StreamDecision,
    SubtitleDecision, VideoDecision, codec_supported,
};

const COPY_VIDEO: &[&str] = &["h264", "hevc", "av1", "vp9"];
const COPY_AUDIO: &[&str] = &["aac", "ac3", "eac3", "mp3", "flac", "opus"];
const ENCODE_VIDEO: &[&str] = &["h264", "hevc"];
const ENCODE_AUDIO: &[&str] = &["aac", "ac3", "eac3", "opus"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamAction {
    Copy,
    Encode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamPlan {
    pub video: StreamAction,
    pub audio: StreamAction,
}

pub fn apply_delivery_requirements(
    decision: &mut PlaybackDecision,
    force_video: bool,
    force_hls: bool,
    subtitle_selected: bool,
) {
    if force_video {
        decision.overall = StreamDecision::Transcode;
    } else if force_hls && decision.overall == StreamDecision::DirectPlay {
        decision.overall = StreamDecision::DirectStream;
    }
    if subtitle_selected && decision.overall != StreamDecision::DirectPlay {
        decision.overall = StreamDecision::Transcode;
        decision.subtitle = SubtitleDecision::BurnIn;
        decision.subtitle_burn_in_required = true;
    }
}

impl StreamPlan {
    pub fn compile(
        decision: &PlaybackDecision,
        media: &MediaFileInfo,
        force_video: bool,
    ) -> Result<Self, &'static str> {
        if decision.requires_tone_mapping || decision.requires_dv_strip {
            return Err("required HDR transformation is not available in this playback pipeline");
        }
        let copy_video = !force_video
            && !decision.subtitle_burn_in_required
            && matches!(
                decision.video,
                VideoDecision::DirectPlay | VideoDecision::Remux
            )
            && supported_by(&media.video_codec, COPY_VIDEO);
        let copy_audio = decision.audio == AudioDecision::Passthrough
            && supported_by(&media.audio_codec, COPY_AUDIO);
        if !copy_video && known_hdr(media) {
            return Err("HDR video encoding is not available in this playback pipeline");
        }
        Ok(Self {
            video: if copy_video {
                StreamAction::Copy
            } else {
                StreamAction::Encode
            },
            audio: if copy_audio {
                StreamAction::Copy
            } else {
                StreamAction::Encode
            },
        })
    }

    pub fn output_codecs(
        self,
        decision: &PlaybackDecision,
        media: &MediaFileInfo,
        device: &DeviceCapabilities,
        default_video: &str,
        default_audio: &str,
    ) -> Result<(Option<String>, Option<String>), &'static str> {
        if self.video == StreamAction::Encode && known_hdr(media) {
            return Err("HDR video encoding is not available in this playback pipeline");
        }
        let video = output_codec(
            self.video,
            decision.target_video_codec.as_deref(),
            default_video,
            ENCODE_VIDEO,
            &device.video_codecs,
        )
        .ok_or("no compatible implemented video output codec is available")?;
        let audio = output_codec(
            self.audio,
            decision.target_audio_codec.as_deref(),
            default_audio,
            ENCODE_AUDIO,
            &device.audio_codecs,
        )
        .ok_or("no compatible implemented audio output codec is available")?;
        Ok((video, audio))
    }

    pub fn codec_args(
        self,
        video_encode_args: Vec<String>,
        audio_encode_args: Vec<String>,
    ) -> Vec<String> {
        let mut args = match self.video {
            StreamAction::Copy => vec!["-c:v".into(), "copy".into()],
            StreamAction::Encode => video_encode_args,
        };
        args.extend(match self.audio {
            StreamAction::Copy => vec!["-c:a".into(), "copy".into()],
            StreamAction::Encode => audio_encode_args,
        });
        args
    }
}

fn supported_by(codec: &str, formats: &[&str]) -> bool {
    codec_supported(
        codec,
        &formats.iter().map(|format| (*format).to_owned()).collect(),
    )
}

fn known_hdr(media: &MediaFileInfo) -> bool {
    let range = media.video_dynamic_range.trim().to_ascii_lowercase();
    matches!(range.as_str(), "hdr10" | "hdr10_plus" | "hdr10+" | "hlg")
        || range.starts_with("dolby_vision")
        || media
            .additional_streams
            .as_ref()
            .and_then(|streams| streams.get("dolby_vision"))
            .is_some_and(serde_json::Value::is_object)
}

fn output_codec(
    action: StreamAction,
    preferred: Option<&str>,
    default: &str,
    implemented: &[&str],
    supported: &HashSet<String>,
) -> Option<Option<String>> {
    if action == StreamAction::Copy {
        return Some(None);
    }
    for preference in preferred.into_iter().chain(std::iter::once(default)) {
        let aliases = HashSet::from([preference.to_owned()]);
        if let Some(codec) = implemented
            .iter()
            .copied()
            .find(|codec| codec_supported(codec, &aliases) && codec_supported(codec, supported))
        {
            return Some(Some(codec.to_owned()));
        }
    }
    implemented
        .iter()
        .copied()
        .find(|codec| codec_supported(codec, supported))
        .map(|codec| Some(codec.to_owned()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn media() -> MediaFileInfo {
        MediaFileInfo {
            container_format: "mkv".into(),
            video_codec: "h264".into(),
            video_profile: Some("High".into()),
            video_level: Some(4.1),
            video_bit_depth: 8,
            video_resolution: (1920, 1080),
            video_bitrate_bps: 8_000_000,
            video_dynamic_range: "sdr".into(),
            video_frame_rate: 24.0,
            audio_codec: "aac".into(),
            audio_channels: 2,
            audio_bitrate_bps: 192_000,
            audio_language: None,
            has_embedded_subtitles: false,
            subtitle_format: None,
            additional_streams: None,
        }
    }

    fn device(video: &[&str], audio: &[&str]) -> DeviceCapabilities {
        DeviceCapabilities {
            video_codecs: video.iter().map(|codec| (*codec).into()).collect(),
            audio_codecs: audio.iter().map(|codec| (*codec).into()).collect(),
            containers: HashSet::from(["mp4".into()]),
            subtitle_formats: HashSet::new(),
            max_resolution: (3840, 2160),
            max_audio_channels: 2,
            hdr_formats: HashSet::new(),
            max_bitrate_bps: 20_000_000,
            supports_dolby_vision: false,
            allow_client_side_dv_fallback: false,
            max_video_bit_depth: 8,
        }
    }

    fn decision() -> PlaybackDecision {
        PlaybackDecision {
            overall: StreamDecision::DirectStream,
            video: VideoDecision::Remux,
            audio: AudioDecision::Passthrough,
            subtitle: SubtitleDecision::Passthrough,
            target_video_codec: None,
            target_audio_codec: None,
            target_resolution: None,
            target_bitrate_bps: None,
            target_audio_channels: None,
            requires_tone_mapping: false,
            requires_dv_strip: false,
            subtitle_burn_in_required: false,
            decision_reasons: Vec::new(),
        }
    }

    #[test]
    fn forced_delivery_keeps_selected_captions_in_the_executable_video_plan() {
        for (force_video, force_hls) in [(true, false), (false, true), (true, true)] {
            let mut delivery = decision();
            delivery.overall = StreamDecision::DirectPlay;
            delivery.video = VideoDecision::DirectPlay;
            apply_delivery_requirements(&mut delivery, force_video, force_hls, true);
            assert_eq!(delivery.overall, StreamDecision::Transcode);
            assert_eq!(delivery.subtitle, SubtitleDecision::BurnIn);
            assert!(delivery.subtitle_burn_in_required);
            assert_eq!(
                StreamPlan::compile(&delivery, &media(), force_video)
                    .unwrap()
                    .video,
                StreamAction::Encode
            );
        }
        let mut direct = decision();
        direct.overall = StreamDecision::DirectPlay;
        direct.video = VideoDecision::DirectPlay;
        apply_delivery_requirements(&mut direct, false, false, true);
        assert_eq!(direct.overall, StreamDecision::DirectPlay);
        assert_eq!(direct.subtitle, SubtitleDecision::Passthrough);
        assert!(!direct.subtitle_burn_in_required);
    }

    #[test]
    fn forced_and_quality_triggered_hdr_encoding_cannot_enter_the_sdr_pipeline() {
        for range in ["hdr10", "hdr10_plus", "hlg", "dolby_vision_p8"] {
            let mut source = media();
            source.video_dynamic_range = range.into();
            assert!(StreamPlan::compile(&decision(), &source, true).is_err());
            let mut conversion = decision();
            conversion.video = VideoDecision::Transcode;
            assert!(StreamPlan::compile(&conversion, &source, false).is_err());
            conversion.video = VideoDecision::Remux;
            conversion.subtitle_burn_in_required = true;
            assert!(StreamPlan::compile(&conversion, &source, false).is_err());
        }
        let mut source = media();
        source.additional_streams = Some(json!({"dolby_vision":{"profile":8}}));
        assert!(StreamPlan::compile(&decision(), &source, true).is_err());
    }

    #[test]
    fn audio_only_conversion_preserves_hdr_video_and_its_copy_action() {
        let mut source = media();
        source.video_codec = "hvc1".into();
        source.video_dynamic_range = "hdr10".into();
        source.audio_codec = "truehd".into();
        let mut conversion = decision();
        conversion.audio = AudioDecision::Transcode;
        conversion.target_audio_codec = Some("aac".into());
        let plan = StreamPlan::compile(&conversion, &source, false).unwrap();
        assert_eq!(
            plan,
            StreamPlan {
                video: StreamAction::Copy,
                audio: StreamAction::Encode
            }
        );
        assert_eq!(
            plan.output_codecs(
                &conversion,
                &source,
                &device(&["hevc"], &["aac"]),
                "h264",
                "aac"
            ),
            Ok((None, Some("aac".into())))
        );
        assert_eq!(
            plan.codec_args(
                vec!["unused-video-encoder".into()],
                vec!["-c:a".into(), "aac".into()]
            ),
            ["-c:v", "copy", "-c:a", "aac"]
        );
    }

    #[test]
    fn unsupported_defaults_fall_back_to_implemented_device_supported_targets() {
        let plan = StreamPlan {
            video: StreamAction::Encode,
            audio: StreamAction::Encode,
        };
        let mut conversion = decision();
        conversion.target_video_codec = Some("hevc".into());
        conversion.target_audio_codec = Some("eac3".into());
        assert_eq!(
            plan.output_codecs(
                &conversion,
                &media(),
                &device(&["avc1"], &["aac"]),
                "hevc",
                "flac"
            ),
            Ok((Some("h264".into()), Some("aac".into())))
        );
        assert_eq!(
            plan.output_codecs(
                &decision(),
                &media(),
                &device(&["h265"], &["opus"]),
                "vp9",
                "truehd"
            ),
            Ok((Some("hevc".into()), Some("opus".into())))
        );
    }

    #[test]
    fn compatible_alias_preferences_return_the_actual_canonical_encoder_targets() {
        let plan = StreamPlan {
            video: StreamAction::Encode,
            audio: StreamAction::Encode,
        };
        let mut conversion = decision();
        conversion.target_video_codec = Some("h265".into());
        conversion.target_audio_codec = Some("ac-3".into());
        assert_eq!(
            plan.output_codecs(
                &conversion,
                &media(),
                &device(&["hvc1"], &["ac3"]),
                "h264",
                "aac"
            ),
            Ok((Some("hevc".into()), Some("ac3".into())))
        );
    }

    #[test]
    fn absent_implemented_client_output_support_fails_instead_of_inventing_a_codec() {
        let plan = StreamPlan {
            video: StreamAction::Encode,
            audio: StreamAction::Encode,
        };
        assert_eq!(
            plan.output_codecs(
                &decision(),
                &media(),
                &device(&["av1"], &["aac"]),
                "h264",
                "aac"
            ),
            Err("no compatible implemented video output codec is available")
        );
        let audio_only = StreamPlan {
            video: StreamAction::Copy,
            audio: StreamAction::Encode,
        };
        assert_eq!(
            audio_only.output_codecs(
                &decision(),
                &media(),
                &device(&["h264"], &["truehd"]),
                "h264",
                "aac"
            ),
            Err("no compatible implemented audio output codec is available")
        );
        assert_eq!(
            StreamPlan {
                video: StreamAction::Copy,
                audio: StreamAction::Copy
            }
            .output_codecs(
                &decision(),
                &media(),
                &device(&[], &[]),
                "unsupported",
                "unsupported"
            ),
            Ok((None, None))
        );
    }
}
