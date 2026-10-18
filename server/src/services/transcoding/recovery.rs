// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use serde::{Deserialize, Serialize};

use super::{FailureClass, HwAccelMethod, TranscodeManager, TranscodeSession, plan::StreamAction};
use crate::domains::playback::PlaybackError;

pub const MAX_RECOVERY_ATTEMPTS: u8 = 2;
pub const DECODE_SKIP_MS: i64 = 4_000;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct RecoveryBudget {
    pub attempts: u8,
    pub restarted_stall: bool,
    pub skipped_decode: bool,
    pub exhausted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryAction {
    pub position_ms: i64,
    pub software: bool,
    pub skipped_ms: i64,
    pub quality_reduced: bool,
}

impl RecoveryBudget {
    pub fn reserve(
        &mut self,
        failure: FailureClass,
        session: &TranscodeSession,
        confirmed_position_ms: i64,
    ) -> Option<RecoveryAction> {
        self.attempts = self.attempts.max(session.recovery_attempts);
        if self.exhausted || self.attempts >= MAX_RECOVERY_ATTEMPTS || confirmed_position_ms < 0 {
            self.exhausted = true;
            return None;
        }
        let mut action = RecoveryAction {
            position_ms: confirmed_position_ms,
            software: session.hw_accel == HwAccelMethod::Software,
            skipped_ms: 0,
            quality_reduced: false,
        };
        match failure {
            FailureClass::Hardware if session.hw_accel != HwAccelMethod::Software => {
                action.software = true;
            }
            FailureClass::Stalled if !self.restarted_stall => self.restarted_stall = true,
            FailureClass::Decode
                if !self.skipped_decode && session.stream_plan.video == StreamAction::Encode =>
            {
                let failed_near_playhead = session
                    .progress
                    .as_ref()
                    .and_then(|progress| {
                        session
                            .source_start_ms
                            .checked_add(i64::try_from(progress.out_time_ms).ok()?)
                    })
                    .is_some_and(|position| {
                        position >= confirmed_position_ms
                            && position - confirmed_position_ms < DECODE_SKIP_MS
                    });
                if !failed_near_playhead {
                    self.exhausted = true;
                    return None;
                }
                self.skipped_decode = true;
                let Some(position) = confirmed_position_ms
                    .checked_add(DECODE_SKIP_MS)
                    .filter(|position| *position <= i64::from(i32::MAX))
                else {
                    self.exhausted = true;
                    return None;
                };
                action.position_ms = position;
                action.skipped_ms = DECODE_SKIP_MS;
            }
            _ => {
                self.exhausted = true;
                return None;
            }
        }
        self.attempts += 1;
        Some(action)
    }
}

impl TranscodeManager {
    pub fn session_failure(&self, id: uuid::Uuid) -> Option<FailureClass> {
        self.workers.failure(id)
    }

    pub async fn recover_session(
        &self,
        session: TranscodeSession,
        action: RecoveryAction,
        attempts: u8,
        data_dir: &std::path::Path,
    ) -> Result<TranscodeSession, PlaybackError> {
        self.stop_session(session.id).await?;
        let mut params = session.restart_params(action.position_ms);
        if action.software {
            params.hw_accel_override = Some(HwAccelMethod::Software);
        }
        params.recovery_attempts = attempts;
        self.start_session(params, data_dir).await
    }
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
