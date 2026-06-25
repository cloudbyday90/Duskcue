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

//! OpenSubtitles subtitle provider client.
//!
//! OpenSubtitles is the fallback subtitle source. It supports hash-based,
//! filename-based, and TMDB/IMDb ID search. Free tier: 5 downloads per IP
//! per 24 hours. Requires both `Api-Key` and `User-Agent` headers.
//! Base URL: `https://api.opensubtitles.com/api/v1`

use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

use crate::domains::subtitles::error::SubtitleError;
use crate::services::subdl_client::SubtitleSearchResult;

const BASE_URL: &str = "https://api.opensubtitles.com/api/v1";
const USER_AGENT: &str = "Duskcue v1.0";

/// Response from OpenSubtitles `/subtitles` endpoint.
#[derive(Debug, Deserialize)]
struct OsSearchResponse {
    data: Option<Vec<OsSubtitle>>,
}

#[derive(Debug, Deserialize)]
struct OsSubtitle {
    id: Option<String>,
    attributes: Option<OsSubtitleAttributes>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OsSubtitleAttributes {
    language: Option<String>,
    release: Option<String>,
    #[serde(rename = "file_name")]
    file_name: Option<String>,
    format: Option<String>,
    #[serde(rename = "download_count")]
    download_count: Option<u64>,
    #[serde(rename = "feature_id")]
    feature_id: Option<String>,
    #[serde(rename = "feature_type")]
    feature_type: Option<String>,
    #[serde(rename = "file_id")]
    file_id: Option<u64>,
    #[serde(rename = "fps")]
    fps: Option<f64>,
    #[serde(rename = "hearing_impaired")]
    hearing_impaired: Option<bool>,
    forced: Option<bool>,
}

/// Response from OpenSubtitles `/download` endpoint.
#[derive(Debug, Deserialize)]
struct OsDownloadResponse {
    #[serde(rename = "file_name")]
    file_name: Option<String>,
    link: String,
}

pub struct OpensubtitlesClient {
    api_key: String,
    http: Client,
}

impl OpensubtitlesClient {
    pub fn new(api_key: String) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();

        Self { api_key, http }
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }

    /// Search subtitles by TMDB ID.
    pub async fn search_by_tmdb(
        &self,
        tmdb_id: u64,
        language: &str,
        item_type: Option<&str>,
    ) -> Result<Vec<SubtitleSearchResult>, SubtitleError> {
        let tmdb_type = match item_type {
            Some("tv") => "tv",
            _ => "movie",
        };

        let lang_code = normalize_language_os(language);
        let url = format!(
            "{BASE_URL}/subtitles?tmdb_id={tmdb_id}&tmdb_type={tmdb_type}&languages={lang_code}"
        );

        self.search(&url).await
    }

    /// Search subtitles by IMDb ID.
    pub async fn search_by_imdb(
        &self,
        imdb_id: &str,
        language: &str,
        item_type: Option<&str>,
    ) -> Result<Vec<SubtitleSearchResult>, SubtitleError> {
        let imdb = imdb_id.strip_prefix("tt").unwrap_or(imdb_id);
        let imdb_type = match item_type {
            Some("tv") => "tv",
            _ => "movie",
        };

        let lang_code = normalize_language_os(language);
        let url =
            format!("{BASE_URL}/subtitles?imdb_id={imdb}&type={imdb_type}&languages={lang_code}");

        self.search(&url).await
    }

    /// Search subtitles by file hash + file size.
    pub async fn search_by_hash(
        &self,
        hash: &str,
        file_size: u64,
        language: &str,
    ) -> Result<Vec<SubtitleSearchResult>, SubtitleError> {
        let lang_code = normalize_language_os(language);
        let url = format!(
            "{BASE_URL}/subtitles?moviehash={hash}&moviebytesize={file_size}&languages={lang_code}"
        );

        self.search(&url).await
    }

    /// Search subtitles by file name (query).
    pub async fn search_by_query(
        &self,
        query: &str,
        language: &str,
    ) -> Result<Vec<SubtitleSearchResult>, SubtitleError> {
        let lang_code = normalize_language_os(language);
        let encoded = urlencoding::encode(query);
        let url = format!("{BASE_URL}/subtitles?query={encoded}&languages={lang_code}");

        self.search(&url).await
    }

