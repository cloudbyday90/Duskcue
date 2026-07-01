# Project Structure

## Overview

This document defines the monorepo layout for the entire platform — Rust server, SvelteKit web client, Tauri desktop wrapper, and Flutter mobile client. It covers Cargo workspace design, SvelteKit module conventions, and how the components relate.

## Monorepo Layout

```
project/
├── Cargo.toml                    # Workspace root (virtual manifest)
├── Cargo.lock                    # Single lockfile for all Rust crates
├── docker-compose.yml            # Production: single-container with embedded PostgreSQL
├── .env.example                  # Environment variable template
├── Dockerfile                    # Multi-stage Alpine build (x86_64 + ARM64)
├── docker/
│   └── entrypoint.sh             # PUID/PGID user creation, privilege drop
├── .gitignore
├── CHANGELOG.md
├── PROJECT.md
├── DATABASE.md
├── ERROR_HANDLING.md
├── BACKUP_RECOVERY.md
├── MIGRATION_STRATEGY.md
├── DOCKER_DEPLOYMENT.md
├── OS_HARDENING.md
├── API_SECURITY.md
│
├── server/                       # Rust server (main crate)
│   ├── Cargo.toml
│   ├── sqlx.toml                 # sqlx-cli configuration
│   ├── migrations/               # Timestamp-based SQL migrations
│   │   ├── 20260530030000_create_core_media_tables.sql
│   │   ├── 20260530030100_create_trakt_integration.sql
│   │   ├── 20260530030200_create_activity_analytics.sql
│   │   ├── 20260530030300_create_playback_domain.sql
│   │   ├── 20260530040000_create_auth_domain.sql
│   │   ├── 20260530050000_create_system_domain.sql
│   │   ├── 20260530060000_create_cross_cutting_concerns.sql
│   │   ├── 20260530060100_create_audit_triggers.sql
│   │   ├── 20260530060200_create_full_text_search.sql
│   │   └── 20260530070000_seed_default_data.sql
│   └── src/
│       ├── main.rs               # Entry point: config → DB → migrate → serve
│       ├── lib.rs                # App builder, Router assembly, AppState
│       ├── config.rs             # Bootstrap config (TOML/ENV), AppState construction
│       ├── state.rs              # AppState struct, Clone impl, RateLimitState, GeoIP (ArcSwap), HW accel cache, Webauthn (Arc), WebauthnChallenge (DashMap), LibraryWatcherManager (Arc)
│       ├── error.rs              # Unified AppError + IntoResponse
│       ├── extractors.rs         # Custom Axum extractors (AuthenticatedUser, Require<C>, PaginationParams, DeviceProfile, etc.)
│       ├── middleware.rs         # Tower middleware (logging, CORS, rate limiting, HTTP metrics, metrics subnet guard)
│       ├── logging.rs            # Tracing subscriber init + Prometheus metrics recorder init
│       ├── router.rs             # Top-level router assembly (/health, /metrics), merges domain routers
│       │
│       ├── bin/
│       │   └── verify_migrations.rs  # Disposable PostgreSQL migration verifier
│       │
│       ├── db/                   # Database layer (sqlx queries)
│       │   ├── mod.rs
│       │   └── models/           # Row structs, FromRow derives
│       │       ├── mod.rs
│       │       ├── user.rs
│       │       ├── library.rs
│       │       ├── media_item.rs
│       │       ├── media_file.rs
│       │       ├── play_session.rs
│       │       ├── server_config.rs
│       │       ├── scheduled_task.rs
│       │       ├── subtitle.rs
│       │       ├── artwork.rs
│       │       ├── collection.rs
│       │       ├── segment.rs
│       │       ├── storyboard.rs
│       │       ├── overlay.rs
│       │       ├── device_profile.rs
│       │       └── migration.rs
│       │
│       ├── domains/              # Domain modules (one per business domain)
│       │   ├── mod.rs
│       │   ├── auth/             # Authentication & authorization
│       │   │   ├── mod.rs        # Router assembly
│       │   │   ├── handlers.rs   # Axum route handlers
│       │   │   ├── service.rs    # Business logic
│       │   │   ├── error.rs      # Domain error enum
│       │   │   └── types.rs      # Request/response DTOs
│       │   │
│       │   ├── users/            # User management
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs
│       │   │   └── types.rs
│       │   │
│       │   ├── libraries/        # Library management
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs
│       │   │   └── types.rs
│       │   │
│       │   ├── media/            # Media items, files, metadata
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs
│       │   │   └── types.rs
│       │   │
│       │   ├── playback/         # Play sessions, bookmarks, playlists
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs
│       │   │   └── types.rs
│       │   │
│       │   ├── analytics/        # Activity, trust scores, Tautulli-style
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs
│       │   │   └── types.rs
│       │   │
│       │   ├── trakt/            # Trakt.tv native integration
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs
│       │   │   └── types.rs
│       │   │
│       │   ├── system/           # Server config, scheduled tasks, notifications
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs
│       │   │   └── types.rs
│       │   │
│       │   ├── notifications/    # In-app notification center CRUD + preferences
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs
│       │   │   └── types.rs
│       │   │
│       │   └── backup/           # Backup coordination, WAL-G integration
│       │       ├── mod.rs
│       │       ├── handlers.rs
│       │       ├── service.rs
│       │       ├── error.rs
│       │       └── types.rs
│       │
│       │   ├── migration/         # Platform migration (Plex/Jellyfin/Emby watch data import)
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs       # MIGR_001–011
│       │   │   └── types.rs
│       │   │
│       │   ├── subtitles/         # Subtitle discovery, conversion, sync, fetching
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs       # SUB_001–006
│       │   │   └── types.rs
│       │   │
│       │   ├── quality/           # Device profiles, network assessment, transcoding decisions
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs       # QUALITY_001–012
│       │   │   └── types.rs
│       │   │
│       │   ├── overlays/          # Overlay compositing, badge/text/backdrop overlays
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs       # OVERLAY_001–006
│       │   │   └── types.rs
│       │   │
│       │   ├── collections/       # Static, dynamic, smart collections; builders
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs       # COLL_001–008
│       │   │   └── types.rs
│       │   │
│       │   ├── segments/          # Intro/credit/recap/preview detection
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs       # SEGMENT error codes
│       │   │   └── types.rs
│       │   │
│       │   ├── storyboards/       # Seek preview thumbnails (WebVTT + WebP sprites)
│       │   │   ├── mod.rs
│       │   │   ├── handlers.rs
│       │   │   ├── service.rs
│       │   │   ├── error.rs
│       │   │   └── types.rs
│       │   │
│       │   └── search/            # Full-text search coordination
│       │       ├── mod.rs
│       │       ├── handlers.rs
│       │       ├── service.rs
│       │       ├── error.rs
│       │       └── types.rs
│       │
│       ├── services/             # Cross-domain services
│       │   ├── mod.rs
│       │   ├── encryption.rs    # AES-256-GCM encryption at rest (ring::aead, provider key encrypt/decrypt, secret masking)
│       │   ├── conditions.rs    # Media condition/filter evaluation engine (JSONB rule evaluator, 8 operators, nested AND/OR, shared by overlays + smart collections) — **implemented**
│       │   ├── event_bus.rs     # Per-user pub/sub for SSE: DashMap<Uuid, broadcast::Sender>, 100-event ring buffer, ConnectionGuard — **implemented**
│       │   ├── events_handler.rs # SSE transport: GET /api/v1/events handler (?types= filter, Last-Event-ID replay, X-Accel-Buffering, 15s KeepAlive) — **implemented**
│       │   ├── scheduler.rs      # Scheduled task runner
│       │   ├── fs_watcher.rs     # Filesystem watcher (notify + notify-debouncer-full)
│       │   ├── media_matching.rs # 5-layer identification cascade (.media-match, provider ID tags)
│       │   ├── nfo_parser.rs     # NFO XML parsing (quick-xml streaming StAX, Kodi/Jellyfin/Emby formats)
│       │   ├── transcoding.rs    # FFmpeg integration (tokio-process-tools, -progress pipe:1)
│       │   ├── metadata.rs       # Provider registry, enrichment orchestrator, provider traits, provider stubs
│       │   ├── tmdb_client.rs    # TMDB v3 API client (Bearer token, append_to_response, search/details/find/config)
│       │   ├── tvdb_client.rs    # TVDB v4 API client (JWT auth, Arc<Inner> for Clone, search/series/movies/artworks)
│       │   ├── fanart_client.rs  # Fanart.tv v3 API client (api_key query param, movie/TV artwork by TMDB/TVDB ID)
│       │   ├── omdb_client.rs    # OMDb API client (apikey query param, ratings lookup by IMDb ID)
│       │   ├── trakt_client.rs   # Trakt.tv OAuth + user-settings HTTP client (device code flow, token refresh with write-back, /users/settings) — **implemented**
│       │   ├── subdl_client.rs   # SubDL subtitle API client (api_key query param, search by TMDB/IMDb/name, ZIP download) — **implemented**
│       │   ├── opensubtitles_client.rs  # OpenSubtitles subtitle API client (Api-Key header, hash/TMDB/IMDb/query search, two-step download) — **implemented**
│       │   ├── notifications.rs  # Notification dispatch
│       │   ├── search.rs         # Full-text search coordination
│       │   ├── security.rs       # TLS (rustls), HMAC signing (ring), security headers
│       │   ├── subtitle_discovery.rs  # External + embedded subtitle discovery during library scan — **implemented**
│       │   ├── subtitles.rs      # Subtitle text processing (format conversion SRT/ASS/WebVTT, FPS adjustment, offset correction, OCR scaffold, voice activity alignment) — **implemented**
│       │   ├── quality.rs        # Device capability probing, network assessment
│       │   ├── overlays.rs       # Compositing pipeline (image + ab_glyph + resvg) — **implemented**
│       │   ├── collections.rs    # Dynamic collection builder engine and manual sync persistence — **implemented**
│       │   ├── segments.rs       # Chromaprint, black frame, silence detection
│       │   ├── storyboards.rs    # FFmpeg thumbnail extraction, WebP sprite generation
│       │   ├── geoip.rs          # MaxMind GeoLite2 MMDB lookups (maxminddb + ArcSwap) — **implemented**
│       │   ├── sandbox.rs        # FFmpeg per-process sandboxing (landlock + seccompiler) — **implemented**
│       │   ├── hw_accel.rs       # Hardware acceleration runtime detection (FFmpeg probe, platform checks, priority selection) — **implemented**
│       │   ├── image_pipeline.rs # WebP encode/resize/variant generation (image 0.25 decode + webp 0.3 libwebp encode; alpha-aware lossy/lossless) — **implemented**
│       │   ├── artwork_delivery.rs # Artwork delivery orchestration (resolve artwork row, cache lookup, on-demand WebP variant generation, overlaid-result check) — **implemented**
│       │   ├── overlays.rs       # Overlay compositing pipeline (image + text + backdrop; ab_glyph + resvg; group/suppress/queue resolution) — **implemented**
│       │   ├── conditions.rs     # Pure condition evaluation engine (JSONB filter rules, 8 operators, nested AND/OR) — **implemented**
│       │   ├── clean_art.rs      # Clean art preservation (content-addressed clean backups, Blake3 config hash, artwork_overlay_state CRUD, overlaid-result resolution) — **implemented**
│       │   ├── backup.rs         # Shared backup coordinator (WAL-G/pg_dump/verify command spawning, WAL-G env construction, retention cleanup, operation lock) — **implemented**
│       │   ├── recovery_drill.rs # Recovery drill service (disposable PostgreSQL via Docker Compose, pg_restore, structural checks, evidence schema) — **implemented**
│       │  
│       └── workers/              # Background task definitions
│           ├── mod.rs
│           ├── library_scanner.rs
│           ├── metadata_refresh.rs
│           ├── partition_manager.rs
│           ├── backup_runner.rs
│           ├── trakt_sync.rs
│           ├── soft_delete_purge.rs
│           ├── segment_detector.rs
│           ├── storyboard_generator.rs
│           ├── overlay_compositor.rs
│           ├── collection_sync.rs
│           ├── geoip_updater.rs
│           ├── subtitle_processor.rs
│           ├── reindex_maintenance.rs
│           ├── disk_space_check.rs
│           ├── asset_directory_scanner.rs
│           └── recovery_drill_runner.rs # Scheduler adapter for backup_recovery_drill (disposable PostgreSQL restore + structural checks) — **implemented**
│
├── crates/                       # Shared Rust crates (workspace members)
│   ├── types/                    # Shared types, DTOs, error definitions
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs          # Error code registry (AUTH_001, MEDIA_001, etc.)
│   │       └── config.rs         # ServerConfig structs (network, backup, etc.)
│   │
│   └── db/                       # Shared database types and query helpers
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           └── models/           # Re-exported row models (mirrors server/db/models/)
│
├── clients/                      # Client applications
│   ├── web/                      # SvelteKit web client
│   │   ├── package.json          # "type": "module" for ESM
│   │   ├── svelte.config.js
│   │   ├── vite.config.js
│   │   ├── src/
│   │   │   ├── routes/           # SvelteKit file-based routing
│   │   │   │   ├── +layout.svelte
│   │   │   │   ├── +page.svelte
│   │   │   │   ├── auth/
│   │   │   │   │   ├── login/+page.svelte
│   │   │   │   │   ├── setup/+page.svelte
│   │   │   │   │   └── link/+page.svelte     # Device linking
│   │   │   │   ├── libraries/
│   │   │   │   │   ├── +page.svelte
│   │   │   │   │   └── [id]/+page.svelte
│   │   │   │   ├── media/
│   │   │   │   │   ├── +page.svelte
│   │   │   │   │   └── [id]/+page.svelte
│   │   │   │   ├── play/
│   │   │   │   │   └── [id]/+page.svelte          # Full-screen player
│   │   │   │   ├── settings/
│   │   │   │   │   ├── +page.svelte           # Server overview
│   │   │   │   │   ├── users/+page.svelte
│   │   │   │   │   ├── libraries/+page.svelte
│   │   │   │   │   ├── backups/+page.svelte
│   │   │   │   │   ├── subtitles/+page.svelte
│   │   │   │   │   ├── quality/+page.svelte
│   │   │   │   │   ├── overlays/+page.svelte
│   │   │   │   │   ├── collections/+page.svelte
│   │   │   │   │   ├── migration/+page.svelte  # Migration wizard
│   │   │   │   │   ├── security/+page.svelte
│   │   │   │   │   └── storage/+page.svelte
│   │   │   │   ├── dashboard/+page.svelte
│   │   │   │   ├── analytics/+page.svelte
│   │   │   │   ├── search/+page.svelte
│   │   │   │   └── collections/+page.svelte
│   │   │   │
│   │   │   ├── lib/
│   │   │   │   ├── api/          # API client layer (ESM modules)
│   │   │   │   │   ├── index.js  # Barrel export
│   │   │   │   │   ├── core.js   # HTTP client (fetch wrapper)
│   │   │   │   │   ├── auth.js   # Auth endpoints
│   │   │   │   │   ├── users.js
│   │   │   │   │   ├── libraries.js
│   │   │   │   │   ├── media.js
│   │   │   │   │   ├── playback.js
│   │   │   │   │   ├── analytics.js
│   │   │   │   │   ├── trakt.js
│   │   │   │   │   ├── settings.js
│   │   │   │   │   ├── search.js
│   │   │   │   │   ├── subtitles.js
│   │   │   │   │   ├── quality.js
│   │   │   │   │   ├── overlays.js
│   │   │   │   │   ├── collections.js
│   │   │   │   │   ├── segments.js
│   │   │   │   │   ├── migration.js
│   │   │   │   │   └── storyboards.js
│   │   │   │   │
│   │   │   │   ├── stores/       # Svelte stores
│   │   │   │   │   ├── auth.js
│   │   │   │   │   ├── user.js
│   │   │   │   │   ├── libraries.js
│   │   │   │   │   ├── player.js
│   │   │   │   │   ├── notifications.js
│   │   │   │   │   ├── events.js
│   │   │   │   │   ├── subtitles.js
│   │   │   │   │   ├── quality.js
│   │   │   │   │   └── collections.js
│   │   │   │   │
│   │   │   │   ├── components/   # Reusable UI components
│   │   │   │   │   ├── MediaCard.svelte
│   │   │   │   │   ├── Player.svelte
│   │   │   │   │   ├── SearchBar.svelte
│   │   │   │   │   ├── NotificationToast.svelte
│   │   │   │   │   ├── SkipButton.svelte
│   │   │   │   │   ├── SeekPreview.svelte
│   │   │   │   │   ├── OverlayEditor.svelte
│   │   │   │   │   ├── CollectionGrid.svelte
│   │   │   │   │   └── SubtitleSelector.svelte
│   │   │   │   │
│   │   │   │   ├── composables/  # Reusable logic (Svelte actions/runes)
│   │   │   │   │   ├── useInfiniteScroll.js
│   │   │   │   │   ├── useMediaQuery.js
│   │   │   │   │   └── useEventSource.js
│   │   │   │   │
│   │   │   │   └── utils/        # Utility functions
│   │   │   │       ├── format.js
│   │   │   │       └── constants.js
│   │   │   │
│   │   │   └── app.html
│   │   │
│   │   ├── static/               # Static assets (favicons, etc.)
│   │   └── tests/                # Vitest + Playwright tests
│   │       ├── unit/
│   │       └── e2e/
│   │
│   ├── desktop/                  # Tauri 2 desktop wrapper
│   │   ├── package.json          # "type": "module" for ESM
│   │   ├── vite.config.js        # Shared config referencing ../web/vite.config.js
│   │   ├── src-tauri/
│   │   │   ├── Cargo.toml
│   │   │   ├── tauri.conf.json
│   │   │   ├── capabilities/
│   │   │   │   └── default.json
│   │   │   └── src/
│   │   │       ├── main.rs
│   │   │       └── lib.rs        # Tauri commands (system tray, file dialogs, deeplinks)
│   │   └── src/                  # Imports from ../web/src/ (SvelteKit shared code)
│   │       ├── app.html          # Tauri-specific shell (no SSR)
│   │       └── routes/           # Re-exports web client routes
│   │
│   └── mobile/                   # Flutter mobile client
│       ├── pubspec.yaml
│       ├── lib/
│       │   ├── main.dart
│       │   ├── api/              # API client
│       │   ├── models/           # Data models
│       │   ├── screens/          # UI screens
│       │   ├── widgets/          # Reusable widgets
│       │   ├── services/         # Business logic services
│       │   ├── stores/           # State management
│       │   └── utils/
│       └── test/
│
├── scripts/                      # Development and CI scripts
│   ├── dev.sh                    # Start all services for development
│   ├── migrate.sh                # Run database migrations
│   ├── seed.sh                   # Seed development data
│   └── verify-migrations.ps1     # Disposable Docker PostgreSQL migration verification
│
├── docker/
│   ├── entrypoint.sh             # Container startup entrypoint
│   └── compose.migrations.yml    # Disposable PostgreSQL 18 migration verification stack
│
└── docs/                         # Additional documentation
    ├── api/                      # OpenAPI specs, endpoint docs
    └── development/              # Setup guides, contributing
```

