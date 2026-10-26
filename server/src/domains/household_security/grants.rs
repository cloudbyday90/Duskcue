// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    error::OwnerError,
    operator::fingerprint,
    transactions,
    types::{Proven, Scope},
};
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};

pub(super) async fn rotate(
    tx: &mut Transaction<'_, Postgres>,
    scope: Scope,
) -> Result<Proven, OwnerError> {
    transactions::scope(tx, scope).await?;
    let token = crate::domains::auth::service::generate_session_token();
    sqlx::query("DELETE FROM household_owner_challenges WHERE session_id=$1")
        .bind(scope.session)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM household_owner_grants WHERE session_id=$1")
        .bind(scope.session)
        .execute(&mut **tx)
        .await?;
    let session_expires_at: DateTime<Utc> = sqlx::query_scalar("UPDATE user_sessions SET token_hash=$2,credential_version=credential_version+1,last_active_at=clock_timestamp() WHERE id=$1 RETURNING expires_at")
        .bind(scope.session).bind(fingerprint(token.as_bytes())).fetch_one(&mut **tx).await?;
    let expires_at=sqlx::query_scalar("INSERT INTO household_owner_grants(session_id,credential_version,user_id,profile_id,revision,expires_at) SELECT s.id,s.credential_version,$2,$3,o.revision,LEAST(clock_timestamp()+interval '5 minutes',$4) FROM household_owner_state o JOIN user_sessions s ON s.user_id=o.user_id WHERE s.id=$1 AND o.user_id=$2 RETURNING household_owner_grants.expires_at")
        .bind(scope.session).bind(scope.user).bind(scope.profile).bind(session_expires_at).fetch_one(&mut **tx).await?;
    sqlx::query(
        "UPDATE household_owner_state SET failed_attempts=0,locked_until=NULL WHERE user_id=$1",
    )
    .bind(scope.user)
    .execute(&mut **tx)
    .await?;
    transactions::audit(tx, scope.user, Some(scope.user), "owner_proof_verified").await?;
    let context = super::context::read(
        tx,
        Scope {
            version: scope
                .version
                .checked_add(1)
                .ok_or(OwnerError::Unavailable)?,
            binding: None,
            ..scope
        },
    )
    .await?;
    Ok(Proven {
        context,
        token,
        expires_at,
        session_expires_at,
    })
}
