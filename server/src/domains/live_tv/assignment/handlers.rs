// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{error::AssignmentError, policy, preconditions, service, types::*};
use crate::{
    domains::household_security::service::request_scope,
    error::AppError,
    extractors::{AuthenticatedUser, CanManageUsers, Require},
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
use uuid::Uuid;
use validator::Validate;

fn body<T: Validate>(input: Result<Json<T>, JsonRejection>) -> Result<T, AppError> {
    let Json(value) = input.map_err(|error| match error.status() {
        StatusCode::PAYLOAD_TOO_LARGE => AppError::PayloadTooLarge,
        StatusCode::UNSUPPORTED_MEDIA_TYPE => AppError::UnsupportedMediaType,
        _ => AssignmentError::InvalidRequest.into(),
    })?;
    value
        .validate()
        .map_err(|_| AssignmentError::InvalidPolicy)?;
    Ok(value)
}

fn target(path: Result<Path<Uuid>, PathRejection>) -> Result<Uuid, AppError> {
    path.map(|Path(id)| id)
        .map_err(|_| AssignmentError::InvalidRequest.into())
}

fn response(value: AssignmentResponse) -> Result<(HeaderMap, Json<AssignmentResponse>), AppError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ETAG,
        HeaderValue::from_str(&policy::etag(&value)?).map_err(|_| AssignmentError::Unavailable)?,
    );
    Ok((headers, Json(value)))
}

pub async fn admin_read(
    headers: HeaderMap,
    auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<(HeaderMap, Json<AssignmentResponse>), AppError> {
    response(
        service::read(
            &state,
            request_scope(&auth.user, &headers)?,
            Some(target(path)?),
        )
        .await?,
    )
}

pub async fn create(
    auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    path: Result<Path<Uuid>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<CreateRequest>, JsonRejection>,
) -> Result<(StatusCode, HeaderMap, Json<AssignmentResponse>), AppError> {
    let value = service::create(
        &state,
        request_scope(&auth.user, &headers)?,
        target(path)?,
        preconditions::condition(&headers, true)?,
        body(input)?,
    )
    .await?;
    let (headers, value) = response(value)?;
    Ok((StatusCode::CREATED, headers, value))
}

pub async fn admin_patch(
    auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    path: Result<Path<Uuid>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<AdminPatch>, JsonRejection>,
) -> Result<(HeaderMap, Json<AssignmentResponse>), AppError> {
    response(
        service::admin_patch(
            &state,
            request_scope(&auth.user, &headers)?,
            target(path)?,
            preconditions::condition(&headers, false)?,
            body(input)?,
        )
        .await?,
    )
}

pub async fn remove(
    auth: Require<CanManageUsers>,
    State(state): State<AppState>,
    path: Result<Path<Uuid>, PathRejection>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    service::remove(
        &state,
        request_scope(&auth.user, &headers)?,
        target(path)?,
        preconditions::condition(&headers, false)?,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn owner_read(
    headers: HeaderMap,
    user: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<(HeaderMap, Json<AssignmentResponse>), AppError> {
    response(service::read(&state, request_scope(&user, &headers)?, None).await?)
}

pub async fn owner_patch(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    headers: HeaderMap,
    input: Result<Json<OwnerPatch>, JsonRejection>,
) -> Result<(HeaderMap, Json<AssignmentResponse>), AppError> {
    response(
        service::owner_patch(
            &state,
            request_scope(&user, &headers)?,
            preconditions::condition(&headers, false)?,
            body(input)?,
        )
        .await?,
    )
}

pub async fn profiles(
    headers: HeaderMap,
    user: AuthenticatedUser,
    State(state): State<AppState>,
    query: Result<Query<ProfileQuery>, QueryRejection>,
) -> Result<Json<ProfilePage>, AppError> {
    let Query(query) = query.map_err(|_| AssignmentError::InvalidRequest)?;
    Ok(Json(
        service::profiles(&state, request_scope(&user, &headers)?, query).await?,
    ))
}
