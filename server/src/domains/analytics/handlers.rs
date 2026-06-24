// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use axum::extract::{Path, Query, State};
use axum::Json;
use uuid::Uuid;

use crate::error::AppError;
use crate::extractors::{CanViewAnalytics, Require};
use crate::state::AppState;

use super::service;
use super::types::*;

pub async fn get_overview(
    State(state): State<AppState>,
    _auth: Require<CanViewAnalytics>,
    Query(query): Query<AnalyticsQuery>,
) -> Result<Json<AnalyticsOverviewResponse>, AppError> {
    let result = service::get_analytics_overview(&state.pool, &query).await?;
    Ok(Json(result))
}

pub async fn get_play_history(
    State(state): State<AppState>,
    _auth: Require<CanViewAnalytics>,
    Query(query): Query<PlayHistoryQuery>,
) -> Result<Json<PlayHistoryResponse>, AppError> {
    let result = service::list_play_history(&state.pool, &query).await?;
    Ok(Json(result))
}

pub async fn get_top_media(
    State(state): State<AppState>,
    _auth: Require<CanViewAnalytics>,
    Query(query): Query<TopMediaQuery>,
) -> Result<Json<TopMediaResponse>, AppError> {
    let result = service::get_top_media(&state.pool, &query).await?;
    Ok(Json(result))
}

pub async fn get_bandwidth(
    State(state): State<AppState>,
    _auth: Require<CanViewAnalytics>,
    Query(query): Query<AnalyticsQuery>,
) -> Result<Json<BandwidthResponse>, AppError> {
    let result = service::get_bandwidth_usage(&state.pool, &query).await?;
    Ok(Json(result))
}

pub async fn get_concurrent(
    State(state): State<AppState>,
    _auth: Require<CanViewAnalytics>,
) -> Result<Json<ConcurrentStreamsResponse>, AppError> {
    let result = service::get_concurrent_streams(&state.pool).await?;
    Ok(Json(result))
}

pub async fn get_trust_scores(
    State(state): State<AppState>,
    _auth: Require<CanViewAnalytics>,
) -> Result<Json<TrustScoreListResponse>, AppError> {
    let result = service::list_trust_scores(&state.pool).await?;
    Ok(Json(result))
}

pub async fn get_trust_events(
    State(state): State<AppState>,
    _auth: Require<CanViewAnalytics>,
    Query(query): Query<TrustEventQuery>,
) -> Result<Json<TrustEventListResponse>, AppError> {
    let result = service::list_trust_events(&state.pool, &query).await?;
    Ok(Json(result))
}

pub async fn acknowledge_event(
    State(state): State<AppState>,
    auth: Require<CanViewAnalytics>,
    Path(event_id): Path<Uuid>,
) -> Result<Json<AcknowledgeEventResponse>, AppError> {
    let result =
        service::acknowledge_trust_event(&state.pool, event_id, auth.user.user_id).await?;
    Ok(Json(result))
}

pub async fn get_geoip_status(
    State(state): State<AppState>,
    _auth: Require<CanViewAnalytics>,
) -> Result<Json<GeoIpStatusResponse>, AppError> {
    let enabled = state.runtime_config.load().analytics.geoip_enabled;
    let result = service::get_geoip_status(&state.geoip, enabled).await?;
    Ok(Json(result))
}
