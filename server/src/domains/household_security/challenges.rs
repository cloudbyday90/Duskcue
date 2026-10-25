// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{error::OwnerError, factors, grants, operator, transactions, types::*};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use sqlx::{Acquire, Postgres, Row, Transaction};
use uuid::Uuid;
use webauthn_rs::prelude::{
    PasskeyAuthentication, PasskeyRegistration, PublicKeyCredential, RegisterPublicKeyCredential,
};

#[derive(Serialize, Deserialize)]
enum Ceremony {
    Authentication {
        state: PasskeyAuthentication,
        factors: Vec<(Uuid, String)>,
    },
    Registration {
        state: PasskeyRegistration,
    },
}

pub(super) async fn start(
    app: &AppState,
    actor: Scope,
    req: PasskeyStartRequest,
) -> Result<ChallengeResponse, OwnerError> {
    let mut tx = transactions::begin(&app.pool, actor.user).await?;
    transactions::scope(&mut tx, actor).await?;
    let revision = transactions::available(&mut tx, actor.user).await?;
    let (purpose, handoff, selected, name) = match req {
        PasskeyStartRequest::Unlock {} => ("unlock", None, None, None),
        PasskeyStartRequest::Enroll {
            handoff,
            passkey_id,
        } => ("enroll", Some(handoff), Some(passkey_id), None),
        PasskeyStartRequest::Register { handoff, name } => {
            if name.trim().is_empty() || name.chars().count() > 200 {
                return Err(OwnerError::InvalidRequest);
            }
            (
                "register",
                Some(handoff),
                None,
                Some(name.trim().to_string()),
            )
        }
    };
    let handoff_id = if let Some(handoff) = handoff {
        match operator::validate(&mut tx, actor.user, &handoff).await {
            Ok(id) => Some(id),
            Err(OwnerError::InvalidProof) => return transactions::failed(tx, actor).await,
            Err(e) => return Err(e),
        }
    } else {
        None
    };
    sqlx::query("SELECT pg_advisory_xact_lock(48027,731)")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM household_owner_challenges WHERE expires_at<=clock_timestamp()")
        .execute(&mut *tx)
        .await?;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM household_owner_challenges")
        .fetch_one(&mut *tx)
        .await?;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM household_owner_challenges WHERE user_id=$1")
            .bind(actor.user)
            .fetch_one(&mut *tx)
            .await?;
    if total >= 4096 || count >= 8 {
        return Err(OwnerError::Locked);
    }
    let (options, ceremony) = if purpose == "register" {
        factors::ensure_enrollment_capacity(&mut tx, actor.user).await?;
        let row = sqlx::query("SELECT username,display_name FROM users WHERE id=$1")
            .bind(actor.user)
            .fetch_one(&mut *tx)
            .await?;
        let ids: Vec<Vec<u8>> = sqlx::query_scalar(
            "SELECT credential_id FROM user_passkeys WHERE user_id=$1 ORDER BY id LIMIT 64",
        )
        .bind(actor.user)
        .fetch_all(&mut *tx)
        .await?;
        let (options, state) = app
            .webauthn
            .start_passkey_registration(
                actor.user,
                row.get::<&str, _>("username"),
                row.get::<&str, _>("display_name"),
                Some(ids),
            )
            .map_err(|_| OwnerError::Unavailable)?;
        (
            serde_json::to_value(options).map_err(|_| OwnerError::Unavailable)?,
            Ceremony::Registration { state },
        )
    } else {
        let values = factors::load(&mut tx, actor.user, selected).await?;
        let keys = values.iter().map(|(_, p, _)| p.clone()).collect::<Vec<_>>();
        let (options, state) = app
            .webauthn
            .start_passkey_authentication(&keys)
            .map_err(|_| OwnerError::Unavailable)?;
        (
            serde_json::to_value(options).map_err(|_| OwnerError::Unavailable)?,
            Ceremony::Authentication {
                state,
                factors: values.into_iter().map(|(id, _, fp)| (id, fp)).collect(),
            },
        )
    };
    let state = serde_json::to_value(ceremony).map_err(|_| OwnerError::Unavailable)?;
    let challenge_id=sqlx::query_scalar("INSERT INTO household_owner_challenges(user_id,session_id,profile_id,revision,handoff_id,purpose,factor_id,state,name,credential_version,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,clock_timestamp()+interval '2 minutes') RETURNING id")
        .bind(actor.user).bind(actor.session).bind(actor.profile).bind(revision).bind(handoff_id).bind(purpose).bind(selected).bind(state).bind(name).bind(actor.version).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(ChallengeResponse {
        challenge_id,
        options,
    })
}

