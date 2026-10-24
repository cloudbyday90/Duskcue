// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::http::{HeaderValue, header::CONTENT_TYPE};
use axum::response::Response;

use crate::error::AppError;
use crate::extractors::{CanManageUsers, Require};

use super::error::body_rejection;
use super::service;
use super::types::{ConnectionValidationRequest, ConnectionValidationResponse};

pub async fn validate_connection(
    _auth: Require<CanManageUsers>,
    body: Result<Json<ConnectionValidationRequest>, JsonRejection>,
) -> Result<Json<ConnectionValidationResponse>, AppError> {
    validate_body(body)
}

pub(super) fn validate_body(
    body: Result<Json<ConnectionValidationRequest>, JsonRejection>,
) -> Result<Json<ConnectionValidationResponse>, AppError> {
    let Json(request) = body.map_err(body_rejection)?;
    Ok(Json(service::validate_connection(request)?))
}

pub(super) async fn problem_media_type(mut response: Response) -> Response {
    if (response.status().is_client_error() || response.status().is_server_error())
        && response
            .headers()
            .get(CONTENT_TYPE)
            .is_some_and(|value| value == "application/json")
    {
        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
    }
    response
}
