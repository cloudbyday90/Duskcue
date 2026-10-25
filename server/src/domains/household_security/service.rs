// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{error::OwnerError, factors, grants, operator, transactions, types::*};
use crate::state::AppState;

pub use super::factors::{
    allow_delete, ensure_enrollment_capacity, trusted_change, validate_client_context,
};

pub async fn status(state: &AppState, actor: Scope) -> Result<StatusResponse, OwnerError> {
    let mut tx = transactions::begin(&state.pool, actor.user).await?;
    transactions::scope(&mut tx, actor).await?;
    let enabled: bool =
        sqlx::query_scalar("SELECT enabled FROM household_owner_state WHERE user_id=$1")
            .bind(actor.user)
            .fetch_one(&mut *tx)
            .await?;
    let expires_at=sqlx::query_scalar("SELECT g.expires_at FROM household_owner_grants g JOIN household_owner_state o ON o.user_id=g.user_id WHERE session_id=$1 AND g.revision=o.revision AND g.credential_version=$2 AND expires_at>clock_timestamp() AND (o.locked_until IS NULL OR o.locked_until<=clock_timestamp())")
        .bind(actor.session).bind(actor.version).fetch_optional(&mut *tx).await?;
    let password_available = factors::password_available(&mut tx, actor.user).await?;
    let trusted_passkeys =
        sqlx::query_scalar("SELECT count(*) FROM user_passkeys WHERE user_id=$1 AND owner_trusted")
            .bind(actor.user)
            .fetch_one(&mut *tx)
            .await?;
    tx.commit().await?;
    Ok(StatusResponse {
        protection_enabled: enabled,
        expires_at,
        password_available,
        trusted_passkeys,
    })
}

pub(super) async fn password(
    state: &AppState,
    actor: Scope,
    req: PasswordRequest,
) -> Result<Proven, OwnerError> {
    if req.password.is_empty()
        || req.password.len() > 1024
        || req.handoff.as_ref().is_some_and(|v| v.len() > 128)
    {
        return Err(OwnerError::InvalidRequest);
    }
    let mut tx = transactions::begin(&state.pool, actor.user).await?;
    transactions::scope(&mut tx, actor).await?;
    transactions::available(&mut tx, actor.user).await?;
    let handoff = if let Some(token) = req.handoff {
        match operator::validate(&mut tx, actor.user, &token).await {
            Ok(id) => Some(id),
            Err(OwnerError::InvalidProof) => return transactions::failed(tx, actor).await,
            Err(e) => return Err(e),
        }
    } else {
        None
    };
    if handoff.is_none() && !factors::password_available(&mut tx, actor.user).await? {
        return transactions::failed(tx, actor).await;
    }
    let hash: Option<String> = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1")
        .bind(actor.user)
        .fetch_one(&mut *tx)
        .await?;
    let Some(hash) = hash else {
        return transactions::failed(tx, actor).await;
    };
    let permit = state
        .owner_proof_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| OwnerError::Locked)?;
    let password = req.password;
    let check_hash = hash.clone();
    let valid = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        crate::domains::auth::service::verify_password_hash(&check_hash, &password).is_ok()
    })
    .await
    .map_err(|_| OwnerError::Unavailable)?;
    if !valid {
        return transactions::failed(tx, actor).await;
    }
    if let Some(id) = handoff {
        operator::consume(&mut tx, actor.user, id).await?;
        sqlx::query("UPDATE household_owner_state SET password_fingerprint=$2 WHERE user_id=$1")
            .bind(actor.user)
            .bind(operator::fingerprint(hash.as_bytes()))
            .execute(&mut *tx)
            .await?;
        factors::activate(&mut tx, actor.user).await?;
    }
    let proven = grants::rotate(&mut tx, actor).await?;
    tx.commit().await?;
    Ok(proven)
}

pub async fn lock(state: &AppState, actor: Scope) -> Result<(), OwnerError> {
    let mut tx = transactions::begin(&state.pool, actor.user).await?;
    transactions::scope(&mut tx, actor).await?;
    sqlx::query("DELETE FROM household_owner_grants WHERE session_id=$1 AND user_id=$2")
        .bind(actor.session)
        .bind(actor.user)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM household_owner_challenges WHERE session_id=$1 AND user_id=$2")
        .bind(actor.session)
        .bind(actor.user)
        .execute(&mut *tx)
        .await?;
    transactions::audit(&mut tx, actor.user, Some(actor.user), "management_locked").await?;
    tx.commit().await?;
    Ok(())
}

pub(super) fn proof_body(proven: &Proven, bearer: bool) -> ProofResponse {
    ProofResponse {
        expires_at: proven.expires_at,
        session_token: bearer.then(|| proven.token.clone()),
    }
}
