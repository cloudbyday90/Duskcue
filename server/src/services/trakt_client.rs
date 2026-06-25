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

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TraktIds {
    #[serde(default)]
    pub trakt: Option<i64>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub imdb: Option<String>,
    #[serde(default)]
    pub tmdb: Option<i64>,
    #[serde(default)]
    pub tvdb: Option<i64>,
}

impl TraktIds {
    pub fn is_empty(&self) -> bool {
        self.trakt.is_none()
            && self.slug.as_deref().unwrap_or("").is_empty()
            && self.imdb.as_deref().unwrap_or("").is_empty()
            && self.tmdb.is_none()
            && self.tvdb.is_none()
    }

    pub fn to_id_object(&self) -> serde_json::Value {
        let mut ids = serde_json::Map::new();
        if let Some(t) = self.trakt {
            ids.insert("trakt".into(), t.into());
        }
        if let Some(t) = self.tmdb {
            ids.insert("tmdb".into(), t.into());
        }
        if let Some(ref t) = self.imdb
            && !t.is_empty()
        {
            ids.insert("imdb".into(), t.clone().into());
        }
        if let Some(t) = self.tvdb {
            ids.insert("tvdb".into(), t.into());
        }
        serde_json::Value::Object(ids)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TraktMediaObject {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub year: Option<i32>,
    pub ids: TraktIds,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TraktEpisodeObject {
    #[serde(default)]
    pub season: Option<i32>,
    #[serde(default)]
    pub number: Option<i32>,
    #[serde(default)]
    pub title: Option<String>,
    pub ids: TraktIds,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TraktWatchedMovie {
    #[serde(default)]
    pub plays: Option<i32>,
    #[serde(default)]
    pub last_watched_at: Option<String>,
    #[serde(default)]
    pub last_updated_at: Option<String>,
    pub movie: TraktMediaObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TraktWatchedEpisode {
    #[serde(default)]
    pub plays: Option<i32>,
    #[serde(default)]
    pub last_watched_at: Option<String>,
    #[serde(default)]
    pub last_updated_at: Option<String>,
    pub episode: TraktEpisodeObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TraktRating {
    #[serde(default)]
    pub rated_at: Option<String>,
    #[serde(default)]
    pub rating: Option<i32>,
    #[serde(default, rename = "type")]
    pub rating_type: Option<String>,
    #[serde(default)]
    pub movie: Option<TraktMediaObject>,
    #[serde(default)]
    pub show: Option<TraktMediaObject>,
    #[serde(default)]
    pub episode: Option<TraktEpisodeObject>,
    #[serde(default)]
    pub season: Option<TraktEpisodeObject>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TraktCollectionMovie {
    #[serde(default)]
    pub collected_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    pub movie: TraktMediaObject,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TraktSyncCounts {
    #[serde(default)]
    pub movies: i64,
    #[serde(default)]
    pub shows: i64,
    #[serde(default)]
    pub seasons: i64,
    #[serde(default)]
    pub episodes: i64,
}

impl TraktSyncCounts {
    pub fn total(&self) -> i64 {
        self.movies + self.shows + self.seasons + self.episodes
    }
}

impl std::ops::AddAssign for TraktSyncCounts {
    fn add_assign(&mut self, other: TraktSyncCounts) {
        self.movies += other.movies;
        self.shows += other.shows;
        self.seasons += other.seasons;
        self.episodes += other.episodes;
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TraktSyncPostResponse {
    #[serde(default)]
    pub added: TraktSyncCounts,
    #[serde(default)]
    pub existing: TraktSyncCounts,
    #[serde(default)]
    pub updated: TraktSyncCounts,
    #[serde(default)]
    pub not_found: Option<serde_json::Value>,
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
        default_headers.insert("Content-Type", HeaderValue::from_static("application/json"));
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

    pub async fn get_watched_movies(
        &self,
        access_token: &str,
    ) -> Result<Vec<TraktWatchedMovie>, TraktError> {
        self.paginate(access_token, "/sync/watched/movies").await
    }

    pub async fn get_watched_episodes(
        &self,
        access_token: &str,
    ) -> Result<Vec<TraktWatchedEpisode>, TraktError> {
        self.paginate(access_token, "/sync/watched/episodes").await
    }

    pub async fn get_ratings(
        &self,
        access_token: &str,
        media_type: &str,
    ) -> Result<Vec<TraktRating>, TraktError> {
        self.paginate(access_token, &format!("/sync/ratings/{media_type}"))
            .await
    }

    pub async fn get_collection_movies(
        &self,
        access_token: &str,
    ) -> Result<Vec<TraktCollectionMovie>, TraktError> {
        self.paginate(access_token, "/sync/collection/movies").await
    }

    pub async fn add_to_history(
        &self,
        access_token: &str,
        body: &serde_json::Value,
    ) -> Result<TraktSyncPostResponse, TraktError> {
        self.authed_post(access_token, "/sync/history", body).await
    }

    pub async fn add_to_ratings(
        &self,
        access_token: &str,
        body: &serde_json::Value,
    ) -> Result<TraktSyncPostResponse, TraktError> {
        self.authed_post(access_token, "/sync/ratings", body).await
    }

    pub async fn add_to_collection(
        &self,
        access_token: &str,
        body: &serde_json::Value,
    ) -> Result<TraktSyncPostResponse, TraktError> {
        self.authed_post(access_token, "/sync/collection", body)
            .await
    }

    async fn paginate<T: serde::de::DeserializeOwned>(
        &self,
        access_token: &str,
        path: &str,
    ) -> Result<Vec<T>, TraktError> {
        const PAGE_LIMIT: u32 = 250;
        const MAX_PAGES: u32 = 1000;
        let mut all: Vec<T> = Vec::new();
        let mut page: u32 = 1;
        loop {
            let url = format!("{BASE_URL}{path}?page={page}&limit={PAGE_LIMIT}");
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
            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                return Err(extract_rate_limited(response).await);
            }
            if !response.status().is_success() {
                return Err(map_oauth_error(response).await);
            }

            let page_items: Vec<T> = response
                .json()
                .await
                .map_err(|_| TraktError::ServiceUnavailable)?;

            if page_items.is_empty() {
                break;
            }
            all.extend(page_items);

            page += 1;
            if page > MAX_PAGES {
                tracing::warn!(
                    path = path,
                    "Trakt pagination exceeded {MAX_PAGES} pages; stopping to prevent runaway loop"
                );
                break;
            }
        }
        Ok(all)
    }

    async fn authed_post(
        &self,
        access_token: &str,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<TraktSyncPostResponse, TraktError> {
        let url = format!("{BASE_URL}{path}");
        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {access_token}"))
            .json(body)
            .send()
            .await;
        let response = map_network_error(response)?;

        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(TraktError::TokenExpired);
        }
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(extract_rate_limited(response).await);
        }

        let status = response.status();
        if !status.is_success() {
            return Err(map_oauth_error(response).await);
        }

        response
            .json()
            .await
            .or_else(|_| Ok(TraktSyncPostResponse::default()))
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

async fn extract_rate_limited(response: reqwest::Response) -> TraktError {
    let retry_after = response
        .headers()
        .get("Retry-After")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok());
    TraktError::RateLimited {
        retry_after_secs: retry_after,
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
        return TraktError::RateLimited {
            retry_after_secs: retry_after,
        };
    }

    let body = response.text().await.unwrap_or_default();
    let parsed: Option<TraktTokenErrorResponse> = serde_json::from_str(&body).ok();
    let code = parsed
        .as_ref()
        .and_then(|p| p.error.as_deref())
        .unwrap_or("");
    let message = parsed
        .as_ref()
        .and_then(|p| p.error_description.as_deref())
        .unwrap_or("");

    map_oauth_error_code(status, code, message)
}

async fn map_token_endpoint_error(
    response: reqwest::Response,
) -> Result<TraktTokenResponse, TraktError> {
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        let retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok());
        return Err(TraktError::RateLimited {
            retry_after_secs: retry_after,
        });
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let parsed: Option<TraktTokenErrorResponse> = serde_json::from_str(&body).ok();
    let code = parsed
        .as_ref()
        .and_then(|p| p.error.as_deref())
        .unwrap_or("");
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
        assert_eq!(
            parsed.error_description.as_deref(),
            Some("polling too fast")
        );
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

    #[test]
    fn trakt_ids_parse_with_nulls() {
        let json = r#"{"trakt": 14007201, "imdb": null, "tmdb": 6951284, "tvdb": null}"#;
        let parsed: TraktIds = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.trakt, Some(14007201));
        assert_eq!(parsed.imdb, None);
        assert_eq!(parsed.tmdb, Some(6951284));
        assert_eq!(parsed.tvdb, None);
        assert!(!parsed.is_empty());
    }

    #[test]
    fn trakt_ids_is_empty_when_all_missing() {
        let json = r#"{}"#;
        let parsed: TraktIds = serde_json::from_str(json).unwrap();
        assert!(parsed.is_empty());
    }

    #[test]
    fn trakt_ids_to_id_object_omits_missing() {
        let ids = TraktIds {
            trakt: Some(28),
            slug: None,
            imdb: Some("tt2015381".to_string()),
            tmdb: None,
            tvdb: None,
        };
        let obj = ids.to_id_object();
        let map = obj.as_object().unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("trakt").and_then(|v| v.as_i64()), Some(28));
        assert_eq!(map.get("imdb").and_then(|v| v.as_str()), Some("tt2015381"));
        assert!(map.get("tmdb").is_none());
    }

    #[test]
    fn watched_movie_parses() {
        let json = r#"{
            "plays": 3,
            "last_watched_at": "2014-10-11T17:00:54.000Z",
            "last_updated_at": "2014-10-11T17:00:54.000Z",
            "movie": {
                "title": "Guardians of the Galaxy",
                "year": 2014,
                "ids": {"trakt": 28, "slug": "guardians-of-the-galaxy-2014", "imdb": "tt2015381", "tmdb": 118340}
            }
        }"#;
        let parsed: TraktWatchedMovie = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.plays, Some(3));
        assert_eq!(parsed.movie.ids.trakt, Some(28));
        assert_eq!(parsed.movie.year, Some(2014));
    }

    #[test]
    fn watched_episode_parses() {
        let json = r#"{
            "plays": 1,
            "last_watched_at": "2026-04-23T19:02:00.000Z",
            "episode": {
                "season": 4, "number": 8, "title": "DON'T LEAVE ME HANGING HERE",
                "ids": {"trakt": 14007201, "tvdb": 11572740, "imdb": "tt39848785", "tmdb": 6951284}
            }
        }"#;
        let parsed: TraktWatchedEpisode = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.episode.season, Some(4));
        assert_eq!(parsed.episode.number, Some(8));
        assert_eq!(parsed.episode.ids.trakt, Some(14007201));
        assert_eq!(parsed.episode.ids.tvdb, Some(11572740));
    }

    #[test]
    fn rating_parses_movie_type() {
        let json = r#"{
            "rated_at": "2014-09-01T09:10:11.000Z",
            "rating": 9,
            "type": "movie",
            "movie": {"title": "TRON: Legacy", "year": 2010, "ids": {"trakt": 1, "slug": "tron-legacy-2010", "imdb": "tt1104001", "tmdb": 20526}}
        }"#;
        let parsed: TraktRating = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.rating_type.as_deref(), Some("movie"));
        assert_eq!(parsed.rating, Some(9));
        assert!(parsed.movie.is_some());
        assert!(parsed.show.is_none());
    }

