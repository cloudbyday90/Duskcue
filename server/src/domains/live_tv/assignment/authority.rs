// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::error::AssignmentError;
use crate::{
    domains::household_security::{transactions, types::Scope},
    error::AppError,
};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub(super) async fn administrator(
    pool: &PgPool,
    actor: Scope,
    target: Uuid,
    write: bool,
) -> Result<Transaction<'_, Postgres>, AppError> {
    let mut tx = transactions::begin_accounts(pool, &[actor.user, target]).await?;
    if !crate::domains::household_security::access::allowed(&mut tx, actor, "can_manage_users")
        .await?
    {
        return Err(AssignmentError::Forbidden.into());
    }
    if write {
        transactions::require(&mut tx, actor).await?;
    }
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id=$1 AND deleted_at IS NULL)")
            .bind(target)
            .fetch_one(&mut *tx)
            .await
            .map_err(AssignmentError::from)?;
    if !exists {
        return Err(AssignmentError::NotFound.into());
    }
    Ok(tx)
}

pub(super) async fn owner(
    pool: &PgPool,
    actor: Scope,
) -> Result<Transaction<'_, Postgres>, AppError> {
    let mut tx = transactions::begin(pool, actor.user).await?;
    transactions::scope(&mut tx, actor).await?;
    transactions::require(&mut tx, actor).await?;
    Ok(tx)
}

pub(super) async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: Scope,
    assignment: Uuid,
    household: Uuid,
    event: &'static str,
) -> Result<(), AssignmentError> {
    sqlx::query("INSERT INTO audit_log(table_name,row_id,operation,new_data,user_id,db_user,application_name) VALUES ('live_tv_assignments',$1,'UPDATE',jsonb_build_object('event',$2::text,'household_user_id',$4::uuid),$3,current_user,'duskcue-live-tv-assignment')")
        .bind(assignment).bind(event).bind(actor.user).bind(household).execute(&mut **tx).await?;
    Ok(())
}
