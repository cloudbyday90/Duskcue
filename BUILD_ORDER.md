# Build Order

## Purpose

This document defines the implementation sequence for Duskcue. Each phase is dependency-ordered — you cannot build a later phase without its prerequisites. Each phase references its authoritative design document(s) and lists the specific guidelines that apply.

**This is the single context document for building.** Open the referenced MDs as needed for each phase.

## Always-Applicable Documents

These documents apply to every phase. Consult them when making implementation decisions:

| Document | Purpose |
|---|---|
| [PROJECT.md](PROJECT.md) | Architecture overview, tech stack, key decisions, domain table |
| [PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md) | Monorepo layout, Cargo workspace, domain module five-file pattern, SvelteKit conventions |
| [ERROR_HANDLING.md](docs/design/ERROR_HANDLING.md) | `thiserror` v2 + `anyhow` v1, RFC 9457, error code registry, environment-aware responses |
| [API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md) | REST endpoint naming, URI versioning, pagination, rate limiting tiers, auth headers |
| [API_SECURITY.md](docs/security/API_SECURITY.md) | Input validation (`validator` 0.20), BOLA prevention, three-type DTO pattern, SSRF allowlisting, outbound validation |
| [SECURITY.md](docs/security/SECURITY.md) | Three-tier network model, rustls, HMAC signing, FFmpeg sandboxing |
| [CONFIGURATION.md](docs/operations/CONFIGURATION.md) | Two-tier config (bootstrap TOML + runtime DB), 14-step startup sequence |
| [DATABASE.md](docs/design/DATABASE.md) | Full DDL, UUIDv7 key strategy, naming conventions, PG18 features |

### Code Standards

- **ES Modules** — All JavaScript/TypeScript uses `import`/`export`, never `require`/`module.exports` ([PROJECT.md](PROJECT.md))
- **No comments in code** — unless explicitly requested
- **Product naming** — `Duskcue` (prose), `duskcue` (binary/CLI/Docker/DB/Rust modules), `DUSKCUE_` (env vars) ([PROJECT.md](PROJECT.md))
- **Server port** — `48027` ([PROJECT.md](PROJECT.md))
- **Rust edition** — 2024, resolver 3 ([PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md))
- **Domain five-file pattern** — `mod.rs`, `handlers.rs`, `service.rs`, `error.rs`, `types.rs` ([PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md))
- **Three-type DTO** — `XxxRow` (no Serialize), `XxxRequest` (Deserialize + Validate), `XxxResponse` (Serialize only) ([API_SECURITY.md](docs/security/API_SECURITY.md))
- **Handler → Service → DB** — handlers are thin HTTP translation; business logic in service; SQL in service or db/models ([PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md))

---

## Phase 1 — Project Scaffolding

**Goal:** Working Cargo workspace, SvelteKit project, and Docker skeleton. Server compiles and responds to `/health`.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md) | Full monorepo tree — Cargo workspace root, `server/`, `crates/types/`, `crates/db/`, `clients/web/`, `clients/desktop/`, `clients/mobile/` |
| [CONFIGURATION.md](docs/operations/CONFIGURATION.md) | Bootstrap config struct (`TOML` + `ENV` + `CLI`), `AppState` construction |
| [DATABASE.md](docs/design/DATABASE.md) | `sqlx.toml` configuration for sqlx-cli 0.9 |
| [MIGRATION_STRATEGY.md](docs/design/MIGRATION_STRATEGY.md) | sqlx-cli setup, timestamp-based migration naming |

**Tasks:**

1. Initialize Cargo workspace with `resolver = "3"`, workspace members per PROJECT_STRUCTURE.md
2. Create `server/` crate with `main.rs`, `lib.rs`, `config.rs`, `state.rs`, `error.rs`, `router.rs`, `middleware.rs`, `extractors.rs`
3. Create `crates/types/` — shared types with `serde`, `uuid`, `chrono` only (zero DB dependencies)
4. Create `crates/db/` — shared DB types with sqlx dependency
5. Create `clients/web/` — SvelteKit project with `"type": "module"`, `vite.config.js`, `svelte.config.js`
6. Create `clients/desktop/` — Tauri 2 wrapper with `src-tauri/` pointing to web client
7. Create `clients/mobile/` — Flutter project skeleton
8. Create `docker/entrypoint.sh` — PUID/PGID user creation, privilege drop
9. Create `Dockerfile` — multi-stage Alpine build per DOCKER_DEPLOYMENT.md
10. Wire up `/health` endpoint — returns `{"status": "ok"}` on port 48027
11. Create `server/migrations/` directory with sqlx-cli config

