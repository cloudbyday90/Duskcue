// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    PlaybackError,
    source_preparation::ResolvedSource,
    types::{PlaybackActor, StartPlaybackRequest},
};
use crate::{
    extractors::CredentialTransport,
    services::{
        decision_engine::{self, NetworkConditions, PlaybackDecision, StreamDecision},
        transcoding::plan::{PlanError, StreamAction, StreamPlan, apply_delivery_requirements},
    },
    state::RuntimeConfig,
};
use uuid::Uuid;

#[derive(Clone)]
pub(super) struct Prepared {
    pub source: ResolvedSource,
    pub decision: PlaybackDecision,
    pub stream_plan: Option<StreamPlan>,
    pub negotiated_hls: bool,
    pub negotiated_audio_bitrate: Option<u32>,
    pub accepted_route: Option<super::source_context::AcceptedRoute>,
    pub direct_context: Option<super::direct::DirectContext>,
    pub direct_revision: Option<Uuid>,
}

pub(super) async fn prepare(
    source: ResolvedSource,
    actor: PlaybackActor,
    req: &StartPlaybackRequest,
    config: &RuntimeConfig,
    transport: CredentialTransport,
    network: NetworkConditions,
) -> Result<Prepared, PlaybackError> {
    let device_caps =
        super::service::build_device_capabilities(req.device_profile.as_ref(), config);
    let engine_config = super::service::build_decision_engine_config(
        config,
        req.quality_mode.as_deref(),
        req.max_streaming_bitrate,
    );
    let prepared = if req
        .source_capabilities
        .as_ref()
        .is_some_and(|report| report.version == 1)
    {
        Some(
            super::source_candidates::prepare(super::source_candidates::Context {
                source: &source,
                actor,
                request: req,
                device: &device_caps,
                network: &network,
                config: &engine_config,
                transport,
            })
            .await,
        )
    } else {
        None
    };
    let selected_candidate = prepared.as_ref().and_then(|prepared| {
        prepared
            .select(req.source_capabilities.as_ref())
            .filter(|candidate| prepared.accept(candidate, &source).is_some())
    });
    let accepted_route = prepared
        .as_ref()
        .zip(selected_candidate)
        .and_then(|(prepared, candidate)| prepared.accept(candidate, &source));
    let negotiated_audio_channels =
        selected_candidate.and_then(|candidate| candidate.audio_channels);
    let negotiated_audio_bitrate = selected_candidate.and_then(|candidate| candidate.audio_bitrate);
    let negotiated_hls = selected_candidate.is_some_and(|candidate| candidate.plan.is_some());
    let super::source_preparation::ResolvedSource {
        media_item_id,
        library_id,
        playback_mode,
        ambient_channel,
        details: media_file_details,
        media: media_info,
        tracks: selected_tracks,
    } = source;

    let mut decision = decision_engine::decide(&media_info, &device_caps, &network, &engine_config);
    if let Some(candidate) = selected_candidate {
        decision.video = crate::services::decision_engine::VideoDecision::DirectPlay;
        decision.audio = if candidate
            .plan
            .is_some_and(|plan| plan.audio == StreamAction::Encode)
        {
            crate::services::decision_engine::AudioDecision::Transcode
        } else {
            crate::services::decision_engine::AudioDecision::Passthrough
        };
        decision.overall = if candidate.plan.is_none() {
            StreamDecision::DirectPlay
        } else {
            StreamDecision::DirectStream
        };
        decision.requires_tone_mapping = false;
        decision.requires_dv_strip = false;
        decision.target_video_codec = None;
        decision.target_audio_codec = negotiated_audio_channels.map(|_| "aac".into());
        decision.target_audio_channels = negotiated_audio_channels;
        decision.target_resolution = Some(media_info.video_resolution);
        decision.target_bitrate_bps = None;
    }

    apply_delivery_requirements(
        &mut decision,
        req.force_transcode.unwrap_or(false),
        req.force_hls.unwrap_or(false),
        selected_tracks.subtitle_stream_index.is_some(),
    );

    let stream_plan = if decision.overall == StreamDecision::DirectPlay {
        None
    } else {
        let plan =
            StreamPlan::compile(&decision, &media_info, req.force_transcode.unwrap_or(false))
                .map_err(|error| match error {
                    PlanError::UnsupportedHdrConversion => PlaybackError::HdrConversionUnavailable,
                })?;
        let mut output_device = device_caps.clone();
        if negotiated_hls {
            output_device.audio_codecs.insert("aac".into());
        }
        let (video_codec, audio_codec) = plan
            .output_codecs(
                &decision,
                &media_info,
                &output_device,
                &config.transcoding.default_video_codec,
                &config.transcoding.default_audio_codec,
            )
            .map_err(|message| PlaybackError::FfmpegFailed(message.into()))?;
        decision.target_video_codec = video_codec;
        decision.target_audio_codec = audio_codec;
        decision.overall = if plan.video == StreamAction::Copy && plan.audio == StreamAction::Copy {
            StreamDecision::DirectStream
        } else {
            StreamDecision::Transcode
        };
        Some(plan)
    };

    let direct_context = if decision.overall == StreamDecision::DirectPlay
        && super::direct::supports_protocol(req.direct_recovery_protocol)
    {
        Some(if let Some(route) = accepted_route.as_ref() {
            super::direct::DirectContext {
                version: 1,
                candidate: route.fallback.clone(),
            }
        } else {
            super::direct_admission::prepare(
                &media_file_details,
                &media_info,
                &selected_tracks,
                &device_caps,
                &network,
                &engine_config,
                req.max_streaming_bitrate,
            )
            .await
        })
    } else {
        None
    };
    let direct_revision = direct_context.as_ref().map(|_| Uuid::now_v7());

    Ok(Prepared {
        source: ResolvedSource {
            media_item_id,
            library_id,
            playback_mode,
            ambient_channel,
            details: media_file_details,
            media: media_info,
            tracks: selected_tracks,
        },
        decision,
        stream_plan,
        negotiated_hls,
        negotiated_audio_bitrate,
        accepted_route,
        direct_context,
        direct_revision,
    })
}
