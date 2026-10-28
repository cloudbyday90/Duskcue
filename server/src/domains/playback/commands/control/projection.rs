// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::{error::CommandError, output::Output, types::PROTOCOL};
use super::{
    intent,
    types::{ConfigurationResponse, OperationResponse, RecoveryOffer},
};
use crate::{
    services::playback_admission::vod::revisions::{ControlRow, OperationRow, OutputRow},
    state::AppState,
};
use std::sync::{Arc, atomic::Ordering};
use uuid::Uuid;

pub(super) fn offer(output: &Output) -> Result<Option<RecoveryOffer>, CommandError> {
    if output
        .failure
        .lock()
        .map_err(|_| CommandError::Unavailable)?
        .is_some()
    {
        return Ok(None);
    }
    let plan = &output.prepared.plan;
    let (kind, candidate) = if output.generation.is_none() {
        (
            "direct",
            plan.direct_context
                .as_ref()
                .and_then(|c| c.candidate.as_ref()),
        )
    } else {
        (
            "copy",
            plan.accepted_route
                .as_ref()
                .filter(|r| r.candidate_id.starts_with("hls_"))
                .and_then(|r| r.fallback.as_ref()),
        )
    };
    let Some(candidate) = candidate else {
        return Ok(None);
    };
    if candidate.identity != output.prepared.identity
        || !crate::domains::playback::direct::descriptor_valid(&candidate.descriptor)
    {
        return Ok(None);
    }
    if kind == "copy"
        && !output.parameters.as_ref().is_some_and(|p| {
            p.stream_plan.video == crate::services::transcoding::plan::StreamAction::Copy
                && p.output_profile
                    == Some(crate::services::transcoding::OutputProfile::SourceCopySdr)
        })
    {
        return Ok(None);
    }
    let fingerprint = intent::digest(&(PROTOCOL, output.id, kind, &candidate.descriptor))?;
    Ok(Some(RecoveryOffer {
        output_revision: output.id,
        kind,
        descriptor: candidate.descriptor.clone(),
        fingerprint,
    }))
}

pub(super) fn configuration(
    state: &AppState,
    viewer: Uuid,
    control: &ControlRow,
    row: &OutputRow,
    local: &Arc<Output>,
) -> Result<(ConfigurationResponse, String), CommandError> {
    if control.current_output != Some(row.id) || row.id != local.id {
        return Err(CommandError::Unavailable);
    }
    let mut phase = control.state.clone();
    if phase == "ready"
        && (!local.published.load(Ordering::Acquire)
            || !local.group.active()
            || local
                .generation
                .is_some_and(|id| !state.transcode_manager.output_usable(id)))
    {
        phase = "unavailable".into();
    }
    let value = ConfigurationResponse {
        protocol: PROTOCOL,
        request_id: viewer,
        revision: control.revision.to_string(),
        state: phase,
        output_revision: row.id,
        pending_operation: control.pending_operation,
        configuration: row.configuration.clone(),
        playback_state: control.playback_state.clone(),
        recovery_budget: control.recovery_budget.clone(),
        recovery_offer: offer(local)?,
        worker_recovery: control.checkpoint_sequence > 0
            && control.checkpoint_output == Some(local.id)
            && control.checkpoint_position.is_some_and(|position| {
                super::recovery::prepare(
                    state,
                    local,
                    &control.recovery_budget,
                    &super::types::Evidence::Worker,
                    position,
                )
                .is_ok()
            }),
    };
    let etag = format!("\"vod-config-{}\"", intent::digest(&value)?);
    Ok((value, etag))
}

pub(super) fn operation(viewer: Uuid, row: &OperationRow, cleanup: bool) -> OperationResponse {
    OperationResponse {
        protocol: PROTOCOL,
        request_id: viewer,
        operation_id: row.id,
        status: row.state.clone(),
        status_url: format!("/api/v1/playback-commands/{viewer}/operations/{}", row.id),
        output_revision: (!cleanup).then_some(row.candidate_output),
        result: if cleanup { None } else { row.result.clone() },
        cleanup_complete: row.cleanup_at.is_some(),
    }
}
