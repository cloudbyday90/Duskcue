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

use super::metadata::{ArtworkCandidate, ArtworkProvider, MetadataError, MetadataResult};

const BASE_URL: &str = "https://webservice.fanart.tv/v3";

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct FanartImage {
    id: Option<String>,
    url: Option<String>,
    lang: Option<String>,
    likes: Option<String>,
    #[serde(default)]
    width: Option<String>,
    #[serde(default)]
    height: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct FanartMovieResponse {
    #[allow(dead_code)]
    name: Option<String>,
    #[allow(dead_code)]
    tmdb_id: Option<String>,
    movieposter: Option<Vec<FanartImage>>,
    moviebackground: Option<Vec<FanartImage>>,
    hdmovielogo: Option<Vec<FanartImage>>,
    hdmovieclearart: Option<Vec<FanartImage>>,
    moviebanner: Option<Vec<FanartImage>>,
    moviethumb: Option<Vec<FanartImage>>,
    moviedisc: Option<Vec<FanartImage>>,
    movielogo: Option<Vec<FanartImage>>,
    movieart: Option<Vec<FanartImage>>,
}

#[derive(Debug, Clone, Deserialize)]
struct FanartTvResponse {
    #[allow(dead_code)]
    name: Option<String>,
    tvposter: Option<Vec<FanartImage>>,
    showbackground: Option<Vec<FanartImage>>,
    hdtvlogo: Option<Vec<FanartImage>>,
    clearlogo: Option<Vec<FanartImage>>,
    hdclearart: Option<Vec<FanartImage>>,
    tvbanner: Option<Vec<FanartImage>>,
    tvthumb: Option<Vec<FanartImage>>,
    seasonposter: Option<Vec<FanartImage>>,
    seasonthumb: Option<Vec<FanartImage>>,
    characterart: Option<Vec<FanartImage>>,
    seasonbanner: Option<Vec<FanartImage>>,
}

pub struct FanartClient {
    api_key: String,
    http: Client,
}

impl FanartClient {
    pub fn new(api_key: String) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();

        Self { api_key, http }
    }

    async fn fetch_movie(&self, tmdb_id: u64) -> MetadataResult<FanartMovieResponse> {
        let url = format!("{BASE_URL}/movies/{tmdb_id}?api_key={}", self.api_key);
        self.get(&url).await
    }

    async fn fetch_tv(&self, tvdb_id: u64) -> MetadataResult<FanartTvResponse> {
        let url = format!("{BASE_URL}/tv/{tvdb_id}?api_key={}", self.api_key);
        self.get(&url).await
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> MetadataResult<T> {
        let response = self
            .http
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| MetadataError::NetworkError {
                provider: "fanart".to_string(),
                message: e.to_string(),
            })?;

        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(MetadataError::AuthenticationFailed {
                provider: "fanart".to_string(),
            });
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(MetadataError::NotFound {
                provider: "fanart".to_string(),
                id: url.to_string(),
            });
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(MetadataError::RateLimited {
                provider: "fanart".to_string(),
            });
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(MetadataError::InvalidResponse {
                provider: "fanart".to_string(),
                message: format!("HTTP {status}: {body}"),
            });
        }

        let body = response
            .text()
            .await
            .map_err(|e| MetadataError::NetworkError {
                provider: "fanart".to_string(),
                message: e.to_string(),
            })?;

        serde_json::from_str(&body).map_err(|e| MetadataError::InvalidResponse {
            provider: "fanart".to_string(),
            message: format!("JSON parse error: {e}"),
        })
    }

    fn convert_image(image: &FanartImage, artwork_type: &str) -> Option<ArtworkCandidate> {
        let url = image.url.as_deref()?.to_string();
        if url.is_empty() {
            return None;
        }

        if !url.starts_with("http") {
            tracing::debug!(
                url = %url,
                "Fanart.tv returned relative URL, skipping image"
            );
            return None;
        }

        let width = image
            .width
            .as_deref()
            .and_then(|w| w.parse::<u32>().ok())
            .unwrap_or(0);
        let height = image
            .height
            .as_deref()
            .and_then(|h| h.parse::<u32>().ok())
            .unwrap_or(0);
        let likes: u32 = image
            .likes
            .as_deref()
            .and_then(|l| l.parse().ok())
            .unwrap_or(0);

        Some(ArtworkCandidate {
            url,
            artwork_type: artwork_type.to_string(),
            width,
            height,
            language: image
                .lang
                .as_deref()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string()),
            vote_average: None,
            vote_count: Some(likes),
            provider: "fanart".to_string(),
        })
    }

    fn extract_images(
        images: Option<&Vec<FanartImage>>,
        artwork_type: &str,
    ) -> Vec<ArtworkCandidate> {
        images
            .map(|imgs| {
                imgs.iter()
                    .filter_map(|img| Self::convert_image(img, artwork_type))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn movie_candidates(response: &FanartMovieResponse) -> Vec<ArtworkCandidate> {
        let mut candidates = Vec::new();

        candidates.extend(Self::extract_images(response.movieposter.as_ref(), "poster"));
        candidates.extend(Self::extract_images(response.moviebackground.as_ref(), "backdrop"));
        candidates.extend(Self::extract_images(response.hdmovielogo.as_ref(), "clearlogo"));
        candidates.extend(Self::extract_images(response.movielogo.as_ref(), "clearlogo"));
        candidates.extend(Self::extract_images(response.hdmovieclearart.as_ref(), "clearart"));
        candidates.extend(Self::extract_images(response.movieart.as_ref(), "clearart"));
        candidates.extend(Self::extract_images(response.moviebanner.as_ref(), "banner"));
        candidates.extend(Self::extract_images(response.moviethumb.as_ref(), "thumbnail"));
        candidates.extend(Self::extract_images(response.moviedisc.as_ref(), "disc"));

        candidates.sort_by(|a, b| {
            b.vote_count
                .unwrap_or(0)
                .cmp(&a.vote_count.unwrap_or(0))
        });

        candidates
    }

    fn tv_candidates(response: &FanartTvResponse) -> Vec<ArtworkCandidate> {
        let mut candidates = Vec::new();

        candidates.extend(Self::extract_images(response.tvposter.as_ref(), "poster"));
        candidates.extend(Self::extract_images(response.showbackground.as_ref(), "backdrop"));
        candidates.extend(Self::extract_images(response.hdtvlogo.as_ref(), "clearlogo"));
        candidates.extend(Self::extract_images(response.clearlogo.as_ref(), "clearlogo"));
        candidates.extend(Self::extract_images(response.hdclearart.as_ref(), "clearart"));
        candidates.extend(Self::extract_images(response.tvbanner.as_ref(), "banner"));
        candidates.extend(Self::extract_images(response.tvthumb.as_ref(), "thumbnail"));
        candidates.extend(Self::extract_images(response.characterart.as_ref(), "character"));
        candidates.extend(Self::extract_images(response.seasonposter.as_ref(), "seasonposter"));
        candidates.extend(Self::extract_images(response.seasonthumb.as_ref(), "seasonthumb"));
        candidates.extend(Self::extract_images(response.seasonbanner.as_ref(), "seasonbanner"));

        candidates.sort_by(|a, b| {
            b.vote_count
                .unwrap_or(0)
                .cmp(&a.vote_count.unwrap_or(0))
        });

        candidates
    }
}

#[async_trait]
impl ArtworkProvider for FanartClient {
    fn name(&self) -> &str {
        "fanart"
    }

    fn is_configured(&self) -> bool {
        true
    }

    async fn get_movie_artwork(&self, tmdb_id: u64) -> MetadataResult<Vec<ArtworkCandidate>> {
        let response = self.fetch_movie(tmdb_id).await?;
        Ok(Self::movie_candidates(&response))
    }

    async fn get_tv_artwork(&self, tvdb_id: u64) -> MetadataResult<Vec<ArtworkCandidate>> {
        let response = self.fetch_tv(tvdb_id).await?;
        Ok(Self::tv_candidates(&response))
    }
}
