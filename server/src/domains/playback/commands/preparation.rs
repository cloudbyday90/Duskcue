// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database, types::Intent};
use crate::{
    domains::{
        household_security::types::Scope,
        playback::{self, planning::Prepared, types::PlaybackActor},
    },
    services::{
        decision_engine::NetworkConditions,
        playback_admission::{
            AdmissionError, AdmissionOwner, Request, Subject,
            vod::{self, Binding, Mode, Policy},
        },
        transcoding::plan::StreamAction,
        transcoding::{HwAccelMethod, OutputProfile, StartSessionParams},
    },
    state::AppState,
};
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

pub(super) struct PreparedCommand {
    pub request: Request,
    pub binding: Binding,
    pub plan: Prepared,
    pub identity: playback::direct::FileIdentity,
}

pub(super) async fn prepare(
    state: &AppState,
    owner: &Arc<AdmissionOwner>,
    scope: Scope,
    intent: Intent,
) -> Result<PreparedCommand, AdmissionError> {
    let config = state.runtime_config.load_full();
    let idle = config.auth.session_idle_timeout_hours;
    let (source, policy, revision, selection) = owner
        .sql(false, move |conn| {
            Box::pin(async move {
                let revision = database::context(conn, scope, idle).await?;
                let mut selection=intent.selection;
                let item=selection.media_item_id.ok_or(AdmissionError::Authority)?;
                let selected:Option<(Uuid,bool)>=sqlx::query_as("SELECT id,octet_length(file_path) BETWEEN 1 AND 4096 AND octet_length(additional_streams::text)<=65536 FROM media_files WHERE media_item_id=$1 AND ($2::uuid IS NULL OR id=$2) AND is_healthy ORDER BY file_size DESC,id LIMIT 1 FOR SHARE")
                    .bind(item).bind(selection.media_file_id).fetch_optional(&mut *conn).await?;
                let (file,true)=selected.ok_or(AdmissionError::Authority)? else {return Err(AdmissionError::Authority);};
                selection.media_file_id=Some(file);
                let all: bool =
                    sqlx::query_scalar("SELECT has_all_library_access FROM users WHERE id=$1")
                        .bind(scope.user)
                        .fetch_one(&mut *conn)
                        .await?;
                let source = playback::source_preparation::resolve_on_connection(
                    conn,
                    PlaybackActor {
                        user_id: scope.user,
                        profile_id: scope.profile,
                        has_all_library_access: all,
                    },
                    &selection,
                )
                .await
                .map_err(|_| AdmissionError::Authority)?;
                let policy = vod::policy(conn, scope.user).await?;
                Ok((source, policy, revision, selection))
            })
        })
        .await?;
    let actor = PlaybackActor {
        user_id: scope.user,
        profile_id: scope.profile,
        has_all_library_access: false,
    };
    let bandwidth = selection
        .max_streaming_bitrate
        .into_iter()
        .chain(policy.bandwidth.and_then(|v| u64::try_from(v).ok()))
        .min();
    let network = NetworkConditions {
        estimated_throughput_bps: bandwidth,
        network_tier: None,
    };
    let plan =
        playback::planning::prepare(source, actor, &selection, &config, scope.transport, network)
            .await
            .map_err(|_| AdmissionError::Authority)?;
    let identity = playback::direct::FileIdentity::read_owned(
        &state.transcode_manager.output,
        &plan.source.details,
    )
    .await
    .ok_or(AdmissionError::Authority)?;
    let source = serde_json::to_vec(&identity).map_err(|_| AdmissionError::Storage)?;
    let mode = match plan.decision.overall {
        crate::services::decision_engine::StreamDecision::DirectPlay => Mode::DirectPlay,
        crate::services::decision_engine::StreamDecision::DirectStream => Mode::DirectStream,
        crate::services::decision_engine::StreamDecision::Transcode => Mode::Transcode,
    };
    let binding = Binding {
        intent: intent.hash,
        policy: policy.hash()?,
        source: ring::digest::digest(&ring::digest::SHA256, &source)
            .as_ref()
            .try_into()
            .map_err(|_| AdmissionError::Storage)?,
        mode,
        encodes: plan.stream_plan.is_some_and(|plan| {
            plan.video == StreamAction::Encode || plan.audio == StreamAction::Encode
        }) || plan.decision.subtitle_burn_in_required,
        source_4k: plan.source.media.video_resolution.0 >= 3840
            || plan.source.media.video_resolution.1 >= 2160,
        ambient: plan
            .source
            .ambient_channel
            .as_ref()
            .map(|channel| (channel.id, channel.updated_at)),
    };
    validate(&policy, &plan, binding)?;
    let request = Request {
        id: intent.id,
        actor: scope,
        profile_revision: revision,
        subject: Subject::Vod {
            file: plan.source.details.id,
            item: plan.source.media_item_id,
        },
    };
    Ok(PreparedCommand {
        request,
        binding,
        plan,
        identity,
    })
}

