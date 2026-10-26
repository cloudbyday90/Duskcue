// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{error::CatalogError, limits::Budget};
use crate::{
    domains::{
        auth,
        household_security::types::Scope,
        live_tv::{
            assignment::secrets::FeedField,
            feed::source::{self, Fence},
        },
    },
    services::encryption::EncryptionKey,
};
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

pub(crate) struct Viewing {
    pub actor: Scope,
    pub profile_revision: i64,
    pub assignment_revision: i64,
    pub fence: Fence,
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    active_profile_id: Option<Uuid>,
    profile_selection_required: bool,
    credential_version: i64,
    profile_context_revision: i64,
    expires_at: DateTime<Utc>,
    last_active_at: DateTime<Utc>,
}

pub(crate) async fn session(
    conn: &mut PgConnection,
    actor: Scope,
    expected: Option<i64>,
    idle: Option<i32>,
) -> Result<i64, CatalogError> {
    let row:SessionRow=sqlx::query_as("SELECT active_profile_id,profile_selection_required,credential_version,profile_context_revision,expires_at,last_active_at FROM user_sessions WHERE id=$1 AND user_id=$2 FOR UPDATE")
        .bind(actor.session).bind(actor.user).fetch_optional(conn).await?.ok_or(CatalogError::Session)?;
    let now = Utc::now();
    if row.credential_version != actor.version
        || row.expires_at <= now
        || idle.is_some_and(|hours| {
            hours > 0 && (now - row.last_active_at).num_hours() >= i64::from(hours)
        })
    {
        return Err(CatalogError::Session);
    }
    if row.profile_selection_required {
        return Err(CatalogError::Forbidden);
    }
    if row.active_profile_id != Some(actor.profile)
        || expected.is_some_and(|revision| revision != row.profile_context_revision)
    {
        return Err(CatalogError::Changed);
    }
    if row.profile_context_revision <= 0 {
        return Err(CatalogError::Unavailable);
    }
    Ok(row.profile_context_revision)
}

pub(crate) async fn viewing(
    conn: &mut PgConnection,
    actor: Scope,
    key: &EncryptionKey,
    idle: Option<i32>,
    budget: &mut Budget,
) -> Result<Viewing, CatalogError> {
    let row=sqlx::query("SELECT role,status='active' AND deleted_at IS NULL AS active FROM users WHERE id=$1 FOR NO KEY UPDATE").bind(actor.user).fetch_optional(&mut *conn).await?.ok_or(CatalogError::Session)?;
    if !row.try_get::<bool, _>("active")? {
        return Err(CatalogError::Forbidden);
    }
    let role: String = row.try_get("role")?;
    let standard:Option<bool>=sqlx::query_scalar("SELECT profile_type='standard' FROM user_profiles WHERE id=$1 AND owner_user_id=$2 FOR NO KEY UPDATE").bind(actor.profile).bind(actor.user).fetch_optional(&mut *conn).await?;
    if standard != Some(true) {
        return Err(CatalogError::Forbidden);
    }
    let profile_revision = session(conn, actor, None, idle).await?;
    let capabilities = auth::service::resolve_capabilities_on_connection(conn, actor.user, &role)
        .await
        .map_err(|_| CatalogError::Unavailable)?;
    if !capabilities
        .iter()
        .any(|capability| capability == "can_use_live_tv")
    {
        return Err(CatalogError::Forbidden);
    }
    let row=sqlx::query("SELECT a.id,a.revision,a.source_generation,a.enabled,a.playlist_destination IS NOT NULL AND a.guide_destination IS NOT NULL AS approved,COALESCE(p.enabled,false) AS profile_enabled FROM live_tv_assignments a LEFT JOIN live_tv_profile_access p ON p.assignment_id=a.id AND p.household_user_id=a.household_user_id AND p.profile_id=$2 WHERE a.household_user_id=$1 FOR UPDATE OF a")
        .bind(actor.user).bind(actor.profile).fetch_optional(&mut *conn).await?.ok_or(CatalogError::Forbidden)?;
    if !row.try_get::<bool, _>("enabled")?
        || !row.try_get::<bool, _>("approved")?
        || !row.try_get::<bool, _>("profile_enabled")?
    {
        return Err(CatalogError::Forbidden);
    }
    let fence = Fence {
        user: actor.user,
        assignment: row.try_get("id")?,
        generation: row.try_get("source_generation")?,
    };
    let assignment_revision = row.try_get("revision")?;
    for feed in [FeedField::Playlist, FeedField::Guide] {
        let query = match feed {
            FeedField::Playlist => {
                "SELECT id,source_generation,playlist_secret AS secret,playlist_destination AS destination,playlist_address_count AS count FROM live_tv_assignments WHERE id=$1 AND household_user_id=$2"
            }
            FeedField::Guide => {
                "SELECT id,source_generation,guide_secret AS secret,guide_destination AS destination,guide_address_count AS count FROM live_tv_assignments WHERE id=$1 AND household_user_id=$2"
            }
        };
        let row = sqlx::query(query)
            .bind(fence.assignment)
            .bind(actor.user)
            .fetch_one(&mut *conn)
            .await?;
        budget.cipher(
            row.try_get::<String, _>("secret")?.len()
                + row
                    .try_get::<Option<String>, _>("destination")?
                    .as_ref()
                    .map_or(0, String::len),
        )?;
        budget.plain(8192)?;
        let source =
            source::decode(row, key, actor.user, feed, 0).map_err(|_| CatalogError::Unavailable)?;
        if source.fence != fence {
            return Err(CatalogError::Unavailable);
        }
    }
    Ok(Viewing {
        actor,
        profile_revision,
        assignment_revision,
        fence,
    })
}
