// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{admission, error::RefreshError, representation, service, types::*};
use crate::{
    domains::{household_security::service::request_scope, live_tv::assignment::preconditions},
    error::AppError,
    extractors::{CanManageServer, CanManageUsers, Require},
    state::AppState,
};
use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, HeaderValue, StatusCode, header},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn body<T>(value: Result<Json<T>, JsonRejection>) -> Result<T, AppError> {
    value.map(|Json(v)| v).map_err(|e| match e.status() {
        StatusCode::UNSUPPORTED_MEDIA_TYPE => AppError::UnsupportedMediaType,
        StatusCode::PAYLOAD_TOO_LARGE => AppError::PayloadTooLarge,
        _ => RefreshError::Invalid.into(),
    })
}
fn target(value: Result<Path<Uuid>, PathRejection>) -> Result<Uuid, AppError> {
    value
        .map(|Path(v)| v)
        .map_err(|_| RefreshError::Invalid.into())
}
fn response<T: serde::Serialize>(value: T) -> Result<(HeaderMap, Json<T>), AppError> {
    let mut h = HeaderMap::new();
    h.insert(
        header::ETAG,
        HeaderValue::from_str(&representation::etag(&value)?)
            .map_err(|_| RefreshError::Unavailable)?,
    );
    Ok((h, Json(value)))
}

pub async fn control(
    headers: HeaderMap,
    auth: Require<CanManageServer>,
    State(state): State<AppState>,
) -> Result<(HeaderMap, Json<ControlResponse>), AppError> {
    response(service::control(&state, request_scope(&auth.user, &headers)?).await?)
}
pub async fn configure(
    auth: Require<CanManageServer>,
    State(state): State<AppState>,
    headers: HeaderMap,
    input: Result<Json<ControlRequest>, JsonRejection>,
) -> Result<(HeaderMap, Json<ControlResponse>), AppError> {
    response(
        service::configure(
            &state,
            request_scope(&auth.user, &headers)?,
            preconditions::condition(&headers, false)?,
            body(input)?,
        )
        .await?,
    )
}
pub async fn resource(
    headers: HeaderMap,
    auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<(HeaderMap, Json<ResourceResponse>), AppError> {
    response(service::resource(&state, request_scope(&auth.user, &headers)?, target(path)?).await?)
}
pub async fn status(
    headers: HeaderMap,
    auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<StatusQuery>, QueryRejection>,
) -> Result<Json<StatusResponse>, AppError> {
    Ok(Json(
        service::status(
            &state,
            request_scope(&auth.user, &headers)?,
            target(path)?,
            query.map(|Query(v)| v).map_err(|_| RefreshError::Invalid)?,
        )
        .await?,
    ))
}
pub async fn start(
    auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    path: Result<Path<Uuid>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<StartRequest>, JsonRejection>,
) -> Result<(StatusCode, HeaderMap, Json<AcceptedResponse>), AppError> {
    let pins = body(input)?;
    pins.validate()?;
    let request = admission::Request {
        actor: request_scope(&auth.user, &headers)?,
        target: target(path)?,
        condition: preconditions::condition(&headers, false)?,
        pins,
        cancelled: CancellationToken::new(),
    };
    let owner = state
        .live_tv_refresh
        .get()
        .ok_or(RefreshError::Unavailable)?;
    let result = owner.start(request).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::LOCATION,
        HeaderValue::from_str(&result.status_url).map_err(|_| RefreshError::Unavailable)?,
    );
    Ok((StatusCode::ACCEPTED, headers, Json(result)))
}
