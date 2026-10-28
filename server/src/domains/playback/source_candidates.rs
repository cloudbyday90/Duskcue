// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::collections::HashSet;

use serde_json::json;

use super::direct::{FileIdentity, PreparedFallback};
use super::source_context::AcceptedRoute;
use super::source_preparation::ResolvedSource;
use super::types::*;
use crate::extractors::CredentialTransport;
use crate::services::decision_engine::{
    self, AudioDecision, DecisionEngineConfig, DeviceCapabilities, NetworkConditions, VideoDecision,
};
use crate::services::source_capabilities::{AudioDescriptor, PlaybackDescriptor};
use crate::services::transcoding::plan::{StreamAction, StreamPlan};

pub(super) struct Candidate {
    pub response: SourceCandidateResponse,
    pub plan: Option<StreamPlan>,
    pub audio_channels: Option<u32>,
    pub audio_bitrate: Option<u32>,
}

pub(super) struct Preparation {
    pub response: PreparePlaybackResponse,
    pub candidates: Vec<Candidate>,
    pub identity: Option<FileIdentity>,
    pub fallback: Option<PreparedFallback>,
}

#[derive(Clone, Copy)]
pub(super) struct Context<'a> {
    pub source: &'a ResolvedSource,
    pub actor: PlaybackActor,
    pub request: &'a StartPlaybackRequest,
    pub device: &'a DeviceCapabilities,
    pub network: &'a NetworkConditions,
    pub config: &'a DecisionEngineConfig,
    pub transport: CredentialTransport,
}

impl Preparation {
    pub(super) fn select(&self, report: Option<&SourceCapabilitiesRequest>) -> Option<&Candidate> {
        let report = report?;
        if report.version != 1
            || report.source_revision != self.response.source_revision
            || report.observations.len() > 3
        {
            return None;
        }
        let mut seen = HashSet::new();
        for observation in &report.observations {
            if !seen.insert(observation.candidate_id.as_str())
                || !self
                    .candidates
                    .iter()
                    .any(|candidate| candidate.response.id == observation.candidate_id)
            {
                return None;
            }
        }
        self.candidates.iter().find(|candidate| {
            report.observations.iter().any(|observation| {
                observation.candidate_id == candidate.response.id && observation.admitted()
            })
        })
    }

    pub(super) fn accept(
        &self,
        candidate: &Candidate,
        source: &ResolvedSource,
    ) -> Option<AcceptedRoute> {
        let route = AcceptedRoute {
            version: 1,
            candidate_id: candidate.response.id.into(),
            source_revision: self.response.source_revision.clone(),
            identity: self.identity.clone()?,
            duration_ms: duration_ms(source)?,
            fallback: self.fallback.clone(),
        };
        (serde_json::to_vec(&route).ok()?.len() <= 16_384).then_some(route)
    }
}

pub(super) async fn prepare(context: Context<'_>) -> Preparation {
    let Context {
        source,
        actor: _,
        request,
        device,
        network,
        config,
        transport: _,
    } = context;
    let identity = FileIdentity::read(&source.details).await;
    let descriptor = PlaybackDescriptor::from_metadata(&source.details.additional_streams);
    let mut fallback = None;
    let mut candidates = Vec::new();
    if let (Some(identity), Some(descriptor)) = (&identity, &descriptor)
        && super::source_freshness::matches(&source.details, &descriptor.scan).await
        && eligible(source, request, descriptor)
        && config.quality_mode == decision_engine::QualityMode::Auto
        && let Some(audio) = descriptor
            .audio
            .iter()
            .find(|audio| Some(audio.index) == source.tracks.audio_stream_index)
    {
        let media = fallback_media(source, descriptor, audio);
        fallback = super::direct_admission::prepare(
            &source.details,
            &media,
            &source.tracks,
            device,
            network,
            config,
            request.max_streaming_bitrate,
        )
        .await
        .candidate
        .filter(|fallback| fallback.identity == *identity);
        candidates = build(&context, descriptor, audio, fallback.as_ref());
    }
    let responses: Vec<_> = candidates
        .iter()
        .map(|candidate| candidate.response.clone())
        .collect();
    let revision = revision(&context, identity.as_ref(), &responses);
    Preparation {
        response: PreparePlaybackResponse {
            protocol: 1,
            source_revision: revision,
            candidates: responses,
        },
        candidates,
        identity,
        fallback,
    }
}