**Verification:** `cargo build` succeeds, `cargo test` passes, `clients/web/` runs `npm run dev`, `/health` returns 200.

---

## Phase 2 — Database Schema

**Goal:** All migrations applied. PostgreSQL tables exist for every domain.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [DATABASE.md](docs/design/DATABASE.md) | **Primary** — Full DDL for all tables, indexes, constraints, triggers |
| [MIGRATION_STRATEGY.md](docs/design/MIGRATION_STRATEGY.md) | Migration naming convention, idempotency rules, append-only policy |
| [DATABASE_MAINTENANCE.md](docs/operations/DATABASE_MAINTENANCE.md) | Per-table autovacuum tuning, `fillfactor=85` on `user_item_data` |

**Tasks:**

1. Create migration files in timestamp order per DATABASE.md table groups:
   - `20260530_030000_create_core_media_tables.sql` — `libraries`, `media_items`, `movies`, `series`, `seasons`, `episodes`, `media_files`, `subtitle_files`, `artwork`, `genres`, `tags`, `people`, `credits`
   - `20260530_030100_create_trakt_integration.sql` — `trakt_accounts`, `trakt_sync_state`
   - `20260530_030200_create_activity_analytics.sql` — `play_sessions`, `play_session_streams`, `play_events`, `user_trust_events`, `user_trust_scores`
   - `20260530_030300_create_playback_domain.sql` — `user_item_data`, `bookmarks`, `playlists`, `playlist_items`
   - `20260530_040000_create_auth_domain.sql` — `users`, `user_passkeys`, `user_totp`, `user_capabilities`, `user_library_access`, `user_sessions`, `api_keys`, `invitations`, `device_linking_codes`, `reauth_codes`, `streaming_policies`
   - `20260530_050000_create_system_domain.sql` — `server_config`, `scheduled_tasks`, `scheduled_task_runs`, `notification_types`, `notifications`, `user_notification_preferences`
   - `20260530_060000_create_cross_cutting_concerns.sql` — soft delete columns, partitioning
   - `20260530_060100_create_audit_triggers.sql` — audit log trigger functions
   - `20260530_060200_create_full_text_search.sql` — search_vector triggers, GIN indexes
   - `20260530_070000_seed_default_data.sql` — default `server_config` row, notification types, streaming policies, scheduled tasks
2. Add migration for analytics security tables: `user_location_history`
3. Add migration for migration domain tables: `migration_sources`, `migration_user_mapping`, `migration_import_log`
4. Add migration for quality domain tables: `device_profiles`, `device_capability_tests`, `client_network_reports`, `qoe_reports`
5. Add migration for overlay/collection tables: `overlay_definitions`, `artwork_overlay_state`, `collections`, `collection_items`, `collection_templates`
6. Add migration for segment/storyboard tables: `media_segments`, `media_fingerprints`, `storyboards`
7. Apply per-table autovacuum tuning from DATABASE_MAINTENANCE.md
8. Set `fillfactor=85` on `user_item_data`

**Verification:** `cargo sqlx migrate run` succeeds. All tables exist. `server_config` has single default row.

---

## Phase 3 — Core Server Infrastructure

**Goal:** Server boots, connects to PostgreSQL, runs migrations, serves API with middleware stack.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [CONFIGURATION.md](docs/operations/CONFIGURATION.md) | 14-step startup sequence, `AppState` construction, bootstrap config |
| [MEMORY.md](docs/design/MEMORY.md) | Tokio runtime config, graceful shutdown (CancellationToken + TaskTracker), startup lockfile, PG settings validation |
| [LOGGING_OBSERVABILITY.md](docs/operations/LOGGING_OBSERVABILITY.md) | `tracing` ecosystem setup, `tower-http` TraceLayer, Prometheus `/metrics` endpoint |
| [ERROR_HANDLING.md](docs/design/ERROR_HANDLING.md) | `AppError` + `IntoResponse`, RFC 9457 Problem Details |
| [API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md) | Router assembly, CORS, rate limiting (`governor`), pagination extractors |
| [SECURITY.md](docs/security/SECURITY.md) | Security headers as Tower middleware (HSTS, CSP, X-Frame-Options) |

