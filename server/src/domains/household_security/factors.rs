// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{error::OwnerError, operator::fingerprint, transactions};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
use webauthn_rs::prelude::{AuthenticationResult, Passkey};

pub fn validate_client_context(bytes: &[u8]) -> Result<(), OwnerError> {
    if bytes.len() > 16384 {
        return Err(OwnerError::InvalidProof);
    }
    let data: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| OwnerError::InvalidProof)?;
    if !data.is_object()
        || data.get("topOrigin").is_some()
        || data
            .get("crossOrigin")
            .is_some_and(|value| value.as_bool() != Some(false))
    {
        return Err(OwnerError::InvalidProof);
    }
    Ok(())
}

pub(super) async fn activate(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
) -> Result<(), OwnerError> {
    transactions::invalidate(tx, user).await?;
    sqlx::query("UPDATE household_owner_state SET enabled=true WHERE user_id=$1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    transactions::audit(tx, user, Some(user), "owner_factor_designated").await
}

pub(super) async fn password_available(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
) -> Result<bool, OwnerError> {
    let row = sqlx::query("SELECT u.password_hash,o.password_fingerprint FROM users u JOIN household_owner_state o ON o.user_id=u.id WHERE u.id=$1")
        .bind(user).fetch_one(&mut **tx).await?;
    let password: Option<String> = row.get("password_hash");
    let trusted: Option<String> = row.get("password_fingerprint");
    Ok(password.is_some_and(|p| trusted.as_deref() == Some(fingerprint(p.as_bytes()).as_str())))
}

pub(super) async fn load(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    selected: Option<Uuid>,
) -> Result<Vec<(Uuid, Passkey, String)>, OwnerError> {
    let rows = sqlx::query("SELECT id,public_key FROM user_passkeys WHERE user_id=$1 AND octet_length(public_key)<=65536 AND (($2::uuid IS NOT NULL AND id=$2) OR ($2 IS NULL AND owner_trusted)) ORDER BY id LIMIT 65")
        .bind(user).bind(selected).fetch_all(&mut **tx).await?;
    if rows.is_empty() || rows.len() > 64 {
        return Err(OwnerError::InvalidProof);
    }
    rows.iter()
        .map(|row| {
            let bytes: Vec<u8> = row.get("public_key");
            Ok((
                row.get("id"),
                serde_json::from_slice(&bytes).map_err(|_| OwnerError::Unavailable)?,
                fingerprint(&bytes),
            ))
        })
        .collect()
}

pub(super) async fn update(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    id: Uuid,
    mut passkey: Passkey,
    result: &AuthenticationResult,
    trusted: bool,
) -> Result<(), OwnerError> {
    let old: i64 = sqlx::query_scalar(
        "SELECT sign_count FROM user_passkeys WHERE id=$1 AND user_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(user)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(OwnerError::InvalidProof)?;
    if !result.user_verified()
        || (result.counter() != 0 && old != 0 && i64::from(result.counter()) <= old)
    {
        return Err(OwnerError::InvalidProof);
    }
    passkey.update_credential(result);
    let bytes = serde_json::to_vec(&passkey).map_err(|_| OwnerError::Unavailable)?;
    sqlx::query("UPDATE user_passkeys SET public_key=$3,sign_count=$4,last_used_at=clock_timestamp(),owner_trusted=owner_trusted OR $5 WHERE id=$1 AND user_id=$2")
        .bind(id).bind(user).bind(bytes).bind(i64::from(result.counter())).bind(trusted).execute(&mut **tx).await?;
    Ok(())
}

pub async fn trusted_change(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
) -> Result<(), OwnerError> {
    transactions::invalidate(tx, user).await?;
    transactions::audit(tx, user, Some(user), "owner_factors_changed").await
}

pub async fn ensure_enrollment_capacity(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
) -> Result<(), OwnerError> {
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM user_passkeys WHERE user_id=$1 AND owner_trusted")
            .bind(user)
            .fetch_one(&mut **tx)
            .await?;
    if count >= 64 {
        Err(OwnerError::Locked)
    } else {
        Ok(())
    }
}

pub async fn allow_delete(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    id: Uuid,
) -> Result<bool, OwnerError> {
    let trusted: bool =
        sqlx::query_scalar("SELECT owner_trusted FROM user_passkeys WHERE id=$1 AND user_id=$2")
            .bind(id)
            .bind(user)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(OwnerError::InvalidProof)?;
    if trusted && !password_available(tx, user).await? {
        let others: Vec<Vec<u8>> = sqlx::query_scalar(
            "SELECT public_key FROM user_passkeys WHERE user_id=$1 AND owner_trusted AND id<>$2 AND octet_length(public_key)<=65536 LIMIT 65",
        )
        .bind(user)
        .bind(id)
        .fetch_all(&mut **tx)
        .await?;
        if !others
            .iter()
            .any(|bytes| serde_json::from_slice::<Passkey>(bytes).is_ok())
        {
            return Err(OwnerError::LastFactor);
        }
    }
    Ok(trusted)
}