fn fallback_media(
    source: &ResolvedSource,
    descriptor: &PlaybackDescriptor,
    audio: &AudioDescriptor,
) -> decision_engine::MediaFileInfo {
    let mut media = source.media.clone();
    if let Some(verified) = descriptor.video.bitrate {
        media.video_bitrate_bps = if media.video_bitrate_bps > 0 {
            media.video_bitrate_bps.min(verified)
        } else {
            verified
        };
    }
    if let Some(verified) = audio.bitrate {
        media.audio_bitrate_bps = if media.audio_bitrate_bps > 0 {
            media.audio_bitrate_bps.min(verified)
        } else {
            verified
        };
    }
    media
}

fn eligible(
    source: &ResolvedSource,
    request: &StartPlaybackRequest,
    descriptor: &PlaybackDescriptor,
) -> bool {
    request
        .quality_mode
        .as_deref()
        .unwrap_or("auto")
        .eq_ignore_ascii_case("auto")
        && duration_ms(source).is_some()
        && !request.force_transcode.unwrap_or(false)
        && source.tracks.subtitle_stream_index.is_none()
        && source.media.video_dynamic_range == "sdr"
        && qualified_color(source)
        && !source
            .details
            .additional_streams
            .get("dolby_vision")
            .is_some_and(serde_json::Value::is_object)
        && descriptor.video.dynamic_range == "sdr"
        && source.tracks.video_stream_index == Some(descriptor.video.index)
        && matches!(descriptor.video.codec.as_str(), "h264" | "hevc" | "av1")
        && (2..=16_384).contains(&descriptor.video.width)
        && (2..=16_384).contains(&descriptor.video.height)
        && descriptor.video.frame_rate.is_finite()
        && (0.0..=240.0).contains(&descriptor.video.frame_rate)
        && descriptor.video.frame_rate > 0.0
        && descriptor
            .video
            .bitrate
            .is_some_and(|bitrate| (1..=1_000_000_000).contains(&bitrate))
        && descriptor.video.width == source.media.video_resolution.0
        && descriptor.video.height == source.media.video_resolution.1
        && u32::from(descriptor.video.bit_depth) == source.media.video_bit_depth
        && decision_engine::codec_supported(
            &source.media.video_codec,
            &HashSet::from([descriptor.video.codec.clone()]),
        )
}

pub(super) fn duration_ms(source: &ResolvedSource) -> Option<i64> {
    let seconds = source.details.additional_streams["format"]["duration_seconds"].as_f64()?;
    let milliseconds = (seconds * 1000.0).floor();
    (seconds.is_finite() && (1.0..=f64::from(i32::MAX)).contains(&milliseconds))
        .then_some(milliseconds as i64)
}

fn qualified_color(source: &ResolvedSource) -> bool {
    let video = &source.details.additional_streams["video"];
    ["color_primaries", "color_transfer", "color_space"]
        .into_iter()
        .all(|field| video.get(field).and_then(serde_json::Value::as_str) == Some("bt709"))
        && video.get("color_range").and_then(serde_json::Value::as_str) == Some("tv")
}

