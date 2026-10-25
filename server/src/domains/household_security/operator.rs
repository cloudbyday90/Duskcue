// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{error::OwnerError, transactions};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ring::{
    digest::{SHA256, digest},
    rand::{SecureRandom, SystemRandom},
};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub fn fingerprint(value: &[u8]) -> String {
    digest(&SHA256, value)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub async fn issue_handoff(pool: &PgPool, user: Uuid) -> Result<String, OwnerError> {
    let mut bytes = [0; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| OwnerError::Unavailable)?;
    let token = format!("duskcue-owner-{}", URL_SAFE_NO_PAD.encode(bytes));
    let mut tx = transactions::begin(pool, user).await?;
    transactions::invalidate(&mut tx, user).await?;
    sqlx::query("UPDATE user_passkeys SET owner_trusted=false WHERE user_id=$1 AND owner_trusted")
        .bind(user)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE household_owner_state SET password_fingerprint=NULL,failed_attempts=0,locked_until=NULL WHERE user_id=$1")
        .bind(user).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM household_owner_handoffs WHERE user_id=$1")
        .bind(user)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO household_owner_handoffs(user_id,token_hash,expires_at) VALUES ($1,$2,clock_timestamp()+interval '10 minutes')")
        .bind(user).bind(fingerprint(token.as_bytes())).execute(&mut *tx).await?;
    transactions::audit(&mut tx, user, None, "operator_handoff_issued").await?;
    tx.commit().await?;
    Ok(token)
}

pub(super) async fn validate(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    token: &str,
) -> Result<Uuid, OwnerError> {
    if token.len() != 57 || !token.starts_with("duskcue-owner-") {
        return Err(OwnerError::InvalidProof);
    }
    sqlx::query_scalar("SELECT id FROM household_owner_handoffs WHERE user_id=$1 AND token_hash=$2 AND expires_at>clock_timestamp()")
        .bind(user).bind(fingerprint(token.as_bytes())).fetch_optional(&mut **tx).await?.ok_or(OwnerError::InvalidProof)
}

pub(super) async fn consume(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    handoff: Uuid,
) -> Result<(), OwnerError> {
    if sqlx::query("DELETE FROM household_owner_handoffs WHERE id=$1 AND user_id=$2 AND expires_at>clock_timestamp()")
        .bind(handoff).bind(user).execute(&mut **tx).await?.rows_affected()!=1 { return Err(OwnerError::InvalidProof); }
    Ok(())
}

pub async fn run(bootstrap: &crate::config::BootstrapConfig, user: Uuid) -> Result<(), OwnerError> {
    let url = bootstrap
        .database_url
        .as_deref()
        .ok_or(OwnerError::Unavailable)?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(url)
        .await?;
    let result = issue_handoff(&pool, user).await;
    pool.close().await;
    let token = result?;
    println!("{token}");
    Ok(())
}
