// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::secrets::FeedField;
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;
use validator::{Validate, ValidationError, ValidationErrors};

#[derive(sqlx::FromRow)]
pub(super) struct AssignmentRow {
    pub id: Uuid,
    pub household_user_id: Uuid,
    pub revision: i64,
    pub source_generation: i64,
    pub playlist_address_count: i16,
    pub guide_address_count: i16,
    pub playlist_transport: String,
    pub guide_transport: String,
    pub enabled: bool,
    pub max_live_streams: Option<i32>,
    pub allow_owner_profile_access: bool,
    pub allow_owner_live_limit: bool,
    pub owner_max_live_streams: Option<i32>,
}

#[derive(Clone, Copy)]
pub(super) struct AdminPolicy {
    pub enabled: bool,
    pub max_live_streams: Option<i32>,
    pub allow_owner_profile_access: bool,
    pub allow_owner_live_limit: bool,
}

impl AssignmentRow {
    pub fn policy(&self) -> AdminPolicy {
        AdminPolicy {
            enabled: self.enabled,
            max_live_streams: self.max_live_streams,
            allow_owner_profile_access: self.allow_owner_profile_access,
            allow_owner_live_limit: self.allow_owner_live_limit,
        }
    }
}

impl CreateRequest {
    pub(super) fn policy(&self) -> AdminPolicy {
        AdminPolicy {
            enabled: self.enabled,
            max_live_streams: self.max_live_streams,
            allow_owner_profile_access: self.allow_owner_profile_access,
            allow_owner_live_limit: self.allow_owner_live_limit,
        }
    }
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateRequest {
    pub playlist_url: String,
    pub guide_url: String,
    #[serde(default)]
    pub enabled: bool,
    #[validate(range(min = 1))]
    pub max_live_streams: Option<i32>,
    #[serde(default)]
    pub allow_owner_profile_access: bool,
    #[serde(default)]
    pub allow_owner_live_limit: bool,
}

fn nullable_limit<'de, D: Deserializer<'de>>(value: D) -> Result<Option<i32>, D::Error> {
    Option::<i32>::deserialize(value)
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum AdminPatch {
    Configure {
        enabled: bool,
        #[serde(deserialize_with = "nullable_limit")]
        max_live_streams: Option<i32>,
        allow_owner_profile_access: bool,
        allow_owner_live_limit: bool,
    },
    RotateCredentials {
        playlist_url: String,
        guide_url: String,
    },
    ReplaceAssignment {
        playlist_url: String,
        guide_url: String,
    },
    ApproveDestination {
        feed: FeedField,
        expected_origin: String,
        addresses: Vec<String>,
    },
    RevokeDestination {
        feed: FeedField,
    },
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum OwnerPatch {
    SetProfileAccess {
        profile_id: Uuid,
        enabled: bool,
    },
    SetLiveLimit {
        #[serde(deserialize_with = "nullable_limit")]
        max_live_streams: Option<i32>,
    },
}

fn limit_valid(value: Option<i32>) -> Result<(), ValidationErrors> {
    if value.is_none_or(|v| v > 0) {
        Ok(())
    } else {
        let mut errors = ValidationErrors::new();
        errors.add(
            "max_live_streams",
            ValidationError::new("positive_limit_required"),
        );
        Err(errors)
    }
}

impl Validate for AdminPatch {
    fn validate(&self) -> Result<(), ValidationErrors> {
        match self {
            Self::Configure {
                max_live_streams, ..
            } => limit_valid(*max_live_streams),
            Self::RotateCredentials { .. }
            | Self::ReplaceAssignment { .. }
            | Self::ApproveDestination { .. }
            | Self::RevokeDestination { .. } => Ok(()),
        }
    }
}

impl Validate for OwnerPatch {
    fn validate(&self) -> Result<(), ValidationErrors> {
        match self {
            Self::SetLiveLimit { max_live_streams } => limit_valid(*max_live_streams),
            Self::SetProfileAccess { .. } => Ok(()),
        }
    }
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ProfileQuery {
    pub after: Option<Uuid>,
    #[validate(range(min = 1, max = 100))]
    pub limit: Option<u32>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum View {
    Administrator,
    HouseholdOwner,
}

#[derive(Serialize)]
pub struct AssignmentResponse {
    pub assignment_id: Uuid,
    pub household_user_id: Uuid,
    pub revision: i64,
    pub source_generation: i64,
    pub playlist_destination_approved: bool,
    pub guide_destination_approved: bool,
    pub playlist_address_count: i16,
    pub guide_address_count: i16,
    pub configured: bool,
    pub playlist_transport: String,
    pub guide_transport: String,
    pub enabled: bool,
    pub max_live_streams: Option<i32>,
    pub owner_max_live_streams: Option<i32>,
    pub configured_live_tv_cap: Option<i32>,
    pub allow_owner_profile_access: bool,
    pub allow_owner_live_limit: bool,
    pub network_checked: bool,
    pub playback_available: bool,
    pub(super) management_scope: View,
}

#[derive(Serialize)]
pub struct ProfileResponse {
    pub id: Uuid,
    pub name: String,
    pub profile_type: String,
    pub enabled: bool,
    pub eligible: bool,
}

#[derive(Serialize)]
pub struct ProfilePage {
    pub assignment_revision: i64,
    pub items: Vec<ProfileResponse>,
    pub next_cursor: Option<Uuid>,
}
