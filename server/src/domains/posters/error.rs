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

use crate::error::AppError;
use crate::services::poster_management::PosterManagementError;

#[derive(Debug, thiserror::Error)]
pub enum PosterError {
    #[error(transparent)]
    Management(#[from] PosterManagementError),
}

impl From<PosterError> for AppError {
    fn from(value: PosterError) -> Self {
        match value {
            PosterError::Management(PosterManagementError::AssetDirectoryNotConfigured) => {
                AppError::BadRequest("metadata.asset_directory is not configured".into())
            }
            PosterError::Management(PosterManagementError::UnsafePath(path)) => {
                AppError::BadRequest(format!("path is outside the allowed root: {path}"))
            }
            PosterError::Management(PosterManagementError::PathNotFound(path)) => {
                AppError::NotFound(format!("path not found: {path}"))
            }
            PosterError::Management(PosterManagementError::ArtworkNotFound(id)) => {
                AppError::NotFound(format!("artwork not found: {id}"))
            }
            PosterError::Management(PosterManagementError::EmptyCommunityPack) => {
                AppError::BadRequest("community pack contains no artwork entries".into())
            }
            PosterError::Management(PosterManagementError::Image(e)) => {
                AppError::BadRequest(format!("invalid artwork image: {e}"))
            }
            PosterError::Management(PosterManagementError::Database(e)) => {
                AppError::Internal(e.into())
            }
            PosterError::Management(PosterManagementError::Io(e)) => AppError::Internal(e.into()),
        }
    }
}