    async fn search(&self, url: &str) -> Result<Vec<SubtitleSearchResult>, SubtitleError> {
        let response = self
            .http
            .get(url)
            .header("Api-Key", &self.api_key)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| SubtitleError::FetchFailed {
                reason: format!("OpenSubtitles network error: {e}"),
            })?;

        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(SubtitleError::ProviderUnavailable {
                provider: "opensubtitles".to_string(),
            });
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SubtitleError::ProviderRateLimited {
                provider: "opensubtitles".to_string(),
            });
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(SubtitleError::FetchFailed {
                reason: format!("OpenSubtitles HTTP {status}: {body}"),
            });
        }

        let body = response
            .text()
            .await
            .map_err(|e| SubtitleError::FetchFailed {
                reason: format!("OpenSubtitles read error: {e}"),
            })?;

        let parsed: OsSearchResponse =
            serde_json::from_str(&body).map_err(|e| SubtitleError::FetchFailed {
                reason: format!("OpenSubtitles JSON parse error: {e}"),
            })?;

        let subtitles = parsed.data.unwrap_or_default();
        let results: Vec<SubtitleSearchResult> = subtitles
            .into_iter()
            .filter_map(|s| convert_os_result(&s))
            .collect();

        Ok(results)
    }

    /// Download a subtitle file. Uses the two-step download flow.
    /// Returns the raw subtitle bytes and the server-side filename.
    pub async fn download(&self, file_id: u64) -> Result<(Vec<u8>, String), SubtitleError> {
        let response = self
            .http
            .post(format!("{BASE_URL}/download"))
            .header("Api-Key", &self.api_key)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .body(format!("{{\"file_id\":{file_id}}}"))
            .send()
            .await
            .map_err(|e| SubtitleError::FetchFailed {
                reason: format!("OpenSubtitles download request error: {e}"),
            })?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SubtitleError::ProviderRateLimited {
                provider: "opensubtitles".to_string(),
            });
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(SubtitleError::FetchFailed {
                reason: format!("OpenSubtitles download HTTP {status}: {body}"),
            });
        }

        let body = response
            .text()
            .await
            .map_err(|e| SubtitleError::FetchFailed {
                reason: format!("OpenSubtitles download read error: {e}"),
            })?;

        let parsed: OsDownloadResponse =
            serde_json::from_str(&body).map_err(|e| SubtitleError::FetchFailed {
                reason: format!("OpenSubtitles download JSON parse error: {e}"),
            })?;

        let file_name = parsed
            .file_name
            .unwrap_or_else(|| format!("subtitle_{file_id}.srt"));

        let download_response = self
            .http
            .get(&parsed.link)
            .header("User-Agent", USER_AGENT)
            .send()
            .await
            .map_err(|e| SubtitleError::FetchFailed {
                reason: format!("OpenSubtitles file download error: {e}"),
            })?;

        let dl_status = download_response.status();
        if !dl_status.is_success() {
            let body = download_response.text().await.unwrap_or_default();
            return Err(SubtitleError::FetchFailed {
                reason: format!("OpenSubtitles file download HTTP {dl_status}: {body}"),
            });
        }

        let bytes = download_response
            .bytes()
            .await
            .map_err(|e| SubtitleError::FetchFailed {
                reason: format!("OpenSubtitles file download read error: {e}"),
            })?;

        Ok((bytes.to_vec(), file_name))
    }

    pub async fn test_connection(&self) -> Result<(), SubtitleError> {
        match self.search_by_tmdb(27205, "en", Some("movie")).await {
            Ok(_) => Ok(()),
            Err(SubtitleError::ProviderUnavailable { .. }) => {
                Err(SubtitleError::ProviderUnavailable {
                    provider: "opensubtitles".to_string(),
                })
            }
            Err(_) => Ok(()),
        }
    }
}

fn convert_os_result(s: &OsSubtitle) -> Option<SubtitleSearchResult> {
    let attrs = s.attributes.as_ref()?;
    let file_id = attrs.file_id?;
    let id = s.id.as_deref().unwrap_or("0");

    Some(SubtitleSearchResult {
        provider: "opensubtitles",
        language: attrs.language.clone().unwrap_or_default(),
        release_name: attrs.release.clone().unwrap_or_default(),
        file_name: attrs.file_name.clone().unwrap_or_default(),
        format: attrs.format.clone().unwrap_or_else(|| "srt".to_string()),
        is_hearing_impaired: attrs.hearing_impaired.unwrap_or(false),
        is_forced: attrs.forced.unwrap_or(false),
        download_url: format!("{id}:{file_id}"),
        vote_count: attrs.download_count.unwrap_or(0) as u32,
    })
}

fn normalize_language_os(lang: &str) -> String {
    lang.trim().to_lowercase()
}