**Tasks:**

1. Implement `config.rs` — parse bootstrap TOML + ENV + CLI via `config-rs` + `clap`
2. Implement `state.rs` — `AppState` with `PgPool`, rate limit state, provider registry, config handles
3. Implement `error.rs` — unified `AppError` enum with RFC 9457 `IntoResponse`
4. Implement `middleware.rs` — Tower stack: logging, CORS, rate limiting, security headers, compression
5. Implement `extractors.rs` — `AuthenticatedUser`, `PaginationParams`, `AdminOnly`
6. Implement `router.rs` — top-level router assembly merging all domain routers
7. Implement `main.rs` — 14-step startup sequence from CONFIGURATION.md:
   - Parse CLI → load config → acquire lockfile → connect DB → validate PG settings → run migrations → load `server_config` → check auth state → start scheduled tasks → start HTTP server → ready
8. Implement graceful shutdown per MEMORY.md:
   - SIGINT + SIGTERM handling via CancellationToken
   - Double-signal protection (`std::process::exit(1)`)
   - 3-phase: Signal → Drain 30s → Cleanup 90s
   - PG Fast mode checkpoint
9. Wire up `tracing` subscriber — pretty console + JSON file via `tracing-appender`
10. Wire up Prometheus `/metrics` endpoint
11. Implement startup lockfile at `/data/.duskcue.lock`
12. Implement PG settings validation (fsync, data_checksums, wal_level — warn only)

**Verification:** Server boots, connects to PG, runs migrations, `/health` returns 200, `/metrics` returns Prometheus format, SIGTERM triggers graceful shutdown with PG checkpoint.

---

## Phase 4 — Authentication & Users

**Goal:** Users can register, log in with passkeys, manage sessions, and have capability-based access.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [AUTH.md](docs/design/AUTH.md) | **Primary** — passkey-first (WebAuthn/FIDO2), capability-based access control, invite codes, device linking (RFC 8628), re-auth codes |
| [API_SECURITY.md](docs/security/API_SECURITY.md) | Session cookie (HttpOnly, SameSite=Strict) + Bearer token dual auth |
| [API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md) | Auth endpoint patterns, rate limiting tiers (5 tiers) |

**Tasks:**

1. Create `server/src/domains/auth/` — five-file pattern
2. Implement WebAuthn registration and authentication flows
3. Implement invite code system — admin creates invite, user registers with code
4. Implement `user_sessions` — session creation, validation, revocation
5. Implement `user_capabilities` — capability-based access control checks
6. Implement device linking — RFC 8628 device code flow
7. Implement re-auth codes for sensitive operations
8. Create `server/src/domains/users/` — five-file pattern
9. Implement user CRUD — list, get, update, soft-delete
10. Implement `AuthenticatedUser` extractor — validates session from cookie or Bearer token
11. Implement `require_capability()` middleware for admin endpoints

**Verification:** Admin creates invite code, new user registers with passkey, user session is created, authenticated requests succeed, unauthorized requests return 401, admin-only endpoints require `can_manage_server`.

---

## Phase 5 — Libraries & Media Items

**Goal:** Admin can create libraries, scan directories, and media items appear in the database.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [LIBRARY_ORGANIZATION.md](docs/design/LIBRARY_ORGANIZATION.md) | **Primary** — folder structure, sub-folder-as-library, multi-path libraries, metadata ID tags |
| [MEDIA_SCANNING.md](docs/design/MEDIA_SCANNING.md) | **Primary** — 6-phase pipeline (discover → diff → probe → identify → enrich → cleanup), FS watching (`notify`), mtime diff, Blake3 hash, ffprobe |
| [DATABASE.md](docs/design/DATABASE.md) | `libraries`, `library_paths`, `media_items`, `movies`, `series`, `seasons`, `episodes`, `media_files` tables |

**Tasks:**

