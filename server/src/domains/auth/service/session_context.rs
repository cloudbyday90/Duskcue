// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use crate::extractors::{AuthenticatedUser, CredentialTransport};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use ring::digest::{Context, SHA256};
use serde::Serialize;
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use super::AuthError;

#[derive(FromRow)]
struct SessionContextRow {
    credential_version: i64,
    profile_context_revision: i64,
    profile_selection_required: bool,
    expires_at: DateTime<Utc>,
    observed_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
pub struct SessionContextResponse {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub profile_id: Uuid,
    pub credential_revision: String,
    pub profile_context_revision: String,
    pub profile_selection_required: bool,
    pub transport: CredentialTransport,
    pub session_expires_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub context_id: String,
}

pub async fn current(
    pool: &PgPool,
    actor: &AuthenticatedUser,
) -> Result<SessionContextResponse, AuthError> {
    let work = async {
        let mut conn = pool.acquire().await?;
        conn.close_on_drop();
        let result = read(
            &mut conn,
            actor.user_id,
            actor.session_id,
            actor.profile_id,
            actor.credential_version,
            actor.transport,
        )
        .await;
        conn.close().await?;
        result
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), work)
        .await
        .map_err(|_| AuthError::Database(sqlx::Error::PoolTimedOut))?
}

pub async fn read(
    conn: &mut PgConnection,
    user: Uuid,
    session: Uuid,
    profile: Uuid,
    version: i64,
    transport: CredentialTransport,
) -> Result<SessionContextResponse, AuthError> {
    let row = sqlx::query_as::<_, SessionContextRow>("SELECT s.credential_version,s.profile_context_revision,s.profile_selection_required,s.expires_at,clock_timestamp() AS observed_at FROM user_sessions s JOIN users u ON u.id=s.user_id JOIN user_profiles p ON p.id=s.active_profile_id AND p.owner_user_id=u.id WHERE s.id=$1 AND s.user_id=$2 AND s.active_profile_id=$3 AND s.credential_version=$4 AND s.expires_at>clock_timestamp() AND u.status='active' AND u.deleted_at IS NULL")
        .bind(session).bind(user).bind(profile).bind(version).fetch_optional(conn).await?.ok_or(AuthError::SessionExpired)?;
    let mut hash = Context::new(&SHA256);
    hash.update(b"duskcue-session-context:1");
    for id in [user, session, profile] {
        hash.update(id.as_bytes());
    }
    hash.update(&row.credential_version.to_be_bytes());
    hash.update(&row.profile_context_revision.to_be_bytes());
    hash.update(&[u8::from(row.profile_selection_required)]);
    let context_id = format!("sc1.{}", URL_SAFE_NO_PAD.encode(hash.finish()));
    Ok(SessionContextResponse {
        user_id: user,
        session_id: session,
        profile_id: profile,
        credential_revision: row.credential_version.to_string(),
        profile_context_revision: row.profile_context_revision.to_string(),
        profile_selection_required: row.profile_selection_required,
        transport,
        session_expires_at: row.expires_at,
        observed_at: row.observed_at,
        context_id,
    })
}
