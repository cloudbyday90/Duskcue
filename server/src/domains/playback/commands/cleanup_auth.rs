// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::error::CommandError;
use crate::{
    domains::{auth::AuthError, household_security::types::Scope},
    error::AppError,
    extractors::extract_session_token,
    state::AppState,
};
use axum::{
    extract::FromRequestParts,
    http::request::Parts,
    response::{IntoResponse, Response},
};
use uuid::Uuid;

pub struct CleanupActor(pub(super) Scope);

impl FromRequestParts<AppState> for CleanupActor {
    type Rejection = Response;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let (token, transport) =
            extract_session_token(parts).map_err(IntoResponse::into_response)?;
        let hash = ring::digest::digest(&ring::digest::SHA256, token.as_bytes())
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let owner = state
            .playback_admission
            .get()
            .ok_or_else(|| CommandError::Unavailable.into_response())?;
        let idle = state.runtime_config.load().auth.session_idle_timeout_hours;
        let row:Option<(Uuid,Uuid,Option<Uuid>,i64)>=owner.sql(true,move|conn|Box::pin(async move {
            Ok(sqlx::query_as("SELECT s.id,s.user_id,s.active_profile_id,s.credential_version FROM user_sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=$1 AND s.expires_at>clock_timestamp() AND ($2::int IS NULL OR $2<=0 OR s.last_active_at>clock_timestamp()-make_interval(hours=>$2)) AND u.status='active' AND u.deleted_at IS NULL")
                .bind(hash).bind(idle).fetch_optional(conn).await?)
        })).await.map_err(|error|CommandError::from(error).into_response())?;
        let (session, user, profile, version) =
            row.ok_or_else(|| AppError::Auth(AuthError::SessionExpired).into_response())?;
        Ok(Self(Scope {
            session,
            user,
            profile: profile.unwrap_or(Uuid::nil()),
            version,
            transport,
            binding: None,
        }))
    }
}
