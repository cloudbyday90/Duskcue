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

use std::marker::PhantomData;

use axum::extract::{FromRequestParts, Query};
use axum::http::header::{AUTHORIZATION, COOKIE};
use axum::http::request::Parts;
use serde::Deserialize;
use uuid::Uuid;

use crate::domains::auth;
use crate::error::{AppError, FieldError};
use crate::state::AppState;

const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 100;
const DEFAULT_PAGE: u32 = 1;
const DEFAULT_PAGE_SIZE: u32 = 25;
const MAX_PAGE_SIZE: u32 = 100;

pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub profile_id: Uuid,
    pub device_id: Option<String>,
    pub capabilities: Vec<String>,
    pub role: String,
    pub has_all_library_access: bool,
    pub display_name: String,
}

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_session_token(parts)?;

        let validated = auth::service::validate_session(&state.pool, &token).await?;

        let config = state.runtime_config.load();
        if auth::service::is_idle_expired(
            &validated.session,
            config.auth.session_idle_timeout_hours,
        ) {
            drop(config);
            let _ = sqlx::query("DELETE FROM user_sessions WHERE id = $1")
                .bind(validated.session.id)
                .execute(&state.pool)
                .await;
            return Err(AppError::Auth(auth::AuthError::SessionExpired));
        }
        drop(config);

        let now = chrono::Utc::now();
        let elapsed = now - validated.session.last_active_at;
        let should_update = elapsed.num_seconds() > 60;

        if should_update {
            let _ = sqlx::query("UPDATE user_sessions SET last_active_at = now() WHERE id = $1")
                .bind(validated.session.id)
                .execute(&state.pool)
                .await;
        }

        Ok(AuthenticatedUser {
            user_id: validated.user_id,
            session_id: validated.session.id,
            profile_id: validated.active_profile_id,
            device_id: validated.session.device_id,
            capabilities: validated.capabilities,
            role: validated.role,
            has_all_library_access: validated.has_all_library_access,
            display_name: validated.display_name,
        })
    }
}

fn extract_session_token(parts: &Parts) -> Result<String, AppError> {
    if let Some(cookie_header) = parts.headers.get(COOKIE)
        && let Ok(val) = cookie_header.to_str()
    {
        for cookie in val.split(';') {
            let cookie = cookie.trim();
            if let Some(token) = cookie.strip_prefix("session=") {
                let token = token.trim();
                if !token.is_empty() {
                    return Ok(token.to_string());
                }
            }
        }
    }

    if let Some(auth_header) = parts.headers.get(AUTHORIZATION)
        && let Ok(val) = auth_header.to_str()
        && let Some(token) = val.strip_prefix("Bearer ")
    {
        let token = token.trim();
        if !token.is_empty() {
            return Ok(token.to_string());
        }
    }

    Err(AppError::Unauthorized(
        "Missing authentication token".into(),
    ))
}

pub trait RequiredCapability {
    const CAPABILITY: &'static str;
}

pub struct Require<C: RequiredCapability> {
    pub user: AuthenticatedUser,
    _marker: PhantomData<C>,
}

impl<C: RequiredCapability> FromRequestParts<AppState> for Require<C> {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = AuthenticatedUser::from_request_parts(parts, state).await?;
        let profile_scope = crate::domains::profiles::service::load_profile_scope(
            &state.pool,
            user.user_id,
            user.profile_id,
            user.has_all_library_access,
        )
        .await?;
        if crate::domains::profiles::service::is_kids(&profile_scope)
            && C::CAPABILITY != "play_media"
        {
            return Err(AppError::Forbidden(
                "This feature is unavailable in Kids mode".into(),
            ));
        }
        if C::CAPABILITY == "can_download" && !profile_scope.allow_downloads {
            return Err(AppError::Forbidden(
                "Downloads are disabled by parental controls".into(),
            ));
        }
        auth::service::check_capability(&user.role, &user.capabilities, C::CAPABILITY)?;
        Ok(Require {
            user,
            _marker: PhantomData,
        })
    }
}

pub struct CanManageServer;
impl RequiredCapability for CanManageServer {
    const CAPABILITY: &'static str = "can_manage_server";
}

pub struct CanManageUsers;
impl RequiredCapability for CanManageUsers {
    const CAPABILITY: &'static str = "can_manage_users";
}

pub struct CanManageLibraries;
impl RequiredCapability for CanManageLibraries {
    const CAPABILITY: &'static str = "can_manage_libraries";
}

pub struct CanViewAnalytics;
impl RequiredCapability for CanViewAnalytics {
    const CAPABILITY: &'static str = "can_view_analytics";
}

pub struct CanManageScheduledTasks;
impl RequiredCapability for CanManageScheduledTasks {
    const CAPABILITY: &'static str = "can_manage_scheduled_tasks";
}

pub struct CanTranscode;
impl RequiredCapability for CanTranscode {
    const CAPABILITY: &'static str = "can_transcode";
}

pub struct CanDownload;
impl RequiredCapability for CanDownload {
    const CAPABILITY: &'static str = "can_download";
}

