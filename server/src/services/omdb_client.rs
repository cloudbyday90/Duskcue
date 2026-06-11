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

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::metadata::{MetadataError, MetadataResult, RatingsData, RatingsProvider};

const BASE_URL: &str = "https://www.omdbapi.com";

#[derive(Debug, Clone, Deserialize)]
#[allow(non_snake_case)]
struct OmdbRating {
    Source: Option<String>,
    Value: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(non_snake_case)]
struct OmdbResponse {
    Response: Option<String>,
    Error: Option<String>,

    #[allow(dead_code)]
    Title: Option<String>,
    #[allow(dead_code)]
    Year: Option<String>,
    #[allow(dead_code)]
    Type: Option<String>,
    Rated: Option<String>,
    Awards: Option<String>,
    Metascore: Option<String>,
    imdbRating: Option<String>,
    imdbVotes: Option<String>,
    #[allow(dead_code)]
    imdbID: Option<String>,
    Ratings: Option<Vec<OmdbRating>>,
}

pub struct OmdbClient {
    api_key: String,
    http: Client,
}

impl OmdbClient {
    pub fn new(api_key: String) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();

        Self { api_key, http }
    }

    async fn fetch_by_imdb_id(&self, imdb_id: &str) -> MetadataResult<OmdbResponse> {
        let url = format!(
            "{BASE_URL}/?i={}&apikey={}",
            urlencoding::encode(imdb_id),
            self.api_key,
        );

        let response = self
            .http
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| MetadataError::NetworkError {
                provider: "omdb".to_string(),
                message: e.to_string(),
            })?;

        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(MetadataError::AuthenticationFailed {
                provider: "omdb".to_string(),
            });
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(MetadataError::InvalidResponse {
                provider: "omdb".to_string(),
                message: format!("HTTP {status}: {body}"),
            });
        }

        let body = response
            .text()
            .await
            .map_err(|e| MetadataError::NetworkError {
                provider: "omdb".to_string(),
                message: e.to_string(),
            })?;

        let omdb: OmdbResponse =
            serde_json::from_str(&body).map_err(|e| MetadataError::InvalidResponse {
                provider: "omdb".to_string(),
                message: format!("JSON parse error: {e}"),
            })?;

        if omdb.Response.as_deref() != Some("True") {
            let error_msg = omdb.Error.as_deref().unwrap_or("Unknown error");

            if error_msg.contains("not found") {
                return Err(MetadataError::NotFound {
                    provider: "omdb".to_string(),
                    id: imdb_id.to_string(),
                });
            }

            if error_msg.contains("Invalid API key") {
                return Err(MetadataError::AuthenticationFailed {
                    provider: "omdb".to_string(),
                });
            }

            return Err(MetadataError::InvalidResponse {
                provider: "omdb".to_string(),
                message: error_msg.to_string(),
            });
        }

        Ok(omdb)
    }

    pub async fn test_connection(&self) -> MetadataResult<()> {
        match self.fetch_by_imdb_id("tt0000001").await {
            Ok(_) => Ok(()),
            Err(MetadataError::AuthenticationFailed { .. }) => Err(MetadataError::AuthenticationFailed {
                provider: "omdb".to_string(),
            }),
            Err(_) => Ok(()),
        }
    }

    fn extract_rotten_tomatoes(ratings: &Option<Vec<OmdbRating>>) -> Option<String> {
        ratings
            .as_ref()?
            .iter()
            .find(|r| r.Source.as_deref() == Some("Rotten Tomatoes"))
            .and_then(|r| r.Value.clone())
    }

    fn parse_metascore(value: &Option<String>) -> Option<String> {
        value
            .as_deref()
            .filter(|v| v != &"N/A")
            .map(|s| s.to_string())
    }

    fn parse_imdb_rating(value: &Option<String>) -> Option<f64> {
        value
            .as_deref()
            .filter(|v| v != &"N/A")
            .and_then(|v| v.parse::<f64>().ok())
    }

    fn parse_imdb_votes(value: &Option<String>) -> Option<String> {
        value
            .as_deref()
            .filter(|v| v != &"N/A")
            .map(|s| s.to_string())
    }

    fn parse_string_field(value: &Option<String>) -> Option<String> {
        value
            .as_deref()
            .filter(|v| v != &"N/A")
            .map(|s| s.to_string())
    }

    fn to_ratings_data(response: OmdbResponse) -> RatingsData {
        RatingsData {
            imdb_rating: Self::parse_imdb_rating(&response.imdbRating),
            imdb_votes: Self::parse_imdb_votes(&response.imdbVotes),
            rotten_tomatoes: Self::extract_rotten_tomatoes(&response.Ratings),
            metacritic: Self::parse_metascore(&response.Metascore),
            rated: Self::parse_string_field(&response.Rated),
            awards: Self::parse_string_field(&response.Awards),
        }
    }
}

#[async_trait]
impl RatingsProvider for OmdbClient {
    fn name(&self) -> &str {
        "omdb"
    }

    fn is_configured(&self) -> bool {
        true
    }

    async fn get_ratings(&self, imdb_id: &str) -> MetadataResult<RatingsData> {
        let response = self.fetch_by_imdb_id(imdb_id).await?;
        Ok(Self::to_ratings_data(response))
    }
}
