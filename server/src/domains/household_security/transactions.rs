// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{error::OwnerError, types::Scope};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

pub async fn begin(pool: &PgPool, user: Uuid) -> Result<Transaction<'_, Postgres>, OwnerError> {
    let mut tx = begin_accounts(pool, &[user]).await?;
    let active: Option<bool> = sqlx::query_scalar(
        "SELECT status = 'active' AND deleted_at IS NULL FROM users WHERE id=$1",
    )
    .bind(user)
    .fetch_optional(&mut *tx)
    .await?;
    if active != Some(true) {
        return Err(OwnerError::InvalidProof);
    }
    Ok(tx)
}

pub async fn begin_accounts<'a>(
    pool: &'a PgPool,
    users: &[Uuid],
) -> Result<Transaction<'a, Postgres>, OwnerError> {
    let mut tx = pool.begin().await?;
    lock_accounts(&mut tx, users).await?;
    Ok(tx)
}

pub(crate) async fn lock_accounts(
    conn: &mut PgConnection,
    users: &[Uuid],
) -> Result<(), OwnerError> {
    sqlx::query("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *conn)
        .await?;
    sqlx::query("SET LOCAL statement_timeout = '10s'")
        .execute(&mut *conn)
        .await?;
    let locked: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE id=ANY($1) ORDER BY id FOR NO KEY UPDATE")
            .bind(users)
            .fetch_all(&mut *conn)
            .await?;
    for user in locked {
        sqlx::query(
            "INSERT INTO household_owner_state(user_id) VALUES ($1) ON CONFLICT DO NOTHING",
        )
        .bind(user)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn revoke_management(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
) -> Result<(), OwnerError> {
    invalidate(tx, user).await
}

pub async fn scope(tx: &mut Transaction<'_, Postgres>, actor: Scope) -> Result<(), OwnerError> {
    session_scope(tx, actor, true).await
}

async fn session_scope(
    tx: &mut Transaction<'_, Postgres>,
    actor: Scope,
    strict: bool,
) -> Result<(), OwnerError> {
    let standard:Option<bool>=sqlx::query_scalar("SELECT NOT $3 OR profile_type='standard' FROM user_profiles WHERE id=$1 AND owner_user_id=$2 FOR NO KEY UPDATE")
        .bind(actor.profile).bind(actor.user).bind(strict).fetch_optional(&mut **tx).await?;
    if standard != Some(true) {
        return Err(OwnerError::Required);
    }
    let valid:Option<bool>=sqlx::query_scalar("SELECT active_profile_id=$3 AND (NOT $4 OR NOT profile_selection_required) AND expires_at>clock_timestamp() AND credential_version=$5 FROM user_sessions WHERE id=$1 AND user_id=$2 FOR UPDATE")
        .bind(actor.session).bind(actor.user).bind(actor.profile).bind(strict).bind(actor.version).fetch_optional(&mut **tx).await?;
    if valid != Some(true) {
        return Err(OwnerError::InvalidProof);
    }
    Ok(())
}

pub async fn authorize(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    actor: Option<Scope>,
) -> Result<bool, OwnerError> {
    let enabled: bool =
        sqlx::query_scalar("SELECT enabled FROM household_owner_state WHERE user_id=$1")
            .bind(user)
            .fetch_one(&mut **tx)
            .await?;
    if let Some(actor) = actor {
        if actor.user != user {
            return Err(OwnerError::Required);
        }
        session_scope(tx, actor, enabled).await?;
        if enabled {
            require(tx, actor).await?;
        }
    } else if enabled {
        return Err(OwnerError::Required);
    }
    Ok(enabled)
}

pub async fn require(tx: &mut Transaction<'_, Postgres>, scope: Scope) -> Result<(), OwnerError> {
    let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM household_owner_grants g JOIN household_owner_state o ON o.user_id=g.user_id WHERE g.session_id=$1 AND g.user_id=$2 AND g.profile_id=$3 AND g.revision=o.revision AND g.credential_version=$4 AND o.enabled AND g.expires_at>clock_timestamp() AND (o.locked_until IS NULL OR o.locked_until<=clock_timestamp()))")
        .bind(scope.session).bind(scope.user).bind(scope.profile).bind(scope.version).fetch_one(&mut **tx).await?;
    if valid {
        Ok(())
    } else {
        Err(OwnerError::Required)
    }
}

pub(super) async fn available(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
) -> Result<i64, OwnerError> {
    let row = sqlx::query("SELECT revision, locked_until>clock_timestamp() AS locked FROM household_owner_state WHERE user_id=$1")
        .bind(user).fetch_one(&mut **tx).await?;
    if row.get::<Option<bool>, _>("locked") == Some(true) {
        return Err(OwnerError::Locked);
    }
    Ok(row.get("revision"))
}

pub(super) async fn invalidate(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
) -> Result<(), OwnerError> {
    sqlx::query("UPDATE household_owner_state SET revision=revision+1 WHERE user_id=$1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM household_owner_grants WHERE user_id=$1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM household_owner_challenges WHERE user_id=$1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(super) async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    actor: Option<Uuid>,
    event: &'static str,
) -> Result<(), OwnerError> {
    sqlx::query("INSERT INTO audit_log(table_name,row_id,operation,new_data,user_id,db_user,application_name) VALUES ('household_owner_state',$1,'UPDATE',jsonb_build_object('event',$2::text),$3,current_user,'duskcue-household-owner')")
        .bind(user).bind(event).bind(actor).execute(&mut **tx).await?;
    Ok(())
}

pub(super) async fn failed<T>(
    mut tx: Transaction<'_, Postgres>,
    scope: Scope,
) -> Result<T, OwnerError> {
    sqlx::query("UPDATE household_owner_state SET failed_attempts=LEAST(5,CASE WHEN locked_until<=clock_timestamp() THEN 1 ELSE failed_attempts+1 END), locked_until=CASE WHEN (CASE WHEN locked_until<=clock_timestamp() THEN 1 ELSE failed_attempts+1 END)>=5 THEN clock_timestamp()+interval '15 minutes' ELSE NULL END WHERE user_id=$1")
        .bind(scope.user).execute(&mut *tx).await?;
    audit(&mut tx, scope.user, Some(scope.user), "proof_refused").await?;
    tx.commit().await?;
    Err(OwnerError::InvalidProof)
}