1. Create `server/src/domains/libraries/` — five-file pattern
2. Implement library CRUD — create, list, get, update, soft-delete
3. Implement `library_paths` — multi-path library support
4. Create `server/src/domains/media/` — five-file pattern
5. Implement `server/src/workers/library_scanner.rs`:
   - Phase 1: Discover — walk filesystem using `ignore` (ripgrep) crate
   - Phase 2: Diff — mtime-based change detection with 2s tolerance
   - Phase 3: Probe — ffprobe concurrent queue for codec/resolution/duration
   - Phase 4: Identify — 5-layer cascading pipeline from LIBRARY_ORGANIZATION.md
   - Phase 5: Enrich — stub (metadata provider calls added in Phase 8)
   - Phase 6: Cleanup — remove orphaned items
6. Implement `server/src/services/scheduler.rs` — scheduled task runner
7. Implement FS watching via `notify` + `notify-debouncer-full` for real-time detection
8. Implement `.media-match` sidecar file parsing (Layer 1 of identification)
9. Implement NFO file parsing (Layer 2)
10. Implement provider ID tag parsing `{tmdb-XXX}`, `{imdb-ttXXX}`, `{tvdb-XXX}` (Layer 3)

**Verification:** Admin creates a library pointing to a media directory, triggers scan, media items appear in DB with correct file paths, codecs, and resolutions. FS watching detects new files in real-time.

---

## Phase 6 — Metadata Providers

**Goal:** TMDB enrichment populates titles, overviews, artwork, cast/crew, and external IDs for all media items.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [METADATA_PROVIDERS.md](docs/design/METADATA_PROVIDERS.md) | **Primary** — provider profiles, trait-based abstraction (`MetadataProvider`, `ArtworkProvider`, `RatingsProvider`), `ProviderRegistry`, `EnrichmentOrchestrator`, rate limiters, API key encryption |
| [POSTER_MANAGEMENT.md](docs/design/POSTER_MANAGEMENT.md) | Artwork download, storage layout, `MetadataConfig` Rust struct |
| [API_SECURITY.md](docs/security/API_SECURITY.md) | SSRF allowlist (provider domains), outbound response validation |

**Tasks:**

1. Create `server/src/services/metadata.rs` — `ProviderRegistry`, `EnrichmentOrchestrator`
2. Implement `TmdbClient` — Bearer token auth, `append_to_response` batching, rate limiter (governor, 40 req/s)
3. Implement TMDB search endpoints — `/search/movie`, `/search/tv`
4. Implement TMDB details endpoints — `/movie/{id}`, `/tv/{id}` with `append_to_response=credits,videos,external_ids,images`
5. Implement TMDB `/find` — cross-reference from IMDb ID
6. Implement TMDB `/configuration` caching — image sizes, base URL
7. Wire TMDB client into Phase 5 enrichment (Phase 5 stub → real implementation)
8. Implement artwork download — save to `/data/metadata/artwork/`, create `artwork` table rows
9. Implement `TvdbClient` — JWT auth via `/login`, token refresh, series/episode endpoints
10. Implement `FanartClient` — artwork lookup by TMDB/TVDB ID
11. Implement `OmdbClient` — ratings lookup by IMDb ID
12. Implement provider API key validation on save (test request)
13. Implement API key encryption at rest (AES-256-GCM with `encrypted:` prefix)
14. Implement TMDB daily ID export download and caching
15. Implement `server/src/workers/metadata_refresh.rs` — periodic enrichment using TMDB `/changes`

**Verification:** Library scan enriches items with TMDB data — titles, overviews, ratings, genres, cast, artwork. Admin can configure TVDB/Fanart.tv/OMDb keys in settings UI. Provider failures are non-blocking.

---

## Phase 7 — Streaming & Playback

**Goal:** Users can stream media with HLS. Transcoding works for incompatible formats.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [STREAMING.md](docs/design/STREAMING.md) | **Primary** — HLS/fMP4, three-tier decision flow, FFmpeg pipeline, HW accel, ABR ladder, streaming policies, segment skip endpoints |
| [QUALITY_MANAGEMENT.md](docs/design/QUALITY_MANAGEMENT.md) | Device capability profiles, network assessment, transcoding decision engine, quality modes (Auto/Maximum/Manual) |
| [VIDEO_FORMATS.md](docs/design/VIDEO_FORMATS.md) | Supported codecs, containers, HDR, transcode targets |
| [AUDIO_FORMATS.md](docs/design/AUDIO_FORMATS.md) | Audio codecs, channels, spatial audio, passthrough rules |
| [CPU.md](docs/design/CPU.md) | FFmpeg threading, process priority (`nice`/`ionice`), HW accel detection |
| [MEMORY.md](docs/design/MEMORY.md) | FFmpeg subprocess via `tokio-process-tools` v0.11.2, `-progress pipe:1` structured output |
| [SECURITY.md](docs/security/SECURITY.md) | FFmpeg per-process sandboxing — Landlock + seccompiler |

