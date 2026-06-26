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

#[derive(Debug, Error)]
pub enum SystemError {
    #[error("invalid provider: {0}. Valid providers: tmdb, tvdb, fanart, omdb")]
    InvalidProvider(String),

    #[error("missing required credential: {0}")]
    MissingCredential(String),

    #[error("server config is not initialized")]
    ConfigNotInitialized,

    #[error("invalid config key or group: {0}")]
    InvalidConfigKey(String),

    #[error("invalid config value for {field}: {message}")]
    InvalidConfigValue { field: String, message: String },

    #[error("failed to serialize config: {0}")]
    ConfigSerialization(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