## Cargo Workspace Configuration

### Root `Cargo.toml`

```toml
[workspace]
resolver = "3"
members = [
    "server",
    "crates/types",
    "crates/db",
    "clients/desktop/src-tauri",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "AGPL-3.0"

[workspace.dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace", "compression-gzip", "set-header", "request-id"] }
governor = "0.6"
nonzero_ext = "0.3"
rusqlite = { version = "0.32", features = ["bundled"] }
rustls = { version = "0.23", default-features = false, features = ["ring", "std", "tls12"] }
tokio-rustls = { version = "0.26", default-features = false, features = ["ring"] }
ring = "0.17"
maxminddb = { version = "0.28", features = ["mmap"] }
validator = { version = "0.20", features = ["derive"] }
tokio-process-tools = "0.11"
landlock = "0.4"
seccompiler = "0.4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_with = "3"
sqlx = { version = "0.9", features = ["postgres", "runtime-tokio", "uuid", "chrono", "json", "migrate", "sqlx-toml"] }
uuid = { version = "1", features = ["v7", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
toml = "0.8"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
config = "0.15"
clap = { version = "4", features = ["derive", "env"] }
tokio-util = { version = "0.7", features = ["rt"] }
mimalloc = "0.1"
tracing-appender = "0.2"
tracing-error = "0.2"
dirs = "6"
arc-swap = "1"
metrics = "0.24"
metrics-exporter-prometheus = "0.18"
ipnet = "2"
rand = "0.9"
webauthn-rs = { version = "0.6.1-dev", features = ["danger-allow-state-serialisation", "danger-credential-internals"] }
dashmap = "5.5"
url = "2"
base64 = "0.22"
notify = "8"
notify-debouncer-full = "0.7"
ignore = "0.4"
blake3 = "1"
regex = "1"
croner = "3"
quick-xml = "0.40"
async-trait = "0.1"
urlencoding = "2"
flate2 = "1"
zip = "2"
tar = "0.4"
libc = "0.2"
chromaprint-next = "0.1"
image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp"] }
webp = { version = "0.3", default-features = false }
ab_glyph = "0.2"
resvg = "0.47"
```

