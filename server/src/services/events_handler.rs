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

use crate::{
    error::AppError,
    extractors::AuthenticatedUser,
    services::{event_bus::parse_type_filter, event_streams::HEARTBEAT},
    state::AppState,
};
use axum::{
    extract::{Query, State, rejection::QueryRejection},
    http::{HeaderMap, HeaderValue},
    response::{
        IntoResponse, Response,
        sse::{KeepAlive, Sse},
    },
};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Default, Deserialize)]
pub struct EventsQuery {
    pub types: Option<String>,
}

pub async fn events_handler(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    query: Result<Query<EventsQuery>, QueryRejection>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let Query(query) =
        query.map_err(|_| AppError::BadRequest("Invalid event stream query".into()))?;
    let last = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| Uuid::parse_str(v).ok())
        .filter(|id| *id != Uuid::nil());
    let output = state
        .event_streams
        .open(
            state.event_bus.clone(),
            user.user_id,
            parse_type_filter(query.types.as_deref()),
            last,
        )
        .await?;
    let sse = Sse::new(output).keep_alive(KeepAlive::new().interval(HEARTBEAT).text("keep-alive"));
    let mut extra = HeaderMap::new();
    extra.insert("x-accel-buffering", HeaderValue::from_static("no"));
    Ok((extra, sse).into_response())
}
