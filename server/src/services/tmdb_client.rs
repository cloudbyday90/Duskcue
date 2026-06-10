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

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use crate::state::TmdbProviderConfig;

use super::metadata::{
    CastEntry, CreditsData, CrewEntry, ExternalIds, ImageEntry, ImagesData,
    MetadataError, MetadataProvider, MetadataResult, MovieDetails, SearchResult,
    SeasonDetails, TvDetails, VideoEntry,
};

const BASE_URL: &str = "https://api.themoviedb.org/3";

#[derive(Debug, Clone, Deserialize)]
struct TmdbSearchResponse {
    results: Vec<TmdbSearchItem>,
    #[allow(dead_code)]
    total_results: Option<u32>,
    #[allow(dead_code)]
    total_pages: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum TmdbSearchItem {
    Movie(TmdbMovieSearchItem),
    Tv(TmdbTvSearchItem),
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbMovieSearchItem {
    id: u64,
    title: String,
    original_title: Option<String>,
    overview: Option<String>,
    release_date: Option<String>,
    #[allow(dead_code)]
    adult: Option<bool>,
    backdrop_path: Option<String>,
    poster_path: Option<String>,
    #[allow(dead_code)]
    genre_ids: Option<Vec<u32>>,
    popularity: Option<f64>,
    vote_average: Option<f64>,
    #[allow(dead_code)]
    vote_count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbTvSearchItem {
    id: u64,
    name: String,
    original_name: Option<String>,
    overview: Option<String>,
    first_air_date: Option<String>,
    backdrop_path: Option<String>,
    poster_path: Option<String>,
    #[allow(dead_code)]
    genre_ids: Option<Vec<u32>>,
    popularity: Option<f64>,
    vote_average: Option<f64>,
    #[allow(dead_code)]
    vote_count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbMovieDetailsResponse {
    id: u64,
    title: String,
    original_title: Option<String>,
    #[allow(dead_code)]
    original_language: Option<String>,
    overview: Option<String>,
    tagline: Option<String>,
    release_date: Option<String>,
    runtime: Option<u32>,
    vote_average: Option<f64>,
    #[allow(dead_code)]
    vote_count: Option<u32>,
    popularity: Option<f64>,
    adult: Option<bool>,
    backdrop_path: Option<String>,
    poster_path: Option<String>,
    imdb_id: Option<String>,
    genres: Option<Vec<TmdbGenre>>,
    production_companies: Option<Vec<TmdbProductionCompany>>,
    credits: Option<TmdbCredits>,
    videos: Option<TmdbVideos>,
    images: Option<TmdbImages>,
    external_ids: Option<TmdbExternalIds>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbTvDetailsResponse {
    id: u64,
    name: String,
    original_name: Option<String>,
    #[allow(dead_code)]
    original_language: Option<String>,
    overview: Option<String>,
    tagline: Option<String>,
    first_air_date: Option<String>,
    last_air_date: Option<String>,
    number_of_seasons: Option<u32>,
    number_of_episodes: Option<u32>,
    vote_average: Option<f64>,
    #[allow(dead_code)]
    vote_count: Option<u32>,
    popularity: Option<f64>,
    backdrop_path: Option<String>,
    poster_path: Option<String>,
    genres: Option<Vec<TmdbGenre>>,
    networks: Option<Vec<TmdbNetwork>>,
    credits: Option<TmdbCredits>,
    videos: Option<TmdbVideos>,
    images: Option<TmdbImages>,
    external_ids: Option<TmdbExternalIds>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbSeasonDetailsResponse {
    id: u64,
    season_number: Option<u32>,
    name: Option<String>,
    overview: Option<String>,
    air_date: Option<String>,
    #[allow(dead_code)]
    episode_count: Option<u32>,
    poster_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbGenre {
    id: u32,
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbProductionCompany {
    id: u32,
    name: String,
    logo_path: Option<String>,
    origin_country: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbNetwork {
    id: u32,
    name: String,
    logo_path: Option<String>,
    origin_country: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbCredits {
    cast: Option<Vec<TmdbCast>>,
    crew: Option<Vec<TmdbCrew>>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbCast {
    id: u64,
    name: Option<String>,
    character: Option<String>,
    order: Option<u32>,
    profile_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbCrew {
    id: u64,
    name: Option<String>,
    job: Option<String>,
    department: Option<String>,
    profile_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbVideos {
    results: Option<Vec<TmdbVideo>>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbVideo {
    id: Option<String>,
    key: Option<String>,
    name: Option<String>,
    site: Option<String>,
    #[serde(rename = "type")]
    video_type: Option<String>,
    official: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbImages {
    posters: Option<Vec<TmdbImage>>,
    backdrops: Option<Vec<TmdbImage>>,
    logos: Option<Vec<TmdbImage>>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbImage {
    file_path: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    aspect_ratio: Option<f64>,
    vote_average: Option<f64>,
    vote_count: Option<u32>,
    #[serde(rename = "iso_639_1")]
    language: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbExternalIds {
    imdb_id: Option<String>,
    tvdb_id: Option<u64>,
    facebook_id: Option<String>,
    instagram_id: Option<String>,
    twitter_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbFindResponse {
    movie_results: Option<Vec<TmdbMovieSearchItem>>,
    tv_results: Option<Vec<TmdbTvSearchItem>>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbConfigResponse {
    images: Option<TmdbConfigImages>,
    #[allow(dead_code)]
    change_keys: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbConfigImages {
    base_url: Option<String>,
    secure_base_url: Option<String>,
    poster_sizes: Option<Vec<String>>,
    backdrop_sizes: Option<Vec<String>>,
    logo_sizes: Option<Vec<String>>,
    profile_sizes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbErrorResponse {
    #[allow(dead_code)]
    status_message: Option<String>,
    #[allow(dead_code)]
    status_code: Option<i32>,
}

#[derive(Clone)]
pub struct TmdbClient {
    config: TmdbProviderConfig,
    language: String,
    http: Client,
}

impl TmdbClient {
    pub fn new(config: &TmdbProviderConfig, language: String) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();

        Self {
            config: config.clone(),
            language,
            http,
        }
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> MetadataResult<T> {
        let url = format!("{BASE_URL}{path}");

        let response = self
            .http
            .get(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.access_token),
            )
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| MetadataError::NetworkError {
                provider: "tmdb".to_string(),
                message: e.to_string(),
            })?;

        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(MetadataError::AuthenticationFailed {
                provider: "tmdb".to_string(),
            });
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(MetadataError::NotFound {
                provider: "tmdb".to_string(),
                id: path.to_string(),
            });
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(MetadataError::RateLimited {
                provider: "tmdb".to_string(),
            });
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let error: Option<TmdbErrorResponse> = serde_json::from_str(&body).ok();
            let message = error
                .and_then(|e| e.status_message)
                .unwrap_or_else(|| format!("HTTP {status}: {body}"));
            return Err(MetadataError::InvalidResponse {
                provider: "tmdb".to_string(),
                message,
            });
        }

        let body = response
            .text()
            .await
            .map_err(|e| MetadataError::NetworkError {
                provider: "tmdb".to_string(),
                message: e.to_string(),
            })?;

        serde_json::from_str(&body).map_err(|e| MetadataError::InvalidResponse {
            provider: "tmdb".to_string(),
            message: format!("JSON parse error: {e}"),
        })
    }

    pub async fn fetch_configuration(
        &self,
    ) -> MetadataResult<super::metadata::TmdbConfig> {
        let resp: TmdbConfigResponse = self.get("/configuration").await?;

        let images = resp.images;
        Ok(super::metadata::TmdbConfig {
            image_base_url: images
                .as_ref()
                .and_then(|i| i.base_url.clone())
                .unwrap_or_else(|| "https://image.tmdb.org/t/p/".to_string()),
            secure_image_base_url: images
                .as_ref()
                .and_then(|i| i.secure_base_url.clone())
                .unwrap_or_else(|| "https://image.tmdb.org/t/p/".to_string()),
            poster_sizes: images
                .as_ref()
                .and_then(|i| i.poster_sizes.clone())
                .unwrap_or_else(|| {
                    vec![
                        "w92".into(),
                        "w154".into(),
                        "w185".into(),
                        "w342".into(),
                        "w500".into(),
                        "w780".into(),
                        "original".into(),
                    ]
                }),
            backdrop_sizes: images
                .as_ref()
                .and_then(|i| i.backdrop_sizes.clone())
                .unwrap_or_else(|| {
                    vec![
                        "w300".into(),
                        "w780".into(),
                        "w1280".into(),
                        "original".into(),
                    ]
                }),
            logo_sizes: images
                .as_ref()
                .and_then(|i| i.logo_sizes.clone())
                .unwrap_or_else(|| {
                    vec![
                        "w45".into(),
                        "w92".into(),
                        "w154".into(),
                        "w185".into(),
                        "w300".into(),
                        "w500".into(),
                        "original".into(),
                    ]
                }),
            profile_sizes: images
                .as_ref()
                .and_then(|i| i.profile_sizes.clone())
                .unwrap_or_else(|| {
                    vec![
                        "w45".into(),
                        "w185".into(),
                        "h632".into(),
                        "original".into(),
                    ]
                }),
            change_keys: resp.change_keys.unwrap_or_default(),
        })
    }

    fn movie_search_to_result(item: TmdbMovieSearchItem) -> SearchResult {
        let year = item
            .release_date
            .as_deref()
            .and_then(|d| d.get(..4))
            .and_then(|y| y.parse::<u32>().ok());
        SearchResult {
            provider_id: item.id,
            title: item.title,
            original_title: item.original_title,
            year,
            overview: item.overview,
            media_type: "movie".to_string(),
            popularity: item.popularity,
            vote_average: item.vote_average,
            poster_path: item.poster_path,
            backdrop_path: item.backdrop_path,
        }
    }

    fn tv_search_to_result(item: TmdbTvSearchItem) -> SearchResult {
        let year = item
            .first_air_date
            .as_deref()
            .and_then(|d| d.get(..4))
            .and_then(|y| y.parse::<u32>().ok());
        SearchResult {
            provider_id: item.id,
            title: item.name,
            original_title: item.original_name,
            year,
            overview: item.overview,
            media_type: "tv".to_string(),
            popularity: item.popularity,
            vote_average: item.vote_average,
            poster_path: item.poster_path,
            backdrop_path: item.backdrop_path,
        }
    }

    fn convert_credits(credits: TmdbCredits) -> CreditsData {
        CreditsData {
            cast: credits
                .cast
                .unwrap_or_default()
                .into_iter()
                .filter_map(|c| {
                    Some(CastEntry {
                        id: c.id,
                        name: c.name?,
                        character: c.character,
                        order: c.order,
                        profile_path: c.profile_path,
                    })
                })
                .collect(),
            crew: credits
                .crew
                .unwrap_or_default()
                .into_iter()
                .filter_map(|c| {
                    Some(CrewEntry {
                        id: c.id,
                        name: c.name?,
                        job: c.job,
                        department: c.department,
                        profile_path: c.profile_path,
                    })
                })
                .collect(),
        }
    }

    fn convert_videos(videos: TmdbVideos) -> Vec<VideoEntry> {
        videos
            .results
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| {
                Some(VideoEntry {
                    id: v.id?,
                    key: v.key?,
                    name: v.name?,
                    site: v.site?,
                    video_type: v.video_type?,
                    official: v.official?,
                })
            })
            .collect()
    }

    fn convert_images(images: TmdbImages) -> ImagesData {
        fn convert_list(items: Vec<TmdbImage>) -> Vec<ImageEntry> {
            items
                .into_iter()
                .filter_map(|i| {
                    Some(ImageEntry {
                        file_path: i.file_path?,
                        width: i.width?,
                        height: i.height?,
                        aspect_ratio: i.aspect_ratio?,
                        vote_average: i.vote_average?,
                        vote_count: i.vote_count?,
                        language: i.language,
                    })
                })
                .collect()
        }

        ImagesData {
            posters: convert_list(images.posters.unwrap_or_default()),
            backdrops: convert_list(images.backdrops.unwrap_or_default()),
            logos: convert_list(images.logos.unwrap_or_default()),
        }
    }

    fn convert_external_ids(ids: TmdbExternalIds) -> ExternalIds {
        ExternalIds {
            imdb_id: ids.imdb_id,
            tvdb_id: ids.tvdb_id,
            facebook_id: ids.facebook_id,
            instagram_id: ids.instagram_id,
            twitter_id: ids.twitter_id,
        }
    }
}

#[async_trait]
impl MetadataProvider for TmdbClient {
    fn name(&self) -> &str {
        "tmdb"
    }

    fn is_configured(&self) -> bool {
        !self.config.access_token.is_empty()
    }

    async fn test_connection(&self) -> MetadataResult<()> {
        self.get::<TmdbConfigResponse>("/configuration").await?;
        Ok(())
    }

    async fn search_movie(
        &self,
        query: &str,
        year: Option<u32>,
    ) -> MetadataResult<Vec<SearchResult>> {
        let mut path = format!(
            "/search/movie?query={}&language={}&page=1&include_adult={}",
            urlencoding::encode(query),
            self.language,
            self.config.include_adult,
        );
        if let Some(y) = year {
            path = format!("{path}&primary_release_year={y}");
        }

        let resp: TmdbSearchResponse = self.get(&path).await?;
        let results = resp
            .results
            .into_iter()
            .filter_map(|item| match item {
                TmdbSearchItem::Movie(m) => Some(Self::movie_search_to_result(m)),
                _ => None,
            })
            .collect();
        Ok(results)
    }

    async fn search_tv(
        &self,
        query: &str,
        year: Option<u32>,
    ) -> MetadataResult<Vec<SearchResult>> {
        let mut path = format!(
            "/search/tv?query={}&language={}&page=1&include_adult={}",
            urlencoding::encode(query),
            self.language,
            self.config.include_adult,
        );
        if let Some(y) = year {
            path = format!("{path}&first_air_date_year={y}");
        }

        let resp: TmdbSearchResponse = self.get(&path).await?;
        let results = resp
            .results
            .into_iter()
            .filter_map(|item| match item {
                TmdbSearchItem::Tv(t) => Some(Self::tv_search_to_result(t)),
                _ => None,
            })
            .collect();
        Ok(results)
    }

    async fn get_movie_details(&self, id: u64) -> MetadataResult<MovieDetails> {
        let append = "credits,videos,external_ids,images";
        let image_langs = format!("{},null", self.language);
        let path = format!(
            "/movie/{id}?language={}&append_to_response={append}&include_image_language={image_langs}",
            self.language,
        );

        let resp: TmdbMovieDetailsResponse = self.get(&path).await?;

        let credits = resp.credits.map(Self::convert_credits);
        let videos = resp.videos.map(Self::convert_videos);
        let images = resp.images.map(Self::convert_images);
        let external_ids = resp.external_ids.map(Self::convert_external_ids);

        Ok(MovieDetails {
            provider_id: resp.id,
            title: resp.title,
            original_title: resp.original_title,
            overview: resp.overview,
            tagline: resp.tagline,
            release_date: resp.release_date,
            runtime: resp.runtime,
            vote_average: resp.vote_average,
            vote_count: resp.vote_count,
            popularity: resp.popularity,
            adult: resp.adult.unwrap_or(false),
            backdrop_path: resp.backdrop_path,
            poster_path: resp.poster_path,
            imdb_id: resp.imdb_id,
            tvdb_id: external_ids.as_ref().and_then(|e| e.tvdb_id),
            genres: resp
                .genres
                .unwrap_or_default()
                .into_iter()
                .map(|g| super::metadata::GenreEntry {
                    id: g.id,
                    name: g.name,
                })
                .collect(),
            production_companies: resp
                .production_companies
                .unwrap_or_default()
                .into_iter()
                .map(|p| super::metadata::ProductionCompany {
                    id: p.id,
                    name: p.name,
                    logo_path: p.logo_path,
                    origin_country: p.origin_country,
                })
                .collect(),
            credits,
            videos: Some(videos.unwrap_or_default()),
            images,
            external_ids,
        })
    }

    async fn get_tv_details(&self, id: u64) -> MetadataResult<TvDetails> {
        let append = "credits,videos,external_ids,images";
        let image_langs = format!("{},null", self.language);
        let path = format!(
            "/tv/{id}?language={}&append_to_response={append}&include_image_language={image_langs}",
            self.language,
        );

        let resp: TmdbTvDetailsResponse = self.get(&path).await?;

        let credits = resp.credits.map(Self::convert_credits);
        let videos = resp.videos.map(Self::convert_videos);
        let images = resp.images.map(Self::convert_images);
        let external_ids = resp.external_ids.map(Self::convert_external_ids);

        Ok(TvDetails {
            provider_id: resp.id,
            name: resp.name,
            original_name: resp.original_name,
            overview: resp.overview,
            tagline: resp.tagline,
            first_air_date: resp.first_air_date,
            last_air_date: resp.last_air_date,
            number_of_seasons: resp.number_of_seasons,
            number_of_episodes: resp.number_of_episodes,
            vote_average: resp.vote_average,
            vote_count: resp.vote_count,
            popularity: resp.popularity,
            backdrop_path: resp.backdrop_path,
            poster_path: resp.poster_path,
            imdb_id: external_ids.as_ref().and_then(|e| e.imdb_id.clone()),
            tvdb_id: external_ids.as_ref().and_then(|e| e.tvdb_id),
            genres: resp
                .genres
                .unwrap_or_default()
                .into_iter()
                .map(|g| super::metadata::GenreEntry {
                    id: g.id,
                    name: g.name,
                })
                .collect(),
            networks: resp
                .networks
                .unwrap_or_default()
                .into_iter()
                .map(|n| super::metadata::NetworkEntry {
                    id: n.id,
                    name: n.name,
                    logo_path: n.logo_path,
                    origin_country: n.origin_country,
                })
                .collect(),
            credits,
            videos: Some(videos.unwrap_or_default()),
            images,
            external_ids,
        })
    }

    async fn get_season_details(
        &self,
        tv_id: u64,
        season: u32,
    ) -> MetadataResult<SeasonDetails> {
        let path = format!("/tv/{tv_id}/season/{season}?language={}", self.language);
        let resp: TmdbSeasonDetailsResponse = self.get(&path).await?;

        Ok(SeasonDetails {
            provider_id: resp.id,
            season_number: resp.season_number.unwrap_or(season),
            name: resp.name,
            overview: resp.overview,
            air_date: resp.air_date,
            episode_count: resp.episode_count,
            poster_path: resp.poster_path,
        })
    }

    async fn find_by_imdb_id(&self, imdb_id: &str) -> MetadataResult<Option<SearchResult>> {
        let path = format!(
            "/find/{imdb_id}?external_source=imdb_id&language={}",
            self.language,
        );

        let resp: TmdbFindResponse = self.get(&path).await?;

        if let Some(movies) = &resp.movie_results
            && let Some(m) = movies.first()
        {
            return Ok(Some(Self::movie_search_to_result(m.clone())));
        }

        if let Some(tv) = &resp.tv_results
            && let Some(t) = tv.first()
        {
            return Ok(Some(Self::tv_search_to_result(t.clone())));
        }

        Ok(None)
    }
}