**TLS backend note:** `rustls`, `tokio-rustls`, and `reqwest` use the `ring` crypto backend instead of the default `aws-lc-rs`. The `aws-lc-sys` crate requires NASM and CMake on Windows, which are not present in standard development environments. `ring` is pure Rust + precompiled assembly, builds everywhere, and is the same library used by `ring` 0.17 for HMAC signing. This is a workspace-level decision that applies to all workspace members.

### Dependency Flow

```
clients/desktop/src-tauri  →  server (for shared types via crates/types)
server                     →  crates/types, crates/db
crates/db                  →  crates/types, sqlx
crates/types               →  serde, uuid, chrono (no sqlx dependency)
```

`crates/types` has zero database dependencies — it only contains serializable DTOs, error code constants, and config struct definitions. This keeps the Tauri desktop crate lightweight.

## Rust Server Module Conventions

### Domain Module Pattern

Every domain follows the same five-file pattern:

| File | Purpose | Exports |
|---|---|---|
| `mod.rs` | Assembles the domain's `Router`, re-exports | `pub fn router() -> Router` |
| `handlers.rs` | Axum route handler functions | `pub async fn list_items(...)` |
| `service.rs` | Business logic, database queries | `pub async fn get_item(...)` |
| `error.rs` | Domain error enum with `#[error(...)]` | `pub enum LibraryError` |
| `types.rs` | Request/response DTOs, query params | `pub struct CreateLibraryRequest` |

### Handler → Service → Database Layering

```
handlers.rs          → service.rs          → db/models/
(HTTP in/out)          (business logic)       (SQL queries)
                        
Axum extractors       Validates input         sqlx::query_as!
JSON serialization    Enforces rules          FromRow derives
Error mapping         Coordinates deps        Transaction management
```