**Tasks:**

1. Create `server/src/domains/playback/` — five-file pattern
2. Implement `server/src/services/transcoding.rs`:
   - FFmpeg subprocess management via `tokio-process-tools`
   - Structured progress parsing via `-progress pipe:1`
   - HLS/fMP4 segment generation (6-second duration)
   - ABR ladder: 480p/1.5Mbps, 720p/3Mbps, 1080p/6Mbps, 1080p HQ/10Mbps
   - Three-tier decision: Direct Play → Remux → Transcode
3. Implement `server/src/services/sandbox.rs`:
   - Landlock filesystem isolation (Linux 5.13+)
   - seccomp-BPF syscall filtering via `seccompiler`
   - Graceful degradation on unsupported platforms
4. Create `server/src/domains/quality/` — five-file pattern
5. Implement device capability detection — runtime probe
6. Implement network quality assessment — segment download telemetry
7. Implement transcoding decision engine — 10-factor evaluation from QUALITY_MANAGEMENT.md
8. Implement streaming policy system — `streaming_policies` table with per-user overrides
9. Implement HLS manifest generation and segment serving
10. Implement direct play / remux for compatible formats (no transcode)
11. Implement HW accel runtime detection — NVIDIA, VAAPI, VideoToolbox, AMF
12. Implement play session tracking — create `play_sessions` rows, heartbeat updates
13. Implement `user_item_data` — watch state, resume position, play count

**Verification:** User clicks play on a movie, HLS stream starts, segments are served, play session is tracked, resume position updates. Transcoding activates for incompatible formats. HW acceleration detected and used when available.

---

## Phase 8 — Web Client Core

**Goal:** Functional web UI for browsing libraries, playing media, and basic settings.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md) | SvelteKit routes, API client layer pattern, stores, components |
| [UI_FOUNDATIONS.md](docs/branding/UI_FOUNDATIONS.md) | Visual direction, navigation language, core reusable surfaces |
| [NAME_BRANDING.md](docs/branding/NAME_BRANDING.md) | Product identity, logo usage |
| [API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md) | API client layer — `core.js` fetch wrapper, per-domain modules |

**Tasks:**

1. Build `clients/web/src/lib/api/core.js` — HTTP client with session cookie handling, error parsing (RFC 9457)
2. Build API client modules per domain — `auth.js`, `users.js`, `libraries.js`, `media.js`, `playback.js`, `settings.js`, `search.js`
3. Build Svelte stores — `auth.js`, `user.js`, `libraries.js`, `player.js`, `notifications.js`
4. Build core components — `MediaCard.svelte`, `Player.svelte` (hls.js integration), `SearchBar.svelte`, `NotificationToast.svelte`
5. Build route pages:
   - Auth: login, setup, device linking
   - Dashboard: home screen with recently added, continue watching
   - Libraries: library list, library detail (grid of media items)
   - Media: item detail page with metadata, cast, play button
   - Player: full-screen HLS player with quality selector
   - Search: search results page
   - Settings: server overview, users, libraries
6. Implement responsive layout — desktop and mobile breakpoints

**Verification:** User can log in, browse libraries, search for items, view metadata, and play media through the web client.

---

## Phase 9 — Subtitles

**Goal:** Subtitle discovery, delivery, and auto-fetch from external providers.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [SUBTITLES.md](docs/design/SUBTITLES.md) | **Primary** — subtitle discovery, conversion, sync correction, fetching, delivery |
| [METADATA_PROVIDERS.md](docs/design/METADATA_PROVIDERS.md) | SubDL and OpenSubtitles provider profiles, rate limiting |

**Tasks:**

