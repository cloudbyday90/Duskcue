use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

#[derive(sqlx::FromRow)]
pub struct FireCatalogMappingRow {
    pub id: Uuid,
    pub media_item_id: Option<Uuid>,
    pub original_media_item_id: Uuid,
    pub media_type: String,
    pub catalog_reference: String,
    pub amazon_content_id: String,
    pub acceptance_reference: String,
    pub distribution_rights_reference: String,
    pub rights_expires_at: DateTime<Utc>,
    pub is_enabled: bool,
    pub revision: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct FireCatalogMappingRequest {
    #[validate(length(min = 1, max = 200))]
    pub catalog_reference: String,
    #[validate(length(min = 1, max = 512))]
    pub amazon_content_id: String,
    #[validate(length(min = 1, max = 200))]
    pub acceptance_reference: String,
    #[validate(length(min = 1, max = 200))]
    pub distribution_rights_reference: String,
    pub rights_expires_at: DateTime<Utc>,
    pub is_enabled: bool,
    #[validate(range(min = 1))]
    pub expected_revision: Option<i64>,
}

#[derive(Serialize)]
pub struct FireCatalogMappingResponse {
    pub mapping_id: Uuid,
    pub platform_content_id: String,
    pub catalog_reference: String,
    pub amazon_content_id: String,
    pub acceptance_reference: String,
    pub distribution_rights_reference: String,
    pub rights_expires_at: DateTime<Utc>,
    pub is_enabled: bool,
    pub revision: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct FirePlaybackAuthorizationResponse {
    pub eligible: bool,
    pub catalog_content_id: Option<String>,
    pub opaque_profile_key: Option<String>,
    pub mapping_revision: Option<i64>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl FirePlaybackAuthorizationResponse {
    pub fn denied() -> Self {
        Self {
            eligible: false,
            catalog_content_id: None,
            opaque_profile_key: None,
            mapping_revision: None,
            expires_at: None,
        }
    }
}
