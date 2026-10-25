// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;
use crate::domains::household_security as owner;

pub async fn start_passkey_registration(
    state: &AppState,
    user_id: Uuid,
    username: &str,
    display_name: &str,
    _passkey_name: &str,
    actor: crate::domains::household_security::types::Scope,
) -> Result<(String, serde_json::Value), AuthError> {
    let mut tx = owner::transactions::begin(&state.pool, user_id).await?;
    let enabled = owner::transactions::authorize(&mut tx, user_id, Some(actor)).await?;
    if enabled {
        owner::service::ensure_enrollment_capacity(&mut tx, user_id).await?;
    }
    let revision: i64 =
        sqlx::query_scalar("SELECT revision FROM household_owner_state WHERE user_id=$1")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
    let existing_passkeys = load_user_passkey_credentials(&state.pool, user_id).await?;

    let exclude_credentials: Vec<CredentialID> = existing_passkeys
        .iter()
        .map(|pk| pk.credential_id.clone())
        .collect();

    let (ccr, registration_state) = state
        .webauthn
        .start_passkey_registration(
            user_id,
            username,
            display_name,
            if exclude_credentials.is_empty() {
                None
            } else {
                Some(exclude_credentials)
            },
        )
        .map_err(|_| AuthError::WebauthnRegistrationFailed {
            reason: "Passkey operation failed".to_string(),
        })?;

    let challenge_id = generate_challenge_id();
    let creation_options =
        serde_json::to_value(&ccr).map_err(|_| AuthError::WebauthnRegistrationFailed {
            reason: "Passkey options unavailable".to_string(),
        })?;

    state.webauthn_challenges.insert(
        challenge_id.clone(),
        WebauthnChallenge {
            registration_state: Some(registration_state),
            authentication_state: None,
            user_id: Some(user_id),
            owner_scope: Some(actor),
            owner_revision: Some(revision),
            created_at: std::time::Instant::now(),
        },
    );

    tx.commit().await?;
    Ok((challenge_id, creation_options))
}

pub async fn finish_passkey_registration(
    state: &AppState,
    challenge_id: &str,
    credential_json: &serde_json::Value,
    passkey_name: &str,
    actor: crate::domains::household_security::types::Scope,
) -> Result<PasskeyResponse, AuthError> {
    let (_, challenge) = state
        .webauthn_challenges
        .remove(challenge_id)
        .ok_or(AuthError::WebauthnChallengeExpired)?;

    let registration_state = challenge
        .registration_state
        .ok_or(AuthError::WebauthnChallengeExpired)?;

    if challenge.created_at.elapsed() > std::time::Duration::from_secs(300) {
        return Err(AuthError::WebauthnChallengeExpired);
    }

    let user_id = challenge
        .user_id
        .ok_or(AuthError::WebauthnChallengeExpired)?;

    if challenge.owner_scope.is_none_or(|s| {
        s.user != actor.user
            || s.session != actor.session
            || s.profile != actor.profile
            || s.version != actor.version
    }) || user_id != actor.user
    {
        return Err(AuthError::WebauthnChallengeExpired);
    }
    let mut tx = owner::transactions::begin(&state.pool, user_id).await?;
    let enabled = owner::transactions::authorize(&mut tx, user_id, Some(actor)).await?;
    if enabled {
        owner::service::ensure_enrollment_capacity(&mut tx, user_id).await?;
    }
    let revision: i64 =
        sqlx::query_scalar("SELECT revision FROM household_owner_state WHERE user_id=$1")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
    if challenge.owner_revision != Some(revision) {
        return Err(AuthError::WebauthnChallengeExpired);
    }

    let registration = serde_json::from_value::<RegisterPublicKeyCredential>(
        credential_json.clone(),
    )
    .map_err(|_| AuthError::WebauthnRegistrationFailed {
        reason: "Invalid credential data".to_string(),
    })?;
    if enabled {
        owner::service::validate_client_context(&registration.response.client_data_json)?;
    }

    let passkey = state
        .webauthn
        .finish_passkey_registration(&registration, &registration_state)
        .map_err(|_| AuthError::WebauthnRegistrationFailed {
            reason: "Passkey operation failed".to_string(),
        })?;

    let credential_id_bytes = passkey.cred_id().clone();
    let public_key_bytes =
        serde_json::to_vec(&passkey).map_err(|_| AuthError::WebauthnRegistrationFailed {
            reason: "Passkey storage unavailable".to_string(),
        })?;

    let credential: webauthn_rs::prelude::Credential = passkey.clone().into();
    let counter_val: u32 = credential.counter;

    let transports_list: Vec<String> = credential
        .transports
        .unwrap_or_default()
        .iter()
        .map(|t| format!("{:?}", t).to_lowercase())
        .collect();
    let transports_json =
        serde_json::to_value(&transports_list).unwrap_or(serde_json::Value::Array(vec![]));

    let row = sqlx::query(
        r#"
        INSERT INTO user_passkeys (user_id, credential_id, public_key, sign_count, transports, attestation_type, aaguid, name, owner_trusted)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id, created_at
        "#,
    )
    .bind(user_id)
    .bind(&credential_id_bytes)
    .bind(&public_key_bytes)
    .bind(counter_val as i64)
    .bind(&transports_json)
    .bind("none")
    .bind(credential_id_bytes.get(..16).map(|b| {
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(b);
        Uuid::from_bytes(bytes)
    }))
    .bind(passkey_name)
    .bind(enabled)
    .fetch_one(&mut *tx)
    .await?;

    let passkey_id: Uuid = row.get("id");
    let created_at: DateTime<Utc> = row.get("created_at");

    if enabled {
        owner::service::trusted_change(&mut tx, user_id).await?;
    }
    tx.commit().await?;
    Ok(PasskeyResponse {
        id: passkey_id,
        name: passkey_name.to_string(),
        aaguid: None,
        transports: transports_list,
        last_used_at: None,
        created_at,
    })
}

pub async fn delete_passkey(
    pool: &sqlx::PgPool,
    passkey_id: Uuid,
    user_id: Uuid,
    actor: crate::domains::household_security::types::Scope,
) -> Result<(), AuthError> {
    let mut tx = owner::transactions::begin(pool, user_id).await?;
    let enabled = owner::transactions::authorize(&mut tx, user_id, Some(actor)).await?;
    if enabled {
        owner::service::allow_delete(&mut tx, user_id, passkey_id).await?;
    }
    let result = sqlx::query("DELETE FROM user_passkeys WHERE id = $1 AND user_id = $2")
        .bind(passkey_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AuthError::PasskeyNotFound);
    }

    if enabled {
        owner::service::trusted_change(&mut tx, user_id).await?;
    }
    tx.commit().await?;
    Ok(())
}
