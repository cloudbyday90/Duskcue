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

use serde_json::{Map, Value, json};
use sqlx::Row;

use crate::services::encryption::{ENCRYPTED_PREFIX, mask_secret};
use crate::services::metadata::{ProviderValidationRequest, validate_provider_key};
use crate::state::AppState;

use super::error::SystemError;
use super::types::{
    ConfigGroupResponse, ServerConfigResponse, ServerConfigRow, ValidateProviderResponse,
};

const SCALAR_CONFIG_KEYS: &[&str] = &[
    "server_name",
    "base_url",
    "http_port",
    "https_port",
    "ssl_certificate_path",
    "ssl_private_key_path",
];

const JSON_CONFIG_GROUPS: &[&str] = &[
    "network",
    "transcoding",
    "metadata",
    "auth",
    "security",
    "notifications",
    "backup",
    "integrations",
    "logging",
    "storage",
    "maintenance",
    "resource_limits",
    "cpu",
    "quality",
    "subtitles",
    "analytics",
];

pub async fn validate_provider(
    provider: &str,
    access_token: Option<&str>,
    api_key: Option<&str>,
) -> Result<ValidateProviderResponse, SystemError> {
    let req = ProviderValidationRequest {
        provider: provider.to_string(),
        access_token: access_token.map(|s| s.to_string()),
        api_key: api_key.map(|s| s.to_string()),
    };

    let result = validate_provider_key(&req).await;
    Ok(ValidateProviderResponse {
        provider: result.provider,
        valid: result.valid,
        error: result.error,
    })
}

