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

use axum::extract::State;
use axum::Json;
use validator::Validate;

use crate::error::AppError;
use crate::extractors::Require;
use crate::extractors::CanManageServer;
use crate::state::AppState;

use super::service;
use super::types::{ValidateProviderRequest, ValidateProviderResponse};

pub async fn validate_provider_key(
    _auth: Require<CanManageServer>,
    State(_state): State<AppState>,
    Json(req): Json<ValidateProviderRequest>,
) -> Result<Json<ValidateProviderResponse>, AppError> {
    req.validate().map_err(|e| {
        let errors: Vec<crate::error::FieldError> = e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errs)| {
                errs.iter().map(move |err| crate::error::FieldError {
                    field: field.to_string(),
                    code: err.code.to_string(),
                    message: err.message.as_ref().map(|m| m.to_string()).unwrap_or_default(),
                })
            })
            .collect();
        AppError::Validation {
            errors,
            instance: Some("/api/v1/settings/providers/validate".to_string()),
        }
    })?;

    req.validate_credentials()?;

    let result = service::validate_provider(
        &req.provider,
        req.access_token.as_deref(),
        req.api_key.as_deref(),
    )
    .await?;

    Ok(Json(result))
}
