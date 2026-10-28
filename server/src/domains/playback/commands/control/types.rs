// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use crate::domains::playback::types::DirectFallbackResponse;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChangeRequest {
    pub protocol: String,
    pub operation_id: Uuid,
    pub expected_output: Uuid,
    pub action: Action,
}

#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Action {
    Seek {
        position_ms: i32,
    },
    Reconfigure {
        position_ms: i32,
        selection: Value,
    },
    Recover {
        checkpoint_sequence: u64,
        evidence: Evidence,
    },
}

#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Evidence {
    Worker,
    NativeDecode {
        offer: String,
        supported: bool,
        smooth: Option<bool>,
    },
    HlsMedia {
        offer: String,
        supported: bool,
        smooth: bool,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckpointRequest {
    pub protocol: String,
    pub output_revision: Uuid,
    pub sequence: u64,
    pub position_ms: i32,
    pub playback_state: String,
}

#[derive(Serialize)]
pub(super) struct CheckpointResponse {
    pub protocol: &'static str,
    pub request_id: Uuid,
    pub output_revision: Option<Uuid>,
    pub sequence: i64,
    pub position_ms: Option<i32>,
    pub playback_state: String,
}

#[derive(Clone, Serialize)]
pub(super) struct RecoveryOffer {
    pub output_revision: Uuid,
    pub kind: &'static str,
    pub descriptor: DirectFallbackResponse,
    pub fingerprint: String,
}

#[derive(Serialize)]
pub(super) struct ConfigurationResponse {
    pub protocol: &'static str,
    pub request_id: Uuid,
    pub revision: String,
    pub state: String,
    pub output_revision: Uuid,
    pub pending_operation: Option<Uuid>,
    pub configuration: Value,
    pub playback_state: String,
    pub recovery_budget: Value,
    pub recovery_offer: Option<RecoveryOffer>,
}

#[derive(Serialize)]
pub(super) struct OperationResponse {
    pub protocol: &'static str,
    pub request_id: Uuid,
    pub operation_id: Uuid,
    pub status: String,
    pub status_url: String,
    pub output_revision: Option<Uuid>,
    pub result: Option<Value>,
    pub cleanup_complete: bool,
}
