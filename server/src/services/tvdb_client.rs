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

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::RwLock;

use super::metadata::{
    ArtworkCandidate, MetadataError, MetadataProvider, MetadataResult, MovieDetails,
    SearchResult, SeasonDetails, TvDetails,
};

const BASE_URL: &str = "https://api4.thetvdb.com/v4";
const TOKEN_REFRESH_BUFFER: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TvdbResponse<T> {
    status: Option<String>,
    data: Option<T>,
}

#[derive(Debug, Clone, Deserialize)]
struct TvdbLoginResponse {
    token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TvdbSearchResult {
    object_id: Option<String>,
    tvdb_id: Option<String>,
    r#type: Option<String>,
    name: Option<String>,
    overview: Option<String>,
    year: Option<String>,
    poster: Option<String>,
    thumbnail: Option<String>,
    score: Option<f64>,
    remote_ids: Option<Vec<TvdbRemoteId>>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TvdbRemoteId {
    id: Option<String>,
    r#type: Option<i64>,
    source_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TvdbSeriesExtended {
    id: Option<i64>,
    name: Option<String>,
    overview: Option<String>,
    first_aired: Option<String>,
    last_aired: Option<String>,
    average_runtime: Option<i64>,
    original_country: Option<String>,
    original_language: Option<String>,
    score: Option<f64>,
    slug: Option<String>,
    status: Option<TvdbStatus>,
    year: Option<String>,
    image: Option<String>,
    artworks: Option<Vec<TvdbArtwork>>,
    episodes: Option<Vec<TvdbEpisode>>,
    seasons: Option<Vec<TvdbSeason>>,
    genres: Option<Vec<TvdbGenre>>,
    remote_ids: Option<Vec<TvdbRemoteId>>,
    companies: Option<TvdbCompanies>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TvdbStatus {
    id: Option<i64>,
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TvdbArtwork {
    id: Option<i64>,
    image: Option<String>,
    thumbnail: Option<String>,
    r#type: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    language: Option<String>,
    score: Option<f64>,
    includes_text: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TvdbEpisode {
    id: Option<i64>,
    name: Option<String>,
    overview: Option<String>,
    season_number: Option<i64>,
    number: Option<i64>,
    absolute_number: Option<i64>,
    aired: Option<String>,
    runtime: Option<i64>,
    image: Option<String>,
    finale_type: Option<String>,
    series_id: Option<i64>,
    seasons: Option<Vec<TvdbSeason>>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TvdbSeason {
    id: Option<i64>,
    r#type: Option<TvdbSeasonType>,
    number: Option<i64>,
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TvdbSeasonType {
    id: Option<i64>,
    r#type: Option<i64>,
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TvdbGenre {
    id: Option<i64>,
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TvdbCompanies {
    studio: Option<Vec<TvdbCompany>>,
    network: Option<Vec<TvdbCompany>>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TvdbCompany {
    id: Option<i64>,
    name: Option<String>,
    country: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TvdbEpisodesResponse {
    episodes: Option<Vec<TvdbEpisode>>,
    series: Option<TvdbSeriesExtended>,
}

#[derive(Debug, Clone, Deserialize)]
struct TvdbRemoteIdSearchResult {
    series: Option<Vec<TvdbSeriesExtended>>,
    movie: Option<Vec<TvdbMovieBase>>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TvdbMovieBase {
    id: Option<i64>,
    name: Option<String>,
    overview: Option<String>,
    runtime: Option<i64>,
    score: Option<f64>,
    year: Option<String>,
    image: Option<String>,
    slug: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TvdbErrorResponse {
    message: Option<String>,
}

struct TokenState {
    token: Option<String>,
    expires_at: Option<Instant>,
}

struct Inner {
    api_key: String,
    http: Client,
    token_state: RwLock<TokenState>,
}

#[derive(Clone)]
pub struct TvdbClient {
    inner: Arc<Inner>,
}

impl TvdbClient {
    pub fn new(api_key: String) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();

        Self {
            inner: Arc::new(Inner {
                api_key,
                http,
                token_state: RwLock::new(TokenState {
                    token: None,
                    expires_at: None,
                }),
            }),
        }
    }

    async fn ensure_token(&self) -> MetadataResult<String> {
        {
            let state = self.inner.token_state.read().await;
            if let (Some(token), Some(expires_at)) = (&state.token, state.expires_at)
                && Instant::now() + TOKEN_REFRESH_BUFFER < expires_at
            {
                return Ok(token.clone());
            }
        }

        let mut state = self.inner.token_state.write().await;
        if let (Some(token), Some(expires_at)) = (&state.token, state.expires_at)
            && Instant::now() + TOKEN_REFRESH_BUFFER < expires_at
        {
            return Ok(token.clone());
        }

        let token = self.login().await?;
        state.token = Some(token.clone());
        state.expires_at = Some(Instant::now() + Duration::from_secs(30 * 24 * 60 * 60));
        Ok(token)
    }

    async fn login(&self) -> MetadataResult<String> {
        let url = format!("{BASE_URL}/login");
        let body = serde_json::json!({ "apikey": self.inner.api_key });

        let response = self
            .inner
            .http
            .post(&url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| MetadataError::NetworkError {
                provider: "tvdb".to_string(),
                message: e.to_string(),
            })?;

        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(MetadataError::AuthenticationFailed {
                provider: "tvdb".to_string(),
            });
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let error: Option<TvdbErrorResponse> = serde_json::from_str(&body).ok();
            let message = error
                .and_then(|e| e.message)
                .unwrap_or_else(|| format!("HTTP {status}: {body}"));
            return Err(MetadataError::InvalidResponse {
                provider: "tvdb".to_string(),
                message,
            });
        }

        let wrapped: TvdbResponse<TvdbLoginResponse> = response
            .json()
            .await
            .map_err(|e| MetadataError::InvalidResponse {
                provider: "tvdb".to_string(),
                message: format!("Login response parse error: {e}"),
            })?;

        wrapped
            .data
            .and_then(|d| d.token)
            .ok_or_else(|| MetadataError::InvalidResponse {
                provider: "tvdb".to_string(),
                message: "Login response missing token".to_string(),
            })
    }

    fn clear_token(&self) {
        if let Ok(mut state) = self.inner.token_state.try_write() {
            state.token = None;
            state.expires_at = None;
        }
    }

    async fn authenticated_get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> MetadataResult<T> {
        let token = self.ensure_token().await?;
        let url = format!("{BASE_URL}{path}");

        let response = self
            .inner
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| MetadataError::NetworkError {
                provider: "tvdb".to_string(),
                message: e.to_string(),
            })?;

        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            self.clear_token();
            return Err(MetadataError::AuthenticationFailed {
                provider: "tvdb".to_string(),
            });
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(MetadataError::NotFound {
                provider: "tvdb".to_string(),
                id: path.to_string(),
            });
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(MetadataError::RateLimited {
                provider: "tvdb".to_string(),
            });
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let error: Option<TvdbErrorResponse> = serde_json::from_str(&body).ok();
            let message = error
                .and_then(|e| e.message)
                .unwrap_or_else(|| format!("HTTP {status}: {body}"));
            return Err(MetadataError::InvalidResponse {
                provider: "tvdb".to_string(),
                message,
            });
        }

        let body = response
            .text()
            .await
            .map_err(|e| MetadataError::NetworkError {
                provider: "tvdb".to_string(),
                message: e.to_string(),
            })?;

        let wrapped: TvdbResponse<T> = serde_json::from_str(&body).map_err(|e| {
            MetadataError::InvalidResponse {
                provider: "tvdb".to_string(),
                message: format!("JSON parse error: {e}"),
            }
        })?;

        wrapped.data.ok_or_else(|| MetadataError::InvalidResponse {
            provider: "tvdb".to_string(),
            message: "Response missing data field".to_string(),
        })
    }

    fn search_to_result(item: TvdbSearchResult) -> Option<SearchResult> {
        let id_str = item.tvdb_id.as_deref().or(item.object_id.as_deref())?;
        let provider_id = id_str.parse::<u64>().ok()?;
        let year = item.year.as_deref().and_then(|y| y.parse::<u32>().ok());

        Some(SearchResult {
            provider_id,
            title: item.name.unwrap_or_default(),
            original_title: None,
            year,
            overview: item.overview,
            media_type: "series".to_string(),
            popularity: item.score,
            vote_average: None,
            poster_path: item.poster,
            backdrop_path: item.thumbnail,
        })
    }

    fn series_to_tv_details(series: TvdbSeriesExtended) -> TvDetails {
        let mut imdb_id = None;
        if let Some(remote_ids) = &series.remote_ids {
            for rid in remote_ids {
                if rid.source_name.as_deref() == Some("IMDB") {
                    imdb_id = rid.id.clone();
                    break;
                }
            }
        }

        let networks = series
            .companies
            .and_then(|c| c.network)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|n| {
                Some(super::metadata::NetworkEntry {
                    id: n.id? as u32,
                    name: n.name?,
                    logo_path: None,
                    origin_country: n.country,
                })
            })
            .collect();

        let genres = series
            .genres
            .unwrap_or_default()
            .into_iter()
            .filter_map(|g| {
                Some(super::metadata::GenreEntry {
                    id: g.id? as u32,
                    name: g.name?,
                })
            })
            .collect();

        TvDetails {
            provider_id: series.id.unwrap_or(0) as u64,
            name: series.name.unwrap_or_default(),
            original_name: None,
            overview: series.overview,
            tagline: None,
            first_air_date: series.first_aired,
            last_air_date: series.last_aired,
            number_of_seasons: series
                .seasons
                .as_ref()
                .map(|s| s.len() as u32),
            number_of_episodes: series
                .episodes
                .as_ref()
                .map(|e| e.len() as u32),
            vote_average: None,
            vote_count: None,
            popularity: series.score,
            backdrop_path: None,
            poster_path: series.image,
            imdb_id,
            tvdb_id: series.id.map(|id| id as u64),
            genres,
            networks,
            credits: None,
            videos: None,
            images: None,
            external_ids: None,
        }
    }
}

#[async_trait]
impl MetadataProvider for TvdbClient {
    fn name(&self) -> &str {
        "tvdb"
    }

    fn is_configured(&self) -> bool {
        true
    }

    async fn test_connection(&self) -> MetadataResult<()> {
        self.ensure_token().await?;
        Ok(())
    }

    async fn search_movie(
        &self,
        query: &str,
        _year: Option<u32>,
    ) -> MetadataResult<Vec<SearchResult>> {
        let path = format!(
            "/search?query={}&type=movie",
            urlencoding::encode(query),
        );
        let results: Vec<TvdbSearchResult> = self.authenticated_get(&path).await?;
        Ok(results.into_iter().filter_map(Self::search_to_result).collect())
    }

    async fn search_tv(
        &self,
        query: &str,
        _year: Option<u32>,
    ) -> MetadataResult<Vec<SearchResult>> {
        let path = format!(
            "/search?query={}&type=series",
            urlencoding::encode(query),
        );
        let results: Vec<TvdbSearchResult> = self.authenticated_get(&path).await?;
        Ok(results.into_iter().filter_map(Self::search_to_result).collect())
    }

    async fn get_movie_details(&self, id: u64) -> MetadataResult<MovieDetails> {
        let path = format!("/movies/{id}");
        let movie: TvdbMovieBase = self.authenticated_get(&path).await?;

        Ok(MovieDetails {
            provider_id: movie.id.unwrap_or(id as i64) as u64,
            title: movie.name.unwrap_or_default(),
            original_title: None,
            overview: movie.overview,
            tagline: None,
            release_date: movie.year.clone(),
            runtime: movie.runtime.map(|r| r as u32),
            vote_average: None,
            vote_count: None,
            popularity: movie.score,
            adult: false,
            backdrop_path: None,
            poster_path: movie.image,
            imdb_id: None,
            tvdb_id: movie.id.map(|id| id as u64),
            genres: vec![],
            production_companies: vec![],
            credits: None,
            videos: None,
            images: None,
            external_ids: None,
        })
    }

    async fn get_tv_details(&self, id: u64) -> MetadataResult<TvDetails> {
        let path = format!("/series/{id}/extended?meta=episodes");
        let series: TvdbSeriesExtended = self.authenticated_get(&path).await?;
        Ok(Self::series_to_tv_details(series))
    }

    async fn get_season_details(
        &self,
        _tv_id: u64,
        _season: u32,
    ) -> MetadataResult<SeasonDetails> {
        Err(MetadataError::NoProviderConfigured)
    }

    async fn find_by_imdb_id(&self, imdb_id: &str) -> MetadataResult<Option<SearchResult>> {
        let path = format!(
            "/search/remoteid/{}",
            urlencoding::encode(imdb_id),
        );
        let result: TvdbRemoteIdSearchResult = self.authenticated_get(&path).await?;

        if let Some(series) = result.series.and_then(|s| s.into_iter().next()) {
            let id = series.id.unwrap_or(0) as u64;
            return Ok(Some(SearchResult {
                provider_id: id,
                title: series.name.unwrap_or_default(),
                original_title: None,
                year: series.year.as_deref().and_then(|y| y.parse::<u32>().ok()),
                overview: series.overview,
                media_type: "series".to_string(),
                popularity: series.score,
                vote_average: None,
                poster_path: series.image,
                backdrop_path: None,
            }));
        }

        if let Some(movie) = result.movie.and_then(|m| m.into_iter().next()) {
            let id = movie.id.unwrap_or(0) as u64;
            return Ok(Some(SearchResult {
                provider_id: id,
                title: movie.name.unwrap_or_default(),
                original_title: None,
                year: movie.year.as_deref().and_then(|y| y.parse::<u32>().ok()),
                overview: movie.overview,
                media_type: "movie".to_string(),
                popularity: movie.score,
                vote_average: None,
                poster_path: movie.image,
                backdrop_path: None,
            }));
        }

        Ok(None)
    }
}

#[async_trait]
impl super::metadata::ArtworkProvider for TvdbClient {
    fn name(&self) -> &str {
        "tvdb"
    }

    fn is_configured(&self) -> bool {
        true
    }

    async fn get_movie_artwork(&self, _tmdb_id: u64) -> MetadataResult<Vec<ArtworkCandidate>> {
        Ok(vec![])
    }

    async fn get_tv_artwork(&self, tvdb_id: u64) -> MetadataResult<Vec<ArtworkCandidate>> {
        let path = format!("/series/{tvdb_id}/artworks");
        let series: TvdbSeriesExtended = self.authenticated_get(&path).await?;

        let candidates = series
            .artworks
            .unwrap_or_default()
            .into_iter()
            .filter_map(|a| {
                let image = a.image?;
                let artwork_type = match a.r#type? {
                    1 => "poster",
                    2 => "banner",
                    3 => "backdrop",
                    4 => "clearlogo",
                    5 => "thumbnail",
                    _ => "other",
                };
                Some(ArtworkCandidate {
                    url: image,
                    artwork_type: artwork_type.to_string(),
                    width: a.width.unwrap_or(0) as u32,
                    height: a.height.unwrap_or(0) as u32,
                    language: a.language,
                    vote_average: a.score,
                    vote_count: None,
                    provider: "tvdb".to_string(),
                })
            })
            .collect();

        Ok(candidates)
    }
}
