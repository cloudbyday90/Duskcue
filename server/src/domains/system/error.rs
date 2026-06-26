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
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SystemError {
    #[error("scheduled task not found: {0}")]
    ScheduledTaskNotFound(Uuid),

    #[error("scheduled task already running: {0}")]
    ScheduledTaskAlreadyRunning(Uuid),

    #[error("invalid cron expression: {0}")]
    InvalidCronExpression(String),

    #[error("scheduled task runner is not available")]
    SchedulerUnavailable,

    #[error("scheduled task executor is not registered: {0}")]
    TaskExecutorUnavailable(String),

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

impl From<crate::services::scheduler::SchedulerError> for SystemError {
    fn from(err: crate::services::scheduler::SchedulerError) -> Self {
        use crate::services::scheduler::SchedulerError;

        match err {
            SchedulerError::InvalidCron(expr) => Self::InvalidCronExpression(expr),
            SchedulerError::TaskNotFound(id) => Self::ScheduledTaskNotFound(id),
            SchedulerError::AlreadyRunning(id) => Self::ScheduledTaskAlreadyRunning(id),
            SchedulerError::ExecutorNotRegistered(task_type) => {
                Self::TaskExecutorUnavailable(task_type)
            }
            SchedulerError::Database(err) => Self::Database(err),
        }
    }
}