Handlers are thin — they extract parameters, call the service, and return the result. Business logic lives in service files. SQL queries live in service files or the `db/models` layer.

### `mod.rs` Router Assembly

```rust
// server/src/domains/libraries/mod.rs
pub use self::{error::LibraryError, handlers::*, types::*};

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/libraries", get(list).post(create))
        .route("/api/v1/libraries/{id}", get(get_one).delete(delete))
        .route("/api/v1/libraries/{id}/scan", post(scan))
        .with_state(state)
}
```

### `router.rs` — Top-Level Assembly

```rust
// server/src/router.rs
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(auth::router(state.clone()))
        .merge(users::router(state.clone()))
        .merge(libraries::router(state.clone()))
        .merge(media::router(state.clone()))
        .merge(notifications::router(state.clone()))
        .merge(playback::router(state.clone()))
        .merge(analytics::router(state.clone()))
        .merge(trakt::router(state.clone()))
        .merge(system::router(state.clone()))
        .merge(backup::router(state.clone()))
        .merge(migration::router(state.clone()))
        .merge(subtitles::router(state.clone()))
        .merge(quality::router(state.clone()))
        .merge(overlays::router(state.clone()))
        .merge(collections::router(state.clone()))
        .merge(segments::router(state.clone()))
        .merge(storyboards::router(state.clone()))
        .merge(search::router(state.clone()))
        .merge(system::admin_router(state.clone()))  // /api/v1/admin/* — requires can_manage_server
        .route("/health", get(health_check))
        .layer(middleware_stack())
}
```

### Response DTO Pattern (Three-Type)

Every domain uses three distinct types per entity (see [API_SECURITY.md](../security/API_SECURITY.md) for full policy):

| Type | File | Traits | Purpose |
|---|---|---|---|
| `XxxRow` | `db/models/*.rs` | `FromRow`, no `Serialize` | Database model — never sent to clients |
| `XxxRequest` | `domains/*/types.rs` | `Deserialize`, `Validate` | Inbound request — only user-writable fields |
| `XxxResponse` | `domains/*/types.rs` | `Serialize` | Outbound response — only safe fields |