pub fn config_groups() -> Vec<String> {
    JSON_CONFIG_GROUPS
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

pub async fn get_server_config(state: &AppState) -> Result<ServerConfigResponse, SystemError> {
    let row = load_config_row(state).await?;
    Ok(row.into_response())
}

pub async fn get_config_group(
    state: &AppState,
    group: &str,
) -> Result<ConfigGroupResponse, SystemError> {
    ensure_config_key(group)?;
    let row = load_config_row(state).await?;
    let value = row
        .config
        .get(group)
        .cloned()
        .ok_or_else(|| SystemError::InvalidConfigKey(group.to_string()))?;
    Ok(ConfigGroupResponse {
        group: group.to_string(),
        value,
    })
}

pub async fn update_server_config(
    state: &AppState,
    updates: &Map<String, Value>,
) -> Result<ServerConfigResponse, SystemError> {
    let existing = load_config_row_unmasked(state).await?;
    let mut prepared_updates = Vec::with_capacity(updates.len());

    for (key, value) in updates {
        let prepared = prepare_update_value(state, key, value.clone(), &existing.config)?;
        prepared_updates.push((key.clone(), prepared));
    }

    for (key, prepared) in prepared_updates {
        apply_config_update(state, &key, prepared).await?;
    }

    reload_runtime_config(state).await?;
    get_server_config(state).await
}

pub async fn update_config_group(
    state: &AppState,
    group: &str,
    value: Value,
) -> Result<ConfigGroupResponse, SystemError> {
    ensure_json_group(group)?;
    let existing = load_config_row_unmasked(state).await?;
    let prepared = prepare_update_value(state, group, value, &existing.config)?;

    apply_config_update(state, group, prepared).await?;
    reload_runtime_config(state).await?;
    get_config_group(state, group).await
}

fn ensure_config_key(key: &str) -> Result<(), SystemError> {
    if SCALAR_CONFIG_KEYS.contains(&key) || JSON_CONFIG_GROUPS.contains(&key) {
        Ok(())
    } else {
        Err(SystemError::InvalidConfigKey(key.to_string()))
    }
}

fn ensure_json_group(group: &str) -> Result<(), SystemError> {
    if JSON_CONFIG_GROUPS.contains(&group) {
        Ok(())
    } else {
        Err(SystemError::InvalidConfigKey(group.to_string()))
    }
}

fn prepare_update_value(
    state: &AppState,
    key: &str,
    value: Value,
    existing: &Map<String, Value>,
) -> Result<Value, SystemError> {
    ensure_config_key(key)?;

    match key {
        "server_name" => validate_string(key, value, false, 1, 200),
        "base_url" | "ssl_certificate_path" | "ssl_private_key_path" => {
            validate_string(key, value, true, 1, 1000)
        }
        "http_port" => validate_port(key, value, false),
        "https_port" => validate_port(key, value, true),
        group if JSON_CONFIG_GROUPS.contains(&group) => {
            if !value.is_object() {
                return Err(SystemError::InvalidConfigValue {
                    field: group.to_string(),
                    message: "JSONB config groups must be objects".to_string(),
                });
            }
            let existing_group = existing.get(group);
            prepare_json_group_for_storage(state, value, existing_group)
        }
        _ => Err(SystemError::InvalidConfigKey(key.to_string())),
    }
}

fn validate_string(
    field: &str,
    value: Value,
    nullable: bool,
    min_len: usize,
    max_len: usize,
) -> Result<Value, SystemError> {
    if nullable && value.is_null() {
        return Ok(Value::Null);
    }
    let Some(s) = value.as_str() else {
        return Err(SystemError::InvalidConfigValue {
            field: field.to_string(),
            message: "expected string".to_string(),
        });
    };
    let len = s.trim().len();
    if len < min_len || len > max_len {
        return Err(SystemError::InvalidConfigValue {
            field: field.to_string(),
            message: format!("length must be between {min_len} and {max_len}"),
        });
    }
    Ok(Value::String(s.to_string()))
}

fn validate_port(field: &str, value: Value, nullable: bool) -> Result<Value, SystemError> {
    if nullable && value.is_null() {
        return Ok(Value::Null);
    }
    let Some(port) = value.as_u64() else {
        return Err(SystemError::InvalidConfigValue {
            field: field.to_string(),
            message: "expected integer port".to_string(),
        });
    };
    if !(1..=65535).contains(&port) {
        return Err(SystemError::InvalidConfigValue {
            field: field.to_string(),
            message: "port must be between 1 and 65535".to_string(),
        });
    }
    Ok(json!(port as i32))
}

fn prepare_json_group_for_storage(
    state: &AppState,
    mut incoming: Value,
    existing: Option<&Value>,
) -> Result<Value, SystemError> {
    preserve_and_encrypt_sensitive_values(state, &mut incoming, existing)?;
    Ok(incoming)
}

fn preserve_and_encrypt_sensitive_values(
    state: &AppState,
    incoming: &mut Value,
    existing: Option<&Value>,
) -> Result<(), SystemError> {
    match incoming {
        Value::Object(map) => {
            let existing_map = existing.and_then(Value::as_object);
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                let existing_value = existing_map.and_then(|m| m.get(&key));
                let Some(value) = map.get_mut(&key) else {
                    continue;
                };
                if is_sensitive_key(&key) {
                    secure_sensitive_value(state, &key, value, existing_value)?;
                } else {
                    preserve_and_encrypt_sensitive_values(state, value, existing_value)?;
                }
            }
            if let Some(existing_map) = existing_map {
                for (key, existing_value) in existing_map {
                    if is_sensitive_key(key) && !map.contains_key(key) {
                        map.insert(key.clone(), existing_value.clone());
                    }
                }
            }
        }
        Value::Array(values) => {
            for (idx, value) in values.iter_mut().enumerate() {
                let existing_value = existing
                    .and_then(Value::as_array)
                    .and_then(|arr| arr.get(idx));
                preserve_and_encrypt_sensitive_values(state, value, existing_value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn secure_sensitive_value(
    state: &AppState,
    field: &str,
    value: &mut Value,
    existing: Option<&Value>,
) -> Result<(), SystemError> {
    if value.is_null() {
        return Ok(());
    }
    let Some(s) = value.as_str() else {
        return Err(SystemError::InvalidConfigValue {
            field: field.to_string(),
            message: "sensitive values must be strings or null".to_string(),
        });
    };

    if s.is_empty() {
        return Ok(());
    }

    if let Some(existing_string) = existing.and_then(Value::as_str)
        && (s == "***encrypted***" || s == mask_secret(existing_string))
    {
        *value = Value::String(existing_string.to_string());
        return Ok(());
    }

    if s.starts_with(ENCRYPTED_PREFIX) {
        return Ok(());
    }

    let encrypted = state
        .encryption_key
        .encrypt(s)
        .map_err(|e| SystemError::ConfigSerialization(e.to_string()))?;
    *value = Value::String(encrypted);
    Ok(())
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    matches!(
        key.as_str(),
        "api_key"
            | "access_token"
            | "api_token"
            | "client_secret"
            | "secret"
            | "password"
            | "token"
            | "webhook_secret"
            | "hmac_secret"
            | "private_key"
    ) || key.ends_with("_secret")
        || key.ends_with("_token")
        || key.ends_with("_password")
}

async fn apply_config_update(state: &AppState, key: &str, value: Value) -> Result<(), SystemError> {
    ensure_config_key(key)?;

    let affected = match key {
        "server_name" => {
            let s = value
                .as_str()
                .ok_or_else(|| SystemError::InvalidConfigValue {
                    field: key.to_string(),
                    message: "expected string".to_string(),
                })?;
            sqlx::query("UPDATE server_config SET server_name = $1, updated_at = now() WHERE id = (SELECT id FROM server_config LIMIT 1)")
                .bind(s)
                .execute(&state.pool)
                .await?
                .rows_affected()
        }
        "base_url" => update_optional_string(state, "base_url", value).await?,
        "ssl_certificate_path" => {
            update_optional_string(state, "ssl_certificate_path", value).await?
        }
        "ssl_private_key_path" => {
            update_optional_string(state, "ssl_private_key_path", value).await?
        }
        "http_port" => update_required_port(state, "http_port", value).await?,
        "https_port" => update_optional_port(state, "https_port", value).await?,
        group if JSON_CONFIG_GROUPS.contains(&group) => {
            let sql = format!(
                "UPDATE server_config SET {group} = $1::jsonb, updated_at = now() WHERE id = (SELECT id FROM server_config LIMIT 1)"
            );
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(value)
                .execute(&state.pool)
                .await?
                .rows_affected()
        }
        _ => return Err(SystemError::InvalidConfigKey(key.to_string())),
    };

    if affected == 0 {
        Err(SystemError::ConfigNotInitialized)
    } else {
        Ok(())
    }
}

async fn update_optional_string(
    state: &AppState,
    column: &str,
    value: Value,
) -> Result<u64, SystemError> {
    let sql = format!(
        "UPDATE server_config SET {column} = $1, updated_at = now() WHERE id = (SELECT id FROM server_config LIMIT 1)"
    );
    let q = sqlx::query(sqlx::AssertSqlSafe(sql));
    let q = if value.is_null() {
        q.bind(None::<String>)
    } else {
        q.bind(value.as_str().map(|s| s.to_string()))
    };
    Ok(q.execute(&state.pool).await?.rows_affected())
}

async fn update_required_port(
    state: &AppState,
    column: &str,
    value: Value,
) -> Result<u64, SystemError> {
    let port = value
        .as_i64()
        .ok_or_else(|| SystemError::InvalidConfigValue {
            field: column.to_string(),
            message: "expected integer port".to_string(),
        })?;
    let sql = format!(
        "UPDATE server_config SET {column} = $1, updated_at = now() WHERE id = (SELECT id FROM server_config LIMIT 1)"
    );
    Ok(sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(port as i32)
        .execute(&state.pool)
        .await?
        .rows_affected())
}

async fn update_optional_port(
    state: &AppState,
    column: &str,
    value: Value,
) -> Result<u64, SystemError> {
    let sql = format!(
        "UPDATE server_config SET {column} = $1, updated_at = now() WHERE id = (SELECT id FROM server_config LIMIT 1)"
    );
    let q = sqlx::query(sqlx::AssertSqlSafe(sql));
    let q = if value.is_null() {
        q.bind(None::<i32>)
    } else {
        q.bind(value.as_i64().map(|n| n as i32))
    };
    Ok(q.execute(&state.pool).await?.rows_affected())
}

async fn reload_runtime_config(state: &AppState) -> Result<(), SystemError> {
    let reloaded =
        crate::state::load_runtime_config(&state.pool, Some(&state.encryption_key)).await?;
    state.reload_runtime_config(reloaded);
    Ok(())
}

async fn load_config_row(state: &AppState) -> Result<ServerConfigRow, SystemError> {
    let mut row = load_config_row_unmasked(state).await?;
    let mut config = row.config;
    let mut value = Value::Object(config);
    mask_sensitive_values(&mut value);
    config = value.as_object().cloned().unwrap_or_default();
    row.config = config;
    Ok(row)
}

async fn load_config_row_unmasked(state: &AppState) -> Result<ServerConfigRow, SystemError> {
    let row = sqlx::query(
        r#"
        SELECT
            id,
            created_at,
            updated_at,
            server_name,
            base_url,
            http_port,
            https_port,
            ssl_certificate_path,
            ssl_private_key_path,
            network,
            transcoding,
            metadata,
            auth,
            security,
            notifications,
            backup,
            integrations,
            logging,
            storage,
            maintenance,
            resource_limits,
            cpu,
            quality,
            subtitles,
            analytics,
            schema_version
        FROM server_config
        LIMIT 1
        "#,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(SystemError::ConfigNotInitialized)?;

    let mut config = Map::new();
    config.insert(
        "server_name".to_string(),
        json!(
            row.try_get::<String, _>("server_name")
                .unwrap_or_else(|_| "My Duskcue".to_string())
        ),
    );
    config.insert(
        "base_url".to_string(),
        option_string_value(row.try_get("base_url").unwrap_or(None)),
    );
    config.insert(
        "http_port".to_string(),
        json!(row.try_get::<i32, _>("http_port").unwrap_or(48027)),
    );
    config.insert(
        "https_port".to_string(),
        option_i32_value(row.try_get("https_port").unwrap_or(None)),
    );
    config.insert(
        "ssl_certificate_path".to_string(),
        option_string_value(row.try_get("ssl_certificate_path").unwrap_or(None)),
    );
    config.insert(
        "ssl_private_key_path".to_string(),
        option_string_value(row.try_get("ssl_private_key_path").unwrap_or(None)),
    );
    for group in JSON_CONFIG_GROUPS {
        config.insert(
            (*group).to_string(),
            row.try_get::<Value, _>(*group)
                .unwrap_or_else(|_| Value::Object(Map::new())),
        );
    }
    let schema_version = row.try_get("schema_version").unwrap_or(2);

    Ok(ServerConfigRow {
        id: row.try_get("id")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        schema_version,
        config,
    })
}

fn option_string_value(value: Option<String>) -> Value {
    value.map(Value::String).unwrap_or(Value::Null)
}

fn option_i32_value(value: Option<i32>) -> Value {
    value.map(|n| json!(n)).unwrap_or(Value::Null)
}

fn mask_sensitive_values(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if is_sensitive_key(key) {
                    if let Some(s) = value.as_str() {
                        *value = Value::String(mask_secret(s));
                    }
                } else {
                    mask_sensitive_values(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                mask_sensitive_values(value);
            }
        }
        _ => {}
    }
}

impl ServerConfigRow {
    fn into_response(self) -> ServerConfigResponse {
        ServerConfigResponse {
            id: self.id,
            created_at: self.created_at,
            updated_at: self.updated_at,
            schema_version: self.schema_version,
            config: Value::Object(self.config),
            groups: config_groups(),
        }
    }
}
