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

//! SubDL subtitle provider client.
//!
//! SubDL is the primary subtitle source. It supports direct TMDB ID and IMDb ID
//! search. Free tier: 2,000 requests/day, 300 downloads/day (IP-limited).
//! Subtitles are served as ZIP archives from `dl.subdl.com`.

use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

use crate::domains::subtitles::error::SubtitleError;

const BASE_URL: &str = "https://api.subdl.com/api/v1";
const DOWNLOAD_BASE_URL: &str = "https://dl.subdl.com";

/// Normalized subtitle search result from a provider.
#[derive(Debug, Clone)]
pub struct SubtitleSearchResult {
    pub provider: &'static str,
    pub language: String,
    pub release_name: String,
    pub file_name: String,
    pub format: String,
    pub is_hearing_impaired: bool,
    pub is_forced: bool,
    pub download_url: String,
    pub vote_count: u32,
}

/// Response from SubDL `/subtitles` endpoint.
#[derive(Debug, Deserialize)]
struct SubdlSearchResponse {
    status: bool,
    subtitles: Option<Vec<SubdlSubtitle>>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SubdlSubtitle {
    release_name: Option<String>,
    name: Option<String>,
    url: Option<String>,
    language: Option<String>,
    hi: Option<bool>,
    author: Option<String>,
    lang: Option<String>,
    format: Option<String>,
}

pub struct SubdlClient {
    api_key: String,
    http: Client,
}

impl SubdlClient {
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
        let lang_code = normalize_language_subdl(language);
        let mut url = format!(
            "{BASE_URL}/subtitles?api_key={}&tmdb_id={tmdb_id}&languages={lang_code}",
            self.api_key
        );
        if let Some(t) = item_type {
            url.push_str("&type=");
            url.push_str(t);
        }

        self.search(&url).await
    }

    /// Search subtitles by IMDb ID.
    pub async fn search_by_imdb(
        &self,
        imdb_id: &str,
        language: &str,
        item_type: Option<&str>,
    ) -> Result<Vec<SubtitleSearchResult>, SubtitleError> {
        let lang_code = normalize_language_subdl(language);
        let imdb = imdb_id.strip_prefix("tt").unwrap_or(imdb_id);
        let mut url = format!(
            "{BASE_URL}/subtitles?api_key={}&imdb_id={imdb}&languages={lang_code}",
            self.api_key
        );
        if let Some(t) = item_type {
            url.push_str("&type=");
            url.push_str(t);
        }

        self.search(&url).await
    }

    /// Search subtitles by film name (title).
    pub async fn search_by_name(
        &self,
        film_name: &str,
        language: &str,
        item_type: Option<&str>,
    ) -> Result<Vec<SubtitleSearchResult>, SubtitleError> {
        let lang_code = normalize_language_subdl(language);
        let encoded = urlencoding::encode(film_name);
        let mut url = format!(
            "{BASE_URL}/subtitles?api_key={}&film_name={encoded}&languages={lang_code}",
            self.api_key
        );
        if let Some(t) = item_type {
            url.push_str("&type=");
            url.push_str(t);
        }

        self.search(&url).await
    }

