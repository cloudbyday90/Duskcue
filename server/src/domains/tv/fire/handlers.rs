// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use super::service;
use super::types::*;
use crate::error::AppError;
use crate::extractors::{AuthenticatedUser, CanManageServer, Require};
use crate::state::AppState;

pub async fn get_catalog_mapping(
    State(state): State<AppState>,
    auth: Require<CanManageServer>,
    Path(platform_content_id): Path<String>,
) -> Result<Json<FireCatalogMappingResponse>, AppError> {
    let catalog = state
        .runtime_config
        .load()
        .integrations
        .fire_tv
        .catalog_reference
        .clone();
    Ok(Json(
        service::get_catalog_mapping(&state.pool, &auth.user, &platform_content_id, &catalog)
            .await?,
    ))
}

pub async fn put_catalog_mapping(
    State(state): State<AppState>,
    auth: Require<CanManageServer>,
    Path(platform_content_id): Path<String>,
    Json(request): Json<FireCatalogMappingRequest>,
) -> Result<Json<FireCatalogMappingResponse>, AppError> {
    let catalog = state
        .runtime_config
        .load()
        .integrations
        .fire_tv
        .catalog_reference
        .clone();
    Ok(Json(
        service::put_catalog_mapping(
            &state.pool,
            &auth.user,
            &platform_content_id,
            &catalog,
            request,
        )
        .await?,
    ))
}

pub async fn get_playback_authorization(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(session_id): Path<Uuid>,
) -> Result<Json<FirePlaybackAuthorizationResponse>, AppError> {
    let config = state.runtime_config.load().integrations.fire_tv.clone();
    Ok(Json(
        service::playback_authorization(&state.pool, &user, session_id, &config).await?,
    ))
}