fn build(
    context: &Context<'_>,
    descriptor: &PlaybackDescriptor,
    audio: &AudioDescriptor,
    fallback: Option<&PreparedFallback>,
) -> Vec<Candidate> {
    let Context {
        source,
        request,
        device,
        network,
        config,
        transport,
        ..
    } = *context;
    let mut candidates = Vec::new();
    let audio_bitrate = audio
        .bitrate
        .filter(|bitrate| (1..=1_000_000_000).contains(bitrate));
    if audio.channels == 0
        || audio.sample_rate == 0
        || u32::from(audio.channels) != source.media.audio_channels
        || !decision_engine::codec_supported(
            &source.media.audio_codec,
            &HashSet::from([audio.codec.clone()]),
        )
    {
        return candidates;
    }
    let mut media = source.media.clone();
    media.video_bitrate_bps = descriptor.video.bitrate.unwrap_or(0);
    media.audio_bitrate_bps = audio_bitrate
        .unwrap_or(0)
        .max(fallback.map_or(0, |fallback| u64::from(fallback.descriptor.audio.bitrate)));
    let mut measured = device.clone();
    measured.video_codecs = HashSet::from([descriptor.video.codec.clone()]);
    measured.audio_codecs = HashSet::from([audio.codec.clone()]);
    measured.containers = HashSet::from([source.details.container_format.clone()]);
    measured.max_video_bit_depth = u32::from(descriptor.video.bit_depth);
    let policy = decision_engine::decide(&media, &measured, network, config);
    if !matches!(
        policy.video,
        VideoDecision::DirectPlay | VideoDecision::Remux
    ) || policy.requires_tone_mapping
        || policy.requires_dv_strip
        || policy.subtitle_burn_in_required
    {
        return candidates;
    }
    let audio_copy = policy.audio == AudioDecision::Passthrough;
    if transport == CredentialTransport::Cookie
        && !request.force_hls.unwrap_or(false)
        && request.audio_stream_index.is_none()
        && super::direct::supports_protocol(request.direct_recovery_protocol)
        && descriptor.moving_video_count == 1
        && audio_copy
        && native_default(source)
        && let Some(codec) = descriptor.video.file_codec.as_ref()
        && let Some(container) = source_container(&source.details.container_format)
        && let Some(audio_bitrate) = audio_bitrate
    {
        candidates.push(candidate(
            "native_file",
            "file",
            descriptor,
            format!("video/{container}; codecs=\"{codec}\""),
            audio_configuration(
                container,
                &audio.file_codec,
                u32::from(audio.channels),
                audio_bitrate,
                audio.sample_rate,
            ),
            None,
        ));
    }
    if let Some(fallback) = fallback {
        if audio.codec == "aac"
            && audio_copy
            && let Some(codec) = audio.copy_codec.as_ref()
            && let Some(audio_bitrate) = audio_bitrate
        {
            candidates.push(candidate(
                "hls_video_copy_audio_copy",
                "hls",
                descriptor,
                format!("video/mp4; codecs=\"{}\"", descriptor.video.copy_codec),
                audio_configuration(
                    "mp4",
                    codec,
                    u32::from(audio.channels),
                    audio_bitrate,
                    audio.sample_rate,
                ),
                Some(StreamPlan {
                    video: StreamAction::Copy,
                    audio: StreamAction::Copy,
                }),
            ));
        }
        if let Ok(channels) = fallback.descriptor.audio.channels.parse::<u32>() {
            candidates.push(candidate(
                "hls_video_copy_aac",
                "hls",
                descriptor,
                format!("video/mp4; codecs=\"{}\"", descriptor.video.copy_codec),
                audio_configuration(
                    "mp4",
                    "mp4a.40.2",
                    channels,
                    u64::from(fallback.descriptor.audio.bitrate),
                    48_000,
                ),
                Some(StreamPlan {
                    video: StreamAction::Copy,
                    audio: StreamAction::Encode,
                }),
            ));
        }
    }
    candidates
}

