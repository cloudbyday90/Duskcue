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

use sqlx::PgPool;
use uuid::Uuid;

use super::error::OverlayError;
use super::types::*;

pub fn validate_overlay_type(value: &str) -> Result<(), OverlayError> {
    if VALID_OVERLAY_TYPES.contains(&value) {
        Ok(())
    } else {
        Err(OverlayError::InvalidConditions(format!(
            "invalid overlay_type: {value}"
        )))
    }
}

pub fn validate_applies_to(value: &str) -> Result<(), OverlayError> {
    if VALID_APPLIES_TO.contains(&value) {
        Ok(())
    } else {
        Err(OverlayError::InvalidConditions(format!(
            "invalid applies_to: {value}"
        )))
    }
}

pub fn validate_horizontal_align(value: &str) -> Result<(), OverlayError> {
    if VALID_HORIZONTAL_ALIGN.contains(&value) {
        Ok(())
    } else {
        Err(OverlayError::InvalidConditions(format!(
            "invalid horizontal_align: {value}"
        )))
    }
}

pub fn validate_vertical_align(value: &str) -> Result<(), OverlayError> {
    if VALID_VERTICAL_ALIGN.contains(&value) {
        Ok(())
    } else {
        Err(OverlayError::InvalidConditions(format!(
            "invalid vertical_align: {value}"
        )))
    }
}

pub fn generate_slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub async fn list_overlays(
    _pool: &PgPool,
    _library_id: Option<Uuid>,
    _enabled_only: bool,
    _page: u32,
    _page_size: u32,
) -> Result<OverlayListResponse, OverlayError> {
    todo!("Phase 12 — overlay definition listing (CRUD)")
}

pub async fn get_overlay(_pool: &PgPool, _overlay_id: Uuid) -> Result<OverlayDefinitionResponse, OverlayError> {
    todo!("Phase 12 — overlay definition fetch (CRUD)")
}

pub async fn create_overlay(_pool: &PgPool, _req: CreateOverlayRequest) -> Result<OverlayDefinitionResponse, OverlayError> {
    todo!("Phase 12 — overlay definition creation (CRUD)")
}

pub async fn update_overlay(
    _pool: &PgPool,
    _overlay_id: Uuid,
    _req: UpdateOverlayRequest,
) -> Result<OverlayDefinitionResponse, OverlayError> {
    todo!("Phase 12 — overlay definition update (CRUD)")
}

pub async fn delete_overlay(_pool: &PgPool, _overlay_id: Uuid) -> Result<(), OverlayError> {
    todo!("Phase 12 — overlay definition deletion (CRUD)")
}

pub async fn apply_overlays(
    _pool: &PgPool,
    _req: ApplyOverlaysRequest,
) -> Result<ApplyOverlaysResponse, OverlayError> {
    todo!("Phase 12 Task 8 — overlay application worker integration")
}

pub async fn preview_overlay(
    _pool: &PgPool,
    _req: PreviewOverlayRequest,
) -> Result<PreviewOverlayResponse, OverlayError> {
    todo!("Phase 12 Task 2 — compositing pipeline preview")
}

pub async fn list_templates(_pool: &PgPool) -> Result<Vec<OverlayTemplateSummary>, OverlayError> {
    todo!("Phase 12 — community template listing")
}

pub async fn import_template(
    _pool: &PgPool,
    _import: OverlayTemplateImport,
) -> Result<OverlayTemplateResponse, OverlayError> {
    todo!("Phase 12 — community template import")
}
