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

#![allow(unused_variables)]

use sqlx::PgPool;
use uuid::Uuid;

use crate::domains::trakt::error::TraktError;
use crate::domains::trakt::types::*;

pub async fn get_account(pool: &PgPool, user_id: Uuid) -> Result<TraktAccountResponse, TraktError> {
    todo!()
}

pub async fn start_device_link(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<DeviceCodeResponse, TraktError> {
    todo!()
}

pub async fn poll_device_code(
    pool: &PgPool,
    user_id: Uuid,
    device_code: &str,
) -> Result<TraktAccountResponse, TraktError> {
    todo!()
}

pub async fn unlink_account(pool: &PgPool, user_id: Uuid) -> Result<(), TraktError> {
    todo!()
}

pub async fn get_sync_settings(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<SyncSettingsResponse, TraktError> {
    todo!()
}

pub async fn update_sync_settings(
    pool: &PgPool,
    user_id: Uuid,
    settings: &UpdateSyncSettingsRequest,
) -> Result<SyncSettingsResponse, TraktError> {
    todo!()
}

pub async fn trigger_sync(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<SyncTriggerResponse, TraktError> {
    todo!()
}

pub async fn get_sync_status(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<SyncStatusResponse, TraktError> {
    todo!()
}

pub async fn list_history(
    pool: &PgPool,
    user_id: Uuid,
    query: &HistoryQuery,
) -> Result<TraktHistoryResponse, TraktError> {
    todo!()
}

pub async fn list_ratings(
    pool: &PgPool,
    user_id: Uuid,
    query: &HistoryQuery,
) -> Result<TraktHistoryResponse, TraktError> {
    todo!()
}