fn candidate(
    id: &'static str,
    delivery: &'static str,
    descriptor: &PlaybackDescriptor,
    video_content_type: String,
    audio: SourceAudioConfigurationResponse,
    plan: Option<StreamPlan>,
) -> Candidate {
    let audio_encode = plan.is_some_and(|plan| plan.audio == StreamAction::Encode);
    let audio_channels = audio.channels.parse().ok();
    let audio_bitrate = u32::try_from(audio.bitrate).ok();
    Candidate {
        response: SourceCandidateResponse {
            id,
            delivery,
            configuration: SourceConfigurationResponse {
                media_type: if delivery == "file" {
                    "file"
                } else {
                    "media-source"
                },
                video: SourceVideoConfigurationResponse {
                    content_type: video_content_type,
                    width: descriptor.video.width,
                    height: descriptor.video.height,
                    bitrate: descriptor.video.bitrate.unwrap_or(0),
                    framerate: descriptor.video.frame_rate,
                },
                audio,
            },
        },
        plan,
        audio_channels: audio_encode.then_some(audio_channels).flatten(),
        audio_bitrate: audio_encode.then_some(audio_bitrate).flatten(),
    }
}

fn audio_configuration(
    container: &str,
    codec: &str,
    channels: u32,
    bitrate: u64,
    samplerate: u32,
) -> SourceAudioConfigurationResponse {
    SourceAudioConfigurationResponse {
        content_type: format!("audio/{container}; codecs=\"{codec}\""),
        channels: channels.to_string(),
        bitrate,
        samplerate,
    }
}

fn source_container(container: &str) -> Option<&'static str> {
    match container {
        "mp4" => Some("mp4"),
        "mkv" => Some("x-matroska"),
        "webm" => Some("webm"),
        _ => None,
    }
}

fn native_default(source: &ResolvedSource) -> bool {
    let streams = &source.details.additional_streams;
    let Some(audio) = streams.get("audio").and_then(serde_json::Value::as_array) else {
        return false;
    };
    let defaults = audio
        .iter()
        .filter(|stream| {
            stream["disposition"]["default"].as_i64() == Some(1)
                || stream["is_default"].as_bool() == Some(true)
        })
        .count();
    (defaults == 1 || audio.len() == 1)
        && super::track_selection::default_audio_stream_index(streams)
            == source.tracks.audio_stream_index
}

fn revision(
    context: &Context<'_>,
    identity: Option<&FileIdentity>,
    candidates: &[SourceCandidateResponse],
) -> String {
    let Context {
        source,
        actor,
        request,
        device,
        network,
        config,
        transport,
    } = *context;
    let value = json!({
        "version":1,"user":actor.user_id,"profile":actor.profile_id,"transport":transport,
        "item":source.media_item_id,"identity":identity,"video":source.tracks.video_stream_index,
        "duration_seconds":source.details.additional_streams["format"]["duration_seconds"],
        "duration_ms":duration_ms(source),
        "audio":source.tracks.audio_stream_index,"subtitle":source.tracks.subtitle_stream_index,
        "subtitle_ordinal":source.tracks.subtitle_stream_ordinal,"playback_mode":source.playback_mode,
        "ambient":request.ambient_channel_id,"ambient_updated":request.ambient_channel_updated_at,
        "force_video":request.force_transcode.unwrap_or(false),"force_hls":request.force_hls.unwrap_or(false),
        "direct_protocol":request.direct_recovery_protocol,"requested_bitrate":request.max_streaming_bitrate,
        "quality":format!("{:?}",config.quality_mode),"manual_resolution":config.manual_max_resolution,
        "resolution":device.max_resolution,"channels":device.max_audio_channels,"bitrate":device.max_bitrate_bps,
        "throughput":network.estimated_throughput_bps,"safety":config.throughput_safety_factor,
        "audio_passthrough":config.audio_passthrough_enabled,"candidates":candidates,
    });
    let encoded = serde_json::to_vec(&value).unwrap_or_default();
    ring::digest::digest(&ring::digest::SHA256, &encoded)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
#[path = "source_candidates_tests.rs"]
mod tests;
