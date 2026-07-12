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
use axum::Json;
use axum::extract::{Query, State};

use crate::error::AppError;
use crate::extractors::AuthenticatedUser;
use crate::state::AppState;

use super::service;
use super::types::{SearchQuery, SearchResponse};

pub async fn search(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, AppError> {
    let params = service::validate_search_query(query)?;
    let mut response = service::search_media(&state.pool, &user, params).await?;
    let scope = crate::domains::profiles::service::load_profile_scope(
        &state.pool,
        user.user_id,
        user.profile_id,
        user.has_all_library_access,
    )
    .await?;
    if !scope.allow_search {
        return Err(crate::domains::profiles::ProfilesError::FeatureDisabled.into());
    }
    response.items.retain(|item| {
        crate::domains::profiles::service::is_media_allowed(
            &scope,
            item.library_id,
            item.content_rating.as_deref(),
        )
    });
    if crate::domains::profiles::service::is_kids(&scope) {
        response.facets = super::types::SearchFacets::default();
    }
    Ok(Json(response))
}
