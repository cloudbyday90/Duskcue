// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::{
    error::CommandError,
    handlers::{scope, target},
};
use super::{
    checkpoint, database, service,
    types::{ChangeRequest, CheckpointRequest},
};
use crate::{extractors::AuthenticatedUser, state::AppState};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, Request, State, rejection::PathRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use tokio_stream::StreamExt;
use uuid::Uuid;

async fn body(request: Request, limit: usize) -> Result<Bytes, CommandError> {
    let headers = request.headers();
    if headers.get_all(header::CONTENT_TYPE).iter().count() != 1
        || !headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .is_some_and(|v| v.trim() == "application/json")
    {
        return Err(CommandError::MediaType);
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async move {
        let mut chunks = request.into_body().into_data_stream();
        let mut value = Vec::new();
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk.map_err(|_| CommandError::Invalid)?;
            if chunk.len() > limit.saturating_sub(value.len()) {
                return Err(CommandError::TooLarge);
            }
            value.extend_from_slice(&chunk);
        }
        Ok(Bytes::from(value))
    })
    .await
    .map_err(|_| CommandError::Invalid)?
}

fn condition(headers: &HeaderMap) -> Result<String, CommandError> {
    if headers.get_all(header::IF_MATCH).iter().count() == 0 {
        return Err(CommandError::ConditionRequired);
    }
    if headers.get_all(header::IF_MATCH).iter().count() != 1 {
        return Err(CommandError::Invalid);
    }
    let value = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .ok_or(CommandError::Invalid)?
        .trim();
    let digest = value
        .strip_prefix("\"vod-config-")
        .and_then(|v| v.strip_suffix('"'))
        .ok_or(CommandError::Invalid)?;
    if !crate::domains::playback::source_context::valid_revision(digest) {
        return Err(CommandError::Invalid);
    }
    Ok(value.into())
}

fn json<T: serde::Serialize>(value: T) -> Result<Response, CommandError> {
    if serde_json::to_vec(&value)
        .map_err(|_| CommandError::Unavailable)?
        .len()
        > 32768
    {
        return Err(CommandError::Unavailable);
    }
    Ok(Json(value).into_response())
}

pub(in crate::domains::playback::commands) async fn configuration(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, CommandError> {
    let scope = scope(&user, &headers)?;
    let id = target(path)?;
    let (owner, request, viewer) = database::selected(&state, scope, id).await?;
    let (value, etag) = database::configuration(&state, &owner, request, &viewer).await?;
    let mut response = json(value)?;
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&etag).map_err(|_| CommandError::Unavailable)?,
    );
    Ok(response)
}

pub(in crate::domains::playback::commands) async fn change(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
    request: Request,
) -> Result<Response, CommandError> {
    let scope = scope(&user, request.headers())?;
    let id = target(path)?;
    let condition = condition(request.headers())?;
    let input: ChangeRequest =
        serde_json::from_slice(&body(request, 32768).await?).map_err(|_| CommandError::Invalid)?;
    let value = service::change(&state, scope, id, input, condition).await?;
    let location = value.status_url.clone();
    let mut response = json(value)?;
    *response.status_mut() = StatusCode::ACCEPTED;
    response.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(&location).map_err(|_| CommandError::Unavailable)?,
    );
    Ok(response)
}

pub(in crate::domains::playback::commands) async fn operation(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, CommandError> {
    let (id, operation) = target(path)?;
    json(database::status(&state, scope(&user, &headers)?, id, operation, false).await?)
}

pub(in crate::domains::playback::commands) async fn retirement(
    State(state): State<AppState>,
    super::super::cleanup_auth::CleanupActor(scope): super::super::cleanup_auth::CleanupActor,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, CommandError> {
    let (id, operation) = target(path)?;
    json(database::status(&state, scope, id, operation, true).await?)
}

pub(in crate::domains::playback::commands) async fn checkpoint(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
    request: Request,
) -> Result<Response, CommandError> {
    let scope = scope(&user, request.headers())?;
    let id = target(path)?;
    let input: CheckpointRequest =
        serde_json::from_slice(&body(request, 1024).await?).map_err(|_| CommandError::Invalid)?;
    json(checkpoint::report(&state, scope, id, input).await?)
}

pub(in crate::domains::playback::commands) async fn read_checkpoint(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, CommandError> {
    json(checkpoint::read(&state, scope(&user, &headers)?, target(path)?).await?)
}
