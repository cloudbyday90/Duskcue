# Metadata Providers

## Overview

This document is the authoritative design for metadata provider integration. It covers: provider selection, tier architecture, API key management, data flow, caching strategy, rate limit handling, attribution requirements, error handling, and configuration.

Duskcue integrates with external metadata providers to enrich the media library with titles, descriptions, ratings, cast/crew, artwork, subtitles, and other metadata. The system is designed so that a single primary provider (TMDB) delivers a complete experience, while supplementary providers add unique data that TMDB does not offer.

## Provider Tier Architecture

### Tier 1 — Primary (Built-In, Always Active)

The primary provider is active by default and requires no user configuration beyond the initial setup wizard. It provides all core metadata needed for a fully functional media library.

| Provider | Role | Data |
|---|---|---|
| **TMDB v3** | Primary metadata for movies and TV | Titles, overviews, ratings, cast/crew, genres, certifications, runtime, studios, networks, episode data, trailers, artwork (posters/backdrops/logos/profiles), external IDs (IMDb, TVDB), translations in 150+ languages |

**Why TMDB as primary:**

- Deepest movie and TV coverage available — industry standard used by Plex, Jellyfin, Kodi, and Trakt
- Free for non-commercial use with attribution; generous rate limits (~40 requests/second)
- `append_to_response` reduces API calls dramatically (movie + credits + videos + images in 1 request)
- Daily ID exports enable offline bulk matching without API calls
- `/find?external_source=imdb_id` cross-references directly from IMDb IDs
- CC BY 4.0 image license — clean legal use
- OpenAPI 3.0 spec at `/openapi` — Rust types can be generated
- Multi-language support with `language=en-US` parameter
- Both Jellyfin and Plex have converged on TMDB as their primary source (2024-2026 trend)

### Tier 2 — Supplementary (Built-In, Opt-In with API Key)

Supplementary providers add data that TMDB does not offer. Each requires a separate user account and API key entered by the admin. None are required for a functional experience.

| Provider | Role | Unique Data | Auth |
|---|---|---|---|
| **TVDB v4** | TV episode refinement | Alternate episode ordering (DVD, absolute), supplementary TV episode metadata | API key → JWT via `/login` |
| **Fanart.tv v3.2** | Supplementary artwork | Clear logos (transparent), 4K backgrounds, character art, banners, thumbs, CD art | `api_key` query param |
| **OMDb** | Supplementary ratings | IMDb rating, Rotten Tomatoes score, Metacritic score | `apikey` query param |

**Why these as supplementary:**

- **TVDB:** Only source for alternate episode ordering (DVD order, absolute numbering). Some series have different season structures across regions. Free for projects under $50K/year revenue.
- **Fanart.tv:** Only source for transparent clear logos, 4K backgrounds, and character artwork. These image types are not available on TMDB. Free personal API keys.
- **OMDb:** Only source for Rotten Tomatoes and Metacritic ratings alongside IMDb scores. Free tier: 1,000 requests/day (sufficient for supplementary use). English only.

### Tier 3 — Subtitles (Built-In, Opt-In with API Key)

Subtitle providers are managed separately from metadata providers. See [SUBTITLES.md](SUBTITLES.md) for the full subtitle domain design.

| Provider | Priority | Auth | Free Tier |
|---|---|---|---|
| **SubDL** | Primary subtitle source | `api_key` query param or `x-api-key` header | 2,000 requests/day, 300 downloads/day |
| **OpenSubtitles** | Secondary subtitle source | API key + user token | Limited; VIP subscription for meaningful use |

**Why SubDL as primary:**

