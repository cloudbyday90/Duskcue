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
use serde_json::{Map, Value};
use uuid::Uuid;
use validator::Validate;
use validator::{ValidationError, ValidationErrors};

use super::error::SystemError;

#[derive(Debug, Deserialize, Validate)]
pub struct ValidateProviderRequest {
    #[validate(length(min = 1, message = "Provider name is required"))]
    pub provider: String,
    pub access_token: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ValidateProviderResponse {
    pub provider: String,
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct ServerConfigRow {
    pub id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub schema_version: i32,
    pub config: Map<String, Value>,
}

#[derive(Debug, Serialize)]
pub struct ServerConfigResponse {
    pub id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub schema_version: i32,
    pub config: Value,
    pub groups: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ConfigGroupResponse {
    pub group: String,
    pub value: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScheduledTaskRunsQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduledTaskResponse {
    pub id: Uuid,
    pub name: String,
    pub task_type: String,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i32>,
    pub is_enabled: bool,
    pub timeout_seconds: i32,
    pub max_retries: i32,
    pub retry_delay_seconds: i32,
    pub state: String,
    pub consecutive_failures: i32,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_run_duration_ms: Option<i32>,
    pub last_run_result: Option<String>,
    pub last_error: Option<String>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub config: Value,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduledTaskListResponse {
    pub items: Vec<ScheduledTaskResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduledTaskRunResponse {
    pub id: Uuid,
    pub scheduled_task_id: Uuid,
    pub trigger_type: String,
    pub state: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i32>,
    pub result: Option<String>,
    pub error_message: Option<String>,
    pub error_details: Option<Value>,
    pub stats: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduledTaskRunListResponse {
    pub items: Vec<ScheduledTaskRunResponse>,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduledTaskTriggerResponse {
    pub task_id: Uuid,
    pub run_id: Uuid,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduledTaskCancelResponse {
    pub task_id: Uuid,
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateServerConfigRequest {
    #[serde(flatten)]
    pub values: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigGroupRequest {
    pub value: Value,
}

impl Validate for UpdateServerConfigRequest {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if self.values.is_empty() {
            errors.add("config", ValidationError::new("required"));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Validate for UpdateConfigGroupRequest {
    fn validate(&self) -> Result<(), ValidationErrors> {
        Ok(())
    }
}

impl ValidateProviderRequest {
    pub fn validate_credentials(&self) -> Result<(), SystemError> {
        match self.provider.as_str() {
            "tmdb" => {
                if self
                    .access_token
                    .as_ref()
                    .is_none_or(|t| t.trim().is_empty())
                {
                    return Err(SystemError::MissingCredential(
                        "access_token is required for TMDB".to_string(),
                    ));
                }
            }
            "tvdb" | "fanart" | "omdb" => {
                if self.api_key.as_ref().is_none_or(|k| k.trim().is_empty()) {
                    return Err(SystemError::MissingCredential(format!(
                        "api_key is required for {}",
                        self.provider
                    )));
                }
            }
            _ => {
                return Err(SystemError::InvalidProvider(self.provider.clone()));
            }
        }
        Ok(())
    }
}
