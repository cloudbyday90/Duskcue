// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::{error::CommandError, output::Output, preparation::PreparedCommand};
use super::{projection, types::Evidence};
use crate::{
    services::{
        playback_admission::vod::Mode,
        transcoding::{
            FailureClass, HwAccelMethod, OutputProfile, StartSessionParams,
            plan::{StreamAction, StreamPlan},
            recovery::RecoveryBudget,
        },
    },
    state::AppState,
};
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) struct Recovery {
    pub prepared: Arc<PreparedCommand>,
    pub parameters: StartSessionParams,
    pub position: i32,
    pub budget: Value,
    pub result: Value,
}

pub(super) fn prepare(
    state: &AppState,
    current: &Arc<Output>,
    saved: &Value,
    evidence: &Evidence,
    position: i32,
) -> Result<Recovery, CommandError> {
    current.snapshot(state)?;
    let mut budget: RecoveryBudget =
        serde_json::from_value(saved.clone()).map_err(|_| CommandError::Forbidden)?;
    let (prepared, mut parameters, at, reason, skipped, reduced) = match evidence {
        Evidence::Worker => {
            let class = current
                .failure
                .lock()
                .map_err(|_| CommandError::Unavailable)?
                .ok_or(CommandError::Conflict)?;
            let session = current
                .session
                .lock()
                .map_err(|_| CommandError::Unavailable)?
                .clone()
                .ok_or(CommandError::Conflict)?;
            if matches!(class, FailureClass::Hardware)
                && session.hw_accel == HwAccelMethod::Software
            {
                return Err(CommandError::Conflict);
            }
            let action = budget
                .reserve(class, &session, i64::from(position))
                .ok_or(CommandError::Conflict)?;
            let mut parameters = session.restart_params(action.position_ms);
            if action.software {
                parameters.hw_accel_override = Some(HwAccelMethod::Software);
            }
            (
                current.prepared.clone(),
                parameters,
                i32::try_from(action.position_ms).map_err(|_| CommandError::Invalid)?,
                match class {
                    FailureClass::Stalled => "stalled",
                    FailureClass::Decode => "decode",
                    FailureClass::Hardware => "hardware",
                    _ => return Err(CommandError::Conflict),
                },
                action.skipped_ms,
                action.quality_reduced,
            )
        }
        Evidence::NativeDecode {
            offer, supported, ..
        }
        | Evidence::HlsMedia {
            offer, supported, ..
        } => {
            let offered = projection::offer(current)?.ok_or(CommandError::Conflict)?;
            let smooth = match evidence {
                Evidence::NativeDecode { smooth, .. } => *smooth == Some(true),
                Evidence::HlsMedia { smooth, .. } => *smooth,
                _ => false,
            };
            if !*supported
                || offered.fingerprint != *offer
                || (offered.kind == "copy" && !smooth)
                || (offered.kind == "direct" && matches!(evidence, Evidence::HlsMedia { .. }))
                || budget.exhausted
                || budget.attempts >= 2
            {
                return Err(CommandError::Conflict);
            }
            let candidate = if offered.kind == "direct" {
                current
                    .prepared
                    .plan
                    .direct_context
                    .as_ref()
                    .and_then(|c| c.candidate.as_ref())
            } else {
                current
                    .prepared
                    .plan
                    .accepted_route
                    .as_ref()
                    .and_then(|c| c.fallback.as_ref())
            }
            .ok_or(CommandError::Conflict)?;
            if candidate.tracks.subtitle.is_some()
                || crate::domains::playback::direct::ResolvedTracks::from_selected(
                    &current.prepared.plan.source.tracks,
                )
                .as_ref()
                    != Some(&candidate.tracks)
            {
                return Err(CommandError::Conflict);
            }
            if !crate::domains::playback::direct_admission::within_saved_limits(
                candidate,
                &current.prepared.plan.source.media,
            ) {
                return Err(CommandError::Forbidden);
            }
            let mut prepared = current.prepared.as_ref().clone();
            prepared.binding.mode = Mode::Transcode;
            prepared.binding.encodes = true;
            prepared.plan.stream_plan = Some(StreamPlan {
                video: StreamAction::Encode,
                audio: StreamAction::Encode,
            });
            prepared.plan.negotiated_hls = false;
            prepared.plan.direct_context = None;
            prepared.plan.accepted_route = None;
            prepared.plan.direct_revision = None;
            let descriptor = &candidate.descriptor;
            let parameters = StartSessionParams {
                media_file_id: candidate.identity.file_id,
                user_id: prepared.request.actor.user,
                source_path: candidate.identity.path.clone().into(),
                source_video_codec: prepared.plan.source.media.video_codec.clone(),
                source_video_resolution: prepared.plan.source.media.video_resolution,
                source_audio_codec: prepared.plan.source.media.audio_codec.clone(),
                source_video_stream_index: Some(candidate.tracks.video),
                source_audio_stream_index: Some(candidate.tracks.audio),
                stream_plan: StreamPlan {
                    video: StreamAction::Encode,
                    audio: StreamAction::Encode,
                },
                output_profile: Some(OutputProfile::CompatibleSdr),
                target_audio_channels: descriptor.audio.channels.parse().ok(),
                target_audio_bitrate: Some(descriptor.audio.bitrate),
                subtitle_stream_index: None,
                subtitle_stream_ordinal: None,
                subtitle_burn_in: false,
                target_video_codec: Some("h264".into()),
                target_audio_codec: Some("aac".into()),
                target_resolution: Some((descriptor.video.width, descriptor.video.height)),
                target_bitrate: Some(descriptor.video.bitrate),
                seek_position_ms: Some(i64::from(position)),
                hw_accel_override: Some(HwAccelMethod::Software),
                recovery_attempts: 0,
            };
            OutputProfile::CompatibleSdr
                .validate(&parameters)
                .map_err(|_| CommandError::Conflict)?;
            budget.attempts += 1;
            (
                Arc::new(prepared),
                parameters,
                position,
                if offered.kind == "direct" {
                    "native_decode"
                } else {
                    "copy_decode"
                },
                0,
                true,
            )
        }
    };
    parameters.recovery_attempts = budget.attempts;
    budget.exhausted |= budget.attempts >= 2;
    Ok(Recovery {
        prepared,
        parameters,
        position: at,
        budget: serde_json::to_value(&budget).map_err(|_| CommandError::Unavailable)?,
        result: json!({"status":"recovered","attempts":budget.attempts,"reason":reason,"skipped_ms":skipped,"quality_reduced":reduced}),
    })
}