1. Create `server/src/domains/subtitles/` — five-file pattern
2. Implement subtitle discovery — scan for SRT/ASS/VTT/PGS/VobSub sidecars alongside media files
3. Implement `subtitle_files` rows — populate during library scan (Phase 5)
4. Implement subtitle delivery — serve WebVTT for HLS streams, serve text-based subtitles directly
5. Implement `server/src/services/subtitles.rs`:
   - SRT ↔ ASS ↔ WebVTT format conversion
   - FPS adjustment (23.976 ↔ 24 ↔ 25 ↔ 29.97)
   - Offset correction (user-applied timestamp shift)
   - PGS/VobSub OCR stub (PaddleOCR — one-time background task)
6. Implement subtitle fetching from providers:
   - SubDL client — search by TMDB ID, download, save
   - OpenSubtitles client — search by hash/filename, download, save
   - Provider priority: SubDL first, OpenSubtitles fallback
7. Implement `server/src/workers/subtitle_processor.rs` — auto-fetch during scan
8. Implement subtitle settings UI in web client

**Verification:** Media items show available subtitles. User can select subtitle during playback. Auto-fetch downloads missing subtitles during scan. SubDL returns results by TMDB ID.

---

## Phase 10 — Segment Detection & Storyboards

**Goal:** Intro/credit skip markers and seek preview thumbnails.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [SEGMENT_DETECTION.md](docs/design/SEGMENT_DETECTION.md) | **Primary** — 4-method pipeline (chapter markers → chromaprint → black frame → silence), skip buttons |
| [STORYBOARDS.md](docs/design/STORYBOARDS.md) | **Primary** — WebVTT + WebP spritesheets, keyframe-only mode, adaptive interval |

**Tasks:**

1. Create `server/src/domains/segments/` — five-file pattern
2. Implement `server/src/services/segments.rs`:
   - Chapter marker extraction from container metadata
   - Chromaprint fingerprinting for intro detection
   - Black frame detection via FFmpeg
   - Silence detection via FFmpeg
   - Confidence scoring and 2s padding
3. Create `server/src/domains/storyboards/` — five-file pattern
4. Implement `server/src/services/storyboards.rs`:
   - FFmpeg thumbnail extraction at adaptive intervals
   - WebP spritesheet generation
   - WebVTT seek file generation
5. Implement `server/src/workers/segment_detector.rs` — background segment detection
6. Implement `server/src/workers/storyboard_generator.rs` — background thumbnail generation
7. Implement skip button in web client player — `SkipButton.svelte`
8. Implement seek preview in web client player — `SeekPreview.svelte`

**Verification:** After detection runs, media items have intro/credit markers. Skip button appears during intros in player. Seek bar shows thumbnail previews.

---

## Phase 11 — Analytics & Trakt Integration

**Goal:** Activity tracking, analytics dashboard, and Trakt.tv sync.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [DATABASE.md](docs/design/DATABASE.md) | `play_sessions`, `play_events`, `user_trust_events`, `user_trust_scores`, `trakt_accounts`, `trakt_sync_state` |
| [ANALYTICS_SECURITY.md](docs/security/ANALYTICS_SECURITY.md) | Impossible travel detection, GeoIP (MaxMind GeoLite2), 5-layer false positive suppression |
| [AUTH.md](docs/design/AUTH.md) | Trakt.tv account linking flow |

**Tasks:**

1. Create `server/src/domains/analytics/` — five-file pattern
2. Implement analytics dashboard — play history, top media, concurrent streams, bandwidth usage
3. Implement `server/src/domains/trakt/` — five-file pattern
4. Implement Trakt OAuth flow — account linking, token refresh
5. Implement Trakt sync — watch state push/pull, play count sync
6. Implement `server/src/workers/trakt_sync.rs` — periodic sync scheduled task
7. Implement `server/src/services/geoip.rs`:
   - MaxMind GeoLite2 City MMDB loading with `maxminddb` crate (mmap)
   - `ArcSwap` hot-reload on weekly update
   - Graceful degradation when MMDB absent
8. Implement impossible travel detection:
   - Haversine distance + 1,000 km/h threshold
   - 5-layer false positive suppression
   - Notification-first response (admin dashboard alert, no auto-blocking)
9. Implement `server/src/workers/geoip_updater.rs` — weekly MMDB download

**Verification:** Play sessions generate analytics data visible in dashboard. Trakt-linked users sync watch state. Impossible travel alerts appear in admin dashboard for suspicious logins.

