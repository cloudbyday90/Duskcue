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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize)]
pub struct SessionResponse {
    pub session_token: String,
    pub user: UserSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserSummary {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub capabilities: Vec<String>,
    pub has_all_library_access: bool,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SetupRequest {
    #[validate(length(min = 1, max = 100))]
    pub username: String,
    #[validate(length(min = 1, max = 200))]
    pub display_name: String,
    #[validate(length(min = 8, max = 200))]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct InviteAuthRequest {
    #[validate(length(min = 1, max = 200))]
    pub code: String,
    #[validate(length(min = 1, max = 200))]
    pub server: String,
    pub device_name: Option<String>,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub client_platform: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct PasswordLoginRequest {
    #[validate(length(min = 1, max = 100))]
    pub username: String,
    #[validate(length(min = 1, max = 200))]
    pub password: String,
    pub device_name: Option<String>,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub client_platform: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct TotpVerifyRequest {
    #[validate(length(min = 6, max = 6))]
    pub code: String,
    pub session_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct WebauthnStartRequest {
    pub username: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebauthnFinishRequest {
    pub credential: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebauthnRegisterStartResponse {
    pub creation_options: serde_json::Value,
    pub challenge_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebauthnAuthStartResponse {
    pub request_options: serde_json::Value,
    pub challenge_id: String,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct PasskeyRegisterStartRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PasskeyRegisterFinishRequest {
    pub credential: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ReauthRequest {
    #[validate(length(min = 1, max = 200))]
    pub code: String,
    #[validate(length(min = 1, max = 200))]
    pub server: String,
    pub device_name: Option<String>,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub client_platform: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReauthCodeResponse {
    pub prefix: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct DeviceCodeRequest {
    #[validate(length(min = 1, max = 200))]
    pub client_name: Option<String>,
    pub client_platform: Option<String>,
    pub client_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: i32,
    pub interval: i32,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct DeviceTokenRequest {
    #[validate(length(min = 1, max = 512))]
    pub device_code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceTokenResponse {
    pub session_token: String,
    pub user: UserSummary,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceVerifyRequest {
    pub user_code: String,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateInvitationRequest {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 1, max = 200))]
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub library_ids: Option<Vec<Uuid>>,
    pub has_all_library_access: Option<bool>,
    pub max_uses: Option<i32>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvitationResponse {
    pub id: Uuid,
    pub code: Option<String>,
    pub code_prefix: String,
    pub email: String,
    pub display_name: Option<String>,
    pub role: String,
    pub max_uses: i32,
    pub use_count: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_revoked: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvitationListResponse {
    pub items: Vec<InvitationResponse>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionListResponse {
    pub items: Vec<SessionDetailResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionDetailResponse {
    pub id: Uuid,
    pub device_name: Option<String>,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub client_platform: Option<String>,
    pub ip_address: Option<String>,
    pub is_secure: bool,
    pub last_active_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PasskeyResponse {
    pub id: Uuid,
    pub name: String,
    pub aaguid: Option<Uuid>,
    pub transports: Vec<String>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PasskeyListResponse {
    pub items: Vec<PasskeyResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: Option<String>,
    pub new_password: String,
}

#[derive(Debug, Clone)]
pub struct UserSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub client_platform: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub is_secure: bool,
    pub expires_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct UserCapabilities {
    pub user_id: Uuid,
    pub role: String,
    pub capabilities: Vec<String>,
    pub has_all_library_access: bool,
}

#[derive(Debug, Clone)]
pub struct ValidatedSession {
    pub session: UserSession,
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub capabilities: Vec<String>,
    pub has_all_library_access: bool,
}

pub struct LoginUser {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub password_hash: Option<String>,
    pub status: String,
    pub failed_login_attempts: i32,
    pub locked_until: Option<DateTime<Utc>>,
    pub has_all_library_access: bool,
}
