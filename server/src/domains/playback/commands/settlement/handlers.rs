// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    service,
    types::{SettlementResponse, WithdrawRequest},
};
use crate::{
    domains::playback::commands::{
        cleanup_auth::CleanupActor, error::CommandError, handlers::target, types::PROTOCOL,
    },
    state::AppState,
};
use axum::{
    Json,
    body::to_bytes,
    extract::{Path, Request, State, rejection::PathRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
};
use uuid::Uuid;

pub async fn withdraw(
    State(state): State<AppState>,
    CleanupActor(scope): CleanupActor,
    path: Result<Path<Uuid>, PathRejection>,
    request: Request,
) -> Result<(StatusCode, HeaderMap, Json<SettlementResponse>), CommandError> {
    let id = target(path)?;
    if !super::super::handlers::json_type(request.headers()) {
        return Err(CommandError::MediaType);
    }
    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        to_bytes(request.into_body(), 1024),
    )
    .await
    .map_err(|_| CommandError::Invalid)?
    .map_err(|_| CommandError::TooLarge)?;
    let body: WithdrawRequest =
        serde_json::from_slice(&bytes).map_err(|_| CommandError::Invalid)?;
    if body.protocol != PROTOCOL {
        return Err(CommandError::Invalid);
    }
    let response = service::withdraw(&state, scope, id).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::LOCATION,
        HeaderValue::from_str(&response.status_url).map_err(|_| CommandError::Unavailable)?,
    );
    Ok((StatusCode::ACCEPTED, headers, Json(response)))
}

pub async fn read(
    State(state): State<AppState>,
    CleanupActor(scope): CleanupActor,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<SettlementResponse>, CommandError> {
    Ok(Json(service::read(&state, scope, target(path)?).await?))
}
