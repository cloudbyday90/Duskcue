// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{error::CatalogError, limits, query, service, types::*};
use crate::{
    domains::household_security::types::Scope, error::AppError, extractors::AuthenticatedUser,
    state::AppState,
};
use axum::{
    extract::{OriginalUri, Path, Query, State, rejection::PathRejection},
    http::{Uri, header},
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;
use uuid::Uuid;
use validator::Validate;

fn parameters<T: DeserializeOwned + Validate>(uri: &Uri) -> Result<T, CatalogError> {
    if uri.query().is_some_and(|value| value.len() > limits::QUERY) {
        return Err(CatalogError::Invalid);
    }
    let Query(value) = Query::<T>::try_from_uri(uri).map_err(|_| CatalogError::Invalid)?;
    value.validate().map_err(|_| CatalogError::Invalid)?;
    Ok(value)
}

fn response(bytes: Vec<u8>) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], bytes).into_response()
}

fn channel(path: Result<Path<Uuid>, PathRejection>) -> Result<Uuid, CatalogError> {
    path.map(|Path(id)| id).map_err(|_| CatalogError::Invalid)
}

pub(super) async fn channels(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    OriginalUri(uri): OriginalUri,
) -> Result<Response, AppError> {
    let request: ChannelsRequest = parameters(&uri)?;
    Ok(response(
        service::channels(&state, Scope::from(&user), request).await?,
    ))
}

pub(super) async fn programmes(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    OriginalUri(uri): OriginalUri,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, AppError> {
    let request: ProgrammesRequest = parameters(&uri)?;
    let window = query::window(&request.from, &request.to)?;
    Ok(response(
        service::programmes(
            &state,
            Scope::from(&user),
            service::ProgrammeRead {
                channel: channel(path)?,
                snapshot: request.snapshot_id,
                window: Some(window),
                limit: request.limit,
                cursor: request.cursor,
            },
        )
        .await?,
    ))
}

pub(super) async fn unpositionable(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    OriginalUri(uri): OriginalUri,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, AppError> {
    let request: UnpositionableRequest = parameters(&uri)?;
    Ok(response(
        service::programmes(
            &state,
            Scope::from(&user),
            service::ProgrammeRead {
                channel: channel(path)?,
                snapshot: request.snapshot_id,
                window: None,
                limit: request.limit,
                cursor: request.cursor,
            },
        )
        .await?,
    ))
}
