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

//! Trakt.tv OAuth + user-settings HTTP client.
//!
//! Stateless: token state lives in the `trakt_accounts` DB row, not in this
//! client. The domain service is responsible for persistence and proactive
//! refresh; this module only performs HTTP calls and maps failures to
//! [`TraktError`](crate::domains::trakt::TraktError).

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, StatusCode};
use serde::Deserialize;

use crate::domains::trakt::error::TraktError;
use crate::domains::trakt::types::DeviceCodeResponse;

const BASE_URL: &str = "https://api.trakt.tv";

#[derive(Debug, Clone, Deserialize)]
pub struct TraktTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_token: String,
    pub scope: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TraktUserSettings {
    pub user: TraktSettingsUser,
    pub account: TraktAccount,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TraktSettingsUser {
    pub username: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TraktAccount {
    pub id: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct TraktTokenErrorResponse {
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Clone)]
pub struct TraktClient {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    http: Client,
}

impl TraktClient {
    pub fn new(client_id: String, client_secret: String, redirect_uri: String) -> Self {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(
            "Content-Type",
            HeaderValue::from_static("application/json"),
        );
        default_headers.insert("trakt-api-version", HeaderValue::from_static("2"));
        if let Ok(val) = HeaderValue::from_str(&client_id) {
            default_headers.insert("trakt-api-key", val);
        }

        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .default_headers(default_headers)
            .build()
            .unwrap_or_default();

        Self {
            client_id,
            client_secret,
            redirect_uri,
            http,
        }
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub async fn request_device_code(&self) -> Result<DeviceCodeResponse, TraktError> {
        let url = format!("{BASE_URL}/oauth/device/code");
        let body = serde_json::json!({ "client_id": self.client_id });

        let response = self.http.post(&url).json(&body).send().await;
        let response = map_network_error(response)?;

        if !response.status().is_success() {
            return Err(map_oauth_error(response).await);
        }

        let parsed: RawDeviceCode = response
            .json()
            .await
            .map_err(|_e| TraktError::ServiceUnavailable)?;
        Ok(DeviceCodeResponse {
            device_code: parsed.device_code,
            user_code: parsed.user_code,
            verification_url: parsed.verification_url,
            verification_url_complete: parsed.verification_url_complete,
            expires_in: parsed.expires_in,
            interval: parsed.interval,
        })
    }

    pub async fn exchange_device_code(
        &self,
        device_code: &str,
    ) -> Result<TraktTokenResponse, TraktError> {
        let url = format!("{BASE_URL}/oauth/token");
        let body = serde_json::json!({
            "code": device_code,
            "client_id": self.client_id,
            "client_secret": self.client_secret,
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
        });

        let response = self.http.post(&url).json(&body).send().await;
        let response = map_network_error(response)?;

        if response.status() == StatusCode::OK {
            return response
                .json()
                .await
                .map_err(|_| TraktError::ServiceUnavailable);
        }

        map_token_endpoint_error(response).await
    }

    pub async fn refresh_token_pair(
        &self,
        refresh_token: &str,
    ) -> Result<TraktTokenResponse, TraktError> {
        let url = format!("{BASE_URL}/oauth/token");
        let body = serde_json::json!({
            "refresh_token": refresh_token,
            "client_id": self.client_id,
            "client_secret": self.client_secret,
            "redirect_uri": self.redirect_uri,
            "grant_type": "refresh_token",
        });

        let response = self.http.post(&url).json(&body).send().await;
        let response = map_network_error(response)?;

        if response.status() == StatusCode::OK {
            return response
                .json()
                .await
                .map_err(|_| TraktError::ServiceUnavailable);
        }

        let status = response.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::BAD_REQUEST {
            return Err(TraktError::TokenExpired);
        }

        map_token_endpoint_error(response).await
    }

    pub async fn get_user_settings(
        &self,
        access_token: &str,
    ) -> Result<TraktUserSettings, TraktError> {
        let url = format!("{BASE_URL}/users/settings");

        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await;
        let response = map_network_error(response)?;

        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(TraktError::TokenExpired);
        }

        if !response.status().is_success() {
            return Err(map_oauth_error(response).await);
        }

        response
            .json()
            .await
            .map_err(|_| TraktError::ServiceUnavailable)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RawDeviceCode {
    device_code: String,
    user_code: String,
    verification_url: String,
    verification_url_complete: Option<String>,
    expires_in: i64,
    interval: i64,
}

fn map_network_error(
    result: Result<reqwest::Response, reqwest::Error>,
) -> Result<reqwest::Response, TraktError> {
    match result {
        Ok(response) => Ok(response),
        Err(e) => {
            if e.is_timeout() {
                Err(TraktError::Timeout)
            } else {
                tracing::warn!(error = %e, "Trakt API network error");
                Err(TraktError::ServiceUnavailable)
            }
        }
    }
}

async fn map_oauth_error(response: reqwest::Response) -> TraktError {
    let status = response.status();
    if status == StatusCode::TOO_MANY_REQUESTS {
        let retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok());
        return TraktError::RateLimited { retry_after_secs: retry_after };
    }

    let body = response.text().await.unwrap_or_default();
    let parsed: Option<TraktTokenErrorResponse> = serde_json::from_str(&body).ok();
    let code = parsed.as_ref().and_then(|p| p.error.as_deref()).unwrap_or("");
    let message = parsed
        .as_ref()
        .and_then(|p| p.error_description.as_deref())
        .unwrap_or("");

    map_oauth_error_code(status, code, message)
}

async fn map_token_endpoint_error(response: reqwest::Response) -> Result<TraktTokenResponse, TraktError> {
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        let retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok());
        return Err(TraktError::RateLimited { retry_after_secs: retry_after });
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let parsed: Option<TraktTokenErrorResponse> = serde_json::from_str(&body).ok();
    let code = parsed.as_ref().and_then(|p| p.error.as_deref()).unwrap_or("");
    let message = parsed
        .as_ref()
        .and_then(|p| p.error_description.as_deref())
        .unwrap_or("");

    Err(map_oauth_error_code(status, code, message))
}

fn map_oauth_error_code(status: StatusCode, code: &str, message: &str) -> TraktError {
    match code {
        "authorization_pending" => TraktError::DeviceCodePending,
        "slow_down" => TraktError::DeviceCodePending,
        "expired_token" => TraktError::DeviceCodeExpired,
        "access_denied" => TraktError::DeviceCodeDenied,
        _ if status.is_server_error() => TraktError::ServiceUnavailable,
        _ => {
            tracing::warn!(status = %status, code = %code, message = %message, "Trakt OAuth error");
            TraktError::ServiceUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_response_parses() {
        let json = r#"{
            "access_token": "abc",
            "token_type": "Bearer",
            "expires_in": 7776000,
            "refresh_token": "def",
            "scope": null,
            "created_at": 1700000000
        }"#;
        let parsed: TraktTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.access_token, "abc");
        assert_eq!(parsed.refresh_token, "def");
        assert_eq!(parsed.expires_in, 7776000);
        assert!(parsed.scope.is_none());
    }

    #[test]
    fn token_response_parses_with_scope() {
        let json = r#"{
            "access_token": "abc",
            "token_type": "Bearer",
            "expires_in": 7776000,
            "refresh_token": "def",
            "scope": "",
            "created_at": 1700000000
        }"#;
        let parsed: TraktTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.scope.as_deref(), Some(""));
    }

    #[test]
    fn user_settings_parses() {
        let json = r#"{
            "user": { "username": "sean" },
            "account": { "id": 4220132 }
        }"#;
        let parsed: TraktUserSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.user.username, "sean");
        assert_eq!(parsed.account.id, 4220132);
    }

    #[test]
    fn user_settings_ignores_extra_fields() {
        let json = r#"{
            "user": { "username": "sean", "name": "Sean", "vip": true, "ids": { "slug": "sean" } },
            "account": { "id": 4220132, "timezone_id": "America/Los_Angeles" },
            "connections": { "twitter": true }
        }"#;
        let parsed: TraktUserSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.user.username, "sean");
        assert_eq!(parsed.account.id, 4220132);
    }

    #[test]
    fn error_response_parses_pending() {
        let json = r#"{ "error": "authorization_pending" }"#;
        let parsed: TraktTokenErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.error.as_deref(), Some("authorization_pending"));
    }

    #[test]
    fn error_response_parses_slow_down_with_description() {
        let json = r#"{ "error": "slow_down", "error_description": "polling too fast" }"#;
        let parsed: TraktTokenErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.error.as_deref(), Some("slow_down"));
        assert_eq!(parsed.error_description.as_deref(), Some("polling too fast"));
    }

    #[test]
    fn map_error_pending() {
        let err = map_oauth_error_code(StatusCode::BAD_REQUEST, "authorization_pending", "");
        assert!(matches!(err, TraktError::DeviceCodePending));
    }

    #[test]
    fn map_error_slow_down() {
        let err = map_oauth_error_code(StatusCode::BAD_REQUEST, "slow_down", "");
        assert!(matches!(err, TraktError::DeviceCodePending));
    }

    #[test]
    fn map_error_expired() {
        let err = map_oauth_error_code(StatusCode::BAD_REQUEST, "expired_token", "");
        assert!(matches!(err, TraktError::DeviceCodeExpired));
    }

    #[test]
    fn map_error_denied() {
        let err = map_oauth_error_code(StatusCode::BAD_REQUEST, "access_denied", "");
        assert!(matches!(err, TraktError::DeviceCodeDenied));
    }

    #[test]
    fn map_error_server_5xx() {
        let err = map_oauth_error_code(StatusCode::INTERNAL_SERVER_ERROR, "", "");
        assert!(matches!(err, TraktError::ServiceUnavailable));
    }

    #[test]
    fn map_error_unknown_falls_back_to_service_unavailable() {
        let err = map_oauth_error_code(StatusCode::BAD_REQUEST, "invalid_request", "weird");
        assert!(matches!(err, TraktError::ServiceUnavailable));
    }

    #[test]
    fn client_constructs_with_defaults() {
        let client = TraktClient::new(
            "cid".to_string(),
            "csecret".to_string(),
            "http://localhost:48027/trakt/callback".to_string(),
        );
        assert_eq!(client.client_id(), "cid");
    }

    #[test]
    fn client_constructs_with_empty_creds() {
        let client = TraktClient::new(String::new(), String::new(), String::new());
        assert_eq!(client.client_id(), "");
    }

    #[test]
    fn raw_device_code_parses() {
        let json = r#"{
            "device_code": "DC",
            "user_code": "ABCD1234",
            "verification_url": "https://trakt.tv/activate",
            "verification_url_complete": "https://trakt.tv/activate?user_code=ABCD1234",
            "expires_in": 600,
            "interval": 5
        }"#;
        let parsed: RawDeviceCode = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.device_code, "DC");
        assert_eq!(parsed.interval, 5);
        assert_eq!(
            parsed.verification_url_complete.as_deref(),
            Some("https://trakt.tv/activate?user_code=ABCD1234")
        );
    }

    #[test]
    fn raw_device_code_parses_without_complete_url() {
        let json = r#"{
            "device_code": "DC",
            "user_code": "ABCD1234",
            "verification_url": "https://trakt.tv/activate",
            "expires_in": 600,
            "interval": 5
        }"#;
        let parsed: RawDeviceCode = serde_json::from_str(json).unwrap();
        assert!(parsed.verification_url_complete.is_none());
    }
}