pub struct CanDeleteMedia;
impl RequiredCapability for CanDeleteMedia {
    const CAPABILITY: &'static str = "can_delete_media";
}

pub struct CanUseLiveTv;
impl RequiredCapability for CanUseLiveTv {
    const CAPABILITY: &'static str = "can_use_live_tv";
}

pub struct CanShareContent;
impl RequiredCapability for CanShareContent {
    const CAPABILITY: &'static str = "can_share_content";
}

pub struct CanRemoteControl;
impl RequiredCapability for CanRemoteControl {
    const CAPABILITY: &'static str = "can_remote_control";
}

pub struct PlayMedia;
impl RequiredCapability for PlayMedia {
    const CAPABILITY: &'static str = "play_media";
}

pub type AdminOnly = Require<CanManageServer>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl std::fmt::Display for SortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SortOrder::Asc => write!(f, "asc"),
            SortOrder::Desc => write!(f, "desc"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct PaginationQuery {
    limit: Option<u32>,
    cursor: Option<String>,
    order: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum PaginationParams {
    Cursor {
        limit: u32,
        cursor: Option<String>,
        order: SortOrder,
    },
    Offset {
        page: u32,
        page_size: u32,
    },
}

impl PaginationParams {
    pub fn limit(&self) -> u32 {
        match self {
            Self::Cursor { limit, .. } => *limit,
            Self::Offset { page_size, .. } => *page_size,
        }
    }

    pub fn is_cursor(&self) -> bool {
        matches!(self, Self::Cursor { .. })
    }

    pub fn cursor(&self) -> Option<&str> {
        match self {
            Self::Cursor { cursor, .. } => cursor.as_deref(),
            Self::Offset { .. } => None,
        }
    }

    pub fn order(&self) -> SortOrder {
        match self {
            Self::Cursor { order, .. } => *order,
            Self::Offset { .. } => SortOrder::Desc,
        }
    }

    pub fn page(&self) -> Option<u32> {
        match self {
            Self::Cursor { .. } => None,
            Self::Offset { page, .. } => Some(*page),
        }
    }

    pub fn page_size(&self) -> Option<u32> {
        match self {
            Self::Cursor { .. } => None,
            Self::Offset { page_size, .. } => Some(*page_size),
        }
    }
}

impl FromRequestParts<AppState> for PaginationParams {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let query = Query::<PaginationQuery>::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::BadRequest("Invalid query parameters".into()))?;

        validate_pagination(query.0)
    }
}

fn validate_pagination(query: PaginationQuery) -> Result<PaginationParams, AppError> {
    let has_cursor = query.cursor.as_ref().is_some_and(|c| !c.trim().is_empty());
    let is_offset_request = query.page.is_some() || query.page_size.is_some();

    if has_cursor && is_offset_request {
        return Err(field_error(
            "cursor",
            "CONFLICT",
            "Cannot use both cursor and offset pagination",
        ));
    }

    if is_offset_request {
        let page = query.page.unwrap_or(DEFAULT_PAGE);
        if page < 1 {
            return Err(field_error("page", "MIN_VALUE", "page must be at least 1"));
        }

        let page_size = query.page_size.unwrap_or(DEFAULT_PAGE_SIZE);
        if page_size < 1 {
            return Err(field_error(
                "page_size",
                "MIN_VALUE",
                "page_size must be at least 1",
            ));
        }
        if page_size > MAX_PAGE_SIZE {
            return Err(field_error(
                "page_size",
                "MAX_VALUE",
                &format!("page_size must not exceed {MAX_PAGE_SIZE}"),
            ));
        }

        Ok(PaginationParams::Offset { page, page_size })
    } else {
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
        if limit < 1 {
            return Err(field_error(
                "limit",
                "MIN_VALUE",
                "limit must be at least 1",
            ));
        }
        if limit > MAX_LIMIT {
            return Err(field_error(
                "limit",
                "MAX_VALUE",
                &format!("limit must not exceed {MAX_LIMIT}"),
            ));
        }

        let cursor = query
            .cursor
            .filter(|c| !c.trim().is_empty())
            .map(|c| {
                if is_valid_base64(&c) {
                    Ok(c)
                } else {
                    Err(field_error(
                        "cursor",
                        "INVALID_FORMAT",
                        "Invalid cursor encoding",
                    ))
                }
            })
            .transpose()?;

        let order = match query.order.as_deref() {
            Some("asc") => SortOrder::Asc,
            Some("desc") | None => SortOrder::Desc,
            Some(_) => {
                return Err(field_error(
                    "order",
                    "INVALID_VALUE",
                    "order must be 'asc' or 'desc'",
                ));
            }
        };

        Ok(PaginationParams::Cursor {
            limit,
            cursor,
            order,
        })
    }
}

fn is_valid_base64(s: &str) -> bool {
    let trimmed = s.trim();
    !trimmed.is_empty()
        && trimmed.len().is_multiple_of(4)
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
}

fn field_error(field: &str, code: &str, message: &str) -> AppError {
    AppError::Validation {
        errors: vec![FieldError {
            field: field.to_string(),
            code: code.to_string(),
            message: message.to_string(),
        }],
        instance: None,
    }
}
