// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{error::OwnerError, types::Scope};
use crate::{
    domains::auth::{error::AuthError, service::session_context},
    extractors::AuthenticatedUser,
};
use axum::http::HeaderMap;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sqlx::PgConnection;

pub fn scope(user: &AuthenticatedUser, headers: &HeaderMap) -> Result<Scope, OwnerError> {
    let mut scope = Scope::from(user);
    let mut values = headers.get_all("x-session-context").iter();
    if let Some(value) = values.next() {
        if values.next().is_some() {
            return Err(OwnerError::InvalidRequest);
        }
        let value = value.to_str().map_err(|_| OwnerError::InvalidRequest)?;
        let digest = value
            .strip_prefix("sc1.")
            .filter(|_| value.len() == 47)
            .ok_or(OwnerError::InvalidRequest)?;
        let bytes = URL_SAFE_NO_PAD
            .decode(digest)
            .map_err(|_| OwnerError::InvalidRequest)?;
        scope.binding = Some(bytes.try_into().map_err(|_| OwnerError::InvalidRequest)?);
    }
    Ok(scope)
}

pub async fn read(
    conn: &mut PgConnection,
    scope: Scope,
) -> Result<session_context::SessionContextResponse, OwnerError> {
    session_context::read(
        conn,
        scope.user,
        scope.session,
        scope.profile,
        scope.version,
        scope.transport,
    )
    .await
    .map_err(|e| match e {
        AuthError::Database(e) => OwnerError::Database(e),
        _ => OwnerError::ContextChanged,
    })
}

pub async fn check(conn: &mut PgConnection, scope: Scope) -> Result<(), OwnerError> {
    if let Some(binding) = scope.binding {
        let context = read(conn, scope).await?;
        if context.context_id != format!("sc1.{}", URL_SAFE_NO_PAD.encode(binding)) {
            return Err(OwnerError::ContextChanged);
        }
    }
    Ok(())
}
