// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use crate::extractors::AuthenticatedUser;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::{Validate, ValidationError, ValidationErrors};

#[derive(Clone, Copy)]
pub struct Scope {
    pub version: i64,
    pub user: Uuid,
    pub session: Uuid,
    pub profile: Uuid,
}

impl From<&AuthenticatedUser> for Scope {
    fn from(user: &AuthenticatedUser) -> Self {
        Self {
            version: user.credential_version,
            user: user.user_id,
            session: user.session_id,
            profile: user.profile_id,
        }
    }
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PasswordRequest {
    #[validate(length(min = 1, max = 1024))]
    pub password: String,
    #[validate(length(min = 1, max = 128))]
    pub handoff: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum PasskeyStartRequest {
    Unlock {},
    Enroll { handoff: String, passkey_id: Uuid },
    Register { handoff: String, name: String },
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PasskeyFinishRequest {
    pub challenge_id: Uuid,
    #[validate(custom(function = "credential_shape"))]
    pub credential: serde_json::Value,
}

fn credential_shape(value: &serde_json::Value) -> Result<(), ValidationError> {
    if value.is_object() {
        Ok(())
    } else {
        Err(ValidationError::new("invalid_credential"))
    }
}

impl Validate for PasskeyStartRequest {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let valid = match self {
            Self::Unlock {} => true,
            Self::Enroll { handoff, .. } => !handoff.is_empty() && handoff.len() <= 128,
            Self::Register { handoff, name } => {
                !handoff.is_empty()
                    && handoff.len() <= 128
                    && !name.trim().is_empty()
                    && name.chars().count() <= 200
            }
        };
        if valid {
            Ok(())
        } else {
            let mut errors = ValidationErrors::new();
            errors.add("body", ValidationError::new("invalid_request"));
            Err(errors)
        }
    }
}

#[derive(Serialize)]
pub struct ChallengeResponse {
    pub challenge_id: Uuid,
    pub options: serde_json::Value,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub protection_enabled: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub password_available: bool,
    pub trusted_passkeys: i64,
}

#[derive(Serialize)]
pub struct ProofResponse {
    pub expires_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
}

pub(super) struct Proven {
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub session_expires_at: DateTime<Utc>,
}
