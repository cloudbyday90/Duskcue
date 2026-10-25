// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{challenges, service, types::*};
use crate::{
    error::AppError,
    extractors::{AuthenticatedUser, CredentialTransport},
    state::{AppState, NetworkMode},
};
use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use axum::{extract::Request, middleware::Next};

fn body<T: validator::Validate>(input: Result<Json<T>, JsonRejection>) -> Result<T, AppError> {
    let value = input
        .map(|Json(value)| value)
        .map_err(|rejection| match rejection.status() {
            StatusCode::PAYLOAD_TOO_LARGE => AppError::PayloadTooLarge,
            StatusCode::UNSUPPORTED_MEDIA_TYPE => AppError::UnsupportedMediaType,
            _ => super::error::OwnerError::InvalidRequest.into(),
        })?;
    value
        .validate()
        .map_err(|_| super::error::OwnerError::InvalidRequest)?;
    Ok(value)
}

pub async fn status(
    State(app): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<StatusResponse>, AppError> {
    Ok(Json(service::status(&app, Scope::from(&user)).await?))
}

pub async fn password(
    State(app): State<AppState>,
    user: AuthenticatedUser,
    input: Result<Json<PasswordRequest>, JsonRejection>,
) -> Result<(HeaderMap, Json<ProofResponse>), AppError> {
    let proven = service::password(&app, Scope::from(&user), body(input)?).await?;
    response(&app, &user, proven)
}

pub async fn start(
    State(app): State<AppState>,
    user: AuthenticatedUser,
    input: Result<Json<PasskeyStartRequest>, JsonRejection>,
) -> Result<Json<ChallengeResponse>, AppError> {
    Ok(Json(
        challenges::start(&app, Scope::from(&user), body(input)?).await?,
    ))
}

pub async fn finish(
    State(app): State<AppState>,
    user: AuthenticatedUser,
    input: Result<Json<PasskeyFinishRequest>, JsonRejection>,
) -> Result<(HeaderMap, Json<ProofResponse>), AppError> {
    let proven = challenges::finish(&app, Scope::from(&user), body(input)?).await?;
    response(&app, &user, proven)
}

pub async fn lock(
    State(app): State<AppState>,
    user: AuthenticatedUser,
) -> Result<StatusCode, AppError> {
    service::lock(&app, Scope::from(&user)).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn response(
    app: &AppState,
    user: &AuthenticatedUser,
    proven: Proven,
) -> Result<(HeaderMap, Json<ProofResponse>), AppError> {
    let bearer = user.transport == CredentialTransport::Bearer;
    let body = service::proof_body(&proven, bearer);
    let mut headers = HeaderMap::new();
    if !bearer {
        let age = (proven.session_expires_at - chrono::Utc::now())
            .num_seconds()
            .max(0);
        let secure = matches!(
            app.runtime_config.load().auth.network_mode,
            NetworkMode::Exposed
        );
        let cookie = format!(
            "session={}; Path=/; HttpOnly; SameSite=Strict; Max-Age={age}{}",
            proven.token,
            if secure { "; Secure" } else { "" }
        );
        headers.insert(
            header::SET_COOKIE,
            HeaderValue::from_str(&cookie).map_err(|_| super::error::OwnerError::Unavailable)?,
        );
    }
    Ok((headers, Json(body)))
}

pub(super) async fn problem_media_type(mut response: Response) -> Response {
    if (response.status().is_client_error() || response.status().is_server_error())
        && response
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|v| v == "application/json")
    {
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
    }
    response
}

pub(crate) async fn response_policy(request: Request, next: Next) -> Response {
    let owner_path = request
        .uri()
        .path()
        .starts_with("/api/v1/household-management/");
    let mut response = next.run(request).await;
    if owner_path {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        return problem_media_type(response).await;
    }
    response
}