- Direct TMDB ID and IMDb ID search — natural fit with our identification pipeline
- Generous free tier (2,000 requests/day vs OpenSubtitles' restricted free access)
- Supports SRT, ASS, VTT formats
- Unpack endpoint extracts individual files from season packs
- Active API development

**Why OpenSubtitles as secondary:**

- Largest subtitle database available
- Hash-based matching for exact file identification
- Paywalled since 2023 — requires VIP subscription for downloads
- Community frustration over this change (documented across Reddit and Kodi forums)

## Provider Profiles

### TMDB v3 API

| Attribute | Value |
|---|---|
| **Base URL** | `https://api.themoviedb.org/3/` |
| **Image CDN** | `https://image.tmdb.org/t/p/{size}/{path}` |
| **Auth** | `Authorization: Bearer {access_token}` header |
| **Rate limit** | ~40 requests/second per IP (soft limit; respect 429 responses) |
| **Cost** | Free for non-commercial use with attribution |
| **Attribution** | "This product uses the TMDB API but is not endorsed or certified by TMDB" + approved logo |
| **License** | CC BY 4.0 for images; API terms of use for data |
| **Spec** | OpenAPI 3.0 at `https://developer.themoviedb.org/openapi` |
| **Status** | `https://status.themoviedb.org` |

**Key endpoints used:**

| Endpoint | Purpose | Used During |
|---|---|---|
| `/search/movie`, `/search/tv` | Text search by title | Phase 4 identification |
| `/find/{id}?external_source=imdb_id` | Cross-reference from IMDb ID | Phase 3 identification |
| `/movie/{id}`, `/tv/{id}` | Full details | Phase 5 enrichment |
| `/movie/{id}/credits`, `/tv/{id}/credits` | Cast and crew | Phase 5 enrichment |
| `/movie/{id}/images`, `/tv/{id}/images` | Artwork | Phase 5 enrichment, poster management |
| `/movie/{id}/videos`, `/tv/{id}/videos` | Trailers and clips | Phase 5 enrichment |
| `/movie/{id}/external_ids`, `/tv/{id}/external_ids` | IMDb/TVDB IDs | Phase 5 enrichment |
| `/movie/popular`, `/tv/popular` | Popular lists | Collection builders |
| `/movie/top_rated`, `/tv/top_rated` | Top rated lists | Collection builders |
| `/trending/movie/day`, `/trending/tv/day` | Trending lists | Collection builders |
| `/movie/now_playing`, `/movie/upcoming` | Current releases | Collection builders |
| `/genre/movie/list`, `/genre/tv/list` | Genre definitions | Phase 5 enrichment |
| `/configuration` | Image sizes, change keys | Startup cache |
| `/movie/{id}/changes`, `/tv/{id}/changes` | Detect metadata updates | Metadata refresh task |

**`append_to_response` batching:**

A single request to `/movie/{id}?append_to_response=credits,videos,external_ids,images` returns all related data in one API call. This is the primary method for Phase 5 enrichment, reducing API calls by 4-5x per item.

```rust
let url = format!(
    "https://api.themoviedb.org/3/movie/{}?language=en-US&append_to_response=credits,videos,external_ids,images&include_image_language=en,null",
    tmdb_id
);
```

**Daily ID exports:**

TMDB publishes daily JSONL gzip files at `https://files.tmdb.org/p/exports/` containing all valid IDs. These enable offline bulk matching without API calls:

| File | Content |
|---|---|
| `movie_ids_MM_DD_YYYY.json.gz` | All valid movie IDs with title, original_title, popularity, adult flag |
| `tv_series_ids_MM_DD_YYYY.json.gz` | All valid TV series IDs |
| `person_ids_MM_DD_YYYY.json.gz` | All valid person IDs |
| `collection_ids_MM_DD_YYYY.json.gz` | All valid collection IDs |

Files are available daily at ~08:00 UTC, retained for 3 months. No authentication required.

**Image sizes:**

| Type | Available Sizes |
|---|---|
| Poster | `w92`, `w154`, `w185`, `w342`, `w500`, `w780`, `original` |
| Backdrop | `w300`, `w780`, `w1280`, `original` |
| Logo | `w45`, `w92`, `w154`, `w185`, `w300`, `w500`, `original` (SVG and PNG) |
| Profile | `w45`, `w185`, `h632`, `original` |

Duskcue downloads `original` size for best quality (per POSTER_MANAGEMENT.md `artwork_download_originals_only = true`). Server-side resizing generates thumbnails.

### TVDB v4 API

| Attribute | Value |
|---|---|
| **Base URL** | `https://api4.thetvdb.com/v4/` |
| **Auth** | POST `/login` with `{"apikey": "..."}` → JWT token (2-hour TTL) |
| **Rate limit** | Undocumented; rate-limited per API key |
| **Cost** | Free if revenue < $50K/year (attribution required) |
| **Attribution** | Required with direct link to thetvdb.com |
| **Spec** | OpenAPI/Swagger at `https://thetvdb.github.io/v4-api/` |

**Key endpoints used:**

| Endpoint | Purpose |
|---|---|
| `/login` | Authenticate and receive JWT |
| `/series/{id}` | Series details |
| `/series/{id}/episodes` | Episode listing with alternate orders |
| `/series/{id}/artworks` | Series artwork |
| `/series/{id}/extended` | Extended series info with episodes |
| `/movies/{id}` | Movie details (if needed) |

**JWT token lifecycle:**

```rust
struct TvdbClient {
    api_key: String,
    token: RwLock<Option<String>>,
    token_expires: RwLock<Option<Instant>>,
}

impl TvdbClient {
    async fn ensure_token(&self) -> Result<&str> {
        let expires = self.token_expires.read().await;
        if let Some(exp) = *expires {
            if exp.duration_since(Instant::now()) > Duration::from_secs(300) {
                return Ok(self.token.read().await.as_deref().unwrap());
            }
        }
        drop(expires);
        self.refresh_token().await
    }
}
```

Token is refreshed 5 minutes before expiry. On authentication failure, the token is cleared and re-acquired on the next request.

### Fanart.tv v3.2 API

| Attribute | Value |
|---|---|
| **Base URL** | `https://webservice.fanart.tv/v3/` |
| **Auth** | `api_key` query param (personal key from fanart.tv/get-an-api-key/) |
| **Rate limit** | Undocumented; sponsor tiers for higher limits |
| **Cost** | Free personal key; sponsor tiers available |

**Key endpoints used:**

| Endpoint | Purpose |
|---|---|
| `/movies/{id}?api_key={key}` | Movie artwork (identified by TMDB ID) |
| `/tv/{id}?api_key={key}` | TV artwork (identified by TVDB ID) |

**Artwork types available:**

| Type | Description | Size |
|---|---|---|
| `movieposter` / `tvposter` | Standard posters | 1000×1426 |
| `moviebackground` / `showbackground` | Background/backdrop | 1920×1080, 3840×2160 |
| `hdmovielogo` / `hdtvlogo` | Clear logos (transparent) | 800×310 |
| `moviedisc` / `tvbanner` | Disc art / banners | 1000×563 |
| `moviethumb` / `tvthumb` | Thumbnail/landscape | 1000×562 |
| `movieart` | Clear art | 1000×562 |
| `characterart` | Character portraits | 512×512 |

### OMDb API

| Attribute | Value |
|---|---|
| **Base URL** | `https://www.omdbapi.com/` |
| **Auth** | `apikey` query param |
| **Rate limit** | 1,000 requests/day (free); Patreon for higher |
| **Cost** | Free tier; Patreon supporters get higher limits |
| **Coverage** | English only |

**Key endpoints used:**

| Endpoint | Purpose |
|---|---|
| `/?i={imdb_id}&apikey={key}` | Lookup by IMDb ID |
| `/?t={title}&y={year}&apikey={key}` | Search by title and year |

**Data extracted:**

```json
{
    "imdbRating": "8.5",
    "Ratings": [
        { "Source": "Internet Movie Database", "Value": "8.5/10" },
        { "Source": "Rotten Tomatoes", "Value": "94%" },
        { "Source": "Metacritic", "Value": "82/100" }
    ],
    "Metascore": "82",
    "imdbVotes": "1,234,567",
    "Rated": "R",
    "Awards": "Won 3 Oscars"
}
```

OMDb is queried only for items that have an `imdb_id` in `media_items`. Results are stored in `media_items.metadata` JSONB under a `ratings` key.

### SubDL API

| Attribute | Value |
|---|---|
| **Base URL** | `https://api.subdl.com/api/v1/` |
| **Download URL** | `https://dl.subdl.com/subtitle/{path}` |
| **Auth** | `api_key` query param or `x-api-key` header |
| **Rate limit** | 2,000 requests/day (free); 30,000/day (paid) |
| **Cost** | Free tier; paid plans available |

**Key endpoints used:**

| Endpoint | Purpose |
|---|---|
| `/subtitles?tmdb_id={id}&languages=en` | Search by TMDB ID |
| `/subtitles?imdb_id={id}&languages=en` | Search by IMDb ID |
| `/subtitles?film_name={name}&type=movie` | Search by title |
| `/auto?query={query}&type=movie` | Autocomplete (paid only) |

**Search response structure:**

```json
{
    "status": true,
    "subtitles": [
        {
            "release_name": "Movie.2024.1080p.BluRay",
            "name": "Movie.2024.1080p.BluRay.srt",
            "url": "/subtitle/3197651-3213944.zip",
            "language": "EN",
            "hi": false,
            "format": "srt"
        }
    ]
}
```

Download links are prefixed with `https://dl.subdl.com`. Free users are IP-limited to 300 downloads/day.

### OpenSubtitles REST API

| Attribute | Value |
|---|---|
| **Base URL** | `https://api.opensubtitles.com/api/v1/` |
| **Auth** | API key + user token (login required) |
| **Rate limit** | Limited on free tier; VIP for higher |
| **Cost** | Free tier restricted; VIP subscription for meaningful downloads |

OpenSubtitles is secondary due to its paywall but offers the largest subtitle library and hash-based matching. Full integration design is in [SUBTITLES.md](SUBTITLES.md).

## Data Flow

### Phase 5 Enrichment Pipeline

Metadata enrichment occurs during Phase 5 of the media scanning pipeline (see [MEDIA_SCANNING.md](MEDIA_SCANNING.md)). The flow for each newly identified item:

```
1. Lookup TMDB
   ├─ If tmdb_id known (from identification layers 1-3):
   │   GET /movie/{id}?append_to_response=credits,videos,external_ids,images
   │   → Single request fetches all core data
   │
   └─ If only title/year known:
       GET /search/movie?query={title}&year={year}
       → Match by confidence score
       → Then fetch details with append_to_response

2. Store core metadata
   ├─ media_items: title, overview, rating, release_date, runtime, certification, tmdb_id, imdb_id, tvdb_id
   ├─ credits: cast and crew from TMDB credits
   ├─ genres: from TMDB genre IDs
   └─ media_items.metadata JSONB: trailers, external IDs, original_language, tagline, production_companies

3. Download artwork (if artwork_auto_download enabled)
   ├─ Primary: TMDB images (poster, backdrop, logo)
   ├─ Supplementary: Fanart.tv (if key configured) → clear logos, 4K backgrounds
   └─ Store in /data/metadata/artwork/ and /cache/images/

4. Fetch supplementary data (if keys configured)
   ├─ TVDB: alternate episode ordering for TV series
   ├─ OMDb: Rotten Tomatoes, Metacritic ratings
   └─ Store in media_items.metadata JSONB

5. Match subtitles (if auto_fetch enabled)
   ├─ SubDL: search by TMDB ID → download SRT
   └─ Store as subtitle_files rows
```

### Metadata Refresh

Existing items are periodically refreshed to pick up metadata changes:

```
metadata_refresh scheduled task (configurable, default every 6 hours):
  1. Query TMDB /changes endpoint for items modified since last refresh
  2. For each changed item, re-fetch with append_to_response
  3. Merge updated fields into existing data (never overwrite user-locked artwork)
  4. Log changes at DEBUG level
```

TMDB's `/changes` endpoint tracks modifications to movies, TV shows, and people. The server stores `last_metadata_refresh_at` per item and only re-fetches items with changes after that timestamp.

## Rate Limit Management

### Per-Provider Rate Limiters

Each provider has an independent token-bucket rate limiter:

| Provider | Bucket Size | Refill Rate | Backoff |
|---|---|---|---|
| TMDB | 40 tokens | 40/second | 1s → 2s → 4s exponential on 429 |
| TVDB | 5 tokens | 1/second | 2s → 4s → 8s exponential on 429 |
| Fanart.tv | 3 tokens | 1/second | 2s → 4s → 8s exponential on 429 |
| OMDb | 10 tokens | ~0.7/second (1,000/day budget) | Daily budget tracking |
| SubDL | 10 tokens | ~0.14/second (2,000/day budget) | Daily budget tracking |
| OpenSubtitles | 5 tokens | 0.5/second | 2s → 4s → 8s on 429 |

**Implementation:**

```rust
use governor::{RateLimiter, Quota, Jitter};
use nonzero_ext::nonzero;

struct ProviderRateLimiter {
    tmdb: RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>,
    tvdb: RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>,
    fanart: RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>,
    omdb: RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>,
    subdl: RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>,
}

impl ProviderRateLimiter {
    fn new() -> Self {
        Self {
            tmdb: RateLimiter::direct(Quota::per_second(nonzero!(40u32))),
            tvdb: RateLimiter::direct(Quota::per_second(nonzero!(1u32)).allow_burst(nonzero!(5u32))),
            fanart: RateLimiter::direct(Quota::per_second(nonzero!(1u32)).allow_burst(nonzero!(3u32))),
            omdb: RateLimiter::direct(Quota::per_second(nonzero!(1u32)).allow_burst(nonzero!(10u32))),
            subdl: RateLimiter::direct(Quota::per_second(nonzero!(1u32)).allow_burst(nonzero!(10u32))),
        }
    }
}
```

All rate limiters use `governor` v0.6 — the same crate used for API rate limiting (see [API_CONVENTIONS.md](API_CONVENTIONS.md)).

### 429 Response Handling

When a provider returns HTTP 429:

1. Parse `Retry-After` header (if present)
2. Wait `max(Retry-After, backoff_time)` before retrying
3. Exponential backoff: 1s → 2s → 4s → 8s → 16s (max)
4. After 5 consecutive 429s, mark provider as temporarily unavailable
5. Admin notified via notification system if provider is down for > 5 minutes
6. Provider auto-recovers on next successful request

### Daily Budget Tracking (OMDb, SubDL)

For providers with daily request budgets:

```rust
struct DailyBudget {
    date: NaiveDate,
    used: AtomicU32,
    limit: u32,
}

impl DailyBudget {
    fn check(&self) -> bool {
        let today = Local::now().date_naive();
        if today != self.date {
            self.used.store(0, Ordering::Relaxed);
            self.date = today;
        }
        self.used.load(Ordering::Relaxed) < self.limit
    }

    fn increment(&self) {
        self.used.fetch_add(1, Ordering::Relaxed);
    }
}
```

When daily budget is exhausted, the provider is skipped until midnight UTC. Admin notified via dashboard warning.

## API Key Management

### Storage

All provider API keys are stored in `server_config.metadata` JSONB under a `providers` key:

```json
{
    "providers": {
        "tmdb": {
            "api_key": "encrypted:...",
            "access_token": "encrypted:...",
            "enabled": true
        },
        "tvdb": {
            "api_key": "encrypted:...",
            "enabled": false
        },
        "fanart": {
            "api_key": "encrypted:...",
            "enabled": false
        },
        "omdb": {
            "api_key": "encrypted:...",
            "enabled": false
        }
    }
}
```

Subtitle provider keys are in `server_config.integrations.subtitle_providers` (see [SUBTITLES.md](SUBTITLES.md)).

### Encryption

Provider API keys are encrypted at rest using AES-256-GCM:

- Encryption key derived from the server's bootstrap encryption key (same key used for backup encryption in [BACKUP_RECOVERY.md](../operations/BACKUP_RECOVERY.md))
- Keys stored with `encrypted:` prefix in JSONB to distinguish from plaintext during migration
- Admin API returns masked values: `"api_key": "abc...xyz"` (first 3 + last 3 characters)
- Keys are decrypted in memory only when making outbound requests; never logged, never cached in plaintext

### Access Controls

- Provider API keys are admin-only accessible via the settings UI
- The `GET /api/v1/admin/settings/metadata` endpoint returns masked keys
- The `PUT /api/v1/admin/settings/metadata` endpoint accepts full keys and encrypts before storage
- Non-admin users cannot access provider keys through any endpoint
- Keys are never included in error responses, logs, or API responses to non-admin users

### Key Validation

When an admin saves a provider API key, the server validates it immediately:

1. Make a lightweight test request to the provider (e.g., TMDB `/configuration` endpoint)
2. If the request succeeds, save the key
3. If the request fails with 401, reject the save and show error to admin
4. Store the validation timestamp for dashboard display

### Outbound Request Security

All outbound provider requests go through the SSRF prevention pipeline defined in [API_SECURITY.md](../security/API_SECURITY.md):

| Provider | Allowed Hosts |
|---|---|
| TMDB | `api.themoviedb.org`, `image.tmdb.org`, `files.tmdb.org` |
| TVDB | `api4.thetvdb.com` |
| Fanart.tv | `webservice.fanart.tv` |
| OMDb | `www.omdbapi.com` |
| SubDL | `api.subdl.com`, `dl.subdl.com` |
| OpenSubtitles | `api.opensubtitles.com` |

All responses are validated against expected schemas before processing (per API_SECURITY.md outbound validation rules).

## Caching Strategy

### TMDB Configuration Cache

TMDB's `/configuration` endpoint returns image base URLs, available sizes, and change keys. This data changes rarely and is cached at startup:

```rust
struct TmdbConfig {
    image_base_url: String,
    secure_image_base_url: String,
    poster_sizes: Vec<String>,
    backdrop_sizes: Vec<String>,
    logo_sizes: Vec<String>,
    profile_sizes: Vec<String>,
    change_keys: Vec<String>,
}
```

Cached in `AppState` as `Arc<TmdbConfig>`. Refreshed every 24 hours via scheduled task.

### Metadata Response Cache

Fetched metadata is stored in the database (the source of truth). In-memory caching is limited to:

- TMDB configuration (refreshed daily)
- TVDB JWT token (refreshed every 2 hours)
- Daily ID export files (downloaded daily, stored in `/cache/metadata/`)
- Provider rate limiter state (in-memory, lost on restart — acceptable)

### Daily ID Export Cache

TMDB daily ID exports are downloaded once per day and stored locally:

```
/cache/metadata/exports/
├── movie_ids_06_06_2026.json.gz
├── tv_series_ids_06_06_2026.json.gz
├── person_ids_06_06_2026.json.gz
└── collection_ids_06_06_2026.json.gz
```

Used for bulk matching during full library scans. Downloaded by the `metadata_refresh` scheduled task. Old files cleaned up after 7 days.

## Provider Client Architecture

### Trait-Based Abstraction

Each provider implements a common trait for type-safe access:

```rust
#[async_trait]
trait MetadataProvider: Send + Sync {
    fn name(&self) -> &str;
    fn is_configured(&self) -> bool;
    async fn test_connection(&self) -> Result<()>;

    async fn search_movie(&self, query: &str, year: Option<u32>) -> Result<Vec<SearchResult>>;
    async fn search_tv(&self, query: &str, year: Option<u32>) -> Result<Vec<SearchResult>>;

    async fn get_movie_details(&self, id: ProviderId) -> Result<MovieDetails>;
    async fn get_tv_details(&self, id: ProviderId) -> Result<TvDetails>;
    async fn get_season_details(&self, id: ProviderId, season: u32) -> Result<SeasonDetails>;

    async fn get_movie_artwork(&self, id: ProviderId) -> Result<Vec<ArtworkCandidate>>;
    async fn get_tv_artwork(&self, id: ProviderId) -> Result<Vec<ArtworkCandidate>>;
}
```

```rust
#[async_trait]
trait ArtworkProvider: Send + Sync {
    fn name(&self) -> &str;
    fn is_configured(&self) -> bool;

    async fn get_movie_artwork(&self, tmdb_id: u64) -> Result<Vec<ArtworkCandidate>>;
    async fn get_tv_artwork(&self, tvdb_id: u64) -> Result<Vec<ArtworkCandidate>>;
}
```

```rust
#[async_trait]
trait RatingsProvider: Send + Sync {
    fn name(&self) -> &str;
    fn is_configured(&self) -> bool;

    async fn get_ratings(&self, imdb_id: &str) -> Result<RatingsData>;
}
```

### Provider Registry

```rust
struct ProviderRegistry {
    primary: Box<dyn MetadataProvider>,
    supplementary_metadata: Vec<Box<dyn MetadataProvider>>,
    artwork: Vec<Box<dyn ArtworkProvider>>,
    ratings: Vec<Box<dyn RatingsProvider>>,
}
```

The registry is constructed at startup from `server_config.metadata.providers`:

```rust
impl ProviderRegistry {
    fn from_config(config: &MetadataConfig) -> Self {
        let primary = TmdbClient::new(config.tmdb_api_key(), config.tmdb_access_token());

        let mut supplementary_metadata = Vec::new();
        let mut artwork = Vec::new();
        let mut ratings = Vec::new();

        if config.tvdb_enabled() {
            let tvdb = TvdbClient::new(config.tvdb_api_key());
            supplementary_metadata.push(Box::new(tvdb) as Box<dyn MetadataProvider>);
        }

        if config.fanart_enabled() {
            let fanart = FanartClient::new(config.fanart_api_key());
            artwork.push(Box::new(fanart) as Box<dyn ArtworkProvider>);
        }

        if config.omdb_enabled() {
            let omdb = OmdbClient::new(config.omdb_api_key());
            ratings.push(Box::new(omdb) as Box<dyn RatingsProvider>);
        }

        Self { primary, supplementary_metadata, artwork, ratings }
    }
}
```

### Enrichment Orchestrator

The orchestrator coordinates all providers during Phase 5 enrichment:

```rust
struct EnrichmentOrchestrator {
    registry: ProviderRegistry,
    rate_limiters: ProviderRateLimiter,
    db: PgPool,
    outbound_client: reqwest::Client,
}

impl EnrichmentOrchestrator {
    async fn enrich_movie(&self, item: &MediaItemRow) -> Result<EnrichmentResult> {
        let mut result = EnrichmentResult::default();

        let details = self.registry.primary.get_movie_details(item.tmdb_id.into()).await?;
        result.apply_tmdb_details(details);

        for provider in &self.registry.artwork {
            let artwork = provider.get_movie_artwork(item.tmdb_id.into()).await?;
            result.apply_artwork(artwork);
        }

        if let Some(imdb_id) = &item.imdb_id {
            for provider in &self.registry.ratings {
                let ratings = provider.get_ratings(imdb_id).await?;
                result.apply_ratings(ratings);
            }
        }

        self.persist_enrichment(item.id, &result).await?;
        Ok(result)
    }
}
```

Providers are called sequentially within each tier (primary → artwork → ratings) to avoid overwhelming rate limits. Failed supplementary providers are skipped silently — the enrichment succeeds with available data.

## Error Handling

### Provider Error Categories

| Category | Example | Response |
|---|---|---|
| **Authentication failure** | Invalid API key (401) | Log warning; mark provider as misconfigured; admin notification |
| **Rate limited** | Too many requests (429) | Exponential backoff; retry up to 3 times |
| **Not found** | Item not in provider database (404) | Not an error; skip this provider for this item |
| **Network failure** | Connection timeout, DNS failure | Retry with backoff; mark provider as temporarily unavailable |
| **Invalid response** | Schema validation failure | Log error; skip this provider for this item; admin notification |
| **Daily budget exhausted** | OMDb/SubDL daily limit reached | Skip provider until midnight UTC; dashboard warning |

### Error Codes

Provider errors map to existing error domains. No new error domain is needed:

| Scenario | Error Code | Domain |
|---|---|---|
| TMDB unavailable during enrichment | `LIB_011` | Library |
| TVDB authentication failure | `LIB_012` | Library |
| Provider rate limit hit | `LIB_013` | Library |
| Provider response validation failure | `LIB_014` | Library |
| TMDB key not configured | `SYS_001` | System |

These are added to the existing LIB domain (currently LIB_001 through LIB_010 defined in [ERROR_HANDLING.md](ERROR_HANDLING.md)).

### Graceful Degradation

The enrichment pipeline is designed to succeed even when providers fail:

1. **Primary (TMDB) failure:** If TMDB is completely unavailable, enrichment is retried on the next scheduled refresh. Items remain in `matched` state with whatever data was collected during identification. Admin is notified.
2. **Supplementary failure:** Silently skipped. Items get TMDB data only. No user-visible impact.
3. **Artwork failure:** Items get TMDB artwork only. Fanart.tv's unique types (clear logos, 4K backgrounds) are absent until the provider recovers.
4. **Ratings failure:** Items get TMDB's own rating. OMDb's Rotten Tomatoes and Metacritic scores are absent.

## Attribution

Duskcue must display attribution for each active provider. This is a legal requirement of the provider APIs.

### Where Attribution Appears

Attribution is displayed in the admin UI under Settings → About → Legal, and in the web client's footer on media detail pages.

| Provider | Required Attribution |
|---|---|
| TMDB | "This product uses the TMDB API but is not endorsed or certified by TMDB" + approved TMDB logo |
| TVDB | "Metadata provided by TheTVDB" + link to thetvdb.com/subscribe |
| Fanart.tv | "Artwork provided by Fanart.tv" |
| OMDb | "OMDb API used under license" |

### TMDB Logo

The approved TMDB logo must be displayed in the application's About or Credits section. Approved logos are available at `https://www.themoviedb.org/about/logos-attribution`. The logo must be:
- Less prominent than the Duskcue logo
- Not modified in color, aspect ratio, or orientation
- One of the approved color variants

## Configuration

### MetadataConfig Extension

The existing `MetadataConfig` Rust struct (defined in [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md)) is extended with provider fields:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetadataConfig {
    // Existing fields (artwork, overlays, collections) — see POSTER_MANAGEMENT.md
    pub artwork_language_priority: Vec<String>,
    pub artwork_auto_download: bool,
    pub artwork_download_originals_only: bool,
    pub asset_directory: Option<String>,
    pub overlays_enabled: bool,
    pub overlay_apply_schedule: String,
    pub overlay_image_format: String,
    pub overlay_image_quality: i32,
    pub overlay_max_image_size_mb: i32,
    pub overlay_default_font: String,
    pub overlay_reapply_on_artwork_change: bool,
    pub collections_enabled: bool,
    pub collection_sync_schedule: String,
    pub collection_default_poster_source: String,
    pub collection_max_items_default: i32,
    pub collection_track_missing: bool,
    pub collection_external_rate_limit_per_minute: i32,

    // Provider configuration
    pub providers: ProviderConfig,
    pub auto_refresh_hours: u32,
    pub max_concurrent_probes: u32,
    pub metadata_language: String,
    pub enrichment_timeout_seconds: u32,
    pub export_cache_days: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderConfig {
    pub tmdb: TmdbProviderConfig,
    pub tvdb: OptionalProviderConfig,
    pub fanart: OptionalProviderConfig,
    pub omdb: OptionalProviderConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TmdbProviderConfig {
    pub api_key: String,
    pub access_token: String,
    pub enabled: bool,
    pub include_adult: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OptionalProviderConfig {
    pub api_key: Option<String>,
    pub enabled: bool,
}

impl Default for MetadataConfig {
    fn default() -> Self {
        Self {
            // Existing defaults — see POSTER_MANAGEMENT.md
            artwork_language_priority: vec!["en".to_string()],
            artwork_auto_download: true,
            artwork_download_originals_only: true,
            asset_directory: None,
            overlays_enabled: true,
            overlay_apply_schedule: "0 5 * * *".to_string(),
            overlay_image_format: "webp".to_string(),
            overlay_image_quality: 90,
            overlay_max_image_size_mb: 10,
            overlay_default_font: "Inter".to_string(),
            overlay_reapply_on_artwork_change: true,
            collections_enabled: true,
            collection_sync_schedule: "0 6 * * *".to_string(),
            collection_default_poster_source: "auto".to_string(),
            collection_max_items_default: 100,
            collection_track_missing: true,
            collection_external_rate_limit_per_minute: 30,

            // Provider defaults
            providers: ProviderConfig {
                tmdb: TmdbProviderConfig {
                    api_key: String::new(),
                    access_token: String::new(),
                    enabled: true,
                    include_adult: false,
                },
                tvdb: OptionalProviderConfig {
                    api_key: None,
                    enabled: false,
                },
                fanart: OptionalProviderConfig {
                    api_key: None,
                    enabled: false,
                },
                omdb: OptionalProviderConfig {
                    api_key: None,
                    enabled: false,
                },
            },
            auto_refresh_hours: 6,
            max_concurrent_probes: 2,
            metadata_language: "en".to_string(),
            enrichment_timeout_seconds: 30,
            export_cache_days: 7,
        }
    }
}
```

**Field semantics:**
- `providers.tmdb.api_key` — TMDB API key (required during first-run setup wizard)
- `providers.tmdb.access_token` — TMDB Bearer token (preferred auth method; required during setup wizard)
- `providers.tmdb.include_adult` — include adult content in search results (default: false)
- `providers.tvdb.enabled` — enable TVDB as supplementary TV metadata source
- `providers.fanart.enabled` — enable Fanart.tv as supplementary artwork source
- `providers.omdb.enabled` — enable OMDb as supplementary ratings source
- `auto_refresh_hours` — how often to check for metadata updates (default: 6)
- `metadata_language` — ISO 639-1 language code for metadata fetching (default: `en`)
- `enrichment_timeout_seconds` — per-item timeout for enrichment (default: 30)
- `export_cache_days` — how many days of TMDB export files to keep locally (default: 7)

### First-Run Setup Wizard Integration

The first-run setup wizard (see [CONFIGURATION.md](../operations/CONFIGURATION.md)) prompts the admin for:

1. **TMDB API key** (required) — link to `https://www.themoviedb.org/settings/api` with instructions
2. **TMDB access token** (required) — same page, labeled "API Read Access Token"
3. **TVDB API key** (optional) — link to `https://thetvdb.com/api-information/signup`
4. **Fanart.tv API key** (optional) — link to `https://fanart.tv/get-an-api-key/`
5. **OMDb API key** (optional) — link to `https://www.omdbapi.com/apikey.aspx`

After the wizard completes, the server validates all provided keys with test requests before saving.

## Scheduled Tasks

Two existing scheduled tasks interact with metadata providers:

| Task | Schedule | Purpose |
|---|---|---|
| `metadata_refresh` | Every 6 hours (configurable) | Re-enriches items with changed metadata; downloads TMDB daily exports |
| `library_scan` | On-demand + scheduled | Full scan pipeline including Phase 5 enrichment |

No new scheduled tasks are needed. The `metadata_refresh` task is extended to:
1. Download and cache TMDB daily ID exports
2. Check TMDB `/changes` for items modified since last refresh
3. Re-enrich changed items through the enrichment orchestrator
4. Refresh supplementary provider data for changed items

## Metrics

Provider metrics are exposed via the existing Prometheus endpoint (see [LOGGING_OBSERVABILITY.md](../operations/LOGGING_OBSERVABILITY.md)):

| Metric | Type | Labels |
|---|---|---|
| `metadata_provider_requests_total` | Counter | `provider`, `endpoint`, `status` |
| `metadata_provider_request_duration_seconds` | Histogram | `provider`, `endpoint` |
| `metadata_provider_rate_limit_hits_total` | Counter | `provider` |
| `metadata_provider_errors_total` | Counter | `provider`, `error_type` |
| `metadata_enrichment_duration_seconds` | Histogram | `media_type` |
| `metadata_enrichment_items_total` | Counter | `media_type`, `status` |

## Research Sources

- TMDB Getting Started: `https://developer.themoviedb.org/docs/getting-started`
- TMDB Rate Limiting: `https://developer.themoviedb.org/docs/rate-limiting`
- TMDB FAQ: `https://developer.themoviedb.org/docs/faq`
- TMDB Authentication: `https://developer.themoviedb.org/docs/authentication-application`
- TMDB Image Basics: `https://developer.themoviedb.org/docs/image-basics`
- TMDB Append To Response: `https://developer.themoviedb.org/docs/append-to-response`
- TMDB Finding Data: `https://developer.themoviedb.org/docs/finding-data`
- TMDB Daily ID Exports: `https://developer.themoviedb.org/docs/daily-id-exports`
- TMDB Search & Query: `https://developer.themoviedb.org/docs/search-and-query-for-details`
- TMDB Languages: `https://developer.themoviedb.org/docs/languages`
- TVDB API v4 Swagger: `https://thetvdb.github.io/v4-api/`
- TVDB API & Licensing: `https://thetvdb.com/api-information`
- Fanart.tv API: `https://fanarttv.docs.apiary.io/`
- OMDb API: `https://www.omdbapi.com/`
- SubDL API Docs: `https://subdl.com/api-doc`
- Jellyfin Metadata Providers: `https://jellyfin.org/docs/general/server/metadata/`
- Jellyfin Provider Identifiers: `https://jellyfin.org/docs/general/server/metadata/identifiers/`

## Implementation Notes

### TmdbClient (Tasks 2–6)

- **Module:** `server/src/services/tmdb_client.rs` — dedicated module following project convention (modular service files over large singletons). metadata.rs retains traits/types/registry/orchestrator; tmdb_client.rs owns the concrete HTTP implementation.
- **HTTP client:** `reqwest::Client` owned per TmdbClient instance; 30s request timeout; 10s connect timeout; `redirect(Policy::none())` per API_SECURITY.md SSRF hardening. Bearer token set per-request via `Authorization` header.
- **Response deserialization:** 17 TMDB-specific `Deserialize` types with `Option<T>` throughout (TMDB API responses vary significantly between items). Converted to domain types via private helper methods (`convert_credits`, `convert_videos`, `convert_images`, `convert_external_ids`).
- **Search:** `#[serde(untagged)]` enum `TmdbSearchItem` handles both movie (`title`/`release_date`) and TV (`name`/`first_air_date`) result shapes. Year extracted from date string via `d.get(..4)`.
- **Details:** `append_to_response=credits,videos,external_ids,images` in single request per METADATA_PROVIDERS.md batching recommendation. `include_image_language=en,null` ensures English + language-neutral images.
- **Find by IMDb:** `/find/{id}?external_source=imdb_id` returns separate `movie_results` and `tv_results` arrays; movies checked first (more common).
- **Configuration caching:** `fetch_configuration()` calls `/configuration` and returns `TmdbConfig` with fallback defaults. Stored in `EnrichmentOrchestrator` as `Arc<ArcSwap<TmdbConfig>>` for atomic hot-reload via `refresh_tmdb_config()`.
- **Error mapping:** HTTP 401 → `AuthenticationFailed`, 404 → `NotFound`, 429 → `RateLimited`, other → `InvalidResponse` with parsed TMDB error message; JSON parse failures → `InvalidResponse`.
- **Dependencies added:** `urlencoding = "2"` for query parameter encoding (TMDB search queries may contain special characters).
- **TmdbClient derives Clone:** `reqwest::Client` is cheaply cloneable; enables storing in both the registry (as `Box<dyn MetadataProvider>`) and orchestrator (for direct config refresh access).
