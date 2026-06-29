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

use thiserror::Error;

#[derive(Error, Debug)]
pub enum NotificationsError {
    #[error("notification not found")]
    NotFound,

    #[error("notification type not found")]
    NotificationTypeNotFound,

    #[error("invalid category: {0}")]
    InvalidCategory(String),

    #[error("invalid priority: {0}")]
    InvalidPriority(String),

    #[error("invalid channel configuration: {0}")]
    InvalidChannelConfig(String),

    #[error("push device not found")]
    PushDeviceNotFound,

    #[error("invalid push provider: {0}")]
    InvalidPushProvider(String),

    #[error("invalid push token: {0}")]
    InvalidPushToken(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