---

## Phase 12 — Kometa-Like System (Overlays, Collections, Posters)

**Goal:** Overlay compositing engine, dynamic collections, and multi-source poster management.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [METADATA_OVERLAYS.md](docs/design/METADATA_OVERLAYS.md) | **Primary** — overlay types (image/text/backdrop), canvas standards, groups, queues, conditions, compositing pipeline (pure Rust: `image` + `ab_glyph` + `resvg`) |
| [COLLECTIONS.md](docs/design/COLLECTIONS.md) | **Primary** — three collection types (static/dynamic/smart), 14 internal + 13 external builders, templates |
| [POSTER_MANAGEMENT.md](docs/design/POSTER_MANAGEMENT.md) | **Primary** — five artwork sources, selection priority, poster locking, asset directory, community packs |

**Tasks:**

1. Create `server/src/domains/overlays/` — five-file pattern
2. Implement `server/src/services/overlays.rs`:
   - Compositing pipeline using `image` + `ab_glyph` + `resvg`
   - Image overlay (alpha blending)
   - Text overlay (with special variables: resolution, ratings, codecs)
   - Backdrop overlay
   - Group mutual exclusion, queue auto-stacking
3. Implement condition evaluation — JSONB filter rules against `media_items`/`media_files`
4. Implement clean art preservation — source artwork never modified
5. Create `server/src/domains/collections/` — five-file pattern
6. Implement collection builders:
   - Internal: genre, decade, actor, director, franchise, resolution, audio_codec
   - External: `tmdb_popular`, `tmdb_top_rated`, `tmdb_trending`, `tmdb_now_playing`, `tmdb_upcoming`
7. Implement `server/src/workers/collection_sync.rs` — periodic builder execution
8. Implement `server/src/workers/overlay_compositor.rs` — apply overlays to artwork
9. Implement poster management — asset directory scanning, poster locking, community pack import
10. Build admin UI for overlays — overlay editor, template browser, condition builder
11. Build admin UI for collections — collection list, builder configuration, template import

**Verification:** Default overlays (resolution badge, audio codec) are applied to poster artwork. Dynamic collections auto-populate from TMDB popular/trending. Admin can create custom overlays and collections. Source artwork is preserved.

---

## Phase 13 — System Operations

**Goal:** Backup system, scheduled task management, system settings, notifications.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [BACKUP_RECOVERY.md](docs/operations/BACKUP_RECOVERY.md) | WAL-G continuous archiving, pg_dump logical backups, AES-256-GCM encryption, 3-2-1 storage |
| [CACHE_STORAGE.md](docs/operations/CACHE_STORAGE.md) | Three-tier storage, per-cache-type limits, LRU eviction, disk space monitoring |
| [DATABASE_MAINTENANCE.md](docs/operations/DATABASE_MAINTENANCE.md) | REINDEX CONCURRENTLY task, partition ANALYZE, `pgstattuple` bloat measurement |

**Tasks:**

1. Create `server/src/domains/system/` — five-file pattern
2. Implement `server_config` runtime API — get/update JSONB config fields
3. Implement scheduled task management — list, trigger, cancel, view history
4. Implement notification system — notification types, user preferences, dispatch
5. Implement `server/src/domains/backup/` — five-file pattern
6. Implement backup coordination — WAL-G status check, pg_dump trigger, verification
7. Implement `server/src/workers/backup_runner.rs` — scheduled backup execution
8. Implement `server/src/workers/reindex_maintenance.rs` — weekly REINDEX CONCURRENTLY
9. Implement `server/src/workers/disk_space_check.rs` — 30-minute disk monitoring
10. Build admin settings UI — all `server_config` JSONB fields as toggles, sliders, dropdowns
11. Build notifications UI — notification center, preferences

**Verification:** Admin can configure all settings via UI. Backups run on schedule. Disk space alerts trigger when thresholds are exceeded.

---

## Phase 14 — Platform Migration

**Goal:** Import watch history from Plex, Jellyfin, and Emby.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [MIGRATIONS.md](docs/design/MIGRATIONS.md) | **Primary** — three source platforms, user mapping via invite code display names, provider ID matching, merge strategy |

**Tasks:**

