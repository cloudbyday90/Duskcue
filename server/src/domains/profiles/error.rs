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

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProfilesError {
    #[error("profile not found")]
    NotFound,

    #[error("profile access denied")]
    AccessDenied,

    #[error("profile type is invalid: {0}")]
    InvalidProfileType(String),

    #[error("content rating is invalid: {0}")]
    InvalidContentRating(String),

    #[error("profile cannot be deleted")]
    CannotDelete,

    #[error("content is not available for this profile")]
    ContentNotAllowed,

    #[error("profile feature is disabled by parental controls")]
    FeatureDisabled,

    #[error("ambient channel not found")]
    ChannelNotFound,

    #[error("ambient channel is unavailable for this profile")]
    ChannelUnavailable,

    #[error("ambient channel has no playable items")]
    ChannelEmpty,

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