    #[test]
    fn rating_parses_episode_with_parent_show() {
        let json = r#"{
            "rated_at": "2014-09-01T09:10:11.000Z",
            "rating": 10,
            "type": "episode",
            "episode": {"season": 4, "number": 1, "title": "Box Cutter", "ids": {"trakt": 49, "imdb": "tt1683084", "tmdb": 62118, "tvdb": 2639411}},
            "show": {"title": "Breaking Bad", "year": 2008, "ids": {"trakt": 1, "slug": "breaking-bad", "imdb": "tt0903747", "tmdb": 1396, "tvdb": 81189}}
        }"#;
        let parsed: TraktRating = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.rating_type.as_deref(), Some("episode"));
        assert!(parsed.episode.is_some());
        assert!(parsed.show.is_some());
    }

    #[test]
    fn sync_post_response_parses_history() {
        let json = r#"{
            "added": {"episodes": 72, "movies": 2},
            "not_found": {"episodes": [], "movies": [{"ids": {"imdb": "tt0000111"}}], "seasons": [], "shows": []}
        }"#;
        let parsed: TraktSyncPostResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.added.episodes, 72);
        assert_eq!(parsed.added.movies, 2);
        assert_eq!(parsed.added.shows, 0);
        assert_eq!(parsed.added.total(), 74);
        assert!(parsed.existing.movies == 0);
    }

    #[test]
    fn sync_post_response_parses_collection_with_existing() {
        let json = r#"{
            "added": {"episodes": 12, "movies": 1},
            "existing": {"episodes": 0, "movies": 0},
            "updated": {"episodes": 0, "movies": 0},
            "not_found": {"movies": [], "episodes": [], "shows": [], "seasons": []}
        }"#;
        let parsed: TraktSyncPostResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.existing.movies, 0);
        assert_eq!(parsed.updated.movies, 0);
        assert!(parsed.not_found.is_some());
    }

    #[test]
    fn sync_counts_add_assign() {
        let mut a = TraktSyncCounts {
            movies: 2,
            episodes: 5,
            ..Default::default()
        };
        let b = TraktSyncCounts {
            movies: 3,
            shows: 1,
            ..Default::default()
        };
        a += b;
        assert_eq!(a.movies, 5);
        assert_eq!(a.episodes, 5);
        assert_eq!(a.shows, 1);
    }

    #[test]
    fn empty_ratings_array_parses() {
        let json = "[]";
        let parsed: Vec<TraktRating> = serde_json::from_str(json).unwrap();
        assert!(parsed.is_empty());
    }
}
