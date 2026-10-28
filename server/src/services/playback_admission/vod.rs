// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::AdmissionError;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

pub(crate) mod revisions;

pub(crate) async fn lock(conn: &mut PgConnection, user: Uuid) -> Result<(), AdmissionError> {
    super::policy::fence(conn).await?;
    super::database::household(conn, user).await
}

pub(crate) async fn missing(conn: &mut PgConnection, id: Uuid) -> Result<(), AdmissionError> {
    super::retention::database::missing(conn, id).await
}

pub(crate) async fn cleanup_lock(
    conn: &mut PgConnection,
    user: Uuid,
) -> Result<(), AdmissionError> {
    super::policy::fence(conn).await?;
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR NO KEY UPDATE")
        .bind(user)
        .execute(conn)
        .await?;
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Binding {
    pub intent: [u8; 32],
    pub policy: [u8; 32],
    pub source: [u8; 32],
    pub mode: Mode,
    pub encodes: bool,
    pub source_4k: bool,
    pub ambient: Option<(Uuid, chrono::DateTime<chrono::Utc>)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    DirectPlay,
    DirectStream,
    Transcode,
}
impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DirectPlay => "direct_play",
            Self::DirectStream => "direct_stream",
            Self::Transcode => "transcode",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Policy {
    pub total: Option<i32>,
    pub transcode: Option<i32>,
    pub bandwidth: Option<i64>,
    pub allow_direct_play: bool,
    pub allow_direct_stream: bool,
    pub allow_transcode: bool,
    pub resolution: Option<String>,
    pub allow_transcode_4k: bool,
    pub require_direct_play_4k: bool,
    pub supported: bool,
    pub valid: bool,
}
impl Policy {
    pub fn hash(&self) -> Result<[u8; 32], AdmissionError> {
        let bytes = serde_json::to_vec(self).map_err(|_| AdmissionError::Policy)?;
        ring::digest::digest(&ring::digest::SHA256, &bytes)
            .as_ref()
            .try_into()
            .map_err(|_| AdmissionError::Policy)
    }
    pub fn permits(&self, binding: Binding) -> bool {
        self.valid
            && self.supported
            && match binding.mode {
                Mode::DirectPlay => self.allow_direct_play,
                Mode::DirectStream => self.allow_direct_stream,
                Mode::Transcode => self.allow_transcode,
            }
            && (!binding.encodes || self.allow_transcode)
            && (!binding.source_4k
                || ((!binding.encodes || self.allow_transcode_4k)
                    && (binding.mode == Mode::DirectPlay || !self.require_direct_play_4k)))
    }
}

pub(crate) async fn policy(conn: &mut PgConnection, user: Uuid) -> Result<Policy, AdmissionError> {
    let value: Option<serde_json::Value> = sqlx::query_scalar("SELECT playback_vod_policy($1)")
        .bind(user)
        .fetch_one(conn)
        .await?;
    let value: Policy = serde_json::from_value(value.ok_or(AdmissionError::Policy)?)
        .map_err(|_| AdmissionError::Policy)?;
    if !value.valid || !value.supported {
        return Err(AdmissionError::Policy);
    }
    Ok(value)
}

#[derive(sqlx::FromRow)]
struct SavedBinding {
    intent_hash: Vec<u8>,
    policy_hash: Vec<u8>,
    source_hash: Vec<u8>,
    delivery_mode: String,
    encodes: bool,
    source_4k: bool,
    ambient_channel_id: Option<Uuid>,
    ambient_revision: Option<chrono::DateTime<chrono::Utc>>,
}

pub(super) async fn matches(
    conn: &mut PgConnection,
    id: Uuid,
    binding: Option<Binding>,
) -> Result<(), AdmissionError> {
    let saved:Option<SavedBinding>=sqlx::query_as("SELECT intent_hash,policy_hash,source_hash,delivery_mode,encodes,source_4k,ambient_channel_id,ambient_revision FROM playback_vod_commands WHERE reservation_id=$1").bind(id).fetch_optional(conn).await?;
    match (saved, binding) {
        (None, None) => Ok(()),
        (Some(saved), Some(binding))
            if saved.intent_hash == binding.intent
                && saved.policy_hash == binding.policy
                && saved.source_hash == binding.source
                && saved.delivery_mode == binding.mode.as_str()
                && saved.encodes == binding.encodes
                && saved.source_4k == binding.source_4k
                && saved.ambient_channel_id.zip(saved.ambient_revision) == binding.ambient =>
        {
            Ok(())
        }
        _ => Err(AdmissionError::Conflict),
    }
}

pub(super) async fn insert(
    conn: &mut PgConnection,
    id: Uuid,
    binding: Binding,
) -> Result<(), AdmissionError> {
    sqlx::query("INSERT INTO playback_vod_commands(reservation_id,intent_hash,policy_hash,source_hash,delivery_mode,encodes,source_4k,ambient_channel_id,ambient_revision,protocol) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,2)")
        .bind(id).bind(binding.intent.as_slice()).bind(binding.policy.as_slice()).bind(binding.source.as_slice()).bind(binding.mode.as_str()).bind(binding.encodes).bind(binding.source_4k).bind(binding.ambient.map(|v|v.0)).bind(binding.ambient.map(|v|v.1)).execute(conn).await?;
    Ok(())
}

pub(super) async fn ambient(
    conn: &mut PgConnection,
    request: super::Request,
    binding: Binding,
) -> Result<(), AdmissionError> {
    if let Some((id, revision)) = binding.ambient {
        let super::Subject::Vod { item, .. } = request.subject else {
            return Err(AdmissionError::Authority);
        };
        let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ambient_channels c JOIN user_profiles p ON p.owner_user_id=c.owner_user_id JOIN ambient_channel_items i ON i.channel_id=c.id WHERE c.id=$1 AND c.owner_user_id=$2 AND c.updated_at=$3 AND c.is_enabled AND p.id=$4 AND p.allow_ambient_channels AND p.profile_type=c.audience AND i.media_item_id=$5)")
            .bind(id).bind(request.actor.user).bind(revision).bind(request.actor.profile).bind(item).fetch_one(conn).await?;
        if !allowed {
            return Err(AdmissionError::Authority);
        }
    }
    Ok(())
}
