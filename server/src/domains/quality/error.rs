// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum QualityError {
    #[error("capability wizard test not found")]
    WizardTestNotFound,

    #[error("capability wizard already completed for this device")]
    WizardAlreadyCompleted,

    #[error("invalid telemetry report: {0}")]
    InvalidTelemetry(String),

    #[error("too many telemetry reports")]
    TelemetryRateLimited,

    #[error("invalid bandwidth probe result: {0}")]
    InvalidProbeResult(String),

    #[error("device profile not found")]
    DeviceProfileNotFound,

    #[error("transcode decision conflict")]
    TranscodeDecisionConflict,

    #[error("subtitle burn-in required")]
    SubtitleBurnInRequired,

    #[error("unsupported tone mapping algorithm: {0}")]
    UnsupportedToneMappingAlgorithm(String),

    #[error("tone mapping unavailable")]
    ToneMappingUnavailable,

    #[error("invalid quality mode: {0}")]
    InvalidQualityMode(String),

    #[error("requested media version not found")]
    MediaVersionNotFound,

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