1. Create `server/src/domains/migration/` — five-file pattern
2. Implement Jellyfin/Emby migration — REST API connection, user mapping, watch state import
3. Implement Plex migration — SQLite DB upload, `com.plexapp.plugins.library.db` parsing via `rusqlite`
4. Implement user mapping — invite code `display_name` field links source users to platform users
5. Implement provider ID matching — TMDb/IMDb/TVDB ID cross-reference, title+year+type fallback
6. Implement merge strategy — `is_watched` OR, `play_count` MAX, `resume_position_ms` MAX
7. Build migration wizard UI — step-by-step admin flow

**Verification:** Admin can import watch history from Jellyfin via REST API and Plex via SQLite upload. Watch states appear correctly in `user_item_data`.

---

## Phase 15 — Docker & Deployment

**Goal:** Production-ready Docker image with embedded PostgreSQL.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [DOCKER_DEPLOYMENT.md](docs/operations/DOCKER_DEPLOYMENT.md) | **Primary** — hybrid embedded/external PG, volume strategy, security hardening |
| [OS_HARDENING.md](docs/operations/OS_HARDENING.md) | Docker Engine version minimums, Alpine 3.22 pinning |
| [CACHE_STORAGE.md](docs/operations/CACHE_STORAGE.md) | Docker volumes: `duskcue-data`, `duskcue-cache`, tmpfs for transcode |

**Tasks:**

1. Finalize `Dockerfile` — multi-stage Alpine build for x86_64 + ARM64
2. Finalize `docker/entrypoint.sh`:
   - Embedded PG mode: `initdb` → `pg_ctl start` → `pg_isready` → `createdb` → start server
   - External PG mode: skip PG lifecycle, use `DUSKCUE_DATABASE_URL`
   - PUID/PGID user creation and privilege drop via `su-exec`
3. Create `docker-compose.yml` — single-container with embedded PG, volumes, tmpfs
4. Test multi-arch build: `docker buildx build --platform linux/amd64,linux/arm64`
5. Test PUID/PGID mapping on Linux
6. Test embedded PG lifecycle — startup, shutdown checkpoint, crash recovery
7. Verify security: `read_only: true`, `no-new-privileges`, `cap_drop: ALL`

**Verification:** `docker compose up` starts a single container with embedded PG, server listens on 48027, health check passes, graceful shutdown preserves data.

---

## Phase 16 — Desktop & Mobile Clients

**Goal:** Tauri desktop app and Flutter mobile app connecting to the server.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md) | Tauri 2 desktop wrapper (imports web client), Flutter mobile project structure |
| [QUALITY_MANAGEMENT.md](docs/design/QUALITY_MANAGEMENT.md) | Device capability profiles, network quality assessment |

**Tasks:**

1. Wire Tauri desktop app — `clients/desktop/src-tauri/` wrapping web client
2. Implement Tauri-specific features — system tray, file dialogs, deeplinks
3. Build Flutter mobile client:
   - API client layer
   - Auth flow (passkey support)
   - Library browsing
   - HLS player integration
   - Settings screens
4. Implement mobile-specific quality management — cellular vs WiFi detection, adaptive streaming

**Verification:** Tauri app launches with web client UI. Flutter app connects to server, authenticates, browses library, plays media.

---

## Dependency Graph

```
Phase 1: Scaffolding
    ↓
Phase 2: Database Schema
    ↓
Phase 3: Core Server Infrastructure
    ↓
Phase 4: Auth & Users
    ↓
Phase 5: Libraries & Media ──────────────────────────────┐
    ↓                                                      │
Phase 6: Metadata Providers ←─── (enriches Phase 5)       │
    ↓                                                      │
Phase 7: Streaming & Playback                             │
    ↓                                                      │
Phase 8: Web Client Core ←─── (consumes all above) ←──────┘
    ↓
    ├── Phase 9:  Subtitles
    ├── Phase 10: Segments & Storyboards
    ├── Phase 11: Analytics & Trakt
    ├── Phase 12: Kometa-Like System
    ├── Phase 13: System Operations
    └── Phase 14: Platform Migration
    ↓
Phase 15: Docker & Deployment
    ↓
Phase 16: Desktop & Mobile Clients
```

Phases 9–14 can be built in any order after Phase 8, since they are independent domains that each add functionality on top of the core.
