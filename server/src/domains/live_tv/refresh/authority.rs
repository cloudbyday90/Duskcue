// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{error::RefreshError, types::ControlResponse};
use crate::{
    domains::household_security::{access, transactions, types::Scope},
    error::AppError,
};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(crate) struct Initiator {
    pub actor: Scope,
    pub owner_revision: i64,
    pub policy_revision: i64,
}

pub(super) async fn authorize(
    conn: &mut PgConnection,
    actor: Scope,
    target: Uuid,
    server: bool,
    fresh: bool,
) -> Result<(), AppError> {
    transactions::lock_accounts(conn, &[actor.user, target]).await?;
    if !access::allowed(
        conn,
        actor,
        if server {
            "can_manage_server"
        } else {
            "can_manage_users"
        },
    )
    .await?
    {
        return Err(RefreshError::Forbidden.into());
    }
    if fresh {
        transactions::require_conn(conn, actor).await?;
    }
    Ok(())
}
pub(super) async fn control(
    conn: &mut PgConnection,
    write: bool,
) -> Result<ControlResponse, AppError> {
    let sql = if write {
        "SELECT id,revision,enabled FROM live_tv_refresh_control WHERE singleton FOR UPDATE"
    } else {
        "SELECT id,revision,enabled FROM live_tv_refresh_control WHERE singleton FOR SHARE"
    };
    sqlx::query_as::<_, (Uuid, i64, bool)>(sql)
        .fetch_optional(conn)
        .await
        .map_err(|_| RefreshError::Unavailable)?
        .map(|(id, revision, enabled)| ControlResponse {
            id,
            revision,
            enabled,
        })
        .ok_or_else(|| RefreshError::Unavailable.into())
}

impl Initiator {
    pub async fn locked(
        self,
        conn: &mut PgConnection,
        target: Uuid,
    ) -> Result<(), super::super::feed::imports::ImportError> {
        let work = async {
            authorize(conn, self.actor, target, false, false).await?;
            let policy = control(conn, false).await?;
            if !policy.enabled || policy.revision != self.policy_revision {
                return Err(RefreshError::Forbidden.into());
            }
            let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM household_owner_state WHERE user_id=$1 AND enabled AND revision=$2 AND (locked_until IS NULL OR locked_until<=clock_timestamp()))")
                .bind(self.actor.user).bind(self.owner_revision).fetch_one(conn).await.map_err(|_| RefreshError::Unavailable)?;
            if !valid {
                return Err(RefreshError::Forbidden.into());
            }
            Ok::<_, AppError>(())
        }.await;
        work.map_err(|_| super::super::feed::imports::ImportError::Authority)
    }
    pub async fn current(
        self,
        conn: &mut PgConnection,
    ) -> Result<(), super::super::feed::imports::ImportError> {
        let row=sqlx::query("SELECT u.role,EXISTS(SELECT 1 FROM user_capabilities c WHERE c.user_id=u.id AND c.capability='can_manage_users' AND c.is_granted) AS granted,EXISTS(SELECT 1 FROM user_capabilities c WHERE c.user_id=u.id AND c.capability='can_manage_users' AND NOT c.is_granted) AS denied FROM users u JOIN user_sessions s ON s.user_id=u.id JOIN user_profiles p ON p.id=s.active_profile_id AND p.owner_user_id=u.id JOIN household_owner_state o ON o.user_id=u.id CROSS JOIN live_tv_refresh_control r WHERE u.id=$1 AND u.status='active' AND u.deleted_at IS NULL AND s.id=$2 AND s.active_profile_id=$3 AND s.credential_version=$4 AND s.expires_at>clock_timestamp() AND NOT s.profile_selection_required AND p.profile_type='standard' AND o.enabled AND o.revision=$5 AND (o.locked_until IS NULL OR o.locked_until<=clock_timestamp()) AND r.singleton AND r.enabled AND r.revision=$6")
            .bind(self.actor.user).bind(self.actor.session).bind(self.actor.profile).bind(self.actor.version).bind(self.owner_revision).bind(self.policy_revision).fetch_optional(conn).await.map_err(|_|super::super::feed::imports::ImportError::Authority)?;
        let valid = row.is_some_and(|r| {
            r.get::<String, _>("role") == "owner"
                || r.get::<bool, _>("granted")
                || (r.get::<String, _>("role") == "admin" && !r.get::<bool, _>("denied"))
        });
        if valid {
            Ok(())
        } else {
            Err(super::super::feed::imports::ImportError::Authority)
        }
    }
}
