// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::PathBuf;

use serde_json::{Value, json};
use sqlx::PgConnection;

use super::PlaybackError;
use super::direct::{self, PreparedFallback, ResolvedTracks};
use super::history::emit_play_event;
use super::media_source::{build_media_file_info, fetch_media_file_details_on_connection};
use super::recovery::{RecoveryContext, stored_recovery};
use super::types::RecoveryResponse;
use crate::domains::profiles::service::{
    assert_media_access_on_connection, load_profile_scope_on_connection,
};
use crate::services::transcoding::plan::{StreamAction, StreamPlan};
use crate::services::transcoding::recovery::{MAX_RECOVERY_ATTEMPTS, RecoveryBudget};
use crate::services::transcoding::{
    HwAccelMethod, OutputProfile, PendingTranscodeSession, StartSessionParams,
};

pub(super) async fn recover(
    connection: &mut PgConnection,
    context: RecoveryContext<'_>,
    metadata: &mut Value,
) -> Result<(Option<PendingTranscodeSession>, Option<RecoveryResponse>), PlaybackError> {
    if !direct::has_context(metadata) || !context.request.recovery_requested {
        return Ok((None, None));
    }
    let Some(revision) = direct::revision(metadata) else {
        return Ok((None, None));
    };
    if context.request.direct_source_revision != Some(revision) {
        return Ok((None, None));
    }
    let mut budget: RecoveryBudget = match metadata.get("recovery_budget") {
        Some(value) => serde_json::from_value(value.clone()).unwrap_or(RecoveryBudget {
            exhausted: true,
            ..Default::default()
        }),
        None => RecoveryBudget::default(),
    };
    if budget.exhausted {
        return Ok((None, stored_recovery(metadata)));
    }
    let scope = load_profile_scope_on_connection(
        connection,
        context.actor.user_id,
        context.actor.profile_id,
        context.actor.has_all_library_access,
    )
    .await
    .map_err(|_| PlaybackError::AccessDenied)?;
    assert_media_access_on_connection(connection, &scope, context.media_item_id)
        .await
        .map_err(|_| PlaybackError::AccessDenied)?;
    let mut status = RecoveryResponse {
        status: "exhausted",
        reason: "fallback_unavailable",
        attempts: budget.attempts,
        skipped_ms: 0,
        quality_reduced: false,
    };
    let candidate = direct::context(metadata).and_then(|context| context.candidate);
    let mut pending = None;
    if !context.request.direct_fallback_supported {
        status.reason = "output_unsupported";
        budget.exhausted = true;
    } else if budget.attempts >= MAX_RECOVERY_ATTEMPTS {
        status.reason = "budget_exhausted";
        budget.exhausted = true;
    } else if let Some(candidate) = candidate {
        match parameters(connection, &context, metadata, &candidate).await? {
            Some(mut params) => {
                budget.attempts += 1;
                status.attempts = budget.attempts;
                params.recovery_attempts = budget.attempts;
                match context
                    .manager
                    .start_session(params, context.data_dir)
                    .await
                {
                    Ok(replacement) => {
                        pending = Some(context.manager.pending_session(replacement.id));
                        status.status = "recovered";
                        status.reason = "native_decode";
                        status.quality_reduced = true;
                        metadata["transcode_session_id"] = json!(replacement.id);
                        metadata["direct_source_revision"] = Value::Null;
                        metadata["stream_offset_ms"] = json!(replacement.source_start_ms);
                        metadata["stream_decision"] = json!("transcode");
                        sqlx::query("UPDATE play_sessions SET stream_decision='transcode' WHERE id=$1 AND user_id=$2")
                            .bind(context.session_id).bind(context.actor.user_id).execute(&mut *connection).await?;
                    }
                    Err(_) => {
                        budget.exhausted = true;
                        status.reason = "replacement_failed";
                    }
                }
            }
            None => {
                budget.exhausted = true;
                status.reason = "source_changed";
            }
        }
    } else {
        budget.exhausted = true;
    }
    metadata["recovery_budget"] = json!(budget);
    metadata["recovery"] = json!(status);
    emit_play_event(connection, context.session_id, context.actor.user_id, "transcode_change", None,
        json!({"cause":"recovery","status":status.status,"reason":status.reason,"attempts":status.attempts,"skipped_ms":0,"quality_reduced":status.quality_reduced})).await?;
    Ok((pending, Some(status)))
}

pub(super) async fn parameters(
    connection: &mut PgConnection,
    context: &RecoveryContext<'_>,
    metadata: &Value,
    candidate: &PreparedFallback,
) -> Result<Option<StartSessionParams>, PlaybackError> {
    if !direct::descriptor_valid(&candidate.descriptor)
        || metadata
            .get("media_file_id")
            .and_then(Value::as_str)
            .and_then(|id| id.parse::<uuid::Uuid>().ok())
            != Some(candidate.identity.file_id)
    {
        return Ok(None);
    }
    let details = match fetch_media_file_details_on_connection(
        connection,
        candidate.identity.file_id,
        context.media_item_id,
    )
    .await
    {
        Ok(details) => details,
        Err(PlaybackError::FileNotFound) => return Ok(None),
        Err(error) => return Err(error),
    };
    if direct::FileIdentity::read(&details).await.as_ref() != Some(&candidate.identity) {
        return Ok(None);
    }
    let (media, selected) = match build_media_file_info(
        &details,
        Some(candidate.tracks.audio),
        candidate.tracks.subtitle,
    ) {
        Ok(source) => source,
        Err(PlaybackError::TrackUnavailable(_)) => return Ok(None),
        Err(error) => return Err(error),
    };
    if ResolvedTracks::from_selected(&selected).as_ref() != Some(&candidate.tracks)
        || !super::direct_admission::within_saved_limits(candidate, &media)
    {
        return Ok(None);
    }
    let position = metadata
        .get("current_position_ms")
        .and_then(Value::as_i64)
        .filter(|position| (0..=i64::from(i32::MAX)).contains(position));
    let Some(position) = position else {
        return Ok(None);
    };
    let descriptor = &candidate.descriptor;
    let params = StartSessionParams {
        media_file_id: details.id,
        user_id: context.actor.user_id,
        source_path: PathBuf::from(details.file_path),
        source_video_codec: media.video_codec,
        source_video_resolution: media.video_resolution,
        source_audio_codec: media.audio_codec,
        source_video_stream_index: Some(candidate.tracks.video),
        source_audio_stream_index: Some(candidate.tracks.audio),
        stream_plan: StreamPlan {
            video: StreamAction::Encode,
            audio: StreamAction::Encode,
        },
        target_audio_channels: descriptor.audio.channels.parse().ok(),
        target_audio_bitrate: Some(descriptor.audio.bitrate),
        subtitle_stream_index: None,
        subtitle_stream_ordinal: None,
        subtitle_burn_in: false,
        target_video_codec: Some("h264".into()),
        target_audio_codec: Some("aac".into()),
        target_resolution: Some((descriptor.video.width, descriptor.video.height)),
        target_bitrate: Some(descriptor.video.bitrate),
        seek_position_ms: Some(position),
        hw_accel_override: Some(HwAccelMethod::Software),
        recovery_attempts: 0,
        output_profile: Some(OutputProfile::CompatibleSdr),
    };
    Ok(OutputProfile::CompatibleSdr
        .validate(&params)
        .is_ok()
        .then_some(params))
}
