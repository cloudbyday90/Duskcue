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

pub mod error;
pub mod handlers;
pub mod service;
pub mod types;

pub use error::NotificationsError;

use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/notifications", get(handlers::list_notifications))
        .route(
            "/api/v1/notifications/unread-count",
            get(handlers::get_unread_count),
        )
        .route(
            "/api/v1/notifications/read-all",
            post(handlers::mark_all_read),
        )
        .route("/api/v1/notifications/read", delete(handlers::delete_read))
        .route(
            "/api/v1/notifications/test",
            post(handlers::send_test_notification),
        )
        .route(
            "/api/v1/notifications/{notification_id}/read",
            post(handlers::mark_notification_read),
        )
        .route(
            "/api/v1/notifications/{notification_id}",
            delete(handlers::delete_notification),
        )
        .route(
            "/api/v1/notification-types",
            get(handlers::list_notification_types),
        )
        .route(
            "/api/v1/user/notification-preferences",
            get(handlers::list_user_preferences),
        )
        .route(
            "/api/v1/user/notification-preferences/{type_id}",
            put(handlers::update_user_preference),
        )
        .route(
            "/api/v1/user/push-devices",
            post(handlers::register_push_device).get(handlers::list_push_devices),
        )
        .route(
            "/api/v1/user/push-devices/{device_id}",
            put(handlers::update_push_device).delete(handlers::delete_push_device),
        )
        .with_state(state)
}