pub(super) async fn finish(
    app: &AppState,
    actor: Scope,
    req: PasskeyFinishRequest,
) -> Result<Proven, OwnerError> {
    let mut tx = transactions::begin(&app.pool, actor.user).await?;
    transactions::scope(&mut tx, actor).await?;
    let revision = transactions::available(&mut tx, actor.user).await?;
    let row=sqlx::query("DELETE FROM household_owner_challenges WHERE id=$1 AND user_id=$2 AND session_id=$3 AND profile_id=$4 AND credential_version=$5 RETURNING *, expires_at>clock_timestamp() AS time_valid")
        .bind(req.challenge_id).bind(actor.user).bind(actor.session).bind(actor.profile).bind(actor.version).fetch_optional(&mut *tx).await?;
    let Some(row) = row else {
        return transactions::failed(tx, actor).await;
    };
    if row.get::<i64, _>("revision") != revision || !row.get::<bool, _>("time_valid") {
        return transactions::failed(tx, actor).await;
    }
    let mut operation = tx.begin().await?;
    let outcome = complete(app, &mut operation, actor, &row, req.credential).await;
    match outcome {
        Ok(proven) => {
            operation.commit().await?;
            tx.commit().await?;
            Ok(proven)
        }
        Err(OwnerError::InvalidProof) => {
            operation.rollback().await?;
            transactions::failed(tx, actor).await
        }
        Err(e) => {
            operation.rollback().await?;
            Err(e)
        }
    }
}

async fn complete(
    app: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    actor: Scope,
    row: &sqlx::postgres::PgRow,
    credential: serde_json::Value,
) -> Result<Proven, OwnerError> {
    let ceremony: Ceremony =
        serde_json::from_value(row.get("state")).map_err(|_| OwnerError::Unavailable)?;
    let purpose: &str = row.get("purpose");
    let handoff: Option<Uuid> = row.get("handoff_id");
    if purpose != "unlock" {
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM household_owner_handoffs WHERE id=$1 AND user_id=$2 AND expires_at>clock_timestamp())")
            .bind(handoff).bind(actor.user).fetch_one(&mut **tx).await?;
        if !valid {
            return Err(OwnerError::InvalidProof);
        }
    } else if handoff.is_some() {
        return Err(OwnerError::InvalidProof);
    }
    let _permit = app
        .owner_proof_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| OwnerError::Locked)?;
    match ceremony {
        Ceremony::Authentication {
            state,
            factors: bound,
        } => {
            if !matches!(purpose, "unlock" | "enroll") {
                return Err(OwnerError::InvalidProof);
            }
            let assertion: PublicKeyCredential =
                serde_json::from_value(credential).map_err(|_| OwnerError::InvalidProof)?;
            if assertion
                .get_user_unique_id()
                .is_some_and(|handle| handle != actor.user.as_bytes())
            {
                return Err(OwnerError::InvalidProof);
            }
            factors::validate_client_context(&assertion.response.client_data_json)?;
            let result = app
                .webauthn
                .finish_passkey_authentication(&assertion, &state)
                .map_err(|_| OwnerError::InvalidProof)?;
            let selected: Option<Uuid> = row.get("factor_id");
            let values = factors::load(tx, actor.user, selected).await?;
            let (id, key, fp) = values
                .into_iter()
                .find(|(_, key, _)| key.cred_id() == result.cred_id())
                .ok_or(OwnerError::InvalidProof)?;
            if !bound
                .iter()
                .any(|(expected, hash)| *expected == id && *hash == fp)
            {
                return Err(OwnerError::InvalidProof);
            }
            factors::update(tx, actor.user, id, key, &result, purpose == "enroll").await?;
        }
        Ceremony::Registration { state } => {
            if purpose != "register" {
                return Err(OwnerError::InvalidProof);
            }
            factors::ensure_enrollment_capacity(tx, actor.user).await?;
            let registration: RegisterPublicKeyCredential =
                serde_json::from_value(credential).map_err(|_| OwnerError::InvalidProof)?;
            factors::validate_client_context(&registration.response.client_data_json)?;
            let passkey = app
                .webauthn
                .finish_passkey_registration(&registration, &state)
                .map_err(|_| OwnerError::InvalidProof)?;
            let bytes = serde_json::to_vec(&passkey).map_err(|_| OwnerError::Unavailable)?;
            let name: String = row.get("name");
            let result=sqlx::query("INSERT INTO user_passkeys(user_id,credential_id,public_key,name,owner_trusted) VALUES ($1,$2,$3,$4,true) ON CONFLICT (credential_id) DO NOTHING")
                .bind(actor.user).bind(passkey.cred_id()).bind(bytes).bind(name).execute(&mut **tx).await?;
            if result.rows_affected() != 1 {
                return Err(OwnerError::InvalidProof);
            }
        }
    }
    if let Some(id) = handoff {
        operator::consume(tx, actor.user, id).await?;
        factors::activate(tx, actor.user).await?;
    }
    grants::rotate(tx, actor).await
}
