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

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use governor::{
    clock::DefaultClock, state::direct::NotKeyed,
    state::InMemoryState, Quota, RateLimiter,
};
use nonzero_ext::nonzero;
use reqwest::Client;
use sqlx::PgPool;
use thiserror::Error;

use crate::state::MetadataConfig;

use super::artwork_downloader;
use super::fanart_client::FanartClient;
use super::omdb_client::OmdbClient;
use super::tmdb_client::TmdbClient;
use super::tvdb_client::TvdbClient;

pub const VALID_PROVIDERS: &[&str] = &["tmdb", "tvdb", "fanart", "omdb"];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderValidationRequest {
    pub provider: String,
    pub access_token: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderValidationResponse {
    pub provider: String,
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn validate_provider_key(
    req: &ProviderValidationRequest,
) -> ProviderValidationResponse {
    let result = match req.provider.as_str() {
        "tmdb" => validate_tmdb(req.access_token.as_deref()).await,
        "tvdb" => validate_tvdb(req.api_key.as_deref()).await,
        "fanart" => validate_fanart(req.api_key.as_deref()).await,
        "omdb" => validate_omdb(req.api_key.as_deref()).await,
        _ => Err(MetadataError::InvalidResponse {
            provider: req.provider.clone(),
            message: format!("Unknown provider: {}. Valid providers: {}", req.provider, VALID_PROVIDERS.join(", ")),
        }),
    };

    match result {
        Ok(()) => ProviderValidationResponse {
            provider: req.provider.clone(),
            valid: true,
            error: None,
        },
        Err(e) => ProviderValidationResponse {
            provider: req.provider.clone(),
            valid: false,
            error: Some(e.to_string()),
        },
    }
}

async fn validate_tmdb(access_token: Option<&str>) -> MetadataResult<()> {
    let token = access_token.ok_or(MetadataError::InvalidResponse {
        provider: "tmdb".to_string(),
        message: "access_token is required for TMDB validation".to_string(),
    })?;

    let config = crate::state::TmdbProviderConfig {
        access_token: token.to_string(),
        ..Default::default()
    };
    let client = TmdbClient::new(&config, "en".to_string());
    client.test_connection().await
}

async fn validate_tvdb(api_key: Option<&str>) -> MetadataResult<()> {
    let key = api_key.ok_or(MetadataError::InvalidResponse {
        provider: "tvdb".to_string(),
        message: "api_key is required for TVDB validation".to_string(),
    })?;

    let client = TvdbClient::new(key.to_string());
    client.test_connection().await
}

async fn validate_fanart(api_key: Option<&str>) -> MetadataResult<()> {
    let key = api_key.ok_or(MetadataError::InvalidResponse {
        provider: "fanart".to_string(),
        message: "api_key is required for Fanart.tv validation".to_string(),
    })?;

    let client = FanartClient::new(key.to_string());
    client.test_connection().await
}

async fn validate_omdb(api_key: Option<&str>) -> MetadataResult<()> {
    let key = api_key.ok_or(MetadataError::InvalidResponse {
        provider: "omdb".to_string(),
        message: "api_key is required for OMDb validation".to_string(),
    })?;

    let client = OmdbClient::new(key.to_string());
    client.test_connection().await
}

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("provider '{provider}' returned authentication failure")]
    AuthenticationFailed { provider: String },

    #[error("provider '{provider}' rate limited")]
    RateLimited { provider: String },

    #[error("provider '{provider}' returned not found for id {id}")]
    NotFound { provider: String, id: String },

    #[error("provider '{provider}' network error: {message}")]
    NetworkError { provider: String, message: String },

    #[error("provider '{provider}' invalid response: {message}")]
    InvalidResponse { provider: String, message: String },

    #[error("provider '{provider}' daily budget exhausted")]
    DailyBudgetExhausted { provider: String },

    #[error("TMDB is not configured — provide an API key and access token in settings")]
    TmdbNotConfigured,

    #[error("no provider configured for this operation")]
    NoProviderConfigured,

    #[error("enrichment timed out after {seconds}s")]
    EnrichmentTimeout { seconds: u32 },

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

pub type MetadataResult<T> = Result<T, MetadataError>;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub provider_id: u64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<u32>,
    pub overview: Option<String>,
    pub media_type: String,
    pub popularity: Option<f64>,
    pub vote_average: Option<f64>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MovieDetails {
    pub provider_id: u64,
    pub title: String,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub tagline: Option<String>,
    pub release_date: Option<String>,
    pub runtime: Option<u32>,
    pub vote_average: Option<f64>,
    pub vote_count: Option<u32>,
    pub popularity: Option<f64>,
    pub adult: bool,
    pub backdrop_path: Option<String>,
    pub poster_path: Option<String>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<u64>,
    pub genres: Vec<GenreEntry>,
    pub production_companies: Vec<ProductionCompany>,
    pub credits: Option<CreditsData>,
    pub videos: Option<Vec<VideoEntry>>,
    pub images: Option<ImagesData>,
    pub external_ids: Option<ExternalIds>,
}

#[derive(Debug, Clone)]
pub struct TvDetails {
    pub provider_id: u64,
    pub name: String,
    pub original_name: Option<String>,
    pub overview: Option<String>,
    pub tagline: Option<String>,
    pub first_air_date: Option<String>,
    pub last_air_date: Option<String>,
    pub number_of_seasons: Option<u32>,
    pub number_of_episodes: Option<u32>,
    pub vote_average: Option<f64>,
    pub vote_count: Option<u32>,
    pub popularity: Option<f64>,
    pub backdrop_path: Option<String>,
    pub poster_path: Option<String>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<u64>,
    pub genres: Vec<GenreEntry>,
    pub networks: Vec<NetworkEntry>,
    pub credits: Option<CreditsData>,
    pub videos: Option<Vec<VideoEntry>>,
    pub images: Option<ImagesData>,
    pub external_ids: Option<ExternalIds>,
}

#[derive(Debug, Clone)]
pub struct SeasonDetails {
    pub provider_id: u64,
    pub season_number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub episode_count: Option<u32>,
    pub poster_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GenreEntry {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ProductionCompany {
    pub id: u32,
    pub name: String,
    pub logo_path: Option<String>,
    pub origin_country: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NetworkEntry {
    pub id: u32,
    pub name: String,
    pub logo_path: Option<String>,
    pub origin_country: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreditsData {
    pub cast: Vec<CastEntry>,
    pub crew: Vec<CrewEntry>,
}

#[derive(Debug, Clone)]
pub struct CastEntry {
    pub id: u64,
    pub name: String,
    pub character: Option<String>,
    pub order: Option<u32>,
    pub profile_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CrewEntry {
    pub id: u64,
    pub name: String,
    pub job: Option<String>,
    pub department: Option<String>,
    pub profile_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VideoEntry {
    pub id: String,
    pub key: String,
    pub name: String,
    pub site: String,
    pub video_type: String,
    pub official: bool,
}

#[derive(Debug, Clone)]
pub struct ImagesData {
    pub posters: Vec<ImageEntry>,
    pub backdrops: Vec<ImageEntry>,
    pub logos: Vec<ImageEntry>,
}

#[derive(Debug, Clone)]
pub struct ImageEntry {
    pub file_path: String,
    pub width: u32,
    pub height: u32,
    pub aspect_ratio: f64,
    pub vote_average: f64,
    pub vote_count: u32,
    pub language: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExternalIds {
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<u64>,
    pub facebook_id: Option<String>,
    pub instagram_id: Option<String>,
    pub twitter_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ArtworkCandidate {
    pub url: String,
    pub artwork_type: String,
    pub width: u32,
    pub height: u32,
    pub language: Option<String>,
    pub vote_average: Option<f64>,
    pub vote_count: Option<u32>,
    pub provider: String,
}

#[derive(Debug, Clone)]
pub struct RatingsData {
    pub imdb_rating: Option<f64>,
    pub imdb_votes: Option<String>,
    pub rotten_tomatoes: Option<String>,
    pub metacritic: Option<String>,
    pub rated: Option<String>,
    pub awards: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct EnrichmentResult {
    pub title: Option<String>,
    pub overview: Option<String>,
    pub tagline: Option<String>,
    pub release_date: Option<String>,
    pub runtime: Option<u32>,
    pub vote_average: Option<f64>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<u64>,
    pub tmdb_id: Option<u64>,
    pub genres: Vec<GenreEntry>,
    pub credits: Option<CreditsData>,
    pub videos: Vec<VideoEntry>,
    pub images: Option<ImagesData>,
    pub artwork_candidates: Vec<ArtworkCandidate>,
    pub ratings: Option<RatingsData>,
    pub production_companies: Vec<ProductionCompany>,
    pub networks: Vec<NetworkEntry>,
    pub external_ids: Option<ExternalIds>,
}

impl EnrichmentResult {
    pub fn tmdb_id_from_details(&self) -> Option<u64> {
        self.tmdb_id
    }
}

#[async_trait]
pub trait MetadataProvider: Send + Sync {
    fn name(&self) -> &str;
    fn is_configured(&self) -> bool;

    async fn test_connection(&self) -> MetadataResult<()>;

    async fn search_movie(
        &self,
        query: &str,
        year: Option<u32>,
    ) -> MetadataResult<Vec<SearchResult>>;

    async fn search_tv(
        &self,
        query: &str,
        year: Option<u32>,
    ) -> MetadataResult<Vec<SearchResult>>;

    async fn get_movie_details(&self, id: u64) -> MetadataResult<MovieDetails>;
    async fn get_tv_details(&self, id: u64) -> MetadataResult<TvDetails>;
    async fn get_season_details(&self, tv_id: u64, season: u32) -> MetadataResult<SeasonDetails>;

    async fn find_by_imdb_id(&self, imdb_id: &str) -> MetadataResult<Option<SearchResult>>;
}

#[async_trait]
pub trait ArtworkProvider: Send + Sync {
    fn name(&self) -> &str;
    fn is_configured(&self) -> bool;

    async fn get_movie_artwork(&self, tmdb_id: u64) -> MetadataResult<Vec<ArtworkCandidate>>;
    async fn get_tv_artwork(&self, tvdb_id: u64) -> MetadataResult<Vec<ArtworkCandidate>>;
}

#[async_trait]
pub trait RatingsProvider: Send + Sync {
    fn name(&self) -> &str;
    fn is_configured(&self) -> bool;

    async fn get_ratings(&self, imdb_id: &str) -> MetadataResult<RatingsData>;
}

pub type DirectRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

pub struct ProviderRateLimiter {
    pub tmdb: DirectRateLimiter,
    pub tvdb: DirectRateLimiter,
    pub fanart: DirectRateLimiter,
    pub omdb: DirectRateLimiter,
}

impl ProviderRateLimiter {
    pub fn new() -> Self {
        Self {
            tmdb: RateLimiter::direct(Quota::per_second(nonzero!(40u32))),
            tvdb: RateLimiter::direct(
                Quota::per_second(nonzero!(1u32)).allow_burst(nonzero!(5u32)),
            ),
            fanart: RateLimiter::direct(
                Quota::per_second(nonzero!(1u32)).allow_burst(nonzero!(3u32)),
            ),
            omdb: RateLimiter::direct(
                Quota::per_second(nonzero!(1u32)).allow_burst(nonzero!(10u32)),
            ),
        }
    }
}

impl Default for ProviderRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TmdbConfig {
    pub image_base_url: String,
    pub secure_image_base_url: String,
    pub poster_sizes: Vec<String>,
    pub backdrop_sizes: Vec<String>,
    pub logo_sizes: Vec<String>,
    pub profile_sizes: Vec<String>,
    pub change_keys: Vec<String>,
}

impl Default for TmdbConfig {
    fn default() -> Self {
        Self {
            image_base_url: "https://image.tmdb.org/t/p/".to_string(),
            secure_image_base_url: "https://image.tmdb.org/t/p/".to_string(),
            poster_sizes: vec![
                "w92".into(),
                "w154".into(),
                "w185".into(),
                "w342".into(),
                "w500".into(),
                "w780".into(),
                "original".into(),
            ],
            backdrop_sizes: vec![
                "w300".into(),
                "w780".into(),
                "w1280".into(),
                "original".into(),
            ],
            logo_sizes: vec![
                "w45".into(),
                "w92".into(),
                "w154".into(),
                "w185".into(),
                "w300".into(),
                "w500".into(),
                "original".into(),
            ],
            profile_sizes: vec![
                "w45".into(),
                "w185".into(),
                "h632".into(),
                "original".into(),
            ],
            change_keys: vec![],
        }
    }
}

pub struct ProviderRegistry {
    primary: Option<Box<dyn MetadataProvider>>,
    supplementary_metadata: Vec<Box<dyn MetadataProvider>>,
    artwork: Vec<Box<dyn ArtworkProvider>>,
    ratings: Vec<Box<dyn RatingsProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            primary: None,
            supplementary_metadata: vec![],
            artwork: vec![],
            ratings: vec![],
        }
    }

    pub fn from_config(config: &MetadataConfig) -> Self {
        let mut registry = Self::new();

        if config.providers.tmdb.enabled
            && !config.providers.tmdb.access_token.is_empty()
        {
            let client = TmdbClient::new(
                &config.providers.tmdb,
                config.metadata_language.clone(),
            );
            registry.primary = Some(Box::new(client));
        }

        if config.providers.tvdb.enabled
            && let Some(api_key) = &config.providers.tvdb.api_key
        {
            let tvdb = TvdbClient::new(api_key.clone());
            registry.supplementary_metadata.push(Box::new(tvdb.clone()));
            registry.artwork.push(Box::new(tvdb));
        }

        if config.providers.fanart.enabled
            && let Some(api_key) = &config.providers.fanart.api_key
        {
            let fanart = FanartClient::new(api_key.clone());
            registry.artwork.push(Box::new(fanart));
        }

        if config.providers.omdb.enabled
            && let Some(api_key) = &config.providers.omdb.api_key
        {
            let omdb = OmdbClient::new(api_key.clone());
            registry.ratings.push(Box::new(omdb));
        }

        registry
    }

    pub fn primary(&self) -> Option<&dyn MetadataProvider> {
        self.primary.as_deref()
    }

    pub fn supplementary_metadata(&self) -> &[Box<dyn MetadataProvider>] {
        &self.supplementary_metadata
    }

    pub fn artwork_providers(&self) -> &[Box<dyn ArtworkProvider>] {
        &self.artwork
    }

    pub fn ratings_providers(&self) -> &[Box<dyn RatingsProvider>] {
        &self.ratings
    }

    pub fn has_primary(&self) -> bool {
        self.primary.is_some()
    }

    pub fn configured_provider_names(&self) -> Vec<&str> {
        let mut names = Vec::new();
        if let Some(p) = &self.primary
            && p.is_configured()
        {
            names.push(p.name());
        }
        for p in &self.supplementary_metadata {
            if p.is_configured() {
                names.push(p.name());
            }
        }
        for p in &self.artwork {
            if p.is_configured() {
                names.push(p.name());
            }
        }
        for p in &self.ratings {
            if p.is_configured() {
                names.push(p.name());
            }
        }
        names
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct EnrichmentOrchestrator {
    registry: Arc<ProviderRegistry>,
    rate_limiters: Arc<ProviderRateLimiter>,
    db: PgPool,
    http: Client,
    tmdb_config: Arc<arc_swap::ArcSwap<TmdbConfig>>,
    tmdb_client: Option<TmdbClient>,
    config: MetadataConfig,
    data_dir: PathBuf,
}

impl EnrichmentOrchestrator {
    pub fn new(
        registry: ProviderRegistry,
        db: PgPool,
        config: MetadataConfig,
        data_dir: PathBuf,
    ) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(
                config.enrichment_timeout_seconds as u64,
            ))
            .build()
            .unwrap_or_default();

        let tmdb_client = if config.providers.tmdb.enabled
            && !config.providers.tmdb.access_token.is_empty()
        {
            Some(TmdbClient::new(
                &config.providers.tmdb,
                config.metadata_language.clone(),
            ))
        } else {
            None
        };

        Self {
            registry: Arc::new(registry),
            rate_limiters: Arc::new(ProviderRateLimiter::new()),
            db,
            http,
            tmdb_config: Arc::new(arc_swap::ArcSwap::from_pointee(TmdbConfig::default())),
            tmdb_client,
            config,
            data_dir,
        }
    }

    pub fn registry(&self) -> &ProviderRegistry {
        &self.registry
    }

    pub fn tmdb_config(&self) -> Arc<TmdbConfig> {
        Arc::clone(&self.tmdb_config.load())
    }

    pub fn tmdb_client(&self) -> Option<&TmdbClient> {
        self.tmdb_client.as_ref()
    }

    pub async fn refresh_tmdb_config(&self) -> MetadataResult<()> {
        let client = self
            .tmdb_client
            .as_ref()
            .ok_or(MetadataError::TmdbNotConfigured)?;
        let config = client.fetch_configuration().await?;
        self.tmdb_config.store(Arc::new(config));
        tracing::info!("TMDB configuration refreshed successfully");
        Ok(())
    }

    pub fn http_client(&self) -> &Client {
        &self.http
    }

    pub fn rate_limiters(&self) -> &ProviderRateLimiter {
        &self.rate_limiters
    }

    pub fn db(&self) -> &PgPool {
        &self.db
    }

    pub fn config(&self) -> &MetadataConfig {
        &self.config
    }

    pub fn metadata_language(&self) -> &str {
        &self.config.metadata_language
    }

    pub async fn enrich_movie(
        &self,
        tmdb_id: Option<u64>,
        imdb_id: Option<&str>,
        title: &str,
        year: Option<u32>,
        media_item_id: Option<uuid::Uuid>,
    ) -> MetadataResult<EnrichmentResult> {
        let primary = self.registry.primary().ok_or(MetadataError::TmdbNotConfigured)?;

        let details = if let Some(id) = tmdb_id {
            self.rate_limiters.tmdb.until_ready().await;
            primary.get_movie_details(id).await?
        } else {
            self.rate_limiters.tmdb.until_ready().await;
            let results = primary.search_movie(title, year).await?;
            let best = results
                .into_iter()
                .next()
                .ok_or(MetadataError::NotFound {
                    provider: primary.name().to_string(),
                    id: title.to_string(),
                })?;

            self.rate_limiters.tmdb.until_ready().await;
            primary.get_movie_details(best.provider_id).await?
        };

        let mut result = EnrichmentResult {
            title: Some(details.title.clone()),
            overview: details.overview.clone(),
            tagline: details.tagline.clone(),
            release_date: details.release_date.clone(),
            runtime: details.runtime,
            vote_average: details.vote_average,
            poster_path: details.poster_path.clone(),
            backdrop_path: details.backdrop_path.clone(),
            imdb_id: details.imdb_id.clone(),
            tvdb_id: details.tvdb_id,
            tmdb_id: Some(details.provider_id),
            genres: details.genres.clone(),
            credits: details.credits.clone(),
            videos: details.videos.clone().unwrap_or_default(),
            images: details.images.clone(),
            production_companies: details.production_companies.clone(),
            external_ids: details.external_ids.clone(),
            ..Default::default()
        };

        for provider in self.registry.artwork_providers() {
            if let Some(id) = tmdb_id {
                match provider.get_movie_artwork(id).await {
                    Ok(artwork) => result.artwork_candidates.extend(artwork),
                    Err(e) => {
                        tracing::warn!(
                            provider = provider.name(),
                            error = %e,
                            "Artwork provider failed, skipping"
                        );
                    }
                }
            }
        }

        let effective_imdb = imdb_id
            .map(|s| s.to_string())
            .or(result.imdb_id.clone());

        if let Some(ref imdb) = effective_imdb {
            for provider in self.registry.ratings_providers() {
                match provider.get_ratings(imdb).await {
                    Ok(ratings) => result.ratings = Some(ratings),
                    Err(e) => {
                        tracing::warn!(
                            provider = provider.name(),
                            error = %e,
                            "Ratings provider failed, skipping"
                        );
                    }
                }
            }
        }

        if let Some(item_id) = media_item_id
            && let Some(ref images) = result.images
        {
            let effective_tmdb_id = tmdb_id.or(result.tmdb_id_from_details());
            if let Some(tid) = effective_tmdb_id {
                let tmdb_cfg = self.tmdb_config();
                artwork_downloader::download_and_store_artwork(
                    &artwork_downloader::ArtworkDownloadContext {
                        pool: &self.db,
                        http: &self.http,
                        tmdb_config: &tmdb_cfg,
                        data_dir: &self.data_dir,
                    },
                    item_id,
                    tid,
                    images,
                    self.config.artwork_auto_download,
                )
                .await;
            }
        }

        Ok(result)
    }

    pub async fn enrich_tv(
        &self,
        tmdb_id: Option<u64>,
        imdb_id: Option<&str>,
        title: &str,
        year: Option<u32>,
        media_item_id: Option<uuid::Uuid>,
    ) -> MetadataResult<EnrichmentResult> {
        let primary = self.registry.primary().ok_or(MetadataError::TmdbNotConfigured)?;

        let details = if let Some(id) = tmdb_id {
            self.rate_limiters.tmdb.until_ready().await;
            primary.get_tv_details(id).await?
        } else {
            self.rate_limiters.tmdb.until_ready().await;
            let results = primary.search_tv(title, year).await?;
            let best = results
                .into_iter()
                .next()
                .ok_or(MetadataError::NotFound {
                    provider: primary.name().to_string(),
                    id: title.to_string(),
                })?;

            self.rate_limiters.tmdb.until_ready().await;
            primary.get_tv_details(best.provider_id).await?
        };

        let mut result = EnrichmentResult {
            title: Some(details.name.clone()),
            overview: details.overview.clone(),
            tagline: details.tagline.clone(),
            release_date: details.first_air_date.clone(),
            vote_average: details.vote_average,
            poster_path: details.poster_path.clone(),
            backdrop_path: details.backdrop_path.clone(),
            imdb_id: details.imdb_id.clone(),
            tvdb_id: details.tvdb_id,
            tmdb_id: Some(details.provider_id),
            genres: details.genres.clone(),
            credits: details.credits.clone(),
            videos: details.videos.clone().unwrap_or_default(),
            images: details.images.clone(),
            networks: details.networks.clone(),
            external_ids: details.external_ids.clone(),
            ..Default::default()
        };

        for provider in self.registry.artwork_providers() {
            if let Some(tvdb_id) = details.tvdb_id {
                match provider.get_tv_artwork(tvdb_id).await {
                    Ok(artwork) => result.artwork_candidates.extend(artwork),
                    Err(e) => {
                        tracing::warn!(
                            provider = provider.name(),
                            error = %e,
                            "Artwork provider failed, skipping"
                        );
                    }
                }
            }
        }

        let effective_imdb = imdb_id
            .map(|s| s.to_string())
            .or(result.imdb_id.clone());

        if let Some(ref imdb) = effective_imdb {
            for provider in self.registry.ratings_providers() {
                match provider.get_ratings(imdb).await {
                    Ok(ratings) => result.ratings = Some(ratings),
                    Err(e) => {
                        tracing::warn!(
                            provider = provider.name(),
                            error = %e,
                            "Ratings provider failed, skipping"
                        );
                    }
                }
            }
        }

        if let Some(item_id) = media_item_id
            && let Some(ref images) = result.images
        {
            let effective_tmdb_id = tmdb_id.or(result.tmdb_id_from_details());
            if let Some(tid) = effective_tmdb_id {
                let tmdb_cfg = self.tmdb_config();
                artwork_downloader::download_and_store_artwork(
                    &artwork_downloader::ArtworkDownloadContext {
                        pool: &self.db,
                        http: &self.http,
                        tmdb_config: &tmdb_cfg,
                        data_dir: &self.data_dir,
                    },
                    item_id,
                    tid,
                    images,
                    self.config.artwork_auto_download,
                )
                .await;
            }
        }

        Ok(result)
    }

    pub async fn search(
        &self,
        query: &str,
        media_type: &str,
        year: Option<u32>,
    ) -> MetadataResult<Vec<SearchResult>> {
        let primary = self.registry.primary().ok_or(MetadataError::TmdbNotConfigured)?;

        self.rate_limiters.tmdb.until_ready().await;

        match media_type {
            "movie" => primary.search_movie(query, year).await,
            "tv" | "series" => primary.search_tv(query, year).await,
            _ => Err(MetadataError::InvalidResponse {
                provider: "registry".to_string(),
                message: format!("Unknown media type: {media_type}"),
            }),
        }
    }

    pub async fn find_by_imdb(
        &self,
        imdb_id: &str,
    ) -> MetadataResult<Option<SearchResult>> {
        let primary = self.registry.primary().ok_or(MetadataError::TmdbNotConfigured)?;

        self.rate_limiters.tmdb.until_ready().await;
        primary.find_by_imdb_id(imdb_id).await
    }
}


