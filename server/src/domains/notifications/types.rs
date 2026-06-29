// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even implied
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use validator::Validate;

pub const VALID_CATEGORIES: &[&str] = &["media", "system", "security", "user", "task"];
pub const VALID_PRIORITIES: &[&str] = &["low", "medium", "high"];

pub struct NotificationRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub notification_type_id: Uuid,
    pub title: String,
    pub body: String,
    pub priority: String,
    pub link: Option<String>,
    pub is_read: bool,
    pub read_at: Option<DateTime<Utc>>,
    pub delivery_channels: Value,
    pub delivery_status: Value,
    pub related_item_type: Option<String>,
    pub related_item_id: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub notification_type_name: String,
    pub notification_type_category: String,
}

pub struct NotificationTypeRow {
    pub id: Uuid,
    pub name: String,
    pub category: String,
    pub priority: String,
    pub in_app_template: String,
    pub is_enabled_by_default: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NotificationListQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub order: Option<String>,
    pub is_read: Option<bool>,
    pub category: Option<String>,
    pub priority: Option<String>,
    #[serde(rename = "type")]
    pub notification_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdatePreferenceRequest {
    pub in_app_enabled: Option<bool>,
    pub webhook_enabled: Option<bool>,
    pub push_enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct TestNotificationRequest {
    #[serde(rename = "type")]
    pub notification_type: Option<String>,
    pub title: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub notification_type: String,
    pub category: String,
    pub title: String,
    pub body: String,
    pub priority: String,
    pub link: Option<String>,
    pub is_read: bool,
    pub read_at: Option<DateTime<Utc>>,
    pub related_item_type: Option<String>,
    pub related_item_id: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationListResponse {
    pub items: Vec<NotificationResponse>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnreadCountResponse {
    pub unread_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarkReadResponse {
    pub notification_id: Uuid,
    pub read: bool,
    pub read_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BulkMarkReadResponse {
    pub marked_read: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeleteResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BulkDeleteResponse {
    pub deleted: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationTypeResponse {
    pub id: Uuid,
    pub name: String,
    pub category: String,
    pub priority: String,
    pub in_app_template: String,
    pub is_enabled_by_default: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationTypeListResponse {
    pub items: Vec<NotificationTypeResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationPreferenceResponse {
    pub notification_type_id: Uuid,
    pub name: String,
    pub category: String,
    pub priority: String,
    pub is_enabled_by_default: bool,
    pub in_app_enabled: bool,
    pub webhook_enabled: bool,
    pub push_enabled: bool,
    pub is_using_defaults: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationPreferenceListResponse {
    pub preferences: Vec<NotificationPreferenceResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreferenceUpdateResponse {
    pub notification_type_id: Uuid,
    pub in_app_enabled: bool,
    pub webhook_enabled: bool,
    pub push_enabled: bool,
}