fn validate(policy: &Policy, plan: &Prepared, binding: Binding) -> Result<(), AdmissionError> {
    if !policy.permits(binding) {
        return Err(AdmissionError::Policy);
    }
    if binding.encodes
        && let Some(limit) = policy.resolution.as_deref()
    {
        let cap = crate::services::decision_engine::parse_resolution_string(limit);
        let size = plan
            .decision
            .target_resolution
            .unwrap_or(plan.source.media.video_resolution);
        if size.0 > cap.0 || size.1 > cap.1 {
            return Err(AdmissionError::Policy);
        }
    }
    if let Some(limit) = policy.bandwidth {
        let video = plan
            .decision
            .target_bitrate_bps
            .unwrap_or(plan.source.media.video_bitrate_bps);
        let audio = plan
            .negotiated_audio_bitrate
            .map(u64::from)
            .unwrap_or(plan.source.media.audio_bitrate_bps);
        if video == 0
            || audio == 0
            || video
                .checked_add(audio)
                .is_none_or(|n| u64::try_from(limit).map_or(true, |limit| n > limit))
        {
            return Err(AdmissionError::Policy);
        }
    }
    Ok(())
}

pub(super) fn parameters(
    plan: &Prepared,
    user: Uuid,
) -> Result<StartSessionParams, AdmissionError> {
    Ok(StartSessionParams {
        media_file_id: plan.source.details.id,
        user_id: user,
        source_path: PathBuf::from(&plan.source.details.file_path),
        source_video_codec: plan.source.media.video_codec.clone(),
        source_video_resolution: plan.source.media.video_resolution,
        source_audio_codec: plan.source.media.audio_codec.clone(),
        source_video_stream_index: plan.source.tracks.video_stream_index,
        source_audio_stream_index: plan.source.tracks.audio_stream_index,
        stream_plan: plan.stream_plan.ok_or(AdmissionError::Authority)?,
        output_profile: plan.negotiated_hls.then_some(OutputProfile::SourceCopySdr),
        target_audio_channels: plan.decision.target_audio_channels,
        target_audio_bitrate: plan.negotiated_audio_bitrate,
        subtitle_stream_index: plan.source.tracks.subtitle_stream_index,
        subtitle_stream_ordinal: plan.source.tracks.subtitle_stream_ordinal,
        subtitle_burn_in: plan.decision.subtitle_burn_in_required,
        target_video_codec: plan.decision.target_video_codec.clone(),
        target_audio_codec: plan.decision.target_audio_codec.clone(),
        target_resolution: plan.decision.target_resolution,
        target_bitrate: plan
            .decision
            .target_bitrate_bps
            .map(u32::try_from)
            .transpose()
            .map_err(|_| AdmissionError::Policy)?,
        seek_position_ms: None,
        hw_accel_override: Some(HwAccelMethod::Software),
        recovery_attempts: 0,
    })
}
