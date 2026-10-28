// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::types::IntentRow;
use crate::{
    domains::household_security::types::Scope,
    extractors::CredentialTransport,
    services::playback_admission::{AdmissionError, AdmissionOwner},
    state::AppState,
};
use sqlx::PgConnection;
use std::sync::Arc;
use uuid::Uuid;

pub(super) fn transport(scope: Scope) -> Result<i16, AdmissionError> {
    match scope.transport {
        CredentialTransport::Cookie => Ok(0),
        CredentialTransport::Bearer => Ok(1),
        CredentialTransport::Legacy => Err(AdmissionError::Authority),
    }
}

pub(in crate::domains::playback) async fn compatible(
    conn: &mut PgConnection,
) -> Result<(), AdmissionError> {
    let valid: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM playback_instances WHERE start_intent_protocol<>2 AND reconciled_at IS NULL)")
        .fetch_one(conn).await?;
    valid.then_some(()).ok_or(AdmissionError::Closed)
}

pub(super) async fn row(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<IntentRow>, AdmissionError> {
    Ok(sqlx::query_as("SELECT household_id,session_id,credential_version,transport,profile_id,profile_revision,intent_hash,instance_id,admission_fenced,preparation_complete,promoted,settled_at,clock_timestamp() AS observed_at FROM playback_start_intents WHERE id=$1 FOR UPDATE")
        .bind(id).fetch_optional(conn).await?)
}

pub(super) fn actor(row: &IntentRow, scope: Scope) -> Result<(), AdmissionError> {
    (row.household_id == scope.user
        && row.session_id == scope.session
        && row.credential_version == scope.version
        && row.transport == transport(scope)?)
    .then_some(())
    .ok_or(AdmissionError::Conflict)
}

pub(in crate::domains::playback::commands) async fn register(
    state: &AppState,
    owner: &Arc<AdmissionOwner>,
    scope: Scope,
    id: Uuid,
    hash: [u8; 32],
) -> Result<bool, AdmissionError> {
    let instance = owner.instance();
    let idle = state.runtime_config.load().auth.session_idle_timeout_hours;
    owner.sql(false, move |conn| Box::pin(async move {
        sqlx::query("SELECT id FROM playback_instances WHERE id=$1 AND NOT fenced AND start_intent_protocol=2 FOR SHARE").bind(instance).fetch_optional(&mut *conn).await?.ok_or(AdmissionError::Closed)?;
        let revision = super::super::database::context(conn, scope, idle).await?;
        compatible(conn).await?;
        if let Some(saved) = row(conn, id).await? {
            actor(&saved, scope)?;
            if saved.intent_hash.as_deref()!=Some(hash.as_slice()) || saved.profile_id!=Some(scope.profile) || saved.profile_revision!=Some(revision) {
                return Err(AdmissionError::Conflict);
            }
            return Ok(false);
        }
        sqlx::query("INSERT INTO playback_start_intents(id,household_id,session_id,credential_version,transport,profile_id,profile_revision,intent_hash,instance_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(id).bind(scope.user).bind(scope.session).bind(scope.version).bind(transport(scope)?).bind(scope.profile).bind(revision).bind(hash.as_slice()).bind(instance).execute(conn).await?;
        Ok(true)
    })).await
}

pub(in crate::domains::playback::commands) async fn complete(
    owner: &Arc<AdmissionOwner>,
    scope: Scope,
    id: Uuid,
    fence: bool,
) -> Result<(), AdmissionError> {
    let instance = owner.instance();
    owner.sql(true, move |conn| Box::pin(async move {
        sqlx::query("SELECT id FROM playback_instances WHERE id=$1 FOR SHARE").bind(instance).execute(&mut *conn).await?;
        crate::services::playback_admission::vod::cleanup_lock(conn,scope.user).await?;
        let saved = row(conn,id).await?.ok_or(AdmissionError::Conflict)?;
        actor(&saved,scope)?;
        if saved.instance_id!=instance {return Err(AdmissionError::Conflict);}
        sqlx::query("UPDATE playback_start_intents SET preparation_complete=true,admission_fenced=admission_fenced OR $2 WHERE id=$1")
            .bind(id).bind(fence).execute(conn).await?;
        Ok(())
    })).await
}

pub(in crate::domains::playback::commands) async fn pending(
    state: &AppState,
    owner: &Arc<AdmissionOwner>,
    scope: Scope,
    id: Uuid,
    hash: Option<[u8; 32]>,
) -> Result<Option<super::super::types::StatusResponse>, AdmissionError> {
    let instance = owner.instance();
    let idle = state.runtime_config.load().auth.session_idle_timeout_hours;
    owner
        .sql(false, move |conn| {
            Box::pin(async move {
                let revision = super::super::database::context(conn, scope, idle).await?;
                let Some(saved) = row(conn, id).await? else {
                    return Ok(None);
                };
                if saved.household_id != scope.user {
                    return Ok(None);
                }
                actor(&saved, scope)?;
                if saved.profile_id != Some(scope.profile)
                    || saved.profile_revision != Some(revision)
                    || hash
                        .is_some_and(|hash| saved.intent_hash.as_deref() != Some(hash.as_slice()))
                {
                    return Err(AdmissionError::Conflict);
                }
                if saved.promoted {
                    return Ok(None);
                }
                let status = if saved.instance_id != instance && !saved.preparation_complete {
                    "uncertain"
                } else if saved.admission_fenced {
                    if saved.preparation_complete {
                        "withdrawn"
                    } else {
                        "withdrawing"
                    }
                } else {
                    "preparing"
                };
                Ok(Some(super::super::types::StatusResponse::receipt(
                    id,
                    status,
                    saved.observed_at,
                    None,
                )))
            })
        })
        .await
}
