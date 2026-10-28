// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    delivery,
    error::CommandError,
    service,
    types::{BODY_LIMIT, StartRequest, StatusResponse},
};
use crate::{
    domains::household_security::{context, types::Scope},
    extractors::AuthenticatedUser,
    state::AppState,
};
use axum::{
    Json,
    body::{Bytes, to_bytes},
    extract::{Path, Request, State, rejection::PathRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use uuid::Uuid;

pub(super) fn scope(user: &AuthenticatedUser, headers: &HeaderMap) -> Result<Scope, CommandError> {
    context::scope(user, headers).map_err(|_| CommandError::Conflict)
}
pub(super) fn target<T>(path: Result<Path<T>, PathRejection>) -> Result<T, CommandError> {
    path.map(|Path(value)| value)
        .map_err(|_| CommandError::Invalid)
}
fn json_type(headers: &HeaderMap) -> bool {
    headers.get_all(header::CONTENT_TYPE).iter().count() == 1
        && headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.split(';')
                    .next()
                    .is_some_and(|v| v.trim() == "application/json")
            })
}
async fn bytes(request: Request) -> Result<Bytes, CommandError> {
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        to_bytes(request.into_body(), BODY_LIMIT),
    )
    .await
    .map_err(|_| CommandError::Invalid)?
    .map_err(|_| CommandError::Invalid)
}
async fn empty(request: Request) -> Result<(), CommandError> {
    let json = json_type(request.headers());
    if !json {
        return Err(CommandError::Invalid);
    }
    let bytes = bytes(request).await?;
    if bytes.is_empty() {
        return Ok(());
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Empty {}
    serde_json::from_slice::<Empty>(&bytes).map_err(|_| CommandError::Invalid)?;
    Ok(())
}
fn accepted(
    value: StatusResponse,
) -> Result<(StatusCode, HeaderMap, Json<StatusResponse>), CommandError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::LOCATION,
        HeaderValue::from_str(&value.status_url).map_err(|_| CommandError::Unavailable)?,
    );
    Ok((StatusCode::ACCEPTED, headers, Json(value)))
}

pub async fn start(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    request: Request,
) -> Result<(StatusCode, HeaderMap, Json<StatusResponse>), CommandError> {
    let actor = scope(&user, request.headers())?;
    if !json_type(request.headers()) {
        return Err(CommandError::Invalid);
    }
    let body: StartRequest =
        serde_json::from_slice(&bytes(request).await?).map_err(|_| CommandError::Invalid)?;
    accepted(service::start(&state, actor, body).await?)
}
pub async fn status(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<StatusResponse>, CommandError> {
    Ok(Json(
        service::status(&state, scope(&user, &headers)?, target(path)?).await?,
    ))
}
pub async fn renew(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
    request: Request,
) -> Result<Json<super::types::RenewalResponse>, CommandError> {
    let actor = scope(&user, request.headers())?;
    let id = target(path)?;
    empty(request).await?;
    Ok(Json(service::renew(&state, actor, id).await?))
}
pub async fn stop(
    State(state): State<AppState>,
    super::cleanup_auth::CleanupActor(actor): super::cleanup_auth::CleanupActor,
    path: Result<Path<Uuid>, PathRejection>,
    request: Request,
) -> Result<(StatusCode, HeaderMap, Json<StatusResponse>), CommandError> {
    let id = target(path)?;
    empty(request).await?;
    accepted(service::stop(&state, actor, id).await?)
}
pub async fn retirement(
    State(state): State<AppState>,
    super::cleanup_auth::CleanupActor(actor): super::cleanup_auth::CleanupActor,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<StatusResponse>, CommandError> {
    Ok(Json(
        service::retirement(&state, actor, target(path)?).await?,
    ))
}
pub async fn file(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, CommandError> {
    let (id, output) = target(path)?;
    delivery::file(&state, scope(&user, &headers)?, id, output, &headers).await
}
pub async fn manifest(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, CommandError> {
    let (id, output) = target(path)?;
    delivery::hls(&state, scope(&user, &headers)?, id, output, None, None).await
}
pub async fn playlist(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid, String)>, PathRejection>,
) -> Result<Response, CommandError> {
    let (id, output, rendition) = target(path)?;
    delivery::hls(
        &state,
        scope(&user, &headers)?,
        id,
        output,
        Some(&rendition),
        None,
    )
    .await
}
pub async fn segment(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid, String, String)>, PathRejection>,
) -> Result<Response, CommandError> {
    let (id, output, rendition, segment) = target(path)?;
    delivery::hls(
        &state,
        scope(&user, &headers)?,
        id,
        output,
        Some(&rendition),
        Some(&segment),
    )
    .await
}
