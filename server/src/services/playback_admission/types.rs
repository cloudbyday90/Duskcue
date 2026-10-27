// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::AdmissionError;
use crate::{domains::household_security::types::Scope, extractors::CredentialTransport};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Subject {
    Vod { file: Uuid, item: Uuid },
    Live { snapshot: Uuid, channel: Uuid },
}

#[derive(Clone, Copy)]
pub struct Request {
    pub id: Uuid,
    pub actor: Scope,
    pub profile_revision: i64,
    pub subject: Subject,
}

#[derive(Serialize, Deserialize)]
struct Fingerprint {
    user: Uuid,
    session: Uuid,
    profile: Uuid,
    version: i64,
    profile_revision: i64,
    transport: u8,
    subject: Subject,
}

impl Request {
    pub(super) fn encode(self) -> Result<String, AdmissionError> {
        if self.id.get_version_num() != 7 || self.actor.version <= 0 || self.profile_revision <= 0 {
            return Err(AdmissionError::Authority);
        }
        let transport = match self.actor.transport {
            CredentialTransport::Cookie => 0,
            CredentialTransport::Bearer => 1,
            CredentialTransport::Legacy => return Err(AdmissionError::Authority),
        };
        let value = serde_json::to_string(&Fingerprint {
            user: self.actor.user,
            session: self.actor.session,
            profile: self.actor.profile,
            version: self.actor.version,
            profile_revision: self.profile_revision,
            transport,
            subject: self.subject,
        })
        .map_err(|_| AdmissionError::Authority)?;
        if value.len() > 2048 {
            return Err(AdmissionError::Authority);
        }
        Ok(value)
    }
    pub(crate) fn decode(id: Uuid, value: &str) -> Result<Self, AdmissionError> {
        if value.len() > 2048 {
            return Err(AdmissionError::Authority);
        }
        let f: Fingerprint = serde_json::from_str(value).map_err(|_| AdmissionError::Authority)?;
        let request = Self {
            id,
            actor: Scope {
                user: f.user,
                session: f.session,
                profile: f.profile,
                version: f.version,
                binding: None,
                transport: match f.transport {
                    0 => CredentialTransport::Cookie,
                    1 => CredentialTransport::Bearer,
                    _ => return Err(AdmissionError::Authority),
                },
            },
            profile_revision: f.profile_revision,
            subject: f.subject,
        };
        if request.encode()? != value {
            return Err(AdmissionError::Authority);
        }
        Ok(request)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReservationState {
    Held,
    Retiring,
    Released,
}

#[derive(Clone, Debug)]
pub struct Receipt {
    pub id: Uuid,
    pub instance: Uuid,
    pub state: ReservationState,
    pub deadline: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub fresh: bool,
    pub owned_here: bool,
}

#[derive(sqlx::FromRow)]
pub(super) struct Row {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub household_id: Uuid,
    pub fingerprint: String,
    pub kind: String,
    pub assignment_id: Option<Uuid>,
    pub source_generation: Option<i64>,
    pub channel_key: Option<Vec<u8>>,
    pub state: String,
    pub deadline: DateTime<Utc>,
}
impl Row {
    pub fn receipt(
        &self,
        now: DateTime<Utc>,
        fresh: bool,
        owner: Uuid,
    ) -> Result<Receipt, AdmissionError> {
        let original = Request::decode(self.id, &self.fingerprint)?;
        let expected = match original.subject {
            Subject::Vod { .. } => "vod",
            Subject::Live { .. } => "live",
        };
        if self.kind != expected || original.actor.user != self.household_id {
            return Err(AdmissionError::Storage);
        }
        Ok(Receipt {
            id: self.id,
            instance: self.instance_id,
            state: match self.state.as_str() {
                "held" => ReservationState::Held,
                "retiring" => ReservationState::Retiring,
                "released" => ReservationState::Released,
                _ => return Err(AdmissionError::Storage),
            },
            deadline: self.deadline,
            observed_at: now,
            fresh,
            owned_here: self.instance_id == owner,
        })
    }
}