    async fn search(&self, url: &str) -> Result<Vec<SubtitleSearchResult>, SubtitleError> {
        let response = self
            .http
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| SubtitleError::FetchFailed {
                reason: format!("SubDL network error: {e}"),
            })?;

        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(SubtitleError::ProviderUnavailable {
                provider: "subdl".to_string(),
            });
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SubtitleError::ProviderRateLimited {
                provider: "subdl".to_string(),
            });
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(SubtitleError::FetchFailed {
                reason: format!("SubDL HTTP {status}: {body}"),
            });
        }

        let body = response
            .text()
            .await
            .map_err(|e| SubtitleError::FetchFailed {
                reason: format!("SubDL read error: {e}"),
            })?;

        let parsed: SubdlSearchResponse =
            serde_json::from_str(&body).map_err(|e| SubtitleError::FetchFailed {
                reason: format!("SubDL JSON parse error: {e}"),
            })?;

        if !parsed.status {
            return Err(SubtitleError::FetchFailed {
                reason: parsed
                    .error
                    .unwrap_or_else(|| "SubDL returned status=false".to_string()),
            });
        }

        let subtitles = parsed.subtitles.unwrap_or_default();
        let results: Vec<SubtitleSearchResult> = subtitles
            .into_iter()
            .filter_map(|s| convert_subdl_result(&s))
            .collect();

        Ok(results)
    }

    /// Download a subtitle file. Returns the raw bytes (may be a ZIP archive).
    pub async fn download(&self, download_url: &str) -> Result<Vec<u8>, SubtitleError> {
        let full_url = if download_url.starts_with("http") {
            download_url.to_string()
        } else {
            format!("{DOWNLOAD_BASE_URL}{download_url}")
        };

        let url = if full_url.contains("api_key=") {
            full_url
        } else if full_url.contains('?') {
            format!("{full_url}&api_key={}", self.api_key)
        } else {
            format!("{full_url}?api_key={}", self.api_key)
        };

        let response =
            self.http
                .get(&url)
                .send()
                .await
                .map_err(|e| SubtitleError::FetchFailed {
                    reason: format!("SubDL download error: {e}"),
                })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(SubtitleError::FetchFailed {
                reason: format!("SubDL download HTTP {status}: {body}"),
            });
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| SubtitleError::FetchFailed {
                reason: format!("SubDL download read error: {e}"),
            })?;

        Ok(bytes.to_vec())
    }

    pub async fn test_connection(&self) -> Result<(), SubtitleError> {
        match self.search_by_tmdb(27205, "en", Some("movie")).await {
            Ok(_) => Ok(()),
            Err(SubtitleError::ProviderUnavailable { .. }) => {
                Err(SubtitleError::ProviderUnavailable {
                    provider: "subdl".to_string(),
                })
            }
            Err(_) => Ok(()),
        }
    }
}

fn convert_subdl_result(s: &SubdlSubtitle) -> Option<SubtitleSearchResult> {
    let url = s.url.as_deref()?.to_string();
    let language = s
        .language
        .clone()
        .or_else(|| s.lang.clone())
        .unwrap_or_default();
    let normalized_lang = normalize_language_iso(&language);

    Some(SubtitleSearchResult {
        provider: "subdl",
        language: normalized_lang,
        release_name: s.release_name.clone().unwrap_or_default(),
        file_name: s.name.clone().unwrap_or_default(),
        format: s.format.clone().unwrap_or_else(|| "srt".to_string()),
        is_hearing_impaired: s.hi.unwrap_or(false),
        is_forced: false,
        download_url: url,
        vote_count: 0,
    })
}

fn normalize_language_subdl(lang: &str) -> String {
    lang.trim().to_uppercase()
}

fn normalize_language_iso(lang: &str) -> String {
    let lower = lang.trim().to_lowercase();
    match lower.as_str() {
        "english" | "en" | "eng" => "en".to_string(),
        "spanish" | "es" | "spa" | "es-es" => "es".to_string(),
        "french" | "fr" | "fre" | "fra" => "fr".to_string(),
        "german" | "de" | "deu" | "ger" => "de".to_string(),
        "italian" | "it" | "ita" => "it".to_string(),
        "portuguese" | "pt" | "por" | "pt-pt" => "pt".to_string(),
        "portuguese (brazilian)" | "pt-br" | "ptbr" => "pt-br".to_string(),
        "dutch" | "nl" | "nld" | "dut" => "nl".to_string(),
        "russian" | "ru" | "rus" => "ru".to_string(),
        "japanese" | "ja" | "jpn" => "ja".to_string(),
        "korean" | "ko" | "kor" => "ko".to_string(),
        "chinese (simplified)" | "zh-cn" | "zh" => "zh".to_string(),
        "chinese (traditional)" | "zh-tw" => "zh-tw".to_string(),
        "chinese bilingual" | "ze" => "zh".to_string(),
        "arabic" | "ar" | "ara" => "ar".to_string(),
        "hindi" | "hi" | "hin" => "hi".to_string(),
        "turkish" | "tr" | "tur" => "tr".to_string(),
        "polish" | "pl" | "pol" => "pl".to_string(),
        "swedish" | "sv" | "swe" => "sv".to_string(),
        "danish" | "da" | "dan" => "da".to_string(),
        "finnish" | "fi" | "fin" => "fi".to_string(),
        "norwegian" | "no" | "nor" => "no".to_string(),
        "czech" | "cs" | "cze" | "ces" => "cs".to_string(),
        _ => lower,
    }
}
