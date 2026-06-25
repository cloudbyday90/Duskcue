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

use crate::services::conditions;

use super::error::CollectionsError;
use super::types::*;

pub fn validate_collection_type(value: &str) -> Result<(), CollectionsError> {
    if VALID_COLLECTION_TYPES.contains(&value) {
        Ok(())
    } else {
        Err(CollectionsError::InvalidDynamicConfig(format!(
            "invalid collection_type: {value}"
        )))
    }
}

pub fn validate_visibility(value: &str) -> Result<(), CollectionsError> {
    if VALID_VISIBILITY.contains(&value) {
        Ok(())
    } else {
        Err(CollectionsError::InvalidDynamicConfig(format!(
            "invalid visibility: {value}"
        )))
    }
}

pub fn validate_sync_mode(value: &str) -> Result<(), CollectionsError> {
    if VALID_SYNC_MODES.contains(&value) {
        Ok(())
    } else {
        Err(CollectionsError::InvalidDynamicConfig(format!(
            "invalid sync_mode: {value}"
        )))
    }
}

pub fn validate_template_type(value: &str) -> Result<(), CollectionsError> {
    if VALID_TEMPLATE_TYPES.contains(&value) {
        Ok(())
    } else {
        Err(CollectionsError::InvalidDynamicConfig(format!(
            "invalid template_type: {value}"
        )))
    }
}

pub fn validate_dynamic_config(config: &serde_json::Value) -> Result<(), CollectionsError> {
    let builder_type = config
        .get("builder_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            CollectionsError::InvalidDynamicConfig("dynamic_config.builder_type is required".into())
        })?;

    if VALID_BUILDER_TYPES.contains(&builder_type) {
        Ok(())
    } else {
        Err(CollectionsError::InvalidDynamicConfig(format!(
            "invalid builder_type: {builder_type}"
        )))
    }
}

pub fn validate_smart_filter(filter: &serde_json::Value) -> Result<(), CollectionsError> {
    conditions::validate_structure(filter)
        .map_err(|e| CollectionsError::InvalidSmartFilter(e.to_string()))
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

pub async fn list_collections(
    _pool: &PgPool,
    _query: ListCollectionsQuery,
    _page: u32,
    _page_size: u32,
) -> Result<CollectionListResponse, CollectionsError> {
    todo!("Phase 12 — collection listing")
}

pub async fn get_collection(
    _pool: &PgPool,
    _collection_id: Uuid,
) -> Result<CollectionResponse, CollectionsError> {
    todo!("Phase 12 — collection fetch")
}

pub async fn create_collection(
    _pool: &PgPool,
    _req: CreateCollectionRequest,
) -> Result<CollectionResponse, CollectionsError> {
    todo!("Phase 12 — collection creation")
}

pub async fn update_collection(
    _pool: &PgPool,
    _collection_id: Uuid,
    _req: UpdateCollectionRequest,
) -> Result<CollectionResponse, CollectionsError> {
    todo!("Phase 12 — collection update")
}

pub async fn delete_collection(
    _pool: &PgPool,
    _collection_id: Uuid,
) -> Result<(), CollectionsError> {
    todo!("Phase 12 — collection deletion")
}

pub async fn list_collection_items(
    _pool: &PgPool,
    _collection_id: Uuid,
    _query: ListCollectionItemsQuery,
    _page: u32,
    _page_size: u32,
) -> Result<CollectionItemsResponse, CollectionsError> {
    todo!("Phase 12 — collection item listing")
}

pub async fn add_collection_items(
    _pool: &PgPool,
    _collection_id: Uuid,
    _req: AddCollectionItemsRequest,
) -> Result<CollectionItemsResponse, CollectionsError> {
    todo!("Phase 12 — static collection item add")
}

pub async fn reorder_collection_items(
    _pool: &PgPool,
    _collection_id: Uuid,
    _req: ReorderCollectionItemsRequest,
) -> Result<CollectionItemsResponse, CollectionsError> {
    todo!("Phase 12 — static collection item reorder")
}

pub async fn remove_collection_item(
    _pool: &PgPool,
    _collection_id: Uuid,
    _media_item_id: Uuid,
) -> Result<(), CollectionsError> {
    todo!("Phase 12 — static collection item removal")
}

pub async fn sync_collections(
    _pool: &PgPool,
    _req: SyncCollectionsRequest,
) -> Result<SyncCollectionResponse, CollectionsError> {
    todo!("Phase 12 — dynamic collection sync dispatch")
}

pub async fn sync_collection(
    _pool: &PgPool,
    _collection_id: Uuid,
    _req: SyncCollectionRequest,
) -> Result<SyncCollectionResponse, CollectionsError> {
    todo!("Phase 12 — single dynamic collection sync dispatch")
}

pub async fn list_templates(
    _pool: &PgPool,
) -> Result<Vec<CollectionTemplateSummary>, CollectionsError> {
    todo!("Phase 12 — collection template listing")
}

pub async fn import_template(
    _pool: &PgPool,
    _req: ImportCollectionTemplateRequest,
) -> Result<CollectionTemplateResponse, CollectionsError> {
    todo!("Phase 12 — collection template import")
}
