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

use uuid::Uuid;

use crate::extractors::AuthenticatedUser;

use super::error::DownloadError;
use super::types::*;

pub async fn get_download_plan(
    _pool: &sqlx::PgPool,
    _user: &AuthenticatedUser,
    _media_item_id: Uuid,
    _query: DownloadPlanQuery,
) -> Result<DownloadPlanResponse, DownloadError> {
    Err(DownloadError::NotImplemented("download planning"))
}

pub async fn create_download_job(
    _pool: &sqlx::PgPool,
    _user: &AuthenticatedUser,
    _req: CreateDownloadJobRequest,
) -> Result<DownloadJobResponse, DownloadError> {
    Err(DownloadError::NotImplemented("download job creation"))
}

pub async fn get_download_job(
    _pool: &sqlx::PgPool,
    _user: &AuthenticatedUser,
    _id: Uuid,
) -> Result<DownloadJobResponse, DownloadError> {
    Err(DownloadError::NotImplemented("download job status"))
}

pub async fn cancel_download_job(
    _pool: &sqlx::PgPool,
    _user: &AuthenticatedUser,
    _id: Uuid,
    _req: CancelDownloadJobRequest,
) -> Result<DownloadActionResponse, DownloadError> {
    Err(DownloadError::NotImplemented("download job cancellation"))
}

pub async fn list_download_inventory(
    _pool: &sqlx::PgPool,
    _user: &AuthenticatedUser,
    _query: DownloadInventoryQuery,
) -> Result<DownloadInventoryResponse, DownloadError> {
    Err(DownloadError::NotImplemented("download inventory"))
}

pub async fn delete_download_package(
    _pool: &sqlx::PgPool,
    _user: &AuthenticatedUser,
    _id: Uuid,
    _req: DeleteDownloadPackageRequest,
) -> Result<DownloadActionResponse, DownloadError> {
    Err(DownloadError::NotImplemented("download package deletion"))
}

pub async fn get_package_manifest(
    _pool: &sqlx::PgPool,
    _user: &AuthenticatedUser,
    _id: Uuid,
) -> Result<DownloadPackageManifestResponse, DownloadError> {
    Err(DownloadError::NotImplemented("download package manifest"))
}

pub async fn create_package_transfer_urls(
    _pool: &sqlx::PgPool,
    _user: &AuthenticatedUser,
    _id: Uuid,
    _req: PackageTransferUrlsRequest,
) -> Result<PackageTransferUrlsResponse, DownloadError> {
    Err(DownloadError::NotImplemented(
        "download package transfer URLs",
    ))
}

pub async fn serve_package_file(
    _pool: &sqlx::PgPool,
    _user: &AuthenticatedUser,
    _id: Uuid,
    _file_path: String,
) -> Result<(), DownloadError> {
    Err(DownloadError::NotImplemented(
        "download package file serving",
    ))
}

pub async fn sync_download_state(
    _pool: &sqlx::PgPool,
    _user: &AuthenticatedUser,
    _req: DownloadSyncRequest,
) -> Result<DownloadSyncResponse, DownloadError> {
    Err(DownloadError::NotImplemented("download reconnect sync"))
}