**PATCH 3-state nullability:** `UpdateXxxRequest` types use all-`Option` fields for partial-update semantics. Plain `Option<T>` collapses JSON "absent" and "null" into one `None`, so it cannot express "clear to NULL." For PATCH fields where clearing to NULL is a meaningful workflow (e.g. overlay `library_id` NULL = "global"), use `Option<Option<T>>` annotated `#[serde(default, with = "::serde_with::rust::double_option")]` (`None`=unchanged, `Some(None)`=clear, `Some(Some(v))`=set), paired with a conditional-SET `QueryBuilder` rather than `COALESCE` (which can't clear, since `COALESCE(NULL, col) = col`). Adopted first in overlays (Phase 12 Task 10); see [PROJECT.md](../../PROJECT.md) Open Questions for the project-wide adoption recommendation.

## Web Client Conventions (SvelteKit + ESM)

### ESM Configuration

`package.json`:
```json
{
    "type": "module"
}
```

All JavaScript/TypeScript files use `import`/`export` syntax exclusively. No `require()`, no `module.exports`.

### API Client Layer Pattern

Each API module exports named functions — one function per endpoint. Svelte components and stores never call `fetch` directly.

```javascript
// src/lib/api/libraries.js
import { get, post } from './core.js';

export async function listLibraries(params = {}) {
    return get('/libraries', params);
}

export async function getLibrary(id) {
    return get(`/libraries/${id}`);
}

export async function createLibrary(data) {
    return post('/libraries', data);
}

export async function scanLibrary(id) {
    return post(`/libraries/${id}/scan`);
}
```

```javascript
// src/lib/api/core.js
const API_BASE = '/api/v1';

let bearerToken = null;

export function setBearerToken(token) {
    bearerToken = token;
}

export function clearBearerToken() {
    bearerToken = null;
}

export class ApiError extends Error {
    constructor(problem) {
        super(problem.detail || problem.title || `HTTP ${problem.status}`);
        this.name = 'ApiError';
        this.type = problem.type || '';
        this.title = problem.title || '';
        this.status = problem.status || 0;
        this.detail = problem.detail || '';
        this.traceId = problem.trace_id || '';
        this.instance = problem.instance || '';
        this.errors = problem.errors || null;
        this.retryAfter = null;
    }

    get isValidation() { return Array.isArray(this.errors); }
    get isRateLimited() { return this.status === 429; }
    get isUnauthorized() { return this.status === 401; }
    get isForbidden() { return this.status === 403; }
    get isNotFound() { return this.status === 404; }
    get isConflict() { return this.status === 409; }
    get isServerError() { return this.status >= 500; }

    fieldError(fieldName) {
        if (!this.errors) return undefined;
        return this.errors.find((e) => e.field === fieldName);
    }
}

export async function request(method, path, options = {}) {
    const search = new URLSearchParams();
    if (options.params) {
        for (const [key, value] of Object.entries(options.params)) {
            if (value === undefined || value === null) continue;
            search.set(key, Array.isArray(value) ? value.join(',') : String(value));
        }
    }
    const query = search.toString();
    const url = query ? `${API_BASE}${path}?${query}` : `${API_BASE}${path}`;

    const headers = { Accept: 'application/json' };
    if (bearerToken) headers['Authorization'] = `Bearer ${bearerToken}`;
    if (options.body !== undefined) headers['Content-Type'] = 'application/json';
    if (options.ifNoneMatch) headers['If-None-Match'] = options.ifNoneMatch;

    const response = await fetch(url, {
        method,
        headers,
        credentials: 'same-origin',
        body: options.body !== undefined ? JSON.stringify(options.body) : undefined,
        signal: options.signal,
    });

    if (response.status === 204 || response.status === 304) return null;
    if (!response.ok) {
        const problem = await response.json().catch(() => ({
            type: `/errors/http_${response.status}`,
            title: `HTTP_${response.status}`,
            status: response.status,
            detail: response.statusText,
        }));
        const error = new ApiError(problem);
        error.retryAfter = parseInt(response.headers.get('Retry-After'), 10) || null;
        throw error;
    }
    return response.json();
}

export function get(path, params = {}, options = {}) {
    return request('GET', path, { ...options, params });
}

export function post(path, body = undefined, options = {}) {
    return request('POST', path, { ...options, body });
}

export function patch(path, body = undefined, options = {}) {
    return request('PATCH', path, { ...options, body });
}

export function put(path, body = undefined, options = {}) {
    return request('PUT', path, { ...options, body });
}

export function del(path, options = {}) {
    return request('DELETE', path, options);
}
```

### Barrel Export

```javascript
// src/lib/api/index.js
export * from './core.js';
export * from './auth.js';
export * from './users.js';
export * from './libraries.js';
export * from './media.js';
export * from './playback.js';
export * from './analytics.js';
export * from './trakt.js';
export * from './settings.js';
export * from './search.js';
export * from './subtitles.js';
export * from './quality.js';
export * from './overlays.js';
export * from './collections.js';
export * from './segments.js';
export * from './migration.js';
export * from './storyboards.js';
```

`core.js` is included in the barrel export so consumers can import `ApiError`, `setBearerToken`, `clearBearerToken`, and `buildApiUrl` alongside domain functions from a single `import { ... } from '$lib/api'`.

Modules with implemented endpoints (Phases 1–7 complete): `auth.js`, `users.js`, `libraries.js`, `media.js`, `playback.js`, `quality.js`, `settings.js`, `search.js`. Remaining modules (`analytics.js`, `trakt.js`, `subtitles.js`, `overlays.js`, `collections.js`, `segments.js`, `migration.js`, `storyboards.js`) are license-header-only stubs until their respective phases are built.

Streaming endpoints (Direct Play file serving, HLS manifest/playlist/segment) use **URL builder functions** (e.g., `streamFileUrl()`, `transcodeManifestUrl()`) rather than fetch wrappers — these return URL strings consumed by the `<video>` element's `src` attribute or hls.js, since the browser handles those fetches directly with Range headers.

### Store Pattern

```javascript
// src/lib/stores/libraries.js
import { writable, derived } from 'svelte/store';
import { listLibraries } from '../api/libraries.js';

function createLibrariesStore() {
    const { subscribe, set, update } = writable({
        items: [],
        loading: false,
        error: null,
    });

    return {
        subscribe,
        async fetch() {
            update(state => ({ ...state, loading: true, error: null }));
            try {
                const data = await listLibraries();
                set({ items: data, loading: false, error: null });
            } catch (error) {
                set({ items: [], loading: false, error });
            }
        },
    };
}

export const libraries = createLibrariesStore();
```

**Implemented stores (Phase 8 Task 3 + Phase 10 Task 12):** 6 stores built using `svelte/store` writable/derived with the factory-function pattern above. Each store encapsulates `set`/`update` via closures, exposing only `subscribe` + domain-specific action methods. Derived stores are exported for fine-grained subscriptions (e.g., `isPlaying`, `currentPosition`, `progressPercent` from the player store).

| Store | Exports | Responsibility |
|---|---|---|
| `stores/auth.js` | `auth` + 7 derived + `hasCapability()` | Session lifecycle, login flows, user identity, capability checks with owner bypass; caches user in localStorage |
| `stores/user.js` | `user` + 4 derived | Active sessions, passkey management, UI preferences (localStorage) including per-type segment auto-skip toggles (`autoSkipIntro`/`autoSkipCredits`/`autoSkipRecap`/`autoSkipPreview`/`autoSkipOutro`, all default `false` — added Phase 10 Task 7) |
| `stores/libraries.js` | `libraries` + 6 derived | Library CRUD, selection context, path management, per-library scanning flags, `storyboardProgress` SSE consumer (Phase 10 Task 12 — dispatches `storyboard_progress` events to progress state + completion toasts) |
| `stores/player.js` | `player` + 11 derived | Playback lifecycle, 15s heartbeat timer, stream URL resolution (direct file vs HLS), volume persistence |
| `stores/notifications.js` | `notifications` + 1 derived | Toast system with auto-dismiss (5s default, 8s errors), FIFO eviction (max 5) |
| `stores/events.js` | `events` + 4 derived | SSE client store — manages browser `EventSource` lifecycle; handler registry dispatches named events to domain stores; connects on auth, disconnects on logout; SSR-safe (Phase 10 Task 12) |

All stores consume API client functions from `../api/*.js` — never calling `fetch` directly. The single exception is `stores/events.js`, which uses the browser's native `EventSource` API (not `fetch`) for the SSE connection to `GET /api/v1/events`. All localStorage access is SSR-safe (`typeof localStorage !== 'undefined'` checks) for `adapter-node`. WebAuthn credential API calls (`navigator.credentials.get()`/`create()`) are delegated to the caller via injected callbacks, keeping stores framework-agnostic and testable.

**Implemented components (Phase 8 Task 4 + Phase 10 Tasks 7–8 + Phase 12 Task 10):** 7 components built using Svelte 5 runes (`$props`, `$state`, `$derived`, `$effect`) alongside `svelte/store` auto-subscription. All components consume stores and API client functions — never calling `fetch` directly (exception: `SeekPreview.svelte` fetches the WebVTT index via raw `fetch()` since it's a text file, not JSON).

| Component | Props | Responsibility |
|---|---|---|
| `NotificationToast.svelte` | (none — subscribes to store) | Fixed-position toast container; subscribes to `notifications` store; per-type accent colors + SVG icons; fly/fade/flip transitions; dismiss button |
| `SearchBar.svelte` | `value` (bindable), `placeholder`, `compact`, `autofocus`, `navigate`, `onsearch`, `oninput` | Debounced search input (300ms); navigates to `/search?q=...` via `goto()`; compact mode for nav bar |
| `MediaCard.svelte` | `item`, `posterUrl`, `progress`, `showOverview`, `onclick` | Content-first media card — `<a>` linking to `/media/{id}`; poster or gradient placeholder; rating/type badges; hover overview overlay; optional progress bar |
| `Player.svelte` | `mediaItem`, `mediaFileId`, `startPositionMs`, `sessionId`, `title`, `onstop` | Full HLS player with hls.js 1.6.16; transport controls (play/pause, seek, volume, speed, fullscreen); keyboard shortcuts; QoE telemetry; auto-hide controls; wires `SkipButton` — fetches segments on mount, filters by `confidence ≥ 0.7 OR is_manual`, dispatches direct-play vs transcode seeks via `handleSkip` (Phase 10 Task 7); wires `SeekPreview` — fetches storyboard metadata, tracks hover/touch on seek bar, renders thumbnail tooltip during hover and scrub (Phase 10 Task 8) |
| `SkipButton.svelte` | `segments`, `positionMs`, `autoSkipTypes`, `onskip` | Skip-button overlay rendered during detected intro/credits/recap/preview/outro windows; bottom-right placement per industry standard; two-tier prominence by confidence (high=brass accent/10s, medium=subdued blur/5s); per-segment auto-skip + dismiss deduplication via Sets; purely presentational (Phase 10 Task 7) |
| `SeekPreview.svelte` | `storyboard`, `visible`, `positionMs`, `hoverRatio`, `displayWidth` | Seek-preview thumbnail tooltip; lazily fetches + parses WebVTT index; resolves sprite references to absolute URLs; renders correct sprite-sheet region via CSS `background-image` + `background-position`; edge-clamped positioning via CSS `clamp()`; time label bar; binary-search cue lookup; works during hover and active seek drag (Phase 10 Task 8) |
| `ConditionBuilder.svelte` | `node`, `depth`, `onchange`, `onremove` | Recursive JSONB condition-tree editor (overlays + smart collections); "Match all/any" toggle; per-rule smart mini-forms adapting by field type (text/number/boolean); nested groups depth-capped at 3; explicit `onchange`/`onremove` callbacks over fragile `$bindable` recursion (Phase 12 Task 10) |

Design tokens are established in `app.css` as CSS custom properties implementing UI_FOUNDATIONS.md's low-light editorial palette (deep charcoal surfaces, warm off-white text, brass/amber accent, muted semantic colors). `utils/format.js` provides `formatDuration`, `formatTimestamp`, `formatYear`, `formatRating`, `formatPercent`. `utils/constants.js` provides `MEDIA_TYPE_LABELS`, `NOTIFICATION_ICONS`, and player/search timing constants. `utils/storyboards.js` provides `parseStoryboardVtt` (WebVTT cue extraction), `findCueForTime` (binary search), and `parseTimecodeToMs` (Phase 10 Task 8).

## Why This Structure

### Why Cargo Workspace (not separate repos)

| Factor | Monorepo + Workspace | Separate Repos |
|---|---|---|
| **Shared types** | Single source of truth across server + Tauri | Duplicate type definitions |
| **Atomic changes** | Server API change + client update in one PR | Coordinated cross-repo PRs |
| **Build consistency** | Single `Cargo.lock`, same dependency versions | Dependency drift between repos |
| **CI simplicity** | One pipeline, one checkout | N pipelines, version coordination |
| **Refactoring** | `cargo refactor` touches all crates | Manual cross-repo updates |
| **Overhead** | One `cargo build` compiles all crates | N independent build systems |

### Why Domain Modules (not flat files)

| Factor | Domain Modules | Flat Structure |
|---|---|---|
| **Coupling** | Each domain is self-contained | All handlers share one error.rs, one types.rs |
| **Team scaling** | One dev owns one domain | Merge conflicts in shared files |
| **Testing** | Test one domain in isolation | Must import everything |
| **Code review** | PRs touch 1-2 domain dirs | PRs touch scattered files |

### Why Five-File Domain Pattern

The handler → service → model split ensures:
- **Handlers** are pure HTTP translation (no business logic)
- **Service** files are testable without Axum (no HTTP types)
- **Models** are pure data (no behavior)
- **Errors** are domain-specific (typed, not catch-all strings)
- **Types** are the API contract (separate from database models)

### Why Separate `crates/types`

The Tauri desktop app needs access to shared types (API response shapes, error codes, config structs) without pulling in the entire server + sqlx + tokio. `crates/types` is a zero-dependency crate (only `serde`, `uuid`, `chrono`) that both the server and Tauri can import.

### Why Desktop Reuses Web Client

The Tauri desktop app (`clients/desktop/`) imports the SvelteKit web client (`clients/web/`) rather than duplicating UI code. This works because:

- **Same framework** — Tauri 2 renders SvelteKit in a webview; identical code runs in both browser and desktop
- **Shared `src/routes/`** — desktop `src/routes/` re-exports web client routes; no SSR needed (Tauri uses static adapter)
- **Native shell only** — `src-tauri/src/lib.rs` adds system tray, file dialogs, and deeplinks; everything else is the web client
- **Single API client** — both web and desktop use the same `src/lib/api/` layer against the same REST API
- **One change propagates** — UI fix in `clients/web/` appears in desktop on next build

### Phase 16a Client Structure Update

[DESKTOP_MOBILE_CLIENTS.md](DESKTOP_MOBILE_CLIENTS.md) is the authoritative Phase 16a design document for the desktop/mobile clients. Its Task 0 research confirms these structure rules:

- `clients/desktop/` remains a Tauri 2 shell around the web client, not a separate native desktop UI.
- Tauri native access is granted through a minimal `src-tauri/capabilities/default.json`; plugins are added only when a Phase 16a task needs them.
- Desktop token persistence must use Tauri Stronghold or OS-backed secure storage, not browser localStorage.
- `clients/mobile/` must be a generated Flutter Android/iOS project, including `android/`, `ios/`, `lib/`, `test/`, and `integration_test/` folders.
- Flutter owns cross-platform UI, routing, and state; native adapters or vetted plugins own passkeys, push tokens, secure storage, and playback controls where platform APIs are required.
- Android playback is Media3/ExoPlayer-backed and iOS playback is AVPlayer/AVFoundation-backed, even if surfaced through a Flutter plugin.

**Task 1 scaffold status:** `clients/desktop` now has a valid Tauri 2 Rust entrypoint/config/capability set and delegates dev/build to the shared web client. `clients/mobile` now has the Flutter app shell, dependency baseline, Android package scaffold, iOS Runner metadata, tests, lint config, and first-run commands documented in [DESKTOP_MOBILE_CLIENTS.md](DESKTOP_MOBILE_CLIENTS.md).

**Task 3 connection status:** `clients/mobile` now owns the visible server-selection/onboarding flow, saved-server list, last-used server, and `/health/ready` connection test. `clients/desktop/src-tauri` now owns native commands for canonical server-origin validation, saved-server persistence, and readiness testing, while the shared web API core can target an explicit selected origin for Tauri static builds. Both clients canonicalize server URLs to `http(s)://<server>:48027` and reject the internal Docker API port `48028`.

**Task 4 auth status:** `clients/mobile` now contains auth/session models, an `AuthService`, secure device identity generation, a native passkey method-channel adapter, `/auth` routing, saved-session restore, and account/session management in settings. `clients/desktop/src-tauri` stores session tokens in the OS keyring through Tauri commands; only non-secret saved-server metadata remains in app data JSON.

**Task 5 desktop wrapper status:** `clients/desktop` now builds the shared SvelteKit app through a desktop-only static adapter path while normal web/Docker builds keep adapter-node. `clients/desktop/src-tauri` owns tray actions, `duskcue://` deep-link routing, single-instance forwarding, native folder dialogs, and native notification dispatch. `clients/web/src/lib/desktop/tauri.js` is the web-to-native bridge used by the shared web UI when it runs inside Tauri.

**Task 6 mobile shell status:** `clients/mobile/lib/navigation/app_router.dart` now uses a Riverpod-backed GoRouter `StatefulShellRoute.indexedStack` for the authenticated mobile app shell. `clients/mobile/lib/services/content_service.dart` is the current Dart browsing API boundary for libraries, media, search, collections, and notifications. Task 6 screen files live under `clients/mobile/lib/screens/`, reusable authenticated artwork/list state widgets live under `clients/mobile/lib/widgets/`, and shell strings are centralized in `clients/mobile/lib/l10n/app_strings.dart` pending a generated mobile localization catalog.

**Task 7 mobile playback status:** `clients/mobile/lib/services/playback_service.dart` is the Dart boundary for playback start, heartbeat, seek, stop, watch-data refresh, subtitles, segments, and media-file stream metadata. `clients/mobile/lib/screens/playback_entry_screen.dart` owns the current Flutter `video_player` route and in-app controls. `clients/mobile/lib/models/playback_models.dart` contains the tolerant response DTOs used until Phase 16d promotes broader generated client schemas.

**Task 12 packaging status:** `.github/workflows/client-packaging.yml` now runs desktop Tauri package smoke builds on Linux/Windows/macOS and mobile Flutter analysis/tests plus Android debug/release package smoke. `docs/ci/CLIENT_PACKAGING.md` records desktop/mobile package identities, app IDs, permission/privacy notes, signing/notarization placeholders, and the deferred updater decision. `clients/mobile/test/` now includes focused tests for API error mapping, server URL validation, session clearing, playback helpers, notification handling, and quality payloads.

**Task 8 mobile realtime status:** `clients/mobile/lib/services/realtime_service.dart` owns foreground SSE transport, parsing, reconnect, and `Last-Event-ID` replay. `clients/mobile/lib/stores/realtime_store.dart` stores connection status, unread notification count, and the latest event metadata for shell/screens. `clients/mobile/lib/widgets/app_shell.dart` is responsible for tying the SSE lifecycle to authenticated foreground app state.

**Task 9 mobile push status:** `clients/mobile/lib/services/push_registration_service.dart` owns FCM/APNs/optional UnifiedPush token registration, secure storage of returned push-device IDs, 24-hour heartbeat refresh, token-rotation handling, and safe notification tap routing into the authenticated shell. Server-side provider dispatch remains in `server/src/services/notification_dispatch.rs`, backed by nested push config structs in `server/src/state.rs` and encrypted provider private-key handling in `server/src/services/encryption.rs`.

**Task 10 mobile quality status:** `clients/mobile/lib/services/quality_service.dart` owns mobile capability reporting, per-item quality preference storage, bandwidth probes, telemetry, and QoE submission. `clients/mobile/lib/screens/playback_entry_screen.dart` owns the current Quality picker and playback-scoped probe/QoE timers. The server playback quality-mode contract lives in `server/src/domains/playback/types.rs` and `server/src/domains/playback/service.rs`.

**Task 11 mobile settings status:** `clients/mobile/lib/screens/settings_screen.dart` owns the mobile account/settings hub, including profile/server summary, current-device session labeling, session revocation, passkey registration/list/delete, notification preference toggles, push-device status/revocation, default quality mode, and the web-first admin settings handoff. `clients/mobile/lib/services/auth_service.dart` owns the typed account-management API calls, while `server/src/domains/auth/types.rs` and `server/src/domains/auth/handlers.rs` expose `device_id` in session list responses and preserve passkey display names.

### Phase 16b TV Domain Structure Update

[TV_PLATFORM_SURFACES.md](TV_PLATFORM_SURFACES.md) is the authoritative Phase 16b design document for TV and console surface contracts.

**Task 1-13 server-domain status:** `server/src/domains/tv/` now follows the five-file domain pattern with authenticated surface, resolve, settings, and admin diagnostics routes. `server/src/domains/tv/types.rs` owns the TV feed, section, item, resolve, playback-start hints, settings, diagnostics, availability, platform-ID target, access-status, lookup, and SSE event DTOs; `service.rs` owns `platform`/`limit`/`sections` validation, canonical/strict/URL `platform_content_id` utilities, media-type matching, shared `TvAccessScope`, inverse lookup with current user library access, feed queries for Continue Watching, Next Up, New Episodes, deterministic Recommendations, bounded availability details, structured diagnostics, TV surface metrics, playback-ready resolve responses, debounced `tv_surface_changed` event helpers, and per-user TV publication settings persisted in `users.metadata.tv_surface_settings`. Recommendations score enabled collection membership plus recent genre/tag/credit overlap before deterministic rating/date/title fallback. Diagnostics classify excluded candidates without exposing paths or internal server details. Resolve reloads the current item summary, current authenticated user's resume state, and current best healthy media file before returning `/api/v1/playback/start` hints. `mod.rs` applies private TV surface cache headers plus conditional ETags to the feed route, while `error.rs` maps through central `AppError` as `TV_001`-`TV_008`. Event producers now live in playback, libraries, collections, posters, overlays, users, auth capability handlers, scheduled metadata refresh, scheduled library scan, filesystem-triggered scan paths, and TV settings updates. `clients/web/src/lib/api/tv.js` provides the reference web API helpers for the TV route family. The shared living-room UX and platform adapter contracts are documented in `TV_PLATFORM_SURFACES.md`; together they cover row order, focus/back behavior, artwork fallbacks, controls, profile privacy, localization boundaries, identifier mapping, surface classes, local/server storage rules, and token handling for platform clients. `docs/api/fixtures/tv` plus `scripts/verify-tv-surface-fixtures.mjs` provide reusable feed/resolve/diagnostics golden fixtures for future platform clients.

### Phase 16c Offline Downloads Structure Update

[OFFLINE_DOWNLOADS.md](OFFLINE_DOWNLOADS.md) is the authoritative Phase 16c design document for Android/iOS mobile offline downloads. Its Task 0 research confirms these structure rules:

- `server/src/domains/downloads/` will follow the five-file domain pattern for planning, jobs, package manifests, package serving, inventory, revocation, settings, and sync.
- Offline package worker code must use durable database-backed state and bounded package work directories, with concurrency separated from live playback/transcode sessions.
- `clients/mobile/` owns the v1 download manager, protected local package storage, background transfer integration, offline playback entry points, and reconnect sync.
- Desktop, web, TV, console, and casting surfaces do not get offline-download directories or APIs in v1 beyond shared server contracts that mobile consumes.
- Package files are durable user data, not cache. Server package storage and mobile package storage must be separate from `/cache/hls`, `/cache/storyboards`, image cache, and tmpfs transcode output.

**Task 0 design status:** Phase 16c uses a manifest-backed hybrid package model. HLS/fMP4 package directories are canonical; single MP4 packages are allowed only as direct-compatible optimizations. Android uses OS-managed long-running/background download primitives plus WorkManager-style constraints; iOS uses background `URLSession` and `AVAssetDownloadURLSession` where HLS asset behavior fits. Both platforms require OS-protected app storage, backup exclusion for downloaded media, explicit Wi-Fi/cellular/low-storage controls, and reconnect-bound revocation for fully offline devices.

**Task 1 schema status:** Migration `20260701010000_create_download_domain.sql` adds `download_jobs`, `download_packages`, `download_package_files`, `download_device_state`, and `download_events`. These tables provide the durable queue, package inventory, per-file integrity manifest, per-device local state, and explicit operational event stream that `server/src/domains/downloads/` will consume in Task 2.

**Task 2 domain status:** `server/src/domains/downloads/` now exists with `mod.rs`, `handlers.rs`, `service.rs`, `error.rs`, and `types.rs`. Routes are mounted under `/api/v1/downloads/*` for planning, job creation/status/cancel, inventory, package delete, package manifest, transfer URLs, file serving, and reconnect sync. DTOs cover plan/job/inventory/manifest/transfer/sync request and response shapes. Later Phase 16c tasks are filling these route behaviors incrementally; remaining `DOWNLOAD_015` boundaries include inventory, package delete, and reconnect sync.

**Task 3 policy status:** `RuntimeConfig` now includes a `downloads: DownloadsConfig` group backed by `server_config.downloads`. `server/src/domains/downloads/service.rs` owns the initial access and policy helpers: library BOLA checks, healthy-file availability check, global enablement, LAN/remote mode restrictions, active-job quotas, retained-package quotas, retained-byte quotas, policy/quota event recording, and job/package ownership checks.

**Task 4 planning status:** `server/src/domains/downloads/service.rs` now implements `GET /api/v1/downloads/plan/{media_item_id}`. Planning selects a healthy movie/episode source file, chooses MP4 direct copy vs HLS/fMP4 remux/transcode, returns quality options and byte estimates, extracts audio/subtitle options, applies expiry/policy metadata, and emits a deterministic `plan_revision`/`plan_hash`. Job creation remains a later-task boundary.

**Task 5 manifest status:** `GET /api/v1/downloads/packages/{id}/manifest` now reads `download_packages` and `download_package_files` for owned ready/serving packages. The response is the schema-v1 package manifest with package/job/source/quality/strategy metadata, relative file entries, checksums, selected streams, artwork/storyboard metadata, expiry, sync metadata, and access-policy snapshot.

**Task 6 worker status:** `server/src/workers/download_package_worker.rs` owns durable offline package execution. The scheduler registers `download_package_worker`, migration `20260701030000_seed_download_package_worker_task.sql` seeds it for existing installs, and `seed_default_tasks` includes it for fresh installs. The worker claims queued download jobs, recovers stale preparing jobs, runs direct copy or FFmpeg HLS/fMP4 remux/transcode work under `{data_dir}/downloads/{job_id}`, writes manifest/checksum/package rows, updates job progress/status, emits events/metrics, and cleans failed/cancelled/expired package directories. The downloads service now implements job create/status/cancel endpoints.

**Task 7 serving status:** `server/src/domains/downloads/handlers.rs` now returns real package-file HTTP responses for authenticated clients. Manifest, transfer URL, and file-serving endpoints require `device_identifier` and revalidate package user/session/device bindings plus current access/policy before returning metadata or bytes. `POST /api/v1/downloads/packages/{id}/transfer-urls` returns authenticated endpoint URLs for manifest-relative files, while `GET /api/v1/downloads/packages/{id}/files/{*file_path}` serves only manifest-listed package paths under `{data_dir}/downloads/{job_id}` with single-range `206` support, private/no-store cache headers, checksum headers, and `DOWNLOAD_016` invalid-range errors.

**Task 8 notification status:** `server/src/domains/downloads/service.rs` owns `download_job_status` SSE payload publishing for job creation/cancellation, and `server/src/workers/download_package_worker.rs` publishes coalesced worker milestones for preparing/staged/ready/retry/failed states. Ready and final failed jobs call `services/notification_dispatch.rs` through seeded `download_ready` and `download_failed` notification types, so in-app unread records and opt-in mobile push are limited to actionable terminal states.

**Task 9 mobile manager status:** `clients/mobile/lib/models/download_models.dart`, `services/download_service.dart`, `stores/download_manager_store.dart`, and `screens/downloads_screen.dart` now define the mobile offline-download manager surface. The authenticated shell includes a Downloads branch, media detail can queue the current item, secure-storage metadata keeps inventory/settings scoped by server/user/device, and `AppShell` routes `download_job_status` SSE events into the manager store. Package-file storage and native transfer adapters remain Task 10.

**Task 10 protected storage status:** `clients/mobile/lib/services/protected_download_storage_service.dart` owns protected download root/package preparation, redacted metadata writes, sync-queue placeholder storage, and protected package/scope/all deletion. Android `MainActivity.kt` backs the channel with `noBackupFilesDir/duskcue_downloads`; iOS `AppDelegate.swift` backs it with Application Support plus backup exclusion and `completeUntilFirstUserAuthentication` file protection. `AuthService.clearLocalSession()` and `DownloadManagerNotifier` now purge protected local download data on logout/session clear and delete/delete-all flows.

## Development Workflow

### Server Development

```bash
# Start PostgreSQL (Docker)
docker compose up -d postgres

# Run migrations
cargo run -p server -- migrate

# Start server with hot reload
cargo watch -x 'run -p server'
```

### Web Client Development

```bash
# Start SvelteKit dev server (proxies API to running server)
cd clients/web
npm install
npm run dev
```

### Desktop Client Development

```bash
# Start Tauri + SvelteKit dev
cd clients/desktop
npm install
npm run tauri dev
```

### Full Stack Development

```bash
# Terminal 1: Database
docker compose up -d

# Terminal 2: Server
cargo watch -x 'run -p server'

# Terminal 3: Web client
cd clients/web && npm run dev
```

## Docker Build

The Dockerfile is a multi-stage build producing **multi-arch images** (x86_64 + ARM64) on **Alpine Linux**. Uses Docker cross-compilation strategy — the build stage runs on the native x86_64 runner but cross-compiles for both architectures via `cargo-zigbuild`.

```dockerfile
# syntax=docker/dockerfile:1

# Stage 1: Build (runs on native platform, cross-compiles for target)
FROM --platform=$BUILDPLATFORM rust:1.87-alpine AS builder

ARG BUILDPLATFORM
ARG TARGETPLATFORM
ARG TARGETARCH

RUN apk add --no-cache musl-dev pkgconf openssl-dev openssl-libs-static \
    && case "$TARGETARCH" in \
        amd64)  rustup target add x86_64-unknown-linux-musl ;; \
        arm64)  rustup target add aarch64-unknown-linux-musl ;; \
    esac

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY server/ server/
COPY crates/ crates/

RUN case "$TARGETARCH" in \
        amd64)  cargo build --release --target x86_64-unknown-linux-musl -p server ;; \
        arm64)  cargo build --release --target aarch64-unknown-linux-musl -p server ;; \
    esac

# Stage 2: Runtime (minimal Alpine + embedded PostgreSQL)
FROM alpine:3.22

# PostgreSQL 18 runtime + contrib extensions + privilege-drop utilities
# Strip setuid/setgid binaries (CIS Docker Benchmark 4.8)
RUN apk add --no-cache ca-certificates ffmpeg su-exec shadow \
        postgresql18 postgresql18-contrib \
    && find / -perm /6000 -type f -exec chmod a-s {} \; 2>/dev/null || true

ARG TARGETARCH
COPY --from=builder /build/target/*/release/server /usr/local/bin/duskcue
COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

ENV DUSKCUE_DATA_DIR=/data
EXPOSE 48027
HEALTHCHECK --interval=30s --timeout=10s --start-period=30s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:48027/health || exit 1
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
```

**Key additions from embedded PostgreSQL strategy:**
- `postgresql18 postgresql18-contrib` — embedded PG runtime inside the container
- `su-exec shadow` — privilege drop + user management (PUID/PGID pattern)
- `find / -perm /6000 -exec chmod a-s {} \;` — strips setuid/setgid binaries (Classifarr pattern)
- `HEALTHCHECK` baked into image — `wget` for health endpoint; `start_period: 30s` accounts for PG init + migration
- `ENTRYPOINT` points to entrypoint script — handles PG lifecycle + privilege drop

**Entrypoint script** (`docker/entrypoint.sh`):
- Creates runtime user from `PUID`/`PGID` environment variables (defaults to 1000:1000)
- Checks `DUSKCUE_DATABASE_URL`: if set, skips embedded PG (external mode); if unset, manages PG lifecycle (embedded mode)
- Embedded mode: `initdb` → `pg_ctl start` → `pg_isready` wait → `createdb` → export `DATABASE_URL`
- PG listens on Unix socket only (`/var/run/postgresql` tmpfs); trust auth; `data_checksums=on`
- Drops privileges via `su-exec` before executing the server binary
- Full entrypoint script is documented in [DOCKER_DEPLOYMENT.md](../operations/DOCKER_DEPLOYMENT.md)

**Build command (produces multi-platform manifest):**
```bash
docker buildx build --platform linux/amd64,linux/arm64 -t duskcue:latest .
```

**CI workflow (GitHub Actions):**
```yaml
- uses: docker/setup-qemu-action@v4
- uses: docker/setup-buildx-action@v4
- uses: docker/build-push-action@v6
  with:
    platforms: linux/amd64,linux/arm64
    push: true
    tags: duskcue:latest
```

Key decisions:
- **Alpine 3.22 base image** — pin minor version (not patch); auto-tracks security patches; 14 default packages; musl libc. Full rationale in [OS_HARDENING.md](../operations/OS_HARDENING.md)
- **Docker Engine >= v28.0.0 recommended** (v29.4.3+ ideal) — mitigates CVE-2026-31431 (Copy Fail). Full version matrix in [OS_HARDENING.md](../operations/OS_HARDENING.md)
- **Cross-compilation strategy** — build stage pinned to `$BUILDPLATFORM` (runs on x86_64 runner, no QEMU emulation); Rust cross-compiles to the target arch; much faster than QEMU emulation
- **musl static linking** — both architectures produce fully self-contained binaries with no runtime library dependencies
- **FFmpeg in runtime image** — required for transcoding and ffprobe; Alpine's `ffmpeg` package supports both architectures
- **No PostgreSQL in image** — database runs separately (Docker Compose sidecar or external)
- **Single Dockerfile** — one Dockerfile produces both variants via `BUILDPLATFORM`/`TARGETPLATFORM` build args
- **`cargo-zigbuild` alternative** — for CI pipelines that don't use Docker cross-comp, `cargo-zigbuild` 0.22 can cross-compile outside of Docker using Zig as the linker

## Research Sources

- Rust Cargo Workspaces — The Rust Programming Language Book: https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html
- Rust Cargo Workspaces: Organising Multi-Crate Projects (November 2025): https://medium.com/@ashusk_1790/rust-cargo-workspaces-organising-multi-crate-projects-3e67aed55b6b
- Rust for Backend Development: Complete Axum Guide 2026 (February 2026): https://rustify.rs/articles/rust-backend-development-axum-2026
- Build a Desktop App with Tauri v2 in 2026 (March 2026): https://rustify.rs/articles/rust-tauri-v2-desktop-app-tutorial-2026
- GitButler — Tauri + Svelte + Rust production reference: https://github.com/gitbutlerapp/gitbutler
- Tauri + SvelteKit community experience: https://github.com/orgs/tauri-apps/discussions/6423
- Svelte Best Practices (official): https://svelte.dev/docs/svelte/best-practices
- Rust Production Habits: The 12 Core Configurations (March 2026): https://medium.com/@monikasinghall713/rust-production-habits-the-12-core-configurations-i-always-start-with-4d4ebac81eda
