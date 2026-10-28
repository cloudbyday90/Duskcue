// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::{
    error::CommandError,
    types::{self, PROTOCOL, StartRequest},
};
use super::types::{Action, ChangeRequest, Evidence};
use serde_json::json;
use uuid::Uuid;

pub(super) struct Intent {
    pub body: ChangeRequest,
    pub condition: String,
    pub hash: [u8; 32],
    pub selection: Option<types::Intent>,
}

pub(super) fn digest(value: &impl serde::Serialize) -> Result<String, CommandError> {
    let bytes = serde_json::to_vec(value).map_err(|_| CommandError::Invalid)?;
    if bytes.len() > 32768 {
        return Err(CommandError::TooLarge);
    }
    Ok(ring::digest::digest(&ring::digest::SHA256, &bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

pub(super) fn normalize(
    viewer: Uuid,
    body: ChangeRequest,
    condition: String,
) -> Result<Intent, CommandError> {
    if body.protocol != PROTOCOL
        || body.operation_id.get_version_num() != 7
        || body.expected_output.get_version_num() != 7
    {
        return Err(CommandError::Invalid);
    }
    let (action, selection) = match &body.action {
        Action::Seek { position_ms } if *position_ms >= 0 => {
            (json!({"kind":"seek","position_ms":position_ms}), None)
        }
        Action::Reconfigure {
            position_ms,
            selection,
        } if *position_ms >= 0 => {
            let selection = StartRequest {
                protocol: PROTOCOL.into(),
                request_id: viewer,
                selection: selection.clone(),
            }
            .intent()?;
            (
                json!({"kind":"reconfigure","position_ms":position_ms,"selection_hash":selection.hash}),
                Some(selection),
            )
        }
        Action::Recover {
            checkpoint_sequence,
            evidence,
        } if (1..=9007199254740991).contains(checkpoint_sequence) => {
            let evidence = match evidence {
                Evidence::Worker => json!({"kind":"worker"}),
                Evidence::NativeDecode {
                    offer,
                    supported,
                    smooth,
                } => {
                    check_offer(offer)?;
                    json!({"kind":"native_decode","offer":offer,"supported":supported,"smooth":smooth})
                }
                Evidence::HlsMedia {
                    offer,
                    supported,
                    smooth,
                } => {
                    check_offer(offer)?;
                    json!({"kind":"hls_media","offer":offer,"supported":supported,"smooth":smooth})
                }
            };
            (
                json!({"kind":"recover","checkpoint_sequence":checkpoint_sequence,"evidence":evidence}),
                None,
            )
        }
        _ => return Err(CommandError::Invalid),
    };
    let canonical = types::canonical(
        json!({"protocol":PROTOCOL,"viewer":viewer,"expected_output":body.expected_output,"condition":condition,"action":action}),
        0,
        &mut 0,
    )?;
    let bytes = serde_json::to_vec(&canonical).map_err(|_| CommandError::Invalid)?;
    let hash = ring::digest::digest(&ring::digest::SHA256, &bytes)
        .as_ref()
        .try_into()
        .map_err(|_| CommandError::Invalid)?;
    Ok(Intent {
        body,
        condition,
        hash,
        selection,
    })
}

fn check_offer(value: &str) -> Result<(), CommandError> {
    if !crate::domains::playback::source_context::valid_revision(value) {
        return Err(CommandError::Invalid);
    }
    Ok(())
}

pub(super) fn name(action: &Action) -> &'static str {
    match action {
        Action::Seek { .. } => "seek",
        Action::Reconfigure { .. } => "reconfigure",
        Action::Recover { .. } => "recover",
    }
}
