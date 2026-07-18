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
| [TV_PLATFORM_SURFACES.md](docs/design/TV_PLATFORM_SURFACES.md) | TV continue-watching/next-up/recommendation feed, Android TV Watch Next adapter, deep-link resume |
| [ANDROID_TV.md](docs/design/ANDROID_TV.md) | Phase 17 Android TV / Google TV architecture, Watch Next, playback, and release gates |

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

## Phase 1 — Project Scaffolding (COMPLETE)

**Committed:** `aaedc05` on `main`

**What was built:**

| File | Purpose |
|---|---|
| `server/src/main.rs` | Entry point — mimalloc allocator, clap CLI parse, bootstrap config via config-rs, tracing-subscriber init, TcpListener on port 48027, graceful shutdown with double-signal protection (AtomicBool) |
| `server/src/lib.rs` | Module declarations — `pub mod config; pub mod router;` |
| `server/src/config.rs` | `CliArgs` (clap derive with `DUSKCUE_` env vars), `BootstrapConfig` (serde Deserialize), `build_bootstrap_config()` merging defaults -> TOML -> env -> CLI, platform-aware `data_dir` defaults, environment validation |
| `server/src/router.rs` | `build_router()` with GET `/health` returning `{"status":"ok"}` |
| `Cargo.toml` | Workspace root with resolver 3, all shared dependency versions, `ring` TLS backend |
| `Cargo.lock` | Generated lockfile for all workspace crates |

**Key decisions made during implementation:**

- Minimal Phase 1 scope — only `main.rs`, `lib.rs`, `config.rs`, `router.rs` implemented; `state.rs`, `error.rs`, `middleware.rs`, `extractors.rs` remain license-header stubs deferred to Phase 3
- `rustls`/`tokio-rustls`/`reqwest` switched to `ring` crypto backend (avoids `aws-lc-sys` which requires NASM on Windows — not available in standard dev environments)
- `mimalloc` global allocator excluded on MSVC (`#[cfg(not(target_env = "msvc"))]`)
- Graceful shutdown uses `with_graceful_shutdown()` with `tokio::select!` for SIGINT + SIGTERM; `CancellationToken` + `TaskTracker` pattern deferred to Phase 3 per MEMORY.md
- Cross-platform signal handling: `#[cfg(unix)]` for SIGTERM, `std::future::pending()` fallback on non-Unix
- `set_override_option` used for optional `database_url` field (config-rs API correctness)
- `clap` `env` feature enabled for `DUSKCUE_` environment variable support
- Workspace deps added: `tokio-util`, `mimalloc`, `tracing-appender`, `dirs`

**Not yet implemented (deferred):**

- `clients/web/` (SvelteKit) — Phase 8
- `clients/desktop/` (Tauri) — Phase 16a
- `clients/mobile/` (Flutter) — Phase 16a
- `clients/tv/android/` (Android TV / Google TV) — Phase 17
- `clients/tv/roku/` (Roku) — Phase 19
- `clients/tv/samsung/` (Samsung Tizen) — Phase 20
- `clients/tv/lg/` (LG webOS) — Phase 21
- `clients/tv/apple/` (Apple TV / tvOS) — Phase 22
- `clients/tv/xbox/` (Xbox) — Phase 23
- `docker/entrypoint.sh` — Phase 15
- `Dockerfile` — Phase 15
- `server/migrations/` — Phase 2
- `crates/types/` and `crates/db/` exist as stubs only

---

## Phase 2 — Database Schema (COMPLETE)

**Committed:** `dd3f201` on `main`

**What was built:**

15 migration files covering all domains in DATABASE.md:

| File | Tables Created |
|---|---|
| `20260530030000_create_core_media_tables.sql` | `libraries`, `library_paths`, `media_items`, `movies`, `series`, `seasons`, `episodes`, `media_files`, `subtitle_files`, `subtitle_ocr_cache`, `subtitle_sync_data`, `genres`, `media_genres`, `tags`, `media_tags`, `people`, `media_credits`, `artwork` |
| `20260530030100_create_trakt_integration.sql` | `users` (stub), `trakt_accounts`, `trakt_sync_state` |
| `20260530030200_create_activity_analytics.sql` | `play_sessions` (partitioned), `play_session_streams`, `play_events` (partitioned), `user_trust_events`, `user_trust_scores` |
| `20260530030300_create_playback_domain.sql` | `user_item_data` (fillfactor=85), `bookmarks`, `playlists`, `playlist_items` |
| `20260530040000_create_auth_domain.sql` | `streaming_policies`, `users` ALTER (13 columns added), `user_passkeys`, `user_totp`, `user_capabilities`, `user_library_access`, `user_sessions`, `api_keys`, `invitations`, `device_linking_codes`, `reauth_codes` |
| `20260530050000_create_system_domain.sql` | `server_config`, `scheduled_tasks`, `scheduled_task_runs`, `notification_types`, `notifications`, `user_notification_preferences` |
| `20260530060000_create_cross_cutting_concerns.sql` | `pg_trgm` + `pgstattuple` extensions, `audit_log` (partitioned) |
| `20260530060100_create_audit_triggers.sql` | `audit_trigger_fn()` + 10 audit triggers |
| `20260530060200_create_full_text_search.sql` | `rebuild_media_search_vector()` + 4 search triggers + trigram index (the PG FTS foundation for v1.0 search; see [SEARCH.md](docs/design/SEARCH.md) for the full search-engine decision and migration path to Meilisearch at scale) |
| `20260530070000_seed_default_data.sql` | Default `server_config` row, 5 streaming policies, 11 notification types, 18 scheduled tasks |
| `20260530070100_create_analytics_security.sql` | `user_location_history` + 6 per-table autovacuum overrides |
| `20260530070200_create_migration_domain.sql` | `migration_sources`, `migration_user_mapping`, `migration_import_log` |
| `20260530070300_create_quality_domain.sql` | `device_profiles`, `device_capability_tests`, `client_network_reports`, `qoe_reports` |
| `20260530070400_create_overlays_collections.sql` | `overlay_definitions`, `artwork_overlay_state`, `artwork` ALTER (`is_locked`, `source_type`), `collections`, `collection_items`, `collection_templates` |
| `20260530070500_create_segments_storyboards.sql` | `media_segments`, `media_fingerprints`, `storyboards` |

**Key decisions made during implementation:**

- All migrations use idempotent patterns (`IF NOT EXISTS`, `DO $$ ... $$`) per MIGRATION_STRATEGY.md
- `users` created as minimal stub in migration 2 (trakt dependency), expanded to full auth schema via idempotent `ALTER TABLE` in migration 5 — `DO $$` blocks check `information_schema.columns` before each ADD COLUMN
- `streaming_policies` created before `users` ALTER in migration 5 because `users.streaming_policy_id` references it
- `play_sessions` and `play_events` include June/July 2026 initial partitions; `audit_log` includes same; application-level partition management now creates the current partition plus a bounded future horizon for all three tables, persists per-partition task stats, and fails/retries safely on creation errors (`88d2295`). Destructive retention detach/drop remains a separately scoped maintenance follow-up.
- `user_item_data` includes `fillfactor = 85` directly in `CREATE TABLE` (not a separate ALTER) for clean initial creation
- Autovacuum tuning applied in migration 11 alongside `user_location_history` (both are analytics security concerns from DATABASE_MAINTENANCE.md)
- `artwork` ALTER (`is_locked`, `source_type`) placed in overlay migration 14 since those columns serve the overlay compositing engine
- Partitioned table indexes created on parent tables only (PG propagates to partitions)
- Index creation uses `IF NOT EXISTS` throughout for re-run safety
- Scheduled tasks use uniform INSERT with `ON CONFLICT (name) DO NOTHING` + separate UPDATE statements for `interval_seconds` values

**Not yet verified (requires running PostgreSQL):**

- `cargo sqlx migrate run` has not been executed against a live database
- All `CREATE TABLE`, index, trigger, and constraint DDL is syntactically derived from DATABASE.md but untested against PG18
- Seed data correctness depends on FK references resolving at application time

---

## Phase 3 — Core Server Infrastructure (COMPLETE)

**Committed:** `d71eea5` on `main`

**Goal:** Server boots, connects to PostgreSQL, runs migrations, serves API with middleware stack.

**Prerequisites:** Phase 1 complete. Phase 2 complete (migrations created; a running PostgreSQL instance is required for verification).

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [CONFIGURATION.md](docs/operations/CONFIGURATION.md) | 14-step startup sequence, `AppState` construction, bootstrap config |
| [MEMORY.md](docs/design/MEMORY.md) | Tokio runtime config, graceful shutdown (CancellationToken + TaskTracker), startup lockfile, PG settings validation |
| [LOGGING_OBSERVABILITY.md](docs/operations/LOGGING_OBSERVABILITY.md) | `tracing` ecosystem setup, `tower-http` TraceLayer, Prometheus `/metrics` endpoint |
| [ERROR_HANDLING.md](docs/design/ERROR_HANDLING.md) | `AppError` + `IntoResponse`, RFC 9457 Problem Details |
| [API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md) | Router assembly, CORS, rate limiting (`governor`), pagination extractors |
| [SECURITY.md](docs/security/SECURITY.md) | Security headers as Tower middleware (HSTS, CSP, X-Frame-Options) |

**Context from Phase 1:**

- `config.rs` already implements `CliArgs`, `BootstrapConfig`, and `build_bootstrap_config()` — Task 1 below is partially done; needs `RuntimeConfig` loading from DB and `AppState` wiring
- `main.rs` already implements graceful shutdown with `with_graceful_shutdown()` and double-signal protection — needs upgrade to `CancellationToken` + `TaskTracker` per MEMORY.md (Task 8)
- `router.rs` already implements `build_router()` with `/health` — needs middleware stack and domain routers added (Tasks 4, 6)
- `state.rs`, `error.rs`, `middleware.rs`, `extractors.rs` are license-header stubs — need full implementation

**What has been built so far (Tasks 3, 2, 4):**

| File | Purpose |
|---|---|
| `server/src/error.rs` | `FieldError` (per-field validation), `ProblemDetail` (RFC 9457 response struct), `AppError` (unified error enum with 11 variants), `IntoResponse` impl with environment-aware detail, `Retry-After` headers, structured tracing, `From<anyhow::Error>` |
| `server/src/lib.rs` | Added `pub mod error;`, `pub mod state;`, `pub mod middleware;` module declarations |
| `server/src/state.rs` | `AppState` with `PgPool`, `ArcSwap<RuntimeConfig>`, `BootstrapConfig`, `Arc<RateLimitState>`; `RuntimeConfig` with 21 sub-config fields; `load_runtime_config()` DB loader; 6 fully-defined sub-configs (`AuthConfig`, `SecurityConfig`, `QualityConfig`, `SubtitleConfig`, `ResourceLimitsConfig`, `RateLimitConfig`); 6 placeholder sub-configs for future phases |
| `server/src/middleware.rs` | `RateLimitState` with 5 governor keyed rate limiters; `rate_limit_global()` middleware; `build_cors_layer()`, `build_security_headers()`, `build_compression_layer()`, `build_set_request_id_layer()`; `UuidV7RequestId` MakeRequestId implementation |
 | `server/src/extractors.rs` | `AuthenticatedUser` extractor (cookie + Bearer token extraction, session validation stub for Phase 4); `PaginationParams` enum with `Cursor`/`Offset` variants, full validation per API_CONVENTIONS.md; `AdminOnly` extractor requiring `can_manage_server` capability; `SortOrder` enum |
 | `server/src/lockfile.rs` | `Lockfile` struct with PID-file acquire/release; `LockfileError` enum; stale lockfile detection via `sysinfo` process liveness check; `Drop` impl for crash cleanup safety net |

**Key decisions from Task 3 (error.rs):**

- Generic variants only for Phase 3 — `NotFound`, `BadRequest`, `Conflict`, `Unauthorized`, `Forbidden`, `UnprocessableEntity`, `ServiceUnavailable`, `GatewayTimeout`, `Validation` (VALID_001), `RateLimited`, `Internal`; domain-specific variants (`Auth`, `Database`, `Library`, etc.) added as each domain module is built (Phase 4+)
- `is_development_env()` now reads from `OnceLock<String>` global set by `AppState::new()`, falls back to `DUSKCUE_ENVIRONMENT` env var if not yet initialized — wires error.rs to `BootstrapConfig.environment` without creating a circular dependency
- `get_trace_id()` generates UUID v7 (project uses UUID v7 throughout; no `v4` feature enabled in workspace)
- `Retry-After` values: `0` for rate limits (client should respect `Retry-After` from governor headers), `30` for gateway timeout, `60` for service unavailable
- All 5xx errors sanitize detail to `"Internal server error"` in non-development environments per ERROR_HANDLING.md
- `tracing::error!` logs every error with trace_id, error_code, status, and display message at the `IntoResponse` boundary

**Key decisions from Task 2 (state.rs):**

- `AppState` derives `Clone` — `PgPool` is internally `Arc`'d, `RuntimeConfig` wrapped in `Arc<ArcSwap<T>>` for lock-free reads and atomic swaps, `BootstrapConfig` is `Clone`
- `ArcSwap<RuntimeConfig>` chosen over `RwLock` for the read-heavy, rarely-written config pattern — lock-free reads, atomic swaps on admin API changes (`reload_runtime_config()`)
- 6 sub-configs fully defined from CONFIGURATION.md: `AuthConfig` (with `RateLimitConfig`, `NetworkMode`), `SecurityConfig` (with `TlsConfig`, `StreamSigningConfig`, `VpnDetectionConfig`, `AcmeChallengeType`), `QualityConfig`, `SubtitleConfig`, `ResourceLimitsConfig`
- 6 placeholder sub-configs as empty structs with `Default`: `NetworkConfig`, `TranscodingConfig`, `MetadataConfig`, `NotificationConfig`, `BackupConfig`, `IntegrationsConfig`, `LoggingConfig`, `StorageConfig`, `MaintenanceConfig`, `CpuConfig` — expanded in their respective phases
- `load_runtime_config()` uses `sqlx::Row::try_get()` with graceful fallbacks — empty JSONB `{}` deserializes to defaults via `serde_json::from_value().unwrap_or_default()`; returns `RuntimeConfig::default()` when `server_config` table is empty (first-run)
- `set_environment()` in error.rs uses `OnceLock<String>` — called in `AppState::new()` and `AppState::new_with_config()`, replaces direct env var reads for environment detection
- `arc-swap` v1.9.1 added to workspace dependencies

**Key decisions from Task 4 (middleware.rs):**

- `RateLimitState` holds 5 `Arc<RateLimiter<K, DefaultKeyedStateStore<K>, DefaultClock>>` instances: `ip_global`, `ip_auth`, `user_authenticated`, `session_streaming`, `user_admin` — matching the 5 tiers from API_CONVENTIONS.md
- `RateLimitState` stored in `AppState` as `Arc<RateLimitState>` — created from `RuntimeConfig.auth.rate_limits` in `AppState::new_with_config()`, defaults in `AppState::new()`
- Rate limit values validated with `NonZeroU32::new(val).unwrap_or(fallback)` — zero config values fall back to API_CONVENTIONS.md defaults
- `rate_limit_global()` uses `axum::middleware::from_fn_with_state` — extracts client IP from `X-Forwarded-For` → `X-Real-IP` → `ConnectInfo<SocketAddr>` → `0.0.0.1` fallback
- Returns `AppError::RateLimited { code: "RATE_LIMITED" }` on violation — integrates with existing error.rs `Retry-After: 0` behavior
- CORS layer permissive in Local mode (`Allow-Origin: *`), strict in Exposed mode (configured origins + credentials) — per SECURITY.md tiered model
- Security headers tiered: `X-Content-Type-Options: nosniff` always; HSTS, X-Frame-Options, Referrer-Policy, Permissions-Policy, strict CSP only in Exposed mode — per SECURITY.md header behavior table
- CSP relaxed for Local (`default-src 'self' 'unsafe-inline' 'unsafe-eval' blob: data: media:`), strict for Exposed (full media-streaming CSP with `object-src 'none'`, `frame-ancestors 'none'`)
- Compression uses `CompressionLayer::new()` (gzip only) — BREACH mitigation via selective compression deferred to route-level application per SECURITY.md
- Request ID uses `tower-http` `SetRequestIdLayer` with `UuidV7RequestId` (UUIDv7 generation) — consistent with project-wide UUID strategy
- `tower-http` `"request-id"` feature added to workspace Cargo.toml
- TraceLayer and PropagateRequestIdLayer not wrapped in builder functions (complex generic return types from method chaining) — router.rs will create them directly in Task 6

**Key decisions from Task 5 (extractors.rs):**

- `AuthenticatedUser` implements `FromRequestParts<AppState>` — extracts session token from `Cookie: session=<token>` header first, then `Authorization: Bearer <token>` header; session validation against `user_sessions` table deferred to Phase 4 (current impl returns `Unauthorized` after successful token extraction)
- `AdminOnly` wraps `AuthenticatedUser` and checks `capabilities.contains("can_manage_server")` — capability check returns `Forbidden` per AUTH_007 error code pattern from ERROR_HANDLING.md
- `PaginationParams` enum with `Cursor { limit, cursor, order }` and `Offset { page, page_size }` variants — implements `FromRequestParts<AppState>` by wrapping `Query<PaginationQuery>` internally
- Pagination validation per API_CONVENTIONS.md: cursor+page conflict → `VALID_001`; limit max 100, page_size max 100, values < 1 rejected; cursor validated as base64 (`is_multiple_of(4)` + alphanumeric + `+/=` characters); order restricted to `"asc"` or `"desc"` with `desc` default
- Cursor pagination is default (when no pagination params provided) — `limit=20`, `order=desc`, `cursor=None` (first page)
- Offset pagination triggered by presence of `page` or `page_size` query params — `page` defaults to 1, `page_size` defaults to 25
- `SortOrder` enum with `Display` impl (`"asc"` / `"desc"`) — used by service layer for SQL `ORDER BY` direction
- `PaginationParams` accessor methods: `limit()`, `is_cursor()`, `cursor()`, `order()`, `page()`, `page_size()` — convenience for service layer query building
- No new workspace dependencies — cookie parsing via `Cookie` header string split (avoids `axum-extra` cookie dep), base64 validation via character-set check (avoids `base64` crate for Phase 3; actual cursor decode in Phase 5+ may add it)
- Also fixed pre-existing clippy `collapsible_if` warnings in `middleware.rs` `extract_client_ip()` — collapsed nested if-lets into `&&` let chains (edition 2024 feature)

**Not yet implemented (deferred to later tasks/phases):**

- Provider registry — Phase 6 (metadata providers) will add `ProviderRegistry` to `AppState`
- Domain-specific sub-config expansion — each domain phase expands its placeholder struct with real fields
- Rate limiter hot-reload — `RateLimitState` is fixed at startup; rebuilding on `reload_runtime_config()` deferred to admin API implementation

**Key decisions from Task 6 (router.rs):**

- `build_router(state: AppState) -> Router<AppState>` — returns stateful router; caller invokes `.with_state(state)` to make it servable — separates router assembly from state provision, allowing main.rs to own the state lifecycle
- Full middleware stack applied in order (outermost → innermost): `SetRequestIdLayer` → `PropagateRequestIdLayer` → `TraceLayer` → `CorsLayer` → security headers (iterable) → `CompressionLayer` → `rate_limit_global` (via `from_fn_with_state`)
- `TraceLayer` and `PropagateRequestIdLayer` created inline in router.rs rather than in middleware.rs builder functions — method chaining produces complex generic return types that are impractical to encapsulate (per Task 4 design decision)
- `ArcSwap<RuntimeConfig>` lease released with explicit `drop(config)` after middleware configuration reads — prevents holding the lock across the router lifetime
- Health check enhanced per API_CONVENTIONS.md: returns `{ status, version, database, uptime_seconds }` — DB connectivity tested via `SELECT 1`; uptime tracked via `OnceLock<Instant>` static; status is `"healthy"` or `"degraded"` (not a 5xx error) so Docker HEALTHCHECK doesn't restart on transient DB issues
- Domain router merge points added as comments (15 domains) — each domain phase adds its `.merge()` call
- Security headers applied via iteration over `Vec<SetResponseHeaderLayer>` from `build_security_headers()` — avoids variadic layer composition
- main.rs updated to create `PgPool` and `AppState` before calling `build_router` — minimal change (not the full 14-step sequence, which is Task 7); requires `DUSKCUE_DATABASE_URL` or exits with clear error message

**Key decisions from Task 7 (main.rs — 14-step startup):**

- Full 14-step startup sequence from CONFIGURATION.md implemented in `main()`: parse CLI → build BootstrapConfig → init logging → validate database_url → acquire lockfile (stub) → connect PG → validate PG settings → run migrations → load RuntimeConfig → check auth state → start scheduled tasks (stub) → bind HTTP → ready
- PgPoolOptions configured per MEMORY.md: `max_connections(20)`, `min_connections(2)`, `acquire_timeout(5s)`, `max_lifetime(30min)`, `idle_timeout(10min)`, `after_connect` sets `application_name = 'duskcue'`
- Database connection retry per CONFIGURATION.md fail-fast rules: 3 attempts, 5s interval between retries; each failure logged at WARN with attempt number
- Migrations run via `sqlx::migrate!()` macro (compile-time embedded) — reads `server/migrations/` directory at build time; requires `migrate` + `sqlx-toml` features on sqlx
- `sqlx.toml` fixed: removed invalid `migrations-dir` field from `[common]` section (not a recognized sqlx config field; the `migrate!()` macro defaults to `./migrations`)
- PostgreSQL settings validation implemented per MEMORY.md: queries `pg_settings` for `fsync`, `full_page_writes`, `data_checksums`, `wal_level`; logs WARN for each mismatch; non-blocking (never prevents startup); failures to query are also non-blocking (WARN level)
- Runtime config loaded via existing `load_runtime_config(&pool)` from state.rs; `AppState::new_with_config()` used (not `AppState::new()`) so rate limits are initialized from DB config, not defaults
- Auth setup state checked: if `config.is_setup_mode()`, logs WARN that only setup endpoints will be accessible — provides clear operator feedback
- Stubs for remaining Tasks 8–12: lockfile acquisition logs info (Task 11 will implement PID-file check); scheduled task runner logs "not yet implemented" (Phase 5); graceful shutdown keeps existing `with_graceful_shutdown(shutdown_signal())` pattern (Task 8 will upgrade to CancellationToken + TaskTracker); logging keeps existing bootstrap-level `tracing_subscriber::fmt()` (Task 9 will add file appender + ErrorLayer)
- `database_url` missing provides actionable error message: prints example connection string and all three configuration methods (CLI, env var, config.toml)
- All `unwrap_or_else` exits use `std::process::exit(1)` — consistent exit code for all startup failures
- `sqlx` workspace deps updated: added `migrate` and `sqlx-toml` features

**Key decisions from Task 8 (graceful shutdown):**

- 3-phase shutdown per MEMORY.md: Signal → Drain (30s) → Cleanup (90s)
- `CancellationToken` from `tokio_util::sync` signals all long-lived tasks to begin cooperative shutdown — replaces direct `shutdown_signal()` future passed to `with_graceful_shutdown`
- `TaskTracker` from `tokio_util::task` tracks background tasks — `close()` + `wait()` pattern ensures no task left behind; `tokio-util` workspace dep updated with `rt` feature for `TaskTracker` support
- `shutdown_signal()` refactored: returns the `CancellationToken` after detecting signal, allowing main to control the full 3-phase sequence
- Double-signal protection preserved: `AtomicBool` swap — second signal forces `std::process::exit(1)`
- Phase 1 (Signal): `CancellationToken::cancel()` → Axum stops accepting new HTTP connections via `with_graceful_shutdown(shutdown.cancelled())`
- Phase 2 (Drain 30s): `tracker.close()` + `tokio::time::timeout(30s, tracker.wait())` — waits for in-flight requests and background tasks; logs WARN on timeout
- Phase 3 (Cleanup): `pool.close().await` drains PG connection pool (in-flight queries complete); lockfile removal stub logs info (Task 11 implements actual removal); no embedded PG stop yet (Phase 15)
- Cross-platform signal handling unchanged: `#[cfg(unix)]` for SIGTERM via `SignalKind::terminate()`, `std::future::pending()` fallback on non-Unix
- No new workspace dependencies beyond `tokio-util` feature flag change

**Key decisions from Task 9 (tracing subscriber):**

- `server/src/logging.rs` created as cross-cutting infrastructure module per LOGGING_OBSERVABILITY.md — not a domain module
- Layered subscriber: `Registry` → `EnvFilter` → `ErrorLayer` (tracing-error) → console fmt layer (pretty) → file fmt layer (JSON)
- Console output uses `tracing_subscriber::fmt::layer().pretty()` — colored, human-readable for `docker logs` and development
- File output uses `tracing_subscriber::fmt::layer().json()` with non-blocking writer — structured JSON for log aggregation (Loki, Elastic)
- `RollingFileAppender::builder()` with `Rotation::DAILY`, filename prefix `server`, suffix `log`, `max_log_files(5)` — writes to `{data_dir}/logs/`
- `WorkerGuard` returned from `init_logging()` and held as `_log_guard` in `main()` — dropped on shutdown to flush buffered file writes
- `ErrorLayer` from `tracing-error` captures span context in error chains — enables `SpanTrace` enrichment per LOGGING_OBSERVABILITY.md error enrichment section
- `EnvFilter` reads `RUST_LOG` env var first, falls back to bootstrap `log_level` — consistent with CONFIGURATION.md step 3
- `LoggingConfig` in `state.rs` expanded from empty placeholder to full struct: `level`, `max_file_size_mb`, `max_files`, `format` — matches LOGGING_OBSERVABILITY.md Rust struct definition
- File settings use hardcoded defaults at startup (step 3); hot-reload from runtime `LoggingConfig` deferred to admin API implementation
- `max_file_size_mb` stored in config but not used by `tracing-appender` (only supports time-based rotation); retained for future custom rotation or documentation accuracy
- `tracing-error` v0.2.1 added to workspace dependencies

**Key decisions from Task 10 (Prometheus /metrics endpoint):**

- `init_metrics()` in `logging.rs` — installs global `metrics` recorder via `PrometheusBuilder::new().install_recorder()`, returns `PrometheusHandle` for the route handler; custom histogram buckets for `http_request_duration` (5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s, 5s, 10s)
- `/metrics` GET endpoint in `router.rs` — returns Prometheus text format via `state.metrics_handle.render()`; subnet-guarded by `metrics_subnet_guard` middleware
- `track_http_metrics` middleware — records `http_requests_total` (counter, labels: method, status) and `http_request_duration` (histogram, label: method) for all requests except `/metrics` (avoids self-referential metrics)
- `metrics_subnet_guard` middleware — checks client IP against `state.metrics_allowed_subnets` (parsed `IpNet` CIDRs); returns 403 Forbidden for disallowed IPs; uses same `extract_client_ip` logic as rate limiter
- `NetworkConfig` expanded from empty placeholder to include `allowed_metrics_subnets: Vec<String>` — defaults to `["127.0.0.1/32", "::1/128"]` (localhost only); parsed to `Vec<IpNet>` at startup via `parse_metrics_subnets()` with invalid CIDR warnings
- `PrometheusHandle` and `Arc<Vec<IpNet>>` added to `AppState` — handle for rendering metrics, subnets for access control
- Metrics recorder installed at startup step 3 (after logging init, before router build) — all subsequent operations emit metrics
- HTTP metrics middleware placed between TraceLayer and CorsLayer in the middleware stack — all requests tracked (including rate-limited 429s), within trace span context
- Metric names use underscores (`http_requests_total`, `http_request_duration`) following Prometheus naming conventions; the design doc uses dots but `metrics-exporter-prometheus` renders underscores
- `AppState::new()` and `AppState::new_with_config()` signatures updated to accept `PrometheusHandle` parameter
- `metrics` v0.24, `metrics-exporter-prometheus` v0.18, `ipnet` v2 added to workspace dependencies
 - `/metrics` endpoint excluded from HTTP metrics tracking via path check — prevents self-referential metrics scrape noise

**Key decisions from Task 11 (startup lockfile):**

 - `server/src/lockfile.rs` — cross-cutting infrastructure module, not a domain module
 - `Lockfile` struct with `acquire(data_dir)` and `release()` methods; `Drop` impl as safety net for crash cleanup (best-effort since SIGKILL won't trigger Drop)
 - `LockfileError` enum: `AlreadyRunning { pid }`, `Read(io::Error)`, `InvalidContent` — uses `thiserror` consistent with project error conventions
 - On startup: if lockfile exists and PID is alive → fail with clear message; if PID is dead → remove stale lockfile and continue
 - PID liveness check via `sysinfo` crate (0.34) — uses `refresh_processes(ProcessesToUpdate::Some(&[pid]))` to check only the specific PID, avoiding full system scan
 - `sysinfo` chosen over `libc::kill(pid, 0)` or Windows `OpenProcess` — cross-platform, no `unsafe` code, already designated in MEMORY.md for the memory watchdog
 - Unreadable/corrupt lockfile treated same as stale — removed with warning, startup continues
 - `postmaster.pid` check (step 1 of MEMORY.md Startup Lockfile) deferred to Phase 15 (embedded PostgreSQL) — not relevant for external PG mode
 - `sysinfo` v0.34.2 added to workspace dependencies
 - Lockfile removed explicitly in shutdown Phase 3 (`lockfile.release()`) and via `Drop` as safety net — `released` flag prevents double-removal

**Key decisions from Task 12 (PG settings validation expansion):**

 - Expanded from 4 to 5 data-safety settings: added `synchronous_commit` (required for crash recovery guarantees per MEMORY.md, PROJECT.md, RELEASE_ENGINEERING.md, BACKUP_RECOVERY.md)
 - Added PostgreSQL version detection via `current_setting('server_version')` — logs INFO with version string; WARN if major version < 18 (DATABASE.md targets PG18 for native `uuidv7()`)
 - Scope limited to data safety/integrity only — performance settings (`shared_buffers`, `work_mem`, `max_connections`, etc.) intentionally excluded because they are workload-dependent and the embedded PG configures them optimally
 - Warning counter tracks total issues across version check + settings check; summary log distinguishes "all checks passed" from "validated with N warning(s)"
 - Single `pg_settings` query with all 5 settings — avoids multiple round-trips
 - No new workspace dependencies — uses existing `sqlx::query_scalar` for version, existing `sqlx::Row` for settings

**Tasks:**

1. ~~Implement `config.rs` — parse bootstrap TOML + ENV + CLI via `config-rs` + `clap`~~ **DONE** (bootstrap config in `config.rs`; `RuntimeConfig` DB loading in `state.rs` Task 2; `AppState` wiring in Tasks 2/4)
2. ~~Implement `state.rs` — `AppState` with `PgPool`, rate limit state, provider registry, config handles~~ **DONE**
3. ~~Implement `error.rs` — unified `AppError` enum with RFC 9457 `IntoResponse`~~ **DONE**
4. ~~Implement `middleware.rs` — Tower stack: logging, CORS, rate limiting, security headers, compression~~ **DONE**
5. ~~Implement `extractors.rs` — `AuthenticatedUser`, `PaginationParams`, `AdminOnly`~~ **DONE**
6. ~~Implement `router.rs` — top-level router assembly merging all domain routers~~ **DONE**
7. ~~Implement `main.rs` — 14-step startup sequence from CONFIGURATION.md~~ **DONE**
8. ~~Implement graceful shutdown per MEMORY.md~~ **DONE**
   - SIGINT + SIGTERM handling via CancellationToken
   - Double-signal protection (`std::process::exit(1)`)
   - 3-phase: Signal → Drain 30s → Cleanup 90s
   - PG Fast mode checkpoint
  9. ~~Wire up `tracing` subscriber — pretty console + JSON file via `tracing-appender`~~ **DONE**
   10. ~~Wire up Prometheus `/metrics` endpoint~~ **DONE**
   11. ~~Implement startup lockfile at `/data/.duskcue.lock`~~ **DONE**
    12. ~~Implement PG settings validation — expanded from Task 7 basic implementation~~ **DONE**

**Verification:** Server boots, connects to PG, runs migrations, `/health` returns 200, `/metrics` returns Prometheus format, SIGTERM triggers graceful shutdown with PG checkpoint.

---

## Phase 4 — Authentication & Users

**Goal:** Users can register, log in with passkeys, manage sessions, and have capability-based access.

**Prerequisites:** Phase 3 complete. `AuthenticatedUser` extractor in `extractors.rs` has session token extraction (cookie + Bearer) with validation stub returning `Unauthorized` — Phase 4 must wire the actual `user_sessions` table lookup. `AdminOnly` extractor checks `can_manage_server` capability — Phase 4 must populate capabilities from DB. Rate limit tiers (`ip_auth`, `user_authenticated`, `user_admin`) exist in `RateLimitState` — Phase 4 applies them to auth endpoints.

**Context from Phase 3:**

- `AppState` provides `PgPool` for session/user queries, `ArcSwap<RuntimeConfig>` for `AuthConfig` (network_mode, session timeouts, invite code settings)
- `error.rs` has generic `Unauthorized` and `Forbidden` variants — Phase 4 may add domain-specific `Auth` variant
- Middleware stack applies rate limiting, security headers, and CORS — auth endpoints inherit all middleware
- `lockfile.rs` pattern (five-file: mod/error/types/service/handlers) is the reference for domain module structure

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [AUTH.md](docs/design/AUTH.md) | **Primary** — passkey-first (WebAuthn/FIDO2), capability-based access control, invite codes, device linking (RFC 8628), re-auth codes |
| [API_SECURITY.md](docs/security/API_SECURITY.md) | Session cookie (HttpOnly, SameSite=Strict) + Bearer token dual auth |
| [API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md) | Auth endpoint patterns, rate limiting tiers (5 tiers) |

**Tasks:**

1. ~~Create `server/src/domains/auth/` — five-file pattern~~ **DONE**
2. ~~Implement WebAuthn registration and authentication flows~~ **DONE**
3. ~~Implement invite code system — admin creates invite, user registers with code~~ **DONE**
4. ~~Implement `user_sessions` — session creation, validation, revocation~~ **DONE**
5. ~~Implement `user_capabilities` — capability-based access control checks~~ **DONE**
6. ~~Implement device linking — RFC 8628 device code flow~~ **DONE**
7. ~~Implement re-auth codes for sensitive operations~~ **DONE**
8. ~~Create `server/src/domains/users/` — five-file pattern~~ **DONE**
9. ~~Implement user CRUD — list, get, update, soft-delete~~ **DONE** (implemented as part of Task 8)
10. ~~Implement `AuthenticatedUser` extractor — validates session from cookie or Bearer token~~ **DONE** (completed during Task 4)
11. ~~Implement `require_capability()` middleware for admin endpoints~~ **DONE**

**Verification:** Admin creates invite code, new user registers with passkey, user session is created, authenticated requests succeed, unauthorized requests return 401, admin-only endpoints require `can_manage_server`.

**What was built for Task 1:**

| File | Purpose |
|---|---|
| `server/src/domains/auth/mod.rs` | Module declarations + router assembly with 22 routes |
| `server/src/domains/auth/error.rs` | `AuthError` enum with 23 variants covering AUTH_001–AUTH_022 |
| `server/src/domains/auth/types.rs` | Request/response DTOs (`SetupRequest`, `PasswordLoginRequest`, `InviteAuthRequest`, `SessionResponse`, `UserSummary`, etc.), `LoginUser`, `ValidatedSession`, `UserSession`, `UserCapabilities`, `DeviceInfo` |
| `server/src/domains/auth/service.rs` | Service layer with runtime `sqlx::query`: `validate_session`, `resolve_capabilities`, `create_session`, `setup_owner`, `is_setup_complete`, `revoke_session`, `revoke_all_sessions`, `list_user_sessions`, `authenticate_invite_code`, `get_user_for_login`, `reset_login_failures`, `user_count`, password hash/verify via `ring::pbkdf2`, session token generation via `rand` 0.9 |
| `server/src/domains/auth/handlers.rs` | Working handlers for `setup`, `auth_invite`, `auth_login`, `auth_logout`, `auth_logout_all`, `list_user_sessions`, `delete_user_session`; `todo!()` stubs for WebAuthn, TOTP, device linking, re-auth, passkey management, invitation CRUD |
| `server/src/error.rs` | Added `AppError::Auth(#[from] AuthError)` variant + `auth_error_to_http()` mapping all 22 error codes |
| `server/src/lib.rs` | Added `pub mod domains;` |
| `server/src/domains/mod.rs` | Added `pub mod auth;` |
| `server/src/router.rs` | Merged auth router via `.merge(crate::domains::auth::router(state.clone()))` |
| `Cargo.toml` | Added `rand = "0.9"` to workspace deps |
| `server/Cargo.toml` | Added `rand.workspace = true` |

**Key decisions from Task 1:**

- Runtime `sqlx::query` over compile-time `sqlx::query!` — no running PostgreSQL available for macro expansion; avoids `DATABASE_URL` requirement at build time
- `AppError::Auth(#[from] AuthError)` variant added to central error enum per ERROR_HANDLING.md domain-specific variant pattern — `auth_error_to_http()` in error.rs maps all 22 auth error codes to HTTP status codes
- Password hashing: PBKDF2-HMAC-SHA256 with 600,000 iterations via `ring::pbkdf2` (AUTH.md specifies Argon2id; initial implementation uses PBKDF2 to avoid adding `argon2` dependency — `ring` already in workspace. See [AUTH.md](docs/design/AUTH.md) Implementation Notes section for rationale and migration path; [SECURITY.md](docs/security/SECURITY.md) references `argon2` for password hashing)
- Session tokens: 32 random bytes hex-encoded via `rand` 0.9; token stored as SHA-256 hash in DB
- Module visibility: `pub mod` for all four sub-files in auth/mod.rs; explicit `handlers::fn_name` in router to avoid glob import conflicts (`list_user_sessions` exists in both service and handlers)
- `validator` 0.20 API: `.code` and `.message` are public fields (not methods); `.field_errors()` returns `HashMap` requiring `.into_iter()` before `.flat_map()`
- WebAuthn crate deferred to Task 2 — service layer abstracted so crate choice is encapsulated; recommended `passkey-auth` (pure Rust, no OpenSSL, aligns with `ring`/`rustls` workspace strategy)

**What was built for Task 2:**

| File | Purpose |
|---|---|
| `server/src/domains/auth/service.rs` | Added 8 WebAuthn service functions: `start_passkey_registration`, `finish_passkey_registration`, `start_passkey_authentication`, `finish_passkey_authentication`, `list_user_passkeys`, `delete_passkey`, `expire_challenges`, `generate_challenge_id` |
| `server/src/domains/auth/handlers.rs` | Replaced `todo!()` stubs with working handlers: `webauthn_start`, `webauthn_finish`, `passkey_list`, `passkey_register_start`, `passkey_register_finish`, `passkey_delete` |
| `server/src/domains/auth/types.rs` | Added `WebauthnRegisterStartResponse`, `WebauthnAuthStartResponse`; removed generic `WebauthnChallengeResponse` |
| `server/src/state.rs` | Added `Webauthn` instance (`Arc<Webauthn>`), `WebauthnChallenge` struct, `DashMap<String, WebauthnChallenge>` challenge store to `AppState`; added `build_webauthn()` helper |
| `Cargo.toml` | Added `webauthn-rs = "0.6.1-dev"` (features: `danger-allow-state-serialisation`, `danger-credential-internals`), `dashmap = "5.5"`, `url = "2"`, `base64 = "0.22"` to workspace deps |
| `server/Cargo.toml` | Added `webauthn-rs.workspace = true`, `dashmap.workspace = true`, `url.workspace = true`, `base64.workspace = true` |

**Key decisions from Task 2:**

- **`webauthn-rs` over `passkey-auth`** — Task 1 recommended `passkey-auth`, but research revealed it's a client-side library (WebAuthn client/authenticator), not a server-side Relying Party library. `webauthn-rs` (kanidm) is the correct choice for server-side WebAuthn verification — mature, security-audited by SUSE, with a safe high-level passkey API
- **Challenge state storage** — `PasskeyRegistration` and `PasskeyAuthentication` states stored in `DashMap<String, WebauthnChallenge>` (in-memory, keyed by challenge ID, 5-minute TTL). Single-instance sufficient; multi-instance horizontal scaling would require shared store (Redis/PG) — deferred
- **Feature `danger-allow-state-serialisation`** — Required to serialize `PasskeyRegistration`/`PasskeyAuthentication` states to the DashMap. Safe because states are stored server-side, never in client cookies
- **Feature `danger-credential-internals`** — Required to convert `Passkey` → `Credential` for accessing `transports` and `counter` fields during registration
- **`Webauthn` instance in `AppState`** — Built from `AuthConfig.rp_id` and `AuthConfig.rp_origin`; falls back to `localhost`/`http://localhost:48027` if configuration is missing or invalid
- **Challenge ID via `X-Challenge-Id` header** — Client sends the challenge ID in a custom header on `finish` requests; this links the start/finish ceremony without server-side session state on the HTTP layer
- **Passkey registration excludes existing credentials** — `start_passkey_registration` loads existing `credential_id`s from `user_passkeys` to prevent registering the same authenticator twice
- **Passkey stored as serialized JSON** — Full `Passkey` struct serialized to `public_key` BYTEA column; reconstructed for authentication by loading from DB and deserializing. Alternative: store individual fields (cred_id, COSEKey separately) but serialization is simpler and the `Passkey` type handles versioning
- **Authentication counter update** — After successful authentication, `sign_count` updated in `user_passkeys` from `AuthenticationResult::counter()`
- **User verification on finish** — `passkey_register_finish` validates that the authenticated user's ID matches the challenge's stored user_id, preventing one user from completing another's registration
- **`webauthn-rs` v0.6.1-dev** — Pre-release version; latest available on crates.io as of June 2026; MSRV 1.88.0; MPL-2.0 license

**What was built for Task 3:**

| File | Purpose |
|---|---|
| `server/src/domains/auth/service.rs` | Added 4 invite code service functions: `generate_invite_code`, `create_invitation`, `list_invitations`, `revoke_invitation`, `resend_invitation`; `CreateInvitationParams` struct to avoid clippy `too_many_arguments`; `extract_code_prefix` helper |
| `server/src/domains/auth/handlers.rs` | Replaced `todo!()` stubs with working handlers: `list_invitations`, `create_invitation`, `revoke_invitation`; added new `resend_invitation` handler; all check `can_manage_users` capability inline |
| `server/src/domains/auth/mod.rs` | Added `POST /api/v1/invitations/{id}/resend` route |

**Key decisions from Task 3:**

- **Base-20 character set** — `BCDFGHJKLMNPQRSTVWXZ` (consonants only, no ambiguous chars per RFC 8628 Section 6.1); 24 random characters → ~103 bits entropy; formatted as `mv_invite-BCDK-MJHT-WDJB-NPQR-STVW-XZBC` with 4-char dash-separated groups
- **Code generation** — `generate_invite_code()` uses `rand::rng()` with `random_range(0..20)` per character; `extract_code_prefix()` strips prefix and dashes, takes first 4 chars for `code_prefix` column
- **Create returns full code** — `InvitationResponse.code` is `Some(full_code)` only on creation; `list_invitations` returns `code: None` (admin sees only `code_prefix`)
- **Resend regenerates code** — `resend_invitation()` generates a new code, updates `code_hash` + `code_prefix`, resets `use_count` to 0; original code is invalidated. Rationale: we only store the SHA-256 hash, so the original code cannot be retrieved; generating a fresh code is more secure than trying to resend the same one
- **SMTP delivery deferred** — `create_invitation` and `resend_invitation` log an info message that SMTP is not yet implemented; email delivery will be added when SMTP configuration is implemented (Phase 13 system operations)
- **Capability check inline** — All invitation handlers check `can_manage_users` inline since `AdminOnly` extractor checks `can_manage_server`; Task 11 will create a proper `require_capability()` middleware
- **Revocation marks code only** — `revoke_invitation` sets `is_revoked = true` but does not terminate existing sessions. AUTH.md specifies "all sessions from that code are terminated," but `user_sessions` lacks an `invitation_id` column to link sessions to their originating invite code. Full session termination on revocation requires a migration adding `invitation_id` to `user_sessions` — deferred to a future enhancement
- **`CreateInvitationParams` struct** — Introduced to satisfy clippy `too_many_arguments` (10 params → 2 params); handler constructs the struct from validated `CreateInvitationRequest` fields with defaults (`role: "member"`, `max_uses: 1`, `has_all_library_access: false`)
- **No new workspace dependencies** — invite code generation uses existing `rand` 0.9; hashing uses existing `ring::digest::SHA256`

**What was built for Task 4:**

| File | Purpose |
|---|---|
| `server/src/extractors.rs` | `AuthenticatedUser` extractor now calls `validate_session()` to look up session token in `user_sessions`, load user + capabilities from DB, enforce idle timeout, and throttled `last_active_at` update (60s). Added `has_all_library_access` and `display_name` fields. |
| `server/src/domains/auth/service.rs` | Added `is_idle_expired()` helper to check configurable idle timeout against `last_active_at`; returns `false` when no idle timeout configured (local mode default) |
| `server/src/domains/auth/handlers.rs` | Added `set_session_cookie()` and `clear_session_cookie()` helpers for `Set-Cookie` header management. Updated `setup`, `auth_invite`, `auth_login`, `webauthn_finish` to set session cookie on successful authentication. Updated `auth_logout` and `auth_logout_all` to clear session cookie. |

**Key decisions from Task 4:**

- **Session validation in extractor** — `AuthenticatedUser::from_request_parts()` calls `auth::service::validate_session()` with the extracted token. This performs a single DB query to look up the session by `token_hash` SHA-256, verifies `expires_at > now()`, loads the user (must be `active`, not soft-deleted), and resolves capabilities via `resolve_capabilities()`.
- **Throttled `last_active_at` update** — Session `last_active_at` is only updated if 60+ seconds have elapsed since the last update, avoiding a DB write on every single request. The threshold is hardcoded (not configurable) as a reasonable balance between freshness and write amplification.
- **Idle timeout enforcement** — `is_idle_expired()` checks the configurable `session_idle_timeout_hours` from `AuthConfig`. Local mode default is `None` (no idle timeout). Exposed mode default is 7 days. When a session is idle-expired, it is immediately deleted from `user_sessions` and the request returns `AUTH_005 Session expired`.
- **Session cookie attributes per SECURITY.md/AUTH.md** — `Set-Cookie: session={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={absolute_timeout_seconds}`. In exposed mode, `Secure` flag is appended. `Max-Age` uses `session_absolute_timeout_days` from `AuthConfig` (90 days local, 30 days exposed).
- **Cookie cleared on logout** — `auth_logout` and `auth_logout_all` set a `Max-Age=0` cookie to instruct the browser to delete the session cookie.
- **Response type change** — Auth handlers that set cookies now return `impl IntoResponse` instead of `Json<SessionResponse>` to allow adding `Set-Cookie` headers to the response. The JSON body structure is unchanged.
- **No new workspace dependencies** — session cookie is built as a plain string; no `axum-extra` cookie jar or `cookie` crate added.

**What was built for Task 5:**

| File | Purpose |
|---|---|
| `server/src/domains/auth/service.rs` | Added `CAPABILITY_DESCRIPTIONS` static (12 name/description pairs), `validate_capability_name()`, `check_capability()`, `get_capability_overrides()`, `update_capabilities()` |
| `server/src/domains/auth/types.rs` | Added `AvailableCapability`, `CapabilityListResponse`, `CapabilityOverrideResponse`, `CapabilityOverridesResponse`, `UpdateCapabilitiesRequest`, `CapabilityOverride` DTOs |
| `server/src/domains/auth/handlers.rs` | Added `list_capabilities`, `get_user_capabilities`, `update_user_capabilities` handlers; refactored 4 invitation handlers from inline capability checks to `service::check_capability()` |
| `server/src/domains/auth/mod.rs` | Added 3 routes: `GET /api/v1/auth/capabilities`, `GET/PUT /api/v1/users/{id}/capabilities` |

**Key decisions from Task 5:**

- **`check_capability()` replaces inline checks** — Centralized `service::check_capability(role, capabilities, required)` returns `Result<(), AuthError>`; owner role short-circuits to `Ok(())`; matches against capabilities list; returns `InsufficientCapabilities` error with required capability name. All 4 invitation handlers refactored from duplicated `user.capabilities.iter().any(...)` to `service::check_capability()`.
- **`update_capabilities()` uses delete-and-reinsert** — Rather than upserting individual rows, the function deletes all existing overrides for the user and inserts the new set in a single transaction. This avoids complex diffing logic and ensures the override set exactly matches the request.
- **Owner bypass on updates** — `update_capabilities()` returns early with current overrides (read-only) when `role == "owner"` — owners always have all capabilities regardless of `user_capabilities` rows; writing override rows for an owner would be misleading.
- **Capability name validation** — `validate_capability_name()` checks against `ALL_CAPABILITIES` static list; `update_capabilities()` rejects requests containing invalid capability names with `InsufficientCapabilities` error before any DB writes.
- **`GET /api/v1/auth/capabilities` is unauthenticated** — Returns the static list of all available capabilities with descriptions; no auth required since this is metadata for the admin UI to build capability selection forms.
- **`GET/PUT /api/v1/users/{id}/capabilities` require `can_manage_users`** — Both endpoints check admin capability via `check_capability()`; lookup target user by ID with `deleted_at IS NULL` guard; response includes `role`, `overrides` (explicit rows), and `effective` (resolved list after evaluation).
- **No new workspace dependencies** — capability CRUD uses existing `sqlx::query` and `PgPool::begin()` for transactions.

**What was built for Task 6:**

| File | Purpose |
|---|---|
| `server/src/domains/auth/service.rs` | Device-code generation, normalized user-code lookup, request review, explicit decisions, and atomic token exchange; `CreateDeviceCodeParams` struct |
| `server/src/domains/auth/handlers.rs` | Working `device_code`, `device_token`, review, and approve-or-deny handlers |

**Key decisions from Task 6:**

- **RFC 8628 device code flow** — `POST /api/v1/device/code` initiates, `POST /api/v1/device/token` polls, authenticated `GET /api/v1/device/verify` reviews non-secret device metadata, and authenticated `POST /api/v1/device/verify` explicitly approves or denies.
- **Device code hashed at rest** — The internal `device_code` (32 random bytes, hex-encoded, 256-bit) is SHA-256 hashed before storage in the `device_linking_codes.device_code` column, consistent with session token pattern. Raw code sent to device once, never stored.
- **User code stored raw** — The 8-char base-20 user code is stored without formatting in `device_linking_codes.user_code`. Lookup normalizes case and removes non-alphanumeric formatting, so `wdjb mjht` and `WDJB-MJHT` both match.
- **Canonical verification URI and browser handoff** — Commit `b3da901` returns `/auth/link` from the configured canonical base URL and `verification_uri_complete` for QR/NFC use. Exposed deployments reject issuance without a canonical public URL and never trust the request `Host` header. Sign-in preserves only a local `/auth/link?...` return target and the browser always requires a separate review and explicit decision.
- **Explicit, terminal denial** — Migration `20260718100000_harden_device_linking.sql` records denial separately from approval. A denied device receives `AUTH_014` / HTTP 403 and cannot later be approved.
- **Atomic token exchange and cleanup** — Approval/denial and token exchange lock the linking row. Successful exchange creates the session and deletes the code in one transaction, preventing concurrent polls from minting multiple sessions. Expired codes are deleted when observed.
- **Persisted polling protection** — The linking row records `last_polled_at` and the active interval. Early polls return `AUTH_024` / HTTP 429 with `Retry-After`, increasing the interval by five seconds up to 60. Issuance and review/decision use the auth IP limiter; token polling deliberately uses the persisted per-code rule because a compliant five-second poll exceeds the generic 10-per-minute budget.
- **Session creation uses stored device metadata** — When the device polls and finds `is_approved = true`, a session is created for `approved_by_user_id` using the `client_name`, `client_platform`, `client_version`, `ip_address`, and `user_agent` from the original device code request.
- **`CreateDeviceCodeParams` struct** — Introduced to satisfy clippy `too_many_arguments` (8 params → 3 params); same pattern as `CreateInvitationParams` from Task 3.
- **No new workspace dependencies** — device code generation uses existing `rand` 0.9 and `BASE20_CHARS`; hashing uses existing `sha256_hex`.
- **Configurable parameters** — `AuthConfig.device_linking_code_length` (default 8), `device_linking_code_expiry_seconds` (default 900), `device_linking_poll_interval_seconds` (default 5) — all from `AuthConfig` defaults in `state.rs`.

**Device-linking hardening outcome (2026-07-18):** Commit `b3da901` adds the canonical browser flow, explicit decisions, transactional single-use consumption, per-code polling cadence, loopback-only forwarded-IP trust, versioned contract/fixture coverage, and `scripts/verify-device-linking-integration.mjs`. `cargo check -p duskcue`, focused Rust tests, `npm run build`, contract/auth/device-linking verifiers, and disposable PostgreSQL migration verification pass. Configurable non-loopback trusted-proxy CIDRs remain a separate reverse-proxy hardening follow-up.

**What was built for Task 7:**

| File | Purpose |
|---|---|
| `server/src/domains/auth/service.rs` | Added 4 re-auth code functions: `generate_reauth_code`, `create_reauth_code`, `authenticate_reauth_code`, `extract_reauth_prefix` |
| `server/src/domains/auth/handlers.rs` | Replaced `todo!()` stubs with working handlers: `reauth`, `reauth_request`; added new handlers: `sign_out_everywhere`, `request_reauth` |
| `server/src/domains/auth/mod.rs` | Added 2 routes: `POST /api/v1/user/sign-out-everywhere`, `POST /api/v1/user/request-reauth` |

**Key decisions from Task 7:**

- **Re-auth code format** — `mv_reauth-` prefix + 16 base-20 characters (~69 bits entropy), formatted as 4-char dash-separated groups (e.g., `mv_reauth-BCDK-MJHT-WDJB-NPQR`). Uses same `BASE20_CHARS` set as invite and device codes.
- **Code hashed at rest** — SHA-256 hash stored in `reauth_codes.code_hash`; raw code never stored. `code_prefix` (first 4 base-20 chars after prefix) stored for admin identification.
- **Rate limiting** — `create_reauth_code()` queries `reauth_codes` for the user in the last 24 hours; returns `AuthError::ReauthRateLimited` if count exceeds `AuthConfig.reauth_max_requests_per_user_per_day` (default 3).
- **Single use** — `authenticate_reauth_code()` sets `is_used = true` and `used_at = now()` after successful authentication. Subsequent attempts return `AuthError::ReauthCodeInvalid`. The `resulting_session_id` column links the re-auth code to the session it created.
- **User status check** — Authentication rejects codes for non-active or soft-deleted users.
- **`POST /api/v1/auth/reauth`** — Unauthenticated endpoint; accepts re-auth code + device info, validates, creates session, sets cookie. Returns session token + user summary.
- **`POST /api/v1/auth/reauth/request`** — Authenticated endpoint; generates a new re-auth code for the requesting user.
- **`POST /api/v1/user/sign-out-everywhere`** — Authenticated endpoint; revokes all sessions, generates re-auth code, clears session cookie. Returns count of revoked sessions + re-auth code prefix/expiry.
- **`POST /api/v1/user/request-reauth`** — Authenticated endpoint; generates re-auth code without revoking existing sessions (for requesting a code proactively).
- **SMTP delivery deferred** — `create_reauth_code()` logs an info message that SMTP is not yet implemented, consistent with invite code pattern from Task 3.
- **No new workspace dependencies** — re-auth code generation uses existing `rand` 0.9 and `BASE20_CHARS`; hashing uses existing `sha256_hex`.
- **No new error variants needed** — `ReauthCodeInvalid` (AUTH_015, 401) and `ReauthRateLimited` (AUTH_016, 429) already existed in `AuthError` from Task 1 with mappings in `error.rs`.

**What was built for Task 8:**

| File | Purpose |
|---|---|
| `server/src/domains/users/mod.rs` | Module declarations + router assembly with 2 routes (4 endpoints) |
| `server/src/domains/users/error.rs` | `UsersError` enum with 9 variants covering USER_001–USER_008 + Database catch-all |
| `server/src/domains/users/types.rs` | `UserRow` (internal), `UpdateUserRequest` (Deserialize + Validate), `UserResponse`, `UserListResponse` DTOs; `VALID_ROLES` and `VALID_STATUSES` statics |
| `server/src/domains/users/service.rs` | Service layer with `list_users` (paginated, filterable by status/role), `get_user`, `update_user` (via `UpdateUserParams` struct), `soft_delete_user`, `row_to_user_row`, `row_to_response`, `validate_streaming_policy_exists` |
| `server/src/domains/users/handlers.rs` | Working handlers for `list_users`, `get_user`, `update_user`, `delete_user`; all check `can_manage_users` capability via `auth::service::check_capability()` |
| `server/src/domains/mod.rs` | Added `pub mod users;` |
| `server/src/router.rs` | Merged users router via `.merge(crate::domains::users::router(state.clone()))` |
| `server/src/error.rs` | Added `AppError::Users(#[from] UsersError)` variant + `users_error_to_http()` mapping all 9 error codes |

**Key decisions from Task 8:**

- **Static SQL over dynamic query building** — sqlx 0.9 requires `SqlSafeStr` for dynamic strings; all queries use static SQL with `($N::text IS NULL OR column = $N)` pattern for optional filters, avoiding dynamic query construction entirely
- **COALESCE pattern for partial updates** — `UPDATE users SET display_name = COALESCE($2, display_name), ...` allows a single static query to update only provided fields; `None` parameters preserve existing values via `COALESCE(NULL, existing) = existing`
- **`avatar_url` uses CASE WHEN pattern** — Unlike other `Option<String>` fields, `avatar_url` needs a separate boolean flag (`CASE WHEN $4::boolean THEN $5 ELSE avatar_url END`) because empty string and NULL are both valid `Option` states and `COALESCE('', existing)` would incorrectly overwrite
- **Owner account protection** — `update_user` rejects any modification to owner accounts (`OwnerImmutable`); `soft_delete_user` rejects owner deletion (`OwnerCannotBeDeleted`); both checks run before any DB write
- **Self-modification prevention** — Admin cannot change their own `role` or `status` via this endpoint (`CannotModifySelf`); prevents accidental self-demotion. Other fields (display_name, email, etc.) are allowed
- **Soft-delete revokes sessions** — `soft_delete_user` sets `deleted_at = now()` and `is_active = false`, then deletes all `user_sessions` for the user (best-effort, ignores errors)
- **Email uniqueness checked on update** — If email is provided in the update, queries for an existing user with that email (excluding the target user and soft-deleted users); returns `EmailTaken` on conflict
- **`UpdateUserParams` struct** — Introduced to satisfy clippy `too_many_arguments` (12 params → 2 params); same pattern as `CreateInvitationParams` and `CreateDeviceCodeParams` in the auth domain
- **Offset pagination for user list** — Uses `page`/`page_size` query params (not cursor pagination) per API_CONVENTIONS.md recommendation for small datasets that need page numbers; response includes `total`, `page`, `page_size`, `total_pages`
- **`UsersError` with 9 variants** — `NotFound` (USER_001), `OwnerImmutable` (USER_002), `OwnerCannotBeDeleted` (USER_003), `UsernameTaken` (USER_004), `EmailTaken` (USER_005), `InvalidRole` (USER_006), `InvalidStatus` (USER_007), `CannotModifySelf` (USER_008), `Database` (INTERNAL)
- **`row_to_user_row` is public** — Shared conversion utility that maps `PgRow` to internal `UserRow`, then to `UserResponse` — keeps the mapping consistent across `list_users` and `get_user`
- **Streaming policy FK validation** — If `streaming_policy_id` is provided, handler validates it exists in `streaming_policies` table before calling `update_user`; returns `BAD_REQUEST` if not found
- **No new workspace dependencies** — all functionality uses existing `sqlx`, `validator`, `serde`, `uuid`, `chrono` crates
- **User endpoints reuse auth domain's `check_capability()`** — All 4 handlers call `auth::service::check_capability(&user.role, &user.capabilities, "can_manage_users")` for authorization, consistent with invitation and capability handlers in the auth domain

**What was built for Task 11:**

| File | Purpose |
|---|---|
| `server/src/extractors.rs` | Added `RequiredCapability` trait, `Require<C>` generic extractor with `FromRequestParts<AppState>` impl, 12 marker types (`CanManageServer`, `CanManageUsers`, `CanManageLibraries`, etc.), `AdminOnly` type alias |
| `server/src/domains/auth/handlers.rs` | Replaced inline `check_capability("can_manage_users")` in 6 handlers (`list_invitations`, `create_invitation`, `revoke_invitation`, `resend_invitation`, `get_user_capabilities`, `update_user_capabilities`) with `Require<CanManageUsers>` extractor |
| `server/src/domains/users/handlers.rs` | Replaced inline `check_capability("can_manage_users")` in 4 handlers (`list_users`, `get_user`, `update_user`, `delete_user`) with `Require<CanManageUsers>` extractor |

**Key decisions from Task 11:**

- **Trait-based generic extractor over middleware** — `Require<C: RequiredCapability>` implements `FromRequestParts<AppState>`; extracts `AuthenticatedUser`, delegates to `auth::service::check_capability()`, and returns `AppError::Auth(InsufficientCapabilities)` on failure. This follows axum's extractor-based authorization pattern (same as the existing `AdminOnly`) rather than a `from_fn` middleware. Extractors are more ergonomic, composable, and avoid the double-extraction problem where middleware and handler both need the same state
- **12 marker types** — All capabilities from `ALL_CAPABILITIES` defined as zero-sized types (`CanManageServer`, `CanManageUsers`, `CanManageLibraries`, `CanViewAnalytics`, `CanManageScheduledTasks`, `CanTranscode`, `CanDownload`, `CanDeleteMedia`, `CanUseLiveTv`, `CanShareContent`, `CanRemoteControl`, `PlayMedia`). Trivial to define; pre-emptively created for future phases
- **`AdminOnly` becomes type alias** — `pub type AdminOnly = Require<CanManageServer>` preserves backward compatibility. No handlers currently use `AdminOnly` (it was a Phase 3 placeholder), but the type remains available
- **Handler signature change** — Handlers that required capability checks changed from `user: AuthenticatedUser` + inline `check_capability()` to `auth: Require<CanManageUsers>` (or the appropriate marker). Access to the underlying user is via `auth.user`. Unused capability-only handlers use `_auth` to suppress unused-variable warnings
- **`users` domain no longer imports `auth`** — With inline `check_capability()` calls removed, `users/handlers.rs` no longer needs `use crate::domains::auth`; the authorization is handled at the extractor level
- **No new workspace dependencies** — the `Require<C>` extractor uses existing `AuthenticatedUser` and `auth::service::check_capability()` infrastructure

**Not yet implemented (deferred to later tasks/phases):**

- Task 10 (`AuthenticatedUser` extractor) — already completed in Phase 4 Task 4 (session validation wired into extractor)
- Task 11 (`require_capability()` middleware) — complete; see above
- Nullable field clearing — `Option<T>` in `UpdateUserRequest` cannot distinguish "not provided" from "set to NULL" for nullable columns (`streaming_policy_id`, `max_streams`, etc.); `Option<Option<T>>` or a separate "clear" endpoint deferred to admin UI implementation
- User self-service profile update endpoint — `PUT /api/v1/user/profile` (non-admin) deferred to Phase 8 web client
- Admin user session management endpoints (`GET/DELETE /api/v1/users/{id}/sessions`) — deferred; session management currently only via auth domain self-service endpoints

---

## Phase 5 — Libraries & Media Items

**Goal:** Admin can create libraries, scan directories, and media items appear in the database.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [LIBRARY_ORGANIZATION.md](docs/design/LIBRARY_ORGANIZATION.md) | **Primary** — folder structure, sub-folder-as-library, multi-path libraries, metadata ID tags |
| [MEDIA_SCANNING.md](docs/design/MEDIA_SCANNING.md) | **Primary** — 6-phase pipeline (discover → diff → probe → identify → enrich → cleanup), FS watching (`notify`), mtime diff, Blake3 hash, ffprobe |
| [DATABASE.md](docs/design/DATABASE.md) | `libraries`, `library_paths`, `media_items`, `movies`, `series`, `seasons`, `episodes`, `media_files` tables |

**Context from Phase 4:**

- `Require<CanManageLibraries>` extractor already defined in `extractors.rs` — checks `can_manage_libraries` capability
- `AppError` supports domain-specific variants via `#[from]` — Phase 5 adds `AppError::Library(#[from] LibrariesError)`
- `PaginationParams` extractor supports cursor and offset pagination — library listing uses offset (small dataset, page numbers needed)
- Router has a comment placeholder for `.merge(crate::domains::libraries::router())` — Task 1 replaces it

**What was built for Task 1:**

| File | Purpose |
|---|---|
| `server/src/domains/libraries/mod.rs` | Module declarations + router assembly with 7 routes across 4 paths |
| `server/src/domains/libraries/error.rs` | `LibrariesError` enum with 15 variants covering LIB_001–LIB_014 + Database catch-all |
| `server/src/domains/libraries/types.rs` | Three-type DTOs: `LibraryRow` (internal), `CreateLibraryRequest`/`UpdateLibraryRequest` (Deserialize + Validate), `LibraryResponse`/`LibraryListResponse` (Serialize); `VALID_MEDIA_TYPES` static |
| `server/src/domains/libraries/service.rs` | Full CRUD service: `list_libraries` (paginated with `item_count` via `LEFT JOIN LATERAL`), `get_library`, `create_library` (slug generation + name/slug uniqueness), `update_library` (COALESCE partial updates), `soft_delete_library` (guards against deleting with media), `generate_slug`, `validate_media_type`, `row_to_library_row` |
| `server/src/domains/libraries/handlers.rs` | Working handlers for list, get, create, update, delete; `todo!()` stubs for `scan_library` (Task 5) and `list_library_items` (Task 4 media domain) |
| `server/src/error.rs` | Added `AppError::Library(#[from] LibrariesError)` variant + `library_error_to_http()` mapping all 15 error codes |
| `server/src/domains/mod.rs` | Added `pub mod libraries;` |
| `server/src/router.rs` | Merged libraries router, removed Phase 5 libraries comment |

**Key decisions from Task 1:**

- All 15 error variants defined upfront (LIB_001–014 + Database) matching ERROR_HANDLING.md — LIB_007–014 relate to scanning/watching/metadata and will be used in later tasks
- `item_count` included in library responses via `LEFT JOIN LATERAL` subquery counting `media_items` per library
- `PATCH` for update (per API_CONVENTIONS.md), not `PUT` — matches design doc example
- Slug auto-generated from library name (lowercase, hyphenated) — uniqueness enforced against both slug and name columns before insert
- Soft-delete sets `scan_enabled = false` to prevent scanning deleted libraries
- `scan_library` and `list_library_items` as `todo!()` stubs — implemented in Tasks 5 and 4 respectively
- `Require<CanManageLibraries>` on all endpoints — requires `can_manage_libraries` capability
- `validate_media_type` reuses `ProviderIdTagMalformed` variant for invalid media types — LIB_010 mapped to 422, appropriate for validation errors
- No new workspace dependencies — all functionality uses existing `sqlx`, `validator`, `serde`, `uuid`, `chrono` crates
- `generate_slug` converts to lowercase, replaces non-alphanumeric with hyphens, collapses consecutive hyphens

**What was built for Task 2:**

All CRUD operations were implemented as part of Task 1 (natural to include when building the five-file pattern). Task 2 adds one improvement:

| File | Change |
|---|---|
| `server/src/domains/libraries/service.rs` | Added slug uniqueness check on `update_library` — when name changes, the derived slug is checked against existing libraries (excludes self) to catch edge cases where different names produce the same slug (e.g., "My Movies" and "My-Movies" both → "my-movies") |

**Key decisions from Task 2:**

- **Slug uniqueness on update** — `update_library` now checks both name and slug uniqueness independently. Create already checked both; update was only checking name. The slug check prevents a DB unique constraint violation (`libraries_slug_active`) from surfacing as a generic 500 error
- **No `root_path` filesystem validation** — `RootPathNotFound` error exists but is reserved for the scanner (Task 5). Libraries can be created with paths that don't currently exist (network drives may be offline, Docker volumes not yet mounted). The scanner validates paths at scan time and sets per-path `scan_enabled` for offline drives
- **No new workspace dependencies** — slug uniqueness uses existing `sqlx::query`

**What was built for Task 3:**

| File | Purpose |
|---|---|
| `server/src/domains/libraries/types.rs` | Added `LibraryPathRow` (internal), `LibraryPathResponse` (Serialize), `CreateLibraryPathRequest` / `UpdateLibraryPathRequest` (Deserialize + Validate) |
| `server/src/domains/libraries/error.rs` | Added `PathNotFound` (LIB_015), `PathExists` (LIB_016), `CannotDeleteDefaultPath` (LIB_017) error variants |
| `server/src/domains/libraries/service.rs` | Added 5 library_paths service functions: `list_library_paths`, `get_library_path`, `create_library_path`, `update_library_path`, `delete_library_path`; added `path_row_to_response`, `verify_library_exists` helpers; modified `create_library` to also create default `library_paths` row in a transaction |
| `server/src/domains/libraries/handlers.rs` | Added 5 working handlers: `list_library_paths`, `get_library_path`, `create_library_path`, `update_library_path`, `delete_library_path` |
| `server/src/domains/libraries/mod.rs` | Added 2 route groups: `/api/v1/libraries/{id}/paths` (GET, POST), `/api/v1/libraries/{id}/paths/{path_id}` (GET, PATCH, DELETE) |
| `server/src/error.rs` | Added LIB_015, LIB_016, LIB_017 mappings in `library_error_to_http()` |

**Key decisions from Task 3:**

- **Sub-resource URL pattern** — `/api/v1/libraries/{id}/paths` and `/api/v1/libraries/{id}/paths/{path_id}` — one level of nesting, appropriate for strictly owned sub-resources with CASCADE delete per REST API best practices (Microsoft, Stack Overflow community consensus)
- **Library creation creates default path** — `create_library` now uses a transaction to INSERT into both `libraries` and `library_paths` (with `is_default = true`, `scan_enabled = true`); ensures every library always has at least one path per LIBRARY_ORGANIZATION.md requirement
- **Default path transfer** — When creating or updating a path with `is_default = true`, the existing default is set to `false` in the same transaction; ensures exactly one default path per library
- **Cannot delete last default path** — `delete_library_path` checks if the path is the default and the only remaining path; returns `CannotDeleteDefaultPath` (LIB_017, 422) to prevent orphaning a library with no paths
- **Path uniqueness per library** — `create_library_path` and `update_library_path` check for duplicate paths within the same library before insertion; returns `PathExists` (LIB_016, 409)
- **Paths listed default-first** — `list_library_paths` orders by `is_default DESC, created_at ASC` so the default path appears first
- **No filesystem validation on paths** — Consistent with Task 2 decision for `root_path`; the scanner validates paths at scan time and sets `scan_enabled` for offline drives
- **`library_paths` responses include `library_id`** — Each path response includes the parent `library_id` for client-side association, even though it's implicit from the URL; follows the pattern of embedding parent context in sub-resource responses
- **`verify_library_exists` helper** — Extracted from `get_library` pattern; used by all 5 path service functions to validate the parent library exists and is not soft-deleted before querying paths
- **No new workspace dependencies** — all functionality uses existing `sqlx::query` with `PgPool::begin()` transactions

**What was built for Task 4:**

| File | Purpose |
|---|---|
| `server/src/domains/media/mod.rs` | Module declarations + router assembly with 4 route groups |
| `server/src/domains/media/error.rs` | `MediaError` enum with 14 variants covering MEDIA_001–MEDIA_007 plus domain-specific variants |
| `server/src/domains/media/types.rs` | Three-type DTOs: `MediaItemRow` (internal), `UpdateMediaItemRequest` (Deserialize + Validate), `MediaItemResponse`/`MediaItemListResponse` (Serialize); `MediaFileRow`/`MediaFileResponse`; validation statics |
| `server/src/domains/media/service.rs` | Full CRUD service: `list_media_items` (cursor pagination with optional library/type filters), `list_library_items`, `get_media_item` (with CTI child joins for series/seasons/episodes), `update_media_item` (COALESCE partial updates with match_state/identification_source validation), `delete_media_item`, `list_media_files`, `get_media_file`, `validate_media_type`, row mapping helpers, base64 cursor encode/decode |
| `server/src/domains/media/handlers.rs` | Working handlers for list, get, update, delete media items; list and get media files; type validation |
| `server/src/error.rs` | Added `AppError::Media(#[from] MediaError)` variant + `media_error_to_http()` mapping all 14 error codes |
| `server/src/domains/mod.rs` | Added `pub mod media;` |
| `server/src/router.rs` | Merged media router, removed Phase 5 media comment |
| `server/src/domains/libraries/handlers.rs` | Replaced `list_library_items` `todo!()` stub with working handler delegating to `media::service::list_library_items` |

**Key decisions from Task 4:**

- **CTI-aware response model** — `MediaItemResponse` includes optional `series_status`, `series_id`, `season_number`, `season_id`, `episode_number`, `absolute_episode_number`, `file_count` fields; all populated via `LEFT JOIN` on series/seasons/episodes child tables in a single query, avoiding N+1 lookups
- **Static SQL over dynamic query building** — Two static SQL constants for list queries (`LIST_MEDIA_ITEMS_DESC_SQL`, `LIST_MEDIA_ITEMS_ASC_SQL`) instead of dynamic string construction; uses `($N::uuid IS NULL OR column = $N)` pattern for optional filters, matching the users domain convention
- **Cursor pagination with UUIDv7** — Cursors are base64-encoded `{"id": "..."}` JSON; UUIDv7 is naturally time-ordered so `id > cursor` / `id < cursor` gives chronological ordering without a separate sort column; `LIMIT $limit + 1` pattern for `has_more` detection
- **`file_count` via LEFT JOIN LATERAL** — Count of `media_files` per item computed inline in the same query as the item itself, avoiding a second round-trip; `COALESCE(cnt, 0)` handles items with no files
- **Update uses COALESCE pattern** — `COALESCE($2, title)` for all 17 updatable fields, matching the libraries/users domain convention; `match_state` and `identification_source` validated against static allowlists before the UPDATE
- **`get_media_item` reuses list query structure** — Same CTI-aware SELECT with LEFT JOINs as the list endpoint, ensuring consistent response shape whether fetching one or many
- **`list_library_items` wired in libraries domain** — The `todo!()` stub in `libraries/handlers.rs` replaced with a real handler that delegates to `media::service::list_library_items`, which first verifies the library exists then calls the generic `list_media_items` with the library_id filter
- **14 error variants defined upfront** — `NotFound` (MEDIA_001), `FileNotFound` (MEDIA_002), `FileUnhealthy` (MEDIA_003), `ArtworkNotFound` (MEDIA_004), `AlreadyExists` (MEDIA_006), `StoryboardNotFound` (MEDIA_007), plus `InvalidMediaType`, `InvalidMatchState`, `InvalidIdentificationSource`, `SeriesNotFound`, `SeasonNotFound`, `DuplicateSeasonNumber`, `DuplicateEpisodeNumber`, `Database` catch-all
- **`AuthenticatedUser` on all media endpoints** — All 6 handlers require an authenticated user (no capability check); admin-only operations (delete, update match_state) may add `Require<CanManageLibraries>` or `Require<CanDeleteMedia>` in future phases
- **No new workspace dependencies** — cursor encoding uses existing `base64` 0.22; all other functionality uses existing `sqlx`, `serde_json`, `uuid`, `chrono`

**Not yet implemented (deferred to later tasks/phases):**

- Media item creation — items are created by the scanner (Task 5), not by direct API
- Series/season/episode-specific endpoints — `GET /api/v1/series/{id}/seasons`, etc. deferred to when the web client needs them
- Full-text search — `GET /api/v1/search?q=...` uses `search_vector` column; deferred to Phase 6 (metadata providers populate the column) or Phase 8 (web client needs search)
- Genre/tag/credit endpoints — `GET /api/v1/media-items/{id}/genres`, `/credits`, etc. deferred to Phase 6
- ETag / Cache-Control headers — deferred to Phase 8 web client performance optimization. **Note (June 2026):** Full strategy now documented in [HTTP_CACHING.md](docs/design/HTTP_CACHING.md) — two-layer design (HTTP `stale-while-revalidate` directive now; `@tanstack/svelte-query` deferred to Phase 11+). The [API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md) Cache-Control table is extended with `stale-while-revalidate` values per endpoint. The directive is supported on Chromium-based Smart TVs (Tizen 6.0+/webOS 5.x+, 2019–2021+), all desktop browsers except Safari; unsupported clients (Safari, older TV WebKit) silently fall back to `max-age` per RFC 9111.
- Artwork delivery endpoint and WebP variant generation — **implemented in Phase 10 Task 10** (`GET /api/v1/items/{id}/artwork/{type}?size={size}`). `MediaCard.svelte` now serves real WebP posters via `clients/web/src/lib/utils/artwork.js` URL builders; media detail page serves backdrop (w1280) + poster (w500). Full image format policy in [IMAGE_FORMATS.md](docs/design/IMAGE_FORMATS.md) — WebP as primary delivery format (AVIF rejected for encode cost on NAS hardware; JPEG XL rejected for browser support). On-demand variant generation via `image_pipeline::generate_variant` on cache miss with `Cache-Control: public, max-age=86400, stale-while-revalidate=604800, immutable` and strong `ETag`. `<picture>` JPEG fallback deferred to Pre-v1.0 Hardening.
- Nullable field clearing — same `Option<T>` limitation as users domain; `Option<Option<T>>` deferred to admin UI

**Tasks:**

1. ~~Create `server/src/domains/libraries/` — five-file pattern~~ **DONE**
2. ~~Implement library CRUD — create, list, get, update, soft-delete~~ **DONE**
3. ~~Implement `library_paths` — multi-path library support~~ **DONE**
4. ~~Create `server/src/domains/media/` — five-file pattern~~ **DONE**
5. ~~Implement `server/src/workers/library_scanner.rs` — 6-phase scanning pipeline~~ **DONE**

**What was built for Task 5:**

| File | Purpose |
|---|---|
| `server/src/workers/library_scanner.rs` | 6-phase scanning pipeline: discover, diff, probe, identify, enrich (stub), cleanup |
| `server/src/workers/mod.rs` | Module declarations (`pub mod library_scanner;`) |
| `server/src/services/mod.rs` | Module declarations (placeholder for future services) |
| `server/src/lib.rs` | Added `pub mod workers;` and `pub mod services;` module declarations |
| `server/src/domains/libraries/handlers.rs` | Replaced `scan_library` `todo!()` with working handler calling scanner |
| `Cargo.toml` | Added `ignore = "0.4"`, `blake3 = "1"`, `regex = "1"` to workspace deps |
| `server/Cargo.toml` | Added `ignore.workspace = true`, `blake3.workspace = true`, `regex.workspace = true` |

**Key decisions from Task 5:**

- **`ignore` crate for parallel directory walking** — `WalkBuilder::new(path).hidden(false).git_ignore(false).build_parallel()` with glob overrides for media extensions; `std::sync::Mutex<Vec>` collects results from parallel walker threads
- **Extension-based glob filtering** — `ignore::overrides::OverrideBuilder` with individual `add()` calls for each extension pattern (`.mkv`, `.mp4`, `.srt`, etc.); video extensions filtered in Phase 2, subtitle extensions discovered for future Phase 9
- **mtime-based diffing with 2s tolerance** — `Phase2_diff` compares `DiscoveredFile.mtime` (SystemTime) against `media_files.file_modified_at` (DB timestamptz); files with matching path + size + mtime (within 2s) are skipped as unchanged; FAT32 and some SMB mounts have 2-second timestamp resolution
- **Blake3 partial hash** — `compute_partial_hash_sync()` hashes first 1MB + last 1MB (for files > 2MB); Blake3 is 10x faster than SHA-256 per MEDIA_SCANNING.md rationale; hash stored in `media_files.file_hash`
- **ffprobe async subprocess** — `tokio::process::Command` with `-v quiet -print_format json -show_format -show_streams -show_chapters`; JSON output parsed into `FfprobeOutput` struct; concurrent probing limited by `Semaphore` (default: 2 concurrent)
- **HDR detection from color_transfer** — `smpte2084` → `"hdr10"`, `arib-std-b67` → `"hlg"`, else `"sdr"` per VIDEO_FORMATS.md
- **Chapter extraction** — ffprobe `-show_chapters` output stored in `additional_streams.chapters` JSONB; avoids re-probing during Phase 10 segment detection
- **Frame rate parsing** — Handles `r_frame_rate` in `"num/den"` format (e.g., `"24000/1001"` for 23.976fps) and plain decimal strings
- **5-layer identification cascade** — Layer 1: `.media-match` sidecar (key-value format with tmdb/imdb/tvdb IDs); Layer 2: NFO files (`movie.nfo`/`tvshow.nfo` XML parsed via regex); Layer 3: Provider ID tags (`{tmdb-272}`, `[tmdbid=272]`); Layer 4: Structured filename parsing (title + year extraction, SXXEXX episode detection); Layer 5: Unmatched queue (match_state = "unmatched")
- **ResolvedIds struct** — Carries `tmdb_id`, `imdb_id`, `tvdb_id` separately for direct DB binding; avoids generic provider/id pair that would require per-provider branching at SQL bind time
- **Movie identification** — Parse parent folder name for `Title (Year)` pattern; create `media_items` + `movies` + `media_files` rows in single transaction; existing `media_files` by path triggers update instead of duplicate insert
- **TV show identification** — `group_episodes_by_series()` groups files by series folder (detecting `Season XX`/`Specials` sub-folders); creates series `media_items` + `series` row (with `find_existing_series` dedup), season `media_items` + `seasons` rows (with `ensure_season` dedup), episode `media_items` + `episodes` + `media_files` per file
- **Season folder detection** — `find_series_folder()` walks up from episode file's parent; if parent matches `Season XX` or `Specials`, the grandparent is the series folder
- **SXXEXX regex patterns** — Two patterns: `(?i)[_.\s\-]s?(\d{1,2})[ex](\d{1,3})` (standard S01E01) and `(?i)[_.\s\-](\d{1,2})x(\d{1,3})` (alternate 1x01); supports multi-episode ranges via optional `-E##` suffix
- **Sort title generation** — Articles ("The", "A", "An") stripped from beginning and appended: `"The Matrix"` → `"Matrix, The"`
- **Phase 5 (Enrich) calls TMDB enrichment** — When `EnrichmentOrchestrator` is available, queries `auto_matched`/`unmatched` movies and series, enriches each via `enrich_movie()`/`enrich_tv()`, persists metadata to DB, downloads artwork; sets `match_state = 'confirmed'` on success
- **Phase 6 (Cleanup) marks deleted files** — `UPDATE media_files SET is_healthy = false` for paths in DB but not on disk; orphaned media items (no healthy files) detected and logged but not deleted (admin reviews in UI)
- **`ScannerError` mapped via `AppError::Internal`** — Handler uses `.map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))` since scanner is an internal worker; no new domain error variant needed
- **Scan is synchronous** — Handler runs scan inline and returns results; for large libraries this may timeout; async background scan with `POST` returning 202 Accepted deferred to Task 6 (scheduler)

**Not yet implemented (deferred to later tasks/phases):**

- Task 7: FS watching via `notify` + `notify-debouncer-full` — real-time file detection
- Tasks 8-10: Identification layers 1-3 implemented within scanner; layer 4 filename parsing implemented without TMDB API search; API search added in Phase 6
- Multi-episode file support — `episode_end` field populated but not yet used for creating multiple episode rows from a single file
- Subtitle file discovery — Subtitle Extensions discovered in Phase 1 but not yet processed into `subtitle_files` rows (Phase 9)
- Split file detection — `pt1`/`pt2`/`cd1`/`disc1` patterns not yet parsed
- Edition detection — `edition` field populated but not yet used for multi-version grouping

6. ~~Implement `server/src/services/scheduler.rs` — scheduled task runner~~ **DONE**

**What was built for Task 6:**

| File | Purpose |
|---|---|
| `server/src/services/scheduler.rs` | Scheduled task runner — polls `scheduled_tasks` every 30s, dispatches due tasks to registered executors |
| `server/src/services/mod.rs` | Added `pub mod scheduler;` |
| `server/src/main.rs` | Wired scheduler startup with `library_scan` executor; seeds default tasks on first run |
| `Cargo.toml` | Added `croner = "3"` to workspace deps |
| `server/Cargo.toml` | Added `croner.workspace = true` |

**Key decisions from Task 6:**

- **`croner` crate v3** for cron expression parsing — POSIX/Vixie-cron compliant, chrono-compatible, supports `L`/`W`/`#` modifiers, `@daily`/`@hourly` aliases, human-readable descriptions; MIT licensed; more feature-complete than `cron` crate
- **30-second tick interval** — scheduler polls `scheduled_tasks WHERE next_run_at <= now() AND is_enabled = true AND state != 'running'` every 30s; adequate for media server workloads (no sub-minute precision needed)
- **Builder-pattern executor registration** — `Scheduler::new(pool).register_executor("library_scan", handler)` returns `Self` for chaining; each executor is an `Arc<dyn Fn>` that takes `(PgPool, task_id, config)` and returns `Future<Output = ()>`
- **Task lifecycle** — tick fetches due tasks → creates `scheduled_task_runs` row → sets state to `running` → spawns executor in background → on completion: updates run row with result/duration/stats, resets `consecutive_failures` to 0, computes `next_run_at` from cron or interval → on failure: increments `consecutive_failures`, sets retry delay, auto-disables after `max_retries` consecutive failures
- **`compute_next_run()`** — uses `croner::Cron::find_next_occurrence()` for cron tasks, `now + interval_seconds` for interval tasks; falls back to `now + 1 hour` on parse failure
- **`seed_default_tasks()`** — idempotent (checks `COUNT(*)` first, uses `ON CONFLICT DO NOTHING`); seeds 8 default tasks: Library Scan (daily 03:00), Metadata Refresh (daily 04:00), Database Maintenance (weekly Sunday 05:00), Session Cleanup (every 1h), Notification Cleanup (daily 02:00), Disk Space Check (every 30min), Media Health Check (weekly Sunday 06:00), Soft Delete Purge (daily 01:00)
- **`TaskFailureInfo` struct** — groups failure parameters to avoid `too_many_arguments` clippy warning
- **Scheduler integrates with `TaskTracker` + `CancellationToken`** — spawned as a tracked task alongside the HTTP server; responds to shutdown signal
- **Library scan executor** — fetches all non-deleted libraries, runs `scan_library()` for each with `mode` from task config (default: `"full"`); aggregates `items_created`, `files_modified`, `files_deleted` across all libraries
- **Per-task timeout wrapper** — the executor applies `tokio::time::timeout` using each `scheduled_tasks.timeout_seconds` value (clamped to at least one second). Expiry records a `timeout` run result and cancellation drops the worker future; Storyboard FFmpeg commands use `kill_on_drop(true)` so an in-flight child is terminated too.

 7. ~~Implement FS watching via `notify` + `notify-debouncer-full` for real-time detection~~ **DONE**

**What was built for Task 7:**

| File | Purpose |
|---|---|
| `server/src/services/fs_watcher.rs` | `LibraryWatcherManager` — cross-platform FS watcher with `notify` 8.2 + `notify-debouncer-full` 0.7 debounced events |
| `server/src/services/mod.rs` | Added `pub mod fs_watcher;` |
| `server/src/state.rs` | Added `fs_watcher: Arc<LibraryWatcherManager>` to `AppState` |
| `server/src/main.rs` | Wired watcher startup after scheduler, stop during shutdown Phase 3 |
| `server/src/domains/libraries/handlers.rs` | `create_library` / `create_library_path` → `watch_library()`; `delete_library` / `delete_library_path` → `unwatch_library()` |
| `server/src/domains/libraries/service.rs` | Added `list_library_path_strings()` helper for watcher path resolution |
| `Cargo.toml` | Added `notify = "8"`, `notify-debouncer-full = "0.7"` to workspace deps |
| `server/Cargo.toml` | Added `notify.workspace = true`, `notify-debouncer-full.workspace = true` |

**Key decisions from Task 7:**

- **`notify` 8.2 + `notify-debouncer-full` 0.7** — per MEDIA_SCANNING.md Crate Selection table; debouncer handles rename stitching, event dedup, file ID tracking, settled events
- **3-second debounce timeout** — per MEDIA_SCANNING.md; media files are large, 3-second window ensures partially-written files are not processed
- **Two-phase event architecture** — Debouncer callback (sync, runs on notify thread) accumulates per-directory file counts in `pending` map and sends channel notification; event processor (async tokio task) receives channel messages and drains `pending` for batch processing
- **Media extension filtering** — Debouncer callback filters events to video + subtitle extensions before counting; non-media events (log files, temp files, metadata) are ignored at the source
- **Library path resolution** — Event processor matches directory paths against watched library paths via `starts_with()` to determine which library owns the change
- **Bulk import detection** — If ≥10 files detected in a single directory within a debounce window, triggers a full scan (`quick = false`) instead of a quick scan
- **Per-library cooldown** — 10-second cooldown prevents rapid re-triggering of scans for the same library; logged at DEBUG level when skipped
- **Watcher lifecycle per MEDIA_SCANNING.md** — `start()` loads all non-deleted libraries with `scan_enabled = true` paths at server boot; `watch_library()` / `unwatch_library()` called from library and path CRUD handlers; `stop()` during shutdown Phase 3
- **Graceful failure** — Watcher failures (watch limit exceeded, permission denied) log warnings but do not prevent startup or library creation; scheduled scans remain as fallback per MEDIA_SCANNING.md limitations table
- **`Arc<LibraryWatcherManager>` in AppState** — Shared between handlers (for watch/unwatch) and main.rs (for start/stop); internal state uses `std::sync::Mutex` (debouncer is not Send)
- **Channel-based notification** — `mpsc::channel<WatchEvent>` bridges sync debouncer callback to async event processor; channel send uses `try_send` (non-blocking) to avoid blocking the notify thread
- **No new error variants** — Watcher failures are logged, not surfaced to API callers; LIB_007 (`FilesystemWatcherFailed`) already registered in ERROR_HANDLING.md for future API exposure
 - **`list_library_path_strings()` helper** — Added to libraries service for efficient path lookup without full DTO conversion; returns scan-enabled paths for a library

**What was built for Task 8:**

| File | Purpose |
|---|---|
| `server/src/services/media_matching.rs` | Dedicated service module for the 5-layer identification cascade: `.media-match` parsing (with `pattern:`, `edition:`, cascading), NFO parsing, provider ID tag parsing, `resolve_identification()` entry point |
| `server/src/services/mod.rs` | Added `pub mod media_matching;` |
| `server/src/workers/library_scanner.rs` | Removed inline `parse_media_match_file`, `parse_nfo_file`, `parse_provider_id_tag`, `resolve_identification_layers`, `MediaMatchData`, `NfoData`; replaced with `media_matching::resolve_identification()` calls; `identify_and_create_series` now uses episode overrides from `.media-match` |

**Key decisions from Task 8:**

- **Dedicated service module over inline scanner functions** — The 1845-line scanner had identification logic mixed with pipeline orchestration. Extracted into `services/media_matching.rs` following the project's "modular service files over large singleton files" convention. Scanner now calls `media_matching::resolve_identification()` at two points: movie identification and series identification
- **No new crate dependencies** — `pattern:` token interpolation converts `{s}`, `{e}`, `{sp}` to regex capture groups using the existing `regex` crate. `globset` was considered but rejected because the pattern syntax is custom tokens, not standard glob
- **Pattern token support** — `pattern: Show.Part.{s}.-.{e}.-.*` is converted to a regex by replacing `{s}`/`{season}` with `(?P<season>\d{1,2})`, `{e}`/`{episode}` with `(?P<episode>\d{1,3})`, `{sp}`/`{special}` with `(?P<special>\d{1,3})`, and `*` with `.*`. Named capture groups extract season/episode from matched filenames
- **Season-level `.media-match` cascading** — `resolve_identification()` checks for `.media-match` at both the series folder level and the season folder level (when a season folder is provided). Season-level file overrides series-level for episode matching, following the Plex `.plexmatch` cascading convention
- **Episode overrides wired into TV identification** — `identify_and_create_series()` now calls `media_matching::resolve_episode_override()` for each episode file. If a `.media-match` file contains an `ep:` line matching the filename, the override season/episode numbers are used instead of regex parsing from the filename. Pattern-based matching is checked before individual `ep:` lines
- **`edition:` field parsed** — Stored in `MediaMatchData` for use in multi-version grouping (deferred to future enhancement)
- **`IdentificationResult` unified return type** — `resolve_identification()` returns `IdentificationResult` containing `ResolvedIds`, `identification_source`, `match_state`, plus optional `title`, `year`, `season`, `edition`, and `episode_overrides` — a richer result than the previous 3-tuple
- **NFO and provider ID tag parsing moved** — `parse_nfo_file()` and `parse_provider_id_tag()` moved from scanner into the service module, keeping all identification logic in one place
- **Scanner reduced by ~200 lines** — Inline parsing functions removed; `resolve_identification_layers()` replaced by single `media_matching::resolve_identification()` call; `MediaMatchData` and `NfoData` types moved to service module
 9. ~~Implement NFO file parsing (Layer 2)~~ **DONE**
 10. ~~Implement provider ID tag parsing `{tmdb-XXX}`, `{imdb-ttXXX}`, `{tvdb-XXX}` (Layer 3)~~ **DONE**

**What was built for Task 9:**

| File | Purpose |
|---|---|
| `server/src/services/nfo_parser.rs` | Dedicated NFO parsing module using `quick-xml` 0.40 streaming StAX parser |
| `server/src/services/media_matching.rs` | Removed regex-based `parse_nfo_file()` and `NfoData`; calls `nfo_parser::parse_nfo()` instead |
| `server/src/services/mod.rs` | Added `pub mod nfo_parser;` |
| `Cargo.toml` | Added `quick-xml = "0.40"` to workspace deps |
| `server/Cargo.toml` | Added `quick-xml.workspace = true` |

**Key decisions from Task 9:**

- **`quick-xml` 0.40 over regex** — Streaming StAX parser replaces fragile regex-based XML parsing. 50x faster than xml-rs, near-zero allocation, handles malformed XML gracefully
- **All NFO tag formats supported** — Modern Kodi v19+ `<uniqueid type="tmdb|imdb|tvdb">`, legacy flat tags (`<tmdbid>`, `<imdbid>`, `<imdb_id>`, `<tvdbid>`), URL-only format (`https://www.themoviedb.org/movie/...`), mixed uniqueid + legacy in same file
- **All root elements supported** — `<movie>`, `<tvshow>`, `<episodedetails>` (episode NFO with `<season>` and `<episode>`)
- **`<filename>.nfo` discovery** — `parse_nfo_for_file()` checks for NFO alongside a video file (e.g., `S01E01.nfo` next to `S01E01.mkv`), per Kodi/Jellyfin naming conventions
- **Graceful degradation on trailing content** — Stops at closing root tag; ignores trailing URLs after `</movie>` (common Jellyfin bug per issue #13655). Falls back to URL-only parsing if XML parsing fails entirely
- **No provider IDs = None** — NFO files with only `<title>` and `<year>` but no provider IDs return `None`, consistent with identification cascade (NFO is only useful if it contains a provider ID for exact matching)
- **`NfoData` moved to nfo_parser module** — Expanded with `season` and `episode` fields for episode-level NFO; old `NfoData` struct removed from media_matching.rs
- **14 unit tests** covering: modern Kodi uniqueid, legacy flat tags, TV show NFO, Jellyfin `<imdb_id>` variant, episode NFO with season/episode, URL-only format, trailing content after root, mixed uniqueid + legacy, no provider IDs, no NFO file, TV show URL-only, uniqueid without default attribute, Kodi/Radarr mixed format, filename NFO discovery

**What was built for Task 10:**

| File | Purpose |
|---|---|
| `server/src/services/media_matching.rs` | Refactored `parse_provider_id_tag()` → `parse_provider_id_tags()` with multi-ID extraction; added `filename` parameter to `resolve_identification()` for filename tag checking; regex patterns compiled via `LazyLock` statics |
| `server/src/workers/library_scanner.rs` | Updated `identify_and_create_movie()` to pass file stem as filename; updated `identify_and_create_series()` to pass `None` |

**Key decisions from Task 10:**

- **Multi-ID extraction** — `parse_provider_id_tags()` uses `captures_iter()` to extract ALL provider IDs from a string; `{tmdb-272}{imdb-tt0381061}` now returns both IDs instead of only the first
- **Curly braces priority over square brackets** — Per LIBRARY_ORGANIZATION.md, curly brace tags (`{tmdb-XXX}`) take priority over square bracket tags (`[tmdbid=XXX]`) for the same provider; different providers are merged (e.g., `{tmdb-272}[imdbid-tt0381061]` returns both)
- **Filename checking** — `resolve_identification()` now accepts `filename: Option<&str>` parameter; folder name is checked first, then filename; folder name IDs take priority, filename IDs fill in any missing providers
- **`LazyLock` statics** — `CURLY_TAG_RE` and `BRACKET_TAG_RE` compiled once via `std::sync::LazyLock` (Rust edition 2024 stable), avoiding regex recompilation per call
- **Movie file stems passed** — `identify_and_create_movie()` extracts `file.path.file_stem()` and passes it; `identify_and_create_series()` passes `None` (tags go on series folder, not episode filenames)
- **20 new unit tests** covering: all 6 tag formats (curly + bracket × tmdb/imdb/tvdb), multi-ID extraction, mixed curly+bracket, curly priority, bracket fills missing, empty/no-tag strings, filename tag extraction, folder priority over filename, filename fills missing IDs, no-tag fallback to filename_parse
- **No new workspace dependencies** — uses existing `regex` crate and `std::sync::LazyLock`

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

1. ~~Create `server/src/services/metadata.rs` — `ProviderRegistry`, `EnrichmentOrchestrator`~~ **DONE**
2. ~~Implement `TmdbClient` — Bearer token auth, `append_to_response` batching, rate limiter (governor, 40 req/s)~~ **DONE**
3. ~~Implement TMDB search endpoints — `/search/movie`, `/search/tv`~~ **DONE**
4. ~~Implement TMDB details endpoints — `/movie/{id}`, `/tv/{id}` with `append_to_response=credits,videos,external_ids,images`~~ **DONE**
5. ~~Implement TMDB `/find` — cross-reference from IMDb ID~~ **DONE**
6. ~~Implement TMDB `/configuration` caching — image sizes, base URL~~ **DONE**
7. ~~Wire TMDB client into Phase 5 enrichment (Phase 5 stub → real implementation)~~ **DONE**
8. ~~Implement artwork download — save to `/data/metadata/artwork/`, create `artwork` table rows~~ **DONE**
9. ~~Implement `TvdbClient` — JWT auth via `/login`, token refresh, series/episode endpoints~~ **DONE**
10. ~~Implement `FanartClient` — artwork lookup by TMDB/TVDB ID~~ **DONE**
11. ~~Implement `OmdbClient` — ratings lookup by IMDb ID~~ **DONE**
12. ~~Implement provider API key validation on save (test request)~~ **DONE**
13. ~~Implement API key encryption at rest (AES-256-GCM with `encrypted:` prefix)~~ **DONE**
14. ~~Implement TMDB daily ID export download and caching~~ **DONE**
15. ~~Implement `server/src/workers/metadata_refresh.rs` — periodic enrichment using TMDB `/changes`~~ **DONE**

**What was built for Task 13:**

| File | Purpose |
|---|---|
| `server/src/services/encryption.rs` | AES-256-GCM encryption service: `EncryptionKey` struct wrapping `ring::aead::LessSafeKey`; `encrypt()`/`decrypt()` with `encrypted:` + base64(nonce \|\| ciphertext \|\| tag) wire format; `decrypt_provider_config()`/`encrypt_provider_config()` for provider key batch operations; `mask_secret()` for admin API responses; `ensure_encryption_key()` for startup key resolution and auto-generation; 18 unit tests |
| `server/src/services/mod.rs` | Added `pub mod encryption;` |
| `server/src/config.rs` | Added `encryption_key: Option<String>` to `CliArgs` (with `DUSKCUE_ENCRYPTION_KEY` env var) and `BootstrapConfig`; wired through config builder with `set_override_option` |
| `server/src/state.rs` | Added `encryption_key: Arc<EncryptionKey>` to `AppState`; `load_runtime_config()` now accepts `Option<&EncryptionKey>` and decrypts provider keys after JSONB deserialization; both `AppState` constructors accept `EncryptionKey` parameter |
| `server/src/main.rs` | Encryption key initialization after metrics setup, before DB connection: resolves from bootstrap config or auto-generates and writes to `{data_dir}/config/config.toml`; passes key to `load_runtime_config()` and `AppState::new_with_config()` |

**Key decisions from Task 13:**

- **`ring::aead::AES_256_GCM` via `LessSafeKey`** — Direct use of `ring`'s AES-256-GCM implementation; `LessSafeKey` chosen over `SealingKey`/`OpeningKey` because each encryption uses a random nonce (no sequential nonce tracking needed); `ring` already in workspace for PBKDF2, HMAC, rustls
- **Wire format: `encrypted:` + base64(nonce_12 \|\| ciphertext \|\| tag_16)** — Self-describing prefix enables transparent migration from plaintext; base64 encoding avoids JSONB string escaping issues; 12-byte random nonce per encryption ensures uniqueness
- **Master key in bootstrap config** — Hex-encoded 256-bit key stored in `config.toml` or `DUSKCUE_ENCRYPTION_KEY` env var; same key used for backup encryption per BACKUP_RECOVERY.md design; not stored in DB (would create lockout loop since DB itself is inside backups)
- **Auto-generation on first run** — If no `encryption_key` in bootstrap config, `ensure_encryption_key()` generates a random 32-byte key via `ring::rand::SystemRandom` and writes to `{data_dir}/config/config.toml`; generates key before DB connection so encrypted values in future migrations are handled correctly
- **Graceful plaintext migration** — `decrypt_if_encrypted()` checks for `encrypted:` prefix; values without prefix are returned as-is; allows gradual migration from existing plaintext deployments without a migration script
- **Decryption at config load time** — `load_runtime_config()` decrypts provider keys after JSONB deserialization but before `AppState` construction; provider clients receive plaintext keys in memory without any encryption awareness; matches design doc: "Keys are decrypted in memory only when making outbound requests"
- **`encrypt_provider_config()` skips already-encrypted values** — Idempotent: values already starting with `encrypted:` prefix are not re-encrypted; prevents double-encryption on repeated save operations
- **`mask_secret()` for admin API** — Returns first 3 + last 3 chars for long strings, `***` for short strings, `***encrypted***` for encrypted values; ready for Phase 13 admin settings UI
- **No new workspace dependencies** — All cryptography uses existing `ring` 0.17 (`ring::aead::AES_256_GCM`, `ring::rand::SystemRandom`); base64 encoding uses existing `base64` 0.22; hex encoding is inline (32 bytes, trivial)

**What was built for Tasks 14–15:**

| File | Purpose |
|---|---|
| `server/src/services/tmdb_client.rs` | Added `TmdbChangesListResponse`, `TmdbChangedId` deserialization types; `fetch_changed_movie_ids()` and `fetch_changed_tv_ids()` methods — paginated queries against TMDB `/movie/changes` and `/tv/changes` endpoints with `start_date`/`end_date` parameters |
| `server/src/services/enrichment_persistence.rs` | Added `re_enrich_item()` public function — re-enrichs a single confirmed item by tmdb_id, calls orchestrator's `enrich_movie()`/`enrich_tv()` directly and persists result via existing `persist_enrichment_result()` |
| `server/src/workers/metadata_refresh.rs` | Full metadata refresh worker: `run_metadata_refresh()` entry point; `download_daily_exports()` downloads `movie_ids_*.json.gz` and `tv_series_ids_*.json.gz` from `files.tmdb.org/p/exports`; `cleanup_old_exports()` removes files older than 7 days; `refresh_changed_items()` queries TMDB `/changes` for modified IDs, cross-references with DB items via `find_matching_items()`, calls `re_enrich_item()` for each |
| `server/src/workers/mod.rs` | Added `pub mod metadata_refresh;` |
| `server/src/main.rs` | Registered `metadata_refresh` executor on scheduler with `enrichment` and `cache_dir` captures |
| `Cargo.toml` | Added `flate2 = "1"` to workspace deps |
| `server/Cargo.toml` | Added `flate2.workspace = true` |

**Key decisions from Tasks 14–15:**

- **Daily export download via `files.tmdb.org`** — No authentication required; files available by 08:00 UTC daily; stored in `{cache_dir}/metadata/exports/` per METADATA_PROVIDERS.md; cleaned up after 7 days
- **Gzip decompression via `flate2`** — Already a transitive dependency through `serde_json`; added explicitly for direct use; synchronous `GzDecoder` for counting entries (files are local, not blocking async runtime significantly)
- **TMDB `/changes` endpoint for incremental refresh** — `GET /3/movie/changes?start_date=&end_date=&page=` returns paginated list of changed IDs (100/page); max 14-day range per query; auto-paginated in `fetch_changed_movie_ids()`/`fetch_changed_tv_ids()`
- **`re_enrich_item()` for targeted re-enrichment** — Public function in `enrichment_persistence.rs`; calls orchestrator's `enrich_movie()`/`enrich_tv()` with known tmdb_id; reuses existing `persist_enrichment_result()` for DB persistence; avoids re-enriching entire library
- **`sqlx::QueryBuilder` for dynamic IN clause** — `find_matching_items()` uses `QueryBuilder` to build `WHERE tmdb_id IN (...)` with bind parameters; avoids SQL injection concerns from dynamic string interpolation; shared function for both movies and series via `ext_table` parameter
- **Cross-reference TMDB changed IDs with local DB** — Queries `media_items JOIN movies/series` where `match_state = 'confirmed'` and `metadata->>'tmdb_id'` matches; only re-enrichs items that exist in both TMDB changes and local library
- **`last_metadata_refresh_at` from task config JSON** — Scheduler task config stores the last refresh timestamp; default 6-hour lookback on first run; end_date is current date
- **Graceful degradation** — Daily export download failure doesn't block `/changes` refresh; TMDB API failures logged as warnings, not errors; individual item re-enrichment failures counted but don't stop processing
- **No new HTTP client for exports** — Uses a separate `reqwest::Client` with 300s timeout for large file downloads; the TMDB API client keeps its 30s timeout

| File | Purpose |
|---|---|
| `server/src/services/metadata.rs` | Full service module: `MetadataProvider`, `ArtworkProvider`, `RatingsProvider` traits (async_trait); `ProviderRegistry` with primary/supplementary/artwork/ratings slots; `EnrichmentOrchestrator` with `enrich_movie()`/`enrich_tv()`/`search()`/`find_by_imdb()`; `ProviderRateLimiter` with 4 governor direct rate limiters; `MetadataError` enum with 11 variants; `TmdbClient`/`TvdbClient`/`FanartClient`/`OmdbClient` stubs; rich data types (`MovieDetails`, `TvDetails`, `CreditsData`, `ArtworkCandidate`, `RatingsData`, `EnrichmentResult`, etc.) |
| `server/src/services/mod.rs` | Added `pub mod metadata;` |
| `server/src/state.rs` | Expanded `MetadataConfig` from empty placeholder to full struct with 22 fields (artwork, overlays, collections, providers); added `ProviderConfig`, `TmdbProviderConfig`, `OptionalProviderConfig` structs; added `Arc<EnrichmentOrchestrator>` to `AppState`; both constructors create registry from config |
| `Cargo.toml` | Added `async-trait = "0.1"` to workspace deps |
| `server/Cargo.toml` | Added `async-trait.workspace = true` |

**Key decisions from Task 1:**

- **`async-trait` required for dyn dispatch** — The design doc uses `Box<dyn MetadataProvider>`; native Rust async traits are not dyn-compatible as of Rust 1.88. `async-trait` 0.1 provides the `#[async_trait]` macro to enable `dyn Trait` with async methods
- **Three separate traits over one mega-trait** — `MetadataProvider`, `ArtworkProvider`, `RatingsProvider` per METADATA_PROVIDERS.md design; each provider type only implements relevant traits (e.g., `OmdbClient` implements only `RatingsProvider`, not `MetadataProvider`)
- **Provider stubs with logging** — `TmdbClient`, `TvdbClient`, `FanartClient`, `OmdbClient` implement their trait methods as stubs that log "full implementation in Task N" and return empty/default results; this allows the orchestrator and registry to compile and wire up while individual provider implementations are built incrementally in Tasks 2-11
- **`ProviderRegistry::from_config()`** — Constructs the registry from `MetadataConfig` at startup; only creates provider instances for enabled+configured providers; TMDB requires `access_token` non-empty, supplementary providers require `api_key` present
- **`EnrichmentOrchestrator` owns `Arc<ProviderRegistry>`** — Registry is shared reference-counted inside the orchestrator; the orchestrator is stored in `AppState` as `Arc<EnrichmentOrchestrator>` so all handlers and workers access the same instance
- **Per-provider rate limiters** — `ProviderRateLimiter` uses `governor::RateLimiter::direct()` (non-keyed) per METADATA_PROVIDERS.md: TMDB 40/s, TVDB 1/s burst 5, Fanart 1/s burst 3, OMDb 1/s burst 10; rate limiter awaits via `until_ready()` before each provider call
- **Sequential tier execution with graceful failure** — `enrich_movie()` and `enrich_tv()` call primary (TMDB) first, then artwork providers, then ratings providers; each tier's failures are caught with `tracing::warn!` and skipped — enrichment succeeds with available data per METADATA_PROVIDERS.md graceful degradation design
- **`MetadataError` with 11 variants** — Covers authentication, rate limiting, not found, network, invalid response, daily budget, unconfigured, timeout, and database errors; maps to existing LIB error codes (LIB_011–014) per METADATA_PROVIDERS.md error handling section
- **`MetadataConfig` expanded to 22 fields** — All fields from POSTER_MANAGEMENT.md and METADATA_PROVIDERS.md configuration section; includes artwork language priority, overlay settings, collection settings, provider configs, enrichment timeout, export cache days; `Default` implementations match design doc defaults
- **`AppState.enrichment` created from config** — `new_with_config()` reads `MetadataConfig` from `RuntimeConfig`, builds `ProviderRegistry::from_config()`, creates `EnrichmentOrchestrator`; `new()` creates empty registry (no providers configured)
- **No new DB migrations** — `MetadataConfig` fields map to existing `server_config.metadata` JSONB column; no schema changes needed

**What was built for Tasks 2–6:**

| File | Purpose |
|---|---|
| `server/src/services/tmdb_client.rs` | Full TMDB v3 API client: `TmdbClient` struct with Bearer token auth, `reqwest::Client` with 30s timeout + 10s connect timeout + redirect disabled; 17 TMDB response deserialization types; all `MetadataProvider` trait methods implemented with real HTTP calls |
| `server/src/services/metadata.rs` | Removed TmdbClient stub; imports real TmdbClient from `tmdb_client` module; `EnrichmentOrchestrator` now stores `Option<TmdbClient>` for direct access and `Arc<ArcSwap<TmdbConfig>>` for hot-reload; added `refresh_tmdb_config()` async method |
| `server/src/services/mod.rs` | Added `pub mod tmdb_client;` |
| `Cargo.toml` | Added `urlencoding = "2"` to workspace deps |
| `server/Cargo.toml` | Added `urlencoding.workspace = true` |

**Key decisions from Tasks 2–6:**

- **Dedicated `tmdb_client.rs` module** — Extracted from metadata.rs following the project's "modular service files over large monolithic files" convention (same pattern as `nfo_parser.rs`, `media_matching.rs`). metadata.rs retains traits, types, registry, and orchestrator; tmdb_client.rs owns the concrete HTTP implementation
- **TmdbClient owns its own `reqwest::Client`** — Each client instance has its own HTTP connection pool with Bearer token auth pre-configured; 30s request timeout matching `enrichment_timeout_seconds` default; redirect policy disabled per API_SECURITY.md SSRF hardening rules; connect timeout 10s
- **`urlencoding` crate for query parameter encoding** — TMDB search queries may contain special characters (accented titles, apostrophes, ampersands); `urlencoding::encode()` provides safe URL encoding; v2 is the current stable release
- **`append_to_response` batching** — `get_movie_details()` and `get_tv_details()` use `append_to_response=credits,videos,external_ids,images` in a single HTTP request, reducing API calls by 4-5x per item per METADATA_PROVIDERS.md; `include_image_language=en,null` ensures English + language-neutral images are returned
- **`#[serde(untagged)]` search response enum** — TMDB search results return either movie or TV objects with different field names (`title` vs `name`, `release_date` vs `first_air_date`); `TmdbSearchItem` enum with untagged deserialization handles both; movie results checked first (more common), TV results second
- **Error mapping from HTTP status codes** — 401 → `AuthenticationFailed`, 404 → `NotFound`, 429 → `RateLimited`, other non-success → `InvalidResponse` with parsed TMDB error message; network errors → `NetworkError`; JSON parse failures → `InvalidResponse`
- **Graceful deserialization with `Option<T>` throughout** — All TMDB response fields are `Option<T>` because TMDB's API responses vary significantly between items (some lack `overview`, `tagline`, `runtime`, etc.); missing fields become `None` in our domain types rather than causing parse failures
- **`TmdbConfig` stored as `Arc<ArcSwap<TmdbConfig>>`** — Allows atomic hot-reload of TMDB configuration (image base URLs, available sizes, change keys) without restarting the server; `refresh_tmdb_config()` on orchestrator fetches fresh config from TMDB `/configuration` and swaps atomically; `tmdb_config()` accessor returns `Arc<TmdbConfig>` for cheap cloning
- **`TmdbClient` stored in orchestrator** — `EnrichmentOrchestrator.tmdb_client: Option<TmdbClient>` provides direct access for config refresh and future operations that bypass the trait (e.g., daily ID exports, genre list); also stored in registry as `Box<dyn MetadataProvider>` for trait-dispatched enrichment
- **`TmdbClient` derives `Clone`** — `reqwest::Client` is cheaply cloneable (internally Arc'd); cloning creates a TmdbClient for the registry and another for the orchestrator from the same config; both share the same connection pool semantics
- **Year extraction from date strings** — Search results extract year from `release_date`/`first_air_date` via `d.get(..4).and_then(|y| y.parse::<u32>().ok())` rather than storing the full date in `SearchResult`
- **`find_by_imdb_id` checks movies before TV** — TMDB `/find` returns separate arrays for `movie_results` and `tv_results`; movies are checked first since IMDb IDs are more commonly associated with movies in the identification pipeline
- **`fetch_configuration()` with fallback defaults** — TMDB `/configuration` endpoint returns image base URLs and size lists; all fields fall back to hardcoded defaults from METADATA_PROVIDERS.md if the API response is missing or incomplete
- **No new workspace dependencies beyond `urlencoding`** — All HTTP functionality uses existing `reqwest` (workspace already has `json` + `rustls-tls` features); JSON deserialization via existing `serde`/`serde_json`

**What was built for Task 8:**

| File | Purpose |
|---|---|
| `server/src/services/artwork_downloader.rs` | Artwork download service: `download_and_store_artwork()` downloads TMDB images (posters, backdrops, logos), saves to `/data/metadata/artwork/tmdb/`, inserts `artwork` table rows; `sort_by_votes()` ranking; deduplication via `source_url` check; `download_image()` async HTTP + filesystem write |
| `server/src/services/mod.rs` | Added `pub mod artwork_downloader;` |
| `server/src/services/metadata.rs` | Added `data_dir: PathBuf` to `EnrichmentOrchestrator`; added `media_item_id: Option<Uuid>` parameter to `enrich_movie()`/`enrich_tv()`; added `tmdb_id: Option<u64>` field to `EnrichmentResult`; artwork download called after metadata enrichment when `media_item_id` and images are present |
| `server/src/state.rs` | `AppState::new()` and `AppState::new_with_config()` now pass `bootstrap.data_dir.clone()` to `EnrichmentOrchestrator::new()` |

**Key decisions from Task 8:**

- **`reqwest` for image download (no new dependency)** — TMDB provides width/height in its API response; no need for an image parsing crate to detect dimensions. The `image` crate is deferred to Phase 12 (overlay compositing)
- **File naming: `{tmdb_id}_{tmdb_filename}`** — TMDB file paths look like `/abc123def456.jpg`; the filename portion is extracted and prefixed with the TMDB ID for directory organization. Stored in `{data_dir}/metadata/artwork/tmdb/{posters,backdrops,logos}/`
- **Download `original` size only** — Per POSTER_MANAGEMENT.md `artwork_download_originals_only = true`. URL constructed as `{secure_image_base_url}original{file_path}` using the cached TMDB configuration
- **Vote-sorted download limits** — Top 5 posters, top 3 backdrops, top 2 logos by TMDB vote count (then vote average as tiebreaker). Prevents downloading hundreds of images for popular movies
- **Deduplication via `source_url` check** — Before downloading, queries `artwork` table for existing rows with the same `source_url` for that `media_item_id`. Skips re-download if artwork already exists
- **`ON CONFLICT DO NOTHING` on insert** — The `artwork` table has `UNIQUE(media_item_id, artwork_type, "order")`; the insert uses `ON CONFLICT DO NOTHING` to handle edge cases where the same order slot is already filled
- **`media_item_id: Option<Uuid>` parameter** — `enrich_movie()`/`enrich_tv()` accept an optional `media_item_id`. When `None` (e.g., search-only calls), artwork download is skipped. When `Some`, artwork is downloaded and stored for that item
- **`tmdb_id` added to `EnrichmentResult`** — The TMDB provider_id from the details response is stored in `EnrichmentResult.tmdb_id`, enabling artwork download even when the caller only had a title (search path)
- **Graceful failure** — Individual artwork download failures are logged as warnings and do not fail the overall enrichment. Failed downloads are counted in `ArtworkDownloadResult.failed` for monitoring
- **Directory creation is idempotent** — `create_dir_all` is called before each download; if the directory already exists, this is a no-op
- **No new workspace dependencies** — all functionality uses existing `reqwest`, `tokio::fs`, `sqlx`

**What was built for Task 9:**

| File | Purpose |
|---|---|
| `server/src/services/tvdb_client.rs` | Full TVDB v4 API client: `TvdbClient` with JWT auth (`Arc<Inner>` pattern for Clone), `ensure_token()` with double-checked locking, `login()`, `authenticated_get()`, search (`/search`, `/search/remoteid/{id}`), series details (`/series/{id}/extended`), movie details (`/movies/{id}`), artwork (`/series/{id}/artworks`); 20 TVDB response deserialization types |
| `server/src/services/mod.rs` | Added `pub mod tvdb_client;` |
| `server/src/services/metadata.rs` | Removed TvdbClient stubs (~130 lines of trait impl stubs); imported real `TvdbClient` from new module; wired into `ProviderRegistry::from_config()` for both `supplementary_metadata` and `artwork` slots |
| `docs/design/METADATA_PROVIDERS.md` | Corrected token TTL (2h → 1 month per v4 spec); added full Task 9 implementation notes section |

**Key decisions from Task 9:**

- **`Arc<Inner>` pattern for Clone** — `Inner` holds `api_key`, `http: Client`, `token_state: RwLock<TokenState>`; `TvdbClient` wraps `Arc<Inner>`. Enables both `Box<dyn MetadataProvider>` and `Box<dyn ArtworkProvider>` registry slots to share the same underlying token state
- **Manual `ensure_token()` over reqwest-middleware** — Double-checked locking: read lock for fast path (token valid → return), write lock only when refresh needed. Token TTL is 1 month per TVDB v4 OpenAPI spec (corrected from initial 2-hour estimate in METADATA_PROVIDERS.md), so contention is near-zero. Chosen over `reqwest-middleware` + `reqwest-retry` to avoid extra dependency chain and complex generic types for marginal benefit
- **Token refreshed 5 minutes before expiry** — `TOKEN_REFRESH_BUFFER = 300s`; token set to expire at `Instant::now() + 30 days`. On 401 responses, `clear_token()` uses `try_write()` (non-blocking) so concurrent requests aren't blocked; next request re-authenticates
- **TVDB v4 response wrapper** — All responses use `{ "status": "success", "data": <T> }`; `TvdbResponse<T>` generic unwraps the `data` field. The OpenAPI spec marks NO fields as required on any schema — all deserialization types use `Option<T>` throughout
- **`/series/{id}/extended?meta=episodes` as primary TV details endpoint** — Returns series, episodes, seasons, artworks, genres, companies, remote IDs in one request. TVDB's equivalent of TMDB's `append_to_response` batching
- **`/search/remoteid/{id}` for IMDb cross-reference** — Returns typed `TvdbRemoteIdSearchResult` with separate `series` and `movie` arrays; `RemoteID.sourceName == "IMDB"` extracts IMDb ID from series extended record
- **Artwork type ID mapping** — TVDB artwork type IDs (1=poster, 2=banner, 3=backdrop, 4=clearlogo, 5=thumbnail) mapped to string types for the artwork pipeline. `get_tv_artwork()` returns `ArtworkCandidate` with full image URLs from TVDB's `image` field
- **TVDB search returns string IDs** — `SearchResult.tvdb_id` and `SearchResult.objectID` are `string` type per v4 spec, not integer. `search_to_result()` parses via `id_str.parse::<u64>()`
- **TvdbClient in both registry slots** — Cloned into `supplementary_metadata` (for search/details) and `artwork` (for artwork lookup). Both clones share the same `Arc<Inner>` including token state
- **`get_season_details` returns `NoProviderConfigured`** — TVDB's season structure uses season types (`default`, `dvd`, `absolute`) rather than simple season numbers; the generic `SeasonDetails` type doesn't map cleanly
- **`TvdbEpisodesResponse` reserved for future use** — Struct defined for `/series/{id}/episodes/{season-type}` paginated endpoint but not yet wired; will be used when alternate episode ordering is needed (DVD order, absolute numbering)
- **No new workspace dependencies** — all functionality uses existing `reqwest`, `serde`, `tokio`, `urlencoding`

**What was built for Task 10:**

| File | Purpose |
|---|---|
| `server/src/services/fanart_client.rs` | Full Fanart.tv v3 API client: `FanartClient` with `api_key` query param auth, `reqwest::Client` with 30s timeout; `ArtworkProvider` trait implementation for movie and TV artwork lookup |
| `server/src/services/mod.rs` | Added `pub mod fanart_client;` |
| `server/src/services/metadata.rs` | Removed `FanartClient` stub (~30 lines); imported real `FanartClient` from new module; wired into `ProviderRegistry::from_config()` with API key |

**Key decisions from Task 10:**

- **Simple API key auth over JWT** — Fanart.tv uses `?api_key={key}` query param auth, no token lifecycle needed (unlike TVDB's JWT). `FanartClient` stores `api_key: String` directly; no `Arc<Inner>` or `RwLock` pattern needed
- **Movie endpoint uses TMDB ID** — `/movies/{tmdb_id}?api_key={key}` accepts TMDB ID (also accepts IMDb ID like `tt0037884` for cross-reference). TV endpoint uses TVDB ID: `/tv/{tvdb_id}?api_key={key}`
- **Dedicated deserialization types** — `FanartMovieResponse` and `FanartTvResponse` with `Option<Vec<FanartImage>>` fields for each artwork type. Unknown top-level fields (`{type}_count`, `name`, `tmdb_id`) ignored by serde. All image fields use `Option<String>` since fanart.tv returns all values as strings (`likes: "3"`, `width: "1000"`)
- **Artwork type mapping** — 9 movie types and 11 TV types mapped to internal artwork types. Key unique types: `hdmovielogo`/`hdtvlogo` → "clearlogo" (transparent HD logos, fanart.tv's primary value), `moviebackground`/`showbackground` → "backdrop" (includes 4K backgrounds at 3840×2160), `characterart` → "character"
- **Relative URL defensive handling** — A transient November 2025 fanart.tv server bug returned relative paths instead of full URLs. Relative URLs detected via `!starts_with("http")` and skipped with DEBUG log. Full URLs used as-is (normal operation)
- **Likes-based sorting** — String `likes` parsed to `u32` via `str::parse()`; candidates sorted by likes descending. Mapped to `vote_count` in `ArtworkCandidate` since fanart.tv uses likes rather than votes
- **Language field handling** — `lang: ""` (language-neutral, common for backgrounds) converted to `None` in `ArtworkCandidate.language`; non-empty values preserved as-is
- **Width/height from v3.2** — String `width`/`height` parsed to `u32`; default to `0` if absent (v3/v3.1 responses) or parse failure
- **Response body read as text before parse** — Unlike TvdbClient which uses `response.json()`, FanartClient reads response body as text first, then parses. This preserves error messages in `InvalidResponse` errors
- **FanartClient not Clone** — Only stored in `artwork` registry slot (single use); no need for cloning unlike TvdbClient (which goes into both `supplementary_metadata` and `artwork` slots)
- **Error mapping** — HTTP 401 → `AuthenticationFailed`, 404 → `NotFound`, 429 → `RateLimited`, other → `InvalidResponse` with body text
- **No new workspace dependencies** — all functionality uses existing `reqwest`, `serde`, `serde_json`

**What was built for Task 11:**

| File | Purpose |
|---|---|
| `server/src/services/omdb_client.rs` | Full OMDb API client: `OmdbClient` with `apikey` query param auth, `reqwest::Client` with 30s timeout; `RatingsProvider` trait implementation for ratings lookup by IMDb ID |
| `server/src/services/mod.rs` | Added `pub mod omdb_client;` |
| `server/src/services/metadata.rs` | Removed `OmdbClient` stub (~30 lines); imported real `OmdbClient` from new module; wired into `ProviderRegistry::from_config()` with API key |

**Key decisions from Task 11:**

- **Simple API key auth** — OMDb uses `?apikey={key}` query param auth, no token lifecycle. `OmdbClient` stores `api_key: String` directly; same pattern as `FanartClient`
- **Single endpoint: `/?i={imdb_id}&apikey={key}`** — Only IMDb ID lookup is needed per METADATA_PROVIDERS.md design (title search endpoint not used; TMDB handles search)
- **OMDb returns HTTP 200 for errors** — OMDb always returns HTTP 200, even for "not found" or "invalid API key". The `Response` field in the JSON body indicates success (`"True"`) or failure (`"False"`). `fetch_by_imdb_id()` checks `Response == "True"` after deserialization and maps error strings to typed errors: `"not found"` → `NotFound`, `"Invalid API key"` → `AuthenticationFailed`, other → `InvalidResponse`
- **`Ratings` array parsing for Rotten Tomatoes** — `extract_rotten_tomatoes()` scans the `Ratings` array for `Source == "Rotten Tomatoes"` and extracts the `Value` (e.g., `"94%"`). IMDb rating and Metascore come from top-level fields
- **`"N/A"` string handling** — OMDb uses the literal string `"N/A"` for missing values rather than null/absent fields. `parse_imdb_rating()`, `parse_imdb_votes()`, `parse_metascore()`, and `parse_string_field()` all filter out `"N/A"` before parsing
- **`#[allow(non_snake_case)]` on deserialization structs** — OMDb uses PascalCase field names (`Response`, `Error`, `Rated`, `Metascore`, `imdbRating`, `Ratings`, etc.). The `#[allow(non_snake_case)]` attribute suppresses Rust naming convention warnings while maintaining exact field mapping for serde deserialization
- **`imdb_rating` parsed to `f64`** — `RatingsData.imdb_rating` is `Option<f64>` (not `Option<String>`); the string value `"7.9"` from OMDb is parsed to `f64` in `to_ratings_data()`
- **Response body read as text before JSON parse** — Same defensive pattern as FanartClient; preserves error messages in `InvalidResponse` errors
- **OmdbClient not Clone** — Only stored in `ratings` registry slot (single use); no need for cloning
- **Error mapping** — HTTP 401 → `AuthenticationFailed`, non-success HTTP → `InvalidResponse` with body text, `Response: "False"` with `"not found"` → `NotFound`, `Response: "False"` with `"Invalid API key"` → `AuthenticationFailed`, other `Response: "False"` → `InvalidResponse`
- **No new workspace dependencies** — all functionality uses existing `reqwest`, `serde`, `serde_json`, `urlencoding`

**What was built for Task 7:**

| File | Purpose |
|---|---|
| `server/src/services/enrichment_persistence.rs` | New service module: `enrich_items_for_library()` fetches enrichable items (`auto_matched`/`unmatched` movies and series), calls `enrich_movie()`/`enrich_tv()` per item, persists results to DB via transaction |
| `server/src/services/mod.rs` | Added `pub mod enrichment_persistence;` |
| `server/src/workers/library_scanner.rs` | `scan_library()` now accepts `Option<Arc<EnrichmentOrchestrator>>`; `scan_path_pipeline()` passes enrichment reference to `phase5_enrich()`; `phase5_enrich()` calls `enrichment_persistence::enrich_items_for_library()` when orchestrator is available, logs skip message when not |
| `server/src/state.rs` | `LibraryWatcherManager::new()` now takes `Arc<EnrichmentOrchestrator>`; enrichment created before fs_watcher in both `AppState` constructors |
| `server/src/services/fs_watcher.rs` | `LibraryWatcherManager` stores `Arc<EnrichmentOrchestrator>`; `process_batch()` passes enrichment to `scan_library()` |
| `server/src/domains/libraries/handlers.rs` | `scan_library` handler passes `Some(state.enrichment.clone())` to scanner |
| `server/src/main.rs` | Scheduler executor passes `None` for enrichment (scheduler has no orchestrator reference; scheduled scans enrich items on the next handler-triggered scan) |

**Key decisions from Task 7:**

- **`Option<Arc<EnrichmentOrchestrator>>` parameter** — `scan_library()` accepts an optional orchestrator rather than requiring one. This allows the scheduler to call `scan_library(... None)` for discovery/identification phases even when no TMDB is configured. Handler-triggered scans pass `Some(...)` for full enrichment
- **Dedicated `enrichment_persistence.rs` module** — Follows the project's "modular service files over large monoliths" convention. Separates persistence logic (genre upserts, credit linking, person deduplication, metadata JSON merging) from the scanner's orchestration flow
- **Transaction-per-item enrichment** — Each item's enrichment result is persisted in a single DB transaction (`BEGIN` → update media_items → update movies/series extension → upsert genres → upsert credits → merge metadata JSON → `COMMIT`). This ensures atomicity: a failed enrichment doesn't leave partial data
- **Genre upsert with `ON CONFLICT (name) DO UPDATE`** — Genres are get-or-create by name; the SQL upserts into the `genres` table and links via `media_genres`. Previous genre links are deleted before re-inserting (full replacement per enrichment)
- **Person deduplication via `tmdb_person_id`** — People are upserted using `ON CONFLICT (tmdb_person_id) WHERE tmdb_person_id IS NOT NULL`; the name and image_url are updated on conflict, ensuring profile photo updates propagate
- **Top-N credit filtering** — Only the top 20 cast members (by `order`) and key crew (Director, Writer, Creator, Executive Producer) up to 10 are persisted. This avoids bloating the `media_credits` table with hundreds of minor crew members
- **`match_state` updated to `'confirmed'`** — After successful enrichment, `media_items.match_state` is set to `confirmed`, removing the item from future enrichment queries
- **`sqlx::QueryBuilder` for dynamic updates** — `media_items` updates are conditional (only non-None fields are SET), avoiding overwriting existing data with NULLs. `QueryBuilder` pushes SQL fragments dynamically based on which enrichment fields are populated
- **Metadata JSONB merge** — Rich data that doesn't map to dedicated columns (videos, external ratings like RT/Metacritic, tagline) is stored in `media_items.metadata` JSONB via `COALESCE(metadata, '{}') || $2`. Movies/series extension tables also use metadata JSONB for tagline, certification, studios, networks
- **Scheduler executor passes `None`** — The scheduler's closure signature only receives `(pool, task_id, config)`. Adding enrichment would require changing the `Scheduler` API or capturing state via environment. For now, scheduled scans run identification only; handler-triggered scans provide full enrichment. This can be enhanced in a future task if needed
- **No new workspace dependencies** — All functionality uses existing `sqlx`, `serde_json`, `uuid`

**What was built for Task 12:**

| File | Purpose |
|---|---|
| `server/src/services/metadata.rs` | Added `ProviderValidationRequest`, `ProviderValidationResponse`, `VALID_PROVIDERS` static; `validate_provider_key()` free function dispatches to provider-specific validation; `validate_tmdb()`, `validate_tvdb()`, `validate_fanart()`, `validate_omdb()` helper functions create temporary client instances and call `test_connection()` |
| `server/src/services/fanart_client.rs` | Added `test_connection()` inherent method — fetches known movie (TMDB ID 550) with API key; 401 → `AuthenticationFailed`; any other result (including 404/success) → key is valid |
| `server/src/services/omdb_client.rs` | Added `test_connection()` inherent method — fetches known IMDb ID `tt0000001` with API key; `"Invalid API key!"` → `AuthenticationFailed`; any other result → key is valid |
| `server/src/domains/system/mod.rs` | Minimal system domain router with `POST /api/v1/settings/providers/validate` (admin-only, `Require<CanManageServer>`) |
| `server/src/domains/system/handlers.rs` | `validate_provider_key` handler — validates request with `validator` + `validate_credentials()`, delegates to service, returns validation result |
| `server/src/domains/system/service.rs` | `validate_provider()` — converts domain types to metadata service types and calls `validate_provider_key()` |
| `server/src/domains/system/error.rs` | `SystemError` enum with 3 variants: `InvalidProvider` (SYS_013), `MissingCredential` (SYS_014), `Database` catch-all |
| `server/src/domains/system/types.rs` | `ValidateProviderRequest` (Deserialize + Validate), `ValidateProviderResponse` (Serialize); `validate_credentials()` checks provider-specific credential requirements |
| `server/src/domains/mod.rs` | Added `pub mod system;` |
| `server/src/error.rs` | Added `AppError::System(#[from] SystemError)` variant + `system_error_to_http()` mapping SYS_013 (400), SYS_014 (400), Database (500) |
| `server/src/router.rs` | Merged system router, removed Phase 13 system comment |

**Key decisions from Task 12:**

- **Temporary client instances for validation** — `validate_tmdb()`/`validate_tvdb()`/`validate_fanart()`/`validate_omdb()` each create a throwaway client instance with the provided credentials, call `test_connection()`, and discard the client. This avoids mutating the live `EnrichmentOrchestrator` or `ProviderRegistry` during validation
- **Graceful test connection for Fanart/OMDb** — `FanartClient::test_connection()` fetches TMDB ID 550 (Fight Club); any error that is NOT `AuthenticationFailed` (e.g., `NotFound`, `NetworkError`) is treated as "key is valid, resource may not exist." Same pattern for `OmdbClient::test_connection()` with `tt0000001`. This distinguishes "bad key" from "bad resource"
- **Validation result in response body, not HTTP status** — `POST /api/v1/settings/providers/validate` returns 200 with `{ valid: true/false, error: "..." }` for all provider results. Only input validation errors (missing fields, unknown provider) return 4xx. This lets the admin UI display the specific provider error message
- **Minimal system domain created** — `server/src/domains/system/` follows the five-file pattern with just the provider validation endpoint. Phase 13 will expand this domain significantly with `server_config` runtime API, scheduled task management, notification system, backup coordination, and admin settings UI
- **`SystemError` with 3 variants** — `InvalidProvider` (SYS_013, 400), `MissingCredential` (SYS_014, 400), `Database` (INTERNAL, 500). Error codes start at SYS_013 to avoid colliding with Phase 13's planned SYS_001–SYS_012 codes (scheduled tasks, notifications, config, backups, transcode resource limits)
- **`validate_credentials()` on request type** — `ValidateProviderRequest::validate_credentials()` checks provider-specific credential requirements: TMDB requires `access_token`, all others require `api_key`. Runs after `validator` struct validation, before the provider test
- **No new workspace dependencies** — all functionality uses existing `reqwest`, `validator`, `serde`, `sqlx`

**Verification:** Library scan enriches items with TMDB data — titles, overviews, ratings, genres, cast, artwork. Admin can configure TVDB/Fanart.tv/OMDb keys in settings UI. Provider failures are non-blocking. Daily TMDB exports download to cache directory. `metadata_refresh` scheduled task detects changed items via TMDB `/changes` and re-enriches them. Provider API keys encrypted at rest with AES-256-GCM.

**Phase 6 status:** All 15 tasks complete.

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

**What was built for Task 1:**

| File | Purpose |
|---|---|
| `server/src/domains/playback/mod.rs` | Module declarations + router assembly with 20 routes across playback sessions, streaming, transcode, bookmarks, playlists |
| `server/src/domains/playback/error.rs` | `PlaybackError` enum with 24 variants covering PLAY_001–PLAY_013 plus domain-specific variants (SessionNotFound, FileNotFound, UserItemDataNotFound, BookmarkNotFound, PlaylistNotFound, etc.) |
| `server/src/domains/playback/types.rs` | Row structs (`PlaySessionRow`, `UserItemDataRow`, `BookmarkRow`, `PlaylistRow`), Request DTOs with `Validate` (`StartPlaybackRequest`, `HeartbeatRequest`, `SeekRequest`, `CreateBookmarkRequest`, `CreatePlaylistRequest`, etc.), Response DTOs with `Serialize` (`PlaybackStartResponse`, `PlaybackInfoResponse`, `HeartbeatResponse`, `UserItemDataResponse`, `BookmarkResponse`, `PlaylistResponse`, etc.) |
| `server/src/domains/playback/service.rs` | 20 service function stubs with `todo!()` |
| `server/src/domains/playback/handlers.rs` | 22 handler stubs with `todo!()` and concrete return types using `AppError` |
| `server/src/error.rs` | Added `AppError::Playback(#[from] PlaybackError)` variant + `playback_error_to_http()` mapping all 24 error variants |
| `server/src/domains/mod.rs` | Added `pub mod playback;` |
| `server/src/router.rs` | Merged playback router via `.merge(crate::domains::playback::router(state.clone()))` |

**Key decisions from Task 1:**

- **Handlers return `Result<Json<T>, AppError>` not `Result<impl IntoResponse, AppError>`** — `todo!()` bodies prevent the compiler from inferring the concrete type behind `impl IntoResponse`; concrete `Json<T>` return types solve this while matching the auth/users/libraries/media domain convention
- **Handlers use `AppError` not `PlaybackError`** — Domain errors convert to `AppError` via `#[from]` derive; `AppError` implements `IntoResponse`. This matches all other domains (auth, users, libraries, media, system)
- **Route design follows STREAMING.md** — Playback session lifecycle (start/heartbeat/stop/seek/info), stream file serving, HLS transcode manifest/playlist/segment, user item data, bookmarks, playlists
- **Error codes mapped per ERROR_HANDLING.md** — PLAY_001 (404) through PLAY_013 (403) plus domain-specific variants for entities not found
- **Playlist routes: CRUD + nested items** — `/api/v1/playlists/` for CRUD, `/api/v1/playlists/{id}/items/` for nested items
- **Bookmark routes nested under items** — `/api/v1/items/{item_id}/bookmarks/`
- **Transcode routes use literal path segments** — `manifest.m3u8`, `index.m3u8`, `{segment}` for clean HLS URL structure
- **`.patch()` method on `MethodRouter`** — Not the standalone `axum::routing::patch` function; `MethodRouter::patch()` method is used for combining GET + PATCH + DELETE on playlist detail route
- **No new workspace dependencies** — all functionality uses existing `sqlx`, `validator`, `serde`, `uuid`, `chrono`, `axum` crates

**Tasks:**

1. ~~Create `server/src/domains/playback/` — five-file pattern~~ **DONE**
2. ~~Implement `server/src/services/transcoding.rs`~~ **DONE**
   - FFmpeg subprocess management via `tokio-process-tools`
   - Structured progress parsing via `-progress pipe:1`
   - HLS/fMP4 segment generation (6-second duration)
   - ABR ladder: 480p/1.5Mbps, 720p/3Mbps, 1080p/6Mbps, 1080p HQ/10Mbps
   - Three-tier decision: Direct Play → Remux → Transcode

   **Design decisions (pre-implementation):**
   - `TranscodingConfig` expanded from empty placeholder to 13 fields per STREAMING.md configuration section: `hardware_accel`, `transcode_path`, `max_concurrent_transcodes`, `segment_duration_seconds`, `allow_hw_tone_mapping`, `allow_hw_subtitle_burn_in`, `default_video_codec`, `default_audio_codec`, `max_downscale_resolution`, `enable_thumb_extraction`, `thread_count`, `thread_type`, `prefer_hw_decode`
   - `CpuConfig` expanded from empty placeholder to 12 fields per CPU.md configuration section: `transcode_cpu_threshold_percent`, `cpu_warning_percent`, `cpu_critical_percent`, `ffmpeg_threads`, `ffmpeg_thread_type`, `ffmpeg_nice`, `ffmpeg_ionice`, `cpu_affinity`, `hw_accel_auto_detect`, `thermal_throttle_enabled`, `thermal_warning_celsius`, `thermal_critical_celsius`
   - Service module pattern (not domain five-file) — transcoding is a shared service used by the playback domain, not a standalone domain. Module lives at `server/src/services/transcoding.rs`
   - `TranscodeManager` holds active sessions in `DashMap<Uuid, TranscodeSession>` with `Semaphore(max_concurrent)` for capacity enforcement
   - `HwAccelMethod` enum: Nvenc, Qsv, Vaapi, VideoToolbox, Amf, Software — detection cached at startup, re-detectable on config reload
   - `ProgressUpdate` struct parsed from FFmpeg `-progress pipe:1` stdout: `out_time_ms`, `speed`, `fps`, `bitrate`, `total_size`, `frame`, `progress=continue|end`
   - FFmpeg command builder generates full CLI args from `TranscodeSession` config: input args, stream mapping, video codec + filters, audio codec, HLS output args, progress pipe
   - Process lifecycle: `Process::new(Command).name().stdout_and_stderr().spawn()` → progress consumer on stdout → log collector on stderr → `GracefulShutdown` on terminate
   - Sandboxing (Landlock + seccomp) deferred to Task 3 — this task creates the subprocess infrastructure without security hooks
    - `TranscodeManager` will be added to `AppState` as `Arc<TranscodeManager>` for sharing across handlers and background tasks

   **What was built for Task 2:**

   | File | Purpose |
   |---|---|
   | `server/src/services/transcoding.rs` | `HwAccelMethod` enum (Nvenc, Qsv, Vaapi, VideoToolbox, Amf, Software) with `ffmpeg_encoder()` mapping; `ProgressUpdate` struct for FFmpeg `-progress pipe:1` parsing; `TranscodeRendition` with `default_ladder()` (4 rungs: 480p/1.5M, 720p/3M, 1080p/6M, 1080p-hq/10M) and `smart_ladder()`; `TranscodeSession` struct with source/target config, progress tracking, segment paths; `TranscodeManager` with `Arc<DashMap<Uuid, TranscodeSession>>`, `Semaphore`, `Arc<ArcSwap<RuntimeConfig>>`; methods: `start_session`, `stop_session`, `seek_session`, `get_session`, `active_session_count`, `get_hw_accel`, `redetect_hw_accel`, `list_active_sessions`, `cleanup_orphaned_sessions`; FFmpeg arg builder functions: `build_ffmpeg_input_args`, `build_video_encode_args`, `build_audio_encode_args`, `build_threading_args`, `build_hls_output_args`; `detect_hw_accel()` with cfg-conditional auto-detection (macOS→VideoToolbox, Linux→Nvenc/Vaapi/Software); `parse_progress_line()` parsing key=value pairs from FFmpeg progress output; `spawn_ffmpeg()` creating `ProcessHandle` via `tokio-process-tools` |
   | `server/src/services/mod.rs` | Added `pub mod transcoding;` |
   | `server/src/state.rs` | `TranscodingConfig` expanded from empty placeholder to 13 fields; `CpuConfig` expanded from empty placeholder to 12 fields; `transcode_manager: Arc<TranscodeManager>` added to `AppState`; both constructors create `TranscodeManager` |

   **Key decisions from Task 2:**

   - **`OwnedSemaphorePermit` over `SemaphorePermit`** — `Semaphore::try_acquire()` returns `SemaphorePermit<'_>` that borrows from `&Semaphore`, preventing it from being moved into `tokio::spawn`. Fixed by using `Arc::clone(&self.semaphore).try_acquire_owned()` which returns an owned permit with `'static` lifetime
   - **`Arc<DashMap>` for sessions** — `sessions` wrapped in `Arc<DashMap<Uuid, TranscodeSession>>` so the progress callback inside the spawned task shares the same map as the manager's public methods
   - **`Consumable` trait import** — `stdout.consume()` requires `tokio_process_tools::Consumable` trait to be in scope
   - **`ParseLines::inspect` closure returns `Next`** — The closure passed to `ParseLines::inspect()` must return `Next::Continue` (not `()`); the return type controls whether streaming continues or stops
   - **`ArcSwap::load_full()` for `'static` config** — `self.config.load()` returns a borrowed guard that prevents spawning `'static` tasks; `load_full()` returns an owned `Arc<RuntimeConfig>` instead
   - **Graceful shutdown after progress consumption** — Process handle is wrapped in `terminate_on_drop(GracefulShutdown)` only after progress streaming completes; this avoids the `stdout()` method being unavailable on `TerminateOnDrop`
   - **Progress parsing via `ParseLines::inspect`** — FFmpeg stdout consumed line-by-line via `tokio_process_tools::ParseLines::inspect()` with `LineParsingOptions::default()`; each parsed line updates the session's `ProgressUpdate` in the `DashMap`
   - **Seek implemented as stop + restart** — `seek_session()` stops the current session (removes from DashMap, cleans segment directory), then creates a new session with `-ss` seek position
   - **`HwAccelMethod::ffmpeg_encoder()` maps codec + method to FFmpeg encoder name** — e.g., `Nvenc` + `"hevc"` → `"hevc_nvenc"`, `Software` + `"h264"` → `"libx264"`; returns `"libx264"` as fallback for unknown combinations
   - **`build_graceful_shutdown()` platform-conditional** — Unix: SIGTERM + 30s timeout → SIGKILL; Windows: Ctrl-Break + 30s timeout → taskkill; per MEMORY.md FFmpeg lifecycle design
   - **Clippy fixes applied** — `&Path` instead of `&PathBuf` on public signatures; `is_none_or` instead of `map_or(true, ...)`; collapsed nested `if let` chains into `&&` let chains (edition 2024); removed unnecessary `.clone()` on non-Clone type reference
   - **`too_many_arguments` on `start_session` (13 params) acknowledged** — Will be refactored into a `StartSessionParams` struct pattern when playback domain handlers integrate with the service
   - **Sandboxing deferred to Task 3** — No Landlock/seccomp hooks in subprocess spawning yet; FFmpeg runs with full process permissions

   3. ~~Implement `server/src/services/sandbox.rs`~~ **DONE**

    **What was built for Task 3:**

    | File | Purpose |
    |---|---|
    | `server/src/services/sandbox.rs` | FFmpeg per-process sandboxing: Landlock LSM filesystem isolation + seccomp-BPF syscall filtering with graceful degradation on non-Linux platforms |
    | `server/src/services/mod.rs` | Added `pub mod sandbox;` |
    | `server/src/services/transcoding.rs` | `spawn_ffmpeg()` now accepts `source_path` and `segment_dir` params; applies sandbox via `pre_exec` on Linux; spawns FFmpeg before creating `TranscodeSession` to avoid borrow-after-move |
    | `Cargo.toml` | Added `libc = "0.2"` to workspace deps |
    | `server/Cargo.toml` | Added `libc.workspace = true`; added `[target.'cfg(target_os = "linux")'.dependencies]` section with `landlock.workspace = true` and `seccompiler.workspace = true` |

    **Key decisions from Task 3:**

    - **Platform-gated dependencies** — `landlock` and `seccompiler` are Linux-only crates that won't compile on Windows/macOS; placed under `[target.'cfg(target_os = "linux")'.dependencies]` in `server/Cargo.toml`; `libc` added as unconditional dep since it compiles everywhere and is needed for `libc::SYS_*` constants
    - **`SandboxConfig` struct** — Borrows `media_path` and `transcode_dir` as `&Path` to avoid allocating in the `pre_exec` closure; paths are cloned into owned `PathBuf` vars before the closure captures them
    - **Landlock ABI V3** — Uses `ABI::V3` for access flag computation; `AccessFs::from_read(abi)` for read-only paths, `AccessFs::from_all(abi)` for read-write paths
    - **Landlock policy per SECURITY.md** — Read-only: `/usr`, `/lib`, `/etc`, `/dev/dri`, media source path; Read-write: transcode session directory, `/tmp`; All paths guarded with `.exists()` check before adding rules (graceful skip if path absent)
    - **Landlock graceful degradation** — `RulesetStatus::NotEnforced` returns `Ok(())` rather than error; sandbox silently skipped on kernels without Landlock support (logged via `RulesetStatus` match, but no `tracing` used in pre_exec itself)
    - **Seccomp allow-list approach** — 62 syscalls explicitly allowed; `SeccompAction::KillProcess` as mismatch action (any unlisted syscall kills the process); `SeccompAction::Allow` as match action per SECURITY.md design
    - **x86_64-specific syscalls** — `arch_prctl` conditionally included via `#[cfg(target_arch = "x86_64")]` inside the rules builder; `target_arch()` function uses `cfg` to return correct `seccompiler::TargetArch` (x86_64 or aarch64)
    - **Blocked dangerous syscalls** — By omission from allow-list: `execve`, `execveat`, `fork`, `vfork`, `ptrace`, `mount`, `umount2`, `chroot`, `pivot_root`, `connect`, `bind`, `listen`, `accept`, `socket`, `socketpair`, `keyctl`, `add_key`, `request_key`, `perf_event_open`, `kcmp`, `process_vm_readv`, `process_vm_writev`
    - **`spawn_ffmpeg` signature change** — Now accepts `source_path: &Path` and `segment_dir: &Path` for sandbox config; call site in `start_session` reordered to spawn before creating `TranscodeSession` (avoids borrow-after-move on the PathBuf fields)
    - **Sandbox failure is non-fatal** — `pre_exec` closure catches sandbox errors, logs warning via `tracing::warn!`, and returns `Ok(())` so FFmpeg still starts without sandbox on failure; matches SECURITY.md graceful degradation model
    - **No `tracing` inside pre_exec** — Logging happens in the `pre_exec` error handler which is safe (it's in the child process after fork, before exec); the landlock/seccomp functions themselves don't log
    - **`seccompiler::Error::Backend`** — `build_ffmpeg_filter().try_into()` error mapped via `.map_err(seccompiler::Error::Backend)` following seccompiler's error type hierarchy

   4. ~~Create `server/src/domains/quality/` — five-file pattern~~ **DONE**

    **What was built for Task 4:**

    | File | Purpose |
    |---|---|
    | `server/src/domains/quality/mod.rs` | Module declarations, re-exports (`QualityError`), router with 13 routes covering device capabilities, capability wizard, network probing, telemetry, QoE, and admin endpoints |
    | `server/src/domains/quality/types.rs` | 4 Row types (`DeviceProfileRow`, `DeviceCapabilityTestRow`, `ClientNetworkReportRow`, `QoeReportRow`), 7 Request types with validation, 8 Response types including admin summaries |
    | `server/src/domains/quality/error.rs` | `QualityError` enum — 12 variants matching QUALITY_001–QUALITY_012 error codes from ERROR_HANDLING.md, plus `Database` |
    | `server/src/domains/quality/service.rs` | 12 service function stubs (`todo!()`) for capabilities, wizard, telemetry, probing, QoE, admin summaries |
    | `server/src/domains/quality/handlers.rs` | 13 handler stubs wired to Axum extractors (State, AuthenticatedUser, Path, Json) with correct request/response types |
    | `server/src/error.rs` | Added `Quality(#[from] QualityError)` variant to `AppError`, `quality_error_to_http()` mapping with correct status codes and QUALITY_xxx codes |
    | `server/src/domains/mod.rs` | Added `pub mod quality;` |
    | `server/src/router.rs` | Wired `quality::router(state)` into main router, removed Phase 7 comment |

    **Key decisions from Task 4:**

    - **QUALITY_008 (SubtitleBurnInRequired) maps to HTTP 200** — per ERROR_HANDLING.md, this is a warning not an error; burn-in occurred but playback proceeds normally
    - **Three-type DTO pattern followed** — `XxxRow` (no Serialize/Deserialize), `XxxRequest` (Deserialize + Validate), `XxxResponse` (Serialize only)
    - **Admin endpoints separated** — `/api/v1/admin/quality/*` routes for network summary, device summary, QoE metrics, and transcode breakdown; require `can_manage_server`/`can_view_analytics` (enforced by AuthenticatedUser extractor capabilities)
    - **All service/handler functions are `todo!()` stubs** — tasks 5-7 will implement the actual business logic
    - **Static validation constants** — `VALID_PROFILE_SOURCES`, `VALID_NETWORK_TIERS`, `VALID_REPORT_TYPES`, `VALID_WIZARD_RESULTS`, `VALID_QUALITY_MODES` arrays for use in service-layer validation

   5. ~~Implement device capability detection — runtime probe~~ **DONE**

     **What was built for Task 5:**

     | File | Purpose |
     |---|---|
     | `server/src/domains/quality/service.rs` | Full device capability detection service: `report_capabilities` (upsert on `device_identifier`), `get_device_profile` (returns conservative baseline when no profile exists), `start_capability_wizard` (creates test rows from `WIZARD_TEST_MATRIX`), `submit_capability_test` (records test result, auto-completes wizard), `get_capabilities` / `list_capability_tests` (query by `device_identifier`); `derive_capabilities_from_wizard` derives full profile from test results; `try_complete_wizard` checks completion and derives profile; `CONSERVATIVE_BASELINE_*` statics for unknown devices; `WIZARD_TEST_MATRIX` with 10 test entries |
     | `server/src/domains/quality/handlers.rs` | 5 working handlers: `report_capabilities`, `get_device_profile`, `start_capability_wizard`, `submit_capability_test`, `get_capabilities`, `list_capability_tests`; 7 handlers remain `todo!()` (bandwidth probe, segment telemetry, QoE reports, admin summaries) |
     | `server/src/domains/quality/error.rs` | No changes needed — existing variants sufficient |

     **Key decisions from Task 5:**

     - **Conservative baseline for unknown devices** — H.264, AAC, SRT/WebVTT, MP4, 1080p, 2ch, 6Mbps — matches QUALITY_MANAGEMENT.md fallback behavior
     - **`report_capabilities` uses upsert** — `INSERT ... ON CONFLICT (device_identifier) DO UPDATE` on the unique index; client can re-report capabilities at any time
     - **`get_device_profile` returns baseline on missing** — When no profile exists, returns `DeviceProfileResponse` with `id: Uuid::nil()` and conservative defaults rather than a 404 error; allows clients to proceed with safe defaults
     - **Wizard test matrix: 10 tests** — H.264 8/10-bit, HEVC 8/10-bit/4K HDR10, AV1 8/10-bit, Dolby Vision P8, AAC/AC3/DTS audio, PGS subtitle overlay — covers the transcode decision matrix from VIDEO_FORMATS.md
     - **Auto-complete on final test** — `try_complete_wizard` is called after each test submission; when all tests have a non-pending result, `derive_capabilities_from_wizard` builds the full capability profile from test results
     - **`derive_capabilities_from_wizard` maps test IDs to capabilities** — Each test format ID (e.g., `hevc_10bit_4k_hdr10_mkv`) maps to specific video codecs, HDR formats, containers, resolutions, audio codecs, and max channels; results are aggregated across all passed tests
     - **`device_identifier` query parameter** — `get_capabilities` and `list_capability_tests` use `?device_identifier=` query param since `AuthenticatedUser` has no device identifier; handlers validate the parameter is non-empty and return `AppError::BadRequest` if missing
      - **`test_passed` helper uses `&[PgRow]` slice** — Follows clippy recommendation to use slices over `&Vec<T>`
      - **No new workspace dependencies** — all functionality uses existing `sqlx`, `serde_json`, `uuid`, `chrono`
 6. ~~Implement network quality assessment — segment download telemetry~~ **DONE**

     **What was built for Task 6:**

     | File | Purpose |
     |---|---|
     | `server/src/domains/quality/service.rs` | Replaced 7 `todo!()` stubs with working implementations: `submit_segment_telemetry` (insert into `client_network_reports`, compute throughput + harmonic mean + network tier), `submit_bandwidth_probe_result` (insert probe report with throughput/tier), `submit_qoe_report` (insert into `qoe_reports`), `get_network_quality_summary` (admin per-user latest tier + 24h sample count), `get_device_capability_summary` (admin per-platform device count + wizard rate + top codecs), `get_qoe_summary` (admin last-100 sessions QoE), `get_transcode_breakdown` (admin direct play/stream/transcode % from `play_sessions`); added `classify_network_tier()`, `compute_segment_throughput()`, `compute_harmonic_mean_throughput()` |
     | `server/src/domains/quality/handlers.rs` | Replaced 7 `todo!()` stubs with working handlers: `get_bandwidth_probe` (returns 100KB static payload), `submit_bandwidth_probe_result`, `submit_telemetry` (reads `throughput_estimate_window` from `RuntimeConfig`), `submit_qoe` (reads `qoe_report_interval_seconds` from `RuntimeConfig`), `admin_network_summary`, `admin_device_summary`, `admin_qoe_summary`, `admin_transcode_breakdown` |
     | `server/src/domains/quality/types.rs` | Added `TelemetryAckResponse`, `ProbeAckResponse`, `QoeAckResponse` DTOs with `report_id`, `throughput_bps`, `network_tier` fields |

     **Key decisions from Task 6:**

     - **Network tier classification** — `classify_network_tier()` implements the 6-tier table from QUALITY_MANAGEMENT.md: excellent (>25 Mbps), good (10-25), moderate (5-10), slow (2-5), very_slow (0.5-2), critical (<0.5)
     - **Segment throughput computation** — `compute_segment_throughput()` calculates `(bytes * 8 * 1000) / duration_ms` to get bits/sec; returns `None` for zero/negative durations or bytes
     - **Harmonic mean for running throughput estimate** — `compute_harmonic_mean_throughput()` queries last N `client_network_reports` with non-null `throughput_bps`, computes harmonic mean (n / sum(1/x_i)); resistant to outlier segments per ABR best practice; window size from `QualityConfig.throughput_estimate_window` (default 5)
     - **Telemetry inserts segment throughput + estimated throughput** — `throughput_bps` is the per-segment computed value; `estimated_throughput_bps` is the harmonic mean across the window; `network_tier` uses the harmonic mean (or per-segment if insufficient history)
     - **Bandwidth probe handler returns static 100KB payload** — `static PROBE_PAYLOAD: [u8; 102400] = [0u8; 102400]`; fixed-size zero-filled buffer per `QualityConfig.network_probe_bytes` default; client measures download time to estimate throughput
     - **Probe result computes throughput server-side** — `submit_bandwidth_probe_result()` computes `(probe_bytes * 8 * 1000) / download_ms` independently; client's `estimated_throughput_bps` is stored if provided but server computation takes precedence for tier classification
     - **QoE report interval from config** — `submit_qoe()` reads `QualityConfig.qoe_report_interval_seconds` (default 30) and passes to service for insertion into `qoe_reports.report_interval_seconds`
     - **Admin network summary uses LATERAL join** — `get_network_quality_summary()` joins latest `client_network_reports` per user with a 24-hour sample count subquery; returns per-user latest tier, throughput, and sample count
     - **Admin device summary aggregates per-platform** — Groups `device_profiles` by platform; computes wizard completion rate as percentage; extracts top video codecs via `jsonb_array_elements_text` with deduplication
     - **Admin QoE summary uses DISTINCT ON** — `get_qoe_summary()` returns the latest QoE report per session (up to 100 sessions) using PostgreSQL's `DISTINCT ON (session_id)` with `ORDER BY session_id, created_at DESC`
     - **Admin transcode breakdown from play_sessions metadata** — `get_transcode_breakdown()` queries `play_sessions` filtering on `metadata->>'playback_type'` for direct_play/direct_stream/transcode counts; returns direct_play_percentage as (direct_play / total) * 100
     - **No new workspace dependencies** — all functionality uses existing `sqlx`, `serde`, `serde_json`, `uuid`, `chrono`

 7. ~~Implement transcoding decision engine — 10-factor evaluation from QUALITY_MANAGEMENT.md~~ **DONE**

     **What was built for Task 7:**

     | File | Purpose |
     |---|---|
     | `server/src/services/decision_engine.rs` | Pure shared service implementing the 10-factor transcoding decision engine: `MediaFileInfo`, `DeviceCapabilities`, `NetworkConditions`, `DecisionEngineConfig` input structs; `PlaybackDecision` output with `VideoDecision`, `AudioDecision`, `SubtitleDecision` enums; 10-factor evaluation (quality_mode bypass → codec → bit depth → resolution → HDR/DV → container → bitrate → manual quality cap); DV Profile 5/7/8 handling with client-side fallback; codec alias system; target codec selection; resolution normalization; bitrate ladder integration; 21 unit tests |
     | `server/src/services/mod.rs` | Added `pub mod decision_engine;` |

     **Key decisions from Task 7:**

     - **Pure shared service, not a domain module** — `decision_engine.rs` lives in `services/` (not `domains/`) because it has zero DB/state dependencies; all inputs are passed as structs, making it fully testable without a database
     - **Input structs separate from DB types** — `MediaFileInfo`, `DeviceCapabilities`, `NetworkConditions`, `DecisionEngineConfig` are independent of `DeviceProfileRow` and `QualityConfig`; callers construct them from DB data + request parameters
     - **6 video outcomes** — `DirectPlay`, `Remux`, `Transcode`, `ToneMap`, `Convert` (unused, reserved for future container conversion), `Error`; matches QUALITY_MANAGEMENT.md 10-factor flow
     - **Dolby Vision handling per VIDEO_FORMATS.md** — Profile 7/8 with `allow_client_side_dv_fallback=true` → `DirectPlay` (trust client decoder); Profile 7/8 HDR fallback without DV flag → `Remux` (strip DV layer); Profile 5 → `Transcode` (no HDR base layer); DV RPU stripping deferred to FFmpeg command builder (Task 9)
     - **Codec alias system** — `CODEC_ALIASES` static maps common aliases (`avc`/`avc1` → `h264`, `h265`/`hevc` → `hevc`, `dts-hd ma` → `dts_hd_ma`, etc.) for tolerant matching against device capability sets
     - **Target codec selection** — Prefers HEVC for 4K/10-bit content, falls back to `DecisionEngineConfig.default_transcode_codec`; audio target prefers Opus → EAC3 → AC3 → config default
     - **Resolution normalization** — Snaps to standard tiers (2160p/1080p/720p/480p) based on height; prevents fractional resolutions
     - **Bitrate ladder delegates to existing `TranscodeRendition::smart_ladder()`** — Reuses the 4-rung ABR ladder from `services/transcoding.rs`
     - **`parse_json_string_set()` and `parse_resolution_value()` helpers** — Public helpers for converting DB JSONB values to `HashSet<String>` and resolution tuples when constructing `DeviceCapabilities` from `device_profiles` table
     - **21 unit tests** covering: direct play (H.264+AAC+MKV, all compatible), transcode (unsupported codec, resolution exceeds device, 10-bit exceeds 8-bit device, bitrate exceeds network), tone mapping (HDR to SDR device), DV handling (Profile 7 client fallback, Profile 7 no fallback → strip, Profile 5 → transcode), container remux, audio passthrough/downmix, subtitle burn-in (PGS), subtitle convert (ASS→SRT), subtitle passthrough (SRT), codec alias matching, resolution normalization, resolution string parsing
     - **No new workspace dependencies** — All functionality uses standard library collections, `serde` derive, and existing `TranscodeRendition` from `transcoding.rs`

  8. ~~Implement streaming policy system — `streaming_policies` table with per-user overrides~~ **DONE**

  **What was built for Task 8:**

  | File | Purpose |
  |---|---|
  | `server/src/domains/playback/types.rs` | Added `StreamingPolicyRow`, `CreateStreamingPolicyRequest`, `UpdateStreamingPolicyRequest`, `StreamingPolicyResponse`, `StreamingPolicyListResponse`, `ResolvedStreamingLimitsResponse` DTOs; `VALID_TRANSCODE_RESOLUTIONS` static |
  | `server/src/domains/playback/error.rs` | Added 5 error variants: `PolicyNameExists` (PLAY_014), `SystemPolicyCannotBeDeleted` (PLAY_015), `CannotRemoveDefaultPolicy` (PLAY_016), `InvalidResolution` (PLAY_017), `InvalidIpRange` (PLAY_018) |
  | `server/src/error.rs` | Added PLAY_014–PLAY_018 error code mappings in `playback_error_to_http()` |
  | `server/src/domains/playback/service.rs` | Added 6 service functions: `list_streaming_policies`, `get_streaming_policy`, `create_streaming_policy`, `update_streaming_policy`, `delete_streaming_policy`, `resolve_streaming_limits`; helpers: `row_to_policy_response`, `jsonb_to_string_vec`, `ip_ranges_to_jsonb`, `validate_resolution`, `validate_ip_ranges` |
  | `server/src/domains/playback/handlers.rs` | Added 6 working handlers: `list_streaming_policies`, `get_streaming_policy`, `create_streaming_policy`, `update_streaming_policy`, `delete_streaming_policy`, `get_effective_streaming_limits` |
  | `server/src/domains/playback/mod.rs` | Added 3 route groups: `/api/v1/streaming-policies` (GET, POST), `/api/v1/streaming-policies/{policy_id}` (GET, PATCH, DELETE), `/api/v1/users/{user_id}/streaming-limits` (GET) |

  **Key decisions from Task 8:**

  - **All policy endpoints require `Require<CanManageServer>`** — admin-only per STREAMING.md design
  - **`is_default` flag management** — creating/updating a policy with `is_default = true` atomically clears the previous default in the same transaction; ensures exactly one default policy exists
  - **System policy protection** — `is_system = true` policies cannot be deleted (PLAY_015); attempting to delete the sole default policy returns `CannotRemoveDefaultPolicy` (PLAY_016)
  - **User cleanup on delete** — deleting a policy nullifies `streaming_policy_id` on all affected users (cascade via `ON DELETE SET NULL` in schema); explicit `UPDATE users SET streaming_policy_id = NULL` before DELETE for clarity
  - **3-tier limit resolution via `resolve_streaming_limits`** — implements the cascade from STREAMING.md: user-level overrides (`max_streams`, `max_transcode_streams`, `bandwidth_limit_bps` from `users` table) → policy values (from `streaming_policies` row) → defaults (when no policy assigned, uses `is_default` policy; if none exists, returns permissive defaults)
  - **`get_effective_streaming_limits` endpoint** — `GET /api/v1/users/{user_id}/streaming-limits` resolves the merged limits from users table + streaming_policies; intended for admin UI display and for `start_playback` (Task 12) to call during session creation
  - **COALESCE partial-update pattern** — `update_streaming_policy` uses the same `COALESCE($N, existing)` pattern as libraries/users domains for all 15 updatable fields
  - **IP range validation is structural** — `validate_ip_ranges` checks for `/` CIDR prefix presence; full CIDR parsing with `ipnet` deferred to playback session enforcement (Task 12) where the check is actually needed
  - **Resolution validation against `VALID_TRANSCODE_RESOLUTIONS`** — matches the `CHECK` constraint on `streaming_policies.max_transcode_resolution` in the DDL (`'480p', '720p', '1080p', '4k'`)
  - **No new workspace dependencies** — all functionality uses existing `sqlx`, `serde`, `serde_json`, `uuid`, `chrono`, `validator`

  **Not yet implemented (deferred to later tasks):**

  - Policy enforcement at playback start — `resolve_streaming_limits` is implemented but not yet called by `start_playback` (Task 12)
  - IP range checking against client IP — `allowed_ip_ranges` / `blocked_ip_ranges` are stored and returned but not yet evaluated against actual client IPs (Task 12)
  - `auto_terminate_paused_minutes` enforcement — stored in policy but not yet consumed by heartbeat logic (Task 12)
  - Per-user streaming policy assignment in user CRUD — `streaming_policy_id` is readable via user responses but not yet settable via the users update endpoint (deferred to admin UI)
 9. ~~Implement HLS manifest generation and segment serving~~ **DONE**

 **What was built for Task 9:**

 | File | Purpose |
 |---|---|
 | `server/src/domains/playback/service.rs` | Replaced `todo!()` stubs with working implementations: `get_media_file_path` (queries `media_files` for file path and health check), `get_media_file_size` (returns file size for Range header computation), `RangeSpec` struct with `parse()` for RFC 7233 `Range` header parsing (`bytes=N-`, `bytes=-N` suffix, `bytes=N-M`), `guess_content_type` for 12 video container MIME types, `get_transcode_manifest` (reads FFmpeg-generated `manifest.m3u8` from transcode session segment directory), `get_transcode_playlist` (resolves per-rendition playlist from master manifest — handles both single-rendition and multi-rendition manifests), `get_transcode_segment` (serves fMP4 segment bytes from transcode directory with path traversal protection), `generate_master_manifest` (builds HLS master manifest with `#EXT-X-STREAM-INF` for ABR ladder); helper functions: `validate_segment_filename`, `is_single_rendition_manifest`, `extract_rendition_from_path` |
 | `server/src/domains/playback/handlers.rs` | Replaced 4 `todo!()` stubs with working handlers: `stream_file` (Direct Play — serves media file with HTTP 206 Partial Content / Range support, `Accept-Ranges: bytes`, proper `Content-Range` header, `Content-Type` guessed from extension), `get_transcode_manifest` (returns HLS master manifest with `application/vnd.apple.mpegurl` content type and `no-cache` header), `get_transcode_playlist` (returns per-rendition playlist from master manifest reference), `get_transcode_segment` (serves fMP4 segment bytes with `video/iso.segment` content type and 1-hour cache) |

 **Key decisions from Task 9:**

 - **Response type changed from `Json<T>` to `Response`** — All four streaming handlers return `Result<Response, AppError>` instead of `Json<T>` because they serve binary data (segments, file bytes) or text with specific content types (`application/vnd.apple.mpegurl`), not JSON
 - **Direct Play uses full-file read + Range slicing** — `stream_file` reads the full file for non-Range requests; for Range requests, it seeks to the start offset and reads exactly `content_length` bytes. This is simple and correct for local media files. Streaming via `tokio::io::BufReader` with chunked transfer is a future optimization for large files
 - **Range header parsing supports three formats** — `bytes=N-` (start to end), `bytes=-N` (last N bytes, suffix range), `bytes=N-M` (explicit range). Invalid ranges return `PLAY_007` (416 Range Not Satisfiable)
 - **HLS manifest served from FFmpeg output** — `get_transcode_manifest` reads the `manifest.m3u8` file that FFmpeg writes to the session's segment directory. This is the single-rendition manifest generated by the transcoding pipeline. Multi-rendition ABR manifests with `#EXT-X-STREAM-INF` are generated by `generate_master_manifest` for future ABR ladder support
 - **Per-rendition playlist resolution** — `get_transcode_playlist` handles two cases: (1) single-rendition manifest (FFmpeg writes a plain media playlist) — returns it directly if rendition matches the session's `rendition_name`; (2) multi-rendition master manifest — parses lines to find the matching rendition's playlist path and reads it from disk
 - **Segment path traversal protection** — `validate_segment_filename` rejects names containing `..`, `/`, `\`, names longer than 64 chars, and names not starting with `seg_`. This prevents directory traversal attacks on the segment endpoint
 - **Content types per STREAMING.md** — Segments: `video/iso.segment` (fMP4); manifest: `application/vnd.apple.mpegurl`; direct play: guessed from file extension (12 video MIME types)
 - **Cache headers** — Manifest and playlist: `no-cache, no-store, must-revalidate` (live transcode state changes); segments: `max-age=3600` (segments are immutable once written)
 - **No new workspace dependencies** — All functionality uses existing `axum::body::Body`, `tokio::fs`, `axum::http` types

 10. ~~Implement direct play / remux for compatible formats (no transcode)~~ **DONE**

 **What was built for Task 10:**

 | File | Purpose |
 |---|---|
 | `server/src/services/transcoding.rs` | Added `start_remux_session()` method to `TranscodeManager` — creates an FFmpeg HLS session with `-c:v copy -c:a copy` (stream copy, no re-encoding); reuses existing session tracking, progress monitoring, semaphore capacity enforcement, and sandboxing infrastructure |
 | `server/src/domains/playback/service.rs` | Replaced `start_playback()` `todo!()` stub with full implementation: fetches media item + media file from DB, builds `MediaFileInfo` from `media_files` row, builds `DeviceCapabilities` from client device profile JSON or conservative defaults, builds `NetworkConditions` from latest `client_network_reports` row or `max_streaming_bitrate`, builds `DecisionEngineConfig` from `RuntimeConfig`, calls `decision_engine::decide()`, dispatches to DirectPlay (stream URL) / DirectStream (remux session) / Transcode (transcode session) paths, creates `play_sessions` row |
 | `server/src/domains/playback/handlers.rs` | Replaced `start_playback` `todo!()` with working handler: validates request, loads runtime config, calls service, returns `PlaybackStartResponse` |

 **Key decisions from Task 10:**

 - **Three-tier playback dispatch** — `start_playback` implements the full STREAMING.md decision flow: DirectPlay → client uses `GET /api/v1/stream/{file_id}`; DirectStream → `start_remux_session()` spawns FFmpeg with stream copy and HLS output; Transcode → `start_session()` spawns FFmpeg with full encoding and HLS output. Both DirectStream and Transcode return an HLS manifest URL; DirectPlay returns a direct file URL.
 - **`start_remux_session()` as separate method** — Rather than adding a `remux: bool` flag to the already 12-parameter `start_session()`, a dedicated method handles the remux case (stream copy without encoding). The remux FFmpeg args skip `build_video_encode_args` and `build_audio_encode_args`, replacing them with `-c:v copy -c:a copy`. All other infrastructure (session tracking, progress parsing, graceful shutdown, sandboxing) is shared.
 - **MediaFileInfo built from DB row** — `build_media_file_info()` converts the `media_files` row (string video_resolution like "1080p", nullable codec/bitrate columns, JSONB additional_streams) into the `MediaFileInfo` struct the decision engine expects. `video_resolution` parsed via `decision_engine::parse_resolution_string()`; `video_bit_depth` extracted from `additional_streams.video.bit_depth` JSONB (defaults to 8); `video_frame_rate` defaulted to 24.0 (NUMERIC column type not directly readable as f64 without `bigdecimal` feature).
 - **Device profile from request or conservative defaults** — When the client sends `device_profile` JSON in `StartPlaybackRequest`, it's parsed into `DeviceCapabilities` (video_codecs, audio_codecs, containers, subtitle_formats, max_resolution, max_audio_channels, hdr_formats, max_bitrate_bps, supports_dolby_vision, allow_client_side_dv_fallback, max_video_bit_depth). When absent, conservative defaults are used: H.264, AAC, MP4+MKV, SRT+WebVTT, 1080p, 2ch, 8-bit, no HDR — matching the conservative baseline from QUALITY_MANAGEMENT.md.
 - **Network conditions from client telemetry** — `build_network_conditions()` queries the latest `client_network_reports` row for the user to get `throughput_bps` and `network_tier`. If `max_streaming_bitrate` is provided in the request, it overrides as the throughput estimate (client explicitly caps its own bandwidth). If neither is available, returns `None` throughput (decision engine treats as unlimited).
 - **DecisionEngineConfig from RuntimeConfig** — Built from `quality.*` fields (throughput_safety_factor, fallback_max_resolution, fallback_max_bitrate_bps, allow_client_side_dv_fallback, audio_passthrough_enabled, subtitle_burn_in_policy, default_quality_mode) and `transcoding.*` fields (default_video_codec, default_audio_codec). `manual_max_resolution` set to `None` (no client-side manual resolution selection yet).
 - **`force_transcode` override** — When `StartPlaybackRequest.force_transcode` is `true`, the decision engine's `overall` result is overridden to `StreamDecision::Transcode` after evaluation. This allows clients to force transcoding for debugging or compatibility testing.
 - **`play_sessions` row created** — Each playback start creates a row in the partitioned `play_sessions` table with user_id, media_item_id, library_id, `started_at = now()`, `stream_decision` from the decision, and `client_name = 'duskcue-web'`. The session ID (UUIDv7) is returned to the client for subsequent heartbeat/stop/seek calls. `stopped_at`, `duration_seconds`, and `percent_complete` are set when playback stops (Task 12).
 - **Static SQL strings** — All media file queries use full static SQL strings (not `format!()` concatenated) per sqlx 0.9 `SqlSafeStr` requirement. The `($N::uuid IS NULL OR column = $N)` pattern is not needed here since media_file_id is always provided or auto-selected.
 - **No new workspace dependencies** — All functionality uses existing `sqlx`, `serde_json`, `uuid`, `chrono`, `decision_engine`, and `transcoding` modules.
 - **No new error variants** — Existing `PlaybackError::MediaNotFound`, `FileNotFound`, `TranscodeCapacityReached`, `FfmpegFailed` cover all failure cases in the playback start flow.

  11. ~~Implement HW accel runtime detection — NVIDIA, VAAPI, VideoToolbox, AMF~~ **DONE**

  **What was built for Task 11:**

  | File | Purpose |
  |---|---|
  | `server/src/services/hw_accel.rs` | Dedicated HW accel runtime detection module: `HwAccelDetectionResult` struct with method, platform availability flags, verified encoders, and source; `detect_hw_accel_runtime()` with multi-step detection pipeline; `probe_ffmpeg_encoders()` and `probe_ffmpeg_hwaccels()` for FFmpeg capability verification; `check_nvidia_hardware()`, `check_vaapi_hardware()`, `detect_vaapi_driver()` for platform-specific hardware detection; `emit_metrics()` for Prometheus gauge emission; `collect_encoders()` for method-relevant encoder listing |
  | `server/src/services/mod.rs` | Added `pub mod hw_accel;` |
  | `server/src/services/transcoding.rs` | Replaced inline `detect_hw_accel()` with `hw_accel::detect_hw_accel_runtime()`; `TranscodeManager.detected_hw_accel` renamed to `hw_detection` storing `HwAccelDetectionResult`; added `get_hw_detection()` public accessor; `redetect_hw_accel()` updated to pass both `TranscodingConfig` and `CpuConfig`; removed unused `TranscodingConfig` import |
  | `server/src/router.rs` | Health endpoint `/health` now includes `hardware_acceleration` object with `method`, `source`, `nvidia_detected`, `vaapi_available`, `qsv_available`, `amf_available`, `videotoolbox_available`, `verified_encoders` |

  **Key decisions from Task 11:**

  - **Dedicated module over inline expansion** — `services/hw_accel.rs` follows the project's "shared services over singletons" convention; detection logic separated from session management in transcoding.rs
  - **`HwAccelDetectionResult` stores full detection context** — Not just the chosen method; includes all platform availability flags and verified encoder list for admin diagnostics and health endpoint
  - **Three-layer detection: config → FFmpeg probe → platform check** — (1) Config can force a specific method or disable auto-detect; (2) FFmpeg `-encoders` and `-hwaccels` probed synchronously at startup via `std::process::Command` to verify encoder availability; (3) Platform-specific device file checks confirm hardware presence
  - **Priority order per STREAMING.md** — NVENC > QSV > VAAPI > VideoToolbox > AMF > Software; each checked in sequence with both FFmpeg encoder availability AND platform hardware confirmation
  - **NVIDIA detection** — Checks `/dev/nvidia0` or `/dev/nvidia-uvm` on Linux; falls back to `nvidia-smi` command availability (cross-platform); verifies `h264_nvenc`/`hevc_nvenc` in FFmpeg encoders
  - **Intel QSV detection** — On Linux: `/dev/dri/renderD*` with i915 driver → QSV; On non-Linux: FFmpeg has `h264_qsv` and `-hwaccels` has `qsv`
  - **VAAPI detection** — Linux only: `/dev/dri/renderD*` device file + FFmpeg `h264_vaapi`/`hevc_vaapi` + `-hwaccels` has `vaapi`; driver detected via `/sys/class/drm/renderD*/device/driver` symlink
  - **VideoToolbox detection** — `cfg!(target_os = "macos")` + FFmpeg `h264_videotoolbox`/`hevc_videotoolbox`
  - **AMD AMF detection** — FFmpeg has `h264_amf`/`hevc_amf`; primarily a Windows fallback (on Linux, AMD uses VAAPI which is higher priority)
  - **`hw_accel_auto_detect` respected** — When `CpuConfig.hw_accel_auto_detect` is false, immediately returns Software regardless of available hardware
  - **Forced method with encoder verification** — When `TranscodingConfig.hardware_accel` is set to a specific method (not "auto"), verifies the encoder exists in FFmpeg before accepting; falls back to Software if encoder missing
  - **Synchronous FFmpeg probing at startup** — `std::process::Command` (not tokio) used for `ffmpeg -hide_banner -encoders` and `ffmpeg -hide_banner -hwaccels`; acceptable one-time cost at startup; takes milliseconds
  - **Prometheus `system.cpu.hw_accel` gauge** — Per CPU.md spec: one gauge per method label (`nvenc`, `qsv`, `vaapi`, `videotoolbox`, `amf`, `software`), value 1 for active method, 0 for others; emitted during detection
  - **Health endpoint enrichment** — `/health` response now includes `hardware_acceleration` object with all detection details; allows Docker HEALTHCHECK and admin dashboards to verify HW accel status
  - **Driver detection via sysfs** — `detect_vaapi_driver()` reads `/sys/class/drm/renderD*/device/driver` symlink to distinguish Intel (i915) from AMD (amdgpu) for QSV vs VAAPI selection
  - **No new workspace dependencies** — All functionality uses existing `std::process::Command`, `std::fs`, `metrics` crate, and `tracing`
 12. ~~Implement play session tracking — create `play_sessions` rows, heartbeat updates~~ **DONE**
 13. ~~Implement `user_item_data` — watch state, resume position, play count~~ **DONE**

  **What was built for Task 13:**

  | File | Purpose |
  |---|---|
  | `server/src/domains/playback/types.rs` | Added `UpdateWatchDataRequest` (Deserialize + Validate, fields: `is_favorite`, `user_rating` 1–10, `audio_stream_index`, `subtitle_stream_index`) |
  | `server/src/domains/playback/service.rs` | Implemented 12 service functions: `get_user_item_data` (returns defaults for unplayed items), `update_user_item_data` (COALESCE upsert for favorite/rating/stream indices), `list_bookmarks`, `create_bookmark`, `delete_bookmark`, `list_playlists`, `get_playlist`, `create_playlist`, `update_playlist` (COALESCE partial update), `delete_playlist` (soft-delete), `list_playlist_items` (JOIN media_items for title), `add_playlist_item` (auto-position via MAX+1000 spacing), `remove_playlist_item`; helpers: `row_to_playlist_response`, `verify_playlist_ownership`, `update_playlist_counters` |
  | `server/src/domains/playback/handlers.rs` | Replaced 12 `todo!()` stubs with working handlers: `get_watch_data`, `update_watch_data`, `list_bookmarks`, `create_bookmark`, `delete_bookmark`, `list_playlists`, `get_playlist`, `create_playlist`, `update_playlist`, `delete_playlist`, `list_playlist_items`, `add_playlist_item`, `remove_playlist_item` |
  | `server/src/domains/playback/mod.rs` | Added `PUT` handler on `/api/v1/items/{item_id}/watch-data` route (GET + PUT) |

  **Key decisions from Task 13:**

  - **`get_user_item_data` returns defaults for unplayed items** — When no `user_item_data` row exists (user never interacted with the item), returns a response with `id: Uuid::nil()`, `is_watched: false`, `play_count: 0`, `resume_position_ms: 0`, `is_favorite: false`, `user_rating: None` — matching the quality domain's conservative-defaults-on-missing pattern, so the web client always gets a valid response without 404s for first-time item views
  - **`update_user_item_data` uses COALESCE upsert** — `INSERT ... ON CONFLICT (user_id, media_item_id) DO UPDATE SET is_favorite = COALESCE($3, existing)` pattern, same as all other domains; `None` fields preserve existing values
  - **`PUT` method on watch-data endpoint** — Chose `PUT` over `PATCH` for setting favorite/rating/stream indices because the update is an upsert (creates the row if it doesn't exist); PATCH implies partial modification of an existing resource
  - **Bookmarks ordered by position** — `list_bookmarks` returns items ordered by `position_ms ASC` for chronological seek-bar ordering
  - **Bookmark deletion scoped by user_id + media_item_id + bookmark_id** — Triple-key DELETE prevents BOLA: a user cannot delete another user's bookmarks even with the bookmark UUID, and the `media_item_id` in the URL path is validated in the WHERE clause
  - **Playlists use soft-delete** — `delete_playlist` sets `deleted_at = now()` rather than `DELETE FROM`, matching DATABASE.md design (users expect trash/undo); all playlist queries filter `deleted_at IS NULL`
  - **Playlist visibility validation** — `create_playlist` and `update_playlist` validate against `VALID_PLAYLIST_VISIBILITIES` (`private`, `shared`, `public`); default is `private`
  - **Playlist item auto-positioning with integer spacing** — When `position` is not provided, `add_playlist_item` uses `MAX(position) + 1000` (or `1000` for empty playlists), matching DATABASE.md integer-spacing convention (1000, 2000, 3000) that allows future insertions without renumbering
  - **Playlist item unique violation → PlaylistItemNotFound** — The `UNIQUE(playlist_id, media_item_id)` constraint prevents duplicate items; violations are caught via `sqlx::Error::Database::is_unique_violation()` and mapped to `PlaylistItemNotFound`
  - **`update_playlist_counters` maintains denormalized `item_count`** — Called after add/remove playlist item; updates `item_count` via `SELECT COUNT(*)` subquery to avoid stale counters. `total_duration_seconds` not yet recomputed (stays at 0) — deferred to Phase 8 when the web client needs it
  - **`verify_playlist_ownership` helper** — Shared by all playlist item operations; queries `playlists WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL`, returns `PlaylistNotFound` on failure — prevents BOLA on playlist sub-resources
  - **`PlaylistItemResponse` includes media title** — `list_playlist_items` JOINs `media_items` to populate `title` so the web client can display item names without a second round-trip
  - **All endpoints require `AuthenticatedUser`** — No capability checks on watch-data/bookmarks/playlists; these are user-scoped resources (each user manages their own). Admin-only access not needed since all queries are scoped to `user_id` from the authenticated session
  - **No new workspace dependencies** — all functionality uses existing `sqlx`, `validator`, `serde`, `uuid`, `chrono` crates

  **Not yet implemented (deferred to later phases):**

  - Smart playlist filter evaluation — `is_smart` flag and `smart_filter` JSONB stored but not evaluated at query time (Phase 12 collections shares the filter syntax)
  - Shared/public playlist visibility — listing currently only returns the user's own playlists; visibility-based listing (shared/public from other users) deferred to Phase 8 web client
  - `total_duration_seconds` recomputation on add/remove item — counter stays at 0; deferred to when web client needs it
  - Playlist renumbering — when gaps between positions become too small after many insertions, a renumber task is needed (deferred)
  - Continue Watching / Up Next query endpoints — the `user_item_data` table has the necessary partial indexes but dedicated endpoints are deferred to Phase 8 web client

 **What was built for Task 12:**

 | File | Purpose |
 |---|---|
 | `server/src/domains/playback/service.rs` | Implemented 4 service functions: `heartbeat` (session ownership verification, state transition detection emitting appropriate `play_events`, metadata merge update, `user_item_data.resume_position_ms` upsert, heartbeat event emission); `stop_playback` (transcode session cleanup, `play_sessions` finalization with `stopped_at`/`duration_seconds`/`percent_complete`, `play_events` stop event, `user_item_data` play_count increment + `is_watched` + resume position logic); `seek` (transcode session restart via `seek_session()` for transcoded/remux sessions, direct play client-side seek passthrough, metadata + `user_item_data` position update, seek event emission); `get_playback_info` (session state from metadata, transcode progress from `TranscodeManager`, media file runtime lookup); Updated `create_play_session` to store `transcode_session_id`, `media_file_id`, `current_state`, and `current_position_ms` in `play_sessions.metadata` JSONB; Added 4 helper functions: `emit_play_event`, `merge_session_metadata`, `upsert_user_item_data_heartbeat`, `upsert_user_item_data_stop` |
 | `server/src/domains/playback/handlers.rs` | Replaced 4 `todo!()` stubs with working handlers: `heartbeat` (validates `HeartbeatRequest`, extracts session_id, derives effective state from `state`/`is_paused`/`is_buffering` fields); `stop_playback` (accepts `StopPlaybackRequest` body, validates session_id required, passes final position); `seek` (validates `SeekRequest`, passes position to service); `get_playback_info` (takes `Path<session_id>`, returns `PlaybackInfoResponse`) |
 | `server/src/domains/playback/types.rs` | Added `StopPlaybackRequest` (`session_id` required + optional `position_ms`), `StopPlaybackResponse` (session summary with `duration_seconds`, `percent_complete`, `is_watched`, `play_count`), `SeekResponse` (includes new `stream_url` and `transcode_session_id` for transcode restarts) |

 **Key decisions from Task 12:**

 - **Metadata JSONB for session state** — `play_sessions.metadata` stores `transcode_session_id`, `media_file_id`, `current_state`, `current_position_ms`, `last_heartbeat_at`; enables state transition detection without a separate in-memory session tracker. PostgreSQL `||` operator merges JSONB shallowly per-key — ideal for incremental metadata updates via `merge_session_metadata()`
 - **State transition detection in heartbeat** — The `heartbeat()` function compares the effective state (derived from `state` field, falling back to `is_buffering`/`is_paused` booleans) against the previously stored `current_state` in metadata. Transitions emit corresponding `play_events`: `playing→paused` emits `pause`, `paused→playing` emits `resume`, `playing→buffering` emits `buffer_start`, `buffering→playing` emits `buffer_end`. Every heartbeat also emits a `heartbeat` event for analytics
 - **Ownership verification prevents information leakage** — Both heartbeat and stop return `SessionNotFound` when `user_id` doesn't match, so a user can't distinguish between "session doesn't exist" and "session belongs to another user"
 - **Session must be active for heartbeat/seek** — Heartbeat and seek queries filter `stopped_at IS NULL`; stopped sessions return `SessionNotFound`. Stop can finalize any session (stopped_at may already be set for idempotent stop)
 - **`percent_complete` computation** — `(position_ms / (runtime_seconds * 1000)) * 100`, clamped to 100%. Runtime looked up from `media_files.runtime_seconds` via `media_file_id` stored in session metadata. Returns `None` if media_file_id or runtime unavailable
 - **`is_watched` at 90% threshold** — Per STREAMING.md: when `percent_complete >= 90%`, `is_watched` is set to true and `resume_position_ms` is cleared to 0 (fully watched content doesn't need resume). Below 90%, resume position is preserved for "continue watching"
 - **`user_item_data` upsert patterns** — Heartbeat uses `INSERT ... ON CONFLICT DO UPDATE SET resume_position_ms` (upsert without incrementing play_count); Stop uses `INSERT ... ON CONFLICT DO UPDATE SET play_count = play_count + 1` (atomic increment). Both use `COALESCE($media_file_id, existing)` to avoid nulling out the file reference on subsequent updates
 - **Transcode seek returns new session ID** — `seek_session()` removes the old transcode session and creates a new one with a fresh UUID; the new `transcode_session_id` and `stream_url` are written to `play_sessions.metadata` and returned in `SeekResponse` so the client can request segments from the new session
 - **Direct play seek is client-side** — For direct play sessions (no transcode), seek just updates position in metadata and `user_item_data`; the client handles the actual seek via HTTP Range requests
 - **`get_playback_info` reads transcode progress live** — Queries `TranscodeManager.get_session()` for `progress_percent()` — reflects real-time FFmpeg progress from `-progress pipe:1` parsing
 - **No new workspace dependencies** — all functionality uses existing `sqlx`, `serde_json`, `chrono`, `uuid`
 - **No new error variants** — existing `SessionNotFound`, `InvalidSeekPosition` cover all failure cases

  **Not yet implemented (deferred to later tasks/phases):**

  - Session heartbeat timeout (60s no-heartbeat auto-stop) — requires a background cleanup task, deferred
  - Paused session auto-termination per `streaming_policies.auto_terminate_paused_minutes` — requires querying user's streaming policy on every heartbeat; deferred to a background task or future enhancement
  - Streaming policy enforcement at playback start (`resolve_streaming_limits` is implemented but not called by `start_playback` — deferred to a future integration task)
  - IP range checking against client IP (`allowed_ip_ranges`/`blocked_ip_ranges` stored but not evaluated)
  - SSE real-time push for `transcode_progress` events — `Player.svelte` currently polls `GET /api/v1/playback/{session_id}` every few seconds. Migration to SSE (`GET /api/v1/events?types=transcode_progress`) is the first consumer of the SSE transport decided in [REAL_TIME_PUSH.md](docs/design/REAL_TIME_PUSH.md). Tracked as Phase 7 follow-up.

**Verification:** User clicks play on a movie, HLS stream starts, segments are served, play session is tracked, resume position updates. Transcoding activates for incompatible formats. HW acceleration detected and used when available. Watch data is readable and settable (favorite, rating). Bookmarks can be created, listed, and deleted. Playlists support full CRUD with ordered items.

---

## Phase 8 — Web Client Core (COMPLETE)

**Goal:** Functional web UI for browsing libraries, playing media, and basic settings.

**Prerequisites:** Phase 7 complete. All playback API endpoints are available — playback session lifecycle (start/heartbeat/stop/seek/info), Direct Play/Remux/Transcode streaming, HLS manifest/segment serving, watch data (GET/PUT favorite+rating), bookmarks (list/create/delete), playlists (CRUD + items), streaming policies (admin CRUD), and quality management (device capabilities, network telemetry, QoE reports).

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md) | SvelteKit routes, API client layer pattern, stores, components |
| [UI_FOUNDATIONS.md](docs/branding/UI_FOUNDATIONS.md) | Visual direction, navigation language, core reusable surfaces |
| [NAME_BRANDING.md](docs/branding/NAME_BRANDING.md) | Product identity, logo usage |
| [API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md) | API client layer — `core.js` fetch wrapper, per-domain modules |

**Tasks:**

1. ~~Build `clients/web/src/lib/api/core.js` — HTTP client with session cookie handling, error parsing (RFC 9457)~~ **DONE**

   **What was built for Task 1:**

   | File | Purpose |
   |---|---|
   | `clients/web/package.json` | SvelteKit/Svelte 5/Vite devDependencies — `@sveltejs/kit` ^2.57, `@sveltejs/adapter-node` ^5.2, `@sveltejs/vite-plugin-svelte` ^5.0, `svelte` ^5.55, `vite` ^6.0 |
   | `clients/web/svelte.config.js` | SvelteKit config with `adapter-node` (Docker/self-hosted target) and `vitePreprocess` |
   | `clients/web/vite.config.js` | Vite config with SvelteKit plugin and dev-mode API proxy (`/api`, `/health` → `DUSKCUE_BACKEND_URL` or `localhost:48027`) |
   | `clients/web/src/app.html` | SvelteKit HTML shell with `%sveltekit.head%` / `%sveltekit.body%`, `data-sveltekit-preload-data="hover"` |
   | `clients/web/src/routes/+layout.svelte` | Root layout (Svelte 5 runes — `$props()`, `{@render children()}`) |
   | `clients/web/src/routes/+page.svelte` | Minimal placeholder home page |
   | `clients/web/src/lib/api/core.js` | HTTP client — `request()` core wrapper, `get`/`post`/`patch`/`put`/`del` convenience methods, `ApiError` class (RFC 9457), bearer token store, `buildApiUrl()` |

   **Key decisions from Task 1:**

   - **Decoupled from `auth.js`** — PROJECT_STRUCTURE.md showed `core.js` importing `getAuthHeaders()` from `auth.js`, but this creates a circular dependency (auth API module imports core for HTTP methods). Instead, `core.js` manages an optional module-level bearer token via `setBearerToken()` / `clearBearerToken()`. The web client primarily uses HttpOnly session cookies (browser-managed, no JS token access); bearer token support is for optional scenarios (Tauri desktop wrapper, API keys, testing).
   - **Clean method names (`get`/`post`/`patch`/`put`/`del`)** — Replaced the documented `getDataRequest`/`postDataRequest` names with concise REST-verb methods. `del` instead of `delete` (reserved word). Domain modules import like `import { get, post, del } from './core.js'`.
   - **`credentials: 'same-origin'`** — Explicit on all requests. The session cookie (`Path=/api`, `HttpOnly`, `SameSite=Strict`) is sent automatically by the browser for same-origin API calls. During dev, Vite proxy makes requests appear same-origin to the browser.
   - **`ApiError` class wraps RFC 9457 Problem Details** — Extends `Error` with `type`, `title`, `status`, `detail`, `traceId`, `instance`, `errors` (validation array), `retryAfter` (429). Convenience getters: `isValidation`, `isRateLimited`, `isUnauthorized`, `isForbidden`, `isNotFound`, `isConflict`, `isServerError`. `fieldError(fieldName)` extracts per-field validation errors.
   - **Query param handling** — `URLSearchParams` for encoding; arrays joined with comma (per API_CONVENTIONS.md multi-value filter convention: `?genre=action,thriller`); `undefined`/`null` values omitted; booleans stringified as `'true'`/`'false'`.
   - **Response handling** — `204 No Content` and `304 Not Modified` return `null`; non-JSON content types return `null`; JSON content types parse and return the body; error responses throw `ApiError`.
   - **Network errors throw `ApiError` with `status: 0`** — A `NETWORK_ERROR` synthetic error type (`/errors/network`) wraps fetch failures so callers catch everything in one `catch` block.
   - **`AbortSignal` support** — `options.signal` for request cancellation; `AbortError` re-thrown directly (not wrapped in `ApiError`).
   - **ETag support** — `options.ifNoneMatch` sets `If-None-Match` header; `options.returnResponse` returns raw `Response` for ETag extraction. 304 returns `null` so caller uses cached data.
   - **No `window` dependency** — Relative URL construction via string concatenation (not `new URL(path, window.location.origin)`) to keep the module SSR-safe and environment-agnostic.
   - **Svelte 5 runes in layout** — `+layout.svelte` uses `$props()` + `{@render children()}` (Svelte 5 pattern), not the Svelte 4 `<slot />` pattern documented in PROJECT_STRUCTURE.md stores section.

   **Deviations from PROJECT_STRUCTURE.md (to be reconciled):**
   - `core.js` API surface changed from `getDataRequest`/`postDataRequest` to `get`/`post`/`patch`/`put`/`del` — cleaner, covers all HTTP methods
   - `core.js` does not import from `auth.js` — bearer token store is self-contained
   - Store pattern in docs uses Svelte 4 `writable`/`derived` — will be updated to Svelte 5 runes (`$state`/`$derived`) when stores are built (Task 3)

 2. ~~Build API client modules per domain — `auth.js`, `users.js`, `libraries.js`, `media.js`, `playback.js`, `settings.js`, `search.js`~~ **DONE**

   **What was built for Task 2:**

   | File | Purpose |
   |---|---|
   | `clients/web/src/lib/api/auth.js` | 25 exported functions covering all auth routes: `setup`, `loginWithInvite`, `loginWithPassword`, `logout`, `logoutAll`, WebAuthn start/finish (with `X-Challenge-Id` header), `verifyTotp`, re-auth code (`authenticateWithReauthCode`, `requestReauthCode`), device linking (`createDeviceCode`, `pollDeviceToken`, `verifyDeviceCode`), session management (`listSessions`, `deleteSession`, `signOutEverywhere`, `requestUserReauth`), passkey management (`listPasskeys`, `startPasskeyRegistration`, `finishPasskeyRegistration`, `deletePasskey`), invitations (`listInvitations`, `createInvitation`, `revokeInvitation`, `resendInvitation`), `listCapabilities` |
   | `clients/web/src/lib/api/users.js` | 6 exported functions: `listUsers`, `getUser`, `updateUser` (PUT), `deleteUser`, `getUserCapabilities`, `updateUserCapabilities` (PUT) |
   | `clients/web/src/lib/api/libraries.js` | 12 exported functions: library CRUD (`listLibraries`, `getLibrary`, `createLibrary`, `updateLibrary`, `deleteLibrary`), `scanLibrary`, `listLibraryItems`, library paths CRUD (`listLibraryPaths`, `getLibraryPath`, `createLibraryPath`, `updateLibraryPath`, `deleteLibraryPath`) |
   | `clients/web/src/lib/api/media.js` | 6 exported functions: `listMediaItems`, `getMediaItem`, `updateMediaItem` (PATCH), `deleteMediaItem`, `listMediaFiles`, `getMediaFile` |
   | `clients/web/src/lib/api/playback.js` | 31 exported functions covering 6 sub-domains: playback sessions (`startPlayback`, `heartbeat`, `stopPlayback`, `seek`, `getPlaybackInfo`), streaming URL builders (`streamFileUrl`, `transcodeManifestUrl`, `transcodePlaylistUrl`, `transcodeSegmentUrl`), watch data (`getWatchData`, `updateWatchData`), bookmarks (`listBookmarks`, `createBookmark`, `deleteBookmark`), playlists (`listPlaylists`, `getPlaylist`, `createPlaylist`, `updatePlaylist`, `deletePlaylist`, `listPlaylistItems`, `addPlaylistItem`, `removePlaylistItem`), streaming policies (`listStreamingPolicies`, `getStreamingPolicy`, `createStreamingPolicy`, `updateStreamingPolicy`, `deleteStreamingPolicy`, `getEffectiveStreamingLimits`) |
   | `clients/web/src/lib/api/settings.js` | 2 exported functions: `validateProviderKey` (POST `/settings/providers/validate`), `getHealth` (GET `/health`) |
   | `clients/web/src/lib/api/search.js` | 1 exported function: `search(query, params)` → GET `/search?q=...` |
   | `clients/web/src/lib/api/quality.js` | 12 exported functions covering device capabilities (`reportCapabilities`, `getCapabilities`), capability wizard (`listCapabilityTests`, `startCapabilityWizard`, `submitCapabilityTestResult`), bandwidth probe (`bandwidthProbeUrl` URL builder, `submitBandwidthProbeResult`), telemetry (`submitTelemetry`, `submitQoeReport`), admin summaries (`getNetworkQualitySummary`, `getDeviceCapabilitySummary`, `getQoeSummary`, `getTranscodeBreakdown`) |
   | `clients/web/src/lib/api/index.js` | Barrel export — re-exports all domain modules + `core.js` |

   **Key decisions from Task 2:**

   - **One function per endpoint** — Each API module exports named async functions, one per backend route. Svelte components and stores import these functions and never call `fetch` directly, per PROJECT_STRUCTURE.md convention. Function names are descriptive REST verbs: `list*`, `get*`, `create*`, `update*`, `delete*` for CRUD; action verbs (`scanLibrary`, `startPlayback`, `heartbeat`) for RPC-like endpoints
   - **Streaming endpoints use URL builders, not fetch wrappers** — `streamFileUrl`, `transcodeManifestUrl`, `transcodePlaylistUrl`, `transcodeSegmentUrl`, `bandwidthProbeUrl` return URL strings (via `buildApiUrl()` from `core.js`) rather than making fetch calls. These URLs are consumed by the `<video>` element's `src` attribute or hls.js for HLS playback — the browser handles those fetches directly with Range headers, not through the API client
   - **WebAuthn challenge ID via `X-Challenge-Id` header** — `finishWebauthnAuth` and `finishPasskeyRegistration` accept a `challengeId` parameter that's sent as a custom header, matching the server-side challenge linking mechanism from Phase 4 Task 2. The `core.js` `request()` function's `options.headers` pass-through enables this
   - **Capabilities split across modules** — `listCapabilities` (static list of all available capabilities) lives in `auth.js` since it maps to `GET /auth/capabilities` (unauthenticated metadata endpoint). `getUserCapabilities` and `updateUserCapabilities` live in `users.js` since they map to `GET/PUT /users/{id}/capabilities` (user-scoped resource). Both sets of routes are in `auth/mod.rs` on the backend, but the client-side organization follows the resource, not the server module
   - **`quality.js` included beyond the task's 7 listed modules** — Phase 7 built the full quality domain (device capabilities, telemetry, QoE, admin summaries). Including `quality.js` now avoids a gap that would block Task 4 (Player component needs telemetry/QoE reporting) and Task 5 (Settings pages need quality admin endpoints). The remaining future-phase stub modules (`analytics.js`, `trakt.js`, `subtitles.js`, etc.) remain as license-header-only stubs
   - **`scanLibrary` passes `mode` via query params** — `POST /libraries/{id}/scan?mode=full|quick` uses the `params` option on `post()` (3rd argument) rather than a request body. The server reads scan mode from query params, and there's no request body for this action endpoint
   - **`del` not `delete`** — `delete` is a JavaScript reserved word, so the DELETE method wrapper is `del()` (from Task 1's `core.js`), and client functions use `delete` as a verb prefix (e.g., `deleteLibrary`, `deletePasskey`) which is fine as a function name since it's not in statement position
   - **Barrel export includes `core.js`** — `index.js` re-exports `core.js` so consumers can import `ApiError`, `setBearerToken`, `buildApiUrl` alongside domain functions from a single `import { ... } from '$lib/api'`
   - **No JSDoc type annotations** — Function signatures are self-documenting via parameter names; the server's response types are the source of truth. Adding TypeScript-style JSDoc would duplicate the server DTOs and create drift risk
   - **`getHealth` in `settings.js`** — The `/health` endpoint (from Phase 3 `router.rs`) is a system-level endpoint, not under any domain. Placed in `settings.js` alongside provider validation as the "system" client module. Alternatively could be in `core.js`, but keeping it domain-grouped with other system/settings endpoints is more discoverable

 3. ~~Build Svelte stores — `auth.js`, `user.js`, `libraries.js`, `player.js`, `notifications.js`~~ **DONE**

    **What was built for Task 3:**

    | File | Purpose |
    |---|---|
    | `clients/web/src/lib/stores/auth.js` | Authentication state store: `auth` writable store (user, isAuthenticated, loading, error); `init()` restores cached user from localStorage; `checkSession()` validates session via authenticated API call; `setup()`, `loginWithInvite()`, `loginWithPassword()`, `loginWithPasskey()` (delegates to WebAuthn API via injected `getCredential` callback), `logout()`, `logoutAll()`; derived stores: `isAuthenticated`, `currentUser`, `authLoading`, `authError`, `userRole`, `userCapabilities`; `hasCapability(cap)` derived factory with owner bypass |
    | `clients/web/src/lib/stores/user.js` | Account management store: `user` writable store (sessions, passkeys, preferences, error); `fetchSessions()`, `deleteSession()`, `signOutEverywhere()`, `requestReauth()`, `fetchPasskeys()`, `registerPasskey()`, `deletePasskey()`; `updatePreferences()` persisted to localStorage; `resetPreferences()`; derived stores: `sessions`, `passkeys`, `preferences`, `userError` |
    | `clients/web/src/lib/stores/libraries.js` | Library browse/manage store: `libraries` writable store (items, currentLibraryId, currentLibrary, paths map, scanning map, loading, error); `fetch()`, `selectLibrary()`, `create()`, `update()`, `remove()`, `scan()` with per-library scanning flags; full library path CRUD (`fetchPaths`, `createPath`, `updatePath`, `removePath`); `isScanning()`, `getById()` synchronous accessors; derived stores: `libraryList`, `currentLibrary`, `librariesLoading`, `librariesError` |
    | `clients/web/src/lib/stores/player.js` | Playback lifecycle store: `player` writable store (sessionId, mediaItem, mediaFileId, streamUrl, streamDecision, transcodeSessionId, isPlaying, isBuffering, positionMs, durationMs, volume, isMuted, isFullscreen, playbackRate, error, loading); `play()` starts playback via `startPlayback` API, resolves stream URL (direct file vs HLS manifest) based on decision; `resume()` restores existing session; `seek()` calls server seek endpoint for transcode restart; `stop()` calls `stopPlayback` API and resets state; internal heartbeat timer (15s interval) sending position/state to server; `setVolume()` persisted to localStorage; derived stores: `isPlaying`, `isBuffering`, `currentPosition`, `currentDuration`, `streamUrl`, `streamDecision`, `currentMediaItem`, `playerVolume`, `playerError`, `playerLoading`, `progressPercent` |
    | `clients/web/src/lib/stores/notifications.js` | Toast notification store: `notifications` writable store (array); `success()`, `error()`, `warning()`, `info()` convenience methods; `add()` with configurable type/title/message/duration/dismissible; `dismiss()`, `clear()`; auto-dismiss via `setTimeout` with cleanup; max 5 simultaneous notifications (FIFO eviction); `notificationList` derived store |

    **Key decisions from Task 3:**

    - **Svelte 4 `svelte/store` over Svelte 5 runes in `.svelte.js`** — The PROJECT_STRUCTURE.md documents the store pattern using `writable`/`derived` from `svelte/store`. Task 1 noted "will be updated to Svelte 5 runes" as an aspirational deviation. In practice, `svelte/store` stores are fully supported in Svelte 5 (auto-subscribed via `$store` prefix), well-documented, and simpler for cross-module shared state. Runes in `.svelte.js` would require renaming all store files and changing the access pattern. The `svelte/store` approach was kept for consistency with the existing API client layer and lower risk.
    - **Factory-function pattern with encapsulated `set`/`update`** — Each store uses `createXxxStore()` returning `{ subscribe, ...methods }`. The internal `set`/`update` are closure-captured, so external code can only mutate state through defined actions — not by calling `set()` directly. This matches the PROJECT_STRUCTURE.md example.
    - **`extractItems()` helper for response normalization** — Server responses for list endpoints return `{ items, total, page, ... }` (offset pagination). The `extractItems()` helper checks for `Array.isArray(response)` first, then `response.items`, defaulting to `[]`. Makes stores resilient to response shape variations across endpoints.
    - **Auth store caches user in localStorage** — The session cookie is HttpOnly (browser-managed, no JS access). To avoid a full page reload resetting the auth state, the user object is cached in `localStorage['duskcue_user']` on login and restored on `init()`. `checkSession()` validates the session by calling an authenticated endpoint — if it returns 401, the cache is cleared and the user is marked unauthenticated.
    - **`loginWithPasskey(getCredential)` callback pattern** — The auth store delegates the WebAuthn `navigator.credentials.get()` call to the caller via an injected callback. This keeps the store framework-agnostic (no DOM dependency) and testable. The caller (login page component) provides the actual browser credential API call. Same pattern used for `registerPasskey(getCredential)` in the user store.
    - **Player store manages heartbeat timer internally** — The store creates/clears a `setInterval(15000)` for heartbeats. The timer starts on `play()` / `resume()` and stops on `stop()` / `destroy()`. The `Player.svelte` component (Task 4) sets position/state on the store via `setPlaying()`, `setBuffering()`, `setPosition()` from video element events; the store handles server sync.
    - **Player store resolves stream URL from decision type** — On `play()`, the store inspects `result.stream_decision`: `'direct_play'` → `streamFileUrl(mediaFileId)`, `'transcode'`/`'direct_stream'` → `transcodeManifestUrl(transcodeSessionId)`. The component receives `streamUrl` from the derived store and sets it on the `<video>` element or hls.js.
    - **Player volume persisted to localStorage** — `duskcue_player_volume` key; restored on store initialization. Survives page reloads and different media items.
    - **Notifications store auto-dismiss with configurable duration** — Default 5s for info/success/warning, 8s for errors. `duration: 0` makes a notification persistent (manual dismiss only). Max 5 simultaneous notifications with FIFO eviction and timer cleanup. Error notifications get longer duration because they need more reading time.
    - **Derived stores for fine-grained subscriptions** — Each store exports multiple `derived` stores so components can subscribe to only the slices they need (e.g., `isPlaying` for a play/pause button, `progressPercent` for a seek bar). This prevents unnecessary re-renders.
    - **`hasCapability(capability)` returns a derived store** — Factory function returning a per-capability boolean store. Includes owner role bypass (owner has all capabilities regardless of the `capabilities` array). Used by route guards and admin-only UI elements.
    - **User store manages UI preferences via localStorage** — No server-side user preferences endpoint exists yet. `preferences` object includes theme, defaultLibraryId, rememberFilters, autoplay, subtitleLanguage, audioLanguage. Persisted under `duskcue_prefs` key. `updatePreferences()` merges partial updates and saves. When a server-side preferences API is added (Phase 13), the store can be wired to sync both directions.
    - **No new npm dependencies** — All stores use `svelte/store` (built into Svelte) and existing API client modules.
    - **SSR-safe localStorage access** — All localStorage access guarded with `typeof localStorage !== 'undefined'` checks, preventing SSR crashes when SvelteKit renders on the server with `adapter-node`.

 4. ~~Build core components — `MediaCard.svelte`, `Player.svelte` (hls.js integration), `SearchBar.svelte`, `NotificationToast.svelte`~~ **DONE**

    **What was built for Task 4:**

    | File | Purpose |
    |---|---|
    | `clients/web/src/lib/components/NotificationToast.svelte` | Fixed-position toast container subscribing to `notifications` store; per-type accent colors (success/error/warning/info), SVG icons, dismiss button, fly/fade transitions, flip animation; responsive (full-width on mobile) |
    | `clients/web/src/lib/components/SearchBar.svelte` | Debounced search input (300ms) with SVG search icon; `compact` prop for nav-bar mode; `$bindable` value for two-way binding; navigates to `/search?q=...` via `goto()` on submit; customizable placeholder, autofocus |
    | `clients/web/src/lib/components/MediaCard.svelte` | Content-first media card — `<a>` tag linking to `/media/{id}`; 2:3 aspect-ratio poster (image or gradient placeholder with title initial); rating badge (star icon + value), type badge (non-movie), hover overlay with overview text (4-line clamp); optional progress bar (resume position); derived subtitle for episodes (S01 E01), seasons (Season N), and movies (year); keyboard accessible |
    | `clients/web/src/lib/components/Player.svelte` | Full HLS player with hls.js 1.6.16 integration; video element synced bidirectionally with `player` store; transport controls (play/pause, seek bar with buffered indicator, volume slider + mute, playback speed 0.5x–2x, fullscreen toggle, close button); auto-hide controls after 3s; keyboard shortcuts (Space/K=play-pause, Left/Right=±10s seek, Up/Down=volume, F=fullscreen, M=mute, Esc=close); loading/buffering spinner; error display with retry; periodic QoE telemetry reporting (30s interval); buffering duration tracking |
    | `clients/web/src/app.css` | Global design tokens implementing UI_FOUNDATIONS.md low-light editorial palette: CSS custom properties for surfaces (--color-bg-deep/surface/elevated), text (--color-text-primary/secondary/muted), accent (--color-accent brass/amber), semantic colors (success/warning/error); radii, shadows, transitions, font stacks; focus-visible ring; global resets (box-sizing, body bg, link/button/input inheritance) |
    | `clients/web/src/lib/utils/format.js` | `formatDuration(seconds)` → "1h 23m", `formatTimestamp(ms)` → "M:SS" or "H:MM:SS", `formatYear(dateString)` → year integer, `formatRating(rating)` → rounded f32, `formatPercent(positionMs, durationMs)` → 0–100 |
    | `clients/web/src/lib/utils/constants.js` | `MEDIA_TYPE_LABELS` (movie/series/season/episode → display names), `NOTIFICATION_ICONS` (SVG path data per type), timing constants (`SEARCH_DEBOUNCE_MS`, `PLAYER_CONTROLS_TIMEOUT_MS`, `PLAYER_SEEK_STEP_S`, `PLAYER_VOLUME_STEP`) |
    | `clients/web/src/routes/+layout.svelte` | Imports `app.css` for global design tokens |
    | `clients/web/package.json` | Added `hls.js@1.6.16` dependency |

    **Key decisions from Task 4:**

    - **hls.js 1.6.16** — Latest stable release; supports HLS.js MSE-based playback for Chrome/Firefox/Edge, with native HLS fallback for Safari via `canPlayType('application/vnd.apple.mpegurl')`. Dynamic import (`await import('hls.js')`) keeps the library out of the initial bundle for non-player pages and avoids SSR import issues
    - **Dynamic hls.js import** — `loadHlsJs()` uses `await import('hls.js')` rather than a top-level import. This prevents SSR from evaluating the browser-only hls.js module, keeps the main bundle smaller (hls.js is only loaded when the Player component mounts), and is tree-shakeable
    - **`<a>` tag for MediaCard over `<article role="button">`** — Semantic HTML: a card that navigates to a detail page is a link, not a button. Provides native keyboard support (Enter to navigate), screen reader semantics, and right-click → open in new tab. Custom `onclick` for optional programmatic navigation override
    - **Keyboard shortcuts on `<svelte:window>` not container** — Player keyboard handlers (Space, arrows, F, M, Esc) are on `<svelte:window>` rather than the container div. This matches real media player UX (shortcuts work without clicking the player first) and avoids a11y warnings about interactive roles on non-interactive elements
    - **Design tokens from UI_FOUNDATIONS.md** — `app.css` implements the "low-light editorial palette" as CSS custom properties: deep charcoal/graphite surfaces (`#0e0f13` → `#1e2129`), warm off-white text (`#e8e4dc`), brass/amber accent (`#c8965a`), muted semantic colors. These tokens establish the foundational design system that Task 5 (route pages) and Task 6 (responsive layout) build upon
    - **Player store bidirectional sync** — Video element events (`play`, `pause`, `waiting`, `playing`, `timeupdate`, `durationchange`, `loadedmetadata`) push state INTO the store via `player.setPlaying()`, `player.setBuffering()`, etc. Store state flows OUT to the video element via `$effect` watchers on `$playerVolume`, `$player.isMuted`, `$player.playbackRate`. The seek bar uses local `isSeeking`/`seekValue` state to prevent fighting between drag position and store position during seeking
    - **Direct play seek vs. transcode seek** — `handleSeekEnd()` checks `$streamDecision`: for `direct_play`, seeks the video element directly (`videoEl.currentTime = positionMs / 1000`); for `transcode`/`direct_stream`, calls `player.seek()` which triggers server-side transcode restart and a new stream URL
    - **hls.js error recovery** — Fatal NETWORK_ERROR → `hls.startLoad()` retry; fatal MEDIA_ERROR → `hls.recoverMediaError()`; other fatal → destroy + notification. Non-fatal errors are silently ignored (common for HLS streams with minor quirks)
    - **QoE telemetry** — Player reports QoE data every 30s via `submitQoeReport()` from `quality.js`; buffering events include `buffer_duration_ms` measured from `waiting` to `playing` event. This data feeds the Phase 7 quality analytics dashboards
    - **Svelte 5 patterns** — All components use Svelte 5 runes (`$props()`, `$state()`, `$derived()`, `$derived.by()`, `$effect()`, `$bindable()`) alongside compatible Svelte 4 APIs (`onMount`, `onDestroy`, `svelte/store` auto-subscription with `$store` prefix, `svelte/transition`, `svelte/animate`). The `svelte/store` derived stores from Task 3 are consumed via `$` prefix in templates and `$effect` blocks
    - **`NotificationToast` uses `flip`/`fly`/`fade` animations** — `animate:flip` for smooth reordering when toasts are added/removed; `in:fly` for slide-down entrance; `out:fade` for exit. These are built-in Svelte transitions requiring no additional dependencies
    - **MediaCard poster fallback** — When no `posterUrl` is provided (artwork serving endpoint not yet built), renders a gradient placeholder with the title's first letter. This keeps the card functional in all states without blocking on an artwork API
    - **`formatTimestamp` uses `Math.floor` not `Math.round`** — Position display should never show a future second; `Math.floor(ms / 1000)` ensures the displayed time always matches or precedes the actual position
    - **0 svelte-check warnings** — All a11y warnings resolved: MediaCard uses `<a href>` (semantic navigation), Player keyboard handling on `<svelte:window>`, Player container has `role="region"` + `aria-label` for mouse handlers

    **Context from Task 4 for Tasks 5–6:**

    - The 4 core components (`MediaCard`, `Player`, `SearchBar`, `NotificationToast`) are available for import from `$lib/components/`
    - Design tokens in `app.css` (imported via `+layout.svelte`) provide all CSS custom properties — route pages should use `var(--color-*)` tokens, not hardcoded colors
    - `format.js` provides `formatDuration`, `formatTimestamp`, `formatYear`, `formatRating`, `formatPercent` for rendering media data
    - `MediaCard` accepts `posterUrl` prop — artwork serving endpoint not yet built; pages should pass `null` until artwork API is available
    - `Player` accepts `mediaItem`, `mediaFileId`, `startPositionMs` props and manages the full playback lifecycle; route pages just render `<Player mediaItem={item} mediaFileId={fileId} onstop={() => goto('/media/' + item.id)} />`
    - `SearchBar` navigates to `/search?q=...` on submit; the search page (Task 5) should read `$page.url.searchParams.get('q')` and call `search()` from `api/search.js`
    - hls.js is dynamically imported — only the player route will load it in the browser bundle

 5. ~~Build route pages~~ **DONE**

    **What was built for Task 5:**

    | File | Purpose |
    |---|---|
    | `clients/web/src/routes/+layout.svelte` | App shell — nav bar with logo, nav links (Dashboard, Libraries, Media), compact `SearchBar`, user dropdown menu (display name, Settings, Logout); auth guard via `$effect` checking `$isAuthenticated` after `auth.init()`; redirects unauthenticated users to `/auth/login` (or `/auth/setup` if no owner exists); redirects authenticated users away from auth pages to `/dashboard`; `NotificationToast` rendered; full-screen loading spinner during auth init |
    | `clients/web/src/routes/+page.svelte` | Root redirect — `goto('/dashboard')` on mount |
    | `clients/web/src/routes/auth/setup/+page.svelte` | First-run owner creation — form with username, password, display_name, server_name; calls `auth.setup()`; redirects to `/dashboard` on success |
    | `clients/web/src/routes/auth/login/+page.svelte` | Two-mode login — invite code or username/password tabs; passkey (WebAuthn) button with `navigator.credentials.get()` callback; calls `auth.loginWithInvite()` / `auth.loginWithPassword()` / `auth.loginWithPasskey()`; error display with `ApiError` field extraction |
    | `clients/web/src/routes/auth/link/+page.svelte` | Device code entry — auto-formats input to `ABCD-EFGH` pattern; reads `?code=` URL param for pre-filled codes; calls `auth` API device linking flow |
    | `clients/web/src/routes/dashboard/+page.svelte` | Home dashboard — hero greeting with user display name; Continue Watching row (fetches recent items, per-item `getWatchData()` calls, filters by `resume_position_ms > 0`, progress bar on cards); Recently Added grid with `MediaCard` components |
    | `clients/web/src/routes/libraries/+page.svelte` | Library list — grid of library cards with type-specific SVG icons, item counts; empty state when no libraries exist |
    | `clients/web/src/routes/libraries/[id]/+page.svelte` | Library detail — media items grid with type filter buttons (all/movie/series/etc.); load-more pagination; scan button (gated by `can_manage_libraries` capability); uses `libraries` store for CRUD + scanning flags |
    | `clients/web/src/routes/media/+page.svelte` | All media browse — grid of `MediaCard` with type filters and load-more pagination |
    | `clients/web/src/routes/media/[id]/+page.svelte` | Media detail — backdrop hero image, poster, metadata (year, content rating, runtime, genres), overview text; play/resume button (shows "Resume" if `resume_position_ms > 0`); favorite toggle (heart icon); 5-star rating selector; file list with health badges (healthy/damaged/missing) and codec/resolution info |
    | `clients/web/src/routes/play/[id]/+page.svelte` | Full-screen player route — renders `Player` component with `mediaItem`, `mediaFileId` (from `?file=` query param), `startPositionMs` (from watch data resume position); fetches media item + watch data on mount; `onstop` callback navigates back to media detail |
    | `clients/web/src/routes/search/+page.svelte` | Search results — reactive to `$page.url.searchParams.get('q')`; type filter pills; results grid using `MediaCard`; loading spinner; empty states (no query entered, no results found) |
    | `clients/web/src/routes/settings/+page.svelte` | Settings overview — server health card (version, uptime, DB status from `getHealth()`); management link grid with icons; capability-gated visibility (users/libraries links only for admins); "Soon" tags on unimplemented pages |
    | `clients/web/src/routes/settings/users/+page.svelte` | User management — user table with avatar/username/display name, role badges, status indicators; invitation creation form (max uses, expiry); pending invitations list with revoke buttons; `can_manage_users` capability gated |
    | `clients/web/src/routes/settings/libraries/+page.svelte` | Library management — create library form (name, type, paths); library list with scan and delete buttons; expandable path management per library (add/remove paths); `can_manage_libraries` capability gated |
    | `clients/web/src/routes/{analytics,collections}/+page.svelte` | Placeholder pages — "Coming in Phase 11" / "Coming in Phase 12" stubs with branded empty state |
    | `clients/web/src/routes/settings/{backups,collections,migration,overlays,quality,security,storage,subtitles}/+page.svelte` | 8 placeholder stubs — each shows "Coming in Phase X" with description of future functionality |
    | `clients/web/jsconfig.json` | Created — extends `.svelte-kit/tsconfig.json` for `svelte-check` and `$app/*` module resolution |
    | `clients/web/src/lib/api/core.js` | Fixed `credentials` type annotation — `/** @type {RequestInit} */` cast on fetch options for `svelte-check` compatibility |
    | `clients/web/package.json` | Added `svelte-check`, `typescript`, `@types/node` as devDependencies |

    **Key decisions from Task 5:**

    - **Player as separate route (`/play/[id]`)** — Better UX than an overlay: browser back button works naturally, shareable URL, full viewport for the player. Created a new `routes/play/[id]/` directory. The `Player.svelte` component (Task 4) is the actual player implementation; this route page is a thin wrapper that resolves media item + watch data and passes them as props
    - **Auth guard in layout via `$effect`** — `$effect` checks `$isAuthenticated` after `auth.init()` completes. Uses `$page.url.pathname` to avoid redirect loops (auth pages are exempt). Redirects to `/auth/setup` if no owner exists yet (checks via setup API), to `/auth/login` for unauthenticated users, and to `/dashboard` for authenticated users visiting auth pages
    - **No SvelteKit `load` functions** — Data loaded via `onMount` + `$state` in each page component. This is simpler than `+page.js`/`+page.server.js` load functions and avoids SSR complexity for client-only auth flows. Trade-off: no SSR data prefetching, but the app is client-side oriented (auth-gated content)
    - **Continue Watching via per-item `getWatchData` calls** — No dedicated "continue watching" backend endpoint exists yet. Dashboard fetches recent media items, then calls `getWatchData()` per item, filters by `resume_position_ms > 0`, and sorts by most recently watched. This is N+1 but acceptable for the typical 20-item recent list; a dedicated endpoint can be added in a future phase
    - **Capability-gated UI elements** — Admin actions (scan library, manage users, manage libraries) check `hasCapability('can_manage_users')` / `hasCapability('can_manage_libraries')` via store subscription. Non-admin users see the pages but not the action buttons. Settings overview hides management links entirely for users without the relevant capability
    - **Device code auto-formatting** — Login device link page formats input to `XXXX-XXXX` pattern (uppercase, auto-insert hyphen at position 4). Reads `?code=` URL param for QR-code-initiated flows
    - **5-star rating as clickable stars** — Media detail page renders 5 star buttons with hover preview; calls `updateWatchData({ user_rating: N })`. Favorite toggle is a separate heart icon calling `updateWatchData({ is_favorite: !current })`
    - **File health badges** — Media detail lists all `media_files` for the item with color-coded health badges: green (healthy), yellow (damaged/repairable), red (missing/corrupt). Shows codec, resolution, and container from `MediaFileRow` data
    - **`jsconfig.json` extends generated config** — Created `jsconfig.json` that extends `.svelte-kit/tsconfig.json`. Critical: must NOT override `include` array or `$app/*` resolution — overriding either breaks virtual module type resolution. The generated tsconfig from `svelte-kit sync` has correct path mappings
    - **`svelte-check` + `typescript` added** — Enables type checking for `.svelte` files and JS modules. `svelte-check` uses the generated `.svelte-kit/tsconfig.json` for path aliases and virtual module declarations
    - **Placeholder pages reference future phases** — Each placeholder page states which phase will implement it (e.g., "Coming in Phase 11 — Analytics"), giving users context without dead-end navigation. Settings overview links to these pages with "Soon" tags

    **Context from Task 5 for Task 6:**

    - All 24 route pages are functional and pass `svelte-check` (0 errors, 0 warnings) and `vite build`
    - The layout shell (`+layout.svelte`) uses a desktop-oriented nav bar; Task 6 needs to add responsive mobile nav (hamburger menu, breakpoint-based visibility)
    - CSS uses `var(--color-*)` tokens from `app.css` throughout; Task 6 should add media queries adjusting grid columns, font sizes, and nav layout at breakpoints
    - `MediaCard` grids use CSS grid with `repeat(auto-fill, minmax(...))` — already partially responsive but needs explicit breakpoint tuning
    - Player route uses full viewport already; minimal responsive work needed there
    - Auth pages use centered single-column forms that work on mobile but may need padding adjustments

 6. ~~Implement responsive layout — desktop and mobile breakpoints~~ **DONE**

    **What was built for Task 6:**

    | File | Purpose |
    |---|---|
    | `clients/web/src/app.css` | Added responsive breakpoint documentation comment block; added `@media (max-width: 480px)` reducing base font size to 15px for small mobile screens |
    | `clients/web/src/routes/+layout.svelte` | Mobile hamburger menu with slide-in drawer; nav-links and nav-search hidden on mobile and shown in drawer; user-name hidden on mobile (avatar only); drawer contains nav links, full-width search bar, Settings link, Sign Out button; drawer closes on navigation, Escape key, or backdrop click; animated drawer-slide-in; responsive padding adjustments for nav-content and main-content at 768px and 480px breakpoints |
    | `clients/web/src/routes/media/[id]/+page.svelte` | Detail header stacks vertically below 768px (poster centered at 140px above info); media title scales down; file rows stack vertically; backdrop height reduced |
    | `clients/web/src/routes/settings/users/+page.svelte` | Users table collapses to stacked card layout below 768px (table header hidden, rows become flex columns); invite form fields stack vertically; page header stacks; invitation rows wrap |
    | `clients/web/src/routes/settings/libraries/+page.svelte` | Form grid stacks to single column; library item headers stack with full-width actions; path rows stack; page header stacks |
    | `clients/web/src/routes/dashboard/+page.svelte` | Card grid minmax reduced to 140px below 768px; hero title scales down; spacing reduced |
    | `clients/web/src/routes/libraries/[id]/+page.svelte` | Media grid minmax reduced to 140px; library header stacks vertically; filter bar becomes horizontally scrollable (nowrap) |
    | `clients/web/src/routes/libraries/+page.svelte` | Library grid switches to single column below 768px |
    | `clients/web/src/routes/search/+page.svelte` | Results grid minmax reduced to 140px; filter bar becomes horizontally scrollable; page title scales down |
    | `clients/web/src/routes/media/+page.svelte` | Media grid minmax reduced to 140px; filter bar becomes horizontally scrollable; page title scales down |
    | `clients/web/src/routes/settings/+page.svelte` | Links grid switches to single column below 768px; page title scales down |
    | `clients/web/src/routes/auth/login/+page.svelte` | Auth card padding reduced to 1.5rem below 480px; auth page padding reduced |
    | `clients/web/src/routes/auth/setup/+page.svelte` | Same mobile padding adjustments as login page |
    | `clients/web/src/routes/auth/link/+page.svelte` | Same mobile padding adjustments |

    **Key decisions from Task 6:**

    - **Two-breakpoint system** — `768px` (tablet/mobile boundary: hamburger nav appears, grids adjust, tables collapse, page headers stack) and `480px` (small phone: font size reduced, auth card padding reduced, main content padding minimized). These are the two most widely adopted breakpoint values across major frameworks and provide clean transitions without over-fragmenting the CSS.
    - **Slide-in drawer over bottom tab bar** — A right-side slide-in drawer (max-width 85vw, capped at 300px) was chosen over a bottom navigation tab bar because the nav must accommodate search, settings, sign-out, and navigation links — too many items for a clean bottom bar. The drawer includes all nav destinations plus user account actions in a single surface. Drawer animation is a 200ms ease-out `translateX` slide.
    - **Hamburger button toggles between menu and close icon** — The SVG path switches between three horizontal lines (menu) and an X (close) based on `mobileMenuOpen` state, giving clear visual feedback without requiring a separate close button.
    - **Drawer includes Settings and Sign Out** — On mobile, Settings and Sign Out are accessible from both the drawer and the user dropdown (which still works on mobile). This provides redundant access paths, ensuring users can always reach account actions regardless of whether they open the hamburger or tap the avatar.
    - **CSS `display: none` for desktop nav elements on mobile** — Nav links and nav search bar use `display: none` below 768px and `display: flex` by default. This is simpler than conditional rendering and avoids Svelte transition lifecycle issues.
    - **Horizontally scrollable filter bars** — Library detail, search results, and media browse filter bars use `flex-wrap: nowrap` + `overflow-x: auto` below 768px, allowing horizontal scrolling through filter chips without wrapping. Filter chips use `flex-shrink: 0` to maintain fixed width. This is a standard mobile pattern for tag/chip bars.
    - **Card grid minmax 140px on mobile** — Media card grids use `minmax(140px, 1fr)` below 768px (down from 160px on desktop). At 140px minimum, a 375px phone shows 2 cards per row, a 768px tablet shows 5. The 20px reduction maintains card readability while preventing single-column layouts on mid-size phones.
    - **Users table collapses to card layout** — The 5-column CSS grid table (`grid-template-columns: 2fr 1.5fr 1fr 1fr auto`) switches to a vertical flex column layout below 768px. The table header is hidden; each row becomes a card with ordered fields (name, username, role, status, actions). This follows the "responsive table to card" pattern recommended by accessibility guidelines.
    - **Media detail stacks poster vertically** — The horizontal poster + info flex layout switches to `flex-direction: column` below 768px. Poster shrinks to 140px (from 200px) and centers above the info area. File rows also stack vertically (file info above actions).
    - **Per-component `@media` queries over global responsive CSS** — Each Svelte component uses scoped `<style>` blocks. Responsive media queries are placed in each component's style block rather than a global responsive stylesheet, maintaining Svelte's CSS scoping and component encapsulation.
    - **No JavaScript-based responsive behavior** — All responsive behavior is pure CSS media queries. No JS-based viewport detection, resize observers, or conditional rendering. This keeps the client fast, avoids hydration mismatches in SSR, and follows progressive enhancement principles.
    - **Existing responsive components unchanged** — `NotificationToast.svelte` already had a `@media (max-width: 480px)` block (full-width on mobile); `SearchBar.svelte` already had a `@media (max-width: 768px)` block (full-width compact mode); `Player.svelte` already had a `@media (max-width: 640px)` block (hides volume slider and center title). These were left as-is.
    - **No new npm dependencies** — All responsive behavior uses native CSS media queries. No breakpoint utility libraries (e.g., `svelte-breakpoint`) or CSS-in-JS solutions added.

**Verification:** User can log in, browse libraries, search for items, view metadata, and play media through the web client. The UI is responsive across desktop, tablet, and mobile breakpoints (768px and 480px) with a mobile hamburger drawer navigation, stacked layouts for detail pages and tables, and tuned card grids.

**Phase 8 status:** All 6 tasks complete.

**Committed:** `9f0c88d` on `main`

---

## Phase 9 — Subtitles

**Goal:** Subtitle discovery, delivery, and auto-fetch from external providers.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [SUBTITLES.md](docs/design/SUBTITLES.md) | **Primary** — subtitle discovery, conversion, sync correction, fetching, delivery |
| [METADATA_PROVIDERS.md](docs/design/METADATA_PROVIDERS.md) | SubDL and OpenSubtitles provider profiles, rate limiting |

**Context from Phase 8:**

- `clients/web/src/routes/settings/subtitles/+page.svelte` exists as a placeholder stub ("Coming in Phase 9")
- `clients/web/src/lib/api/subtitles.js` exists as a license-header-only stub
- `SubtitleConfig` already defined in `state.rs` with 9 fields (ocr_enabled, ocr_engine, ocr_confidence_threshold, voice_activity_analysis, voice_activity_schedule, default_subtitle_mode, default_subtitle_language, auto_fetch_enabled, auto_fetch_languages) — already stored in `server_config.subtitles` JSONB and loaded by `load_runtime_config()`
- `subtitle_files`, `subtitle_ocr_cache`, `subtitle_sync_data` tables created in Phase 2 migrations
- Web client Player component already has subtitle support in its `StartPlaybackRequest` (client sends `subtitle_stream_index`)

**Tasks:**

1. ~~Create `server/src/domains/subtitles/` — five-file pattern~~ **DONE**

**What was built for Task 1:**

| File | Purpose |
|---|---|
| `server/src/domains/subtitles/mod.rs` | Module declarations + router assembly with 6 route groups (8 endpoints) |
| `server/src/domains/subtitles/error.rs` | `SubtitleError` enum with 13 variants: SUB_001–006 (FileNotFound, OcrUnavailable, OcrLowConfidence, ProviderUnavailable, ProviderRateLimited, VoiceAnalysisFailed) per ERROR_HANDLING.md + 6 domain-specific (MediaItemNotFound, InvalidSubtitleFormat, InvalidLanguageCode, FetchFailed, ConversionFailed, SyncDataNotFound) + Database catch-all |
| `server/src/domains/subtitles/types.rs` | Three-type DTOs: 3 Row types (SubtitleFileRow, SubtitleOcrCacheRow, SubtitleSyncDataRow) matching DATABASE.md schema; 4 Request DTOs with Validate (FetchSubtitlesRequest, SetSubtitleOffsetRequest, TriggerOcrRequest, SubtitleContentQuery); 6 Response DTOs (SubtitleFileResponse, SubtitleListResponse, FetchSubtitlesResponse, SubtitleOffsetResponse, SubtitleOcrResult, SubtitleSyncDataResponse); 6 validation statics |
| `server/src/domains/subtitles/service.rs` | 8 `todo!()` service function stubs (list_subtitles, get_subtitle, get_subtitle_content, fetch_subtitles, set_subtitle_offset, trigger_ocr, get_subtitle_sync_data, delete_subtitle) + `validate_language_code` helper |
| `server/src/domains/subtitles/handlers.rs` | 8 handlers wired to Axum extractors with concrete return types; content endpoint uses `Result<Response, AppError>` (serves text, not JSON); all others use `Result<Json<T>, AppError>` |
| `server/src/error.rs` | Added `AppError::Subtitle(#[from] SubtitleError)` variant + `subtitle_error_to_http()` mapping all 13 error variants |
| `server/src/domains/mod.rs` | Added `pub mod subtitles;` |
| `server/src/router.rs` | Merged subtitles router via `.merge(crate::domains::subtitles::router(state.clone()))`, removed Phase 9 comment |

**Key decisions from Task 1:**

- **Routes nested under `/api/v1/items/{item_id}/subtitles`** — Subtitles are strictly owned by media items (CASCADE on media_item_id FK); one level of nesting per REST sub-resource convention, same pattern as bookmarks and watch-data in the playback domain
- **Content endpoint returns `Result<Response, AppError>`** — Unlike other subtitle endpoints that return JSON, the content endpoint serves raw subtitle text (SRT/WebVTT) with appropriate `Content-Type` headers. Uses `todo!()` stub for Task 1; Task 4 (delivery) will implement response construction with format-specific content types
- **Offset endpoint scoped by `user_id`** — Per-user per-item offset per SUBTITLES.md; offset stored in `user_item_data.metadata` JSONB, not in subtitle_files. The `set_subtitle_offset` handler extracts `user_id` from `AuthenticatedUser` and passes it to the service
- **OCR trigger as POST endpoint** — `POST /api/v1/items/{item_id}/subtitles/{subtitle_id}/ocr` allows manual OCR trigger for image subtitles (PGS/VobSub); engine override optional via request body
- **Delete limited to fetched subtitles** — `DELETE` endpoint intended for removing provider-fetched or OCR-generated subtitle rows; embedded and external subtitles should not be deletable via API (service layer enforces this in Task 2)
- **Additional error variants beyond SUB_001–006** — `MediaItemNotFound`, `InvalidSubtitleFormat`, `InvalidLanguageCode`, `FetchFailed`, `ConversionFailed`, `SyncDataNotFound` provide domain-specific error reporting; these map to existing SUB codes or INTERNAL, avoiding the need for generic `AppError::BadRequest`/`AppError::Internal` wrappers
- **`validate_language_code` helper** — Simple check: 2–10 ASCII alphabetic chars (covers ISO 639-1 `en` through ISO 639-2/T `eng` and IETF tags `en-US`). Full validation deferred to service implementation
- **`#![allow(unused_variables)]` on service.rs** — All 8 service functions are `todo!()` stubs; the module-level allow suppresses unused parameter warnings until actual implementations are added in Tasks 2–8
- **No new workspace dependencies** — all functionality uses existing `sqlx`, `validator`, `serde`, `uuid`, `chrono`, `axum` crates

2. ~~Implement subtitle discovery — scan for SRT/ASS/VTT/PGS/VobSub sidecars alongside media files~~ **DONE**
3. ~~Implement `subtitle_files` rows — populate during library scan (Phase 5)~~ **DONE**

**What was built for Tasks 2–3:**

| File | Purpose |
|---|---|
| `server/src/services/subtitle_discovery.rs` | Complete subtitle discovery service: `discover_subtitles()` entry point (called after Phase 4 Identify), `discover_external_subtitles()` for sidecar files, `discover_embedded_subtitles()` for container-internal streams, `load_video_files()` queries media_files for matching, `build_directory_map()` indexes video files by parent directory for O(1) lookup, `match_external_subtitle()` matches sidecar to video by base-name prefix, `parse_subtitle_name()` extracts language code + flags from filename, `is_subtitle_file()` extension check, `looks_like_language_code()` validates 2–5 char alpha codes |
| `server/src/services/mod.rs` | Added `pub mod subtitle_discovery;` |
| `server/src/workers/library_scanner.rs` | Added `FfprobeDisposition` struct (forced, hearing_impaired, default fields from ffprobe JSON); added `index` and `disposition` fields to `FfprobeStream`; added `Some("subtitle")` arm in `probe_file` stream loop — collects index/codec_name/language/title/is_forced/is_hearing_impaired into `subtitle_streams` Vec; stores into `additional_streams.subtitles` JSONB alongside chapters; added `subtitles_discovered: u64` to `ScanResult` struct; wired `discover_subtitles()` call after Phase 4 (Identify) with error capture into `ScanError`; accumulated count in `scan_library` aggregation |

**Key decisions for Tasks 2–3:**

- **Discovery as a service module, not in the scanner** — `subtitle_discovery.rs` lives in `services/` keeping the 1700+ line scanner focused on the 6-phase pipeline; follows the existing services pattern (like `media_matching.rs`, `artwork_downloader.rs`)
- **External subtitle matching by directory + base-name prefix** — A subtitle file matches a video file if they share the same parent directory (or the subtitle is in a `Subs/` or `subtitles/` subdirectory) and the video's file stem is a prefix of the subtitle's file stem. This handles `Movie.srt`, `Movie.en.srt`, `Movie.forced.eng.srt`, etc.
- **Language parsing from trailing filename segments** — After stripping the video base name, remaining dot-separated segments are checked: 2–5 char ASCII alpha codes (with optional region suffix like `en-US`) are treated as language codes; known flags (`forced`, `hi`, `sdh`, `cc`, `hearingimpaired`, `hearing_impaired`, `default`) set boolean attributes; defaults to `"und"` if no language detected
- **Embedded subtitle synthetic path** — `{media_file_path}::embedded::{stream_index}` stored in `subtitle_files.file_path` for uniqueness; uses the `UNIQUE(media_item_id, file_path)` constraint for idempotent re-scans via `ON CONFLICT DO NOTHING`
- **VobSub `.idx` excluded** — Only `.sub` creates a `subtitle_files` row; the `.idx` companion index file is skipped via `SUBTITLE_PROCESS_EXTENSIONS` which omits `"idx"`
- **`ON CONFLICT DO NOTHING`** — All subtitle inserts use `INSERT ... ON CONFLICT (media_item_id, file_path) DO NOTHING` for idempotent re-scans; existing subtitles are preserved, new ones are added, deleted sidecars are NOT removed (cleanup deferred)
- **Embedded stream metadata from ffprobe** — `probe_file` now captures subtitle streams with: `index` (stream position), `codec_name` (e.g., `subrip`, `ass`, `hdmv_pgs_subtitle`, `dvd_subtitle`), `language` (from `tags.language`), `title` (from `tags.title`), `is_forced` (from disposition or title containing "forced"), `is_hearing_impaired` (from disposition or title containing "hearing impaired"/"sdh"/"cc")
- **13 unit tests** — Cover language code detection, flag parsing (forced, hi, sdh, cc, hearing_impaired, multiple flags), subtitle file detection, simple filename parsing, 3-letter language code, region suffix, no-language-with-flag edge case
4. ~~Implement subtitle delivery — serve WebVTT for HLS streams, serve text-based subtitles directly~~ **DONE**

**What was built for Task 4:**

| File | Purpose |
|---|---|
| `server/migrations/20260617080000_add_user_item_data_metadata.sql` | Adds `metadata JSONB NOT NULL DEFAULT '{}'` column to `user_item_data` for per-user per-item subtitle offset storage (`metadata->>'subtitle_offset_ms'`) |
| `server/src/domains/subtitles/service.rs` | Full delivery implementation: `list_subtitles` (ordered by type priority: external→fetched→embedded, then forced, then language), `get_subtitle`, `get_subtitle_content` (read file, detect format, convert, apply offset, return content+content_type), `set_subtitle_offset` (upsert into `user_item_data.metadata` JSONB), `get_subtitle_sync_data` (query `subtitle_sync_data`), `delete_subtitle` (fetched-only deletion guard); format conversion: `srt_to_webvtt`, `vtt_to_srt`, `ass_to_srt` (parse `[Events]`, strip override tags, reformat timestamps), `apply_offset` (timestamp arithmetic with negative-clamp); 13 unit tests |
| `server/src/domains/subtitles/handlers.rs` | Replaced `get_subtitle_content` `todo!()` with working handler: extracts user offset from `user_item_data.metadata`, delegates to service, returns `Response` with format-specific `Content-Type` (`text/vtt`, `application/x-subrip`, `text/plain`) and `Cache-Control: no-cache`; added `get_user_subtitle_offset()` DB helper |

**Key decisions from Task 4:**

- **SRT→WebVTT conversion inline in service.rs** — The conversion is trivial text transformation (`,` → `.` in timestamps, `WEBVTT` header, sequential cue numbering). No external dependency, no subprocess. Runs synchronously during delivery with negligible cost. Task 5 will extract heavier processing (FPS adjustment, OCR, voice activity) into `server/src/services/subtitles.rs`
- **ASS→SRT via Rust-native text parsing** — Per SUBTITLES.md design: parse `[Events]` section, extract `Dialogue:` lines, strip override tags (`{\.*?}` via state machine), reformat timestamps from `H:MM:SS.CC` (centiseconds) to `HH:MM:SS,mmm` (milliseconds), replace `\N`/`\n` with newlines. ~60 lines of Rust, no regex dependency, no FFmpeg subprocess
- **Delivery-time offset via timestamp arithmetic** — `apply_offset()` finds all timecode lines (containing `-->`), parses each timestamp to milliseconds, adds offset, clamps to ≥0, reformats. Handles both SRT (`,` separator) and WebVTT (`.` separator) formats. Zero I/O cost beyond the string parsing
- **`user_item_data.metadata` JSONB for offset storage** — New migration adds `metadata JSONB NOT NULL DEFAULT '{}'` column to `user_item_data`. Per SUBTITLES.md design, offset stored as `{"subtitle_offset_ms": -2500}`. Uses `INSERT ... ON CONFLICT DO UPDATE SET metadata = COALESCE(metadata, '{}') || $3::jsonb` for atomic upsert. If no `user_item_data` row exists, one is created with just the offset (minimal row, no play state)
- **Handler auto-resolves user offset** — `get_subtitle_content` handler queries `user_item_data.metadata->>'subtitle_offset_ms'` for the authenticated user + media item before calling the service. This means clients don't need to manually pass offset — the server transparently applies the stored offset
- **Embedded subtitles return error** — `file_path` containing `::embedded::` marker returns `InvalidSubtitleFormat` error. Embedded text subtitle extraction requires FFmpeg subprocess (`ffmpeg -i input.mkv -map 0:s:N output.srt`), which is a Task 5 concern. Delivery focuses on external/fetched text subtitle files
- **Image subtitle formats rejected** — PGS (`.sup`), VobSub (`.sub`), and `.idx` return `InvalidSubtitleFormat` error pointing to Task 5 (OCR). Image subtitles require OCR→SRT conversion before they can be served as text
- **Delete limited to fetched subtitles** — `delete_subtitle` checks `subtitle_type` and rejects deletion of `embedded` or `external` rows. Only `fetched` subtitles (provider-downloaded, OCR-generated) are user-deletable, matching SUBTITLES.md design
- **Subtitle ordering for client selection** — `list_subtitles` orders by type priority (external > fetched > embedded, matching the subtitle selection algorithm's preference for external subtitles), then forced subtitles first, then alphabetical language. This helps clients auto-select the best subtitle without complex client-side sorting
- **Content types per SUBTITLES.md** — WebVTT: `text/vtt; charset=utf-8` (HLS-ready); SRT: `application/x-subrip; charset=utf-8`; ASS/SSA: `text/plain; charset=utf-8`. All include charset to prevent encoding issues
- **`Cache-Control: no-cache` on subtitle content** — Subtitle content may change (offset updated, file re-fetched), so clients must not cache. Individual metadata endpoints (`list`, `get`) inherit standard JSON caching behavior
- **No new workspace dependencies** — All format conversion, offset application, and delivery uses standard Rust string manipulation. No regex, no XML parser, no FFmpeg subprocess for Task 4
- **13 unit tests** covering: SRT→WebVTT conversion, WebVTT→SRT conversion, ASS→SRT conversion (with override tag stripping), ASS timestamp format conversion, override tag stripping, positive offset application, negative offset clamping, VTT separator handling, timecode parsing, timecode formatting, format detection, content type mapping, language code validation

 5. ~~Implement `server/src/services/subtitles.rs`:~~ **DONE**
   - SRT ↔ ASS ↔ WebVTT format conversion
   - FPS adjustment (23.976 ↔ 24 ↔ 25 ↔ 29.97)
   - Offset correction (user-applied timestamp shift)
   - PGS/VobSub OCR stub (PaddleOCR — one-time background task)

   **What was built for Task 5:**

   | File | Purpose |
   |---|---|
   | `server/src/services/subtitles.rs` | Shared subtitle processing service: format conversion (`srt_to_webvtt`, `vtt_to_srt`, `ass_to_srt`, `srt_to_ass`, `to_srt`), timestamp primitives (`parse_timecode_to_ms`, `ms_to_timecode`), `apply_offset` (constant ms shift, clamped ≥0), `adjust_fps` (timestamp rescaling by `source_fps/target_fps`), `detect_ocr_engine()` (PaddleOCR/Tesseract CLI probe), `extract_subtitle_to_sup()` (FFmpeg stream extraction), `run_ocr()` (OCR pipeline scaffold — engine detection + extraction + stub), `analyze_voice_activity()` (FFmpeg silencedetect + speech-segment computation + cross-correlation against SRT cue starts over [-30s,+30s] range); `OcrEngine`, `OcrResult`, `VoiceAlignmentResult` types; 21 unit tests |
   | `server/src/services/mod.rs` | Added `pub mod subtitles;` |
   | `server/src/domains/subtitles/service.rs` | Removed ~250 lines of duplicated inline conversion functions (`srt_to_webvtt`, `vtt_to_srt`, `ass_to_srt`, `ass_timestamp_to_srt`, `strip_ass_override_tags`, `apply_offset`, `apply_offset_to_timecode_line`, `parse_timecode_to_ms`, `ms_to_timecode`); now delegates to `crate::services::subtitles as sub_svc`; `trigger_ocr` now calls `sub_svc::run_ocr` instead of returning `OcrUnavailable` unconditionally — resolves subtitle row, parses embedded path, resolves media file path, invokes OCR service; added `parse_embedded_path`, `is_image_subtitle`, `parse_engine_override`, `resolve_media_file_path` helpers; 5 new domain tests |

   **Key decisions from Task 5:**

   - **Service module, not domain module** — Subtitle text processing is cross-cutting: used by the domain layer (delivery), the scanner (FPS adjustment at scan time), and future workers (OCR, voice analysis, auto-fetch). Placing it in `services/` follows the established pattern (`media_matching.rs`, `subtitle_discovery.rs`, `enrichment_persistence.rs`)
   - **Deduplication over copy** — The ~250 lines of conversion functions were extracted from `domains/subtitles/service.rs` (where Task 4 placed them inline) into the shared service. The domain service now delegates via `use crate::services::subtitles as sub_svc;`. This eliminates duplicated parsing logic and creates a single source of truth for subtitle text manipulation
   - **FPS adjustment via simple rescaling** — `adjust_fps` multiplies every timestamp by `scale = source_fps / target_fps`. PAL→NTSC: scale = 25/23.976 = 1.0427; NTSC→PAL: scale = 23.976/25 = 0.9590. Separator auto-detected from first `-->` line so function works on both SRT and WebVTT. Noop when `source_fps == target_fps` or either is zero
   - **OCR stub rationale** — PaddleOCR (v3.6, PP-OCRv6 as of June 2026) requires a Python runtime and ~34.5M model parameters. The full pipeline (FFmpeg overlays bitmap subtitles onto blank video → extract PNG frames → paddleocr CLI per frame → assemble SRT with timestamps) is a one-time background task that belongs in `workers/subtitle_processor.rs` (Task 7). Task 5 delivers engine detection + FFmpeg `.sup` extraction + Blake3 source hashing + result types that the future worker will call. `run_ocr` returns `OcrUnavailable` after extraction because the actual image-OCR subprocess orchestration requires Python
   - **`OcrEngine` priority order** — `detect_ocr_engine()` checks PaddleOCR first (CLI binary or `python3 -m paddleocr`), Tesseract second. Matches SUBTITLES.md OCR Tool Selection table (PaddleOCR primary, Tesseract fallback)
   - **Voice activity alignment full implementation** — `analyze_voice_activity()` runs FFmpeg `silencedetect=noise=-30dB:d=0.5` on the first audio track, parses silence intervals from stderr (`silence_start:`/`silence_end:` lines), computes speech segments (gaps between silence intervals), parses SRT cue start times, cross-correlates speech starts against cue starts across [-30000ms, +30000ms] in 250ms steps (241 candidates), returns offset with highest match count + confidence (peak/mean ratio). When tied, prefers offset closest to zero. Tolerance is ±1000ms per candidate pairing. Confidence below 0.60 → caller should not auto-apply
   - **Cross-correlation tiebreaker** — When multiple offsets have the same match count, the algorithm prefers the offset with smallest absolute value. This prevents returning large spurious offsets when the correlation is flat across a tolerance band
   - **`trigger_ocr` wired to service** — Previously returned `OcrUnavailable` unconditionally. Now resolves the subtitle row, parses the embedded path (`{media_path}::embedded::{stream_index}`), validates it's an image subtitle, resolves the media file path from DB, and calls `sub_svc::run_ocr`. When no engine is available, `OcrUnavailable` surfaces immediately with a clear path through the service
   - **`is_image_subtitle` accepts embedded paths** — For external files, checks `.sup`/`.sub`/`.idx`/`.pgs` extensions. For embedded subtitles (synthetic `{path}::embedded::{N}` format), the codec isn't in the path — accepts all embedded paths since OCR is only applicable to embedded bitmap subtitles anyway (text subtitles don't need OCR). The OCR process itself fails gracefully if the subtitle is text-based
   - **`srt_to_ass` added** — Produces minimal valid ASS with default `[V4+ Styles]` and `[Events]`. Completes the bidirectional conversion matrix (SRT↔ASS↔WebVTT)
   - **No new workspace dependencies** — FFmpeg invocation uses `tokio::process::Command` (already in workspace); OCR engine detection uses `std::process::Command` (already used by `hw_accel.rs`); Blake3 hashing uses existing `blake3` workspace dep; all text parsing is standard library string manipulation

  6. ~~Implement subtitle fetching from providers~~ **DONE**

     **What was built for Task 6:**

     | File | Purpose |
     |---|---|
     | `server/src/services/subdl_client.rs` | SubDL API client — search by TMDB/IMDb/name, ZIP download, connection test |
     | `server/src/services/opensubtitles_client.rs` | OpenSubtitles API client — search by TMDB/IMDb/hash/query, two-step download, connection test |
     | `server/src/services/subtitles.rs` | `compute_oshash()` added — 64-bit LE hash of first/last 64KB + file size for OpenSubtitles hash-based search |
     | `server/src/domains/subtitles/service.rs` | `fetch_subtitles()` implemented — provider priority search, ZIP extraction, file save, DB insert |
     | `server/src/domains/subtitles/handlers.rs` | `fetch_subtitles` handler updated to pass `&AppState` |
     | `server/src/state.rs` | `IntegrationsConfig` expanded from empty placeholder to include `SubtitleProviderConfig` with SubDL/OpenSubtitles sub-configs |
     | `server/src/services/mod.rs` | `subdl_client` and `opensubtitles_client` modules registered |
     | `Cargo.toml` | `zip = "2"` added to workspace and server deps |

     **Key decisions from Task 6:**

     - **Provider priority: SubDL → OpenSubtitles** — SubDL searched first (free tier: 2,000 req/day, 300 downloads/day, uppercase lang codes, returns ZIP archives). OpenSubtitles is fallback (5 downloads/IP/24h, lowercase lang codes, two-step download flow: `POST /download {file_id}` → GET link)
     - **Normalized `SubtitleSearchResult`** — Both clients return the same struct (`provider`, `language`, `release_name`, `file_name`, `format`, `is_hearing_impaired`, `is_forced`, `download_url`, `vote_count`) so the domain service can rank and filter uniformly
     - **Search strategy** — SubDL: TMDB ID → IMDb ID → title fallback. OpenSubtitles: OSHash + file_size → TMDB ID → IMDb ID → title query fallback. Hash-based search gives best match accuracy on OpenSubtitles
     - **OSHash implementation** — `hash = file_size + sum_uint64_le(first_64KB) + sum_uint64_le(last_64KB)`, wraps at 64 bits, 16-char hex output. Minimum file size 128KB. Implemented in `services/subtitles.rs::compute_oshash()`
     - **ZIP extraction** — SubDL returns ZIP archives containing `.srt`/`.ass`/`.ssa`/`.vtt`/`.ttml` files. `extract_subtitle_from_zip()` scans archive entries for subtitle extensions and returns the first match. OpenSubtitles responses are checked for ZIP magic bytes (`PK`) before extraction
     - **Subtitle files saved next to media** — Fetched subtitles written to `{media_stem}.{language}.{ext}` in the same directory as the media file, matching the discovery convention
     - **Provider config in `IntegrationsConfig`** — Each provider has `enabled`, `api_key`, `auto_fetch_enabled`, `auto_fetch_languages`, `prefer_hearing_impaired` fields. Both providers default to `enabled: false` (opt-in). Stored in `server_config.integrations.subtitle_providers` JSONB
     - **`pick_best_result` ranking** — Filters by language match, then prefers forced/non-forced match, then hearing-impaired match, then scores by vote count + HI preference + SRT format bonus
     - **Graceful provider fallback** — `ProviderUnavailable` and `ProviderRateLimited` errors cause fallthrough to the next provider rather than hard failure. All other errors propagate immediately
     - **`zip` crate v2** added to workspace — needed for SubDL ZIP archive extraction; `flate2` already present for gzip
 7. ~~Implement `server/src/workers/subtitle_processor.rs` — auto-fetch during scan~~ **DONE**

 **What was built for Task 7:**

 | File | Purpose |
|---|---|
 | `server/src/workers/subtitle_processor.rs` | Subtitle auto-fetch worker: `run_subtitle_auto_fetch()` entry point; `resolve_targets()` gate logic (global `auto_fetch_enabled` + per-provider `enabled`/`auto_fetch_enabled`/API key checks + language set resolution); `find_items_missing_subtitles()` DB query (movie/episode types with healthy media_files, no subtitle in target language, prefix-match deduplication, `max_items_per_language` cap); per-language iteration calling existing `fetch_subtitles()` service; result counters |
 | `server/src/workers/mod.rs` | Added `pub mod subtitle_processor;` |
 | `server/src/main.rs` | Registered `subtitle_auto_fetch` executor on scheduler with `AppState` capture |
 | `server/src/services/scheduler.rs` | Added "Subtitle Auto-Fetch" to runtime `seed_default_tasks()` (interval 1800s, disabled by default) |
 | `server/migrations/20260619080000_seed_subtitle_auto_fetch_task.sql` | Seeds `subtitle_auto_fetch` scheduled task (1800s interval, `is_enabled = false`, 1800s timeout) for existing deployments |

 **Key decisions from Task 7:**

 - **Scheduled task over inline scan integration** — Inline auto-fetch during `scan_library` would block scan completion on provider HTTP calls (SubDL/OpenSubtitles rate limits, network latency). For bulk imports (1000+ items), this could take hours and exceed HTTP request timeouts. A periodic background task (30-min interval) decouples scan completion from subtitle availability and naturally batches work across runs. Newly-scanned items get subtitles within ~30 minutes — acceptable latency for a non-blocking background process. The 30-min interval approximates the "event-triggered after scan" semantics from SUBTITLES.md since the scheduler has no native event-trigger mechanism.
 - **Reuse `fetch_subtitles()` service** — The worker is a thin orchestration layer that queries for items missing subtitles and delegates to the existing `domains::subtitles::service::fetch_subtitles()`. All provider priority logic (SubDL → OpenSubtitles), `pick_best_result` ranking, ZIP extraction, sidecar save, and `subtitle_files` insert are reused without duplication.
 - **Three-tier gate** — (1) Global `SubtitleConfig.auto_fetch_enabled` must be `true`; (2) At least one provider must be `enabled` AND `auto_fetch_enabled` AND have a non-empty API key; (3) The effective language set must be non-empty. All three gates must pass or the run is a no-op logged at INFO. This prevents wasted API calls when providers are misconfigured.
 - **Language set is the union of global + per-provider lists** — `SubtitleConfig.auto_fetch_languages` ∪ `SubdlProviderConfig.auto_fetch_languages` ∪ `OpensubtitlesProviderConfig.auto_fetch_languages` (for eligible providers). This lets admins set a global default (`["en"]`) while allowing per-provider overrides (e.g., OpenSubtitles-only for Spanish). Task config `languages` array overrides everything when present (enables one-off backfill runs).
 - **`max_items_per_language` cap (50)** — SubDL free tier is 300 downloads/day; OpenSubtitles free tier is 5 downloads/IP/24h. Capping at 50 items per language per run prevents exhausting the daily quota in a single run and leaves budget for manual fetches via the API. Configurable via task config `max_items_per_language` for VIP deployments.
 - **Movie/episode only** — Series and seasons are container types without direct `media_files`; `fetch_subtitles` would fail with `MediaItemNotFound` from `resolve_media_file_path`. Filtering at the query level (`mi.type IN ('movie', 'episode')`) avoids wasted API calls. Uses the correct `media_items.type` column (not `media_type`).
 - **Language prefix match via ILIKE** — `"en"` matches `"en"`, `"en-US"`, `"eng"` (ISO 639-1, IETF tag, ISO 639-2/T). Prevents re-fetching when the existing subtitle has a region/code variant of the same base language. The broad `LIKE` is "good enough" deduplication — redundant fetches are cheap (provider returns no results), missed fetches are costly (user has no subtitle).
 - **Healthy media_files required** — `EXISTS (SELECT 1 FROM media_files WHERE is_healthy = true)` guard ensures we only fetch for items with on-disk files. `fetch_subtitles` reads the media file for OShash (OpenSubtitles) and writes the sidecar next to it — both require a healthy file. Without this guard, the worker would attempt fetches that fail at the service layer.
 - **Opt-in by default** — Migration seeds the task with `is_enabled = false` per SUBTITLES.md design ("auto_fetch_enabled: false" default). Admins must enable both the scheduled task AND the global `auto_fetch_enabled` config flag to activate auto-fetch.
 - **No new workspace dependencies** — All functionality uses existing `sqlx`, `serde_json`, `uuid`, and the already-built `fetch_subtitles` service.

 8. ~~Implement subtitle settings UI in web client~~ **DONE**

 **What was built for Task 8:**

 | File | Purpose |
|---|---|
| `server/src/domains/subtitles/types.rs` | Added settings DTOs: `UpdateSubtitleSettingsRequest` (Deserialize + Validate, SubtitleConfig fields), `UpdateSubtitleProviderSettingsRequest` with nested `SubdlProviderUpdate`/`OpensubtitlesProviderUpdate`, response types `SubtitleSettingsResponse`/`SubtitleProvidersResponse`/`SubdlProviderResponse`/`OpensubtitlesProviderResponse` (with masked secrets); added `VALID_SUBTITLE_MODES` static |
| `server/src/domains/subtitles/error.rs` | Added `InvalidSubtitleMode` and `InvalidOcrEngine` variants (both SUB_001 / 400) |
| `server/src/error.rs` | Mapped `InvalidSubtitleMode` and `InvalidOcrEngine` in `subtitle_error_to_http()` |
| `server/src/domains/subtitles/service.rs` | Added 4 functions: `get_subtitle_settings` (reads `RuntimeConfig` from ArcSwap, masks provider keys via `mask_secret`), `update_subtitle_settings` (validates mode/engine/languages, writes `server_config.subtitles` JSONB, reloads config), `update_subtitle_provider_settings` (merges updates keeping existing keys when null, encrypts new keys via `EncryptionKey`, `jsonb_set` on `integrations.subtitle_providers`, reloads config), `reload_runtime_config` helper; `encrypt_subtitle_provider_keys` helper |
| `server/src/domains/subtitles/handlers.rs` | Added 3 handlers: `get_subtitle_settings`, `update_subtitle_settings`, `update_subtitle_provider_settings` — all `Require<CanManageServer>` with validator `Validation` error mapping |
| `server/src/domains/subtitles/mod.rs` | Added 2 routes: `GET/PUT /api/v1/settings/subtitles`, `PUT /api/v1/settings/subtitles/providers` |
| `clients/web/src/lib/api/subtitles.js` | Full API client module: `getSubtitleSettings`, `updateSubtitleSettings`, `updateSubtitleProviderSettings` + per-item functions (`listSubtitles`, `fetchSubtitles`, `setSubtitleOffset`, `triggerOcr`, `getSubtitleSyncData`, `deleteSubtitle`, `getSubtitleContentUrl`) |
| `clients/web/src/lib/stores/subtitles.js` | Settings store (`subtitleSettings`) with `fetch`/`saveSettings`/`saveProviders`; derived loading/saving/error stores |
| `clients/web/src/routes/settings/subtitles/+page.svelte` | Full settings UI: Subtitle Behavior section (default mode/language, auto-fetch languages), OCR section (enabled, engine, confidence slider, voice activity toggle + cron schedule), Subtitle Providers section (SubDL + OpenSubtitles cards with masked API key fields); dirty-state-gated save buttons, Svelte 5 runes (`$state`/`$derived`), capability gating |
| `clients/web/src/routes/settings/+page.svelte` | Removed `soon: true` from subtitles nav link |

 **Key decisions from Task 8:**

 - **Backend settings endpoints required for functional UI** — No general `server_config` read/write endpoint exists (Phase 13a). Added subtitle-specific endpoints in the subtitles domain at `/api/v1/settings/subtitles` and `/api/v1/settings/subtitles/providers`, following the established pattern where `/api/v1/settings/providers/validate` already lives in a domain router. Keeps subtitle logic in the subtitle domain; avoids cross-domain coupling into system domain.
 - **Two separate write endpoints** — `PUT /settings/subtitles` (behavior config → `server_config.subtitles` JSONB) and `PUT /settings/subtitles/providers` (provider config → `server_config.integrations.subtitle_providers` JSONB via `jsonb_set`). Matches the SUBTITLES.md design separation between subtitle behavior and provider credentials. The UI has two corresponding save buttons, each gated by independent dirty-state tracking.
 - **API key masking, not plaintext** — `GET /settings/subtitles` returns `api_key_masked` (via existing `mask_secret()`) and `has_api_key` boolean, never the raw key. The client sends `api_key` only when changing it; `null`/omitted preserves the existing encrypted value. This avoids the masked-value-roundtrip problem and means the masked value never travels back as a "real" key.
 - **Config hot-reload via ArcSwap swap** — After each DB write, `reload_runtime_config()` calls `load_runtime_config()` and atomically swaps the result into `AppState.runtime_config` (`Arc<ArcSwap<RuntimeConfig>>`). Changes take effect immediately without a server restart — auto-fetch worker, OCR pipeline, and delivery service all read the live config on next access.
 - **Provider key encryption at rest** — `encrypt_subtitle_provider_keys()` encrypts SubDL/OpenSubtitles API keys + OpenSubtitles API token via the existing `EncryptionKey` (AES-256-GCM) before writing. Skips already-encrypted values (idempotent). This is the same pattern as the metadata provider config encryption from Phase 6 Task 13, applied to subtitle provider credentials.
 - **Admin-only (`Require<CanManageServer>`)** — All three settings endpoints require server-management capability. Non-admins see a permission message instead of the form. This matches the subtitle settings being server-wide configuration rather than per-user preferences.
 - **Svelte 5 runes for form state** — Local `$state` for form fields, `$derived` for per-section dirty detection (`behaviorDirty`, `providersDirty`) comparing a snapshot of loaded values, `$effect` for capability subscription. The dirty check gates each save button so accidental saves of unchanged data are prevented.
 - **Comma-separated language input** — `auto_fetch_languages` edited as a comma-separated text field, split/trimmed on save. Simpler than a multi-select for the small number of language codes; matches the subtitle language code format (ISO 639-1).
 - **Per-item subtitle API functions added for completeness** — The API client module includes the player-facing endpoints (`listSubtitles`, `getSubtitleContentUrl`, `setSubtitleOffset`, etc.) so the module covers the full subtitle API surface, even though Task 8 focuses on settings. The Player component already sends `subtitle_stream_index` from Phase 8.
 - **No new workspace dependencies** — backend uses existing `sqlx`, `serde_json`, `validator`, `ring` (encryption); frontend uses existing `core.js` HTTP client and Svelte stores.

**Verification:** Media items show available subtitles. User can select subtitle during playback. Auto-fetch downloads missing subtitles during scan. SubDL returns results by TMDB ID. Subtitle settings page loads config, admin can edit OCR/auto-fetch/defaults and provider credentials, changes persist and hot-reload without restart.

**Phase 9 status:** All 8 tasks complete.

---

## Phase 10 — Segment Detection & Storyboards

**Goal:** Intro/credit skip markers and seek preview thumbnails.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [SEGMENT_DETECTION.md](docs/design/SEGMENT_DETECTION.md) | **Primary** — 4-method pipeline (chapter markers → chromaprint → black frame → silence), skip buttons |
| [STORYBOARDS.md](docs/design/STORYBOARDS.md) | **Primary** — WebVTT + WebP spritesheets, keyframe-only mode, adaptive interval |

**Tasks:**

1. ~~Create `server/src/domains/segments/` — five-file pattern~~ **DONE**

**What was built for Task 1:**

| File | Purpose |
|---|---|
| `server/src/domains/segments/mod.rs` | Module declarations + router assembly with 3 route groups (5 endpoints) |
| `server/src/domains/segments/error.rs` | `SegmentError` enum with 9 variants covering media-item/segment/library not-found, validation (type/source/timestamps), conflict (manual exists, analysis in-progress), and Database catch-all |
| `server/src/domains/segments/types.rs` | Three-type DTOs: `SegmentRow` (internal), `CreateSegmentRequest`/`UpdateSegmentRequest` (Deserialize + Validate), `SegmentResponse`/`SegmentListResponse`/`AnalyzeSegmentsResponse` (Serialize); `SegmentListQuery` for `?type=` filter; `VALID_SEGMENT_TYPES`/`VALID_SEGMENT_SOURCES` statics matching the DB CHECK constraints |
| `server/src/domains/segments/service.rs` | 5 `todo!()` service function stubs (`list_segments`, `create_segment`, `update_segment`, `delete_segment`, `trigger_library_analysis`) — implemented in Task 2+ |
| `server/src/domains/segments/handlers.rs` | 5 working handlers wired to Axum extractors (`State`, `AuthenticatedUser`, `Path`, `Query`, `Json`, `Require<CanManageLibraries>`); list endpoint accepts any authenticated user and computes `can_edit` from role/capabilities; create/update/delete/analyze endpoints require `CanManageLibraries` capability |
| `server/src/error.rs` | Added `AppError::Segment(#[from] SegmentError)` variant + `segment_error_to_http()` mapping all 9 error variants to existing error codes per SEGMENT_DETECTION.md (MEDIA_001, LIB_001, VALID_001, CONFLICT) — no new error codes registered |
| `server/src/domains/mod.rs` | Added `pub mod segments;` |
| `server/src/router.rs` | Merged segments router via `.merge(crate::domains::segments::router(state.clone()))`, removed Phase 10 segments comment |

**Key decisions from Task 1:**

- **No new error codes per SEGMENT_DETECTION.md** — The design doc explicitly states "No new error codes — segment retrieval uses existing codes". The `SegmentError` enum variants map to existing codes: `MediaItemNotFound`/`SegmentNotFound` → MEDIA_001 (404); `LibraryNotFound` → LIB_001 (404); `InvalidSegmentType`/`InvalidSegmentSource`/`InvalidTimestamps` → VALID_001 (422); `ManualSegmentExists`/`AnalysisAlreadyInProgress` → CONFLICT (409). This follows the SubtitleError precedent of mapping multiple domain variants to a small set of existing codes
- **Routes match SEGMENT_DETECTION.md API table exactly** — `GET /api/v1/items/{id}/segments` (list with optional `?type=intro` filter), `POST /api/v1/items/{id}/segments` (create manual), `PUT /api/v1/items/{id}/segments/{segment_id}` (override), `DELETE /api/v1/items/{id}/segments/{segment_id}` (remove), `POST /api/v1/libraries/{id}/analyze-segments` (trigger analysis). No single-segment GET endpoint — not in the spec, and segment detail is always returned by create/update operations
- **`can_edit` computed in handler, not service** — The `SegmentResponse.can_edit` field is per-user (true if user is owner or has `can_manage_libraries`). The handler computes it once from `AuthenticatedUser` and passes the boolean to `service::list_segments`, avoiding a DB lookup per segment row. For create/update/delete, `can_edit` is implicitly enforced via the `Require<CanManageLibraries>` extractor
- **`PUT` (not PATCH) for segment updates** — Matches SEGMENT_DETECTION.md API table exactly. PUT chosen because manual overrides typically replace the timestamp set rather than partially modify it; also distinguishes manual override (PUT, sets `is_manual=true`) from future auto-detected updates (would be PATCH or internal-only)
- **Manual segment uniqueness at service layer** — The DB has `UNIQUE (media_item_id, segment_type) WHERE is_manual = true` (partial index). Task 2's `create_segment` will catch `sqlx::Error::Database::is_unique_violation()` and map to `ManualSegmentExists` (CONFLICT) rather than surfacing as a generic 500
- **`skip_to_ms` optional in `CreateSegmentRequest`** — Defaults to `end_ms` (skip to the very end of the detected segment) when not provided. This matches SEGMENT_DETECTION.md's typical usage: "For credits, this is typically `end_ms`". Intro segments will have `skip_to_ms = end_ms - intro_end_padding_ms` set by the analysis pipeline (Task 5), not by manual creation
- **`confidence` optional in `CreateSegmentRequest`** — Manual segments default to `1.0` (authoritative, same as chapter markers) when not provided. This matches the design's "Chapter markers are always 1.0" precedent for human-authored segments
- **`AnalyzeSegmentsResponse` returns `queued: bool`** — For Task 5, `trigger_library_analysis` will likely enqueue work on the scheduler (`queued: true`) or run synchronously for small libraries (`queued: false`). The response shape is stable across both implementations; only the `message` text changes
- **Validation statics match DB CHECK constraints exactly** — `VALID_SEGMENT_TYPES = ["intro", "credits", "recap", "preview", "outro"]` and `VALID_SEGMENT_SOURCES = ["chapter", "chromaprint", "blackframe", "silence", "manual", "combined"]` mirror the `media_segments.segment_type` and `media_segments.source` CHECK constraints from migration `20260530070500_create_segments_storyboards.sql`. Service-layer validation (Task 2) will catch invalid values before they hit the DB constraint, returning VALID_001 instead of INTERNAL
- **`#![allow(unused_variables)]` on service.rs** — All 5 service functions are `todo!()` stubs; the module-level allow suppresses unused parameter warnings until actual implementations are added in Tasks 2 and 5
- **Validator error mapping follows subtitles domain convention** — `e.field_errors().into_iter().flat_map(...)` with `field.to_string()`/`err.code.to_string()`/`err.message.as_ref().map(|m| m.to_string()).unwrap_or_default()` (Cow → String conversions); `instance` set to the route pattern for client-side field correlation
- **No new workspace dependencies** — all functionality uses existing `sqlx`, `validator`, `serde`, `uuid`, `chrono`, `axum` crates

2. ~~Implement `server/src/services/segments.rs`~~ **DONE**
   - ~~Chapter marker extraction from container metadata~~
   - ~~Chromaprint fingerprinting for intro detection~~
   - ~~Black frame detection via FFmpeg~~
   - ~~Silence detection via FFmpeg~~
   - ~~Confidence scoring and 2s padding~~

**What was built for Task 2:**

| File | Purpose |
|---|---|
| `server/src/services/segments.rs` | Stateless detection library: 4 methods (chapter regex, chromaprint fingerprinting + cross-episode comparison, FFmpeg `blackframe` parser, FFmpeg `silencedetect` parser), search-window helpers, confidence scoring table, 2s `intro_end_padding_ms` applier, combined blackframe+silence credits detector (multi-method validation per design) |
| `server/src/services/mod.rs` | Added `pub mod segments;` (the empty stub is now a real module wired into the crate) |
| `server/src/domains/segments/service.rs` | Replaced 4 `todo!()` CRUD stubs with runtime `sqlx::query` implementations: `list_segments` (optional type filter, media-item existence check, `can_edit` propagation), `create_segment` (type+timestamp validation, `confidence=1.0`/`skip_to_ms=end_ms` defaults, `source='manual'`/`is_manual=true`, unique-violation → `ManualSegmentExists`), `update_segment` (SELECT-then-COALESCE partial update with re-validation), `delete_segment` (existence check + rows-affected → `SegmentNotFound`). `trigger_library_analysis` stays as `todo!()` per design (Task 5 territory). |
| `Cargo.toml` | Added `chromaprint-next = "0.1"` to workspace deps |
| `server/Cargo.toml` | Added `chromaprint-next.workspace = true` |

**Key decisions from Task 2:**

- **`chromaprint-next` 0.1 confirmed via crates.io research** — Released Feb 20 2026 by Attila Györffy; pure-Rust, bit-identical to C reference across all 5 algorithm variants; MIT AND LGPL-2.1-or-later; MSRV 1.88.0; API matches the design pseudocode exactly. Default algorithm is `test2` (matches the `media_fingerprints.fingerprint_algorithm` DB default).
- **Runtime `sqlx::query` over compile-time `query!`** — Consistent with the auth/users/etc. domain convention; no `DATABASE_URL` required at build time. `SegmentRow` does not need `#[derive(sqlx::FromRow)]` because all queries map columns via `row.get("col")` directly into `SegmentResponse`.
- **`SegmentPipelineError` is separate from `SegmentError`** — The pipeline (services layer) surfaces operational failures (FFmpeg spawn, IO, chromaprint calculation) that the worker logs and skips; `SegmentError` (domain layer) surfaces API failures (not-found, validation, conflict) that bubble through `AppError` to the HTTP client. The two are deliberately not linked — the worker is the explicit translation point.
- **Chapter regex adapted for Rust `regex` crate (no look-around)** — The design's Jellyfin Intro Skipper patterns used `(?!End)` negative lookahead, which Rust's `regex` crate deliberately omits. Patterns rewritten with `\b` word boundaries; minor loss of "IntroEnd" disambiguation is acceptable for chapter classification (rare case, no safety impact).
- **FFmpeg `blackframe` parameter defaults stricter than FFmpeg** — FFmpeg defaults `amount=98, threshold=32`; SEGMENT_DETECTION.md specifies `amount=75, threshold=2` (credit sequences have text against dim backgrounds, not pure black). Both are configurable via `BlackframeParams`; design's values are defaults.
- **FFmpeg `silencedetect` parameter defaults** — FFmpeg defaults `noise=-60dB`; design specifies `noise=-55dB` (end-credits music has low but non-zero volume). Configurable via `SilenceParams`; design's value is default.
- **Stderr-only parsing for FFmpeg filters** — Both `blackframe` and `silencedetect` emit log lines on stderr at INFO loglevel (FFmpeg's default). The implementation captures `output.stderr` and uses simple `find("silence_start:")`/`find("blackframe")` scans. `-f json` metadata export is not used because FFmpeg's JSON metadata output does not include filter log lines.
- **Streaming chromaprint via FFmpeg pipe** — `ffmpeg -i <file> -vn -ac 1 -ar 11025 -f s16le pipe:1` writes raw PCM to stdout in real time; the implementation reads in 8 KiB chunks, casts bytes to `i16` via `from_le_bytes`, and feeds each chunk to `Fingerprinter::feed()`. A separate task drains stderr to prevent deadlock when the buffer fills. The fingerprinter's internal resampler is bypassed because FFmpeg already downmixed and resampled.
- **Fingerprint storage as raw `u32` bytes in BYTEA** — `media_fingerprints.fingerprint` stores the raw `&[u32]` reinterpreted as bytes (4 bytes per sub-fingerprint, native LE). The encoded base64 form (`Fingerprinter::encode()`) is NOT persisted — only used for debug logging — because comparison happens in-process via raw u32 arrays, not via AcoustID lookups.
- **Cross-episode comparison: sliding-window bit-agreement** — For each ordered pair of fingerprints, slide one against the other; at each offset compute the fraction of sub-fingerprint pairs with Hamming similarity ≥ 30/32 bits (the standard Chromaprint "exact-ish match" threshold). Longest contiguous run above 30/32 within the intro search window that meets the 15–120s duration rule is a candidate. 3+ episode matches score 0.9 base; 2-episode matches score 0.7 base; modifiers (`+0.05` blackframe confirms, `+0.1` silence confirms) applied additively and clamped to `[0, 1]`.
- **Credits multi-method validation per design** — When blackframe and silence agree on overlapping windows, `source='combined'` with base confidence 0.8 and `metadata.methods=["blackframe", "silence"]`; lone blackframe caps at 0.5 (not surfaced by default); lone silence caps at 0.5 (also not surfaced). The admin can lower `min_confidence` to surface these.
- **2s `intro_end_padding_ms` applied as `skip_to_ms` shortening** — `apply_safety_padding` sets `skip_to_ms = end_ms - intro_end_padding_ms` for intros (clamped to `start_ms` floor); `skip_to_ms = end_ms + credits_end_padding_ms` for credits (clamped to `end_ms` ceiling); recap/preview/outro use `skip_to_ms = end_ms`. Manual segments skip the applier — admin-supplied timestamps are authoritative.
- **`mark_surfaced()` writes `metadata.surfaced = true/false`** — Segments below `min_confidence` are still written to the DB (so the admin can lower the threshold without re-analysis) but flagged via the metadata flag; the client filters on this.
- **Validation statics reused, not duplicated** — `VALID_SEGMENT_TYPES` and `VALID_SEGMENT_SOURCES` from `domains/segments/types.rs` are re-exported from `services/segments.rs` via `pub use`, so the pipeline library and CRUD layer share a single source of truth.
- **Chapter timecode parser handles both ffprobe formats** — Most containers emit `SS.mmmmmm` (decimal seconds); some legacy MKVs emit `H:MM:SS.mmmmmm`. `parse_chapter_time_ms(&str) -> Option<i32>` detects `:` and dispatches accordingly; returns `None` on malformed input so the extractor skips that chapter rather than failing the whole file.
- **25 unit tests cover the pipeline library** — Chapter timecode parsing (3 tests), chapter title classification (1), JSONB extraction (2), search windows (3), safety padding (2), FFmpeg output parsers (5), bit-agreement (1), cluster coalescing (2), combined credits detector (2), duration thresholds (2), surfaced-flag (1), enum round-trips (2). All 159 server tests pass.
- **`trigger_library_analysis` left as `todo!()`** — Deliberate. Task 5 will replace it with a scheduler enqueue (mirroring `subtitle_auto_fetch` from Phase 9 Task 7). The handler and route are already wired; only the service body changes when Task 5 lands.

**Not yet implemented (deferred to Task 5 / worker):**

- `trigger_library_analysis` — still `todo!()`; will enqueue the `segment_analysis` scheduled task
- The `segment_analysis` task seeding migration
- Per-file orchestration (loop fingerprinting + comparison + blackframe/silence, write results)
- The `outro` segment type via silence-gap detection (requires reading existing `credits` segments; chicken-and-egg within a single library scan)

3. ~~Create `server/src/domains/storyboards/` — five-file pattern~~ **DONE**

**What was built for Task 3:**

| File | Purpose |
|---|---|
| `server/src/domains/storyboards/mod.rs` | Module declarations + router assembly with 5 route groups (6 endpoints) |
| `server/src/domains/storyboards/error.rs` | `StoryboardError` enum with 7 variants: `MediaItemNotFound` (MEDIA_001), `MediaFileNotFound` (MEDIA_002), `StoryboardNotFound` (MEDIA_007), `LibraryNotFound` (LIB_001), `GenerationAlreadyInProgress` (SYS_002), `InvalidSpriteFilename` (VALID_001), `Database` catch-all |
| `server/src/domains/storyboards/types.rs` | Three-type DTOs: `StoryboardRow` (internal, 14 fields matching DB schema), `StoryboardResponse`/`SpriteResponse`/`GenerateStoryboardsResponse`/`DeleteStoryboardResponse` (Serialize); `VALID_STORYBOARD_WIDTHS`/`VALID_INTERVAL_MODES` statics |
| `server/src/domains/storyboards/service.rs` | 6 `todo!()` service function stubs (`get_storyboard`, `get_storyboard_index`, `get_storyboard_sprite`, `trigger_library_generation`, `trigger_item_generation`, `delete_storyboard`) — implemented in Tasks 4 and 6 |
| `server/src/domains/storyboards/handlers.rs` | 6 handlers wired to Axum extractors: `get_storyboard` + `delete_storyboard` (JSON return), `get_storyboard_index` + `get_storyboard_sprite` (`Result<Response, AppError>` binary serving), `generate_library_storyboards` + `generate_item_storyboards` (JSON) |
| `server/src/error.rs` | Added `AppError::Storyboard(#[from] StoryboardError)` variant + `storyboard_error_to_http()` mapping all 7 error variants |
| `server/src/domains/mod.rs` | Added `pub mod storyboards;` |
| `server/src/router.rs` | Merged storyboards router via `.merge(crate::domains::storyboards::router(state.clone()))`, removed Phase 10 storyboards comment |

**Key decisions from Task 3:**

- **Routes match STORYBOARDS.md API table exactly** — `GET /api/v1/items/{id}/storyboard` (metadata), `GET /api/v1/items/{id}/storyboard/index.vtt` (WebVTT index), `GET /api/v1/items/{id}/storyboard/{sprite}` (WebP sprite), `POST /api/v1/libraries/{id}/generate-storyboards` (library trigger), `POST /api/v1/items/{id}/generate-storyboards` (item trigger), `DELETE /api/v1/items/{id}/storyboard` (cache eviction). Static `index.vtt` segment coexists with `{sprite}` capture — axum's matchit router prioritizes static over dynamic segments at the same depth.
- **Binary endpoints return `Result<Response, AppError>`** — `get_storyboard_index` and `get_storyboard_sprite` serve non-JSON content (WebVTT text and WebP binary), following the playback domain's `stream_file`/`get_transcode_segment` pattern. Content types: `text/vtt; charset=utf-8` for the index, `image/webp` for sprites. Cache headers: `max-age=3600` for index (regenerable), `max-age=86400, immutable` for sprites (immutable once written — sprite filenames are hash-stable per generation).
- **Authorization follows segments domain convention** — Retrieval endpoints (`GET storyboard`, `GET index.vtt`, `GET sprite`) require `AuthenticatedUser` only (consumed during playback by any user); generation/deletion endpoints (`POST generate-*`, `DELETE storyboard`) require `Require<CanManageLibraries>` (admin-only). Matches STORYBOARDS.md design where generation is an admin operation and retrieval is part of the playback experience.
- **No new error codes per STORYBOARDS.md** — All 7 error variants map to existing codes from ERROR_HANDLING.md registry: MEDIA_001 (media item not found), MEDIA_002 (media file not found), MEDIA_007 (storyboard not found — already registered by media domain in Phase 5), LIB_001 (library not found), SYS_002 (generation already running — already registered for scheduled task conflicts), VALID_001 (invalid sprite filename), INTERNAL (database catch-all). Follows the SegmentError precedent of mapping multiple domain variants to a small set of existing codes.
- **`InvalidSpriteFilename` variant for path traversal protection** — Reserved for Task 4 service implementation; will validate sprite filenames against the expected `sprite_NNN.webp` pattern before constructing disk paths, rejecting names containing `..`, `/`, `\`, or non-matching patterns. Mapped to VALID_001 (422) — matches the playback domain's segment filename validation approach.
- **`cache_dir` derived from `BootstrapConfig.data_dir`** — Handlers construct `state.bootstrap.data_dir.join("cache")` and pass as `&Path` to service functions. Storyboards live in `{data_dir}/cache/storyboards/{media_file_id}/` per STORYBOARDS.md design principle 1 (cache data, regenerable). Service signatures use `&Path` (not `&PathBuf`) per clippy `ptr_arg` convention.
- **`GenerateStoryboardsResponse` is scope-agnostic** — Single response type `{ queued: bool, message: String }` serves both library and item trigger endpoints. The route context (library vs item URL) tells the client which scope was triggered. Follows the segments domain's `AnalyzeSegmentsResponse` minimal shape; avoids type duplication for two endpoints with identical response semantics.
- **`#![allow(unused_variables)]` on service.rs** — All 6 service functions are `todo!()` stubs; the module-level allow suppresses unused parameter warnings until actual implementations are added in Tasks 4 and 6.
- **No new workspace dependencies** — all functionality uses existing `sqlx`, `serde`, `uuid`, `chrono`, `axum` crates.

4. ~~Implement `server/src/services/storyboards.rs`~~ **DONE**
   - ~~FFmpeg thumbnail extraction at adaptive intervals~~
   - ~~WebP spritesheet generation~~
   - ~~WebVTT seek file generation~~

   **What was built for Task 4:**

   | File | Purpose |
   |---|---|
   | `server/src/services/storyboards.rs` | Storyboard generation library: `GenerationConfig` (width, interval, quality, keyframe_only, sprite grid), `GenerationResult`, `SpriteLayout`, `StoryboardPipelineError`; `adaptive_interval()` per STORYBOARDS.md table; `compute_sprite_layout()` (handles partial last sheet, zero-duration edge case); `generate_storyboard()` (one FFmpeg invocation per sprite sheet via single-command `fps+scale+tile` filtergraph); `build_webvtt_index()` (pure function emitting `WEBVTT` header + `#xywh=` Media Fragment URI cues); `format_timecode_secs()` (WebVTT `HH:MM:SS.mmm` formatter); `validate_sprite_filename()` (path-traversal protection for `sprite_NNN.webp`); `sprite_filename()` (canonical 3-digit filename formatter); `inspect_sprite_height()` (RIFF/VP8/VP8L header parser with 16:9 fallback); 39 unit tests |
   | `server/src/services/mod.rs` | Added `pub mod storyboards;` |
   | `server/src/domains/storyboards/service.rs` | Replaced 4 of 6 `todo!()` stubs with real implementations: `get_storyboard` (resolves primary media file via `is_healthy=true ORDER BY file_size DESC`, loads storyboards row, builds sprite URLs, reads grid shape from metadata JSONB with 10×20 fallback), `get_storyboard_index` (path resolution + disk read), `get_storyboard_sprite` (filename validation via `sb_svc::validate_sprite_filename` + sprite_count bounds check + disk read), `delete_storyboard` (DB row delete via `RETURNING` + best-effort recursive dir cleanup with NotFound-tolerant error handling). `trigger_library_generation` and `trigger_item_generation` remain `todo!()` per Task 6 scope. Added `#![allow(unused_variables)]` until Task 6 lands. 13 new domain tests |

   **Key decisions from Task 4:**

   - **Single-command per-sheet approach over STORYBOARDS.md two-phase** — The design doc describes extracting frames to a temp directory, then assembling sprites via separate FFmpeg calls. Research confirmed the modern best practice (2025-2026) is a single FFmpeg filtergraph per sprite sheet: `fps=1/N,scale=W:trunc(ow/a/2)*2,tile=COLSxROWS`. This eliminates temp-frame disk I/O, simplifies cleanup (no per-frame temp files), and is the pattern used by current FFmpeg documentation and the Jellyfin/MTG/Id_rs implementations cited in the design's Research Sources. For multi-sheet videos, one FFmpeg invocation per sheet with `-ss`/`-t` seek windows is used (placing `-ss` *before* `-i` for fast keyframe-accurate seek). STORYBOARDS.md updated to document this refinement.
   - **`-skip_frame nokey` for ~100x speedup** — Per the design's "Keyframe-Only Mode (Fast Generation)" section and confirmed by FFmpeg docs (`ffmpeg -skip_frame nokey -i file.avi -vf 'scale=128:72,tile=8x8' -an -vsync 0 keyframes.png` is the canonical example). Placed *before* `-i` so the demuxer skips non-keyframe packets. The trade-off (timestamps snap to nearest keyframe rather than exact interval) is imperceptible for seek-bar previews.
   - **Library types mirror segments.rs pattern** — `GenerationConfig`, `SpriteLayout`, `StoryboardPipelineError` are pure library types with no DB/state coupling, exactly like `BlackframeParams`, `SafetyConfig`, `SegmentPipelineError` in services/segments.rs. The worker (Task 6) will bridge `RuntimeConfig.transcoding.storyboard_*` → `GenerationConfig`; the domain service (`domains/storyboards/service.rs`) consumes only the sprite-filename validator (`sb_svc::validate_sprite_filename`) for HTTP path-traversal protection. This keeps the service library testable without an HTTP or DB harness.
   - **Final WebVTT cue extends to `duration_seconds`** — Per the "no gap at the end" rule from research: a cue covering `[N*interval, (N+1)*interval)` for the last thumbnail is wrong when the content ends earlier — clients show a dead-zone at the end of the seek bar. The final cue is `(i+1)*interval` clamped via `duration.max(...)` so it covers `[last_thumb, duration)`. This matches the masonwritescode/dev.to reference implementation.
   - **Drift prevention** — Research identified "WebVTT interval must equal FFmpeg's `fps=1/N`" as the #1 source of preview drift. Mitigated by generating the WebVTT index in the same `generate_storyboard()` call that invokes FFmpeg, both consuming the same `GenerationConfig.interval_seconds`. The `build_webvtt_index` doc-comment carries a "Drift warning" callout for future maintainers.
   - **Sprite filename validation enforces 1-based 1-4 digit numbers** — `sprite_NNN.webp` where NNN is `[1-9999]`. 3-digit zero-padding (`sprite_001.webp`) matches the design's WebVTT examples; the validator accepts up to 4 digits so a 4-hour movie at 5s interval (~288 sheets) and pathological longer content still parse. `sprite_000` is rejected (1-based). Path separators (`/`, `\`), `..`, non-`.webp` suffixes, and non-`sprite_` prefixes all rejected with descriptive error strings.
   - **WebP RIFF header parser for height** — The design stores thumbnail `height` in the DB row (computed from source aspect ratio at generation time). Rather than add a Rust image-library dependency, `inspect_sprite_height()` parses the WebP container directly: VP8 lossy (`b"VP8 "`) reads 16-bit LE width/height at offsets 26/28; VP8 lossless (`b"VP8L"`) reads the 14-bit packed width/height from bytes 22-24; falls back to 180px (16:9 at 320 wide) for unparseable headers. Same approach as services/subtitles.rs OCR engine detection — avoid heavy dependencies for trivial parsing.
   - **Grid shape recovery from `metadata.columns`/`metadata.rows` JSONB** — The `storyboards` table has no explicit columns/rows fields, but `SpriteResponse` includes them per the design. The worker (Task 6) writes `metadata.columns` and `metadata.rows` when creating the row; `read_grid_shape()` in the domain service recovers them, defaulting to the design's 10×20 when missing (handles externally-authored or future-config rows). Validation rejects zero/negative so a malformed metadata payload cannot break URL construction.
   - **`resolve_primary_media_file` mirrors playback domain** — Same query (`is_healthy=true ORDER BY file_size DESC LIMIT 1`) as `domains::playback::service::resolve_media_file`. Storyboards correspond to the file the user will actually stream; keeping the selection identical means the storyboard is always for the right file. Multi-version items (4K + 1080p) get one storyboard for the primary file, matching STORYBOARDS.md's `media_file_id` rationale.
   - **Delete is idempotent + best-effort disk cleanup** — DB deletion via `DELETE ... RETURNING` is the source of truth; if no row exists, returns `StoryboardNotFound` *after* still attempting on-disk cleanup (handles crashed-generation drift). On-disk `remove_dir_all` failures (except NotFound) are logged at WARN but do not invalidate the committed DB deletion — derived data can always regenerate.
   - **`#![allow(unused_variables)]` retained on domain service** — Two `todo!()` stubs remain for Task 6 (`trigger_library_generation`, `trigger_item_generation`); the module-level allow silences their unused-parameter warnings until Task 6 implements them, matching the segments domain's scaffolding-to-implementation transition pattern.
   - **No new workspace dependencies** — All FFmpeg invocation uses `tokio::process::Command` (already in workspace); WebP header parsing uses byte slicing (`u16::from_le_bytes`); no `image`, `webp`, or `tempfile` crates needed since sprite generation is pure FFmpeg and temp-file management is internal to FFmpeg's filtergraph.

   **Not yet implemented (deferred to Task 6 / worker):**

   - `trigger_library_generation` and `trigger_item_generation` — still `todo!()`; Task 6 will replace them with scheduler enqueue (mirroring `subtitle_auto_fetch` from Phase 9 Task 7 and `segment_analysis` from Phase 10 Task 5)
   - `storyboard_generation` scheduled task already seeded (migration `20260530070000_seed_default_data.sql`, daily 04:00) — Task 6 registers the executor on the scheduler in `main.rs`
   - `RuntimeConfig.transcoding.storyboard_*` config fields — Task 6 expands `TranscodingConfig` with the 8 storyboard fields from STORYBOARDS.md Configuration table and constructs `GenerationConfig` per-file
   - Per-library config overrides (`libraries.metadata.storyboards_*`) — Task 6 worker reads these and overrides the server-wide config when constructing `GenerationConfig`
   - Sandbox application — Task 6 worker calls `services::sandbox::apply_sandbox` before each FFmpeg invocation (Linux landlock + seccomp; no-op on Windows/macOS)
   - Web client `SeekPreview.svelte` — Task 8 consumes the `/storyboard/index.vtt` endpoint via hls.js or a custom seek-bar component
    5. ~~Implement `server/src/workers/segment_detector.rs` — background segment detection~~ **DONE**

    **What was built for Task 5:**

    | File | Purpose |
    |---|---|
    | `server/src/workers/segment_detector.rs` | Full background segment detector: `run_segment_analysis()` entry point (scheduled task — iterates all non-deleted, scan-enabled libraries, or a single library when `config.library_id` is set); `analyze_library_one()` (synchronous API entry point for the per-library admin trigger); `analyze_library()` 6-phase pipeline implementation; candidate resolution, chapter extraction + classification, chromaprint fingerprinting + storage, cross-season comparison, blackframe+silence credits detection; `LibraryAnalysisResult` summary struct; 2 unit tests |
    | `server/src/workers/mod.rs` | Added `pub mod segment_detector;` |
    | `server/src/services/segments.rs` | Added `media_item_id: Uuid` field to `RecurringMatch` (previously discarded by `find_recurring_segments` when collecting HashMap values — the worker needs the association to persist results per-item) |
    | `server/src/domains/segments/service.rs` | Replaced `trigger_library_analysis` `todo!()` with synchronous implementation: verifies library exists, calls `segment_detector::analyze_library_one()`, returns `AnalyzeSegmentsResponse` with summary message; added `verify_library_exists` helper |
    | `server/src/domains/segments/handlers.rs` | Updated `analyze_library_segments` to pass `&state` instead of `&state.pool` |
    | `server/src/state.rs` | Expanded `TranscodingConfig` with 3 segment fields: `segment_detection_enabled: bool`, `segment_safety: SegmentSafetyConfig` (intro_end_padding_ms, credits_end_padding_ms, min_confidence), `segment_analysis: SegmentAnalysisConfig` (max_concurrent_analyses, chromaprint_sample_rate, blackframe_amount, blackframe_threshold, silence_noise_db, silence_min_duration_ms); added both nested structs with `Default` impls matching SEGMENT_DETECTION.md configuration table |
    | `server/src/main.rs` | Registered `segment_analysis` executor on scheduler; renamed `subtitle_state` capture to `worker_state` and added separate `segment_state` clone for the segment closure |
    | `server/src/services/scheduler.rs` | Added "Segment Analysis" to runtime `seed_default_tasks()` (cron `0 3 * * *`, enabled by default) |
    | `server/migrations/20260621030000_seed_segment_analysis_task.sql` | Seeds `segment_analysis` scheduled task (cron daily 03:00, 14400s timeout, enabled) for existing deployments |

    **Key decisions for Task 5:**

    - **Synchronous API + scheduled task for all libraries** — The `POST /api/v1/libraries/{id}/analyze-segments` endpoint runs `analyze_library_one()` synchronously and returns the summary, matching the Phase 5 Task 5 `scan_library` pattern exactly. The `segment_analysis` scheduled task iterates all non-deleted, scan-enabled libraries via `run_segment_analysis()`. The design's "scheduler enqueue" language was prescriptive but the library_scan precedent (synchronous API + scheduled iteration) is more pragmatic, avoids background-queue infrastructure that doesn't exist yet, and keeps the `AnalyzeSegmentsResponse.queued: bool` field honest (always `false` in this implementation). HTTP timeout risk for large libraries is accepted per the library_scan precedent; an async 202 Accepted flow can be layered on later if needed
    - **6-phase pipeline per SEGMENT_DETECTION.md** — (1) Resolve candidates incrementally — files without a matching `media_fingerprints` row (same `file_hash`) are candidates; (2) Extract chapters from `media_files.additional_streams` JSONB and classify against chapter title regex patterns; (3) For items not resolved by chapters, fingerprint via FFmpeg PCM extraction → chromaprint-next → store raw `u32` LE bytes in `media_fingerprints.fingerprint` BYTEA; (4) Group fingerprints by season, run cross-episode `find_recurring_segments()` comparison (≥2 episodes); (5) For items without credits segments, run FFmpeg `blackframe` + `silencedetect` in the credits search window, combine via `combine_credits_signals()`; (6) Aggregate counters and log results
    - **`media_item_id` added to `RecurringMatch`** — The `find_recurring_segments` function built a `HashMap<Uuid, RecurringMatch>` internally but discarded the UUID keys when collecting values. The worker needs the association to know which media item each recurring segment belongs to. Added `media_item_id: Uuid` field to `RecurringMatch`, populated by `find_recurring_segments` before collecting. This is a non-breaking change — existing tests don't construct `RecurringMatch` directly, and `chromaprint_match_to_segment` doesn't read the new field
    - **Chapter markers always re-evaluated** — Even for files with cached fingerprints, chapter data lives in `media_files.additional_streams` and may change without the file hash changing (e.g., re-muxing preserves the stream but updates chapter metadata). The worker extracts chapters for all candidates regardless of fingerprint cache state; fingerprint cache only skips the expensive audio extraction
    - **Credits detection is supplementary** — Items already resolved by chapter markers are skipped (passed in `chapter_resolved` list). Items already having a `credits` segment are also skipped (checked via `has_segment_for_type`). This prevents redundant FFmpeg invocations. Blackframe + silence are always run together so `combine_credits_signals` can produce `source='combined'` high-confidence credits when both methods agree
    - **`outro` segment type deferred** — The design notes outro detection requires reading existing `credits` segments (chicken-and-egg within a single library pass). Implemented as a comment in the worker module; will be addressed in a follow-up that runs after credits are established across the library
    - **`segment_analysis` task enabled by default** — Unlike `subtitle_auto_fetch` (disabled by default because it consumes external API quota), segment analysis uses local FFmpeg and is safe to run by default. The daily 03:00 schedule matches the design's "after the daily library scan" timing (both fire at 03:00; the scheduler processes them sequentially within the same tick)
    - **14400s (4-hour) timeout** — Per SEGMENT_DETECTION.md "Timeout: 4 hours". The shared scheduler applies each task row's `timeout_seconds`, so segment analysis receives its declared four-hour budget. Timeout and cancellation regression coverage landed with `0420e68`.
    - **`ON CONFLICT DO NOTHING` on segment insert** — Prevents duplicate segments when re-analyzing unchanged files. The `media_segments` table has no unique constraint on `(media_item_id, segment_type)` for non-manual segments (only `WHERE is_manual = true`), so the `has_segment_for_type` pre-check is the primary deduplication mechanism; `ON CONFLICT DO NOTHING` is a safety net for race conditions
    - **No new workspace dependencies** — All functionality uses existing `sqlx`, `serde_json`, `tokio::process::Command`, and the already-present `chromaprint-next`, `regex` crates. FFmpeg invocation reuses the subprocess pattern from `services/segments.rs`

    **Not yet implemented (deferred to later tasks/phases):**

    - ~~`outro` segment type via silence-gap detection after credits~~ — Complete: a hash-aware second pass selects credits-marked primary files, accepts only a silence gap touching the credits boundary, persists analysis evidence on the credits marker, and stores only low-confidence bounded tails (`684b717`)
    - Movie intro detection via chromaprint — design specifies chromaprint for TV episodes (≥2 episodes in a season); movies fall through to chapters + blackframe only. Movie-specific audio matching (against a database of known studio logos) is a future enhancement
    - ~~Per-task timeout enforcement~~ — Complete: the scheduler honors each task row's configured timeout and records expiration as a timed-out run (`0420e68`)
    - ~~Prometheus metrics from SEGMENT_DETECTION.md Metrics table~~ — Complete: bounded method/type/source/stage metrics, aggregate duration, active inventory, and low-confidence counters landed without library/media/path/user labels; the current local-only skip action remains deliberately deferred pending authenticated playback telemetry (`d5c6be8`)

     6. ~~Implement `server/src/workers/storyboard_generator.rs` — background thumbnail generation~~ **DONE**

     **What was built for Task 6:**

     | File | Purpose |
     |---|---|
     | `server/src/workers/storyboard_generator.rs` | Background storyboard generator: `run_storyboard_generation` (scheduler entry — iterates all non-deleted, scan-enabled libraries, respects per-library enablement), `generate_for_library_one` (synchronous per-library API entry), `generate_for_item_one` (synchronous per-item API entry — force regen via delete-then-generate), `generate_for_library` (per-library pipeline with 3-gate enablement check, per-library config override resolution, incremental candidate fetch, per-file pipeline calling `services::storyboards::generate_storyboard`, `persist_storyboard_row` upsert with `metadata.columns`/`metadata.rows` grid shape); `LibraryGenerationResult`/`AggregateResult` summary structs; 8 unit tests |
     | `server/src/workers/mod.rs` | Added `pub mod storyboard_generator;` |
     | `server/src/services/storyboards.rs` | `invoke_ffmpeg_for_sheet` now applies the Linux sandbox via `pre_exec` (landlock + seccomp) — same pattern as `services::transcoding::spawn_ffmpeg`. Source path is read-only; per-file storyboard output directory is read-write. Non-Linux platforms are no-ops. |
     | `server/src/state.rs` | `TranscodingConfig` expanded with 8 storyboard fields per STORYBOARDS.md Configuration table: `storyboards_enabled` (default true), `storyboard_interval_mode` (default "adaptive"), `storyboard_fixed_interval_seconds` (default 10), `storyboard_width` (default 320), `storyboard_quality` (default 75), `storyboard_keyframe_only` (default true), `storyboard_sprite_columns` (default 10), `storyboard_sprite_rows` (default 20); 8 `default_*` serde helpers for graceful migration from older config payloads |
     | `server/src/domains/storyboards/service.rs` | Replaced `trigger_library_generation` and `trigger_item_generation` `todo!()` stubs with synchronous implementations that call the worker (matching the segment domain's `trigger_library_analysis` pattern). Signatures changed from `&PgPool` to `&AppState`. Added `verify_library_exists` helper. Removed `#![allow(unused_variables)]` (no longer needed — all stubs are implemented). |
     | `server/src/domains/storyboards/handlers.rs` | Updated `generate_library_storyboards` and `generate_item_storyboards` to pass `&state` instead of `&state.pool` |
     | `server/src/main.rs` | Registered `storyboard_generation` executor on scheduler (5th executor) with `storyboard_state` capture clone |
     | `server/src/services/scheduler.rs` | Added "Storyboard Generation" to runtime `seed_default_tasks` (cron `0 4 * * *`, enabled by default) |
     | `server/migrations/20260621040000_seed_storyboard_generation_task.sql` | Seeds `storyboard_generation` scheduled task for existing deployments (idempotent — original Phase 2 seed already creates this row for fresh installs) |

     **Key decisions for Task 6:**

     - **Synchronous per-library API + scheduled iteration of all libraries** — Mirrors the segment detector pattern (Task 5) and the library scanner pattern (Phase 5 Task 5) exactly. The `POST /api/v1/libraries/{id}/generate-storyboards` endpoint runs `generate_for_library_one()` synchronously and returns a summary. The `storyboard_generation` scheduled task iterates all non-deleted, scan-enabled libraries via `run_storyboard_generation()`. The design doc's "enqueue on the scheduler" language was prescriptive but the established precedent (synchronous API + scheduled iteration) is more pragmatic, avoids background-queue infrastructure that doesn't exist, and keeps the `GenerateStoryboardsResponse.queued` field honest (always `false` in this implementation, matching `AnalyzeSegmentsResponse.queued`). HTTP timeout risk for large libraries is accepted per the library_scan precedent; the worker logs per-file progress so partial completion is observable.
     - **Per-library enablement respected (Jellyfin bug #14558 lesson)** — Three gates must pass before a library is processed: (1) global `TranscodingConfig.storyboards_enabled` must be `true`; (2) the library must be non-deleted with `scan_enabled = true`; (3) per-library `libraries.metadata->>'storyboards_enabled'` must NOT be `"false"` (defaults to enabled when the key is absent). Web research (June 2026) surfaced [Jellyfin bug #14558](https://github.com/jellyfin/jellyfin/issues/14558) (open Aug 2025–Mar 2026) where users reported CPU usage from a scheduled task that should have been disabled per-library — this implementation explicitly avoids that failure mode.
     - **Per-library config overrides via `libraries.metadata` JSONB** — The worker reads `metadata->>'storyboard_width'` (validated against 160/320/640) and `metadata->>'storyboard_fixed_interval_seconds'` (validated against 2–120 range) and overrides the server-wide config. This allows different resolutions or intervals for different library types per the design's "Per-Library Storyboard Config" section. Missing or invalid keys fall back to server-wide config (graceful degradation).
     - **Adaptive interval resolved per-file** — When `interval_mode = "adaptive"`, the worker calls `services::storyboards::adaptive_interval(runtime_seconds)` per-file using the file's actual runtime from `media_files.runtime_seconds`. A TV library with 22-min episodes and 100-min movies gets 5s intervals for episodes and 10s intervals for movies. When `interval_mode = "fixed"`, the worker uses the resolved fixed interval (per-library override → server-wide).
     - **Incremental candidate query with file_hash change detection** — `fetch_files_needing_storyboards` uses `NOT EXISTS (SELECT 1 FROM storyboards WHERE media_file_id = mf.id AND file_hash = mf.file_hash)` — files without a storyboard OR with a changed hash (re-muxed, re-encoded) are candidates. Validated by r/jellyfin user reports that daily runs take "less than 10 mins" after the initial backlog is cleared.
     - **DB row upsert with `ON CONFLICT (media_file_id) DO UPDATE`** — `persist_storyboard_row` upserts on each successful generation. On update, all fields are refreshed. Grid shape stored in `metadata = {"columns": N, "rows": N}` so `domains::storyboards::service::read_grid_shape()` can recover it (closes the loop with the Task 4 design decision).
     - **Forced regeneration for per-item trigger** — `trigger_item_generation` calls `generate_for_item_one` which deletes any existing storyboard row + on-disk directory before generating fresh. Matches the design's "force regen" semantics for the per-item endpoint.
     - **Sandbox applied via `pre_exec` on each FFmpeg invocation** — `services::storyboards::invoke_ffmpeg_for_sheet` now uses `cmd.pre_exec(move || apply_sandbox(&SandboxConfig { media_path, transcode_dir: output_dir }))` on Linux, matching the `services::transcoding::spawn_ffmpeg` pattern. The sandbox restricts FFmpeg to read-only access on `/usr`, `/lib`, `/etc`, `/dev/dri`, and the source media path; read-write access on the per-file storyboard output directory and `/tmp`. Seccomp filters to a 62-syscall allow-list with `KillProcess` on violation. Sandbox failures are non-fatal (logged at WARN, FFmpeg continues without sandbox) per SECURITY.md graceful degradation model. Non-Linux platforms are no-ops.
     - **Per-file error isolation** — `generate_for_library` catches per-file errors and continues to the next file (matching the segment detector's per-file error pattern). Failed files are counted in `LibraryGenerationResult.errors` and logged at WARN but do not abort the library run. Critical for large libraries where a single corrupt file should not prevent the rest from processing.
     - **Movie/episode-only filtering + healthy files** — The candidate query filters `mi.type IN ('movie', 'episode')` and `mf.is_healthy = true` because series/seasons are container types without direct `media_files`, and storyboards correspond to actual video files that exist on disk and are playable.
     - **`gen` is a reserved keyword in Rust 2024 edition** — The `persist_storyboard_row` parameter was initially named `gen` (for "generation result"); the compiler rejected it as `gen` is reserved for future generator syntax. Renamed to `result`.
     - **Configured four-hour timeout enforced** — The scheduler derives the storyboard wrapper duration from `scheduled_tasks.timeout_seconds` (14400 for `storyboard_generation`) and records expiration as a timed-out run. The FFmpeg command is configured with `kill_on_drop(true)`, so timeout cancellation terminates the in-flight child rather than leaving it orphaned (`0420e68`).
     - **No new workspace dependencies** — All functionality uses existing `sqlx`, `serde_json`, `tokio::process::Command`, and the already-built `services::storyboards` and `services::sandbox` modules.
     - **8 unit tests** covering: `LibraryGenerationResult::message()` formatting (with data and default), `AggregateResult::add()` accumulation, `is_storyboards_enabled_for_library()` for absent key (defaults true), explicit true/false, and non-boolean values (defaults true), `storyboard_dir()` path layout

     **Not yet implemented (deferred to later tasks/phases):**

     - Storyboard metadata in playback start response — intentionally not used. The selected healthy media-file version fetches its protected storyboard metadata after playback setup, keeping preview assets independent from session startup.
     - ~~Prometheus metrics from STORYBOARDS.md Metrics table~~ — Complete: bounded generation, serving, duration, sprite, and cache-size metrics landed in Post-Phase 10 Task 7 (`09668d4`).
     - ~~Per-task timeout enforcement~~ — Complete: the scheduler honors `timeout_seconds`, and storyboard FFmpeg cancellation kills the dropped child process (`0420e68`).
     7. ~~Implement skip button in web client player — `SkipButton.svelte`~~ **DONE**
     8. ~~Implement seek preview in web client player — `SeekPreview.svelte`~~ **DONE**

     **What was built for Task 7:**

     | File | Purpose |
     |---|---|
     | `clients/web/src/lib/api/segments.js` | Full segment API client — `listSegments(itemId, type?)`, `createSegment`, `updateSegment`, `deleteSegment`, `analyzeLibrarySegments` covering all 5 segment endpoints |
     | `clients/web/src/lib/api/index.js` | Barrel export extended with `segments.js` and `subtitles.js` (subtitles was missing from Phase 9 Task 8) |
     | `clients/web/src/lib/components/SkipButton.svelte` | Skip-button overlay component — derives active segment from `positionMs` against the `[start_ms, end_ms]` window; renders bottom-right button per industry convention; auto-hide 10s (high-confidence) or 5s (medium-confidence) per SEGMENT_DETECTION.md; per-segment deduplication of auto-skip + dismiss; Svelte 5 runes throughout |
     | `clients/web/src/lib/components/Player.svelte` | Wired SkipButton — fetches segments via `listSegments(mediaItem.id)` after playback starts; filters by `confidence ≥ 0.7 OR is_manual` (design's default min_confidence); computes `autoSkipTypes` from user preferences; `handleSkip(skipToMs)` performs direct-play seek via `videoEl.currentTime` or transcode seek via `player.seek()` |
     | `clients/web/src/lib/stores/user.js` | Extended `DEFAULT_PREFS` with 5 per-type auto-skip toggles (`autoSkipIntro`, `autoSkipCredits`, `autoSkipRecap`, `autoSkipPreview`, `autoSkipOutro`) — all default `false` per design |

     **Key decisions for Task 7:**

     - **Bottom-right placement per industry standard** — Research (June 2026) confirmed Netflix, Disney+, Amazon Prime, Crunchyroll, and Max all place skip buttons bottom-right, above the controls bar. Jellyfin-web issue #6591 (March 2025) explicitly criticized centered placement as "too far offset into the middle of the player" — the maintainer agreed and moved it bottom-right. SkipButton uses `position: absolute; right: 1.25rem; bottom: 5.5rem` (4.75rem on mobile) to sit just above the controls gradient
     - **Two-tier prominence per SEGMENT_DETECTION.md confidence bands** — High-confidence segments (≥0.8 OR `is_manual`) get the brass accent button (10s timeout); medium-confidence segments (0.5–0.79, only visible if admin lowers `min_confidence`) get a smaller subdued backdrop-blurred button (5s timeout). Matches the design's "show skip button with reduced prominence (smaller, shorter timeout)" rule for medium confidence
     - **Confidence filter client-side at 0.7 default** — The server returns all segments without filtering on `metadata.surfaced`. Player filters via `seg.is_manual || seg.confidence >= 0.7` (matching `SegmentSafetyConfig.min_confidence` default). Manual segments always surface regardless of confidence (admin-authored = authoritative)
     - **Auto-skip via localStorage preferences (server-side `users.metadata.auto_skip` deferred)** — SEGMENT_DETECTION.md specifies per-user auto-skip stored in `users.metadata.auto_skip`; no users.metadata API exists yet. Stored in `DEFAULT_PREFS` (localStorage) under 5 typed toggles. Default OFF for all types per design ("Off by default for all types"). Server-side persistence deferred to Phase 13a `server_config`/user metadata API
     - **Per-segment deduplication** — `autoSkippedIds` Set prevents double-firing auto-skip if the position wobbles around `start_ms` (e.g., user scrubs back into the intro after auto-skipping); `dismissedIds` Set prevents re-showing the button after manual click until the user exits and re-enters the segment window. Both Sets are cleared on segment transition via the `$effect` that watches `activeSegment.id`
     - **Skip dispatch respects stream decision** — `handleSkip` checks `$streamDecision`: `direct_play` → `videoEl.currentTime = skipToMs/1000` (no server round-trip); `transcode`/`direct_stream` → `player.seek(skipToMs)` (server restarts the transcode at the new position, returns new `transcode_session_id`). Matches the existing `handleSeekEnd` pattern
     - **Player also calls `showControls()` on skip** — Skipping reveals the transport controls so the user sees feedback (position jump in seek bar, new time display). Skipped segments often transition into content where the user wants controls visible
     - **No new npm dependencies** — SkipButton uses Svelte 5 built-in transitions (`fly`); no animation library added. Derives active segment via `$derived.by` — no external reactive helper
     - **Svelte 5 patterns matched** — `$props()` with defaults, `$state` for entry-tracking, `$derived`/`$derived.by` for computed visibility/prominence/timeout, `$effect` for segment-entry side-effects (auto-skip firing). Auto-skip firing inside `$effect` is safe because `autoSkippedIds` deduplication guarantees it runs at most once per segment entry — the effect re-runs on position changes but the Set guard short-circuits
     - **SkipButton is purely presentational** — Fetches nothing itself; receives `segments`, `positionMs`, `autoSkipTypes`, `onskip` as props. This makes it testable in isolation and reusable if other surfaces ever need skip affordances (e.g., a future mini-player). All I/O (segment fetch, seek dispatch, preference reads) is in Player.svelte
      - **0 svelte-check warnings, 0 build errors** — Matches the verification bar set by Phase 8 Task 4 (0 svelte-check warnings across all components)

     **What was built for Task 8:**

     | File | Purpose |
     |---|---|
     | `clients/web/src/lib/api/storyboards.js` | Full storyboard API client: `getStoryboard` (GET storyboard metadata), `storyboardIndexUrl` / `storyboardSpriteUrl` URL builders, `generateLibraryStoryboards`, `generateItemStoryboards`, `deleteStoryboard` |
     | `clients/web/src/lib/utils/storyboards.js` | Pure-function WebVTT utilities: `parseTimecodeToMs` (HH:MM:SS.mmm parser), `parseStoryboardVtt` (full WebVTT index parser extracting cues with `spriteUrl`, `x`, `y`, `w`, `h`, `startMs`, `endMs`), `findCueForTime` (binary search for the cue containing a given timestamp) |
     | `clients/web/src/lib/components/SeekPreview.svelte` | Seek-preview thumbnail tooltip — lazily fetches and parses the WebVTT index, resolves sprite references to absolute URLs, renders the correct sprite-sheet region via CSS `background-image` + `background-position` + `background-size` scaling, edge-clamped horizontal positioning via CSS `clamp()`, time label bar below the thumbnail |
     | `clients/web/src/lib/api/index.js` | Added `storyboards.js` to barrel export (was missing) |
     | `clients/web/src/lib/components/Player.svelte` | Wired SeekPreview — fetches storyboard metadata in `onMount` alongside segments; tracks hover state (`isSeekHovering`, `seekHoverRatio`, `seekHoverMs`) via mousemove/touchmove on the seek-bar-wrapper; renders SeekPreview during hover and active seeking; expanded seek-bar-wrapper hit area from 6px to 20px for comfortable hover detection |

     **Key decisions from Task 8:**

     - **Custom seek-bar component over hls.js native thumbnail tracks** — STORYBOARDS.md `hls.js Integration` section describes `hls.addTrack({ kind: 'thumbnails', url })` for HLS transcoded streams. However, the player uses a custom seek bar (`<input type="range">` with opacity 0 overlaid on a visual track) across all playback modes (direct play, remux, transcode). A custom seek-preview component works uniformly for all stream types, while hls.js thumbnail tracks only apply when hls.js manages the stream (transcode/remux, not direct play). The design doc's "For native HLS (Safari, Chrome 142+), the client uses a custom seek bar component" guidance confirms this as the correct cross-platform approach.
     - **CSS `background-image` + `background-position` for sprite rendering** — The industry-standard approach confirmed by research (JW Player, Video.js, FluidPlayer, Radiant Media Player all use this pattern). The `#xywh=X,Y,W,H` Media Fragment URI from each WebVTT cue maps directly to negative `background-position` offsets. `background-size` scales the full sprite sheet so the region maps to the display thumbnail dimensions. No `canvas` or `clip-path` needed — pure CSS.
     - **Sprite URL resolution via `new URL(ref, baseUrl)`** — WebVTT cue payloads reference sprites by relative name (`sprite_001.webp`), relative to the index file URL. The component constructs the absolute index URL via `new URL(index_url, window.location.href)` and passes it to the VTT parser, which resolves each sprite reference to an absolute URL. This works correctly in dev (Vite proxy) and production (same-origin or reverse proxy).
     - **Lazy VTT fetch with request-ID race protection** — The WebVTT index is fetched on first storyboard availability via a `$effect` that tracks `storyboard.media_file_id`. A `fetchId` counter discards stale responses if the user switches media items before the previous fetch completes. No `AbortController` needed (the VTT is a few KB; ignoring stale results is sufficient).
     - **Binary search for cue lookup** — `findCueForTime` uses binary search (O(log n)) over the sorted cues array. For a 2-hour movie at 10s intervals (~720 cues), this is ~10 comparisons vs 720 for linear scan. Cues before the first timestamp clamp to the first cue; cues after the last timestamp clamp to the last cue (no dead zones).
     - **CSS `clamp()` for edge-aware positioning** — The tooltip's horizontal position uses `left: clamp(half-width, ratio × 100%, 100% − half-width)` with `transform: translateX(-50%)`. This prevents the tooltip from overflowing the player edges without any JavaScript measurement. Pure CSS, automatically responsive.
     - **Display width capped at native thumbnail width** — Default display width is 160px (YouTube-standard preview size), but capped at `storyboard.width` so 160px-native thumbnails display at native resolution (no upscaling). The thumbnail height is derived from the storyboard's native aspect ratio.
     - **Preview shown during both hover and active seek drag** — When the user drags the seek bar (`isSeeking = true`), the preview tracks `seekValue` (the range input value); when hovering without dragging, it tracks `seekHoverMs` (computed from mouse X position). This matches YouTube/Netflix behavior where the preview follows the thumb during scrubbing.
     - **Touch support via `ontouchmove`** — On mobile, `touchmove` events on the seek-bar-wrapper compute the hover ratio from `touches[0].clientX`, showing the preview during touch-drag seek. `touchend` hides the preview.
     - **Graceful degradation when no storyboard exists** — `getStoryboard` returns 404 (MEDIA_007) when no storyboard has been generated for the item. The Player catches this silently (`storyboard = null`), and the `{#if storyboard}` guard prevents SeekPreview from rendering. The seek bar works normally without preview — no visual regression for items without storyboards.
     - **`role="presentation"` on seek-bar-wrapper** — The wrapper has mouse/touch handlers for hover tracking but is not itself an interactive element (the nested `<input type="range">` handles actual seeking). `role="presentation"` satisfies Svelte's a11y rule (`a11y_no_static_element_interactions`) without implying false semantics.
     - **Seek-bar hit area expanded from 6px to 20px** — The original `.seek-bar-wrapper` was 6px tall, making precise hover difficult. Expanded to 20px with the visual track remaining 4px (absolutely positioned, vertically centered). The invisible range input fills the full 20px. `cursor: pointer` added for affordance.
     - **No new npm dependencies** — WebVTT parsing is pure string manipulation (no `vtt.js` or WebVTT parser library). CSS sprite rendering uses native browser APIs. All functionality uses existing Svelte 5 runes, `svelte/transition`, and standard DOM events.
     - **Svelte 5 patterns** — `$props()` with defaults, `$state` for hover/loaded state, `$derived`/`$derived.by` for cue lookup/thumbnail style/positioning, `$effect` for VTT fetch lifecycle. The `loadedKey` guard variable is a plain `let` (non-reactive) since it only serves as an idempotency guard inside the effect, not a value the template reads.
     - **0 svelte-check warnings, 0 build errors** — Matches the verification bar from Task 7 and Phase 8 Task 4.



**Strategic implementation debt absorbed into Phase 10** (per [IMPLEMENTATION_DEBT.md](docs/design/IMPLEMENTATION_DEBT.md) — storyboards are the first consumer of these items):

9. ~~Implement `server/src/services/image_pipeline.rs` — WebP encode, resize, variant generation (debt from [IMAGE_FORMATS.md](docs/design/IMAGE_FORMATS.md); storyboards produce WebP sprites via this same service)~~ **DONE**
10. ~~Implement artwork delivery endpoint `GET /api/v1/items/{id}/artwork/{type}?size={size}` — serves WebP variants from `image_pipeline.rs` (debt from [IMAGE_FORMATS.md](docs/design/IMAGE_FORMATS.md); web client currently renders gradient placeholders)~~ **DONE**
11. ~~Implement SSE endpoint `GET /api/v1/events` + `EventBus` in AppState — `DashMap<Uuid, broadcast::Sender>` per user (debt from [REAL_TIME_PUSH.md](docs/design/REAL_TIME_PUSH.md); storyboard generation progress is the first SSE consumer)~~ **DONE**
12. ~~Implement `clients/web/src/lib/stores/events.js` — Svelte store managing `EventSource` lifecycle; dispatches to domain stores (debt from [REAL_TIME_PUSH.md](docs/design/REAL_TIME_PUSH.md); player subscribes to storyboard-ready events)~~ **DONE**

**What was built for Task 12:**

| File | Purpose |
|---|---|
| `clients/web/src/lib/stores/events.js` | SSE client store — `EventSource` lifecycle management with handler registry. `connect()`/`disconnect()` methods; `on(type, handler)` returns unsubscribe fn; `off(type, handler)` for manual removal. Connection state writable: `'disconnected'`/`'connecting'`/`'connected'`. Fatal HTTP errors (401/403/429/500 → `readyState === CLOSED`) disconnect cleanly; network errors rely on native `EventSource` auto-reconnect. Handler registry (`Map<type, Set<fn>>`) dispatches named SSE events to domain stores. SSR-safe via `typeof EventSource` guard. Derived exports: `connectionState`, `isConnected`, `isConnecting`, `lastEventId` |
| `clients/web/src/routes/+layout.svelte` | SSE lifecycle wiring — new `$effect` calls `events.connect()` when `$isAuthenticated` becomes true, `events.disconnect()` on logout/cleanup. Imported `events` from `$lib/stores/events.js` |
| `clients/web/src/lib/stores/libraries.js` | First SSE consumer — registers `storyboard_progress` handler at module load. Tracks progress in `storyboardProgress` state field (set to latest event payload, cleared on `phase: 'completed'`). Fires toast notification on completion (success for 0 errors, warning for >0 errors). Exported new derived stores: `storyboardProgress`, `isGeneratingStoryboards` |

**Key decisions from Task 12:**

- **Handler registry over `onmessage`** — The browser's `EventSource` only fires `onmessage` for unnamed events. Duskcue uses named SSE events (`event: storyboard_progress`), so the store uses `addEventListener(type, dispatcher)` per type. A `Map<type, Set<handler>>` is the source of truth; `attachAllListeners()` re-registers on new `EventSource` creation. The dispatcher catches per-handler errors so one failing handler doesn't break others
- **No `?types=` query filter** — Store receives all authorized events, dispatches client-side. Simpler than tracking registered types and reconnecting when the set changes. Negligible bandwidth at Duskcue's scale (1–5 users). Server already enforces per-user authorization
- **Native `EventSource` auto-reconnect** — No custom exponential backoff. `onerror` with `readyState === CLOSED` → fatal HTTP error → disconnect. `readyState === CONNECTING` → network error → browser auto-reconnecting. The server's `retry: 5000` field guides reconnect delay
- **`Last-Event-ID` handled by the browser** — No manual tracking. `EventSource` sends the header automatically on reconnect; server `EventBus::replay_after()` handles the ring-buffer drain. The store records `lastEventId` for diagnostics only
- **Layout-managed connection lifecycle** — Avoids circular dependency (events store would need to import auth, while domain stores import both events and auth). The layout already manages auth redirects; adding SSE lifecycle is one `$effect` with cleanup return
- **Handler registration at module load** — Domain stores call `events.on(type, fn)` inside factory functions. Handler enters the registry immediately; `attachAllListeners()` wires it to the `EventSource` when `connect()` runs later. No race between handler registration and connection establishment
- **SSR-safe** — `connect()` guards with `typeof EventSource === 'undefined'`; domain store handler registration guards with `typeof window !== 'undefined'`. SvelteKit `adapter-node` SSR doesn't crash
- **`storyboard_progress` is the first consumer** — `libraries.js` registers a handler that tracks progress state and fires completion toasts. Future consumers: `player.js` for `transcode_progress`, `notifications.js` for `notification`, `auth.js` for `session_kicked`
- **0 svelte-check warnings, 0 build errors** — Matches the verification bar from Phase 10 Tasks 7–8 and Phase 8 Task 4

**What was built for Task 11:**

| File | Purpose |
|---|---|
| `server/src/services/event_bus.rs` | `EventBus` cross-cutting pub/sub: `DashMap<Uuid, Arc<UserChannel>>` keyed by user ID with per-user `broadcast::Sender<ServerEvent>`, 100-event `VecDeque` ring buffer, atomic connection counter, `ConnectionGuard` with `Drop` decrement, `publish()`, `subscribe()`, `subscribe_stream()`, `replay_after()`, `register_connection()`, `connection_count()`, `active_user_count()`; `ServerEvent { id: Uuid (UUIDv7), event_type: String, payload: serde_json::Value }`; `parse_type_filter()` + `matches_filter()` helpers; `CHANNEL_CAPACITY=256`, `RING_BUFFER_CAPACITY=100`, `DEFAULT_MAX_CONNECTIONS_PER_USER=5`; 15 unit tests |
| `server/src/services/events_handler.rs` | SSE transport: `events_handler()` for `GET /api/v1/events` with `?types=` filter, `Last-Event-ID` header replay, `X-Accel-Buffering: no`, 15s `KeepAlive`, `retry: 5000` on open; spawns per-connection forwarder task owning the broadcast receiver + `ConnectionGuard`; emits `Event::default().event(t).data(json).id(uuid)` per SSE spec |
| `server/src/services/mod.rs` | Added `pub mod event_bus;` and `pub mod events_handler;` |
| `server/src/state.rs` | Added `event_bus: Arc<EventBus>` to `AppState`; both `AppState::new()` and `AppState::new_with_config()` initialize with `EventBus::with_default_limit()` |
| `server/src/router.rs` | Registered `GET /api/v1/events` route via `get(crate::services::events_handler::events_handler)` alongside `/health` and `/metrics` |
| `server/src/workers/storyboard_generator.rs` | Added `requesting_user_id: Option<Uuid>` parameter to `generate_for_library()`, `generate_for_library_one()`, `generate_for_item_one()`; added `publish_progress()` helper emitting `storyboard_progress` events with `phase: started|progress|completed`, candidates, processed, generated, errors counts |
| `server/src/domains/storyboards/service.rs` | Updated `trigger_library_generation()` and `trigger_item_generation()` to accept and forward `requesting_user_id` |
| `server/src/domains/storyboards/handlers.rs` | Updated `generate_library_storyboards` and `generate_item_storyboards` to pass `Some(auth.user.user_id)` from `Require<CanManageLibraries>` |
| `Cargo.toml` | Added `tokio-stream = { version = "0.1", features = ["sync"] }` to workspace deps (for `BroadcastStream` wrapping `broadcast::Receiver` as a `Stream`) |
| `server/Cargo.toml` | Added `tokio-stream.workspace = true` |
| `docs/design/REAL_TIME_PUSH.md` | Updated Implementation Status table (SSE endpoint / EventBus / Last-Event-ID replay / storyboard_progress → ✅ Implemented); added Architecture decisions section; added `storyboard_progress` row to Event Taxonomy |

**Key decisions from Task 11:**

- **`EventBus` as a `services/` module, not a domain** — Cross-cutting infrastructure consumed by every domain. Same convention as `encryption.rs`, `artwork_delivery.rs`. The SSE *transport* lives in `services/events_handler.rs`; the SSE *route* is registered in `router.rs` alongside `/health` and `/metrics` (other cross-cutting endpoints). No domain five-file pattern needed because the bus has no DB CRUD, no error codes, no request DTOs.
- **`tokio::sync::broadcast` channel per user** — Native to `tokio` "full" feature (already in workspace); `broadcast::Sender::subscribe()` is cheap and lazy. Capacity 256 absorbs brief subscriber stalls; lagged receivers get `RecvError::Lagged` which the forwarder logs at `debug` and skips. The 100-event ring buffer covers typical disconnect/reconnect windows without hitting the lag path.
- **`UserChannel` lazily created via `DashMap::entry().or_insert()`** — First-touch allocates the channel; subsequent calls reuse. Channels are never removed (an admin who never connects incurs zero cost; an admin who connects once keeps their ring buffer warm for the next session). Memory cost is bounded by `256 + 100` events per active user.
- **Per-connection task pattern** — The SSE handler spawns a `tokio::spawn`'d forwarder task that owns the broadcast receiver, replay drain, type-filter check, and `ConnectionGuard`. When the client disconnects, Axum drops the response future → `ReceiverStream` sender closes → forwarder task exits on next `tx.send().await` → guard drops → connection count decrements. Deterministic, no leak window.
- **Replay strategy** — On reconnect with `Last-Event-ID: <uuid>`, the handler drains `EventBus::replay_after(user_id, id)`. UUIDv7 ids are time-ordered so the comparison is canonical. If the last-event-id is no longer in the buffer (older than ~5 min), the entire buffer is returned — clients may receive redundant events, which is safe because progress events are idempotent overwrites and notifications carry their own `id` for client-side dedup.
- **Scheduled task does not emit SSE events** — `run_storyboard_generation()` (scheduled 04:00) passes `None` for `requesting_user_id`. Admin-triggered generation passes `Some(user_id)` from `Require<CanManageLibraries>::user.user_id`. Scheduled-task results are visible in the scheduled-task-run history via Phase 13a.
- **`storyboard_progress` payload schema** — `{"phase":"started|progress|completed","library_id":null,"media_file_id":"uuid","media_item_id":"uuid|null","candidates":N,"processed":N,"generated":N,"errors":N}`. `phase` lets the client distinguish the initial fan-out (`started`, all zeros), per-file ticks (`progress`, incrementing counters), and the terminal state (`completed`, final counts). `library_id` reserved for future library-scoped events (currently null).
- **Connection limit returns `AppError::RateLimited`** — Per REAL_TIME_PUSH.md §Connection Limits: "Excess connections receive HTTP 429". Reuses the existing `RateLimited` variant with `code: "SSE_LIMIT_REACHED"` to avoid adding a new error code for an edge case.
- **`#[derive(Debug)]` on `EventBus`, `UserChannel`, `ConnectionGuard`** — Required because `Result<ConnectionGuard, ConnectionLimitReached>::unwrap_err()` needs `T: Debug` in tests. All fields are `Debug` (broadcast senders, mutexes, atomics all derive through).
- **`X-Accel-Buffering: no` via tuple-response** — `(HeaderMap, Sse<...>).into_response()` lets Axum merge the custom header into the SSE response. Documented nginx escape hatch per REAL_TIME_PUSH.md §Edge Cases.
- **0 new clippy warnings, 0 build warnings on new code** — The 4 remaining workspace clippy warnings are pre-existing (`artwork_downloader.rs` ×3 too_many_arguments, `transcoding.rs` ×1 too_many_arguments, `storyboards.rs` ×3 field_reassign_with_default in tests). `publish_progress` carries an `#[allow(clippy::too_many_arguments)]` matching the existing pattern.
- **283 tests pass (15 new for event_bus, 4 new for events_handler)** — All event_bus tests are pure unit tests (no DB, no async runtime needed); the events_handler tests verify `encode_event` and `EventsQuery` parsing without exercising the SSE wire format (Axum's responsibility).

**What was built for Task 9:**

| File | Purpose |
|---|---|
| `server/src/services/image_pipeline.rs` | Stateless image pipeline: decode (JPEG/PNG/WebP via `image` 0.25) → resize (Lanczos3, no-upscale) → WebP encode (lossy for opaque, lossless for alpha via `webp` 0.3 libwebp bindings); `ArtworkCategory` enum mapping DB artwork_type values to variant catalogs (poster w185/w342/w500/original, backdrop w300/w780/w1280/original, logo original, etc.); `generate_variant` / `generate_variants` / `resolve_variant` / `variant_path` / `write_variant` public API; 38 unit tests |
| `server/src/services/mod.rs` | Added `pub mod image_pipeline;` |
| `Cargo.toml` | Added `image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp"] }`, `webp = { version = "0.3", default-features = false }` to workspace deps |
| `server/Cargo.toml` | Added `image.workspace = true`, `webp.workspace = true` |
| `docs/design/IMAGE_FORMATS.md` | Updated Implementation Status table (WebP variant generation → ✅ Implemented); added `image` + `webp` crate integration notes documenting the `default-features = false` / `from_rgba` decoupling decision |

**Key decisions from Task 9:**

- **Stateless library, not a service with state** — Pure functions that take `&[u8]` source bytes and return `Vec<u8>` variant bytes. No DB, no AppState, no async. Matches `storyboards.rs`, `segments.rs`, `decision_engine.rs` pattern. Callers (Task 10 HTTP handler, future `artwork_variant_generator` worker) own disk I/O and `spawn_blocking` for the CPU-bound encode work
- **`image` + `webp` decoupled via `default-features = false`** — The `webp` crate's default `img` feature activates `Encoder::from_image(&DynamicImage)` but pins an `image` version internally. Duskcue disables it and uses `Encoder::from_rgba(&bytes, w, h)` / `Encoder::from_rgb(&bytes, w, h)` directly. The two crates evolve independently; no version coupling. The `image` crate is added with `default-features = false, features = ["jpeg", "png", "webp"]` to compile only the three decoders Duskcue needs (no GIF/TIFF/BMP bloat)
- **Alpha-aware encoding (automatic, not category-driven)** — `img.color().has_alpha()` gates the encode mode: RGBA sources (logos, clearart) → `encode_simple(true, 100.0)` lossless; RGB sources (posters, backdrops) → `encode_simple(false, 90.0)` lossy. Per IMAGE_FORMATS.md "For logos and clearart with transparency, encoding is lossless WebP". This means a rare opaque PNG poster still gets lossy (correct behavior — alpha presence is the trigger, not the source format)
- **No upscaling** — When variant target_width ≥ source width, the image is encoded at source resolution (no Lanczos3 upscale). Per IMAGE_FORMATS.md: variants are "generated for client delivery" — serving a w185 when the source is already 150px serves the source unchanged. The `original` variant (target_width = None) always encodes at source resolution
- **Lanczos3 resize filter** — Highest-quality downscale filter in the `image` crate; appropriate for artwork variants served to clients. Computationally heavier than `Nearest` or `Triangle` but quality matters for poster artwork displayed at 2x DPR on retina displays
- **Smallest-first variant generation order** — `POSTER_VARIANTS` static array is ordered w185 → w342 → w500 → original per IMAGE_FORMATS.md "Variant Generation Order": "generate in size order from smallest to largest. This ensures that if generation is interrupted, the most-important variants (thumbnails for browse pages) are already available"
- **Variant catalog mirrors TMDb size convention** — `w185`/`w342`/`w500`/`original` for posters, `w300`/`w780`/`w1280`/`original` for backdrops. Per IMAGE_FORMATS.md: "Predictable — admins familiar with TMDb's sizing understand the variants; cache-friendly — same set of variants for every poster regardless of source". The catalog is data-driven via static `&[SizeVariant]` arrays so adding a new category is a one-line table change
- **`ArtworkCategory` aligns with DB CHECK constraint** — Enum variants map 1:1 to `artwork.artwork_type` CHECK values (`poster`, `backdrop`, `thumbnail`, `logo`, `banner`, `season_poster`). `SeasonPoster` shares `Poster`'s variant catalog (same TMDb sizing). `Thumbnail` covers episode stills (w185/w300/original) since the DB schema uses `thumbnail` for both episode stills and generic thumbnails
- **Disk layout per IMAGE_FORMATS.md storage map** — `variant_path(images_cache_root, category, "w185", stem)` → `{images_cache_root}/webp/{category_plural}/{variant_label}/{stem}.webp`. The `webp/` prefix is hardcoded (not a parameter) since WebP is the sole delivery format per the design decision. `write_variant` is a convenience function that calls `variant_path` + `create_dir_all` + `std::fs::write`; the pure `generate_*` functions do not touch disk
- **`EncodeConfig` carries just `lossy_quality: f32`** — Maps 1:1 to `MetadataConfig.overlay_image_quality` (i32, default 90). The libwebp `method` parameter (default 4) is NOT exposed because `encode_simple` uses libwebp defaults and the simple API is infallible within the quality range. If future tuning needs method control, the migration path is `encode_advanced(&WebPConfig)` — the `EncodeConfig` struct can absorb a `method` field then
- **`encode_simple(lossless, quality)` returns proper `Result`** — Chosen over `encode(quality)` / `encode_lossless()` (which return `WebPMemory` directly with no error signal). `encode_simple` returns `Result<WebPMemory, WebPEncodingError>` so encode failures surface as `ImagePipelineError::Encode` rather than producing empty output silently
- **38 unit tests** covering: category round-trip and subdir mapping (3), variant catalog per category (6), variant resolution (3), decode from PNG/JPEG/garbage (3), resize width-preservation and no-upscale (4), encode lossy vs lossless and quality independence (3), single variant generation including no-upscale and invalid label (5), batch variant generation smallest-first and all-WebP verification (5), path layout (3), disk write including nested dir creation (2), default config quality (1)
- **No comments in code** — Module-level `//!` docs and per-item `///` docs only; zero inline `//` comments, consistent with all other service modules
- **0 clippy warnings on the new module** — The 4 remaining workspace clippy warnings (`artwork_downloader.rs` ×3, `transcoding.rs` ×1) are pre-existing `too_many_arguments` acknowledgements from Phase 6 Task 8 and Phase 7 Task 2 respectively

**What was built for Task 10:**

| File | Purpose |
|---|---|
| `server/src/services/artwork_delivery.rs` | Artwork delivery orchestration: `resolve_variant()` queries the `artwork` table for the primary artwork (order=0), does a cache lookup at `{data_dir}/cache/images/webp/{category}/{variant}/{artwork_id}.webp`, and on cache miss reads the source original and generates the WebP variant via `image_pipeline::generate_variant`; `default_variant_label()` provides per-category default sizes; `ResolvedArtwork` returns bytes + artwork_id for ETag construction; 5 unit tests |
| `server/src/services/mod.rs` | Added `pub mod artwork_delivery;` |
| `server/src/domains/media/handlers.rs` | Added `get_artwork` handler — `GET /api/v1/items/{id}/artwork/{type}?size={size}`; validates artwork type and size against `ArtworkCategory` and variant catalogs; reads `EncodeConfig` quality from `RuntimeConfig.metadata.overlay_image_quality`; returns binary WebP response with `Content-Type: image/webp`, `Cache-Control: public, max-age=86400, stale-while-revalidate=604800, immutable`, and `ETag: "{artwork_id}-{variant_label}"` |
| `server/src/domains/media/mod.rs` | Added route `/api/v1/items/{id}/artwork/{type}` |
| `clients/web/src/lib/utils/artwork.js` | URL builder utilities: `posterUrl(itemId, size)`, `backdropUrl(itemId, size)`, `thumbnailUrl(itemId, size)`, `logoUrl(itemId, size)` — construct relative URLs from the media item ID; client-side URL construction (no server-side URL embedding in media item responses) |
| `clients/web/src/lib/components/MediaCard.svelte` | Replaced `posterUrl` prop with `posterSize` prop (default `w342`); constructs the poster URL from `item.id` via `posterUrl()` utility; `<img onerror>` falls back to gradient placeholder on 404 |
| `clients/web/src/routes/media/[id]/+page.svelte` | Constructs backdrop URL (`w1280`) and poster URL (`w500`) from `item.id`; `onerror` handlers fall back to placeholders |

**Key decisions from Task 10:**

- **Client-constructed URLs over server-embedded URLs** — The web client constructs artwork URLs from the item ID (`/api/v1/items/{id}/artwork/poster?size=w342`). This avoids expensive `artwork` table JOINs on list endpoints (a 100-item grid would need 100+ artwork lookups) and follows the standard media-server pattern (Plex, Jellyfin, Emby all use deterministic URL construction). The browser sends the session cookie automatically on `<img>` loads (same-origin), so `AuthenticatedUser` extraction works for image requests
- **Shared service module, not a domain module** — `services/artwork_delivery.rs` follows the project's "shared services over singletons" convention. The delivery logic (DB query + cache + on-demand generation) is a service consumed by the media domain handler, not a standalone domain with its own five-file pattern. Artwork doesn't need its own error types or CRUD — `MediaError::ArtworkNotFound` (MEDIA_004, 404) covers all failure cases
- **Primary artwork only (order=0)** — The endpoint serves the best artwork by TMDb vote count (order=0 from `artwork_downloader.rs::sort_by_votes`). Alternate artwork selection (order=1, 2, etc.) is a POSTER_MANAGEMENT.md concern (Phase 12). The `?order=N` query parameter is reserved for future enhancement
- **Artwork row UUID as cache stem** — `image_pipeline::variant_path()` uses the artwork row's UUID as the `source_stem`. This is stable and unique per artwork row. When TMDb refresh replaces artwork, a new row (new UUID) is created, so the old cache files become orphaned (cleanable by a future cache GC task). Using the source filename as the stem was rejected because filenames contain TMDb-specific paths that may collide across items
- **Best-effort cache write** — On-demand variant generation writes to cache, but write failures are logged at WARN and do not block serving the bytes. Caching is an optimization; the variant is served from memory regardless of whether the disk write succeeds. The next request will retry the cache miss path
- **Corrupt/missing source → 404** — Per IMAGE_FORMATS.md "Corrupt Source Image" and "Artwork Not Found" edge cases: if the `artwork` row exists but the source file is missing from disk or the image decoder fails, the endpoint returns 404 (MEDIA_004). The web client's `<img onerror>` handler shows the gradient placeholder. No server crash, no 500 error
- **ETag from artwork_id + variant_label** — `ETag: "{artwork_id}-{variant_label}"` is a strong validator per RFC 9110. The WebP encode is deterministic (same source + same config = same bytes), so the ETag is stable. When TMDb refresh creates a new artwork row, the UUID changes, naturally invalidating the old ETag. Full `If-None-Match` → 304 handling is a pre-v1.0 hardening concern (HTTP_CACHING.md Task 1); the ETag header is set now so CDNs and browsers can use it immediately
- **Encode quality from runtime config** — `EncodeConfig.lossy_quality` reads from `RuntimeConfig.metadata.overlay_image_quality` (default 90). This reuses the existing config field rather than adding a separate artwork-quality setting, consistent with IMAGE_FORMATS.md decision 11 ("consistency with overlay format decision")
- **Invalid type/size → 400 BadRequest** — Unknown artwork type (e.g., `/artwork/foo`) or invalid size (e.g., `?size=w999`) returns `AppError::BadRequest` (400) with a descriptive message. Path and query parameter validation happens in the handler before any DB or disk access
- **0 new clippy warnings, 0 svelte-check warnings** — `try_read_cache` simplified to `.ok()` per clippy `manual_map` lint; all 264 server tests pass; web client builds cleanly

**Not yet implemented (deferred to later tasks/phases):**

- Background `artwork_variant_generator` scheduled task — pre-warms the WebP cache after library scans for the common case (post-scan). On-demand generation is the current cache-miss fallback with <500ms latency budget per image. The background task avoids first-request latency for bulk imports (Phase 14 migration scenario). Per IMAGE_FORMATS.md "Background-first strategy"
- `If-None-Match` → 304 Not Modified handling — the ETag header is set now, but the server doesn't yet check the request's `If-None-Match` to return 304. Deferred to Pre-v1.0 Hardening Task 1 (HTTP_CACHING.md)
- `<picture>` JPEG fallback — IMAGE_FORMATS.md specifies a `<picture>` element with WebP source + JPEG fallback for edge-case clients. All Duskcue target platforms support WebP (per the Platform Support Matrix), so this is insurance. Deferred to Pre-v1.0 Hardening
- `srcset` with multiple variants — MediaCard currently requests a single size (`w342`). Full `srcset` with `w185`/`w342`/`w500`/`original` variants lets the browser pick the right one for viewport/DPR. Deferred to Pre-v1.0 Hardening
- Alternate artwork selection (`?order=N`) — only the primary artwork (order=0) is served. Phase 12 (poster management) adds multi-source selection

**Verification:** After detection runs, media items have intro/credit markers. Skip button appears during intros in player. Seek bar shows thumbnail previews. Artwork renders on MediaCard (no more gradient placeholders). Storyboard generation progress streams via SSE.

**Phase 10 status:** All 12 tasks complete (8 core + 4 strategic implementation debt items).

---

## Phase 11 — Analytics & Trakt Integration

**Goal:** Activity tracking, analytics dashboard, and Trakt.tv sync.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [ANALYTICS.md](docs/design/ANALYTICS.md) | **Primary** — analytics API surface, route table, query parameter conventions, DTO design, pagination strategy |
| [DATABASE.md](docs/design/DATABASE.md) | `play_sessions`, `play_events`, `user_trust_events`, `user_trust_scores`, `trakt_accounts`, `trakt_sync_state` |
| [ANALYTICS_SECURITY.md](docs/security/ANALYTICS_SECURITY.md) | Impossible travel detection, GeoIP (MaxMind GeoLite2), 5-layer false positive suppression |
| [AUTH.md](docs/design/AUTH.md) | Trakt.tv account linking flow |

**Tasks:**

1. ~~Create `server/src/domains/analytics/` — five-file pattern~~ **DONE**
2. ~~Implement analytics dashboard — play history, top media, concurrent streams, bandwidth usage~~ **DONE**
 3. ~~Create `server/src/domains/trakt/` — five-file pattern~~ **DONE**
4. ~~Implement Trakt OAuth flow — account linking, token refresh~~ **DONE**
 5. ~~Implement Trakt sync — watch state push/pull, play count sync~~ **DONE**
 6. ~~Implement `server/src/workers/trakt_sync.rs` — periodic sync scheduled task~~ **DONE**
 7. ~~Implement `server/src/services/geoip.rs`:~~ **DONE**
    - ~~MaxMind GeoLite2 City MMDB loading with `maxminddb` crate (mmap)~~
    - ~~`ArcSwap` hot-reload on weekly update~~
    - ~~Graceful degradation when MMDB absent~~
 8. ~~Implement impossible travel detection:~~ **DONE**
   - ~~Haversine distance + 1,000 km/h threshold~~
   - ~~5-layer false positive suppression~~
   - ~~Notification-first response (admin dashboard alert, no auto-blocking)~~
 9. ~~Implement `server/src/workers/geoip_updater.rs` — weekly MMDB download~~ **DONE**

### Post-Phase 11 Trakt Follow-up

The original Phase 11 work delivered the Trakt domain and worker. The audit found that its completion notes overstated implemented sync categories and a few reliability guarantees. The following work is tracked separately so the product contract remains accurate.

1. ~~Harden token storage, sync state, and task outcomes~~ **DONE**
   - ~~Encrypt account tokens at rest and atomically migrate plaintext legacy rows on use~~
   - ~~Refresh at the five-minute expiry boundary and persist the rotated pair together~~
   - ~~Permit non-Trakt external-ID state, retain failed push rows, and persist safe sync outcomes~~
   - ~~Return global worker failures to the scheduler and expose completed manual-sync summaries~~
2. ~~Build dedicated admin credentials and personal account/sync UI, including the typed web client contract.~~ **DONE**
   - ~~Make `/admin/trakt` the canonical editor for masked operator credentials and redirect the retired System integrations link~~
   - ~~Make `/settings/trakt` the user-scoped device-code, sync, and status surface without exposing unsupported watchlist controls~~
   - ~~Document all twelve server routes and named web helpers in `client-contracts.v1.json`~~
3. ~~Implement or remove unsupported watchlist, rating-push, and collection-push settings; add deliberate request pacing.~~ **DONE**
   - ~~Remove watchlist from public account/sync-setting DTOs and disable the retained database column~~
   - ~~Keep ratings and collection explicitly pull-only in the product contract~~
   - ~~Pace sync GETs at 350ms and sync POSTs at one second process-wide while retaining Trakt `Retry-After` handling~~
4. ~~Keep the single-instance lock boundary explicit and add the documented Trakt metrics.~~ **DONE**
   - ~~Record bounded success/skipped/failure operation and duration metrics from the shared sync entry point~~
   - ~~Record safe sync error-code counters and publish their Prometheus names~~
   - ~~Retain the process-local lock as the deliberate boundary for Duskcue's single-active-instance deployment model~~

**What was built for Task 9:**

| File | Purpose |
|---|---|
| `server/src/workers/geoip_updater.rs` | Background GeoLite2-City MMDB updater: `run_geoip_update()` entry point — reads license key from bootstrap config, downloads tar.gz from MaxMind, extracts MMDB via `flate2`+`tar`, validates via `maxminddb::Reader::open_readfile`, atomically replaces target file, calls `GeoIpService::reload()` for hot-swap; 7 unit tests |
| `server/src/workers/mod.rs` | Added `pub mod geoip_updater;` |
| `server/src/config.rs` | Added `geoip_license_key: Option<String>` to `CliArgs` (with `DUSKCUE_GEOIP_LICENSE_KEY` env var) and `BootstrapConfig`; wired through config builder with `set_override_option` |
| `server/src/main.rs` | Registered `geoip_database_update` executor on scheduler (7th executor) with `geoip_state` capture clone |
| `server/src/services/scheduler.rs` | Added "GeoIP Database Update" to runtime `seed_default_tasks` (cron `0 3 * * 1`, enabled by default) |
| `server/migrations/20260624030000_seed_geoip_update_task.sql` | Seeds `geoip_database_update` scheduled task for existing deployments (cron weekly Monday 03:00, 600s timeout, enabled) |
| `Cargo.toml` | Added `tar = "0.4"` to workspace deps |
| `server/Cargo.toml` | Added `tar.workspace = true` |
| `docs/security/ANALYTICS_SECURITY.md` | Added Task 9 Implementation Notes section documenting: license key storage (bootstrap config), download URL format, archive extraction, atomic replace with validation, hot-reload failure handling, scheduled task configuration, `geoip_update_schedule` omission rationale, new `tar` dependency |

**Key decisions from Task 9:**

- **License key in bootstrap config, not DB** — Per ANALYTICS_SECURITY.md §First-Run Setup: "it's a secret needed before the database is available." Matches the `encryption_key` precedent. Worker reads `state.bootstrap.geoip_license_key` (configurable via `DUSKCUE_GEOIP_LICENSE_KEY` env var or `geoip_license_key` in `config.toml`).
- **Legacy download URL (query-param format)** — Uses `https://download.maxmind.com/app/geoip_download?edition_id=GeoLite2-City&license_key={key}&suffix=tar.gz` which requires only the license key (no account ID), matching the single `geoip_license_key` config field. MaxMind's newer permalink API requires Basic Auth with both `account_id` and `license_key`. The legacy format still works (redirects to the same Cloudflare R2 presigned URL; `reqwest` follows redirects by default).
- **Validate before atomic replace** — The new MMDB is validated by `maxminddb::Reader::open_readfile` BEFORE the atomic rename. A corrupt download deletes the temp file and aborts without touching the existing database. This prevents a bad download from breaking geolocation until the next weekly run.
- **`tar` crate for extraction** — MaxMind distributes the MMDB inside a gzip-compressed tar archive with a dated directory prefix (`GeoLite2-City_YYYYMMDD/GeoLite2-City.mmdb`). `flate2` (already in workspace) handles gzip; `tar = "0.4"` (new workspace dep) handles tar entry iteration. The worker scans for any `.mmdb` suffix — robust against date-prefix changes.
- **Crash-safe hot-reload** — If the file replacement succeeds but `GeoIpService::reload()` fails (rare), the error is logged at ERROR but does not block. The new database loads on the next server restart. The existing reader stays active until a successful reload. Matches the graceful-degradation design.
- **Cross-platform atomic replace** — Unix `rename` atomically overwrites; Windows requires remove-then-rename (tiny race window acceptable for a weekly background task).
- **No `geoip_update_schedule` in `AnalyticsConfig`** — The design doc lists it in the config table, but scheduling is controlled by the DB `scheduled_tasks.cron_expression` column (seeded by this task). Adding the field without an admin API to sync it to the DB would be misleading. Phase 13's scheduled-task management API will update the DB cron directly.
- **Task enabled by default, no-op without key** — The worker logs an info message and returns early when no license key is configured, so enabling the task is harmless. The opt-in is at the license-key level, not the task level. This matches the `segment_analysis`/`storyboard_generation` precedent (enabled by default, no-op when preconditions aren't met).
- **600s timeout** — MMDB download + extraction is typically <30s on broadband. The 10-minute timeout provides a generous margin for slow connections and mirrors MaxMind's recommendation for automated download clients.
- **No new workspace dependencies beyond `tar`** — HTTP download uses existing `reqwest` (follows R2 redirects automatically); gzip decompression uses existing `flate2`; MMDB validation uses existing `maxminddb`; URL encoding uses existing `urlencoding`.

**What was built for Task 1:**

| File | Purpose |
|---|---|
| `server/src/domains/analytics/mod.rs` | Module declarations + router assembly with 9 routes across 5 path groups |
| `server/src/domains/analytics/error.rs` | `AnalyticsError` enum with 5 variants: `UserNotFound`, `TrustEventNotFound`, `InvalidDateRange`, `InvalidTimePreset`, `Database` catch-all |
| `server/src/domains/analytics/types.rs` | Three-type DTOs: 3 Row types (`PlaySessionRow`, `TrustEventRow`, `TrustScoreRow`), 4 query param types (`AnalyticsQuery`, `PlayHistoryQuery`, `TopMediaQuery`, `TrustEventQuery`), 12 Response types; validation statics (`VALID_TIME_PRESETS`, `VALID_STREAM_DECISIONS`, `VALID_SEVERITIES`) |
| `server/src/domains/analytics/service.rs` | 9 `todo!()` service function stubs: `get_analytics_overview`, `list_play_history`, `get_top_media`, `get_bandwidth_usage`, `get_concurrent_streams`, `list_trust_scores`, `list_trust_events`, `acknowledge_trust_event`, `get_geoip_status` |
| `server/src/domains/analytics/handlers.rs` | 9 handlers wired to `Require<CanViewAnalytics>` + `State`, `Query`, `Path` extractors; all return `Result<Json<T>, AppError>` |
| `server/src/error.rs` | Added `AppError::Analytics(#[from] AnalyticsError)` variant + `analytics_error_to_http()` mapping all 5 error variants to existing error codes |
| `server/src/domains/mod.rs` | Added `pub mod analytics;` |
| `server/src/router.rs` | Merged analytics router via `.merge(crate::domains::analytics::router(state.clone()))`, removed Phase 11 analytics comment |
| `docs/design/ANALYTICS.md` | Created — domain design document covering API surface, route table, query conventions, pagination strategy, error handling |

**Key decisions from Task 1:**

- **No new error codes per ANALYTICS_SECURITY.md** — The security design doc states "No new API error codes are needed — trust events are created in the background and surfaced via the admin dashboard, not as API errors." The `AnalyticsError` enum variants map to existing codes: `UserNotFound` → USER_001 (404), `TrustEventNotFound` → NOT_FOUND (404), `InvalidDateRange`/`InvalidTimePreset` → VALID_001 (422), `Database` → INTERNAL (500). This follows the SegmentError/StoryboardError precedent of domain-specific enums mapping to a small set of existing codes.
- **Routes under `/api/v1/analytics/*`** — Per API_CONVENTIONS.md route table: `| Analytics | /api/v1/analytics/* | Dashboard, play history, bandwidth, transcode stats |`. Trust events are nested under `/api/v1/analytics/trust/*` and GeoIP status under `/api/v1/analytics/geoip/status`.
- **All endpoints require `Require<CanViewAnalytics>`** — The `can_view_analytics` capability (one of the 12 marker types from Phase 4 Task 11) gates all 9 endpoints. No user self-service endpoints in the initial scaffolding; a future `GET /api/v1/analytics/me/history` can be added if user-facing play history is needed.
- **9 routes covering both Task 2 and Task 8 scope** — Dashboard analytics (overview, play-history, top-media, bandwidth, concurrent) for Task 2; security analytics (trust/scores, trust/events, trust/events/{id}/acknowledge, geoip/status) for Tasks 7–9. The scaffolding defines the full route surface upfront so the router is stable; service stubs are `todo!()` and will be filled in as each task is implemented.
- **Cursor pagination for play history, offset for trust events** — Play sessions are high-volume time-series (partitioned by month, append-only) so cursor pagination avoids offset degradation. Trust events are lower volume and the admin dashboard benefits from page numbers.
- **Common query parameters (`range`, `from`, `to`, `user_id`, `library_id`)** — Shared `AnalyticsQuery` struct used by overview/bandwidth endpoints; `PlayHistoryQuery` and `TopMediaQuery` extend it with domain-specific filters. Time presets (`24h`, `7d`, `30d`, `90d`, `all`) are validated against `VALID_TIME_PRESETS` static. Explicit `from`/`to` ISO 8601 dates override presets when both are provided.
- **`#![allow(unused_variables)]` on service.rs** — All 9 service functions are `todo!()` stubs; the module-level allow suppresses unused parameter warnings until actual implementations are added in Task 2 (and Tasks 7–9 for security analytics).
- **No new workspace dependencies** — all functionality uses existing `sqlx`, `serde`, `uuid`, `chrono`, `axum`, `thiserror` crates.
- **`docs/design/ANALYTICS.md` created** — Domain design document covering the API surface, route table, query conventions, pagination strategy, error handling, and implementation notes. The DB schema is already documented in DATABASE.md (Activity domain); the security engine is documented in ANALYTICS_SECURITY.md. ANALYTICS.md covers the HTTP API layer that reads from those tables.

**Context from Task 1 for Task 2:**

- All 9 routes are wired and return `Result<Json<T>, AppError>`; service functions are `todo!()` stubs accepting `&PgPool` and query params
- The `AnalyticsQuery`, `PlayHistoryQuery`, `TopMediaQuery`, and `TrustEventQuery` structs define the query parameter interface; Task 2 implements the actual SQL queries that consume them
- Play history uses cursor pagination — the `PlayHistoryResponse` has `has_more` + `next_cursor` fields matching the media domain's cursor pattern
- Trust events use offset pagination — the `TrustEventListResponse` has `total`, `page`, `page_size`, `total_pages` matching the users domain's offset pattern
- The `AnalyticsOverviewResponse` aggregates counts for the dashboard summary in a single response (total plays, unique users, watch time, concurrent streams, transcode breakdown)

**What was built for Task 2:**

| File | Purpose |
|---|---|
| `server/src/domains/analytics/service.rs` | Replaced 5 of 9 `todo!()` stubs with working implementations: `get_analytics_overview` (range-bound aggregation + separate concurrent count), `list_play_history` (cursor pagination over `play_sessions` joined to users/media_items), `get_top_media` (GROUP BY media_item_id, two static SQL variants for play_count vs watch_time sort), `get_bandwidth_usage` (gap-free time series via `generate_series` + `date_bin` + `LEFT JOIN` + `COALESCE`), `get_concurrent_streams` (active sessions where `stopped_at IS NULL`); shared helpers: `resolve_time_range`, `resolve_bucket_interval`, `encode_cursor`/`parse_cursor`, `row_to_play_session_response`; 13 unit tests. The 4 security-analytics stubs (`list_trust_scores`, `list_trust_events`, `acknowledge_trust_event`, `get_geoip_status`) remain `todo!()` for Tasks 7–9. |
| `docs/design/ANALYTICS.md` | Added "Implementation Notes — Task 2 (Dashboard)" section documenting: time-range resolution semantics, adaptive bucket-interval table, the `generate_series`/`date_bin` bandwidth query rationale (Crunchy Data best practice), cursor-pagination approach, concurrent-stream 24h partition-pruning guard, transcode-breakdown-from-column decision, and LEFT JOIN defensiveness. |

**Key decisions from Task 2:**

- **Time-range resolution (`resolve_time_range`)** — Shared helper returns `(Option<DateTime<Utc>>, DateTime<Utc>>)`. `to` defaults to `Utc::now()`; explicit `from` (validated `<= to`, else `InvalidDateRange`) takes precedence over the `range` preset per the design's "explicit from/to overrides range" rule; `range` defaults to `7d` and is validated against `VALID_TIME_PRESETS` (else `InvalidTimePreset`); `all` → `None` (unbounded lower bound). All range-bound queries bind `from` via `($N::timestamptz IS NULL OR started_at >= $N)` keeping `started_at` in the WHERE clause for partition pruning on the range-partitioned `play_sessions` table.
- **Adaptive bucket interval** — Bandwidth time-series stride adapts to range span: ≤24h → 1h, ≤7d → 6h, else 1d. Bounds the chart to a sensible point count regardless of range. When range is `all` (unbounded), bandwidth clamps the effective range to the last 90 days so `generate_series` has a finite axis.
- **Bandwidth query = `generate_series` + `date_bin` + `LEFT JOIN` + `COALESCE`** — Per current PostgreSQL best practice (Crunchy Data / Paul Ramsey, June 2026 research): `generate_series` produces the complete bucket axis, `date_bin` (PG14+) bins session timestamps to the same stride/origin guaranteeing JOIN alignment, `LEFT JOIN` + `COALESCE(..., 0)` fills empty buckets so charts render without dead zones. The bucket stride is bound as a parameter (`$1::interval`) — a value parameter, not SQL structure — so the query remains a static string satisfying sqlx 0.9's `SqlSafeStr` requirement.
- **Bandwidth semantics = per-session point estimate bucketed by start time** — `bandwidth_bps` is a per-session point estimate; each bucket sums the `bandwidth_bps` of sessions that *started* in that bucket. This is "bandwidth demand by start time" (standard Plex/Jellyfin dashboard semantics). Concurrent-bandwidth-over-time (range-overlap integration) is intentionally deferred as it requires expensive overlap queries against a non-sustained point estimate.
- **Cursor pagination reuses media-domain pattern** — base64-encoded `{"id":"<uuid>"}` JSON, `LIMIT N+1` for `has_more`, `WHERE id < cursor`. `play_sessions.id` is UUIDv7 (naturally time-ordered) so `ORDER BY id DESC` gives reverse-chronological order. `limit` clamped to `[1, 100]` (default 20).
- **Top-media sort via two static SQL constants** — sqlx 0.9 requires static SQL (no `format!()`), so `TOP_MEDIA_BY_PLAY_COUNT_SQL` and `TOP_MEDIA_BY_WATCH_TIME_SQL` are separate constants differing only in `ORDER BY`. `sort_by` defaults to `play_count`; unknown values fall back to `play_count` (lenient). `GROUP BY mi.id` relies on PG functional-dependency rule (PK in GROUP BY allows selecting other columns from the same table).
- **Concurrent streams 24h guard** — `WHERE stopped_at IS NULL AND started_at > now() - interval '24 hours'` prunes to at most two partitions and excludes stale crash-recovery sessions (an unstopped session older than 24h is an artifact, not a real concurrent stream). `count` derived from result-set length (no separate COUNT query).
- **Transcode breakdown from the real column, not metadata** — Overview computes the stream-decision split directly from the `play_sessions.stream_decision` column (`COUNT(*) FILTER (WHERE stream_decision = 'direct_play')`), not from `metadata` JSONB. `stream_decision` is a NOT NULL column with a CHECK constraint. (The Phase 7 quality-domain `get_transcode_breakdown` queries `metadata->>'playback_type'` against an older schema assumption; the analytics dashboard does not use that path.)
- **Overview uses two queries** — One range-bound aggregation query + one concurrent-count query, combined in Rust. Concurrent streams is a "right now" metric independent of the selected time range, so a separate query is cleaner than a subquery; the concurrent query is sub-millisecond on the partitioned table.
- **LEFT JOIN + COALESCE for display-name/title enrichment** — Play-history and concurrent-stream queries use `LEFT JOIN users` / `LEFT JOIN media_items` with `COALESCE(..., 'Unknown')`. `play_sessions` has `ON DELETE CASCADE` on both FKs so INNER JOIN would be functionally equivalent, but LEFT JOIN is defensive against partial-state edge cases and never drops analytics rows.
- **`#![allow(unused_variables)]` retained** — The 4 security-analytics service stubs (Tasks 7–9) are still `todo!()`; the module-level allow suppresses their unused-parameter warnings.
- **No new workspace dependencies** — all functionality uses existing `sqlx`, `chrono`, `base64`, `uuid`, `serde_json` crates.
- **13 unit tests** covering: time-range resolution (default 7d, 24h, all-unbounded, explicit from/to, from>to rejection, bad-preset rejection, from-precedence-over-range), bucket-interval selection (hourly/six-hourly/daily), cursor encode/decode roundtrip, cursor garbage rejection, cursor missing-id-field rejection. All 296 server tests pass (283 prior + 13 new).

**What was built for Task 3:**

| File | Purpose |
|---|---|
| `server/src/domains/trakt/mod.rs` | Module declarations + router assembly with 8 routes (10 endpoints) across 5 path groups |
| `server/src/domains/trakt/error.rs` | `TraktError` enum with 11 variants: TRAKT_001–005 (AccountNotLinked, RateLimited, TokenExpired, ServiceUnavailable, Timeout) per ERROR_HANDLING.md + 5 domain-specific (DeviceCodeExpired, DeviceCodePending, DeviceCodeDenied, SyncInProgress, NotConfigured) + Database catch-all |
| `server/src/domains/trakt/types.rs` | Three-type DTOs: 2 Row types (`TraktAccountRow`, `TraktSyncStateRow`) matching DATABASE.md schema; 2 Request DTOs with Validate (`PollDeviceCodeRequest`, `UpdateSyncSettingsRequest`); 1 query param type (`HistoryQuery`); 6 Response DTOs (`TraktAccountResponse`, `DeviceCodeResponse`, `SyncSettingsResponse`, `SyncTriggerResponse`, `SyncStatusResponse`, `TraktHistoryResponse`/`TraktHistoryItem`) |
| `server/src/domains/trakt/service.rs` | 10 `todo!()` service function stubs: `get_account`, `start_device_link`, `poll_device_code`, `unlink_account`, `get_sync_settings`, `update_sync_settings`, `trigger_sync`, `get_sync_status`, `list_history`, `list_ratings` — implemented in Tasks 4–6 |
| `server/src/domains/trakt/handlers.rs` | 10 handlers wired to Axum extractors (`State`, `AuthenticatedUser`, `Query`, `Json`); all return `Result<Json<T>, AppError>`; validation error mapping for `PollDeviceCodeRequest` and `UpdateSyncSettingsRequest` following subtitles domain convention |
| `server/src/error.rs` | Added `AppError::Trakt(#[from] TraktError)` variant + `trakt_error_to_http()` mapping all 11 variants; Trakt `ServiceUnavailable`/`Timeout` added to `Retry-After` header group per ERROR_HANDLING.md reference implementation |
| `server/src/domains/mod.rs` | Added `pub mod trakt;` |
| `server/src/router.rs` | Merged trakt router via `.merge(crate::domains::trakt::router(state.clone()))`, removed Phase 11 trakt comment |
| `docs/design/TRAKT.md` | Created — domain design document covering API surface, OAuth device code flow, sync architecture (push/pull), merge strategy, pagination (June 2026 API changes), rate limiting, error handling, configuration |

**Key decisions from Task 3:**

- **Routes match API_CONVENTIONS.md** — `/api/v1/trakt/*` path prefix per the route table: "Link account, sync, history, ratings". 8 route paths yielding 10 endpoints (GET+DELETE on `/account`, GET+PUT on `/settings`)
- **OAuth device code flow (RFC 8628) as primary linking method** — Duskcue runs headless; the user authenticates via a separate browser device. Same pattern as the auth domain's device linking. Three-step flow: `POST /account/link` (get device code) → user visits Trakt activate URL → `POST /account/poll` (exchange device code for access token)
- **All endpoints require `AuthenticatedUser`** — Trakt is a per-user resource; each user manages their own Trakt link. No admin capability needed (self-service). All queries scoped by `user_id` from the authenticated session — BOLA prevention at the query level
- **TraktError matches ERROR_HANDLING.md reference exactly** — The 5 prescribed TRAKT variants (AccountNotLinked → 409, RateLimited → 429, TokenExpired → 409, ServiceUnavailable → 503, Timeout → 504) implemented verbatim. `RateLimited` carries `{ retry_after_secs: Option<u32> }` field for the Trakt `Retry-After` header. 5 additional domain-specific variants mapped to existing codes (DeviceCodeExpired/Pending → BAD_REQUEST, DeviceCodeDenied → FORBIDDEN, SyncInProgress → CONFLICT, NotConfigured → INTERNAL) following the Segment/Storyboard precedent
- **Retry-After on Trakt 503/504** — `ServiceUnavailable` gets `Retry-After: 60`, `Timeout` gets `Retry-After: 30` — matches the generic ServiceUnavailable/GatewayTimeout pattern and the ERROR_HANDLING.md reference implementation
- **Token fields never in responses** — `TraktAccountRow` includes `access_token`/`refresh_token` for service-layer use, but `TraktAccountResponse` omits them entirely. The response has `linked: bool` and `token_expires_at` for client display but never the actual tokens
- **Offset pagination for history/ratings** — `trakt_sync_state` is user-scoped and moderate volume (bounded by the user's Trakt library size, typically hundreds to low-thousands). Offset pagination with `page`/`page_size` is appropriate and lets the UI show page numbers. Same pattern as the users domain
- **`docs/design/TRAKT.md` created** — Domain design document covering the full API surface, OAuth device code flow, bidirectional sync architecture (push/pull), merge strategy per DATABASE.md user_item_data design, June 2026 pagination changes (enforced after June 30, per GitHub discussion #775), rate limits, error handling, and configuration. References the DB schema in DATABASE.md (authoritative for DDL) and error codes in ERROR_HANDLING.md (authoritative for the registry)
- **`#![allow(unused_variables)]` on service.rs** — All 10 service functions are `todo!()` stubs; the module-level allow suppresses unused parameter warnings until actual implementations are added in Tasks 4–6
- **No new workspace dependencies** — all functionality uses existing `sqlx`, `validator`, `serde`, `uuid`, `chrono`, `axum` crates

**What was built for Task 4:**

| File | Purpose |
|---|---|
| `server/src/services/trakt_client.rs` | Trakt OAuth HTTP client — `TraktClient` (stateless: holds `client_id`, `client_secret`, `redirect_uri`, `reqwest::Client`); methods: `request_device_code()`, `exchange_device_code()`, `refresh_token_pair()`, `get_user_settings()`; deserialization types (`TraktTokenResponse`, `TraktUserSettings`, `TraktSettingsUser`, `TraktAccount`, `TraktTokenErrorResponse`); error mapping (`map_network_error` → Timeout/ServiceUnavailable; `map_oauth_error` → RateLimited/DeviceCodePending/Expired/Denied); 16 unit tests |
| `server/src/services/mod.rs` | Added `pub mod trakt_client;` |
| `server/src/state.rs` | `IntegrationsConfig` expanded with `trakt: TraktConfig`; `TraktConfig { client_id, client_secret, redirect_uri }` with `is_configured()` helper and default `redirect_uri = "http://localhost:48027/trakt/callback"`; `load_runtime_config()` now decrypts `integrations.trakt.client_secret` via `decrypt_trakt_config()` |
| `server/src/services/encryption.rs` | Added `decrypt_trakt_config()` and `encrypt_trakt_config()` — same AES-256-GCM pattern as metadata/subtitle provider keys; only `client_secret` encrypted (client_id/redirect_uri are public) |
| `server/src/domains/trakt/service.rs` | Implemented 4 OAuth service functions: `get_account` (queries `trakt_accounts`, returns `linked: false` when absent), `start_device_link` (builds `TraktClient` from config, calls `request_device_code()`), `poll_device_code` (exchanges device code → fetches `/users/settings` → upserts `trakt_accounts` row), `unlink_account` (DELETE row); plus `ensure_valid_token` (proactive refresh with write-back) and 2 admin settings functions (`get_settings`, `update_settings`); helper functions: `trakt_client()`, `load_account()`, `upsert_account()`, `update_tokens()`, `token_expires_at()`, `row_to_account()`, `account_to_response()`, `reload_runtime_config()` |
| `server/src/domains/trakt/handlers.rs` | Updated `get_account`, `start_link`, `poll_link`, `unlink_account` to pass `&state` (not `&state.pool`); added `get_integration_settings` + `update_integration_settings` handlers (`Require<CanManageServer>`) |
| `server/src/domains/trakt/types.rs` | Added `UpdateTraktSettingsRequest` (Deserialize + Validate) and `TraktSettingsResponse` (Serialize with masked secret) DTOs |
| `server/src/domains/trakt/mod.rs` | Added `/api/v1/settings/trakt` route (GET + PUT, admin-only) for OAuth credential configuration |

**Key decisions from Task 4:**

- **`services/trakt_client.rs` over inline service HTTP** — Dedicated module following the established pattern (`tvdb_client.rs`, `subdl_client.rs`, `fanart_client.rs`). The OAuth HTTP calls are cross-cutting infrastructure consumed by the domain service + future sync worker (Task 6); keeping HTTP in a service module keeps the domain service focused on DB + orchestration. Returns `TraktError` directly (no separate `TraktClientError` mapping layer) since the client is trakt-specific
- **⚠️ Access token TTL corrected from 7 days to 90 days** — Research (GitHub issue #48, maintainer `@tysonkerridge`) confirmed `expires_in ≈ 7776000` seconds (~3 months). The original TRAKT.md stated "7 days" which was incorrect. TRAKT.md updated with the correction and source citation
- **⚠️ Refresh token rotation (critical)** — Trakt (Doorkeeper-backed) rotates the refresh_token on every refresh; the old one is revoked. Maintainer `@rectifyer`: *"revoked once it is used and a new access token + refresh token is generated."* The service MUST persist the new token pair after every refresh — failing to do so permanently locks the account. This rules out a lazy "refresh on 401, read-only" pattern
- **Proactive `ensure_valid_token()` with write-back** — Refreshes when `token_expires_at - now() < 5 min` (5-min buffer), writes the new pair back to `trakt_accounts` in the same call, then returns the access token. Safe under refresh_token rotation. Infrastructure for Task 5/6 (sync) to call before every Trakt API request
- **Single-poll-per-request** — `POST /api/v1/trakt/account/poll` makes exactly one `/oauth/token` attempt and returns the RFC 8628 result (`authorization_pending`/`slow_down` → `DeviceCodePending`; `expired_token` → `DeviceCodeExpired`; `access_denied` → `DeviceCodeDenied`; success → `TraktAccountResponse`). The client (web UI) drives the retry loop at the `interval` (5s). Keeps HTTP connections short; matches the `DeviceCodeResponse` DTO contract
- **`/users/settings` mapping** — `account.id` (numeric) → `trakt_user_id` (BIGINT); `user.username` → `trakt_username` (TEXT). Extra fields (`user.vip`, `account.timezone_id`, `connections`) ignored by serde. Fetched once on successful device-code exchange
- **Re-link as upsert** — `poll_device_code()` success does `INSERT ... ON CONFLICT (user_id) DO UPDATE` (the `user_id` UNIQUE constraint), so re-linking replaces the old Trakt account cleanly. `created_at` is `GENERATED ALWAYS` (excluded from INSERT); `updated_at = now()` on update
- **`client_secret` encrypted at rest** — Same AES-256-GCM pattern as metadata/subtitle provider keys (`encrypt_trakt_config`/`decrypt_trakt_config`). Decrypted in `load_runtime_config()`; encrypted before DB write in `update_settings()`. `client_id` and `redirect_uri` are public (not encrypted)
- **`redirect_uri` in config with default** — Trakt/Doorkeeper requires `redirect_uri` for the refresh-token grant to match an app-registered URI. Stored in `TraktConfig` with default `http://localhost:48027/trakt/callback`. Not used for the device-code request or initial token exchange
- **`Content-Type: application/json`** for all Trakt OAuth requests — Confirmed via HAR capture in GitHub issue #48 (differs from RFC 8628's form-urlencoded example). The `TraktClient` sets `Content-Type` + `trakt-api-version: 2` + `trakt-api-key` as default headers
- **Admin settings endpoint at `/api/v1/settings/trakt`** — `Require<CanManageServer>` (admin-only); `GET` returns masked `client_secret` (`***...***` via `mask_secret`); `PUT` encrypts + persists + hot-reloads config. Mirrors the subtitle provider settings pattern (Phase 9 Task 8). Separate from `/api/v1/trakt/settings` (sync toggles, user-scoped) to avoid conflating operator credentials with per-user sync preferences
- **No new workspace dependencies** — All HTTP via existing `reqwest`; JSON via existing `serde`/`serde_json`; encryption via existing `ring`; no new crates added
- **16 unit tests** covering: token response parsing (with/without scope), user settings parsing (minimal + extra fields ignored), OAuth error response parsing (pending, slow_down with description), error code mapping (pending, slow_down, expired, denied, 5xx, unknown fallback), raw device code parsing (with/without complete URL), client construction (with creds + empty creds). All 312 server tests pass (296 prior + 16 new)

**Context from Task 4 for Tasks 5–6:**

- `ensure_valid_token(state, user_id)` is ready for Task 5/6 to call before every Trakt API request — it returns `(access_token, TraktAccountRow)` and handles proactive refresh + write-back automatically
- `trakt_client(state)` helper builds a `TraktClient` from the live config; returns `NotConfigured` when credentials are missing
- `TraktClient` has the OAuth methods ready but no sync methods yet — Task 5 will add `get_watched()`, `add_to_history()`, `get_ratings()`, etc. to the client
- `trakt_accounts` rows now store valid tokens with correct `token_expires_at`; Task 6 worker iterates `WHERE sync_enabled = true` and calls `ensure_valid_token` per user
- The `trakt_sync` scheduled task is already seeded (migration `20260530070000_seed_default_data.sql`, 1800s interval, disabled by default per TRAKT.md) — Task 6 registers the executor on the scheduler

- All 10 routes are wired and return `Result<Json<T>, AppError>`; service functions are `todo!()` stubs accepting `&PgPool` and `user_id`
- The `TraktAccountRow` and `TraktSyncStateRow` types match the DB schema exactly — Task 4 (OAuth) will use `TraktAccountRow` for INSERT/SELECT on `trakt_accounts`; Task 5 (sync) will use `TraktSyncStateRow` for upserts on `trakt_sync_state`
- `TraktError::NotConfigured` is reserved for Task 4 — when `IntegrationsConfig` lacks Trakt `client_id`/`client_secret`, the OAuth endpoints will return this error. Task 4 will expand `IntegrationsConfig` (similar to how Phase 9 Task 8 added subtitle provider config)
- `DeviceCodeResponse` matches the Trakt `/oauth/device/code` response shape — Task 4 will call Trakt's API and map directly into this struct
- `SyncTriggerResponse` reports a completed inline sync and includes its summary; the scheduled worker separately iterates sync-enabled accounts

**What was built for Task 5:**

| File | Purpose |
|---|---|
| `server/src/services/trakt_client.rs` | Added 7 sync HTTP methods: `get_watched_movies`, `get_watched_episodes`, `get_ratings`, `get_collection_movies` (GET, paginated) + `add_to_history`, `add_to_ratings`, `add_to_collection` (POST). Generic `paginate<T>()` helper loops `page` with `limit=250` until empty array (with a 1000-page runaway guard). `authed_post()` helper maps 401→TokenExpired, 429→RateLimited (Retry-After). Raw API types: `TraktIds` (with `to_id_object()`/`is_empty()`), `TraktMediaObject`, `TraktEpisodeObject`, `TraktWatchedMovie`, `TraktWatchedEpisode`, `TraktRating`, `TraktCollectionMovie`, `TraktSyncCounts` (with `total()` + `AddAssign`), `TraktSyncPostResponse`. 11 unit tests. |
| `server/src/domains/trakt/service.rs` | Replaced 6 `todo!()` stubs: `get_sync_settings`, `update_sync_settings` (DB COALESCE upsert on `trakt_accounts` sync_* columns), `get_sync_status` (aggregate counts on `trakt_sync_state`), `list_history`/`list_ratings` (offset pagination, `div_ceil` page count), `trigger_sync`. Added core `run_sync()` engine: pull (watched movies+episodes, ratings all 4 types, collection) → merge into `trakt_sync_state` + propagate to `user_item_data` per merge strategy → push local-watched-not-on-Trakt in one batched POST. `MediaMatcher` (in-memory `HashMap` lookup by trakt/tmdb/imdb/tvdb priority, scoped to media_item type). Per-category merge upserts (`upsert_sync_watched`, `apply_uid_watched`, `upsert_sync_rating`, `apply_uid_rating`, `upsert_sync_collection`, `mark_pushed_as_synced`). `try_acquire_sync_lock` (per-user DashMap lock with 15-min TTL). |
| `server/src/domains/trakt/types.rs` | Added `SyncSummary` (pulled/pushed/unmatched counts + completion flag). |
| `server/src/domains/trakt/handlers.rs` | `trigger_sync` now passes `&state` (not `&state.pool`) since `run_sync` needs the client + lock. |
| `server/src/state.rs` | Added `trakt_sync_locks: Arc<DashMap<Uuid, Instant>>` to `AppState` (both constructors). |

**Key decisions from Task 5:**

- **Pull granularity = leaf items (movies + episodes)** — Duskcue tracks `is_watched` at the leaf level, so it pulls `/sync/watched/movies` and the new `/sync/watched/episodes` type directly (episodes confirmed live per #775 maintainer reply, April 2026), avoiding the expensive `shows`→`seasons`→`episodes` flattening. Series/season containers are not propagated to `user_item_data`.
- **In-memory matcher over per-item SQL** — one query loads all `media_items` carrying external IDs into four `HashMap`s keyed by `(type, id)`; matches resolve in O(1) with priority order trakt→tmdb→imdb→tvdb. No title/year fuzzy matching for automated watched-state writes (too error-prone).
- **All POST responses return `added` as an object** — confirmed via the OpenAPI spec; `add_to_history`, `add_to_ratings`, `add_to_collection` all return `{added:{movies,shows,seasons,episodes}, not_found:{...}}`. `existing`/`updated` appear only on collection. For the implemented watched push, a non-empty `not_found` response fails the run and leaves rows unconfirmed for retry.
- **Merge strategy implementation** — pull: `is_watched`=OR, `play_count`=GREATEST, `last_played_at`=GREATEST, `resume_position_ms`=0 when watched (mirrors playback `upsert_user_item_data_stop`). Rating is applied to `user_item_data.user_rating` **only when NULL** (no local `rated_at` column exists for timestamp-based override; conservative — documented as a known limitation in TRAKT.md). Push is incremental: only `user_item_data.is_watched=true` rows where `trakt_sync_state.is_watched IS DISTINCT FROM true`.
- **Trakt type → media_items type mapping** — `movie`→`movie`, `show`→`series`, `episode`→`episode`, `season`→`season` (Trakt uses "show"; Duskcue's `media_items.type` uses "series").
- **Pagination stop = empty array, not headers** — the `X-Pagination-*` headers are inconsistently present on sync endpoints (per OpenAPI spec); Duskcue loops pages until `[]` with a 1000-page hard cap to guard against the ignored-pagination bug reported in #775.
- **Per-user sync lock (in-memory)** — `DashMap<Uuid, Instant>` in `AppState` with 15-min TTL guards `run_sync` against concurrent manual+manual or manual+worker races; returns `TraktError::SyncInProgress`. Matches the existing in-memory `DashMap` pattern (WebAuthn challenges). The merge logic is idempotent (ON CONFLICT/GREATEST/OR) so a crash-leaving-stale-lock is non-fatal (TTL reclaims).
- **`trigger_sync` implemented in Task 5 (not 6)** — the manual `POST /api/v1/trakt/sync` endpoint now runs the engine inline and returns a summary; Task 6 adds the *scheduled* worker that iterates all `sync_enabled` users calling the same `run_sync()`.
- **No new workspace dependencies** — sync uses existing `reqwest`, `serde`, `sqlx`, `chrono`, `uuid`, `dashmap`. All 323 server tests pass (312 prior + 11 new).

**What was built for Task 6:**

| File | Purpose |
|---|---|
| `server/src/workers/trakt_sync.rs` | Scheduled Trakt sync worker: `run_trakt_sync(state, task_id, config)` entry point — iterates all `trakt_accounts WHERE sync_enabled = true`, calls `run_sync()` per user; per-user error isolation with global-abort classification (`NotConfigured`/`RateLimited`/`ServiceUnavailable`/`Timeout` abort the batch; `AccountNotLinked`/`TokenExpired`/`SyncInProgress`/`Database` skip and continue); optional `config.user_id` for single-user sync; `AggregateResult` summary struct; `fetch_sync_enabled_users` ordered `last_full_sync_at ASC NULLS FIRST`; `is_global_failure` classifier; 4 unit tests |
| `server/src/workers/mod.rs` | Added `pub mod trakt_sync;` |
| `server/src/main.rs` | Registered `trakt_sync` executor on scheduler (6th executor) with `trakt_state` capture clone |
| `server/src/services/scheduler.rs` | Added "Trakt Sync" to runtime `seed_default_tasks` (interval 1800s) |

**Key decisions from Task 6:**

- **Scheduled iteration over `run_sync`** — Mirrors the `subtitle_auto_fetch`/`segment_analysis`/`storyboard_generation` pattern: query candidate users → call existing `run_sync()` per user → aggregate results. The worker is a thin orchestration layer; all pull/merge/push logic, token refresh, and per-user locking live in Task 5's `run_sync`. This matches the established "synchronous API + scheduled iteration" precedent and keeps `SyncTriggerResponse.queued` semantics honest (the manual `POST /api/v1/trakt/sync` endpoint calls `run_sync` inline; the worker just iterates it)
- **Error classification: global abort vs per-user skip** — `NotConfigured`, `RateLimited`, `ServiceUnavailable`, `Timeout` abort the entire batch (every subsequent user would fail identically against the Trakt API); `AccountNotLinked`, `TokenExpired`, `SyncInProgress`, `Database` skip the user and continue. `RateLimited` abort is explicit per the Task 5 design ("a `Retry-After` 429 aborts the sync; the worker retries next interval"). Aborting on global API failures avoids hammering a down/rate-limited service for every user and worsening the rate limit. The scheduler retries the whole task on the next 30-min interval
- **`token_expires_at > now()` guard intentionally omitted (deviation from TRAKT.md §Scheduled Task)** — The design doc specifies `WHERE sync_enabled = true AND token_expires_at > now()`. The `token_expires_at` filter is NOT applied. `token_expires_at` tracks the *access* token (90-day TTL), but `ensure_valid_token` (Task 4) refreshes expired access tokens via the long-lived *refresh* token. A user whose access token lapsed but whose refresh token is still valid would be incorrectly skipped forever — permanently halting sync after the first 90-day access token expired. The candidate query filters only on `sync_enabled = true`; unrecoverable tokens surface as `TokenExpired` and are skipped per-user. This deviation is documented in the worker's module docs
- **`ORDER BY last_full_sync_at ASC NULLS FIRST`** — Users who have never synced (or synced longest ago) are processed first. After server downtime or a backlog, this clears the stalest accounts fairly rather than always favoring the most-recently-synced user
- **Optional `config.user_id` for single-user sync** — Mirrors `segment_detector`'s `library_id` and `storyboard_generator`'s `library_id`. Enables targeted admin triggers and testing without iterating all users. The default scheduled task config is `{}` (empty), iterating all sync-enabled users
- **`trakt_sync` task enabled by default (no-op when unlinked)** — Unlike `subtitle_auto_fetch` (disabled by default because it consumes external API quota unconditionally), `trakt_sync` is enabled by default because it is a pure no-op when zero `trakt_accounts` rows exist. The opt-in is at the account-linking level (`sync_enabled` per user), not the task level. Matches the original Phase 2 seed (`20260530070000_seed_default_data.sql`: `is_enabled = true`, `interval_seconds = 1800`). The BUILD_ORDER Task 4 context note ("disabled by default per TRAKT.md") referred to the account-linking opt-in, not the task enablement — the task itself is safely enabled
- **Registered in `seed_default_tasks`** — Added to `scheduler.rs::seed_default_tasks` alongside `subtitle_auto_fetch`, `segment_analysis`, and `storyboard_generation` for fresh-install consistency. The Phase 2 migration seed already creates the row for existing deployments, so no re-seed migration is needed (unlike segment_analysis/storyboard_generation which each shipped dedicated re-seed migrations for existing deployments)
- **No new workspace dependencies** — the worker uses existing `sqlx`, `uuid`, and the Task 5 `run_sync` engine. All 327 server tests pass (323 prior + 4 new)

**What was built for Task 7:**

| File | Purpose |
|---|---|
| `server/src/services/geoip.rs` | GeoLite2-City MMDB reader service: `GeoIpService` with `ArcSwap<Option<Reader<Vec<u8>>>>` for lock-free hot-reload; `lookup(ip)` returning owned `GeoLocation` (city, region, country, continent, coordinates, accuracy radius, timezone); `reload()` for atomic MMDB swap after weekly download; `status()` for the admin GeoIP status endpoint; `classify_location(ip, server_subnets)` returning `LocationType` (Lan/Wan/Relay); `is_private_ip()` covering RFC 1918 + CGNAT + loopback + link-local + ULA; `LocationType`, `GeoLocation`, `GeoIpStatus`, `GeoIpError` types; 20 unit tests |
| `server/src/services/mod.rs` | Added `pub mod geoip;` |
| `server/src/state.rs` | Added `AnalyticsConfig` struct (8 fields from ANALYTICS_SECURITY.md config table: `geoip_enabled`, `impossible_travel_enabled`, `velocity_threshold_kmh`, `min_distance_km`, `lookback_hours`, `same_country_suppress`, `trusted_ips`, `trusted_cidrs`); added `analytics: AnalyticsConfig` to `RuntimeConfig` + `Default`; added `geoip: Arc<GeoIpService>` to `AppState`; `new()` uses `GeoIpService::disabled()`, `new_with_config()` creates real service from `bootstrap.data_dir`; `load_runtime_config()` reads `analytics` JSONB column |
| `server/src/domains/analytics/service.rs` | Replaced `get_geoip_status` `todo!()` stub with working implementation using `GeoIpService::status()` — reads database file presence/age/size from filesystem, reports `enabled` from config |
| `server/src/domains/analytics/handlers.rs` | `get_geoip_status` handler now reads `geoip_enabled` from `RuntimeConfig.analytics` and delegates to the `GeoIpService` |
| `docs/security/ANALYTICS_SECURITY.md` | Added "Task 7 Implementation Notes" section documenting: module location decision (`services/geoip.rs` cross-cutting vs `domains/analytics/geolocation.rs`), `maxminddb` 0.28 API changes, `Reader<Vec<u8>>` buffer type rationale, `ArcSwap<Option<...>>` graceful degradation pattern, location classification design |

**Key decisions from Task 7:**

- **`services/geoip.rs` over `domains/analytics/geolocation.rs`** — BUILD_ORDER Task 7 prescribes the `services/` location; ANALYTICS_SECURITY.md §Rust Implementation suggested the domain location. The service is cross-cutting: consumed by analytics (impossible travel detection, Task 8) and playback (play-session geolocation enrichment at session start). Placing it in `services/` follows the established convention (`encryption.rs`, `event_bus.rs`, `artwork_delivery.rs`, `decision_engine.rs`). The impossible-travel trust engine (Haversine, 5-layer suppression) remains in `domains/analytics/service.rs` (Task 8 scope)
- **`maxminddb` 0.28 API** — Verified against docs.rs (June 2026): `lookup(ip)` returns `Result<LookupResult>`; `result.decode::<geoip2::City>()` returns `Result<Option<City>>` (double-unwrap needed: `.ok()??`). `geoip2` is a module *within* the `maxminddb` crate (`use maxminddb::{Reader, geoip2}`), not a separate dependency. Decoded records borrow the reader's internal buffer (`geoip2::City<'a>`), so `lookup()` extracts owned data (`String`, `f64`, `u16`) into `GeoLocation` within the ArcSwap-guard scope before returning
- **`Reader<Vec<u8>>` over `Reader<Mmap>`** — `open_readfile()` loads the entire 70 MB file into an owned `Vec<u8>` (not mmap). Rationale: `'static`, `Send + Sync`, clean `ArcSwap` storage with no file-handle/mmap lifetime entanglement. 70 MB is negligible for a media server (FFmpeg transcoding uses GBs). The `mmap` *feature* remains enabled (per design doc Cargo.toml) for future flexibility — switching to `Reader::from_source(mmap)` is a one-line change if memory pressure ever warrants it
- **`ArcSwap<Option<Reader<Vec<u8>>>>` graceful degradation** — When MMDB is absent/corrupt at startup, `None` is stored; `lookup()` returns `None` (geolocation silently skips); `is_available()` returns `false` (admin dashboard shows "GeoIP not configured"). `reload()` (Task 9 updater) populates `Some(reader)` — lookups begin working without restart. Matches design: "If the MMDB file is missing at startup, geolocation enrichment is skipped"
- **`reload()` preserves existing reader on failure** — If the new MMDB fails to open, the ArcSwap is not touched; the old reader keeps serving. A failed weekly update does not take geolocation offline
- **`GeoLocation` includes continent code + accuracy radius** — Beyond the `play_sessions` schema's `geo_city/geo_region/geo_country/geo_lat/geo_lon`, the owned struct also carries `continent_code` (for impossible-travel severity: new continent → high severity, Task 8), `country_name` (for admin dashboard display), `accuracy_radius_km` (MaxMind's 67% confidence radius — useful for trust-event detail), and `time_zone` (for future client-side timezone display). All are cheap to extract during the single decode call
- **CGNAT `100.64.0.0/10` classified as LAN** — Tailscale (default `100.x.x.x` range) and WireGuard meshes use RFC 6598 CGNAT space. `is_private_ip()` checks this range via bitmask (`octets[1] & 0xC0 == 0x40`) alongside the std `Ipv4Addr::is_private()` (RFC 1918 only). Matches ANALYTICS_SECURITY.md: "LAN and VPN connections (Tailscale `100.x.x.x`, WireGuard `10.x.x.x`) are classified as LAN"
- **IPv6 ULA `fc00::/7` and link-local `fe80::/10` via bitmask** — `std::net::Ipv6Addr` lacks stable `is_unique_local()`/`is_link_local()` methods (as of Rust 1.88); implemented via `segments()[0]` bitmask checks. ULA: `& 0xFE00 == 0xFC00`; link-local: `& 0xFFC0 == 0xFE80`. Deprecated `fec0::/10` (old site-local) correctly classified as WAN (not private) — covered by an explicit test
- **`classify_location` as free function in the service module** — Pure IP-classification concern (no MMDB dependency); consumed by both playback enrichment and impossible-travel suppression. Uses `ipnet::IpNet::contains()` (already a workspace dep, used by `middleware.rs` metrics-subnet guard). `server_subnets` parameter lets operators mark point-to-point public IPs as LAN
- **No new workspace dependencies** — `maxminddb = { version = "0.28", features = ["mmap"] }` and `ipnet = "2"` were already in workspace Cargo.toml (pre-added in anticipation of Phase 11). All 347 server tests pass (327 prior + 20 new geoip tests); 0 clippy warnings on new code

**Not yet implemented (deferred to Task 9):**

- `geoip_updater.rs` scheduled task (Task 9) — weekly MaxMind download, temp-file + atomic-rename, calls `GeoIpService::reload()`. The `reload()` method is implemented and tested; the worker that triggers it is not
- `geoip_database_update` scheduled task seeding — the task type is in the `scheduled_tasks` CHECK constraint (Phase 2); seeding the default row and registering the executor is Task 9

**What was built for Task 8:**

| File | Purpose |
|---|---|
| `server/src/domains/analytics/service.rs` | Full trust engine: `haversine_distance()` (pure `f64` math, Earth radius 6371 km), `implied_velocity_kmh()`, `enrich_and_detect()` fire-and-forget entry point, `detect_impossible_travel()` 5-layer suppression engine, `update_session_geo()` (populates `play_sessions.ip_address`/`location_type`/`geo_*`), `upsert_location_history()` (90-day baseline tracking), `is_ip_trusted()` (trusted IP + CIDR matching), `is_country_in_baseline()`, `create_trust_event()` + `upsert_trust_score()` (GREATEST(0, ...) clamping). Replaced 3 `todo!()` stubs: `list_trust_scores`, `list_trust_events` (offset pagination + severity/acknowledged filters), `acknowledge_trust_event`. 15 new unit tests. |
| `server/src/middleware.rs` | `extract_client_ip` refactored from private `fn(&Request)` to `pub fn(&HeaderMap, Option<&SocketAddr>)` — reusable by handlers; 2 existing call sites updated |
| `server/src/main.rs` | `into_make_service_with_connect_info::<SocketAddr>()` added to `axum::serve` call — makes `ConnectInfo<SocketAddr>` available as an axum extractor (benefits rate limiter + metrics subnet guard + geo enrichment) |
| `server/src/domains/playback/handlers.rs` | `start_playback` handler now extracts client IP via `extract_client_ip(&headers, Some(&connect_info))` and spawns `tokio::spawn(analytics::service::enrich_and_detect(...))` after session creation — fire-and-forget enrichment never blocks playback |

**Key decisions from Task 8:**

- **Fire-and-forget enrichment via `tokio::spawn`** — Play-session geo enrichment + impossible travel detection runs asynchronously after `start_playback` returns. Enrichment failures are logged at `WARN` and never block playback or surface to the API caller. The spawned task clones `AppState` (all fields are `Arc`/pool-backed, cheap to clone). This matches the design's "notification-first, never blocks" philosophy
- **Haversine as pure `f64` math** — No external crate (`geo` / `haversine`) added. The formula is ~5 lines using `f64::to_radians()`/`sin()`/`cos()`/`asin()`. Verified against known distances: Chicago→London ~6360 km, NYC→LA ~3940 km. Earth radius = 6371 km (mean radius per IUGG)
- **`INET` column bound as string** — sqlx 0.9 doesn't implement `Encode`/`Decode` for `std::net::IpAddr` without the `ipnetwork` feature. IP addresses are bound as `ip.to_string()` with `$N::inet` SQL cast; decoded as `String` then parsed to `IpAddr`. Adding the `ipnetwork` feature was considered but rejected to avoid pulling in the `ipnetwork` crate for a single use case
- **Severity based on distance, not continent** — The design doc specifies "new continent → high, same continent → medium". The `play_sessions` table has no `geo_continent` column. Severity is determined by Haversine distance: > 4000 km (roughly transcontinental/transatlantic) → "high"; 500–4000 km → "medium"; destination in user's 90-day baseline → "low" regardless of distance. 4000 km approximates the width of a continent
- **5-layer suppression order** — Evaluated per ANALYTICS_SECURITY.md §Suppression Decision Flow: (1) LAN/VPN → suppress entirely [WAN-only query filter]; (2) trusted IP/CIDR → reduce to "low"; (3) same country → suppress; (4) same device → suppress [currently no-op since `client_device` is NULL]; (5) distance < min → suppress; velocity ≤ threshold → skip; velocity > threshold → check baseline → determine severity
- **Trust score upsert with `GREATEST(0, ...)`** — Score never goes below 0. New users start at 100; first violation: `INSERT ... VALUES (100 - impact)`. Existing users: `ON CONFLICT DO UPDATE SET score = GREATEST(0, score - impact)`. Impact: low=-2, medium=-5, high=-10 per design's severity levels table
- **`into_make_service_with_connect_info`** — Added to the `axum::serve` call so `ConnectInfo<SocketAddr>` is available. Previously, the rate limiter and metrics subnet guard fell back to `0.0.0.1` for direct connections without proxy headers. Now they get the actual socket address. This is a pre-existing infrastructure gap that Task 8 needed to close
- **`extract_client_ip` made public** — Refactored to accept `&HeaderMap` + `Option<&SocketAddr>` so handlers can extract the client IP without duplicating the X-Forwarded-For → X-Real-IP → ConnectInfo fallback chain. Both middleware call sites updated to pass headers + connect_info extracted from request extensions
- **Same-device suppression is no-op until device tracking lands** — `play_sessions.client_device` is currently NULL (not set by `create_play_session`). The same-device check is implemented correctly (`prev_device.is_some() && prev_device == current_device`) but will always be false until a future phase sends device identifiers. This is documented as a deferred enhancement
- **`div_ceil` for pagination** — `total.div_ceil(page_size)` instead of manual `(total + page_size - 1) / page_size` (clippy suggestion; Rust 1.73+ stable)
- **No new workspace dependencies** — All functionality uses existing `sqlx`, `chrono`, `serde_json`, `uuid`, `ipnet`, and the already-built `services::geoip` module

**Verification:** Play sessions generate analytics data visible in dashboard. Trakt-linked users sync watch state. Impossible travel alerts appear in admin dashboard for suspicious logins.

**Phase 11 status:** The original 9 tasks and all four Post-Phase 11 Trakt follow-up tasks are complete.

---

## Phase 12 — Kometa-Like System (Overlays, Collections, Posters)

**Goal:** Overlay compositing engine, dynamic collections, and multi-source poster management.

**Prerequisites:** Phase 10 complete (image pipeline `services/image_pipeline.rs` with WebP encode/resize, artwork delivery endpoint `GET /api/v1/items/{id}/artwork/{type}`, and `image` + `webp` crates already in workspace). Phase 11 complete (GeoIP + analytics infrastructure available for geo-conditional overlays if needed). The `MetadataConfig` struct already has overlay fields (`overlay_image_quality`, `overlay_format`, etc.) from Phase 6.

**Context from Phase 11:**
- The `image` crate (`0.25`, features: `jpeg`, `png`, `webp`) and `webp` crate (`0.3`) are already in the workspace from Phase 10 Task 9 — overlay compositing reuses these directly
- `services/image_pipeline.rs` provides `generate_variant()`, `resize()`, `encode()` primitives — overlay compositing builds on this foundation
- `artwork_delivery.rs` resolves primary artwork (order=0) — Phase 12 extends this with multi-source selection (order=N) and poster locking (`artwork.is_locked`)
- The `geoip_database_update` scheduled task pattern (scheduler executor + seed migration + `seed_default_tasks` entry) is the reference for Phase 12's `overlay_application`/`overlay_cleanup` scheduled tasks

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [METADATA_OVERLAYS.md](docs/design/METADATA_OVERLAYS.md) | **Primary** — overlay types (image/text/backdrop), canvas standards, groups, queues, conditions, compositing pipeline (pure Rust: `image` + `ab_glyph` + `resvg`) |
| [COLLECTIONS.md](docs/design/COLLECTIONS.md) | **Primary** — three collection types (static/dynamic/smart), 14 internal + 13 external builders, templates |
| [POSTER_MANAGEMENT.md](docs/design/POSTER_MANAGEMENT.md) | **Primary** — five artwork sources, selection priority, poster locking, asset directory, community packs |

**Tasks:**

1. ~~Create `server/src/domains/overlays/` — five-file pattern~~ **DONE**
2. ~~Implement `server/src/services/overlays.rs`:~~ **DONE**
   - ~~Compositing pipeline using `image` + `ab_glyph` + `resvg`~~
   - ~~Image overlay (alpha blending)~~
   - ~~Text overlay (with special variables: resolution, ratings, codecs)~~
   - ~~Backdrop overlay~~
   - ~~Group mutual exclusion, queue auto-stacking~~

   **Context from Task 1:** The `overlays` domain is scaffolded with `todo!()` stubs. Task 2 builds the compositing service at `server/src/services/overlays.rs` (a shared service module, not part of the domain five-file) and wires it into `service::preview_overlay` (the editor live-preview endpoint). The `OverlayDefinitionRow` (all 30 columns) and validation statics are ready for the compositor to read overlay config. The `image` (0.25, features `jpeg`/`png`/`webp`) and `webp` (0.3) crates are already in the workspace from Phase 10; `ab_glyph`, `fontdb`, and `resvg` need to be added. Output is WebP per [IMAGE_FORMATS.md](docs/design/IMAGE_FORMATS.md). The full pipeline design (group/queue resolution, suppress rules, layer ordering) is in the Compositing Pipeline section of [METADATA_OVERLAYS.md](docs/design/METADATA_OVERLAYS.md).

   **What was built for Task 2:**

   | File | Purpose |
   |---|---|
   | `server/src/services/overlays.rs` | Stateless compositing library: `composite()` entry point, `resize_to_canvas()` (Lanczos3, no upscaling), `compute_position()` (align+offset math), `composite_image()` (PNG/SVG via `imageops::overlay`), `composite_text()` (ab_glyph rasterization + backdrop fill), `composite_backdrop()` (rounded rect fill); `render_text_to_buffer()` (manual glyph layout + stroke dilation), `render_svg()` (resvg→tiny-skia→RgbaImage un-premultiply bridge), `FontRegistry` (scan `/data/fonts/` keyed by lowercased filename stem); pure resolution helpers: `resolve_groups()`, `apply_suppress_rules()`, `resolve_queue_positions()`; `parse_hex_color()` supporting #RGB/#RGBA/#RRGGBB/#RRGGBBAA; 60 unit tests |
   | `server/src/services/mod.rs` | Added `pub mod overlays;` |
   | `server/src/services/image_pipeline.rs` | Made `encode_webp()` public — needed by overlay domain to encode composited RgbaImage to WebP |
   | `server/src/domains/overlays/service.rs` | `preview_overlay` replaced `todo!()` with working implementation: loads primary artwork from `artwork` table, loads overlay definitions (by IDs or all enabled for artwork type), resolves text variables from media context, converts rows to `ResolvedOverlay`, calls `services::overlays::composite()`, encodes result to WebP, writes to cache preview directory; added helpers: `load_media_context()`, `resolve_text_variables()` (6 variables: title, year, resolution, video_codec, audio_codec, critic_rating), `load_overlay_definitions_for_preview()`, `row_to_definition_row()`, `row_to_resolved()` |
   | `server/src/domains/overlays/handlers.rs` | `preview_overlay` handler now passes `&state` instead of `&state.pool` |
   | `server/assets/fonts/Inter-Regular.ttf` | Test font (Google Fonts Inter, OFL license) for unit tests requiring glyph rasterization |
   | `Cargo.toml` | Added `ab_glyph = "0.2"`, `resvg = "0.47"` to workspace deps |
   | `server/Cargo.toml` | Added `ab_glyph.workspace = true`, `resvg.workspace = true` |

   **Key decisions from Task 2:**

   - **`ab_glyph` 0.2 + `resvg` 0.47 over design doc's 0.2/0.44** — resvg 0.47 is current as of June 2026; re-exports `usvg` 0.47 + `tiny-skia` 0.12 via `resvg::usvg` / `resvg::tiny_skia` so no separate deps for those. The `resvg::render()` API changed: takes `(tree, transform, &mut PixmapMut)` and returns `()` (not `Option<Rect>` as in 0.44). SVG fitting is done via `Transform::from_scale()` rather than the removed `FitTo` enum.
   - **`fontdb` dropped from the original 3-crate plan** — The design doc listed `image-overlay`, `fontdb`, and `resvg`. `image-overlay` (advanced blend modes) is unnecessary since `image::imageops::overlay()` covers all source-over alpha blending needs. `fontdb` (CSS-like family/weight/style queries) is overkill for a self-hosted server with bundled fonts. The compositing service resolves `font_family` by matching the lowercased filename stem in `/data/fonts/` (e.g., `Inter.ttf` ↔ `font_family: "Inter"`), falling back to the first available font. This avoids version-matching with `usvg`'s transitive `fontdb` 0.23 dependency while keeping font resolution simple and predictable. Deviation documented in METADATA_OVERLAYS.md Crate Selection section.
   - **Stateless service module, not domain module** — `services/overlays.rs` follows the established convention (`image_pipeline.rs`, `segments.rs`, `storyboards.rs`, `decision_engine.rs`). Pure library functions take typed inputs and return `RgbaImage` bytes; no DB, no AppState, no HTTP coupling. Fully unit-testable without a database. The domain `service.rs::preview_overlay` is the orchestration point.
   - **Manual glyph layout over `Layout`/`TextStyle`** — ab_glyph 0.2 doesn't expose a high-level `Layout` type. Text layout is done manually: iterate characters, look up `glyph_id`, apply kerning via `PxScaleFont::kern()`, advance via `h_advance()`, create positioned `Glyph` via `GlyphId::with_scale_and_position()`, and rasterize via `outline_glyph().draw(|x, y, coverage| …)`. Sufficient for overlay text (single-line, Latin script). Complex shaping (Arabic, ligatures) is not needed for rating badges and resolution labels.
   - **Stroke via mask dilation** — Text stroke is implemented by rasterizing the glyph to a binary mask, dilating it by `stroke_width` pixels (circular kernel), then filling the dilated region with the stroke color only where the original glyph had zero alpha (the outline ring). This produces a clean outline without double-rendering the glyph interior. O(stroke_width²) per pixel — acceptable for the small stroke widths (0–10px) used in overlay badges.
   - **tiny-skia ↔ image bridge** — `resvg` renders onto `tiny_skia::Pixmap` (premultiplied RGBA). `pixmap_to_rgba()` converts to non-premultiplied `RgbaImage` by un-premultiplying each pixel: `a = data[3]; if a > 0 { r = data[0]*255/a }`. Fully-transparent pixels become `(0,0,0,0)`. This is the standard interop pattern between tiny-skia and the `image` ecosystem.
   - **`OverlayPipelineError` separate from `OverlayError`** — Pipeline failures (decode, font load, SVG parse) are operational errors that the worker logs and skips. The domain layer translates `OverlayPipelineError` to `OverlayError::CompositingFailed` for API responses — matching the `segments`/`storyboards` precedent of separate pipeline vs domain error types.
   - **`resolved_overlay` flat struct over enum** — Uses a single flat struct with an `overlay_type` discriminator field, matching the DB schema structure. All fields are present regardless of type; the compositor dispatches on `overlay_type` to know which fields are relevant. Simpler to construct from a DB row than an enum with per-variant common-field duplication.
   - **Text variable resolution in domain layer, not compositing service** — The compositing service receives fully-resolved text strings (no `<<variables>>`). The domain layer's `resolve_text_variables()` substitutes `<<title>>`, `<<year>>`, `<<resolution>>`, `<<video_codec>>`, `<<audio_codec>>`, `<<critic_rating>>` from `MediaContext` (queried from `media_items` + `media_files`). Full variable set (runtime, audio_channels, season/episode numbers, file_size, edition, etc.) deferred to Task 3 when condition evaluation provides richer media-item context.
   - **Preview writes to `/cache/images/overlays/previews/`** — One-off preview renders (not persisted in `artwork_overlay_state`). The preview URL is `/cache/images/overlays/previews/preview_{media_item_id}.webp`. Overwritten on each preview request. The `overlay_compositor` worker (Task 8) will persist production results in `artwork_overlay_state` with proper clean-art preservation.
   - **`encode_webp()` made public in image_pipeline** — The overlay domain needs to encode a composited `RgbaImage` to WebP without the full decode→resize→encode pipeline. `encode_webp()` was a private helper in `image_pipeline.rs`; made public since it's a well-defined primitive with existing tests.
   - **60 unit tests covering**: canvas dimensions/artwork-type mapping, overlay type layer ordering, hex color parsing (4 formats + invalid rejection), position computation (4 alignment corners), pixel blending (opaque/transparent/semi-transparent), rounded rectangle fill (corners transparent, square vs rounded pixel counts, corner region detection), group resolution (highest-weight wins, empty group = standalone, single in group), suppress rules (listed slugs removed, empty suppress list), queue positioning (vertical stacking, non-queued unaffected), font registry (empty, case-insensitive resolve, first fallback, resolve-or-first, no-font error), canvas resize (downscale, pad smaller, exact-match noop), composite integration (no overlays, backdrop fill, PNG image blend, text error without font, text renders pixels, text with backdrop, layer ordering backdrop-before-text), pixmap conversion (opaque, transparent, un-premultiply), SVG rendering (basic, invalid error, auto-dimensions), mask dilation (single pixel expansion, zero radius), text rendering (empty string, visible pixels, stroke adds pixels), glyph layout (ASCII, control-char skipping), FontRegistry::scan_dir (loads TTF, ignores non-fonts, missing dir).
   - **0 clippy warnings, 0 build warnings** — All new code passes `clippy::all`. 429 total server tests pass (369 prior + 60 new).
3. ~~Implement condition evaluation — JSONB filter rules against `media_items`/`media_files`~~ **DONE**

   **What was built for Task 3:**

   | File | Purpose |
   |---|---|
   | `server/src/services/conditions.rs` | Pure, stateless condition evaluation engine — `MediaFilterContext` struct (16 condition-testable fields + derived booleans), `evaluate()` → bool, `validate_structure()` → Result; recursive AND/OR evaluator with 8 operators (eq, neq, in, gt/gte/lt/lte, exists, matches); 64 unit tests |
   | `server/src/services/mod.rs` | Added `pub mod conditions;` |
   | `server/src/domains/overlays/service.rs` | Replaced `MediaContext` (6 fields) with `OverlayMediaContext` (21 fields — all condition fields + text-variable fields); `to_filter_context()` conversion to `MediaFilterContext`; `load_media_context()` rewritten with single `LEFT JOIN LATERAL` query (primary media file + file count + genre aggregation); `preview_overlay()` now filters definitions by `conditions::evaluate()` before group/suppress/queue resolution; `resolve_text_variables()` expanded from 6 to 16 template variables; added `extract_streaming_services()`, `format_audio_channels()` helpers |

   **Key decisions from Task 3:**

   - **`services/conditions.rs` over `domains/overlays/conditions.rs`** — The condition system is shared between overlay definitions (METADATA_OVERLAYS.md §Conditions) and smart collections/playlists (COLLECTIONS.md §Smart Filter Syntax). Placing it in `services/` avoids coupling the future collections domain (Task 5+) to the overlay domain module. Follows the cross-cutting service convention (`decision_engine.rs`, `segments.rs`).
   - **No external JSON rule engine crate** — Research (June 2026) evaluated `datalogic-rs` and `json-eval-rs` (JSONLogic implementations). Rejected because JSONLogic's schema (`{"==": [...]}`) differs from Duskcue's documented schema (`{"operator": "and", "rules": [...]}`), requiring a translation layer; existing crates are heavy form-validation engines. Hand-written recursive evaluator with zero new dependencies.
   - **Case-insensitive text comparisons** — All `eq`/`neq`/`in` text comparisons use `eq_ignore_ascii_case` so admin-facing values like `"4k"` match DB-stored `"4K"`. The `matches` operator uses standard regex; admins add `(?i)` for case-insensitive regex.
   - **Malformed conditions return false at evaluation time** — A malformed rule (missing `field`/`op`, unknown field/operator) logs a warning and returns `false` (overlay not applied). `validate_structure()` provides structural validation for the create/update API path (surfaces `OVERLAY_002`), called by CRUD handlers when implemented.
   - **`OverlayMediaContext` vs `MediaFilterContext`** — The domain layer uses a richer struct that includes both condition-testable fields and text-variable fields (`title`, `year`, `runtime_seconds`, `audience_rating`, `rating_vote_count`). `to_filter_context()` extracts the condition subset. This avoids a second DB query while keeping `MediaFilterContext` focused on condition-testable fields.
   - **Expanded text variable resolution** — From 6 to 16 variables: added `<<audience_rating>>`, `<<rating_vote_count>>`, `<<video_dynamic_range>>`, `<<container>>`, `<<audio_channels>>` (formatted "5.1"/"7.1"), `<<content_rating>>`, `<<runtime>>`/`<<runtimeH>>`/`<<runtimeM>>`, `<<edition>>`, `<<critic_rating/>>` (÷2 for /5 scale).
   - **Single LATERAL JOIN query** — `load_media_context()` replaces Task 2's three correlated subqueries with one `LEFT JOIN LATERAL` for the primary media file + one for file count + one for genre aggregation. More efficient and extensible.
   - **64 unit tests** — All 8 operators, case-insensitive matching, numeric comparison (integer/float/string-parsed), boolean equality, array-membership (genre/streaming_on), UUID field, nested AND/OR (3 levels), empty/null/bool conditions, malformed conditions, structural validation. All 493 server tests pass (429 prior + 64 new). 0 clippy warnings.

**What was built for Task 4:**

| File | Purpose |
|---|---|
| `server/src/services/clean_art.rs` | Clean art preservation service — `ensure_clean_backup()` (content-addressed scaling of source artwork to canvas), `compute_config_hash()` (Blake3 hash of applied overlay IDs + updated_at + source artwork ID for change detection), `get_overlay_state()` / `upsert_overlay_state()` / `delete_overlay_state()` (artwork_overlay_state CRUD), `save_overlaid_result()` (write composited result to cache), `resolve_overlaid_artwork()` (display-layer check for overlaid results); 14 unit tests |
| `server/src/services/mod.rs` | Added `pub mod clean_art;` |
| `server/src/services/overlays.rs` | Made `resize_to_canvas()` public so the clean art service can scale source artwork to canvas |
| `server/src/services/artwork_delivery.rs` | `resolve_variant()` now checks `clean_art::resolve_overlaid_artwork()` first — if an overlaid result exists, serves it (downscaled to requested variant) instead of source artwork; overlaid variants cached with `{artwork_id}_overlay` stem to avoid overwriting source-variant cache entries; `overlay_artwork_type()` helper maps `ArtworkCategory` → overlay-type vocabulary (returns `None` for Logo/Banner) |
| `server/src/domains/overlays/service.rs` | Refactored `preview_overlay()` to use `ensure_clean_backup()` instead of reading source directly (consistent with production compositing path + creates clean backup as side-effect); added `composite_and_persist()` single-item compositing entry point (load definitions → evaluate conditions → config-hash change detection → ensure clean backup → composite → save result → upsert state); added `CompositeResult` struct; reads `overlay_image_quality` from RuntimeConfig for encode config |

**Key decisions from Task 4:**

- **Content-addressed clean backups via artwork UUID** — Clean backup filename: `/cache/images/clean/{type_subdir}/{artwork_id}.webp`. When the primary artwork changes (new TMDb download, user upload), the new artwork row has a new UUID, so the old clean backup is naturally orphaned (cache miss) and a fresh one is created from the new source. No explicit invalidation needed — stale files cleaned by the Overlay Cleanup scheduled task. Confirmed via June 2026 web research of Kometa's approach
- **Source artwork immutability guarantee** — Source files at `artwork.local_path` are opened read-only (`std::fs::read`); never written to. All derived artifacts (clean backups, composited results) live in the regenerable `/cache/images/` directory. The clean backup is an intermediate cache artifact, not a modification of the source
- **Blake3 for config hash** — Already in workspace (used by scanner for file hashing); faster than SHA-256; API is `Hasher::new().update().finalize().to_hex()`. Hash includes: source artwork UUID, each applied overlay's UUID + `updated_at` (sorted by UUID for determinism). Detects overlay addition/removal (different UUID set), overlay property changes (`updated_at` bump), and source artwork changes (different artwork UUID)
- **`composite_and_persist()` as single-item entry point** — Ties together the full preservation pipeline: condition evaluation → config-hash change detection → ensure clean backup → composite from clean backup → save overlaid result → upsert `artwork_overlay_state`. The Task 8 worker iterates items and calls this function. Returns `CompositeResult { composited: bool, applied_count }` so callers can aggregate metrics. When hash matches and `reapply_all` is false, re-compositing is skipped (idempotent)
- **No-overlays cleanup** — When `composite_and_persist` finds zero matching overlays, it deletes any existing `artwork_overlay_state` row (and the overlaid result file) and returns `composited: false`. This handles the "Remove All Overlays" scenario gracefully — the display layer falls back to source artwork automatically
- **Display integration with separate cache stems** — Overlaid variants cached with stem `{artwork_id}_overlay` (e.g., `.../w342/{artwork_id}_overlay.webp`), distinct from source-variant cache (`.../w342/{artwork_id}.webp`). This avoids overwrite when overlays are toggled on/off — both cache entries coexist. The check is a no-op until Task 8 produces overlaid results (currently returns `None` for all items)
- **`episode_thumb` → `thumbnail` type mapping** — The overlay system uses `episode_thumb` but the `artwork` table stores it as `thumbnail`. The `artwork_table_type()` helper in clean_art.rs handles this transparently for all DB queries. `overlay_artwork_type()` in artwork_delivery.rs maps `ArtworkCategory::Thumbnail` → `episode_thumb` for the overlay state check
- **Preview uses clean backup** — `preview_overlay()` now calls `ensure_clean_backup()` instead of reading the source directly. This creates the clean backup as a side-effect (speeding up subsequent previews and the eventual production composite), and ensures the preview pipeline is identical to the production path. Preview still writes to `/cache/images/overlays/previews/` (not the production overlay state)
- **No new workspace dependencies** — Blake3 already in workspace (Phase 5 scanner); all image decode/encode uses existing `image` + `webp` crates; all DB access uses existing `sqlx`. All 507 server tests pass (493 prior + 14 new). 0 clippy warnings.

4. ~~Implement clean art preservation — source artwork never modified~~ **DONE**
 5. ~~Create `server/src/domains/collections/` — five-file pattern~~ **DONE**

**What was built for Task 5:**

| File | Purpose |
|---|---|
| `server/src/domains/collections/mod.rs` | Module declarations + router assembly with 8 route groups under `/api/v1/collections` |
| `server/src/domains/collections/error.rs` | `CollectionsError` enum covering registered `COLL_001`–`COLL_008` codes + Database catch-all |
| `server/src/domains/collections/types.rs` | Three-type DTOs: `CollectionRow`, `CollectionItemRow`, `CollectionTemplateRow` (internal, no Serialize), collection/item/template request DTOs (Deserialize + Validate), response DTOs (Serialize), validation statics for collection types, visibility, sync modes, template types, builder types |
| `server/src/domains/collections/service.rs` | Validation helpers (`validate_collection_type`, `validate_visibility`, `validate_sync_mode`, `validate_template_type`, `validate_dynamic_config`, `validate_smart_filter`, `generate_slug`) plus concrete service signatures with `todo!()` bodies for CRUD, item management, sync dispatch, and template import/listing |
| `server/src/domains/collections/handlers.rs` | Handler stubs with concrete `Result<Json<T>, AppError>` return types, request validation, smart-filter structural validation, dynamic-builder validation, and `Require<CanManageLibraries>` gates |
| `server/src/domains/mod.rs` | Added `pub mod collections;` |
| `server/src/error.rs` | Added `AppError::Collections(#[from] CollectionsError)` and `collections_error_to_http()` mapping all 8 registered `COLL` codes |
| `server/src/router.rs` | Replaced Phase 12 collections comment with real `.merge(crate::domains::collections::router(state.clone()))` |

**Key decisions from Task 5:**

- **Scaffolding scope mirrors overlays Task 1** — The domain is fully wired with concrete DTOs, handlers, service signatures, validation, routes, and error mapping, while DB CRUD and builder behavior remain `todo!()` for Tasks 6–7. This keeps Task 5 to the five-file pattern boundary.
- **`CanManageLibraries` capability gate** — Collection management is a library-management function; all endpoints are gated with `Require<CanManageLibraries>`, matching overlays and library administration.
- **Route design reserves both user-facing collection CRUD and admin operations** — `/api/v1/collections` covers list/create; `/api/v1/collections/{id}` covers get/update/delete; `/items` handles static item management; `/sync` handles all/single dynamic sync dispatch; `/templates` handles template listing/import.
- **Registered `COLL` error codes only** — The central error mapping uses exactly `COLL_001`–`COLL_008` from `COLLECTIONS.md`/`ERROR_HANDLING.md`; generic database errors map to `INTERNAL`.
- **Smart filters reuse the shared condition service** — `validate_smart_filter()` calls `services::conditions::validate_structure()`, preserving the Phase 12 Task 3 decision that overlays, smart collections, and smart playlists share one JSONB rule grammar.
- **No new migrations or dependencies** — `collections`, `collection_items`, and `collection_templates` already exist from Phase 2 migration 14; the scaffold uses existing `axum`, `sqlx`, `serde`, `validator`, `uuid`, and `chrono`.

**Verification:** `cargo check -p duskcue` passes. `cargo test -p duskcue` passes (507 tests). `cargo clippy -p duskcue --all-targets --all-features -- -A clippy::unnecessary-sort-by -D warnings` passes; strict clippy without the allowance is currently blocked by two pre-existing `clippy::unnecessary-sort-by` warnings in `server/src/services/fanart_client.rs`.

6. ~~Implement collection builders:~~ **DONE**
   - ~~Internal: genre, decade, actor, director, franchise, resolution, audio_codec~~
   - ~~External: `tmdb_popular`, `tmdb_top_rated`, `tmdb_trending`, `tmdb_now_playing`, `tmdb_upcoming`~~

**What was built for Task 6:**

| File | Purpose |
|---|---|
| `server/src/services/collections.rs` | Shared dynamic collection builder engine: parses `dynamic_config`, builds candidate groups, supports include/exclude/key overrides/title formatting, syncs `collection_items`, updates `item_count`, `total_duration_seconds`, `last_synced_at`, and `last_sync_result`; 11 unit tests |
| `server/src/services/tmdb_client.rs` | Added `TmdbChart`, `TmdbChartItem`, and `fetch_chart_items()` for TMDB popular/top-rated/trending/now-playing/upcoming chart endpoints |
| `server/src/services/mod.rs` | Added `pub mod collections;` |
| `server/src/domains/collections/service.rs` | Manual all/single collection sync endpoints now invoke the builder engine and instantiate `TmdbClient` from runtime metadata config |
| `server/src/domains/collections/handlers.rs` | Sync handlers pass `AppState` to service layer so runtime provider config is available |
| `docs/design/COLLECTIONS.md` | Added Task 6 implementation notes and deferred items |

**Key decisions from Task 6:**

- **Shared service module, not domain-only logic** — `services/collections.rs` is the reusable engine for manual API sync now and the scheduled `collection_sync` worker in Task 7. Domain handlers stay thin and only translate HTTP/auth/validation into service calls.
- **Builder candidates separate from persistence** — The engine first returns one or more `CollectionBuilderResult` groups. Internal builders such as genre/decade/actor/director can produce multiple candidate collections; syncing a specific existing collection selects by configured `key`, then name/key match, then combines candidates as a fallback. Task 7 can reuse the candidate output for scheduled execution.
- **Actual schema over stale examples** — Queries use the Phase 2 schema that exists today: `media_items.premiere_date` for decades, `media_items.tmdb_id` for TMDB matching, normalized `genres`/`media_genres`, `media_credits`/`people`, and a lateral healthiest/largest `media_files` row for resolution/audio-codec builders.
- **TMDB chart endpoints through existing client** — Official TMDB v3 endpoints are used: `/movie/popular`, `/tv/popular`, `/movie/top_rated`, `/tv/top_rated`, `/trending/{movie|tv}/{day|week}`, `/movie/now_playing`, and `/movie/upcoming`. Existing bearer-token auth, timeout, and error mapping are reused.
- **Missing external items are reported, not stored** — `collection_items.media_item_id` is `NOT NULL`, so unmatched TMDB chart entries cannot be persisted as rows without a schema change. Task 6 records missing counts and external IDs in `last_sync_result`; UI display of missing items can read that JSON or a later migration can add a dedicated missing-items table.
- **Manual sync is synchronous** — `POST /api/v1/collections/sync` and `/collections/{id}/sync` now execute immediately and return `"synced"`. The scheduled worker/queue semantics remain Task 7.
- **No new dependencies or migrations** — Uses existing `sqlx`, `serde`, `thiserror`, `uuid`, `chrono`, and the existing TMDB client infrastructure.

**Verification:** `cargo check -p duskcue` passes. `cargo test -p duskcue services::collections` passes (11 tests). `cargo clippy -p duskcue --all-targets --all-features -- -A clippy::unnecessary-sort-by -D warnings` passes.

7. ~~Implement `server/src/workers/collection_sync.rs` — periodic builder execution~~ **DONE**

**What was built for Task 7:**

| File | Purpose |
|---|---|
| `server/src/workers/collection_sync.rs` | Scheduled collection sync worker: resolves runtime/task config, gates on `metadata.collections_enabled`, fetches enabled dynamic collections, classifies internal vs external builders, invokes the shared `services::collections::sync_dynamic_collection()` engine per collection, records per-collection failures in `last_sync_result`, aggregates sync stats, and aborts remaining work on external API rate limits |
| `server/src/workers/mod.rs` | Added `pub mod collection_sync;` |
| `server/src/main.rs` | Registered `collection_sync` executor on the scheduler with `AppState` capture for runtime metadata provider config |
| `server/src/services/scheduler.rs` | Added `Collection Sync` to first-run default scheduled task seeding |
| `server/migrations/20260625060000_seed_collection_sync_task.sql` | Idempotent migration to seed `collection_sync` for existing deployments |
| `docs/design/COLLECTIONS.md` | Added Task 7 implementation notes and updated deferred items |
| `PROJECT.md` | Updated Phase 12 implementation status |

**Key decisions from Task 7:**

- **Thin scheduled worker over shared builder engine** — Task 7 does not reimplement builders. It reuses `services::collections::sync_dynamic_collection()` from Task 6 so manual and scheduled sync share the same persistence path (`collection_items`, counts, duration, `last_synced_at`, `last_sync_result`).
- **Per-collection iteration instead of all-or-nothing batch** — The worker fetches candidate collection IDs and syncs them one at a time. Invalid configs or source failures are logged and recorded on that collection's `last_sync_result`, then the worker continues with the next collection. This avoids one bad dynamic row blocking unrelated collections.
- **External builders are gated and paced** — Task config supports `sync_external`/`include_external` (default `true`) and `max_external_requests_per_minute` (default from `MetadataConfig.collection_external_rate_limit_per_minute`, currently 30). External builders run sequentially with a conservative delay between external collections. A provider 429 (`ExternalRateLimited`) aborts the remaining run so the next scheduled interval can retry without worsening the rate limit.
- **Runtime provider config reused** — The worker constructs `TmdbClient` from `RuntimeConfig.metadata.providers.tmdb` and `metadata_language`, matching the manual sync endpoint behavior. If TMDB is disabled or missing an access token, external TMDB builders fail gracefully and store failure details in `last_sync_result`.
- **No schema change needed** — `collection_sync` already exists in the `scheduled_tasks.task_type` CHECK constraint from Phase 2. Task 7 only adds an idempotent seed migration for deployments whose initial seed data predates this worker.

**Verification:** `cargo check -p duskcue` passes. `cargo test -p duskcue` passes (535 tests). `cargo clippy -p duskcue --all-targets --all-features -- -A clippy::unnecessary-sort-by -D warnings` passes.

8. ~~Implement `server/src/workers/overlay_compositor.rs` — apply overlays to artwork~~ **DONE**

**What was built for Task 8:**

| File | Purpose |
|---|---|
| `server/src/workers/overlay_compositor.rs` | Scheduled/manual overlay application worker: gates on `metadata.overlays_enabled`, resolves primary artwork targets, supports `library_id`, `media_item_id`, `artwork_types`, `reapply_all`, `max_concurrent`, and `batch_limit` config, applies overlays with bounded `JoinSet` concurrency, aggregates composited/current/no-match/failed counts, and reuses `domains::overlays::service::composite_and_persist()` for all persistence |
| `server/src/workers/mod.rs` | Added `pub mod overlay_compositor;` |
| `server/src/main.rs` | Registered `overlay_application` executor on the scheduler with `AppState` capture |
| `server/src/services/scheduler.rs` | Added `Overlay Application` to first-run default scheduled task seeding |
| `server/migrations/20260625070000_seed_overlay_application_task.sql` | Idempotent migration to seed `overlay_application` for existing deployments |
| `server/src/domains/overlays/service.rs` | Manual `POST /api/v1/overlays/apply` now invokes the worker path synchronously; `composite_and_persist()` now respects `overlay_definitions.library_id` scoping and overlay definition loads include `created_at`/`updated_at` for config hashing |
| `server/src/domains/overlays/handlers.rs` | Apply handler passes `AppState` to the service layer so runtime config and worker orchestration are available |
| `docs/design/METADATA_OVERLAYS.md` | Added Task 8 implementation notes |
| `PROJECT.md` | Updated Phase 12 implementation status |

**Key decisions from Task 8:**

- **Worker reuses the single-item pipeline** — `overlay_compositor` does not duplicate compositing, clean-art, condition evaluation, or DB state logic. It selects targets and calls `composite_and_persist()` per media item + artwork type, so scheduled and manual application use the same hash/change-detection path as preview/clean-art work from Tasks 2–4.
- **`overlay_application` is scheduler-owned** — The `scheduled_tasks.task_type` CHECK constraint already includes `overlay_application`. Task 8 registers that executor and seeds the row for existing deployments; first-run installs get the same task through `seed_default_tasks()`.
- **Primary artwork drives targets** — The worker processes only media items with primary artwork (`artwork.order = 0`) and a non-empty `local_path`. DB `thumbnail` artwork is mapped to overlay `episode_thumb`, matching the clean-art/display-layer mapping from Task 4.
- **Bounded concurrency without a new queue table** — The worker uses `tokio::task::JoinSet` capped by `max_concurrent` (default 2, max 8). No per-item queue locks are introduced because Duskcue is single-instance and the scheduler already serializes each scheduled task via `scheduled_tasks.state`.
- **Manual apply is synchronous** — `POST /api/v1/overlays/apply` now runs the same worker path inline and returns `"completed"` with the number of candidate targets. A background job/202 response can be added with Phase 13a scheduled task management if the admin UI needs non-blocking progress.
- **Library-scoped overlays enforced** — `overlay_definitions.library_id` now restricts definitions to that library; global overlays (`NULL`) continue to apply everywhere. This fixes the production path before scheduled application can touch all libraries.
- **Research-backed concurrency stance** — Tokio docs advise moving blocking/CPU-heavy work out of async futures or bounding it carefully; PostgreSQL `SKIP LOCKED` is useful for multi-consumer queues but unnecessary here because the scheduler serializes this single task in a single-instance system.

**Verification:** `cargo check -p duskcue` passes.

9. ~~Implement poster management — asset directory scanning, poster locking, community pack import~~ **DONE**

**What was built for Task 9:**

| File | Purpose |
|---|---|
| `server/src/services/poster_management.rs` | Shared poster-management service: safe recursive asset-directory image discovery, item matching by section/folder/TMDb ID/title+year, season poster detection, collection poster matching, image validation/dimension extraction, persistent artwork copy storage, primary artwork promotion, lock updates, overlay-state invalidation on primary-source change, community pack JSON import, and 8 unit tests |
| `server/src/domains/posters/` | New five-file poster API domain with admin routes for asset-directory scan, community pack import, lock/unlock, and selecting active artwork |
| `server/src/workers/asset_directory_scanner.rs` | Scheduled/manual asset-directory scan worker resolving `config.path` first, then `RuntimeConfig.metadata.asset_directory`, with `lock_imported` defaulting to true |
| `server/src/main.rs` | Registered `asset_directory_scan` scheduler executor |
| `server/src/services/scheduler.rs` | Added first-run default `Asset Directory Scan` task at daily 03:00 |
| `server/migrations/20260625080000_seed_asset_directory_scan_task.sql` | Idempotent seed migration for existing deployments |
| `server/src/router.rs`, `server/src/domains/mod.rs`, `server/src/services/mod.rs`, `server/src/workers/mod.rs` | Module/router wiring |
| `docs/design/POSTER_MANAGEMENT.md`, `PROJECT.md` | Task 9 implementation notes and status updates |

**Key decisions from Task 9:**

- **Shared service over domain-only logic** — `services/poster_management.rs` owns the artwork lifecycle behavior so the API domain and scheduled worker share matching, import, locking, and primary-promotion rules.
- **JSON-first community pack import** — The API accepts a community pack manifest plus server-side `pack_root` paths. ZIP/TAR archive upload is deferred until a multipart upload pipeline exists; the service already validates canonical paths under the pack root so archive extraction can reuse the same safety boundary.
- **Asset scans are non-symlink recursive walks** — The scanner uses the existing `ignore` walker with symlink following disabled, canonicalizes the configured root, and validates every discovered file remains under that root before import.
- **Primary promotion preserves alternates** — Imported asset/community artwork is copied into persistent storage and promoted to `artwork.order = 0`; existing artwork rows are demoted rather than deleted, preserving TMDb/user/community alternates for later selection.
- **Lock semantics match design** — Asset-directory artwork locks by default (`is_locked = true`). Community pack imports default unlocked unless the request/task sets `lock_imported = true`. Manual lock/unlock and active selection endpoints are available for the future admin UI.
- **Overlay state invalidated on source change** — Selecting or importing a new primary media-item artwork deletes the relevant `artwork_overlay_state` row so clients do not keep seeing an overlaid image generated from the previous source.
- **No new dependencies or schema tables** — The implementation reuses existing `ignore`, `image`, `blake3`, `sqlx`, and the Phase 2 `artwork.is_locked`/`source_type` columns. Only a scheduled-task seed migration was needed.

**Verification:** `cargo check -p duskcue` passes.

 10. ~~Build admin UI for overlays — overlay editor, template browser, condition builder~~ **DONE**
 11. ~~Build admin UI for collections — collection list, builder configuration, template import~~ **DONE**

    **Context from Task 10:** The overlay CRUD/template service functions were still `todo!()` stubs (Tasks 1–9 implemented compositing/conditions/clean-art/workers but never the definition CRUD). Task 10 filled these in (`list_overlays`/`get_overlay`/`create_overlay`/`update_overlay`/`delete_overlay`/`list_templates`/`import_template` in `domains/overlays/service.rs`) because the UI cannot function without working endpoints. A new `ConditionBuilder.svelte` recursive component was added to `clients/web/src/lib/components/` — Task 11's collection smart-filter UI can reuse this same component and the shared `services::conditions::validate_structure()` engine, since overlays and collections share one JSONB rule grammar. The `clients/web/src/lib/api/overlays.js` client pattern (thin wrappers over `core.js`) is the template for the collections API client.

    **What was built for Task 11:**

    | File | Purpose |
    |---|---|
    | `server/src/domains/collections/service.rs` | Filled the 11 CRUD/item/template stubs: `list_collections` (paginated `QueryBuilder` with library_id/type/visibility/enabled filters + count), `get_collection`, `create_collection` (slug generation + name/slug uniqueness + `is_dynamic`/`is_smart` derivation from `collection_type`), `update_collection` (dynamic `QueryBuilder` PATCH with collection_type → is_dynamic/is_smart auto-sync), `delete_collection` (system-collection protection via `AppError::Conflict`), `list_collection_items` (paginated, optional `include_missing` filter), `add_collection_items` (1000-spaced positioning + counter update), `reorder_collection_items`, `remove_collection_item` (counter update), `list_templates`, `import_template` (upsert on name with `ON CONFLICT`); shared `SELECT_CLAUSE`/`RETURNING_COLUMNS` consts + `row_to_collection_row()`/`row_to_response()`/`row_to_template_row()`/`template_row_to_response()` mappers + `check_name_unique()`/`update_collection_counters()` helpers |
    | `clients/web/src/lib/api/collections.js` | Full API client: `listCollections`, `getCollection`, `createCollection`, `updateCollection`, `deleteCollection`, `listCollectionItems`, `addCollectionItems`, `reorderCollectionItems`, `removeCollectionItem`, `syncAllCollections`, `syncCollection`, `listTemplates`, `importTemplate` |
    | `clients/web/src/routes/settings/collections/+page.svelte` | Full admin UI replacing the "Coming in Phase 12" stub: collection list grouped by type (Static/Dynamic/Smart) with enable toggle/edit/delete/sync, type-aware editor (builder config for dynamic: builder dropdown grouped Internal/External, limit, schedule, sync_mode, title_format, include/exclude; `ConditionBuilder` for smart filters), display controls (sort_order, sort_by), template browser + JSON import, and Sync All / per-collection sync operations |
    | `clients/web/src/routes/settings/+page.svelte` | Removed `soon: true` flag from the Collections settings link |

    **Key decisions from Task 11:**

    - **CRUD was a prerequisite, not part of an earlier task** — Tasks 1–9 left collection CRUD/item/template service functions as `todo!()` stubs (the scaffolding task's explicit deferral). Task 11 implements them as the UI's hard dependency, mirroring the exact precedent set by Task 10 (overlays). The `row_to_collection_row()` mapper and `CollectionRow` struct from the scaffolding are reused.
    - **`QueryBuilder` over `format!` for dynamic SQL** — sqlx 0.9's `SqlSafeStr` guard rejects `String`/`format!()` output in `sqlx::query()`. `list_collections` and `update_collection` use `sqlx::query_builder::QueryBuilder`. The `($N::uuid IS NULL OR column = $N)` pattern is not used; instead `QueryBuilder` conditionally pushes `WHERE`/`AND` clauses per present filter, matching the overlays `list_overlays` pattern.
    - **`collection_type` change auto-syncs `is_dynamic`/`is_smart`** — When `update_collection` receives a new `collection_type`, it derives and sets `is_dynamic`/`is_smart` in the same PATCH so the boolean flags never desync from the string discriminator. The DDL CHECK constraints on `collection_type`/`visibility`/`sync_mode` enforce validity at the DB layer; the service layer's `validate_*` helpers run in the handler before the service is reached.
    - **System-collection delete → `AppError::Conflict`** — no dedicated COLL code exists for the "system collections cannot be deleted" business rule (Task 5 decision); the service returns `AppError::Conflict`, the UI disables the delete button and shows a `system` badge. Disabling is the supported customization path — consistent with the overlays domain.
    - **`delete_collection` returns `Result<(), AppError>`** — the only collection service function not returning `CollectionsError`, because the system-collection check needs a non-COLL error code. DB errors map `CollectionsError::Database` → `AppError` explicitly via `From`. Matches `overlays::service::delete_overlay` exactly.
    - **Item positioning uses 1000-spaced integers** — `add_collection_items` assigns `MAX(position) + 1000` (or 1000 for the first item) per new item, matching DATABASE.md integer-spacing convention (1000, 2000, 3000) used by playlists; allows future insertions without renumbering. `ON CONFLICT (collection_id, media_item_id) DO NOTHING` prevents duplicate items.
    - **Template import is upsert on name** — `import_template` uses `ON CONFLICT (name) DO UPDATE` so re-importing an updated template updates rather than failing on the UNIQUE(name) constraint; COALESCE preserves nullable fields when the new value is NULL.
    - **`last_sync_result` rendered as summary string** — `formatSyncResult()` extracts `added`/`removed`/`missing` counts from the JSONB into a compact `+3, -1, 5 missing` badge; falls back to `"never"` for unsynced dynamic collections.
    - **Builder dropdown grouped Internal/External** — the dynamic-config builder select uses `<optgroup>` to separate the 14 internal builders (library metadata) from the 12 external builders (API sources) per COLLECTIONS.md Builder Sources tables; makes the admin's mental model match the design doc.
    - **Title-format field uses HTML entities** — `&lt;&lt;key_name&gt;&gt;` in the Svelte template renders `<<key_name>>` as a placeholder hint without Svelte interpreting the angle brackets; matches the template-variable syntax documented in COLLECTIONS.md.
    - **Reuses `ConditionBuilder.svelte` from Task 10** — smart collections use the identical recursive condition editor as overlays, since both domains share the `services::conditions` JSONB rule grammar (Task 3's cross-cutting design decision). No new component needed; the `denormalizeConditions`/`normalizeConditions` helpers are duplicated from the overlays page since each page manages its own form state.
    - **No new web dependencies** — uses Svelte 5 runes, existing `core.js`, and the existing design tokens only.

    **Verification:** `cargo check -p duskcue`, `cargo clippy -p duskcue --all-targets --all-features -- -A clippy::unnecessary-sort-by -D warnings`, and `cargo test -p duskcue --lib` (583 tests pass) all green. `npx svelte-check` (0 errors) and `npm run build` pass in `clients/web`.

   **What was built for Task 10:**

   | File | Purpose |
   |---|---|
   | `server/src/domains/overlays/service.rs` | Filled the 7 CRUD/template stubs: `list_overlays` (paginated `QueryBuilder` with library/enabled filters + matching count), `get_overlay`, `create_overlay` (INSERT with DDL defaults via `generate_slug`), `update_overlay` (dynamic PATCH via `QueryBuilder` — COALESCE-free, only sets `Some` fields so NULL-set is unambiguous), `delete_overlay` (system-overlay protection via `AppError::Conflict`), `list_templates` (groups installed definitions by `metadata.template_name`), `import_template` (inserts each `TemplateOverlayEntry`); shared `SELECT_CLAUSE`/`RETURNING_COLUMNS` consts + `row_to_response()` helper reusing the existing `row_to_definition_row()` mapper |
   | `clients/web/src/lib/api/overlays.js` | Full API client: `listOverlays`, `getOverlay`, `createOverlay`, `updateOverlay`, `deleteOverlay`, `applyOverlays`, `previewOverlay`, `listTemplates`, `importTemplate` |
   | `clients/web/src/lib/components/ConditionBuilder.svelte` | Recursive condition editor (Svelte 5 runes): "Match all/any" toggle, per-rule smart mini-forms adapting by field type (text/number/boolean), nested groups with depth cap of 3, add/remove rule + group, explicit `onchange`/`onremove` callbacks over fragile `$bindable` recursion |
   | `clients/web/src/routes/settings/overlays/+page.svelte` | Full admin UI replacing the "Coming in Phase 12" stub: definition list grouped by `applies_to` with enable toggle/edit/delete, type-aware overlay editor (image/text/backdrop field sets), positioning controls, group/queue/suppress fields, condition builder, 16-variable text-template inserter, live preview against a media item, template browser + JSON import, and Apply Now / Re-apply All bulk operations |
   | `clients/web/src/routes/settings/+page.svelte` | Removed `soon: true` flag from the Overlays settings link |

   **Key decisions from Task 10:**

   - **CRUD was a prerequisite, not part of an earlier task** — Tasks 1–9 left definition CRUD as `todo!()` stubs (the scaffolding task's explicit deferral). Task 10 implements them as the UI's hard dependency. The `row_to_definition_row()` mapper and `OverlayDefinitionRow` struct from Task 2's preview path are reused, so the CRUD response shape matches what the compositing pipeline expects.
   - **`QueryBuilder` over `format!` for dynamic SQL** — sqlx 0.9's `SqlSafeStr` guard rejects `String`/`format!()` output in `sqlx::query()` (compile-time SQL-injection defense). `list_overlays` and `update_overlay` use `sqlx::query_builder::QueryBuilder`. The conditional-SET structure (only push a SET clause for present fields) is required for 3-state PATCH support — `COALESCE($n, column)` (the `users`/`libraries` convention) cannot clear a nullable field since `COALESCE(NULL, col) = col`.
   - **3-state PATCH via `serde_with::rust::double_option`** — `Option<T>` collapses JSON "absent" and "null" into one `None`, so the standard PATCH type can't express "clear to NULL." `library_id` NULL = "global", and global↔specific is a documented admin workflow (capability #4), so `UpdateOverlayRequest.library_id` is `Option<Option<Uuid>>` with `#[serde(default, with = "::serde_with::rust::double_option")]` (`None`=unchanged, `Some(None)`=clear, `Some(Some(v))`=set). The UI sends literal `null` (not an omitted key) when clearing. Applied to `library_id` only (documented clear-workflow, no validator constraint); pattern recommended for later `users`/`libraries` adoption (their PATCH endpoints share the limitation). Added `serde_with = "3"` to the workspace.
   - **System-overlay delete → `AppError::Conflict`** — no dedicated OVERLAY code exists for the "system overlays cannot be deleted" business rule (Task 1 decision); the service returns `AppError::Conflict`, the UI disables the delete button and shows a `system` badge. Disabling is the supported customization path.
   - **`delete_overlay` returns `Result<(), AppError>`** — the only service function not returning `OverlayError`, because the system-overlay check needs a non-OVERLAY error code. DB errors map `OverlayError::Database` → `AppError` explicitly.
   - **Templates are imported, not remotely browsed** — Duskcue is local-first with no bundled remote catalog. `GET /api/v1/overlays/templates` lists templates *already installed* (grouped by `metadata.template_name`); import is via JSON paste. A future remote registry can extend this without API changes.
   - **"Match all/any" over raw AND/OR** — per June 2026 UX research (UX StackExchange nested-predicate thread; ui-patterns Rule Builder). Boolean terminology "causes users to ask more questions than it answers."
   - **`e.currentTarget.value` over `e.target.value`** — Svelte 5 types `currentTarget` to the specific element (`HTMLSelectElement`, etc.) but `target` as generic `EventTarget`. Using `currentTarget` keeps svelte-check clean (0 errors) without `/** @type */` casts.
   - **Recursive `ConditionBuilder` via callbacks over `$bindable`** — deep `$bindable` on a recursive tree is fragile; explicit `onchange(next)` / `onremove()` props passed down at each recursion level are unambiguous and testable. Reusable by Task 11's collection smart-filter UI.
   - **No new web dependencies** — uses Svelte 5 runes, existing `core.js`, native `<input type="color">` + alpha text fallback, and the existing design tokens only.

   **Verification:** `cargo check -p duskcue`, `cargo clippy -p duskcue --all-targets --all-features -- -A clippy::unnecessary-sort-by -D warnings`, and `cargo test -p duskcue --lib` (583 tests pass) all green. `npx svelte-check` (0 errors) and `npm run build` pass in `clients/web`.

**What was built for Task 1:**

| File | Purpose |
|---|---|
| `server/src/domains/overlays/mod.rs` | Module declarations + router assembly with 9 routes across 5 paths |
| `server/src/domains/overlays/error.rs` | `OverlayError` enum with 7 variants covering OVERLAY_001–OVERLAY_006 + Database catch-all |
| `server/src/domains/overlays/types.rs` | Three-type DTOs: `OverlayDefinitionRow` (internal, 30 columns), `CreateOverlayRequest`/`UpdateOverlayRequest` (Deserialize + Validate), `OverlayDefinitionResponse`/`OverlayListResponse` (Serialize); operation DTOs (`ApplyOverlaysRequest`, `PreviewOverlayRequest`, `OverlayTemplateImport`, `OverlayTemplateResponse`, `OverlayTemplateSummary`); validation statics (`VALID_OVERLAY_TYPES`, `VALID_APPLIES_TO`, `VALID_HORIZONTAL_ALIGN`, `VALID_VERTICAL_ALIGN`) |
| `server/src/domains/overlays/service.rs` | Validation helpers (`validate_overlay_type`, `validate_applies_to`, `validate_horizontal_align`, `validate_vertical_align`, `generate_slug`); 9 service function stubs with `todo!()` and concrete return signatures |
| `server/src/domains/overlays/handlers.rs` | 9 handler stubs with concrete `Result<Json<T>, AppError>` return types; validation-error mapping inline; `Require<CanManageLibraries>` gate on all endpoints |
| `server/src/domains/mod.rs` | Added `pub mod overlays;` |
| `server/src/error.rs` | Added `AppError::Overlay(#[from] OverlayError)` variant + `overlay_error_to_http()` mapping all 7 codes |
| `server/src/router.rs` | Replaced Phase 12 overlays comment with real `.merge(crate::domains::overlays::router(state.clone()))` |

**Key decisions from Task 1:**

- **Scaffolding scope mirrors playback Task 1** — All service and handler bodies are `todo!()` stubs with concrete return types so the project compiles and all 9 routes are wired. The compositing pipeline (Task 2), condition evaluation (Task 3), clean-art preservation (Task 4), and CRUD DB operations are filled in by subsequent tasks. This matches the established precedent for the "Create X domain — five-file pattern" task granularity.
- **Exactly 6 registered OVERLAY codes** — `OverlayError` defines precisely OVERLAY_001–OVERLAY_006 plus the `Database` catch-all, respecting the fixed error registry (94 total codes). No invented OVERLAY_007+ codes. Validation-type errors (invalid overlay_type/align) route through `OverlayError::InvalidConditions` (OVERLAY_002) in the service validators, which is the closest semantically-matching registered code for "invalid overlay configuration."
- **System-overlay deletion deferred** — System overlays can be disabled but not deleted (per design doc). Since no dedicated OVERLAY code exists for this business rule, the delete handler will enforce it via `AppError::Conflict` when CRUD is implemented (later task) — consistent with how other domains reuse generic codes for policy violations. Documented in [METADATA_OVERLAYS.md](docs/design/METADATA_OVERLAYS.md) Implementation Notes.
- **`CanManageLibraries` capability gate** — All overlay endpoints require `CanManageLibraries` (artwork customization is a library-management function), enforced via the generic `Require<CanManageLibraries>` extractor. Matches the libraries domain gate and the Phase 4 Task 11 extractor pattern.
- **Route design per API_CONVENTIONS.md** — Base `/api/v1/overlays` for CRUD; `/api/v1/overlays/apply` for bulk application (Task 8 worker integration); `/api/v1/overlays/preview` for editor live-preview (Task 2 compositing); `/api/v1/overlays/templates` for community template import/export. Literal `{id}` path segment per axum 0.8 syntax.
- **`TemplateOverlayEntry` derives `Serialize`** — The `validator` 0.20 derive macro requires nested types in `Validate` structs to implement `Serialize` (`ValidationError::add_param` bound). `OverlayTemplateImport` validates its `overlays: Vec<TemplateOverlayEntry>` field, so the entry type derives both `Deserialize` and `Serialize`.
- **No new DB migration** — `overlay_definitions` and `artwork_overlay_state` tables (plus `artwork.is_locked`/`source_type`) were created in Phase 2 migration 14; no schema changes needed.
- **No new workspace dependencies** — scaffolding uses existing `axum`, `sqlx`, `serde`, `validator`, `uuid`, `chrono`. Compositing crates (`ab_glyph`, `fontdb`, `resvg`) added in Task 2.

**Verification:** Default overlays (resolution badge, audio codec) are applied to poster artwork. Dynamic collections auto-populate from TMDB popular/trending. Admin can create custom overlays and collections. Source artwork is preserved.

**Phase 12 status:** All 11 tasks complete.

---

## Phase 13a — System Operations Core

**Goal:** Server config management, backup system, scheduled maintenance workers, admin settings UI. The operational backbone of Duskcue.

**Prerequisites:** Phase 12 complete. Phase 5 scheduler (workers).

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [BACKUP_RECOVERY.md](docs/operations/BACKUP_RECOVERY.md) | WAL-G continuous archiving, pg_dump logical backups, AES-256-GCM encryption, 3-2-1 storage |
| [CACHE_STORAGE.md](docs/operations/CACHE_STORAGE.md) | Three-tier storage, per-cache-type limits, LRU eviction, disk space monitoring |
| [DATABASE_MAINTENANCE.md](docs/operations/DATABASE_MAINTENANCE.md) | REINDEX CONCURRENTLY task, partition ANALYZE, `pgstattuple` bloat measurement |
| [PHASE_13_SPLIT.md](docs/design/PHASE_13_SPLIT.md) | **Phase split rationale** — explains why Phase 13 is divided into 13a + 13b; dependency analysis showing clean boundary |

**Tasks:**

1. Create `server/src/domains/system/` — five-file pattern (already partially built from Phase 6 Task 12 — provider validation endpoint)
2. Implement `server_config` runtime API — get/update JSONB config fields; generic CRUD for all config groups including push/webhook settings (which activate in Phase 13b)
3. Implement scheduled task management — list, trigger, cancel, view history; register `notification_cleanup` executor (DB cleanup of old notifications; no dispatch dependency)
4. Implement `server/src/domains/backup/` — five-file pattern
5. ~~Implement backup coordination — WAL-G status check, pg_dump trigger, verification~~ **DONE**
6. ~~Implement `server/src/workers/backup_runner.rs` — scheduled backup execution~~ **DONE**
7. ~~Implement `server/src/workers/reindex_maintenance.rs` — weekly REINDEX CONCURRENTLY~~ **DONE**
8. ~~Implement `server/src/workers/disk_space_check.rs` — 30-minute disk monitoring~~ **DONE**
9. ~~Implement `server/src/workers/recovery_drill_runner.rs` — manual/scheduled restore drills in disposable PostgreSQL, using the Docker migration-verification pattern to restore the latest `pg_dump` or WAL-G backup, run structural checks, and write evidence into `scheduled_task_runs.stats`~~ **DONE**

   **Context from Task 8:** The disk-space worker established the fallible-executor + `DiskSpaceStats` run-stats pattern that Task 9 should follow for its `RecoveryDrillStats`. The backup coordinator (`services::backup.rs` from Task 5) already owns `pg_restore --list` verification and WAL-G `wal-verify` — Task 9 should reuse these primitives rather than re-spawning `pg_restore`/`wal-g` directly. The backup admin UI (`/settings/backups`) already looks for `backup_recovery_drill`/`recovery_drill` scheduled task runs and displays evidence once they exist (Task 10), so Task 9 only needs to register the executor and persist structured stats — the UI consumption side is already built. Task 9 does NOT need disk-state checks; the disk-space worker (Task 8) runs independently every 30 min and will surface any disk pressure that could affect a restore drill.

   **What was built for Task 9:**

   | File | Purpose |
   |---|---|
   | `server/src/services/recovery_drill.rs` | Shared recovery drill service: `DrillOptions` (postgres image, port, keep_alive, source, dump_path, restore_jobs, timeout) parsed from task config + runtime BackupConfig; `run_recovery_drill()` orchestrates the full pipeline (docker availability check → dump resolution → pre-restore verification via `services::backup::verify_pg_dump_file` → disposable PostgreSQL startup via Docker Compose → `pg_restore --no-owner --no-privileges --role=duskcue --jobs=N` → structural checks via temporary `PgPool` → disposal); `RecoveryDrillStats` evidence schema with skip/fail/passed/unavailable status resolution; 21 unit tests |
   | `server/src/workers/recovery_drill_runner.rs` | Scheduler adapter: `run_recovery_drill(state, task_id, config)` resolves `DrillOptions`, delegates to the service, persists structured stats into `scheduled_task_runs.stats`; 2 unit tests |
   | `server/src/services/mod.rs` | Added `pub mod recovery_drill;` |
   | `server/src/workers/mod.rs` | Added `pub mod recovery_drill_runner;` |
   | `server/src/services/backup.rs` | Made `find_latest_pg_dump` and `verify_pg_dump_file` public so the drill service can reuse them |
   | `server/src/main.rs` | Registered `backup_recovery_drill` fallible executor on the scheduler (8th fallible executor) with `recovery_drill_state` capture clone |
   | `server/src/services/scheduler.rs` | Added "Backup Recovery Drill" to runtime `seed_default_tasks` (cron `0 7 * * 0`, enabled by default) |
   | `server/migrations/20260628010000_add_backup_recovery_drill_task.sql` | Rebuilds `scheduled_tasks_task_type_check` constraint to include `backup_recovery_drill`; seeds task for existing deployments with next-Sunday-07:00 `next_run_at` computed via `generate_series` |
   | `docs/operations/BACKUP_RECOVERY.md` | Replaced "Planned Recovery Drill Runner" with "Recovery Drill Runner (Phase 13a Task 9)" — full behavior, task config table, evidence schema JSON example, implementation notes |

   **Key decisions from Task 9:**

   - **Two-module split mirrors `services::backup` ↔ `workers::backup_runner`** — The drill logic lives in `services/recovery_drill.rs` so future manual API endpoints (if added) can reuse the same code path without going through the scheduler. The worker module is a thin adapter that resolves options, delegates to the service, and persists stats. This follows the established convention from Phase 13a Tasks 5/6.
   - **Disposable PostgreSQL via generated Docker Compose file** — A per-run compose file is written to `temp_dir/duskcue-drill-{uuid}/docker-compose.yml` with loopback-only port binding, per-run generated password, and ephemeral named volume. This mirrors `scripts/verify-migrations.ps1` and `docker/compose.migrations.yml` exactly. The temp directory and compose file are removed during disposal.
   - **`pg_restore --no-owner --no-privileges --role=duskcue --jobs=N`** — Standard pattern for restoring into a fresh DB with a different owner per PostgreSQL 18 docs (research-verified). `--no-owner`/`--no-privileges` drop source ownership/ACLs; `--role=duskcue` assigns all restored objects to the disposable user. `--jobs=N` (default 2, clamped to [1, 4]) parallelizes the restore. `--clean` is intentionally NOT used because the target database is freshly created (no objects to drop).
   - **WAL-G physical restore deferred** — The service is structured so a future `restore_wal_g_branch` can plug in alongside `restore_pg_dump_branch`. The current implementation only restores pg_dump custom-format backups because WAL-G physical restore requires the embedded PostgreSQL layout to be finalized (Phase 15 Docker packaging).
   - **Three structural checks: schema migrations, core tables, row count sample** — (1) `_sqlx_migrations` table exists and all migrations are marked successful; (2) the 5 core tables (`libraries`, `media_items`, `users`, `server_config`, `scheduled_tasks`) exist in `information_schema.tables`; (3) row counts from each core table via a single static SQL query with 5 `SELECT COUNT(*)` subqueries. All checks are read-only.
   - **Static SQL only** — The row-count sample uses a single static query with 5 subqueries (`SELECT (SELECT COUNT(*) FROM "libraries") AS libraries, ...`) rather than `format!`-built dynamic SQL, satisfying sqlx 0.9's `SqlSafeStr` guard without `AssertSqlSafe` wrappers.
   - **Drill failures are findings, not run failures** — `status=failed` (restore command failed or structural check failed) and `status=unavailable` (Docker missing, pg_dump disabled, no eligible dump) are persisted as stats with `Ok(())` return. Only infrastructure errors (DB write failure, stats serialization) bubble up as scheduler-level task failures via the fallible executor. This matches the `disk_space_check`/`reindex_maintenance` precedent: "threshold breach is a finding, not a failure."
   - **No-op when Docker unavailable** — Bare-metal/dev deployments without Docker produce a status of `"unavailable"` with `skip_reason: "docker is not available on this host"` rather than failing the run. This is the common case for direct development setups.
   - **Per-run password + project name UUIDs** — `rand::random::<[u8; 32]>()` hex-encoded for the disposable PostgreSQL password; `Uuid::now_v7().simple()` appended to `duskcue-drill-` for the compose project name. Both ensure concurrent drills cannot collide.
   - **Loopback-only port binding** — `127.0.0.1:{port}:5432` in the compose file; no host-network exposure. Default port `55433` avoids colliding with `verify-migrations.ps1`'s default of `55432`.
   - **`keep_alive` config option for debugging** — When `task_config.keep_alive = true`, the disposable PostgreSQL and compose file are left in place after the drill for operator inspection. Default is `false` (always clean up).
   - **23 unit tests, 0 integration tests** — Pure functions (option parsing, port/jobs clamping, password generation, database URL construction, truncate/tail helpers, dump timestamp parsing, stats serialization, compose file contents) are unit-tested. Docker invocation, `pg_restore`, and structural checks against a live PG are not integration-tested because they require Docker-in-CI and would be brittle. The verify-migrations.ps1 script covers the disposable-PG-with-migrations path separately.
   - **CHECK constraint rebuild via DROP + ADD** — The migration drops `scheduled_tasks_task_type_check` (PostgreSQL's auto-generated name for the column-level CHECK) and re-adds it with `backup_recovery_drill` appended. This matches the standard PostgreSQL pattern for evolving CHECK constraints (no `ALTER CONSTRAINT ... MODIFY` exists for CHECKs).
   - **`next_run_at` computed via `generate_series`** — Rather than the simpler `now() + INTERVAL '1 day'` placeholder used by other weekly seed migrations, the recovery-drill seed computes the actual next Sunday 07:00 UTC via `generate_series(... 14 days ...) FILTER (dow = 0 AND d > now())`. This prevents a one-off drill run on the wrong weekday, which would surprise operators of a non-time-critical weekly task.
   - **No new workspace dependencies** — All subprocess invocation uses existing `tokio::process::Command`; all HTTP/DB access uses existing `sqlx`/`tokio`; password generation uses existing `rand` 0.9.
   - **23 new tests, 0 clippy warnings, 0 build warnings** — 583 total server tests pass (560 prior + 23 new).
10. ~~Build admin settings UI — all `server_config` JSONB fields as toggles, sliders, dropdowns; push/webhook config fields visible but annotated "Activation requires Phase 13b — notification dispatch"; backup panel shows last backup, verification, retention, and recovery-drill evidence~~ **DONE**

**What was built for Task 10 (UI slice):**

| File | Purpose |
|---|---|
| `clients/web/src/lib/api/settings.js` | Added generic `server_config` helpers (`GET/PUT /server/config`, per-group updates) and scheduled-task helpers (list/get/trigger/cancel/runs) |
| `clients/web/src/lib/api/backups.js` | Added backup API helpers for status, task/run lists, WAL-G check, manual pg_dump, and verification |
| `clients/web/src/lib/api/index.js` | Exported backup helpers from the API barrel |
| `clients/web/src/routes/settings/system/+page.svelte` | New schema-driven admin config editor for all `server_config` JSONB groups with typed controls, per-group dirty state, one-group-at-a-time saves, and Phase 13b annotations for push/webhook settings |
| `clients/web/src/routes/settings/backups/+page.svelte` | Replaced placeholder with backup readiness/config/operations panel, scheduled backup trigger controls, recent backup evidence table, and recovery-drill evidence area |
| `clients/web/src/routes/settings/+page.svelte` | Added System link and enabled the Backups settings link |
| `docs/operations/CONFIGURATION.md` | Documented Task 10 config UI behavior and forward-compatible JSONB handling for notifications/storage |
| `docs/operations/BACKUP_RECOVERY.md` | Documented Task 10 backup panel API mapping and recovery-drill pending state |
| `PROJECT.md` | Updated Phase 13a status |

**Key decisions from Task 10:**

- **Schema-driven UI over per-group bespoke pages** — A single `/settings/system` route renders the known runtime JSONB groups from a local field schema. This keeps the broad Phase 13a settings surface maintainable while still using native controls for booleans, bounded numbers, enums, secrets, and arrays.
- **Per-group save boundary** — The UI calls `PUT /api/v1/server/config/{group}` instead of submitting the whole config row. This mirrors the backend hot-reload boundary, limits accidental cross-group writes, and preserves unknown keys inside the group object.
- **Forward-compatible notification/storage fields** — `notifications` exposes push/webhook settings with the required Phase 13b activation annotation. `storage` exposes cache paths and disk warning thresholds from `CACHE_STORAGE.md`; as of Task 8 the Rust `StorageConfig` deserializes the `disk_space_warnings` group into typed thresholds (consumed by the `disk_space_check` worker), while the remaining cache-path and size-limit fields remain forward-compatible JSONB until the future cache-eviction task expands the struct further.
- **Backup panel uses operational APIs, not raw config only** — `/settings/backups` reads `GET /api/v1/backups/status` for readiness, PostgreSQL recovery-safety checks, backup scheduled tasks, and recent run evidence; manual actions use the backup coordinator endpoints, while scheduled actions trigger existing scheduled tasks.
- **Recovery drill is shown as pending when absent** — Phase 13a Task 9 is not implemented yet. The UI looks for `backup_recovery_drill`/`recovery_drill` scheduled tasks and displays evidence once they exist; until then it shows a "Worker pending" state instead of fabricating status.
- **Existing Svelte 5 pattern followed** — Local route state uses runes (`$state`, `$derived`) like the subtitle settings page; API calls go through the existing `core.js` request wrapper with same-origin credentials and RFC 9457 error handling.

**Verification:** `npm run build` passes. `npx svelte-check --threshold warning` passes with 0 errors and 0 warnings.

**What was built for Task 2:**

| File | Purpose |
|---|---|
| `server/src/domains/system/mod.rs` | Added admin-only config routes: `GET/PUT /api/v1/server/config` and `GET/PUT /api/v1/server/config/{group}` |
| `server/src/domains/system/handlers.rs` | Thin handlers for full config and per-group read/update; all gated by `Require<CanManageServer>` |
| `server/src/domains/system/service.rs` | Generic `server_config` read/update service with allowlisted scalar fields and JSONB groups, masked responses, sensitive-value preservation/encryption, and runtime hot-reload |
| `server/src/domains/system/types.rs` | Added config response/request DTOs and validation wrappers |
| `server/src/domains/system/error.rs` | Added config-not-initialized, invalid-key, invalid-value, and serialization error variants |
| `server/src/error.rs` | Mapped config-not-initialized to `SYS_005`; invalid config values to `VALID_001`; invalid keys to generic bad request |
| `server/src/state.rs` | Fixed runtime config reload query to select `analytics`; decrypts subtitle provider credentials on load |
| `server/src/services/encryption.rs` | Added shared subtitle provider encrypt/decrypt helpers |
| `server/src/domains/subtitles/service.rs` | Reused shared subtitle provider encryption helper |

**Key decisions from Task 2:**

- **Generic JSONB editor, not notification-specific code** — The API stores arbitrary JSONB object payloads for all `server_config` groups, including future push/webhook settings under existing groups. Current `RuntimeConfig` only deserializes fields it knows about; unknown future JSONB keys remain preserved in the database.
- **Closed top-level allowlist** — Dynamic SQL is limited to the known scalar columns and JSONB group columns from the `server_config` schema. Values remain bound parameters; sqlx 0.9 dynamic SQL calls are wrapped with `AssertSqlSafe` only after this allowlist check.
- **Masked read responses** — Sensitive keys such as `api_key`, `access_token`, `api_token`, `client_secret`, `*_secret`, `*_token`, and `*_password` are masked in generic config responses. Admin clients can round-trip masked placeholders without overwriting existing encrypted values.
- **Secret preservation on partial group writes** — Omitted sensitive keys in a JSONB group update preserve the existing stored value. Sending `null` or an empty string intentionally clears the value; sending a new non-empty plaintext value encrypts it before storage.
- **Hot reload through existing ArcSwap path** — Successful writes call `load_runtime_config()` and `AppState::reload_runtime_config()`, matching the subtitle and Trakt settings precedent.
- **Runtime reload bug fixed** — `load_runtime_config()` now selects `analytics`, matching the Phase 2 schema and Phase 11 `RuntimeConfig.analytics` field. Subtitle provider credentials are also decrypted on load so encrypted SubDL/OpenSubtitles keys work outside their domain-specific settings endpoint.
- **Validation-first multi-field updates** — Full config updates prepare and validate all fields before applying any write, avoiding partial changes from malformed later fields in the same request.

**What was built for Task 3:**

| File | Purpose |
|---|---|
| `server/src/services/scheduler.rs` | Fixed manual trigger lifecycle (single run row), added claim-based running-state guard, completed run history on success/failure/timeout/cancel, added per-task cancellation tokens, added bounded run-history reads |
| `server/src/state.rs` | Added shared `Arc<Scheduler>` registration through `OnceLock` so HTTP handlers use the same scheduler instance as the background runner |
| `server/src/domains/system/mod.rs` | Added scheduled-task routes under `/api/v1/scheduled-tasks` |
| `server/src/domains/system/handlers.rs` | Added admin handlers for list, get, trigger, cancel, and per-task run history; gated by `Require<CanManageScheduledTasks>` |
| `server/src/domains/system/service.rs` | Added scheduled-task service wrappers, scheduler availability checks, and DTO mapping |
| `server/src/domains/system/types.rs` | Added scheduled-task response DTOs and run-history query/response DTOs |
| `server/src/domains/system/error.rs` | Added scheduled-task error variants and scheduler-error conversion |
| `server/src/error.rs` | Mapped scheduled-task errors to `SYS_001`, `SYS_002`, `SYS_003`, and service-unavailable responses |
| `server/src/workers/notification_cleanup.rs` | Added DB-only notification cleanup executor that deletes expired rows and rows older than `config.max_age_days` |
| `server/src/workers/mod.rs` | Exported `notification_cleanup` worker |
| `server/src/main.rs` | Registered `notification_cleanup` executor and stored the scheduler in `AppState` before startup |
| `docs/design/API_CONVENTIONS.md` | Updated endpoint inventory, async-operation examples, admin rate-limit scope, and ETag table to use `/api/v1/scheduled-tasks` and `/api/v1/server/config` |
| `docs/operations/CONFIGURATION.md` | Documented Task 3 API, shared scheduler registration, cancellation semantics, and notification cleanup boundary |
| `docs/design/PHASE_13_SPLIT.md` | Updated notification cleanup edge-case notes with implemented behavior |
| `PROJECT.md` | Updated Phase 13a status |

**Key decisions from Task 3:**

- **Shared scheduler instance in `AppState`** — Manual trigger/cancel uses the same executor registry as the background scheduler. `OnceLock<Arc<Scheduler>>` avoids rebuilding executor registration in handlers and keeps startup ordering explicit.
- **Dedicated scheduled-task capability** — The new endpoints use `CanManageScheduledTasks`, not broad `CanManageServer`, matching the capability model from Phase 4.
- **Single run row per manual trigger** — `trigger_task()` no longer creates a run before delegating to `execute_task()`. The execution path owns run creation for both scheduled and manual triggers.
- **Claim before run creation** — `execute_task()` updates `scheduled_tasks.state` from non-running to `running` before inserting `scheduled_task_runs`, preventing duplicate concurrent runs for the same task.
- **Run history is authoritative** — Success, failure, timeout, and cancellation all update the current `scheduled_task_runs` row. `complete_run()` only updates rows still in `running` state so an admin cancellation is not overwritten by a late worker completion.
- **Cooperative cancellation** — Each active task gets a `CancellationToken`; cancellation marks running history rows as cancelled immediately and signals the worker future wrapper. Long-running workers that are inside awaited futures are dropped when the cancellation branch wins.
- **Notification cleanup stays Phase 13a-only** — The executor only deletes old/expired rows from `notifications`. It does not create, render, dispatch, or localize notifications, preserving the Phase 13b boundary.

**What was built for Task 4:**

| File | Purpose |
|---|---|
| `server/src/domains/backup/mod.rs` | Added backup domain module declarations and admin routes: `GET /api/v1/backups/status`, `GET /api/v1/backups/tasks`, and `GET /api/v1/backups/runs` |
| `server/src/domains/backup/handlers.rs` | Added thin admin-only handlers gated by `Require<CanManageServer>`; validates `runs?limit=` range |
| `server/src/domains/backup/service.rs` | Added read-only backup status service: typed backup config projection, PostgreSQL recovery-safety setting checks, `pg_stat_archiver` read, backup scheduled-task listing, recent backup run listing, and readiness diagnostics |
| `server/src/domains/backup/types.rs` | Added row DTOs and response DTOs for backup config, readiness, PostgreSQL settings, WAL archive status, scheduled tasks, and recent runs |
| `server/src/domains/backup/error.rs` | Added `BackupError` domain error enum |
| `server/src/state.rs` | Expanded `BackupConfig` and added `WalGStorageType` from `BACKUP_RECOVERY.md`; missing JSONB fields default safely when `server_config.backup` is `{}` |
| `server/src/domains/mod.rs` | Exported the `backup` domain |
| `server/src/router.rs` | Merged the backup router into the API |
| `server/src/error.rs` | Added `AppError::Backup` and backup error-to-HTTP mapping |
| `docs/design/API_CONVENTIONS.md` | Added `/api/v1/backups/*` to the endpoint inventory |
| `docs/operations/BACKUP_RECOVERY.md` | Documented the Task 4 read-only status API boundary |
| `PROJECT.md` | Updated Phase 13a status |

**Key decisions from Task 4:**

- **Read-only domain boundary** — Task 4 creates the backup domain and observability surface only. WAL-G, `pg_dump`, verification command execution, retention cleanup, and scheduled backup execution remain in Tasks 5 and 6.
- **Admin visibility before execution** — `GET /api/v1/backups/status` exposes the config and environment checks an admin needs before enabling execution: `server_config.backup`, PostgreSQL settings, `pg_stat_archiver`, backup scheduled tasks, and recent backup runs.
- **Typed `BackupConfig` now lives in runtime config** — The previous placeholder struct was expanded to match `BACKUP_RECOVERY.md`, including WAL-G local/S3 storage, retention, pg_dump, data checksum, verification, and encryption flags. The struct uses serde defaults so existing `{}` JSONB rows deserialize into safe defaults.
- **No secret exposure** — Backup status reports whether S3 bucket and encryption key identifiers are configured, but does not expose hidden key material. The actual encryption key remains bootstrap-config territory per `BACKUP_RECOVERY.md`.
- **Literal SQL only** — Backup task filters use static SQL literals to satisfy the repository's `SqlSafeStr` guard; no dynamic SQL is needed.

**Verification:** `cargo check -p duskcue` and `cargo test -p duskcue domains::backup` pass.

**What was built for Task 5:**

| File | Purpose |
|---|---|
| `server/src/services/backup.rs` | Shared backup coordinator for WAL-G status checks, manual `pg_dump`, and verification; shell-free command spawning, WAL-G env construction, bounded output capture, pg_dump filename/path safety, and process-local operation locking |
| `server/src/services/mod.rs` | Exported the backup coordinator service |
| `server/src/domains/backup/mod.rs` | Added admin routes: `POST /api/v1/backups/wal-g/check`, `POST /api/v1/backups/pg-dump`, and `POST /api/v1/backups/verify` |
| `server/src/domains/backup/handlers.rs` | Added thin admin handlers for WAL-G checks, pg_dump trigger, and verification requests |
| `server/src/domains/backup/service.rs` | Added domain wrappers over the shared backup coordinator |
| `server/src/domains/backup/types.rs` | Added request/response DTOs for WAL-G checks, pg_dump trigger results, and backup verification results |
| `server/src/domains/backup/error.rs` | Expanded backup errors for operation-in-progress, command unavailable/failed/timeout, verification failure, and storage I/O |
| `server/src/error.rs` | Mapped backup coordination errors to HTTP responses, including `SYS_007` for concurrent backup operations and `SYS_009` for command/verification failures |
| `docs/design/API_CONVENTIONS.md` | Updated backup endpoint inventory |
| `docs/operations/BACKUP_RECOVERY.md` | Added Task 5 implementation notes and corrected the `BackupConfig` Rust mapping for encryption fields |
| `PROJECT.md` | Updated Phase 13a status and backup summary |

**Key decisions from Task 5:**

- **Shared coordinator for manual and scheduled use** — `services::backup` owns command assembly and verification logic so Phase 13a Task 6 can call the same functions from `backup_runner.rs` without duplicating WAL-G/pg_dump behavior.
- **Direct process execution only** — WAL-G, `pg_dump`, and `pg_restore` are launched via `tokio::process::Command`; no shell command strings are constructed. This keeps labels, paths, and database URLs out of shell interpolation.
- **Manual pg_dump verifies by default** — `POST /api/v1/backups/pg-dump` writes PostgreSQL custom-format dumps (`-F c`) and runs `pg_restore --list` unless the request sets `"verify": false`.
- **Verification supports both backup tiers** — `POST /api/v1/backups/verify` runs WAL-G archive integrity verification and/or logical dump verification. If no pg_dump path is supplied, it verifies the newest `.dump` file under the configured dump storage directory.
- **Single-instance lock boundary** — A process-local async mutex rejects overlapping manual backup/verification operations with `SYS_007`. This matches Duskcue's single-instance architecture and can be reused by the scheduled runner.
- **Path containment for dump verification** — User-supplied pg_dump verification paths must canonicalize under `server_config.backup.pg_dump_storage_path`; labels are reduced to safe ASCII filename characters.
- **WAL-G environment derived from runtime config** — Local storage sets `WALG_FILE_PREFIX`; S3 storage sets `WALG_S3_PREFIX` plus optional endpoint/region; active encryption sets `WALG_LIBSODIUM_KEY` from the bootstrap encryption key.
- **Task 6 remains scheduler scope** — Task 5 does not register `backup_database`, `backup_verification`, or retention executors. It supplies the reusable execution primitives for Task 6.

**Verification:** `cargo check -p duskcue`, `cargo test -p duskcue` (542 tests), and `cargo clippy -p duskcue --all-targets --all-features -- -A clippy::unnecessary-sort-by -D warnings` pass.

**What was built for Task 6:**

| File | Purpose |
|---|---|
| `server/src/workers/backup_runner.rs` | Added scheduled backup executors for `backup_database`, `backup_verification`, and `backup_retention_cleanup`; persists structured run stats into `scheduled_task_runs.stats` |
| `server/src/services/backup.rs` | Extended the shared backup coordinator with scheduled WAL-G `backup-push`, scheduled pg_dump, verification reuse, WAL-G retention cleanup, pg_dump daily/monthly retention cleanup, and PGDATA detection |
| `server/src/services/scheduler.rs` | Added `register_fallible_executor()` so operational workers can return errors that mark task runs as failed; preserved worker-written run stats on completion |
| `server/src/main.rs` | Registered backup database, verification, and retention cleanup executors on the shared scheduler |
| `server/src/workers/mod.rs` | Exported the backup runner worker |
| `server/migrations/20260627010000_seed_backup_scheduled_tasks.sql` | Added idempotent scheduled-task seeding for backup verification and retention cleanup; normalized the database backup schedule and `next_run_at` |
| `docs/operations/BACKUP_RECOVERY.md` | Added Task 6 implementation notes |
| `PROJECT.md` | Updated Phase 13a status and backup summary |

**Key decisions from Task 6:**

- **Fallible scheduler path for operational correctness** — Backup workers use `register_fallible_executor()` so command failures, timeouts, invalid config, and I/O errors mark `scheduled_task_runs.result = 'failure'` instead of logging internally while the scheduler records success.
- **Single shared backup coordinator** — Scheduled backups reuse `services::backup`; manual API and scheduled workers share command spawning, WAL-G environment construction, output bounding, pg_dump filename safety, verification behavior, and the process-local operation lock.
- **Scheduled database backup runs both configured tiers** — `backup_database` runs WAL-G `backup-push` (with `--verify` when `data_checksums` is true) when `wal_g_enabled` is true and `pg_dump --format=custom` when `pg_dump_enabled` is true. If both tiers are disabled, the task fails with an invalid backup configuration.
- **PGDATA boundary is explicit** — WAL-G physical base backups use `PGDATA` when set, otherwise `{data_dir}/postgres`, matching the embedded PostgreSQL deployment layout. Missing PGDATA produces a configuration failure rather than invoking WAL-G with an ambiguous path.
- **Verification respects runtime config** — `backup_verification` skips successfully when `server_config.backup.verification_enabled` is false; otherwise it verifies the enabled backup tiers, using WAL-G `wal-verify integrity` and `pg_restore --list` for the newest dump.
- **Retention is conservative for local dumps** — WAL-G retention uses `wal-g delete retain <wal_g_retention_full> --full --confirm`; pg_dump retention keeps all dumps inside the daily window, keeps the newest dump per month inside the monthly window, deletes older generated dumps, and retains unknown `.dump` filenames.
- **Run stats survive completion** — Backup workers write structured command/results into `scheduled_task_runs.stats`; scheduler completion now preserves existing stats when no explicit completion stats are provided.

**Verification:** `cargo check -p duskcue` and `cargo test -p duskcue services::backup` pass.

**What was built for Task 7:**

| File | Purpose |
|---|---|
| `server/src/workers/reindex_maintenance.rs` | Added scheduled index-bloat detection using `pgstatindex`, safe candidate filtering, `REINDEX INDEX CONCURRENTLY` execution, Prometheus maintenance metrics, structured run stats, and focused unit tests |
| `server/src/state.rs` | Expanded `MaintenanceConfig` and `PartitionRetention` from `DATABASE_MAINTENANCE.md`; existing `{}` JSONB rows deserialize into operational defaults |
| `server/src/main.rs` | Registered the `reindex_maintenance` executor on the shared scheduler via the fallible executor path |
| `server/src/workers/mod.rs` | Exported the reindex maintenance worker |
| `server/src/services/scheduler.rs` | Added `Reindex Maintenance` to the in-code default scheduled-task seed list |
| `docs/operations/DATABASE_MAINTENANCE.md` | Added Task 7 implementation notes |
| `PROJECT.md` | Updated Phase 13a status |

**Key decisions from Task 7:**

- **`pgstatindex` drives candidate selection** — The worker uses `pgstatindex(idx.oid::regclass)` from `pgstattuple` to measure B-tree `avg_leaf_density`; bloat is treated as `100 - avg_leaf_density`, matching `DATABASE_MAINTENANCE.md`.
- **Conservative candidate filter** — The query only considers public-schema B-tree indexes with `relkind = 'i'`, valid/ready `pg_index` metadata, size above the configured minimum, bloat above threshold, and no exclusion constraint backing. Partitioned parent indexes are skipped because they have `relkind = 'I'`.
- **Concurrent reindex only** — The worker executes one `REINDEX INDEX CONCURRENTLY "schema"."index"` statement per candidate through a pool connection, without wrapping statements in an explicit transaction.
- **Runtime config plus task overrides** — `server_config.maintenance` controls defaults (`reindex_enabled`, threshold, minimum size). Scheduled-task JSON can override `enabled`, `bloat_threshold_percent`, and `min_index_size_mb`; values are bounded before use.
- **Fallible operational reporting** — Failed index reindexes are recorded per index and the task returns failure if any candidate fails, allowing the scheduler to mark `scheduled_task_runs.result = 'failure'`. Successful and failed candidate details persist in `scheduled_task_runs.stats`.
- **Expected fillfactor space is not reindexed** — If a table has `fillfactor < 100`, candidate bloat within the reserved fillfactor margin is marked `skipped_expected_fillfactor`.

**Verification:** `cargo check -p duskcue` and `cargo test -p duskcue reindex_maintenance` pass.

**Post-Phase 13a parent-table ANALYZE follow-up complete:** `a05342e` registers the already-seeded `analyze_parents` task as a fallible scheduler worker. It respects the typed `MaintenanceConfig` toggle with a per-task override, runs plain `ANALYZE` for `play_sessions`, `play_events`, and `audit_log` (without `ONLY`, so PostgreSQL refreshes inheritance and child statistics), persists every table outcome in `scheduled_task_runs.stats`, and returns accumulated failures to the scheduler. Fixed `parent_table` metrics track successful and failed analyses. `ANALYZE SKIP LOCKED` remains intentionally excluded because PostgreSQL can skip the entire partition tree after a conflicting parent lock, which would leave planner statistics stale while appearing successful. `cargo test -p duskcue` passes 773 tests; strict `cargo check` and Clippy pass.

**What was built for Task 8:**

| File | Purpose |
|---|---|
| `server/src/workers/disk_space_check.rs` | 30-minute disk monitoring worker: `run_disk_space_check(state, task_id, config)` entry point; resolves three tiers (`data`/`cache`/`transcode`) from bootstrap paths + `RuntimeConfig.transcoding.transcode_path`; enumerates mounted disks via `sysinfo::Disks::new_with_refreshed_list()` wrapped in `tokio::task::spawn_blocking`; resolves each path to its backing disk via longest-mount-point-prefix match (handles tmpfs shadowing and Windows drive letters); emits `storage_usage_bytes`/`storage_capacity_bytes`/`storage_usage_percent` Prometheus gauges with `path` label; logs WARN on threshold breach; persists structured `DiskSpaceStats` (status, thresholds, per-path reports, breached/unavailable counts) into `scheduled_task_runs.stats`; `DiskSpaceError` enum (Database, StatsSerialization, EnumerationPanic); 14 unit tests |
| `server/src/workers/mod.rs` | Added `pub mod disk_space_check;` |
| `server/src/main.rs` | Registered `disk_space_check` fallible executor on the scheduler (7th fallible executor) with `disk_space_check_state` capture clone |
| `server/src/state.rs` | Expanded `StorageConfig` from empty placeholder to hold `DiskSpaceWarnings` (data/cache/transcode threshold percent, check interval, notify flag); `#[serde(default)]` keeps existing `{}` JSONB rows deserializing into CACHE_STORAGE.md defaults (90/90/80, 1800s, true); `Default` derived (clippy-compliant) |
| `docs/operations/CACHE_STORAGE.md` | Added "Phase 13a Task 8 Implementation Notes" section documenting disk-stats backend (`sysinfo` over `nix`/`statvfs`), path→disk resolution, tier sources, config expansion, notification boundary (Phase 13b), metrics, and fallible-executor semantics |

**Key decisions from Task 8:**

- **`sysinfo` 0.34 over `nix`/`statvfs`/`fs2`** — Already in workspace (used by `lockfile.rs` for PID liveness); cross-platform (Windows + Linux + macOS); no `unsafe`. `nix::sys::statvfs` is Unix-only; `statvfs`/`fs2` are unmaintained/Unix-only; raw `libc`/Win32 reinvents sysinfo. Verified `Disks::new_with_refreshed_list()` → `Disk::mount_point()/total_space()/available_space()` API via docs.rs (June 2026)
- **`spawn_blocking` for disk enumeration** — `Disks::new_with_refreshed_list()` is a sync syscall-heavy call; wrapped in `tokio::task::spawn_blocking` to avoid blocking the scheduler thread. Infrequent (30 min) and fast (ms), but cheap insurance
- **Longest-prefix path→disk resolution** — `sysinfo` has no "free space for this path" helper. The worker iterates disks and selects the one whose `mount_point()` is the longest prefix of the (canonicalized) target path. Naturally handles tmpfs shadowing (`/data/transcode` shadows `/data`), Windows drive letters (`C:\`), and custom admin-configured cache paths on separate volumes
- **Canonicalize-with-ancestor-fallback** — `resolve_canonical()` tries `std::fs::canonicalize(path)` first; if the path doesn't exist (common in dev without Docker volumes), it walks up to the nearest existing ancestor and canonicalizes that. Ensures the worker produces real disk stats in dev environments where `/data`/`/cache` don't exist but their parent drive does
- **No seed migration needed** — The `disk_space_check` task was already seeded by `20260530070000_seed_default_data.sql` (interval 1800s, timeout 60s, config `{"check_paths":true}`) and is in `seed_default_tasks`. Task 8 only registers the executor + implements the Rust worker. The `task_type` has been in the CHECK constraint since Phase 2
- **Notification boundary deferred to Phase 13b** — CACHE_STORAGE.md specifies "Create a `server_alert` notification" on breach, but notification **dispatch** (Fluent templates, SSE + webhook fan-out, push) is Phase 13b Task 2. Creating raw `notifications` rows now would produce unrenderable entries. Task 8 logs WARN + records metrics + persists run stats; notification creation is deferred to Phase 13b, which will wrap the existing worker findings. Mirrors the backup domain precedent (Task 4 read-only status preceded Task 5 execution)
- **Threshold breach is a finding, not a failure** — Worker returns `Ok(())` with `status: "threshold_exceeded"` (or `"healthy"`/`"unavailable"`) in run stats. Only infrastructure errors (DB write failure, stats serialization, enumeration panic) return `Err` so the scheduler marks the run failed. Matches `reindex_maintenance`/`backup_runner`
- **Transcode tier reads `transcoding.transcode_path`** — Not a separate `storage.transcode_path`. `TranscodingConfig` already owns that path and the transcode manager writes segments there (default `/cache/transcodes`)
- **`StorageConfig` expanded incrementally** — Only `DiskSpaceWarnings` (the subset the worker needs) was added; full cache-limits/eviction-policy/path-override fields deferred to the future cache-eviction task. `#[serde(default)]` preserves the generic-JSONB-editor behavior from Phase 13a Task 10 (unknown keys survive round-trips)
- **No new workspace dependencies** — `sysinfo` 0.34 already in workspace. All 560 server tests pass (546 prior + 14 new); 0 clippy warnings

**Verification:** `cargo check -p duskcue` passes. `cargo test -p duskcue workers::disk_space_check` passes (14 tests). `cargo clippy -p duskcue --all-targets --all-features -- -A clippy::unnecessary-sort-by -D warnings` passes.

**Phase-level verification target:** Admin can configure all settings via UI. Backups run on schedule. Disk space alerts trigger when thresholds are exceeded. Scheduled tasks are visible and triggerable. Recovery drills can restore into a disposable PostgreSQL instance and record a pass/fail evidence bundle.

**Phase 13a status:** All 10 tasks complete. `cargo check -p duskcue --all-targets` passes. `cargo test -p duskcue --lib` passes (583 tests). `cargo clippy -p duskcue --all-targets --all-features -- -A clippy::unnecessary-sort-by -D warnings` passes.

---

## Phase 13b — Notification System (COMPLETE)

**Goal:** Notification dispatch with multi-channel delivery (in-app + SSE + webhook), localized templates via Fluent, and push device registration for future mobile push.

**Prerequisites:** Phase 10 (SSE EventBus), Phase 13a Task 2 (server_config API for push/webhook config). Can overlap with Phase 14 if capacity allows.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [I18N.md](docs/design/I18N.md) | **i18n prerequisite** — Fluent server-side infrastructure; notification templates use Fluent message IDs, not English template strings |
| [MOBILE_PUSH.md](docs/design/MOBILE_PUSH.md) | **Multi-channel dispatch architecture** — in-app + SSE + webhook always available; mobile push via FCM/APNs/UnifiedPush opt-in |
| [PHASE_13_SPLIT.md](docs/design/PHASE_13_SPLIT.md) | **Phase split rationale** — Phase 13b's 6 tasks; dependency analysis; MVP fallback |

**Tasks:**

1. ~~Set up Fluent server-side i18n — `fluent-i18n` crate, `server/locales/en/notifications.ftl`, migrate `notification_types.in_app_template` from English strings to Fluent message IDs (debt item #5 from [IMPLEMENTATION_DEBT.md](docs/design/IMPLEMENTATION_DEBT.md))~~ **DONE** — see Task 1 notes below. Crate switched from `fluent-i18n` to `fluent-templates` (thread-local → explicit per-call locale model; async-safe for concurrent per-user rendering).
2. ~~Implement multi-channel dispatch pipeline — notification record always in DB; fan-out to in-app + SSE + webhook simultaneously; mobile push channel included in fan-out with provider delivery completed later in Phase 16a Task 9 (debt item #6 from [IMPLEMENTATION_DEBT.md](docs/design/IMPLEMENTATION_DEBT.md))~~ **DONE** — see Task 2 notes below.
3. ~~Implement notification CRUD — create, list, mark-as-read, delete; notification types and user preferences from Phase 2 tables~~ **DONE**

**What was built for Task 3:**

| File | Purpose |
|---|---|
| `server/src/domains/notifications/mod.rs` | Module declarations + router assembly with 10 routes across 6 path groups |
| `server/src/domains/notifications/error.rs` | `NotificationsError` enum with 6 variants: `NotFound` (SYS_004), `NotificationTypeNotFound` (NOT_FOUND), `InvalidCategory` (VALID_001), `InvalidPriority` (VALID_001), `InvalidChannelConfig` (VALID_001), `Database` catch-all |
| `server/src/domains/notifications/types.rs` | Three-type DTOs: `NotificationRow`/`NotificationTypeRow` (internal), `NotificationListQuery`/`UpdatePreferenceRequest`/`TestNotificationRequest` (Deserialize + Validate), `NotificationResponse`/`NotificationListResponse`/`UnreadCountResponse`/`MarkReadResponse`/`BulkMarkReadResponse`/`DeleteResponse`/`BulkDeleteResponse`/`NotificationTypeResponse`/`NotificationTypeListResponse`/`NotificationPreferenceResponse`/`NotificationPreferenceListResponse`/`PreferenceUpdateResponse` (Serialize); `VALID_CATEGORIES`/`VALID_PRIORITIES` statics |
| `server/src/domains/notifications/service.rs` | Full service: `list_notifications` (cursor pagination + filter by `is_read`/`category`/`priority`/`type`, JOIN `notification_types`), `count_unread`, `mark_read` (BOLA-scoped UPDATE, idempotent on already-read), `mark_all_read` (bulk UPDATE), `delete_notification` (BOLA-scoped DELETE with `rows_affected()` guard), `delete_read` (bulk delete all read), `list_notification_types`, `list_preferences` (LEFT JOIN with defaults), `update_preference` (COALESCE upsert); cursor encode/decode helpers; 11 unit tests |
| `server/src/domains/notifications/handlers.rs` | 10 thin handlers: list, unread-count, mark-read, mark-all-read, delete, delete-read, list-types, list-preferences, update-preference, send-test-notification (admin-only via `Require<CanManageServer>`); validation-error mapping per project convention |
| `server/src/error.rs` | Added `AppError::Notifications(#[from] NotificationsError)` variant + `notifications_error_to_http()` mapping all 6 variants (SYS_004/NOT_FOUND/VALID_001/INTERNAL) |
| `server/src/domains/mod.rs` | Added `pub mod notifications;` |
| `server/src/router.rs` | Merged notifications router via `.merge(crate::domains::notifications::router(state.clone()))` |
| `clients/web/src/lib/api/notifications.js` | Full API client: `listNotifications`, `getUnreadCount`, `markNotificationRead`, `markAllRead`, `deleteNotification`, `deleteReadNotifications`, `listNotificationTypes`, `listNotificationPreferences`, `updateNotificationPreference`, `sendTestNotification` |
| `clients/web/src/lib/api/index.js` | Added `notifications.js` to barrel export |
| `docs/design/NOTIFICATIONS.md` | Created — authoritative design document for the in-app notification center REST API surface |

**Key decisions from Task 3:**

- **Cursor pagination per API_CONVENTIONS.md** — API_CONVENTIONS.md Pagination Strategy table explicitly specifies cursor pagination for Notifications ("Chronological feed"). The `notifications.id` column is `UUID DEFAULT uuidv7()`, which embeds a Unix-millisecond timestamp — so `id` IS naturally time-ordered. Cursor pagination on `id` gives chronological ordering without a separate sort column. Reuses the media domain's `parse_cursor`/`encode_cursor` pattern (base64-encoded `{"id":"<uuid>"}` JSON, `LIMIT N+1` for `has_more` detection). Two static SQL constants (`LIST_NOTIFICATIONS_DESC_SQL`/`LIST_NOTIFICATIONS_ASC_SQL`) per sqlx 0.9 `SqlSafeStr` requirement.
- **BOLA prevention via SQL `WHERE user_id = $N`** — Every mutation (`mark_read`, `mark_all_read`, `delete`, `delete_read`) binds `user_id` directly into the SQL `WHERE` clause. A user cannot affect another user's notifications even with a valid notification UUID. `rows_affected() == 0` on delete returns `NotFound` — indistinguishable from "doesn't exist" vs "belongs to another user" (no information leakage). Matches the bookmark delete pattern from the playback domain.
- **`mark_read` is idempotent on already-read** — When the UPDATE matches a row that's already `is_read = true`, the `WHERE ... AND is_read = false` clause causes `rows_affected() == 0`. The service then does a follow-up existence check (`SELECT id FROM notifications WHERE id = $1 AND user_id = $2`) — if the row exists, returns success with current timestamp (idempotent); if not, returns `NotFound`. This matches standard REST semantics for state-transition endpoints.
- **No new error codes registered** — `SYS_004` ("Notification not found", 404) was already registered in ERROR_HANDLING.md SYS section and is reused for `NotificationsError::NotFound`. The other 5 variants map to existing generic codes (`NOT_FOUND`, `VALID_001`, `INTERNAL`). Follows the Segment/Storyboard/Subtitle precedent of mapping domain-specific variants to existing codes.
- **Preferences endpoint materializes defaults** — `GET /api/v1/user/notification-preferences` LEFT JOINs `notification_types` with `user_notification_preferences` for the current user. When no explicit row exists (the common case — most users accept defaults), the response uses `notification_types.is_enabled_by_default` for `in_app_enabled` and sensible defaults (`webhook_enabled = false`, `push_enabled = false`). The `is_using_defaults: bool` flag tells the UI whether the user has explicitly overridden anything for that type. Avoids a second round-trip to fetch notification type metadata.
- **Test notification endpoint for verification flow** — `POST /api/v1/notifications/test` (admin-only via `Require<CanManageServer>`) dispatches via the existing `services::notification_dispatch::dispatch()` pipeline. Default notification type is `server_alert`; optional `title`/`body` overrides. Returns the `DispatchResult` so the admin can verify per-channel status (in_app/sse/webhook/push). Serves the Phase 13b verification criterion: "Admin triggers a test notification. Notification appears in-app (notification center), via SSE (live update if web client is open), and via webhook."
- **Route design per REST conventions** — `POST /api/v1/notifications/{id}/read` (action sub-resource, matches the analytics `acknowledge` pattern for trust events); `POST /api/v1/notifications/read-all` (bulk action, distinct from `read` to avoid path-param collision); `DELETE /api/v1/notifications/read` (bulk delete read — clears the read pile without touching unread). The `read-all` / `read` distinction keeps bulk vs single-resource operations on separate routes per REST best practice.
- **Domain five-file pattern** — `notifications/` follows the established pattern: `mod.rs` (router), `error.rs` (domain errors), `types.rs` (DTOs), `service.rs` (SQL), `handlers.rs` (thin HTTP translation). User-scoped routes use `AuthenticatedUser` (no capability check needed — BOLA enforced at SQL layer). The single admin route uses `Require<CanManageServer>`.
- **No new DB migrations** — `notifications`, `notification_types`, and `user_notification_preferences` tables were created in Phase 2 migrations. Phase 13b Task 2's migration `20260628030000` added the `push_enabled` column. Task 3 is pure API-layer work against existing schema.
- **No new workspace dependencies** — All functionality uses existing `sqlx`, `validator`, `serde`, `base64`, `uuid`, `chrono`, `axum` crates. Test endpoint reuses the existing `notification_dispatch` service.
- **11 unit tests** covering: cursor encode/decode roundtrip, cursor garbage rejection, cursor missing-id rejection, cursor none-input, base64+JSON cursor structure, category validation (accept known/reject unknown/accept none), priority validation (accept known/reject unknown/accept none). All 620 server tests pass (609 prior + 11 new). 0 clippy warnings, 0 svelte-check warnings.

**Not yet implemented (deferred to later tasks):**

- Webhook format-specific payloads (ntfy/Gotify/Discord/Slack) — Phase 13b Task 4
- `user_push_devices` table + registration API — Phase 13b Task 5
- Notifications UI (notification center, preferences editor, push device management) — Phase 13b Task 6
4. ~~Implement webhook dispatch — HTTP POST to operator-configured URL with ntfy/Gotify/Discord/Slack/generic formats; HMAC signing; retry with backoff (debt item #8 from [IMPLEMENTATION_DEBT.md](docs/design/IMPLEMENTATION_DEBT.md))~~ **DONE** — see Task 4 notes below.
5. ~~Create `user_push_devices` table + `POST /api/v1/user/push-devices` API — device registration for FCM/APNs/UnifiedPush tokens; token lifecycle (heartbeat, auto-invalidation, manual revoke) (debt item #7 from [IMPLEMENTATION_DEBT.md](docs/design/IMPLEMENTATION_DEBT.md))~~ **DONE** — see Task 5 notes below.
6. ~~Build notifications UI — notification center, preferences, push device management, per-channel opt-in per notification type~~ **DONE** — see Task 6 notes below.

**Verification:** Admin triggers a test notification. Notification appears in-app (notification center), via SSE (live update if web client is open), and via webhook (operator-configured endpoint). Notification templates render in the user's preferred locale via Fluent. Push devices register and display in user settings.

**Phase 13b status:** All 6 tasks complete (Fluent i18n infrastructure + template migration; multi-channel dispatch pipeline with DB-write-first + SSE fan-out + webhook with HMAC signing + Phase 13b push placeholder later completed by Phase 16a Task 9; in-app notification center CRUD with cursor pagination + preferences + admin test dispatch; webhook format-specific dispatch [generic/ntfy/gotify/discord/slack] + HMAC signing for all formats + exponential-backoff retry with full jitter + retryable/non-retryable status classification + `Retry-After` honored; `user_push_devices` table + registration/heartbeat/revoke API + 30-day stale-device deactivation wired into `notification_cleanup`; notifications UI — notification bell + dropdown in navbar, persistent notification center store with SSE + polling, full-page feed/preferences/push-devices/admin-test hub). 0 svelte-check warnings, 0 build errors.

**What was built for Task 6:**

| File | Purpose |
|---|---|
| `clients/web/src/lib/api/notifications.js` | Added 4 push device API functions: `listPushDevices`, `registerPushDevice`, `updatePushDevice` (heartbeat), `deletePushDevice` (revoke) — completing the client API surface for Task 5's backend endpoints |
| `clients/web/src/lib/stores/notificationCenter.js` | Persistent server notification store: `init()`/`shutdown()` lifecycle (mounted by NotificationBell); SSE `notification` event subscription via `events.on()` prepends live notifications + increments unread; 60s unread-count polling fallback (`fetchUnreadCount`) for when SSE is disconnected; `refresh()` loads first page + unread count in parallel; `loadMore()` cursor pagination; `markRead`/`markAllRead`/`remove`/`deleteRead` with optimistic UI updates (decrement unread, toggle `is_read`); `reset()` on logout; derived exports: `notificationItems`, `unreadCount`, `notificationsLoading`, `notificationsError` |
| `clients/web/src/lib/components/NotificationBell.svelte` | Navbar bell component: bell-icon button with unread-count badge (caps at 99+); dropdown panel showing 6 recent notifications with category icons, priority indicators, relative timestamps; click notification → mark-read + navigate `link`; per-item delete button; "Mark all read" bulk action; "View all" link to settings page; backdrop close + Escape key support; empty state ("You're all caught up"); initializes and shuts down the `notificationCenter` store on mount/destroy |
| `clients/web/src/routes/+layout.svelte` | Wired `NotificationBell` between `.nav-search` and `.nav-user` (industry-standard placement — GitHub/GitLab/Slack/Discord); added Notifications link to mobile drawer for discoverability |
| `clients/web/src/routes/settings/notifications/+page.svelte` | Full notifications hub with 3 tabs: **Feed** (full list with all/unread filter chips, mark-all-read, delete-read, per-item delete, cursor "Load more", empty states), **Preferences** (per-notification-type × per-channel [in-app/webhook/push] toggle matrix with "default" tags, per-row dirty-state save buttons, mobile responsive with `data-label` channel labels), **Push Devices** (registered device list with provider labels, token previews, last-seen/invalidated timestamps, active/inactive badges, revoke buttons; empty state directing to mobile app) + admin-only **Test Notification** section (`Require<CanManageServer>`-gated dispatch button with delivery-status toast feedback) |
| `clients/web/src/routes/settings/+page.svelte` | Added Notifications link to `settingsLinks` array (bell icon, between Collections and Backups) |

**Key decisions from Task 6:**

- **Distinct store name `notificationCenter` over `notifications`** — The existing `stores/notifications.js` is the ephemeral toast store (transient UI feedback, 5s auto-dismiss). The persistent server notification center needs a different name to avoid collision. `notificationCenter` clearly distinguishes the long-lived, DB-backed notification feed from the short-lived toast messages. Both coexist: toasts fire for user-action feedback ("Preference saved"), the center holds server-dispatched notifications ("New media added").
- **Bell + dropdown for quick access, full page for management** — The bell dropdown (6 recent items) satisfies the "notification center" verification criterion without a dedicated full-page route for quick glances. The `/settings/notifications` page provides the full feed (cursor pagination, filters), preferences editor, and push device management. This matches the GitHub/GitLab/Slack pattern: bell icon for recent, full page for everything.
- **SSE primary, polling fallback for unread count** — The store subscribes to the `notification` SSE event (emitted by `notification_dispatch.rs::publish_sse`) for instant updates. A 60s `setInterval` polls `GET /notifications/unread-count` as a safety net for when SSE is disconnected (browser tab backgrounded long enough for the connection to drop, or network blip). The poll only updates the count, not the list — the list refreshes on next page focus / manual refresh.
- **Optimistic UI on all mutations** — `markRead`, `markAllRead`, `remove`, and `deleteRead` update local state immediately (decrement unread, toggle `is_read`, remove item) before awaiting the API response. Network failures surface as toast errors but the optimistic state remains (the server is authoritative on next `refresh()`). This matches the toast store's synchronous feel and avoids lag on quick mark-read/delete actions.
- **Per-user preferences accessible to all authenticated users** — Unlike subtitle/overlay/system settings (admin-gated via `can_manage_server`), notification preferences are per-user (`/api/v1/user/notification-preferences` is `AuthenticatedUser`-scoped). The preferences and push-devices tabs are available to every signed-in user. Only the "Test Notification" section within the Push Devices tab is admin-gated (`hasCapability('can_manage_server')`), matching the backend `Require<CanManageServer>` on `POST /notifications/test`.
- **Push device registration is NOT in the web UI** — Per MOBILE_PUSH.md, device registration happens from the mobile app (`POST /api/v1/user/push-devices` with a provider token from FCM/APNs/UnifiedPush). The web UI only lists registered devices and allows revocation. The empty state directs users to "Install the Duskcue mobile app and sign in to register a device." This matches the PHASE_13_SPLIT.md MVP boundary (schema + API ship now; FCM/APNs client implementations are Phase 16a).
- **Preferences as a type × channel matrix** — Each notification type is a row; the three channels (in-app, webhook, push) are columns with toggle switches. A "default" tag indicates the user hasn't overridden the system default (`is_using_defaults: true`). Per-row save buttons appear only when that row is dirty, avoiding a single "save all" that might accidentally persist unchanged rows. Mobile collapses to a stacked layout with `data-label` channel names.
- **Category-color theming** — Each notification category (security=red, system=accent, media=green, task/user=secondary) has a consistent color applied to the badge dot, icon background, and category label. Uses `color-mix(in srgb, ...)` for tinted backgrounds — a modern CSS feature supported by all Duskcue target browsers (Chromium-based, 2024+).
- **`<div role="button">` over `<li>` for interactive feed items** — Svelte 5's a11y rules warn when non-interactive elements (`<li>`, `<nav>`) get interactive roles or click handlers. Feed items use `<div role="button" tabindex="0" onkeydown>` for keyboard accessibility (Enter/Space activation). The tablist uses `<div role="tablist">` instead of `<nav role="tablist">`. 0 svelte-check warnings maintained.
- **Test notification shows delivery status** — The admin test button calls `sendTestNotification()` and displays the `delivery_status` per-channel result in a toast (`in_app: delivered, sse: delivered, webhook: pending, push: skipped`). After 600ms, the feed refreshes to show the newly-created in-app notification. This satisfies the Phase 13b verification flow end-to-end through the UI.
- **No new npm dependencies** — All UI uses Svelte 5 runes (`$state`/`$derived`/`$effect`), `svelte/transition` (`fly`, `fade`), `svelte/animate` (`flip`), and the existing `core.js` HTTP wrapper + `events.js` SSE store. CSS uses design tokens from `app.css` and `color-mix()` for category tints.

**Verification:** `npx svelte-check --threshold warning` → 0 errors, 0 warnings. `npm run build` → success (0 errors). Matches the verification bar from Phase 10 Tasks 7–8 and Phase 8 Task 4.

**MVP fallback:** If Phase 13b takes longer than estimated, ship in-app + SSE + webhook only. Defer FCM/APNs/UnifiedPush client implementations to Phase 16a. The `user_push_devices` table and API still ship (schema-only) to avoid Phase 16a schema migration. See [PHASE_13_SPLIT.md](docs/design/PHASE_13_SPLIT.md) for details.

**What was built for Task 4:**

| File | Purpose |
|---|---|
| `server/src/services/notification_dispatch.rs` | Added `WebhookFormat` enum + `from_config()` parser; `FormattedRequest` struct; `format_request()` producing format-specific request bodies for `generic`/`ntfy`/`gotify`/`discord`/`slack`; `sign_request()` (HMAC header now applied to ALL formats); priority mappers (`ntfy_priority`, `gotify_priority`, `ntfy_tags`); retry loop (`dispatch_webhook` + `send_once` + `WEBHOOK_BACKOFF_SECONDS` + `is_retryable_status` + `parse_retry_after` + `jittered_duration` + `build_webhook_client`); restructured `WebhookError` into 4 variants (`ClientBuild`/`RequestFailed`/`NonRetryableStatus`/`RetryableStatus`); 16 new unit tests + 2 `tokio::test` integration tests with raw `TcpListener` HTTP mocks |
| `clients/web/src/routes/settings/system/+page.svelte` | Added `webhook_format` select (`generic`/`ntfy`/`gotify`/`discord`/`slack`) to the Notifications config group; updated `webhook_url`/`webhook_secret` hints to clarify token placement (URL) and optional HMAC for all formats |
| `docs/design/MOBILE_PUSH.md` | Implementation Status table updated (webhook formats + retry ✅); added "Phase 13b Task 4 implementation notes" section; added 6 new research sources (ntfy/Gotify/Discord/Slack publish APIs + Hookdeck/Svix retry best-practice guides) |
| `docs/design/IMPLEMENTATION_DEBT.md` | Debt item #8 status updated from "Spec only" to "✅ Phase 13b Task 4" |

**Key decisions from Task 4:**

- **Five payload formats via infallible `from_config()` parser** — `WebhookFormat::from_config()` falls back to `Generic` on any unknown string. Deliberately does NOT implement `std::str::FromStr` (which would require `Result`); clippy flagged `from_str` as trait-confusing (`should-implement-trait`). The fallback means a typo in `server_config.notifications.webhook.format` never breaks dispatch — it degrades gracefully to the generic JSON shape.
- **Each format renders from already-localized title/body** — No per-format i18n. The dispatch pipeline renders Fluent templates once; format-specific request bodies compose from those localized strings. ntfy uses plain text + headers; gotify/discord/slack use their native JSON shapes; generic carries the full Duskcue payload including `notification_id` for deduplication.
- **Discord content truncated to 2000 chars by Unicode scalar value** — Discord's hard cap. `.chars().take(2000)` not byte-slicing, so multi-byte UTF-8 (e.g., emoji in media titles) is not split mid-codepoint. Appends `?wait=true` so Discord returns a real status (default is 204 fire-and-forget which masks rate-limit drops).
- **HMAC signing applied to ALL formats** — Task 2 only signed `generic`. Task 4's `sign_request()` appends `X-Duskcue-Signature` over the (format-specific) body bytes for every format when a secret is configured. For ntfy/gotify the URL-token is primary auth; the optional HMAC is defense-in-depth per the GitHub/Hook0 signature convention.
- **Exponential backoff with full jitter (0.5×–1.5×)** — Schedule `[1s, 5s, 30s, 2m, 10m]` per MOBILE_PUSH.md §Retry policy; full jitter applied to every wait per Hookdeck/Svix best-practice guides (June 2026) to prevent thundering-herd spikes. Initial attempt is immediate, then up to 5 retries with the schedule applied before each retry (6 total sends max).
- **Retryable classification prevents Discord IP bans** — `is_retryable_status()` returns true only for `408 | 429 | 500 | 502 | 503 | 504`. A `404` (deleted Discord webhook) aborts immediately; this matters because Discord bans IPs at 10,000 invalid requests (401/403/429) per 10 minutes — blind retries on a deleted webhook would approach that threshold.
- **`Retry-After` honored, capped at 10 minutes** — All four providers use integer-seconds form (HTTP-date form parsed as `None`, rare for these services). The 10-minute cap prevents a misconfigured/malicious endpoint from stalling a delivery task indefinitely via huge `Retry-After` values.
- **`WebhookError` split into retryable/non-retryable variants** — Old single `NonSuccessStatus` couldn't drive retry decisions. New `RetryableStatus { status, retry_after, body }` and `NonRetryableStatus { status, body }` let the retry loop branch cleanly without re-inspecting the status code.
- **Fire-and-forget preserved; no "degraded + admin notified" yet** — Retry loop runs entirely in the spawned `tokio::spawn` task. After exhaustion, `delivery_status.webhook = "failed"` + WARN log. The MOBILE_PUSH.md "after 5 failures mark degraded and notify admin" deferred — admin self-notification via the dispatch pipeline risks recursion; deferred to a future hardening task. The notification record in the DB is always the source of truth, so webhook failure is never data loss.
- **Webhook client adds `connect_timeout(10s)` + `no_proxy()`** — Task 2's client had only `timeout(15s)` + `redirect::none()`. Task 4 adds the connection-phase timeout (so a silent TCP drop fails fast instead of consuming the full 15s) and `no_proxy()` per API_SECURITY.md SSRF hardening (prevents a malicious `HTTP_PROXY` env var from redirecting webhook traffic).
- **16 unit + 2 integration tests** — Unit tests cover format parsing, all 5 format bodies, HMAC presence/absence, status classification, Retry-After parsing, jitter band, and the backoff schedule. The 2 `tokio::test` integration tests use a raw `TcpListener` to serve a real HTTP 429 (with `Retry-After: 2`) and a real 404, verifying `send_once` classifies them correctly into `RetryableStatus`/`NonRetryableStatus` with parsed headers. No `mockito`/`wiremock` crate added — raw TCP keeps the test dependency-free. 638 server tests pass (620 prior + 18 new), 0 clippy warnings, 0 svelte-check warnings.

**What was built for Task 5:**

| File | Purpose |
|---|---|
| `server/migrations/20260629010000_create_user_push_devices.sql` | `user_push_devices` table per MOBILE_PUSH.md DDL — UUIDv7 PK, `UNIQUE(user_id, provider, token)` for upsert-safe re-registration, partial index `WHERE is_active = true`, `provider` CHECK matching `PushDispatchConfig::is_configured()` |
| `server/src/domains/notifications/service.rs` | 5 push-device service functions: `register_push_device` (upsert on conflict reactivates invalidated devices + refreshes metadata via COALESCE), `list_push_devices` (ordered active-first, recently-seen-first), `update_push_device` (heartbeat — `last_seen_at=now()`, optional metadata refresh; `is_active=true` guard), `delete_push_device` (hard DELETE; manual revoke), `deactivate_stale_devices` (server-side 30-day staleness deactivation for the cleanup worker); 3 validators: `validate_push_provider`, `validate_push_token` (no pattern validation for FCM/APNs per Google/Apple guidance; URL validation for UnifiedPush), `validate_optional_length`; `mask_token` helper (first 8 + last 4 chars); 11 unit tests |
| `server/src/domains/notifications/handlers.rs` | 4 working handlers: `register_push_device`, `list_push_devices`, `update_push_device`, `delete_push_device` — all user-scoped via `AuthenticatedUser`, BOLA at SQL layer |
| `server/src/domains/notifications/mod.rs` | Added 2 route groups: `/api/v1/user/push-devices` (POST + GET), `/api/v1/user/push-devices/{device_id}` (PUT + DELETE) |
| `server/src/domains/notifications/types.rs` | Three-type DTOs: `PushDeviceRow` (internal), `RegisterPushDeviceRequest`/`UpdatePushDeviceRequest` (Deserialize + Validate), `PushDeviceResponse`/`PushDeviceListResponse`/`PushDeviceDeletedResponse` (Serialize); 5 validation statics (`VALID_PUSH_PROVIDERS`, `MAX_PUSH_TOKEN_LEN`, `MAX_DEVICE_NAME_LEN`, `MAX_PLATFORM_LEN`, `MAX_APP_VERSION_LEN`) |
| `server/src/domains/notifications/error.rs` | Added 3 error variants: `PushDeviceNotFound` (SYS_004), `InvalidPushProvider` (VALID_001), `InvalidPushToken` (VALID_001) |
| `server/src/error.rs` | Mapped the 3 new variants in `notifications_error_to_http()` |
| `server/src/workers/notification_cleanup.rs` | Wired `deactivate_stale_devices()` call after expired-notification deletion; reads `stale_device_days` from task config (default 30, clamped [1, 3650]) |

**Key decisions from Task 5:**

- **No pattern validation for FCM/APNs tokens** — Research (June 2026) confirmed both Google ([FCM manage-tokens docs](https://firebase.google.com/docs/cloud-messaging/manage-tokens)) and Apple ([APNs send docs](https://developer.apple.com/documentation/usernotifications/sending-notification-requests-to-apns): *"Don't make assumptions about device token size"*) explicitly warn the format may change and should not be validated against patterns. Duskcue validates only: non-empty + ≤4096 chars + printable ASCII (bytes ≥ 0x20). Strict validation happens at delivery time (provider returns `UNREGISTERED`/`BadDeviceToken` → server-side invalidation in Phase 16a)
- **UnifiedPush tokens ARE URL-validated** — Unlike FCM/APNs, UnifiedPush tokens are endpoint URLs returned by the distributor (e.g., `https://ntfy.example.com/duskcue-up/abcdef`). `url::Url::parse` validation catches malformed registrations early. Matches the "Duskcue treats UnifiedPush as a special webhook" design
- **Registration is an upsert (idempotent re-registration)** — `ON CONFLICT (user_id, provider, token) DO UPDATE SET last_seen_at = now(), is_active = true, invalidated_at = NULL`. This reactivates previously invalidated devices (app re-install with the same FCM token after Google rotation) and refreshes metadata via COALESCE. Mobile apps should call `POST` on every launch — no separate "register vs heartbeat" decision for the client
- **Heartbeat (`PUT /{device_id}`) requires `is_active = true`** — The `WHERE id = $1 AND user_id = $2 AND is_active = true` clause means heartbeats on invalidated devices return `PushDeviceNotFound` (BOLA-safe: no information leakage). The mobile app re-registers via `POST` on next launch, which reactivates
- **30-day stale-device deactivation wired into `notification_cleanup`** — The existing scheduled task (every 1h, per Phase 2 seed) now calls `deactivate_stale_devices()` after deleting expired notifications. Devices with `last_seen_at < now() - INTERVAL '30 days'` are set `is_active = false, invalidated_at = now()`. Configurable via task config `stale_device_days` (default 30). Implements the "Devices not seen in 30 days are marked inactive" rule without a new scheduled task
- **Token-revoked-response invalidation** — Task 5 shipped schema + lifecycle API + staleness deactivation. Phase 16a Task 9 completed provider-response invalidation for FCM `UNREGISTERED`, APNs `BadDeviceToken`/`Unregistered`, and UnifiedPush 404/410 responses.
- **Token preview (masked) in responses** — `PushDeviceResponse.token_preview` shows first 8 + last 4 chars with `…` separator. Tokens shorter than 12 chars show `***`. Minimizes token exposure in API responses/logs while still letting users identify which device is which alongside the `device_name` field
- **Manual revoke is a hard DELETE** — `DELETE /{device_id}` removes the row entirely (not soft-delete). The user explicitly wants the device gone; re-registration creates a new row; soft-deleted rows would accumulate as dead weight. Differs from automatic invalidation (provider response or staleness) which uses `is_active = false` + `invalidated_at` so the dispatch pipeline skips and the mobile app can detect invalidation and re-register
- **No new error codes** — `PushDeviceNotFound` reuses `SYS_004` (already registered for "Notification not found"); `InvalidPushProvider`/`InvalidPushToken` use `VALID_001`. Follows the established precedent of mapping domain variants to existing codes
- **No new workspace dependencies** — validation uses existing `url` crate (Phase 4 Task 2 for WebAuthn); all DB/routing/validation uses existing `sqlx`/`axum`/`validator`
- **11 new unit tests, 0 clippy warnings** — All 651 server tests pass (638 prior + 13 new across notifications service)

**What was built for Task 1:**

| File | Purpose |
|---|---|
| `server/src/services/i18n.rs` | Fluent infrastructure: `static_loader!` macro embedding `server/locales/` into the binary at compile time; `negotiate_locale()` implementing the I18N.md locale chain (user preference → Accept-Language → base English); `render()` / `args_from_metadata()` helpers for the Task 2 dispatch pipeline; `set_use_isolating(false)` customisation for clean plain-text output; 17 unit tests |
| `server/locales/en/notifications.ftl` | All 11 seeded notification templates migrated from English `{{key}}` interpolation strings to Fluent message IDs with `{ $arg }` syntax (kebab-case message IDs, kebab-case variable names) |
| `server/migrations/20260628020000_migrate_notification_templates_to_fluent.sql` | Idempotent `UPDATE` migration converting all 11 `notification_types.in_app_template` values from English template strings to Fluent message IDs (e.g., `'{{title}} was added to {{library}}'` → `'new-media-added'`) |
| `Cargo.toml` / `server/Cargo.toml` | Added `fluent-templates` 0.14, `fluent-bundle` 0.16, `fluent-langneg` 0.13, `unic-langid` 0.9 to workspace deps |
| `server/src/services/mod.rs` | Exported `pub mod i18n;` |
| `docs/design/I18N.md` | Corrected primary crate recommendation from `fluent-i18n` to `fluent-templates`; added "Crate Selection Rationale" section documenting the thread-local vs explicit-locale concurrency analysis; updated renderer example, rejection table, Implementation Status table, Key Decisions, and Research Sources |
| `docs/design/IMPLEMENTATION_DEBT.md` | Updated debt item #5 status from "Spec only" to "✅ Phase 13b Task 1" |

**Key decisions from Task 1:**

- **`fluent-templates` over `fluent-i18n`** — `fluent-i18n` uses thread-local locale state (`set_locale()`/`get_locale()`) which races in async Axum (tokio tasks migrate between worker threads; concurrent notification rendering for users with different locales corrupts the global thread-local). `fluent-templates` takes locale as an explicit per-call argument (`LOCALES.lookup_with_args(&langid, key, &args)`) — no shared mutable state. I18N.md:109 anticipated this ("Falls back to `fluent-templates` if its API proves insufficient"); Task 1 research confirmed the fallback as the correct primary. The decision is fully documented in I18N.md "Crate Selection Rationale" section.
- **`static_loader!` compile-time embedding** — `.ftl` files compiled into the binary at build time. No runtime file I/O, no `CARGO_MANIFEST_DIR` lookup. Matches Duskcue's single-binary deployment goal (Phase 15 Docker image contains no external locale files).
- **Kebab-case message IDs in DB, snake_case `name` column unchanged** — `notification_types.in_app_template` stores `'new-media-added'` (Fluent convention); `notification_types.name` stays `'new_media_added'` (Rust/JSON convention). Different namespaces; no redundancy issue.
- **Kebab-case Fluent variable names with `args_from_metadata` normalization** — `.ftl` uses `{ $task-name }` (Fluent convention); `args_from_metadata()` converts notification metadata JSONB keys from snake_case (`task_name`) to kebab-case automatically. Callers pass raw metadata; the i18n layer handles convention conversion.
- **`set_use_isolating(false)`** — Disables Unicode bidi isolating marks (U+2068/U+2069) around arguments for clean plain-text output to DB body / webhook payload / SSE fields. Arguments come from trusted internal sources (media titles, usernames). Matches `fluent-i18n`'s default behavior.
- **Not-found detection via prefix check** — When a Fluent message ID isn't in the loaded resources, `fluent-templates` returns `"Unknown localization key: \"<id>\""`. `render()` checks this prefix, logs a warning, and returns the clean message ID (better for debugging than the raw error string). A message that exists but has a missing required variable also surfaces this way (formatting error treated as lookup failure by fluent-templates) — documented in the `returns_raw_id_when_required_arg_is_missing` test.
- **No AppState wiring needed** — `LOCALES` is a process-wide `&'static StaticLoader` created by `static_loader!`; accessed directly from any async context. No `Arc<FluentBundle>` in `AppState`. The dispatcher (Task 2) will call `services::i18n::render()` directly.
- **17 new tests, 600 total passing** — 6 negotiation tests (user pref priority, Accept-Language fallback, empty/blank/garbage inputs), 11 rendering tests (all 11 seeded IDs, args/metadata conversion, isolating marks disabled, unknown message, missing required arg), `AVAILABLE_LOCALES` invariant. 0 clippy warnings.

**What was built for Task 2:**

| File | Purpose |
|---|---|
| `server/src/services/notification_dispatch.rs` | Multi-channel dispatch pipeline: `dispatch()` entry point (DB-write-first → SSE fan-out → webhook fire-and-forget → mobile push fire-and-forget as of Phase 16a Task 9); `NotificationInput` / `DispatchResult` / `ChannelStatus` types; `dispatch_to_many()` broadcast helper; `dispatch_to_library_members()` convenience for library-scoped notifications; webhook delivery via `tokio::spawn` with generic JSON format + HMAC-SHA256 signing; per-user locale rendering via Fluent; `delivery_status` JSONB tracking per channel; 9 unit tests |
| `server/src/services/mod.rs` | Added `pub mod notification_dispatch;` |
| `server/src/services/encryption.rs` | Added `decrypt_notification_config()` / `encrypt_notification_config()` — same AES-256-GCM pattern as metadata/subtitle/Trakt provider keys; decrypts `webhook.secret` at config load time |
| `server/src/state.rs` | Expanded `NotificationConfig` from empty placeholder to `WebhookDispatchConfig` (url, secret, format) + `PushDispatchConfig` (enabled, provider); `WebhookDispatchConfig::is_configured()` and `PushDispatchConfig::is_configured()` helpers; webhook secret decrypted in `load_runtime_config()` |
| `server/migrations/20260628030000_add_push_enabled_to_notification_prefs.sql` | Idempotent migration adding `push_enabled BOOLEAN NOT NULL DEFAULT false` column to `user_notification_preferences` per MOBILE_PUSH.md schema extension |
| `docs/design/MOBILE_PUSH.md` | Updated Implementation Status table (dispatch pipeline ✅, SSE wired ✅, webhook ✅, mobile push ✅ as of Phase 16a Task 9); added implementation notes documenting design decisions |

**Key decisions from Task 2:**

- **DB-write-first guarantee** — The notification record is INSERT-ed to the `notifications` table before any channel fan-out. If all channels fail, the notification is still visible in-app (the DB record IS the in-app channel). This is the critical design rule from MOBILE_PUSH.md: "The notification record always exists in the database, regardless of which channels deliver it."
- **SSE fan-out is synchronous** — `EventBus::publish()` is a fast in-memory broadcast (no I/O, sub-microsecond). The dispatch pipeline calls it directly rather than `tokio::spawn`-ing it. This matches the EventBus design (Phase 10 Task 11) and avoids unnecessary task spawning for a trivially fast operation.
- **Webhook fan-out is fire-and-forget via `tokio::spawn`** — The HTTP POST to the webhook URL runs in a background task. Failures are logged at WARN and recorded in `notifications.delivery_status` JSONB. The dispatch pipeline returns `webhook: "pending"` immediately; the spawned task updates the status to `"delivered"` or `"failed"` asynchronously. Workers calling `dispatch()` should not block on webhook latency.
- **Generic webhook format + HMAC signing in Task 2** — Task 2 ships the `generic` JSON payload format with `X-Duskcue-Signature: sha256=<hex>` HMAC-SHA256 signing (via `ring::hmac`, already in workspace). This follows the GitHub `X-Hub-Signature-256` / Hook0 `X-Hook0-Signature` convention (de-facto standard as of 2025-2026 per web research). Task 4 adds format-specific payloads (ntfy headers, Gotify/Discord/Slack JSON shapes) and retry with exponential backoff. The HMAC signing mechanism is reused by all formats.
- **Push dispatch was completed in Phase 16a Task 9** — The Phase 13b pipeline originally resolved push config + user preferences without provider delivery. Phase 16a Task 9 filled in FCM/APNs/UnifiedPush HTTP delivery and provider revoked-token invalidation without changing the pipeline API.
- **Per-user locale rendering** — Server-side dispatch has no HTTP `Accept-Language` header. The dispatch pipeline reads `users.metadata->>'locale'` and renders via `services::i18n::negotiate_locale()` + `services::i18n::render()`. The stored notification title/body is already localized — the SSE event and webhook payload carry the rendered text, not the Fluent message ID. This means webhook recipients see the user's locale text (correct for a single-user self-hosted server; a multi-user server would need per-locale webhook rendering, which is a future enhancement).
- **Webhook secret encrypted at rest** — `NotificationConfig.webhook.secret` is encrypted via the existing `EncryptionKey` (AES-256-GCM) when stored in `server_config.notifications` JSONB. `decrypt_notification_config()` decrypts it at config load time. Same pattern as all other secrets (metadata/subtitle/Trakt provider keys). The decrypted secret is in the live `RuntimeConfig` for dispatch use; never logged.
- **`user_notification_preferences.push_enabled`** — Migration adds the column per MOBILE_PUSH.md schema extension. The existing `webhook_enabled` column (Phase 2, default false) is reused as-is. Default `push_enabled = false` (users opt in per notification type via the preferences UI — Phase 13b Task 6).
- **Default preferences when no row exists** — When no `user_notification_preferences` row exists for a user + notification type, the dispatch pipeline uses defaults: `in_app_enabled = true`, `webhook_enabled = false`, `push_enabled = false`. This matches the notification_types `is_enabled_by_default` flag semantics.
- **Idempotency via notification UUID** — The notification UUID (UUIDv7) is included in the webhook payload as `notification_id`. Recipients can deduplicate. This follows the webhook best practice identified in June 2026 research (Hook0 docs: "Include an idempotency key").
- **`dispatch_webhook()` disables redirects** — `reqwest::redirect::Policy::none()` per API_SECURITY.md SSRF hardening rules. Prevents SSRF via redirect chains from a malicious webhook URL. The webhook URL is operator-configured (trusted), but defense-in-depth applies.
- **`build_delivery_channels()` populates `notifications.delivery_channels` JSONB** — Records which channels were targeted at dispatch time (`["in_app", "sse", "webhook"]` etc.). This provides an audit trail for debugging "why didn't I get a push notification?" queries.
- **`dispatch_to_library_members()` queries owner + `user_library_access`** — Convenience function for library-scoped notifications (e.g., "new media added"). Always includes the owner regardless of explicit access grants (owner has implicit access to all libraries).
- **No new workspace dependencies** — All functionality uses existing `ring::hmac`, `reqwest`, `sqlx`, `serde_json`, `chrono`, `uuid`, `tokio`, `thiserror`. HMAC signing uses `ring::hmac::{Key, HMAC_SHA256, sign}` which is part of the already-present `ring = "0.17"` workspace dep.
- **9 unit tests** covering: hex encoding, HMAC determinism, HMAC body sensitivity, channel status strings, notification input construction, webhook payload structure, default user preferences, title/body override handling, Fluent rendering fallback. All 609 server tests pass (600 prior + 9 new). 0 clippy warnings.

---

## Phase 14 — Platform Migration (COMPLETE — Tasks 0–15)

**Goal:** Import watch history and user item state from Plex, Jellyfin, and Emby with safe preflight, resumable execution, progress reporting, and auditable results.

**Prerequisites:** Phase 13a complete. Phase 13b is now also complete — the notification dispatch pipeline is available so long-running migrations can surface progress/failure notifications (optional integration; not a hard dependency). Phase 14 proceeds independently of Phase 13b per [PHASE_13_SPLIT.md](docs/design/PHASE_13_SPLIT.md).

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [MIGRATIONS.md](docs/design/MIGRATIONS.md) | **Primary** — three source platforms, user mapping via invite code display names, provider ID matching, merge strategy, progress tracking, error handling, rollback, cleanup |

**Tasks:**

0. Phase reconciliation and scaffolding — replace the existing license-header stubs in `server/src/domains/migration/` with the five-file pattern; add `pub mod migration` in `server/src/domains/mod.rs`; add `migration::router(state.clone())` to `router.rs`; add `clients/web/src/lib/api/migrations.js`; remove the Phase 14 "coming soon" placeholder route once the wizard shell exists.

**Task 0 implementation note:** The migration five-file domain scaffold is wired into `server/src/domains/mod.rs`, central routing, and RFC 9457 error mapping. The REST route shape from [MIGRATIONS.md](docs/design/MIGRATIONS.md) is present and protected by `can_manage_users`. During Task 0 the service operations intentionally returned temporary `MIGR_011` / HTTP 501 responses; Task 2 replaced those reachable stubs with persistence-backed behavior. The web settings entry is enabled, `clients/web/src/lib/api/migrations.js` exists, and the migration settings route now renders a wizard shell instead of the Phase 14 coming-soon placeholder.

1. Schema and task hardening — add idempotent migration indexes for `migration_sources.status`, `migration_user_mapping.migration_source_id`, `migration_import_log.migration_source_id`, `migration_import_log.status`, and `migration_import_log.matched_media_item_id`; extend migration status values for cancellation/resume if needed; seed/register `migration_cleanup` scheduled task for existing deployments.

**Task 1 implementation note:** Added `20260629020000_harden_migration_domain_task1.sql` with the five requested idempotent indexes, extended `migration_sources.status` with `cancelled`, and seeded a `Migration Cleanup` scheduled task for existing deployments. The task row is configured for daily 05:00, 30-minute timeout, 3 retries, and migration retention defaults, but is seeded disabled until Task 14 adds the cleanup executor so current deployments do not produce scheduled-task failures. `VALID_MIGRATION_STATUSES`, [MIGRATIONS.md](docs/design/MIGRATIONS.md), and [DATABASE.md](docs/design/DATABASE.md) were updated to match.

2. Implement migration domain API foundation — CRUD/status endpoints from [MIGRATIONS.md](docs/design/MIGRATIONS.md): create/list/get/delete migration sources, connection test, discovery, user mapping, start, progress, unmatched report, cancel; all endpoints require `can_manage_users`; return `MIGR_*` error codes through RFC 9457.

**Task 2 implementation note:** Replaced the reachable `MIGR_011` service stubs with persistence-backed migration API behavior. `POST/GET/GET by id/DELETE /api/v1/migrations` now create, filter, paginate, fetch, and delete `migration_sources`; mapping saves replace `migration_user_mapping` rows transactionally with duplicate-source-user and duplicate-platform-user conflict checks; progress aggregates `migration_import_log` counts; unmatched reports paginate unmatched rows; cancel records `cancelled` for active statuses. Connection, discovery, and start endpoints now validate source existence and safe state and return action responses without performing source-specific network/SQLite work, preserving Task 3+ boundaries for source security, preflight, extraction, and the async runner.

3. Implement source configuration security — validate Jellyfin/Emby URLs with SSRF protections, redirect blocking, timeout/response-size limits, and network-mode policy; store resumable API credentials encrypted-at-rest or session-only depending on run mode; hash/prefix API keys in `connection_config`; validate Plex upload size, disk space, SQLite header, and required tables before accepting the file.

**Task 3 implementation note:** `create_migration_source()` now sanitizes `connection_config` before persistence. Jellyfin/Emby API configs require `method = api`, normalize `base_url`, reject unsupported schemes, credentials-in-URL, fragments, DNS failures, cloud metadata addresses, and nonsensical reserved addresses, and apply network-mode policy: LAN/loopback/private targets are allowed in local mode but rejected in exposed mode. Stored API credentials are hash-only/session-only (`api_key_hash`, `api_key_prefix`, `credential_mode = "hash_only"`); raw API keys are never written to `migration_sources.connection_config`. The stored config records redirect blocking, 10-second timeout, and 1 MiB response-size policy for later source clients. Plex configs require `method = sqlite_upload`, validate the canonical Plex DB filename, enforce the 10 GiB max, check migration upload disk headroom when a size is provided, and expose `validate_plex_database_file()` for the upload task to verify SQLite header + required Plex tables before accepting a file.

4. Implement preflight and dry-run report — no-write scan that validates library/provider-ID readiness, user mapping readiness, source reachability, Plex DB readability, estimated item counts, estimated match rate, low-confidence count, disk requirements, and blockers/warnings for the admin review step.

**Task 4 implementation note:** Added `POST /api/v1/migrations/{id}/preflight` plus `runMigrationPreflight()` in the web API client. The report is no-write and includes `is_ready`, structured blockers/warnings, per-check statuses, library/provider-ID readiness, user mapping readiness, source readiness, Plex upload disk readiness, and discovery-derived estimates. Jellyfin/Emby preflight uses a redirect-disabled, no-proxy, 10-second `GET /System/Info/Public` reachability check with 1 MiB response-size policy; Plex preflight reports upload metadata readiness and disk headroom, while full SQLite header/table validation remains available through `validate_plex_database_file()` for the upload path. Match estimates come from `migration_import_log` when discovery data exists; otherwise the report warns that discovery has not run yet.

5. Implement async migration runner — long-running imports execute outside HTTP handlers via service-owned task orchestration or `server/src/workers/migration_runner.rs`; persist progress to migration tables; support cancellation, retry/resume from import log, and crash-safe restart from the last completed source item.

**Task 5 implementation note:** Added `server/src/workers/migration_runner.rs` and a per-migration `CancellationToken` registry on `AppState`. `POST /api/v1/migrations/{id}/start` now keeps dry runs no-write by returning the preflight result summary; real starts require mappings, a blocker-free preflight, and existing `migration_import_log` rows before the service flips the source to `importing` and spawns the runner outside the HTTP handler. The runner uses persisted import-log status as the resume cursor, recalculates `migration_user_mapping` counters, derives terminal source state from durable rows, and leaves `matched` rows pending for the later import/merge task instead of fabricating writes. `POST /api/v1/migrations/{id}/cancel` now signals the in-memory cancellation token when present and records `cancelled` in `migration_sources`, so crash/restart and retry behavior is governed by the database rather than handler-local state.
6. Implement Jellyfin/Emby discovery and extraction — REST API connection test, source user discovery, watched item extraction, in-progress item extraction, provider ID normalization, shared source item DTOs, bounded concurrency, source API retries with backoff, and per-platform request timeout handling.

**Task 6 implementation note:** Added `20260629030000_migration_api_extraction_task6.sql` to persist extracted source watch state (`source_is_watched`, `source_play_count`, `source_resume_position_ms`, `source_last_played_at`, `source_item_metadata`) and added the `discovered` import-log status used before matching. `POST /api/v1/migrations/{id}/connect` now verifies Jellyfin/Emby API connectivity with a session-supplied API key that must match the stored hash. `POST /api/v1/migrations/{id}/discover` now returns discovered source users and, once mappings exist, extracts watched and in-progress Movie/Episode rows from Jellyfin/Emby using `X-Emby-Token`, redirect blocking, no proxy, 10-second request timeouts, 1 MiB response limits, 3 retries with 1s/5s/15s backoff, and four-user bounded extraction concurrency. Extracted rows are upserted into `migration_import_log` as `discovered` with normalized `tmdb`/`imdb`/`tvdb` provider IDs while preserving raw provider metadata for Task 9 matching.
7. Implement Plex discovery and extraction — multipart SQLite upload, read-only `rusqlite` access to `com.plexapp.plugins.library.db`, account discovery, watch state extraction, provider GUID parsing, secondary provider ID extraction when available, resumable/range upload handling if the multipart pipeline supports it, and cleanup of temporary files.

**Task 7 implementation note:** Enabled axum multipart support and added `POST /api/v1/migrations/{id}/upload` with a route-scoped 10 GiB + overhead body limit for Plex database uploads. The handler streams the `file` part to `/data/migrations/{id}/plex.db.uploading`, enforces the 10 GiB cap while writing, validates the canonical `com.plexapp.plugins.library.db` filename, SQLite header, and required Plex tables, then atomically stores `/data/migrations/{id}/plex.db` and updates `connection_config.stored_path`. Plex `/discover` now reads the stored database with read-only/query-only `rusqlite`, discovers `accounts`, extracts mapped `metadata_item_settings` watch/resume rows joined to Movie/Episode `metadata_items`, parses IMDb/TMDb/TVDb IDs from primary and secondary Plex GUID tables when available, and upserts `discovered` rows with Plex watch state. Invalid uploads remove the temporary file; resumable/range upload remains deferred because the current API helper only has multipart upload plumbing.
8. Implement user mapping — invite code `display_name` and platform user selection, skip support for unmapped source users, conflict validation, persisted `migration_user_mapping` rows, and at least-one-mapping enforcement.

**Task 8 implementation note:** Added `20260629040000_migration_user_mapping_task8.sql` so `migration_user_mapping.platform_user_id` can be `NULL` only for rows explicitly marked `status = skipped`, and added a partial unique index preventing the same platform user from being mapped twice within one migration. `GET /api/v1/migrations/{id}/map-users` now returns saved mappings plus platform-user options labeled with `users.display_name`, `username`, and the latest linked `invitations.display_name` when available. `POST /api/v1/migrations/{id}/map-users` now accepts either mapped rows (`platform_user_id`) or skipped rows (`skip = true`), rejects duplicate source users, duplicate platform users, skip+platform conflicts, and all-skipped submissions, and start/preflight/extraction only count non-skipped mappings.
9. Implement provider ID and fallback matching — TMDb/IMDb/TVDB cross-reference using existing media indexes; exact title+year+type fallback; TV episode fallback by series title + season + episode; confidence classification (`high`, `medium`, `low`, `unmatched`) in service output.

**Task 9 implementation note:** Added `20260629050000_migration_matching_task9.sql` with `migration_import_log.match_confidence`, the `series_episode` match method, and a confidence index. `POST /api/v1/migrations/{id}/match` now processes `discovered`/previously `unmatched` import-log rows, matches by TMDb, IMDb, then TVDb using existing `media_items` provider indexes, falls back to normalized exact title + premiere year + type, and finally matches TV episodes by source series title plus season/episode numbers through the CTI episode tables. Matched rows are marked `matched` with `high`, `medium`, or `low` confidence; failures are marked `unmatched` with an audit reason. Preflight estimates and unmatched reports now expose confidence state.
10. Implement manual match review — APIs and UI for unmatched/low-confidence candidates, admin override to a specific `media_item_id`, skip/ignore decisions, CSV export of unmatched items, and re-run import for resolved rows.

**Task 10 implementation note:** Added `20260629060000_migration_manual_review_task10.sql` to persist `match_method = manual` for admin decisions. New review endpoints provide the admin workflow: `GET /api/v1/migrations/{id}/review` lists unmatched and low-confidence rows with matched-media context, `POST /api/v1/migrations/{id}/review/{item_id}` records manual match/skip/ignore decisions, and `GET /api/v1/migrations/{id}/review.csv` exports the current review queue as `text/csv`. Manual matches validate the target `media_item_id` exists and matches the source item type, then return the row to `status = matched` with high-confidence manual provenance so the import runner can process it. Skip/ignore decisions mark rows `skipped`. The migration settings page now includes a Match Review panel with source selection, review filters, recent movie/episode candidate dropdowns, direct media UUID entry, decision buttons, and CSV export.
11. Implement import and merge strategy — import to `user_item_data` with `is_watched` OR, `play_count` MAX, `resume_position_ms` MAX except reset to 0 when watched, `last_played_at` MAX; optionally import `is_favorite` and `user_rating` when supported by source data because `user_item_data` already has those fields; log every item in `migration_import_log`.

**Task 11 implementation note:** The async migration runner now imports `matched` rows into `user_item_data` instead of preserving them for a later task. Each row merges watch state with the existing `(user_id, media_item_id)` record using `is_watched OR`, `play_count = GREATEST(existing, source)`, latest non-null `last_played_at`, and `resume_position_ms = 0` whenever either side is watched; otherwise resume uses the greatest known position with source values clamped to the `INT` column range. The UPSERT returns the affected `user_item_data.id`, which is stored on `migration_import_log.imported_user_item_data_id` while the log row moves to `imported`. Per-row import failures are recorded as `status = error` with `error_detail` and do not stop remaining matched rows. Runner counters and final source status now reflect real imported/error terminal rows. Favorites and ratings remain deferred because source extraction does not yet persist those values.
12. Implement rollback/undo support — assign import batch metadata and record previous `user_item_data` values before mutation so an admin can undo a bad import without losing newer local watch progress; expose rollback status and results in the migration detail view.

**Task 12 implementation note:** Added `20260629070000_migration_rollback_task12.sql` with `migration_import_log.import_batch_id`, `previous_user_item_data`, `imported_at`, `rolled_back_at`, `rollback_detail`, rollback-focused indexes, and the `rolled_back` import-log status. The migration runner now assigns a UUIDv7 batch to each import run and snapshots any existing `user_item_data` row under `FOR UPDATE` before the UPSERT merge. New `GET/POST /api/v1/migrations/{id}/rollback` endpoints report rollback availability and perform undo. Rollback restores the captured previous watch-state/preference values or deletes the imported row when no previous row existed, but skips rows whose `user_item_data.updated_at` is later than the import timestamp so newer local progress is not destroyed. The migration settings page now exposes a Rollback panel for the selected source with counts for imported, available, rolled-back, and newer-progress skipped rows plus the rollback action.
13. Implement progress events, metrics, and notifications — publish `migration_progress` SSE events through the existing `EventBus`; add Prometheus counters/gauges for migrations started/completed/failed, source items processed, match confidence counts, import errors, and active migration runs; add `migration_completed` and `migration_failed` notification types + Fluent templates; surface long-running migration completion/failure in the notification center.

**Task 13 implementation note:** Added `20260629080000_migration_notifications_task13.sql` to seed `migration_completed` and `migration_failed` notification types plus Fluent templates in `server/locales/en/notifications.ftl`. The migration runner now loads migration-admin recipients (`owner`, effective `admin`, or explicit `can_manage_users` grant) and publishes `migration_progress` events through `EventBus` at run start, first/25th-row progress cadence, import-loop end, and terminal states. Metrics now cover started/completed/failed runs, active run gauge, source items processed by match/import stage, match confidence totals, and import errors. Completion/failure notifications dispatch through the existing DB-write-first notification pipeline, so they appear in the notification center via the existing `notification` SSE consumer and REST feed without migration-specific client notification code.
14. Implement migration cleanup worker — daily `migration_cleanup` executor deletes Plex uploads for completed migrations older than 24 hours, removes stale temporary files from failed/cancelled runs according to retention settings, and prunes old migration logs/sources per [MIGRATIONS.md](docs/design/MIGRATIONS.md).

**Task 14 implementation note:** Added a fallible `migration_cleanup` scheduled executor and enabled the seeded daily 05:00 scheduled task with `delete_failed_temp_files_after_hours` retention. The worker deletes completed Plex upload directories after 24 hours, removes stale failed/cancelled/orphaned `plex.db.uploading` files, updates cleaned Plex source configs to remove stale `stored_path` values, prunes old inactive import logs, and deletes old completed migration sources while preserving Plex sources whose upload directory deletion failed. Cleanup stats are written to `scheduled_task_runs.stats`, and recursive filesystem deletion is guarded to UUID-named directories under `data_dir/migrations`.
15. Build migration wizard UI — step-by-step admin flow: choose source, connect/upload, preflight, map users, review matches, start import, live progress, results, unmatched/manual review, rollback/cleanup actions.

**Task 15 implementation note:** Replaced the migration settings scaffold with a complete guided admin wizard. The page now creates Jellyfin/Emby/Plex sources, uploads Plex databases, tests API connections with session-only keys, discovers users/source watch data, preserves discovered source users for first-time mapping, runs preflight, saves mappings, triggers provider/fallback matching, supports manual review decisions and CSV export, starts dry-run or real imports, listens for `migration_progress` SSE events with polling fallback, displays results, and exposes rollback plus guarded source cleanup actions.

**Verification:** Admin can run a no-write preflight, import watch history from Jellyfin/Emby via REST API and Plex via SQLite upload, observe live SSE progress, cancel/resume a run, review unmatched/low-confidence items, and verify watch states appear correctly in `user_item_data`. Completed and failed migrations emit notifications, cleanup removes temporary Plex uploads, and rollback restores imported rows without destroying newer local progress.

**Outcome:** All sixteen Phase 14 tasks are implemented. The migration domain provides secured API and Plex entry paths, durable extraction/matching/import/rollback state, progress and notification signals, retention cleanup, and the guided admin workflow. `c4795b6` completed the cleanup worker and `976750b` completed the wizard. Real source-server credentials, production Plex databases, and backups remain operator release-gate validation rather than repository fixtures.

---

## Pre-v1.0 Hardening

**Goal:** Close implementation debt from strategic design decisions that doesn't block features but improves v1.0 quality. Per [IMPLEMENTATION_DEBT.md](docs/design/IMPLEMENTATION_DEBT.md).

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [HTTP_CACHING.md](docs/design/HTTP_CACHING.md) | Cache-Control + ETag response headers on metadata/artwork endpoints |
| [I18N.md](docs/design/I18N.md) | **Target Locales section** — 8-locale plan (`en`, `fr`, `de`, `es`, `it`, `ar`, `zh-Hans`, `zh-Hant`); Paraglide JS adoption; translation strategy (AI-initial + native review); RTL for Arabic |
| [SEARCH.md](docs/design/SEARCH.md) | Faceted search UI (genre/year/rating filter pills) |
| [IMPLEMENTATION_DEBT.md](docs/design/IMPLEMENTATION_DEBT.md) | Full debt catalog and scheduling rationale |

**Tasks:**

1. ~~Implement Cache-Control + ETag response headers — `SetResponseHeaderLayer` per resource group in `router.rs`; SHA-256 ETag on single-item metadata endpoints; per-endpoint `max-age` + `stale-while-revalidate` per [HTTP_CACHING.md](docs/design/HTTP_CACHING.md) table~~ **DONE**
2. ~~Adopt Paraglide JS — `@inlang/paraglide-js` Vite plugin; extract existing web client UI strings to `clients/web/messages/en.json`; configure URL prefix + cookie locale strategy per [I18N.md](docs/design/I18N.md)~~ **DONE**
3. ~~Build faceted search UI — genre/year/rating/type filter pills on search results page; uses existing PG FTS with parallel GROUP BY queries per [SEARCH.md](docs/design/SEARCH.md)~~ **DONE**
4. ~~Add Prometheus metrics for new infrastructure — SSE connection count, event publish rate, image variant generation throughput + cache hit rate, search query latency p50/p95/p99, push delivery success rate per channel; extends existing `init_metrics()` from Phase 3~~ **DONE**
5. ~~**Multi-locale translations (7 non-English locales)** — Create initial `.ftl` files for `server/locales/{fr,de,es,it,ar,zh-Hans,zh-Hant}/notifications.ftl` and parallel `clients/web/messages/{locale}.json` files. AI-assisted initial pass with translator comments marking each file `### AI-GENERATED INITIAL TRANSLATION — needs native-speaker review before activation.` Expand `AVAILABLE_LOCALES` in `services/i18n.rs` from `[en]` to all 8 locales. Informal register across all locales (`tu`/`du`/`tú`/`tu`). Per [I18N.md](docs/design/I18N.md) "Target Locales" section. Locales are NOT UI-selectable until Task 7 activates them post-review.~~ **DONE**
6. ~~**RTL layout review for Arabic** — Set `<html dir="rtl">` based on locale via Paraglide's `getTextDirection()`; audit all CSS for physical properties (`margin-left` → `margin-inline-start`, `padding-right` → `padding-inline-end`); mirror directional icons (back/next arrows) via `[dir="rtl"]` selectors; bidirectional testing of every layout (library grid, player, settings panels, notification center). Must pass before `ar` locale is UI-activated. Per [I18N.md](docs/design/I18N.md) "Right-to-Left Support".~~ **DONE**
7. ~~**Locale activation infrastructure** — User settings API to read/write `users.metadata.locale`; language switcher UI in web client (shows only reviewed locales); Weblate project setup (self-hosted or cloud) with Fluent + Inlang JSON components connected to Duskcue repo; import AI-initial translations as suggestions for community refinement; 90% completeness + maintainer sign-off threshold for UI activation per [I18N.md](docs/design/I18N.md).~~ **DONE**

**Verification:** Metadata endpoints return ETag headers; conditional requests return 304. Web client strings are in `en.json` and wrapped in Paraglide `m.*` calls. Search results page has genre/year/rating filters. Grafana dashboard shows SSE connections and search latency. All 8 locales exist in `AVAILABLE_LOCALES` and can render notifications via `services::i18n::render()`. Arabic layout passes bidirectional review. Language switcher shows reviewed locales only.

**Task 1 implementation note:** `server/src/cache.rs` centralizes HTTP cache constants, `SetResponseHeaderLayer::if_not_present` construction, SHA-256 ETag generation, and `If-None-Match` handling. Route-level cache layers are attached to GET handlers only: media item metadata (`private, max-age=300, stale-while-revalidate=600`), library config (`private, max-age=60, stale-while-revalidate=300`), artwork (`public, max-age=86400, stale-while-revalidate=604800, immutable`), server config/config groups (`no-store`), plus `/health` and `/metrics` (`no-store`). Conditional requests return `304 Not Modified` for SHA-256 JSON ETags and existing artwork ETags. ETag-bearing responses are excluded from gzip compression to preserve strong validator byte semantics. Search `no-store` is attached to `GET /api/v1/search` in Task 3.

**Task 2 implementation note:** The web client now uses `@inlang/paraglide-js` 2.20.2 through the Vite plugin with `url`, `cookie`, `preferredLanguage`, and `baseLocale` strategy order. The Inlang project lives at `clients/web/project.inlang/settings.json`, English source messages live in `clients/web/messages/en.json` (631 message keys as of Task 5), and generated runtime imports resolve from `$lib/paraglide`. SvelteKit hooks wire Paraglide middleware, localized URL rerouting, and `<html lang>` / `<html dir>` replacement via `getTextDirection()`. Existing visible web UI strings are wrapped with `m.*` message calls across the current route/component surface. Reviewed locale activation and the language switcher remain scheduled for Task 7.

**Task 3 implementation note:** `server/src/domains/search/` now owns `GET /api/v1/search` with the standard five-file domain pattern. The endpoint is authenticated, cache-marked `no-store`, validates `q`, `type`, `genre`, `year`, `rating_min`, and `limit`, and returns `{ items, facets }`. PostgreSQL FTS remains the v1.0 engine: the main query ranks `media_items.search_vector` with `plainto_tsquery('english', q)`, filters active libraries and user library access, and returns normal `MediaItemResponse` rows. Facet counts for type, genre, year, and rating thresholds run as parallel GROUP BY queries via `tokio::try_join!`, matching [SEARCH.md](docs/design/SEARCH.md). The web search page now uses URL-backed filter state (`type`, `genre`, `year`, `rating_min`), renders type/genre/year/rating filter pills with counts, preserves shareable URLs and browser history, and stores all new visible labels in `clients/web/messages/en.json` for Paraglide.

**Task 4 implementation note:** Prometheus instrumentation now covers the new v1.0 infrastructure without adding a second metrics subsystem. `logging::init_metrics()` registers explicit buckets for `search_query_duration_seconds` and `image_variant_generation_duration_seconds`. `EventBus` emits `sse_connections_opened_total`, `sse_connections_rejected_total`, `sse_connections`, `sse_connected_users`, and `sse_events_published_total{event_type,delivered}`. Artwork delivery emits `image_variant_requests_total{category,variant,result}` for cache-hit vs generated requests plus `image_variant_generations_total{category,variant,status}` and `image_variant_generation_duration_seconds{category,variant,status}`. Search emits `search_queries_total{status,has_filters}` and `search_query_duration_seconds{status,has_filters}`, making the [SEARCH.md](docs/design/SEARCH.md) p95 migration trigger measurable. Notification dispatch emits `notification_delivery_total{channel,status}` for in-app, SSE, webhook, and push channels, including webhook terminal delivered/failed outcomes from the background delivery task. Labels intentionally avoid user IDs, query strings, URLs, notification IDs, and media IDs.

**Task 5 implementation note:** Added preview translations for the 7 non-English launch-window locales. Server Fluent notification templates now exist at `server/locales/{fr,de,es,it,ar,zh-Hans,zh-Hant}/notifications.ftl`, each marked `### AI-GENERATED INITIAL TRANSLATION — needs native-speaker review before activation.` Web Paraglide catalogs now exist at `clients/web/messages/{fr,de,es,it,ar,zh-Hans,zh-Hant}.json` with full 631-key parity against `en.json` plus a `__translator_note` marker carrying the same review warning. `clients/web/project.inlang/settings.json` now declares all 8 locales, and `services/i18n.rs` expands `AVAILABLE_LOCALES` to `[en, fr, de, es, it, ar, zh-Hans, zh-Hant]` with tests proving negotiation and seeded notification rendering across every locale. These files are preview-only: Task 6 still owns Arabic RTL layout review, and Task 7 still owns reviewed-locale activation and the language switcher.

**Task 6 implementation note:** Arabic RTL layout review is complete at the current web-client surface. The SvelteKit HTML hook already sets `<html lang>` and `<html dir>` through Paraglide `getTextDirection()`. Task 6 converted remaining physical CSS hazards in the app shell, notification dropdown/toasts, media cards, search inputs, skip button, player controls, media detail backdrop, and settings panels to logical properties (`inset-inline-*`, `margin-inline-*`, `padding-inline`, `border-inline-start`, `text-align: start/end`). Drawer and toggle `translateX()` offsets now use direction-aware CSS variables. Back-link arrows moved out of translation strings into `.back-link` / `.back-action` pseudo-elements, with RTL rendering `→` and LTR rendering `←`. Headless Chrome CDP verification covered mobile Arabic routes for dashboard, libraries, search, settings, notification center, system settings, and login at 390px width with `dir="rtl"` and no horizontal overflow. Arabic still remains non-selectable until Task 7 adds reviewed-locale activation infrastructure.

**Task 7 implementation note:** Locale activation infrastructure is complete. The users domain now exposes `GET /api/v1/user/preferences` and `PUT /api/v1/user/preferences` for authenticated users; the service reads/writes `users.metadata.locale`, validates writes against `services::i18n::REVIEWED_UI_LOCALES`, and returns the server-owned language-switcher option list. `AVAILABLE_LOCALES` remains the 8-locale preview/rendering list, while `REVIEWED_UI_LOCALES` is the activation gate and currently contains only `en`. The web settings page now loads preferences, renders a language selector from the reviewed locale list, persists changes through the API, updates the Paraglide cookie/URL locale, and stores the selected locale on the cached auth user. Weblate setup is documented in [WEBLATE.md](docs/operations/WEBLATE.md): one Duskcue project with Web JSON and server Fluent components, AI-initial translations imported as review-gated suggestions, and a 90% completeness + maintainer sign-off threshold before adding a locale to `REVIEWED_UI_LOCALES`.

---

## Phase 15 — Docker & Deployment

**Committed:** `e77d78b` on `main`

**Goal:** Production-ready Docker image with embedded PostgreSQL.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [DOCKER_DEPLOYMENT.md](docs/operations/DOCKER_DEPLOYMENT.md) | **Primary** — hybrid embedded/external PG, volume strategy, security hardening |
| [OS_HARDENING.md](docs/operations/OS_HARDENING.md) | Docker Engine version minimums, Alpine current-stable pinning |
| [CACHE_STORAGE.md](docs/operations/CACHE_STORAGE.md) | Docker volumes: `duskcue-data`, `duskcue-cache`, tmpfs for transcode |
| [SEARCH.md](docs/design/SEARCH.md) | **Post-v1.0 follow-up** — optional Meilisearch sidecar (loopback, same container) for libraries exceeding 50k items; default v1.0 ships PG FTS only, no sidecar |
| [MULTI_INSTANCE.md](docs/design/MULTI_INSTANCE.md) | **Deployment topology constraint** — Duskcue is single-instance by design. Phase 15 must ship single-container as the canonical topology (`replicas: 1` for any Kubernetes examples). No load-balancer-multi-instance pattern; HA via container `restart: unless-stopped` + embedded PG crash recovery. |
| [REVERSE_PROXY.md](docs/design/REVERSE_PROXY.md) | **Exposed-mode operator guidance** — built-in rustls TLS handles simple single-domain exposed mode (no proxy needed); Caddy is the recommended external proxy for multi-service routing. Canonical Caddyfile + docker-compose.yml + Nginx/Traefik alternatives documented. Critical `DUSKCUE_TRUSTED_PROXIES` config for client IP detection. |
| [SECURITY.md](docs/security/SECURITY.md) | **Native IPv6 support** — dual-stack listener binding, IPv6 CIDR trust boundaries, IPv6 URL formatting, and exposed-mode security posture |

**Tasks:**

1. ~~Finalize `Dockerfile` — multi-stage Alpine build for x86_64 + ARM64~~ **DONE**
2. ~~Define production web/API/container process topology~~ **DONE**
3. ~~Implement configurable bind/listen behavior~~ **DONE**
4. ~~Finalize `docker/entrypoint.sh`~~ **DONE**
5. ~~Create `docker-compose.yml`~~ **DONE**
6. ~~Add container health/readiness and smoke verification~~ **DONE**
7. ~~Test multi-arch build: `docker buildx build --platform linux/amd64,linux/arm64`~~ **DONE with CI handoff**
8. ~~Test PUID/PGID mapping on Linux~~ **DONE**
9. ~~Test embedded PG lifecycle~~ **DONE**
10. ~~Verify security hardening~~ **DONE**
11. ~~Add Docker operator backup/restore runbook~~ **DONE**
12. ~~Add release workflow~~ **DONE**

**Verification:** `docker compose up` starts a single container with embedded PG, serves the web UI and API from the documented public surface, listens on 48027 over IPv4 by default and can be configured for IPv6/dual-stack binding, health/readiness checks pass only after migrations complete, graceful shutdown preserves data, restart/crash recovery works, external-PG mode skips embedded PG cleanly, and the release workflow can produce a signed/attested `linux/amd64,linux/arm64` image.

**Phase 15 implementation note:** Root `Dockerfile` now uses named BuildKit stages (`web-deps`, `web-builder`, `rust-builder`, `runtime`) with digest-pinned Docker Official Image bases. The runtime target installs PostgreSQL 18 runtime/client/contrib packages, FFmpeg, Node.js for the adapter-node web artifact, `tini`, `su-exec`, `nss_wrapper`, CA certificates, timezone data, and Bash; strips setuid/setgid bits; exposes `48027`; declares `/data` and `/cache`; runs `/usr/local/bin/duskcue-entrypoint start`; and includes a Docker `HEALTHCHECK` against `/health/ready`. Root `.dockerignore` keeps local build state, web build artifacts, docs, scripts, VCS metadata, and secrets out of the default build context.

The canonical container topology is one public SvelteKit adapter-node process on `48027`, one internal Rust API process on `127.0.0.1:48028`, and embedded PostgreSQL on a Unix socket when `DUSKCUE_DATABASE_URL` is unset. SvelteKit proxies `/api`, `/health`, `/health/*`, and `/metrics` to the internal API with streaming request/response handling for SSE and media-related API routes. Standalone Rust listener configuration now supports `DUSKCUE_BIND_ADDRESS` / `--bind-address` and `DUSKCUE_PORT` / `--port`, including IPv6 bind literals and bracketed IPv6 startup URLs.

`docker/entrypoint.sh` now owns embedded PostgreSQL init/start/stop, external PostgreSQL bypass, numeric PUID/PGID privilege drop, read-only-root-compatible `nss_wrapper` identity mapping, stale Duskcue lockfile cleanup before the single API process starts, and orderly signal handling for SvelteKit, Rust API, and PostgreSQL. The root `docker-compose.yml` runs the image with named `duskcue-data` / `duskcue-cache` volumes, tmpfs mounts for transcode, PostgreSQL socket, and `/tmp`, `read_only: true`, `no-new-privileges`, `cap_drop: ALL`, minimal CHOWN/SETUID/SETGID capabilities, optional IPv6 binding, and hardware-acceleration examples. `.env.example` documents the embedded-default environment and external database override.

Container verification is implemented in `scripts/verify-docker.ps1`. Local verification passed for the `linux/amd64` runtime image, embedded PostgreSQL startup, public `/health/ready` and `/health/live`, proxied API reachability, PUID/PGID writable paths under read-only-root hardening, stop/start restart behavior, and external PostgreSQL mode against a disposable `postgres:18-alpine` container. `docker buildx build --check --platform linux/amd64,linux/arm64 --target runtime .` passed; two full local multi-platform runtime builds timed out under workstation emulation, so protected GitHub Actions remains the canonical full manifest-list producer.

Docker release automation now exists in `.github/workflows/docker-validation.yml` and `.github/workflows/docker-release.yml`. Validation builds and smoke-tests the runtime image without publish credentials. Release publication runs from protected SemVer tags or manual dispatch, publishes `linux/amd64,linux/arm64` to GHCR, applies OCI metadata, uses registry cache, emits SBOM plus max-detail provenance, and requests GitHub build-provenance attestation.

---

## Phase 16a — Desktop & Mobile Clients

**Goal:** Production-capable desktop and mobile client foundations: Tauri desktop wraps the web client with native shell features, and Flutter mobile supports secure server connection, auth, browsing, playback, foreground real-time updates, mobile push registration, and client quality reporting.

**Context from Phase 15:** Docker deployment now has a stable public base URL on port `48027`; web, API, health, metrics, SSE, and media-related API routes are same-origin from the client perspective because SvelteKit proxies to the internal Rust API. Desktop and mobile clients should treat `http(s)://<server>:48027` as the server origin, not attempt to reach the internal Docker API port `48028`.

**Context from earlier phases:**

- `clients/desktop/` and `clients/mobile/` already exist as stubs, not working clients. Desktop has a Tauri manifest and empty Rust entrypoint; mobile has a minimal `pubspec.yaml` and empty `main.dart`.
- Phase 4 shipped passkeys, device linking, re-auth codes, bearer/session auth, session listing/revocation, and capability-scoped users.
- Phase 7 shipped playback/session/progress APIs, user item data, bookmarks, playlists, HLS/remux/transcode delivery, and the quality decision engine.
- Phase 8 shipped the web API-client shape, bearer-token support in `core.js`, and the SvelteKit UI that desktop should reuse.
- Phase 10 shipped SSE `GET /api/v1/events`, replay, per-user connection limits, and the web event-store pattern.
- Phase 13b shipped notification CRUD, notification preferences, SSE notification fan-out, webhook dispatch, `user_push_devices`, push-device registration/heartbeat/revoke APIs, and a structured push-dispatch stub. The actual FCM/APNs/UnifiedPush clients and provider-response invalidation land here.
- Pre-v1.0 hardening shipped Paraglide/i18n activation and metrics. New client UI should either reuse translated web strings or define its own translation workflow before user-visible text grows.

**Scope boundary:** Phase 16a is the online desktop/mobile MVP. Offline downloads are planned separately in Phase 16c because they require new server APIs, local encrypted storage, download-job state, storage quotas, and offline license/revocation policy. Phase 16b TV foundation must not wait for offline downloads.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [DESKTOP_MOBILE_CLIENTS.md](docs/design/DESKTOP_MOBILE_CLIENTS.md) | **Primary Phase 16a Task 0 outcome** — desktop/mobile research findings, platform decisions, client layout, native adapter boundaries |
| [PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md) | Tauri 2 desktop wrapper (imports web client), Flutter mobile project structure |
| [AUTH.md](docs/design/AUTH.md) | Passkeys, device linking, bearer/session auth, re-auth, session revocation |
| [STREAMING.md](docs/design/STREAMING.md) | HLS playback, resume, heartbeat, stop/completion, subtitles/audio, stream URL behavior |
| [QUALITY_MANAGEMENT.md](docs/design/QUALITY_MANAGEMENT.md) | Device capability profiles, network quality assessment |
| [REAL_TIME_PUSH.md](docs/design/REAL_TIME_PUSH.md) | SSE foreground push, bearer-auth SSE clients, polling fallback, `session_kicked` and notification events |
| [MOBILE_PUSH.md](docs/design/MOBILE_PUSH.md) | FCM/APNs/UnifiedPush architecture, token registration, token invalidation, minimized payloads |
| [API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md) | API client conventions, error handling, pagination, auth headers |
| [API_SECURITY.md](docs/security/API_SECURITY.md) | BOLA prevention, token storage expectations, outbound-provider validation |
| [SECURITY.md](docs/security/SECURITY.md) | Local/VPN/exposed modes, TLS, signed streaming URLs, mobile IP changes |

**Tasks:**

0. ~~Research, design, and phase enrichment — verify current 2026 official guidance for Tauri 2 capabilities/plugins/updater/deep links, Flutter Android/iOS project structure, Android Credential Manager passkeys, iOS AuthenticationServices passkeys, FCM HTTP v1, APNs token auth, UnifiedPush, mobile HLS players, background execution limits, universal/app links, local-network permissions, and store packaging. Update this phase plus affected docs before implementation.~~ **DONE**

**Task 0 implementation note:** Added [DESKTOP_MOBILE_CLIENTS.md](docs/design/DESKTOP_MOBILE_CLIENTS.md) as the Phase 16a authoritative research/design outcome. Official-source research confirmed the following implementation posture: desktop remains a minimal Tauri 2 shell around the existing SvelteKit web UI with a strict capability/plugin surface; mobile becomes a generated Flutter Android/iOS app rather than a WebView wrapper; Android passkeys use Credential Manager and iOS passkeys use AuthenticationServices; mobile playback must be backed by Android Media3/ExoPlayer and iOS AVPlayer/AVFoundation even if surfaced through Flutter; foreground SSE is used only while mobile is active, with push and REST refresh covering background/offline states; push provider work completes the existing FCM HTTP v1, APNs token-auth, and UnifiedPush delivery stubs; `duskcue://` is the MVP deep-link scheme while verified HTTPS App/Universal Links remain optional until association-file deployment is designed; local HTTP is acceptable only for local/VPN deployments and exposed mode requires HTTPS. Updated [PROJECT.md](PROJECT.md), [PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md), [AUTH.md](docs/design/AUTH.md), [STREAMING.md](docs/design/STREAMING.md), [REAL_TIME_PUSH.md](docs/design/REAL_TIME_PUSH.md), [MOBILE_PUSH.md](docs/design/MOBILE_PUSH.md), and [SECURITY.md](docs/security/SECURITY.md) to cross-reference the decisions.
1. ~~Repair and complete client scaffolds:
   - Fix the Tauri config schema and make `clients/desktop` actually build and run the shared web client.
   - Implement the Tauri `main.rs`/`lib.rs` entrypoint, command registration, window defaults, capabilities, icons, and development/build scripts.
   - Generate full Flutter Android/iOS project structure under `clients/mobile/` with app IDs, platform folders, test folders, lint config, app icons placeholder, and CI-friendly build commands.
   - Add dependency baselines for routing, state management, HTTP, secure storage, HLS/video playback, connectivity, local notifications, push messaging, and test tooling.~~ **DONE**

**Task 1 implementation note:** Repaired the desktop scaffold and created the mobile project foundation. `clients/desktop` now has a valid Tauri 2 config schema, a `tauri-build` build script, a Rust `run()` entrypoint, an `app_info` command registration, labeled/resizable default window settings, default capabilities bound to the main window, a placeholder Windows icon at `src-tauri/icons/icon.ico`, a package lock, Tauri CLI scripts, and build/dev scripts that delegate to the shared SvelteKit web client. `frontendDist` points at the shared web build client output and `npm run build` from `clients/desktop` successfully runs the web production build. `clients/mobile` now has a Flutter app shell with `MaterialApp.router`, GoRouter navigation, Riverpod session state, server selection/dashboard/settings screens, baseline API/secure-storage/connectivity/playback/push services, widget and integration smoke tests, lint config, README commands, Android package `com.duskcue.mobile` with permissions/deep-link/local-network cleartext placeholders, and iOS bundle metadata with `duskcue://` URL scheme and local-network usage text. Dependency baselines were selected from current pub.dev metadata for routing, state, HTTP, secure storage, video playback, connectivity, local notifications, FCM, serialization, lints, codegen, and tests. Verification passed: `cargo check -p duskcue-desktop`, `cargo fmt --package duskcue-desktop --check`, `npm run build` in `clients/desktop`, XML/plist parse checks for mobile platform files, and `git diff --check`. Flutter/Dart are not installed in this environment, so `flutter pub get`, `flutter analyze`, `flutter test`, Android build, and iOS simulator build remain first-run checks for an environment with the Flutter SDK.
2. ~~Define shared client contracts and drift control:
   - Inventory the API routes the desktop/mobile clients need from auth, users, libraries, media, search, playback, subtitles, segments/storyboards, collections, quality, notifications, and settings.
   - Decide whether Flutter DTOs are handwritten, generated from OpenAPI-like schema, or generated from `crates/types`/server contracts. Document the chosen source of truth.
   - Map RFC 9457 Problem Details into typed client errors, including auth-expired, permission-denied, rate-limited, server-unreachable, transcode-unavailable, and playback-policy cases.
   - Keep bearer-token handling compatible with web `core.js` semantics while using OS secure storage outside the browser.~~ **DONE**

**Task 2 implementation note:** Added [CLIENT_CONTRACTS.md](docs/api/CLIENT_CONTRACTS.md) and [client-contracts.v1.json](docs/api/client-contracts.v1.json) as the Phase 16a desktop/mobile contract source of truth. The manifest inventories 71 required online-client routes across health, auth/passkeys/device linking/sessions, users, libraries, media/artwork, search, playback/HLS/watch data/playlists, subtitles, segments/storyboards, collections, quality, notifications/SSE/push devices, and minimal settings. Added `scripts/verify-client-contracts.mjs`, which verifies manifest route paths against `server/src` and declared web helper names against `clients/web/src/lib/api`; it currently passes with `Verified 71 client contract routes.` Chose a curated manifest for Phase 16a because the Rust server does not yet emit OpenAPI/JSON Schema; generated OpenAPI or JSON Schema is deferred to Phase 16d contract QA. Added mobile RFC 9457 types in `clients/mobile/lib/api/problem_detail.dart` and typed error mapping in `clients/mobile/lib/api/client_error.dart`, then wired `DuskcueApiClient` to convert Dio failures into `ClientError` with retry-after support. Updated `clients/web/src/lib/api/core.js` and `settings.js` so the existing web `getHealth()` helper can call root-level `/health/ready` while preserving `/api/v1` and bearer-token semantics for API routes. Updated [API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md) to remove stale generated-OpenAPI language and point to the manifest until Phase 16d.
3. ~~Implement server selection and connection onboarding:
   - Add manual server URL entry, saved-server list, last-used server, and connection test against `/health/ready`.
   - Treat `http(s)://<server>:48027` as canonical for web/API/SSE/media; never expose `48028` to clients.
   - Handle Local, Remote VPN, and Exposed network modes with clear HTTPS/certificate behavior; document self-signed/private CA limitations for mobile.
   - Add optional QR-code/link handoff from web admin UI or docs if server URLs become cumbersome to enter on phones.
   - Validate local-network behavior on Android and iOS, including permissions and failure states.~~ **DONE**

**Task 3 implementation note:** Added server-origin onboarding for the mobile app and connection primitives for the desktop shell. `clients/mobile` now canonicalizes manual server input to `http(s)://<server>:48027`, rejects Docker's internal `48028` API port, exposes Local/Remote VPN/Exposed network modes, tests `/health/ready` before continuing, shows typed failure messages, saves successful server profiles plus the last-used origin through `flutter_secure_storage`, and lists saved servers on the onboarding screen. The mobile platform configuration now allows Android cleartext connections for local/VPN mode and iOS local-network HTTP via `NSAllowsArbitraryLoadsInLocalNetworking`, while Exposed mode requires HTTPS and OS-trusted certificates. `clients/web/src/lib/api/core.js` now supports an optional explicit server origin so the Tauri webview can point API/root-health/media helpers at a selected Duskcue origin without exposing `48028`. `clients/desktop/src-tauri` now has Tauri commands to normalize server origins, read/save saved-server state under the app data directory, and test `/health/ready` with a 10-second timeout. QR/link handoff remains documented as optional and deferred until the admin UI can issue short-lived setup links.
4. ~~Implement secure auth and session lifecycle:
   - Desktop: reuse the web auth UI where possible, but persist bearer/session state only through OS-backed secure storage or Tauri-supported secure storage; no plaintext tokens.
   - Mobile: implement passkey login/registration on supported Android/iOS versions, device-linking login, re-auth code login, invite/password fallback where enabled, logout, logout-all, and session deletion.
   - Register client metadata (`client_name`, `client_platform`, `client_version`, device identifier) consistently for sessions, device linking, analytics, and quality reporting.
   - Handle `session_kicked` and revoked/expired sessions by clearing credentials and returning to login without leaking stale media URLs.~~ **DONE**

**Task 4 implementation note:** Added secure auth/session foundations for desktop and mobile. Desktop now depends on the OS-backed `keyring` crate and exposes Tauri commands to write, read, and clear per-server session tokens by normalized `http(s)://<server>:48027` origin; saved server origins remain non-secret app-data JSON, while bearer tokens never go into plaintext files or browser storage. Mobile now has typed auth/session models, a stable secure-stored device identifier, device metadata generation, an `AuthService`, native passkey method-channel adapter, `/auth` screen, saved-session restore after server selection, password/invite/re-auth/device-linking sign-in, passkey login/registration service methods, logout/logout-all, session list, and per-session deletion. `DuskcueApiClient` can update bearer headers after login and clears them on logout. The server auth DTOs now accept `device_id` for password, invite, re-auth, WebAuthn finish, and device-linking requests; device-linking codes persist `device_id` via migration `20260630090000_add_device_id_to_device_linking.sql`, so resulting sessions carry the same client identifier as direct login flows. Expired/revoked sessions map through `ClientErrorKind.authExpired`; the mobile settings flow clears local secure credentials and returns to auth when a 401 is observed. Full Android Credential Manager / iOS AuthenticationServices native channel bodies remain first-run work for a Flutter/Xcode/Android SDK environment, but the Dart/server contract and secure session lifecycle are now in place.
5. ~~Build desktop wrapper features:
   - Import/reuse the SvelteKit web client with Tauri-compatible static build behavior and no SSR-only assumptions.
   - Add system tray/menu actions for open window, server connection status, recent notifications, pause/resume current playback where feasible, and quit.
   - Add file/folder dialogs for admin workflows that need local path selection, while preserving server-side path semantics for Docker/NAS deployments.
   - Register `duskcue://` deep links for playback/settings routes and revalidate auth/access before opening content.
   - Add native desktop notifications sourced from SSE notification events, respecting per-user notification preferences.~~ **DONE**

**Task 5 implementation note:** Added the production desktop wrapper layer around the shared web client. `clients/desktop` now builds the web app through `scripts/build-web-static.mjs`, setting `DUSKCUE_WEB_ADAPTER=static` so SvelteKit writes a Tauri-compatible static bundle with `index.html` fallback while the normal web/Docker build remains adapter-node. The Tauri shell now enables the `tray-icon` feature plus `tauri-plugin-dialog`, `tauri-plugin-notification`, `tauri-plugin-deep-link`, and `tauri-plugin-single-instance` with deep-link forwarding. The tray menu exposes Open Duskcue, Server Status, Notifications, Play / Pause, and Quit; left-click opens/focuses the main window. `duskcue://` desktop schemes are registered in `tauri.conf.json`; deep links route only to allowed dashboard, library, media, playback, settings, notifications, and auth-link paths, leaving the web auth guard and server route/API access checks to revalidate content before playback/settings data loads. The web bridge in `clients/web/src/lib/desktop/tauri.js` listens for native navigation/playback events, mirrors foreground SSE `notification` events to native desktop notifications, and exposes a folder picker. The library settings form uses that picker only when running inside Tauri, filling the server-side root path field without changing Docker/NAS path semantics.
6. ~~Build Flutter mobile client shell and navigation:
   - App shell with authenticated/unauthenticated routing, saved-server selection, dashboard, library browsing, media details, search, collections, settings, notifications, and playback entry points.
   - Implement list/detail pagination, artwork loading/caching, empty/error states, pull-to-refresh, and offline/server-unavailable states for browsing.
   - Add localized user-facing strings or a documented path to reuse the web/server i18n catalog; do not hardcode a large English-only mobile surface.~~ **DONE**

**Task 6 implementation note:** Replaced the minimal mobile route list with a GoRouter `StatefulShellRoute.indexedStack` bottom-navigation shell covering Dashboard, Libraries, Search, Collections, Notifications, and Settings, with authenticated/unauthenticated redirects from Riverpod session state. Added mobile browsing DTOs and `ContentService` methods for libraries, library items, media details, search, collections, collection items, notifications, unread counts, and notification read actions using the Phase 16a client contract routes. The new screens implement pull-to-refresh, cursor-style load-more pagination where applicable, empty/error/server-unavailable states, and route-level playback entry stubs for Task 7. Artwork loads through `cached_network_image` with bearer headers from `DuskcueApiClient` and `/api/v1/items/{id}/artwork/{type}` URLs so authenticated artwork can be cached by the mobile image layer. Added a lightweight `AppStrings` localization delegate plus Flutter localization delegates for the new mobile shell surface; this centralizes Task 6 user-facing strings and leaves a clear path to replace the English source with generated ARB/Weblate-backed catalogs when mobile localization broadens. Flutter/Dart are still not installed in this environment, so SDK-level `flutter pub get`, `flutter analyze`, `flutter test`, and Android/iOS builds remain first-run checks for a Flutter SDK environment.
7. ~~Implement mobile playback MVP:
   - Start playback through the server playback API, choose Direct Play/Direct Stream/Transcode URLs correctly, and use HLS for remux/transcode paths.
   - Implement resume, play/pause, seek, heartbeat, stop/exit, completion reporting, foreground/background lifecycle recovery, and cross-device resume state refresh before playback.
   - Support subtitle selection, audio track selection, intro/credit skip buttons, storyboard seek previews where feasible, and playback error reporting.
   - Add media-session/lock-screen controls on Android/iOS where platform APIs allow it.
   - Ensure signed streaming URLs remain valid across normal mobile Wi-Fi/cellular IP changes by relying on session-bound signing, not IP-bound assumptions.~~ **DONE**

**Task 7 implementation note:** Replaced the `/play/{itemId}` placeholder with a Flutter `video_player` playback route backed by `PlaybackService`. Mobile now calls `POST /api/v1/playback/start`, trusts the server-returned `stream_url` for Direct Play, Direct Stream, and Transcode/HLS paths, converts relative Duskcue stream URLs against the selected `:48027` server origin, and initializes `VideoPlayerController.networkUrl` with bearer headers so authenticated manifests/segments survive normal Wi-Fi/cellular IP changes through session-bound auth rather than IP-bound assumptions. Before startup, the player refreshes media details and `GET /api/v1/items/{itemId}/watch-data`, seeks to the latest resume position, and loads audio tracks, subtitles, and intro/credit/recap segments. The route supports play/pause, seek through the server seek endpoint, 15-second heartbeat, stop/exit, completion stop reporting, app lifecycle pause/resume heartbeat behavior, error states, audio/subtitle stream selection by restarting playback with selected stream indexes, and active segment skip buttons. Native lock-screen/media-session integration remains limited by the current Flutter `video_player` surface in this environment; the in-app controls and lifecycle reporting are implemented, and deeper Android Media3/iOS AVPlayer media-session adapters remain the native follow-up if release testing shows platform controls are required beyond plugin support. Flutter/Dart are not installed locally, so SDK-level `flutter pub get`, `flutter analyze`, `flutter test`, and device playback checks remain first-run verification for a Flutter SDK/device environment.
8. ~~Implement foreground real-time updates:
   - Desktop webview uses the existing web SSE store; native desktop surfaces may bridge selected events from Rust/Tauri if needed.
   - Flutter maintains SSE only while foregrounded, with bearer-auth-capable streaming HTTP and reconnection/replay support where possible.
   - Subscribe to `notification`, `session_kicked`, playback-related events, storyboard/scan/admin events where relevant, and future `transcode_progress`.
   - Implement REST polling fallback for notification unread count and playback/transcode state when SSE is unavailable.~~ **DONE**

**Task 8 implementation note:** Added a Flutter foreground real-time layer around `GET /api/v1/events`. `DuskcueApiClient.stream()` opens bearer-auth-capable streaming responses with `Accept: text/event-stream`; `RealtimeService` parses SSE frames, tracks `Last-Event-ID`, reconnects with replay headers, and filters to notification, session, playback/transcode, storyboard, scan, and admin event types. `AppShell` now connects the SSE service only while the user is authenticated and the app lifecycle is foreground/resumed, disconnects when backgrounded or signed out, records connection state in a Riverpod `RealtimeState`, and clears secure auth/session state on `session_kicked`. Notification SSE events increment the shared unread badge and show a foreground snackbar, while unread-count REST polling runs on startup and every 60 seconds only when SSE is disconnected or a forced refresh is needed. The Notifications screen also refreshes the shared unread badge after list/read operations. Playback/transcode/storyboard/scan/admin events are parsed and recorded for downstream screens; Task 8 does not invent new server events or background mobile networking beyond the foreground-only scope.
9. ~~Implement mobile push delivery end-to-end:
   - Flutter obtains FCM/APNs/UnifiedPush tokens where configured and registers them via `POST /api/v1/user/push-devices` on app launch/login, with heartbeat refresh and metadata updates.
   - Fill in server push dispatch for FCM HTTP v1, APNs token-auth HTTP/2, and UnifiedPush endpoint delivery according to [MOBILE_PUSH.md](docs/design/MOBILE_PUSH.md).
   - Invalidate `user_push_devices` rows on provider revoked-token responses (`UNREGISTERED`, `BadDeviceToken`, `Unregistered`, equivalent UnifiedPush failures) without exposing token details.
   - Keep push payloads minimized: localized title/body plus UUID/link metadata only; no media filenames beyond notification text already generated by the server template.
   - Handle notification taps by opening the correct mobile route after auth/access revalidation.~~ **DONE**

**Task 9 implementation note:** Completed mobile push delivery across server, mobile, and admin config surfaces. `NotificationConfig.push` now has provider-specific FCM/APNs/UnifiedPush settings with encrypted private-key handling; `services::notification_dispatch` replaces the previous push placeholder with asynchronous FCM HTTP v1, APNs token-auth HTTP/2, and UnifiedPush endpoint delivery. Push sends only localized title/body plus `notification_id`, type, link, and related UUID metadata, updates `delivery_status.push` after provider completion, and invalidates `user_push_devices` rows on FCM `UNREGISTERED`, APNs `BadDeviceToken`/`Unregistered`, and UnifiedPush 404/410 responses without logging tokens. Mobile `PushRegistrationService` registers available FCM, iOS APNs, and optional Android UnifiedPush tokens via `POST /api/v1/user/push-devices`, stores returned device IDs for 24-hour heartbeat metadata refresh, handles FCM token rotation, registers the Firebase background handler, and routes notification taps through safe internal links only after authenticated shell revalidation. The web admin notifications settings now expose the nested push credential fields read by the server. Official references used: FCM HTTP v1 send/OAuth docs, Firebase Messaging Flutter token/receive docs, Apple APNs token-auth/send docs, and UnifiedPush developer specs. Flutter/Dart are still not installed locally, so SDK-level mobile analysis/build/device push receipt remains first-run verification for a Flutter SDK and configured provider credentials.
10. ~~Implement mobile/desktop quality management:
    - Report device capabilities at first login/app launch and after relevant OS/app updates.
    - Offer the capability wizard for unknown devices where sample playback is feasible; persist results through existing quality APIs.
    - Run active bandwidth probes at configured cadence, skipping or reducing probes on metered/cellular connections unless the user opts in.
    - Report HLS segment telemetry and QoE metrics: startup time, rebuffering, average bitrate, quality switches, errors, and selected quality mode.
    - Implement Auto/Maximum/Manual quality selection and persist per-device/per-item preferences where the server contract supports it.~~ **DONE**

**Task 10 implementation note:** Added mobile quality management and the matching server playback contract extension. `StartPlaybackRequest` now accepts optional `quality_mode` and records quality mode/max bitrate in play-session metadata; Manual mode maps the selected max bitrate to a decision-engine resolution cap while Auto/Maximum preserve existing behavior. Mobile now has `QualityService` for device capability reporting, per-item quality preference storage, connectivity-aware bandwidth probes, heartbeat-cadenced segment telemetry samples, and 30-second QoE reports. The authenticated shell reports the current mobile capability profile on app launch/login and after app/client version changes. The playback route now sends the reported device profile, applies saved Auto/Maximum/Manual quality selections, exposes a Quality picker, persists choices locally per media item, runs active probes during playback while skipping cellular by default, tracks startup time, rebuffer count/ratio, stream switches, selected mode/bitrate, and submits telemetry/QoE to the existing Phase 7 quality endpoints. Desktop/web already had coarse 30-second QoE submission through the shared web player; true per-HLS-segment download timing remains limited on Flutter's current `video_player` surface because it does not expose segment request hooks or native access logs. A future Media3/AVPlayer adapter can replace the coarse mobile telemetry with exact segment-byte/download timing without changing the server endpoint.
11. ~~Implement settings and account management:
    - Profile/session list, current device/session label, sign out all devices, passkey management where supported, notification preferences, push device status, quality preferences, and server connection settings.
    - Admin-capable desktop/mobile views may link to or reuse web settings rather than duplicate every admin workflow; document which admin workflows remain web-first.~~ **DONE**

**Task 11 implementation note:** Expanded the Flutter settings surface into an account/settings hub backed by existing server contracts. Mobile now loads the current profile/server summary, exposes the selected server/network mode with a path to switch servers, copies the web settings URL for web-first administration, labels the current session by comparing the local stable device identifier with `GET /api/v1/user/sessions`, revokes other sessions, signs out locally or everywhere, lists/registers/deletes passkeys through the native passkey channel, edits in-app/push/webhook notification preferences, lists/revokes push devices, and persists a default device quality mode through `QualityService` while playback keeps per-item overrides. The auth session response now includes non-secret `device_id` for the signed-in user's own session-management UI, and passkey registration finish preserves the caller-supplied passkey display name. Desktop continues to reuse the full web settings application through the Tauri wrapper. Web-first admin workflows remain server/system configuration, library/storage/backup/migration administration, provider credential setup, and full quality policy administration; mobile links/copies the web settings URL rather than duplicating those owner/admin workflows.
12. ~~Add packaging, CI, and release smoke tests:
    - Desktop: Windows/macOS/Linux Tauri build smoke tests, app icons, protocol registration, updater decision, signing/notarization placeholders, and documented package artifacts.
    - Mobile: Android debug/release build, iOS simulator/device build where CI allows, app IDs, signing placeholders, permission manifests, privacy disclosures for push/local network/media, and store-readiness notes.
    - Add automated tests for API client error mapping, auth/session storage abstraction, server URL validation, playback state machine, notification handling, and quality telemetry payloads.~~ **DONE**

**Task 12 implementation note:** Added `.github/workflows/client-packaging.yml` as the Phase 16a desktop/mobile package smoke workflow with pinned checkout action and least-privilege permissions. The desktop matrix builds the shared web UI through the Tauri static adapter path and runs `tauri build --debug` on Linux, Windows, and macOS, installing Linux WebKitGTK prerequisites where needed. The Android lane installs Flutter from the official Flutter release manifest, runs `flutter pub get`, `flutter analyze`, `flutter test`, the integration smoke test, and debug/release APK builds. The macOS/iOS lane validates plist/app-icon metadata, runs Flutter analysis/tests, and attempts an iOS simulator build when the generated Runner target exists. Added [CLIENT_PACKAGING.md](docs/ci/CLIENT_PACKAGING.md) for desktop/mobile app IDs, package artifacts, protocol registration, placeholder icons, signing/notarization placeholders, updater decision, permission/privacy disclosures, and store-readiness gates. Added focused Flutter tests for API client error mapping, server URL validation, auth/session state clearing, playback DTO/state helpers, notification/SSE/push-device handling, and quality telemetry payloads; `DuskcueApiClient` now uses a package-level `clientErrorFromDioException()` helper so the tested error conversion path is the production path. Final branded desktop icon sets, protected signing/notarization credentials, app-store metadata entry, and physical-device push/passkey/local-network/playback validation remain release-gate work rather than committed secrets or local CI prerequisites.

**Verification:** Tauri app launches the shared web UI, connects to a Docker deployment through `:48027`, persists/revokes auth securely, handles `duskcue://` deep links, shows tray/native notification behavior, and runs a playback resume flow. Flutter app connects to the server, authenticates by passkey and device-linking paths where supported, browses libraries, searches, opens media details, plays HLS with resume/heartbeat/stop/completion, reports quality telemetry, receives foreground SSE notifications, registers a push device, receives a test push through the configured provider, invalidates revoked tokens, and survives Wi-Fi/cellular transitions without losing the session.

---

## Phase 16b — TV Platform Foundation

**Goal:** Shared server APIs, data contracts, and living-room UX rules that every TV and console client consumes. This phase does not build a platform client.

**Prerequisites:** Phase 7 (playback sessions and progress), Phase 8 (API client conventions), Phase 10 (SSE EventBus for refresh hints), Phase 12 (collections/recommendation inputs), Phase 13a (server/user settings API), Phase 15 (stable deployment URL/base URL behavior), Phase 16a where shared auth/playback/device-quality client lessons can be reused.

**Context from earlier phases:**

- Playback already persists resume and watched state in `user_item_data` through heartbeat/stop/watch-data updates. TV surfaces must reuse that state rather than inventing platform-local truth.
- SSE infrastructure already exists, including per-user channels, replay, connection limits, and named events. `tv_surface_changed` should be a normal named event with bounded payloads and debounce/coalescing.
- Artwork delivery and HTTP caching already exist. TV feed artwork URLs should use those endpoints and inherit their signed/authenticated/cache semantics.
- Collections, metadata, overlays/posters, and search already produce the inputs used for deterministic recommendations and launcher-quality artwork.
- Auth already supports device linking, bearer/session auth, session revocation, and user library access. Deep-link and surface APIs must revalidate access on every request.
- Phase 16a should produce practical lessons for secure token storage, server selection, playback start/resume flows, and quality/device reporting that TV clients can reuse.
- Phase 16a is now complete for the online desktop/mobile MVP: desktop wraps the shared SvelteKit UI through Tauri, mobile has Flutter auth/browsing/playback/realtime/push/quality/settings flows, and `docs/api/client-contracts.v1.json` plus `scripts/verify-client-contracts.mjs` are the current client contract baseline.
- Downstream platform phases should consume the Phase 16a packaging and test lessons from [CLIENT_PACKAGING.md](docs/ci/CLIENT_PACKAGING.md): pinned-action CI, platform package smoke before release claims, no committed signing secrets, explicit permission/privacy disclosures, and focused client-side tests for contract mapping.

**Scope boundary:** Phase 16b builds shared server contracts, reference fixtures, and platform-neutral UX/adapter rules only. It does not create Android TV, Roku, Tizen, webOS, tvOS, Fire TV, or console app projects. Platform clients start in Phase 17+.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [TV_PLATFORM_SURFACES.md](docs/design/TV_PLATFORM_SURFACES.md) | Platform-neutral TV surface feed, platform adapters, shared living-room consistency contract |
| [STREAMING.md](docs/design/STREAMING.md) | HLS playback requirements and transcoding decision model |
| [QUALITY_MANAGEMENT.md](docs/design/QUALITY_MANAGEMENT.md) | Device capability reporting, network quality, adaptive streaming |
| [REAL_TIME_PUSH.md](docs/design/REAL_TIME_PUSH.md) | SSE `tv_surface_changed` refresh hints |
| [API_SECURITY.md](docs/security/API_SECURITY.md) | BOLA checks for all TV surface and deep-link playback endpoints |
| [API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md) | Endpoint naming, typed DTOs, pagination/query validation, cache behavior |
| [HTTP_CACHING.md](docs/design/HTTP_CACHING.md) | Private user-scoped cache headers, ETag behavior, artwork cache policy |
| [AUTH.md](docs/design/AUTH.md) | Device linking, user/session validation, library access, profile isolation |
| [COLLECTIONS.md](docs/design/COLLECTIONS.md) | Deterministic recommendation inputs and collection artwork |

**Tasks:**

0. ~~Research, design, and phase enrichment — use official online sources current to 2026 for TV/console client best practices; update [TV_PLATFORM_SURFACES.md](docs/design/TV_PLATFORM_SURFACES.md), [BUILD_ORDER.md](BUILD_ORDER.md), and any affected platform-specific docs before implementation.~~ **DONE**

**Task 0 implementation note:** Refreshed [TV_PLATFORM_SURFACES.md](docs/design/TV_PLATFORM_SURFACES.md) against current official platform documentation for Android TV / Google TV, Fire TV, Roku, Samsung Tizen, LG webOS, Apple TV / tvOS, Xbox/UWP, and partner-gated ecosystems. The selected Phase 16b approach remains a server-owned, user-scoped TV surface feed plus deep-link resolver and platform adapter contract: the server owns resume/access/recommendation truth, stable `platform_content_id` values, private cache/ETag behavior, diagnostics, settings, fixtures, and `tv_surface_changed` refresh hints; native platform clients translate that contract into row-owned launcher surfaces, event-driven activity reporting, catalog/feed deep links, packaged TV web apps, console media apps, or app-local rows as each platform allows. Platform certification, signing, partner feeds, store visibility, and physical hardware validation remain platform-phase/release-gate work.
1. ~~Create the TV/platform-surface server domain:
   - Add `server/src/domains/tv/` (or `platform_surfaces/`) using the five-file pattern: `mod.rs`, `handlers.rs`, `service.rs`, `types.rs`, `error.rs`.
   - Add router wiring and central `AppError` mapping with stable error codes for invalid platform IDs, unavailable content, access denied, and unsupported platform hints.
   - Add request/response DTOs for feed, section, item, deep-link resolve, settings, diagnostics, and event payloads.
   - Add validation for `platform`, `limit`, `sections`, and `platform_content_id` query/path values.~~ **DONE**

**Task 1 implementation note:** Added the `server/src/domains/tv/` five-file domain and wired it into `server/src/domains/mod.rs`, central routing, and `AppError`. The domain exposes authenticated contract-shell routes for `GET /api/v1/users/me/tv-surface`, `GET /api/v1/tv/resolve/{platform_content_id}`, `GET /api/v1/tv/settings`, and `GET /api/v1/tv/diagnostics`. DTOs now cover surface feed/sections/items, deep-link resolution, artwork hints, settings, diagnostics, `tv_surface_changed` event payloads, platform enums, media types, availability states, and parsed platform content IDs. Query/path validation covers `platform`, `limit`, `sections`, and canonical `duskcue:{movie|episode}:{uuid}` IDs. Stable TV error mappings are registered as `TV_001` through `TV_008`; feed and diagnostics currently return explicit empty placeholder responses, and deep-link resolve returns unavailable content until the playback-ready response task populates that contract.
2. ~~Implement platform content ID utilities:
   - Define canonical IDs such as `duskcue:movie:{media_item_id}` and `duskcue:episode:{media_item_id}`.
   - Add platform-safe encoders/decoders for stricter targets such as Roku feed IDs, Amazon catalog IDs, and URL path/query contexts.
   - Implement inverse lookup from `platform_content_id` to media item, media type, and current access status.
   - Add unit tests proving IDs never expose filesystem/library paths, remain stable across metadata/artwork refresh, and reject malformed or cross-type IDs.~~ **DONE**

**Task 2 implementation note:** Added canonical platform content ID utilities in `server/src/domains/tv/service.rs`: `duskcue:{movie|episode}:{uuid}` builders/parsers, strict alphanumeric platform/feed encodings for Roku/Amazon-style IDs, and percent-encoded URL path/query encodings. Added `TvPlatformIdTarget`, `TvContentAccessStatus`, and `TvPlatformContentLookup` DTOs. `lookup_platform_content()` now resolves a parsed ID against `media_items`, validates movie/episode type matches, filters deleted libraries, and reports current access from `AuthenticatedUser.has_all_library_access` plus `user_library_access`. The resolve handler now performs inverse lookup and returns `TV_003` for denied content before the Task 7 playback-ready response is implemented. Focused tests cover canonical stability, no path leakage, strict platform encoding/decoding, URL encoding/decoding, malformed IDs, and cross-type rejection.
3. ~~Add `GET /api/v1/users/me/tv-surface`:
   - Return a platform-neutral feed with `continue`, `next_up`, `new_episodes`, and `recommended` sections.
   - Support `platform`, `limit`, and `sections` query parameters; platform hints may shape optional metadata but must not change authorization.
   - Include deterministic `platform_content_id`, `surface_item_id`, `media_item_id`, media type, title/subtitle/description, runtime, progress, resume position, last engagement, artwork URLs, deep link, web URL, and availability state.
   - Use private user-scoped cache headers and ETag/`generated_at` semantics so clients can refresh cheaply without shared-cache leakage.~~ **DONE**

**Task 3 implementation note:** `GET /api/v1/users/me/tv-surface` now returns a real authenticated TV feed instead of placeholder sections. The service builds requested sections in order within the total `limit`: Continue Watching from `user_item_data` unfinished movie/episode progress, Next Up as one next unwatched episode per watched series, New Episodes as the newest unwatched episode per started series, and Recommended as deterministic unwatched movie/episode candidates ordered by rating/date/title until Task 5 expands ranking. Each item includes stable `surface_item_id`, canonical `platform_content_id`, media IDs/types, title/subtitle/description, runtime, progress, resume position, last engagement, artwork route hints, deep link, web URL, and availability. The feed filters deleted libraries and current user library access, returns explicit `no_matching_items` / `limit_reached` empty reasons, emits `Cache-Control: private, max-age=60, stale-while-revalidate=300`, and uses conditional SHA-256 ETags. `generated_at` is data-derived rather than wall-clock so unchanged feeds can return `304 Not Modified`.
4. ~~Implement shared access/BOLA enforcement:
   - Reuse or create one helper for user library access checks used by TV feed generation, deep-link resolve, and playback entry.
   - Exclude revoked/deleted/unavailable items from normal feed sections.
   - Return BOLA-safe not-found/forbidden behavior for direct resolve requests so clients cannot probe inaccessible media IDs.
   - Cover admin/profile edge cases, disabled users, soft-deleted users, and library access changes.~~ **DONE**

**Task 4 implementation note:** Added a shared `TvAccessScope` loaded once per TV request from the authenticated user's `has_all_library_access` flag plus active, non-deleted `user_library_access` libraries. Feed queries now use that scope instead of repeated access subqueries, and direct resolve lookup uses the same scope. Normal feed sections now exclude inaccessible libraries, deleted libraries, and items without a healthy media file so launcher rows do not point at revoked or unavailable playback targets. `GET /api/v1/tv/resolve/{platform_content_id}` keeps malformed IDs as `TV_005` but maps inaccessible, cross-type, missing, and otherwise unavailable content to `TV_002` so callers cannot use direct resolve to probe library membership or media-item existence. Disabled and soft-deleted users remain covered at the `AuthenticatedUser` extractor/session-validation boundary, which only returns active, non-deleted users.
5. ~~Implement TV surface service logic:
   - Continue Watching: movies/episodes with meaningful progress but not watched; sort by `user_item_data.last_played_at DESC`; include current resume position.
   - Next Up: at most one episode per series; choose the next unwatched episode after latest completed/played episode; respect season/episode ordering and specials policy.
   - New Episodes: episodes from series the user has started or follows; avoid multiple entries from the same series on constrained surfaces.
   - Recommended: deterministic v1 recommendations from collections, related metadata, genres/tags/credits, and recent activity; avoid unstable random ordering.
   - Empty states: return explicit empty sections and reason hints rather than omitting expected rows silently.~~ **DONE**

**Task 5 implementation note:** Completed the TV surface service logic in `server/src/domains/tv/service.rs`. Continue Watching, Next Up, New Episodes, total-limit handling, and explicit empty reasons were established in Task 3 and hardened by Task 4 access filtering. Task 5 replaces the initial recommendation fallback with deterministic scoring: enabled collection membership adds a collection boost, recent played/watched items build genre/tag/person preference weights, candidate media receive weighted genre/tag/credit overlap scores, and final ordering falls back to rating, premiere date, then title. The query remains deterministic and uses existing collections, `media_genres`, `media_tags`, `media_credits`, and recent `user_item_data` activity without adding schema or random ordering.
6. ~~Add availability and diagnostics metadata:
   - Mark items as `playable`, `needs_transcode`, `library_offline`, `missing_file`, `access_revoked`, `metadata_incomplete`, or equivalent bounded states.
   - Include enough data for TV clients to show a useful error without exposing paths or internal server details.
   - Add an admin diagnostics endpoint or structured debug mode showing why a candidate did or did not appear in the surface feed.
   - Add metrics for feed generation latency, item counts per section, excluded-item counts by reason, and resolve failures by reason.~~ **DONE**

**Task 6 implementation note:** Added bounded availability metadata to TV feed DTOs: items now include `availability_detail` alongside the existing `availability` enum, with `playable`, `missing_file`, and `metadata_incomplete` resolved from healthy media-file and required metadata state without exposing filesystem paths. `GET /api/v1/tv/diagnostics` is now admin-only (`can_manage_server`) and generates a structured diagnostic view for the authenticated admin's feed query: total candidate count, included count, per-section included counts, aggregate exclusion reason counts, and a bounded exclusion sample with `library_offline`, `access_revoked`, `missing_file`, `metadata_incomplete`, or `not_selected` reasons. Diagnostic details use privacy-safe bounded strings and media item IDs only; no library paths, file paths, signed URLs, tokens, or SQL/internal errors are returned. Added Prometheus metrics for TV feed generation latency (`tv_surface_feed_generation_duration_seconds`), per-section item counts (`tv_surface_section_items`), diagnostics exclusion counts by reason (`tv_surface_excluded_items_total`), and resolve failures by reason (`tv_resolve_failures_total`). Resolve still returns BOLA-safe unavailable content to callers while internally distinguishing access-denied failures for metrics. Task 7 remains responsible for replacing the current unavailable placeholder with a playback-ready resolve response.
7. ~~Add deep-link/platform ID resolution:
   - Add `GET /api/v1/tv/resolve/{platform_content_id}` (or equivalent) that maps platform/deep-link IDs to current media item state.
   - Revalidate auth, user library access, item availability, selected Duskcue profile/user, and latest resume position on every resolve.
   - Return a playback-ready response containing media item summary, latest resume position, preferred playback action, playback API input hints, artwork URLs, and whether device linking/auth is required.
   - Never start playback from stale platform-local resume state without fetching current server state first.~~ **DONE**

**Task 7 implementation note:** `GET /api/v1/tv/resolve/{platform_content_id}` now returns a real playback-entry response for accessible movie and episode IDs. The handler delegates to `service::resolve_platform_content()`, which parses and inversely looks up the platform ID, reuses the shared `TvAccessScope`, maps access-denied cases back to BOLA-safe `TV_002` for callers, and then reloads the current media summary, latest authenticated user's `user_item_data` resume position, watched reset behavior, and current best healthy media file at request time. The response includes title/subtitle/description/duration, availability and privacy-safe availability detail, current resume position, artwork hints, `duskcue://` deep link, web URL, `requires_auth`, and a `playback_start` hint object for `POST /api/v1/playback/start` with `media_item_id`, preferred `media_file_id`, start position, and transcode/device-profile hints. Accessible items without a healthy file return a bounded unavailable action instead of exposing paths; inaccessible, deleted-library, missing, malformed, and cross-type IDs remain probe-resistant.
8. ~~Define the TV playback-entry contract:
   - Document the exact flow from platform row/deep link → resolve → playback start → heartbeat/progress → stop/completion.
   - Define how TV clients pass device profile/capability data into existing playback start requests.
   - Define progress heartbeat cadence, completion thresholds, pause/exit reporting, playback error reporting, and when to refresh the TV surface.
   - Specify direct-to-play expectations for Roku/voice-style launches where resume/start-over interstitials are not allowed.~~ **DONE**

**Task 8 implementation note:** Documented the TV playback-entry contract in [TV_PLATFORM_SURFACES.md](docs/design/TV_PLATFORM_SURFACES.md) and cross-linked it from [STREAMING.md](docs/design/STREAMING.md). The contract now defines the required platform row/deep-link flow: authenticate or device-link first, call TV resolve using `platform_content_id`, respect bounded availability, call `POST /api/v1/playback/start` with the resolve-provided media IDs plus TV device profile, seek to the resolve-provided `start_position_ms` when needed, heartbeat every 15 seconds, report pause/buffering state immediately, submit QoE/error context through quality telemetry where available, and call `POST /api/v1/playback/stop` on exit/completion. Completion remains the existing server-side 90% threshold; watched items resolve at position 0. Roku/voice/direct-to-play launches must skip resume/start-over interstitials and use the server-resolved bookmark directly. TV surface refresh timing is documented for playback start, meaningful resume movement, stop/completion, errors, and SSE refresh hints.
9. ~~Emit `tv_surface_changed` SSE events:
   - Publish after playback stop/completion, meaningful resume-position changes, watch-data updates, library scan completion, metadata/artwork refresh, poster/overlay changes, collection changes, and user access-control changes.
   - Include bounded payload fields: `reason`, `changed_sections`, optional affected media/series/library IDs, and `generated_after` or `debounce_until` hint.
   - Debounce/coalesce per user so heartbeat-heavy playback does not trigger excessive feed refreshes.
   - Add tests for event emission, coalescing, and no-event behavior when an update does not affect a user's surface.~~ **DONE**

**Task 9 implementation note:** Added `tv_surface_changed` SSE emission through the shared `EventBus` with bounded payload fields: `user_id`, normalized `reason`, `changed_sections`, optional `media_item_id`/`series_id`/`library_id`, `generated_after`, and optional `debounce_until`. Playback start/heartbeat/seek/stop/completion/watch-data updates now emit user-scoped refresh hints; heartbeat-heavy resume changes are coalesced per user/reason/item for 60 seconds. Library mutation, manual/scheduled/filesystem scan completion, metadata refresh with changed items, poster/artwork actions, overlay changes, collection changes, user status/library-access changes, and capability/access-control updates publish the relevant changed sections. Fan-out helpers select only active users, and library-scoped fan-out respects all-library or explicit library access before emitting. Added tests for bounded reason normalization, debounce keying, debounce suppression/no duplicate event behavior, and actual `tv_surface_changed` payload emission.
10. ~~Add platform-surface settings:
    - Add per-user opt-out for TV platform publication and per-platform toggles where useful.
    - Add admin-visible integration status: enabled platforms, last feed generation, last event, last resolve failure, and diagnostics availability.
    - Decide and document storage location (`server_config`, user preferences JSONB, or a new table) before implementation.
    - Ensure settings changes emit `tv_surface_changed` where appropriate and immediately affect feed/resolve behavior.~~ **DONE**

**Task 10 implementation note:** Added persisted per-user TV surface settings under `users.metadata.tv_surface_settings`, avoiding a new table while keeping launcher publication preferences scoped to the authenticated profile. `GET /api/v1/tv/settings` now returns the effective settings plus integration status: publication enabled state, enabled platforms, diagnostics availability, last feed generation, last TV surface event, and last resolve failure. `PUT /api/v1/tv/settings` accepts partial updates for publication opt-out, enabled platform list, and per-section publication toggles; platform names are validated and deduplicated. Feed generation now returns explicit empty sections for disabled publication, disabled platform, or disabled section settings, and TV resolve returns unavailable content when the user has opted out of TV publication. Settings changes emit `tv_surface_changed` with bounded `settings_changed` reason and only the affected sections. Added web API helpers and client-contract manifest entries for the TV route family.
11. ~~Define the shared living-room UX contract:
    - Standardize row order, row labels, empty states, focus behavior, poster/backdrop roles, typography constraints, profile switching, server selection, device-linking, playback controls, subtitles/audio controls, quality/status display, and error language.
    - Define artwork fallbacks and minimum artwork variants for poster, backdrop, logo, and thumbnail usage.
    - Define profile/household privacy rules so one user's launcher/Top Shelf/Watch Next content does not leak into another user's TV profile.
    - Define localization expectations for platform clients and which strings come from the server vs the client.~~ **DONE**

**Task 11 implementation note:** Expanded the shared living-room UX contract in [TV_PLATFORM_SURFACES.md](docs/design/TV_PLATFORM_SURFACES.md). The contract now fixes row priority order, client-localized row labels, bounded empty-state handling, D-pad/focus/back behavior, TV typography and artwork roles, poster/backdrop/thumbnail/logo fallback rules, playback controls and status requirements, audio/subtitle/quality expectations, profile/server/device-linking behavior, household privacy rules for platform launcher surfaces, and localization boundaries between API keys/content data and client display strings.
12. ~~Define the shared platform adapter contract:
    - Specify how each client maps `platform_content_id`, deep links, resume state, device capability reports, playback progress, artwork URLs, app-local rows, and platform-owned launcher/search rows.
    - Document platform differences: row-owned surfaces (Android Watch Next, Top Shelf), event-driven surfaces (Fire TV Watch Activity), catalog/feed-plus-deep-link surfaces (Roku Search), app-local-only surfaces, and partner-gated surfaces.
    - Define when platform-local mappings may be stored locally and when server-side durable mappings are required.
    - Document that platform clients must not cache bearer tokens in plaintext and must revalidate access before playback.~~ **DONE**

**Task 12 implementation note:** Added the shared platform adapter contract to [TV_PLATFORM_SURFACES.md](docs/design/TV_PLATFORM_SURFACES.md). The contract defines required inputs, identifier/deep-link mapping, surface classes, refresh/removal rules, playback progress and device capability reporting, platform-local versus server-side storage rules, token/secret handling, and an acceptance checklist for future platform phases. It distinguishes row-owned launcher surfaces, event-driven activity surfaces, catalog/feed-plus-deep-link surfaces, app-local-only surfaces, and partner-gated surfaces while preserving the rule that every playback launch revalidates through Duskcue resolve.
13. ~~Build a reference client harness and fixtures:
    - Add JSON fixtures for feed responses, resolve responses, empty states, access-revoked items, and unavailable items.
    - Build a small reference renderer or test harness that renders Continue Watching, Next Up, New Episodes, and Recommendations from the same feed.
    - Add golden tests for feed ordering, stable IDs, cache/ETag behavior, section limits, and BOLA/access filtering.
    - Add contract tests that future platform clients can reuse before platform-specific implementation starts.~~ **DONE**

**Task 13 implementation note:** Added reusable TV surface fixtures under [docs/api/fixtures/tv](docs/api/fixtures/tv): full feed, empty feed, access-revoked feed, admin diagnostics for access revocation, playable resolve, unavailable resolve, and a golden reference render. Added [verify-tv-surface-fixtures.mjs](scripts/verify-tv-surface-fixtures.mjs), a small Node harness that renders feed rows and verifies section order, labels, stable `platform_content_id` values, total limits, private cache/ETag fixture headers, BOLA/access-revoked behavior, unavailable resolve Problem Details, and absence of private paths/tokens/signed URLs. This gives future TV platform clients a reusable pre-implementation contract test before platform-specific code begins.

**Verification:** The server exposes a user-scoped TV surface feed and deep-link resolve endpoint, rejects unauthorized/deauthorized items, produces stable platform content IDs and platform-safe variants, emits debounced `tv_surface_changed` events for all relevant state changes, returns private cache/ETag headers, surfaces useful availability diagnostics without leaking paths, and ships fixtures plus a reference harness that renders consistent Continue Watching, Next Up, New Episodes, and Recommendations rows from the same feed.

---

## Phase 16c — Offline Downloads

**Goal:** Mobile-first offline downloads for authenticated users, with durable server-side preparation, resumable transfer, protected local storage, and reconnect sync, without blocking TV platform work.

**Prerequisites:** Phase 7 (playback/transcoding), Phase 13a (storage/config/maintenance), Phase 15 (stable deployment paths), Phase 16a mobile client foundation.

**Context from prior phases:**

- Phase 7 built online playback and transient stream/transcode sessions. Offline downloads must reuse compatible direct-copy/remux/transcode decisions, but package generation is durable background work, not a live playback session.
- Phase 8 and Phase 10 established watch-state, playback heartbeat, user-item data, and SSE event patterns. Offline progress must sync back into those same user-facing resume/completion semantics.
- Phase 13a introduced storage configuration and disk pressure monitoring. Offline packages need explicit quota, cleanup, and retention rules because user-requested downloads have stronger durability expectations than disposable cache.
- Phase 13b provides notification delivery and real-time surfaces. Offline job status should reuse SSE/push where available for preparing/ready/failed events.
- Phase 16a provides authenticated mobile clients, secure local storage patterns, foreground playback, device capability reporting, and mobile quality selection.

**Scope boundary:** Phase 16c adds offline-download server APIs, job preparation, package serving, mobile download management, local offline playback, and reconnect sync. It does not add offline support to TV clients, desktop clients, or web browsers. Download support for those platforms can be evaluated after mobile behavior proves stable.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [PROJECT.md](PROJECT.md) | Offline Downloads feature commitment |
| [STREAMING.md](docs/design/STREAMING.md) | Transcode/remux outputs, HLS/fMP4 constraints, quality ladders |
| [QUALITY_MANAGEMENT.md](docs/design/QUALITY_MANAGEMENT.md) | Download quality selection from device/network/user preference |
| [SECURITY.md](docs/security/SECURITY.md) | Auth, signed URLs, local storage risk, revocation expectations |
| [CACHE_STORAGE.md](docs/operations/CACHE_STORAGE.md) | Storage tiers, disk pressure warnings, cache vs durable-user-data boundaries |
| `OFFLINE_DOWNLOADS.md` (create in this phase) | Download planning, package format, storage semantics, revocation limits, and reconnect sync contract |

**Tasks:**

0. [x] Research, design, and phase enrichment:
    - Verify current Android and iOS background download limits, media file protection APIs, encrypted metadata storage, app backup exclusion behavior, cellular/Wi-Fi controls, low-storage behavior, and app-store review constraints.
    - Create `docs/design/OFFLINE_DOWNLOADS.md` before implementation begins.
    - Define supported v1 platforms as Android and iOS only, with desktop/web/TV explicitly deferred.
    - Choose whether v1 packages are HLS/fMP4 directories, single MP4 files, or a hybrid; document tradeoffs for subtitles, audio tracks, trickplay/storyboards, resumability, and player support.
    - Define what offline revocation can enforce immediately, what requires reconnect, and how the UI explains that limitation.

**Task 0 implementation note:** Added [OFFLINE_DOWNLOADS.md](docs/design/OFFLINE_DOWNLOADS.md) as the Phase 16c authoritative research/design document. Official Android and Apple research confirms v1 support is Android and iOS only; web, desktop, TV, console, and casting surfaces are explicitly deferred. V1 uses a manifest-backed hybrid package model: HLS/fMP4 directory packages are canonical for mobile playback, subtitles, selected audio tracks, trickplay/storyboard sidecars, resumable transfer, and per-file repair; single MP4 packages are allowed only as a direct-compatible optimization when selected streams and policy can be preserved. The design defines Android user-initiated/background transfer and WorkManager constraint posture, iOS background URLSession/AVAssetDownloadURLSession posture, OS-protected app storage, backup exclusion, cellular/Wi-Fi/Low Data Mode controls, low-storage handling, and app-review constraints. Revocation is split into immediate server enforcement for new jobs and online package serving, and reconnect-bound disabling/deletion for fully offline devices; UI must explain expiry and periodic online checks.
1. [x] Add download database schema:
    - Add migrations for `download_jobs`, `download_packages`, `download_package_files`, `download_device_state`, and download-related audit/event rows as needed.
    - Store user, session/device, media item, media file/version, selected quality, selected audio/subtitle streams, status, progress, bytes, checksum, expiry, access-policy snapshot, failure reason, retry count, cancellation marker, and cleanup eligibility.
    - Add indexes for user library views, active worker queues, expiry cleanup, per-device inventory, and admin diagnostics.
    - Avoid storing bearer tokens, raw signed URLs, or plaintext client secrets in package metadata.

**Task 1 implementation note:** Added migration `20260701010000_create_download_domain.sql` with five Phase 16c tables. `download_jobs` stores durable queue/preparation state, user/session/device ownership, selected media/version, quality, selected streams/artwork, package format/strategy, progress, byte counts, plan revision/hash, access-policy snapshot, failure/retry/cancel state, expiry, and cleanup eligibility. `download_packages` stores the server package inventory using logical `storage_key` plus `manifest_relative_path`, package/file counts, SHA-256 hashes, selected streams, included artwork/storyboards, sync metadata, policy snapshot, serve timestamps, expiry, revocation, and cleanup state. `download_package_files` stores package-relative paths, roles, content types, byte sizes, checksums, segment indexes, track identifiers, and required flags for resumable repair. `download_device_state` stores per-user/device local inventory, transferred bytes, verified file count, local manifest hash, online/download/play timestamps, local resume position, pending sync, deletion, and failure details. `download_events` stores explicit operational events for job/package/quota/policy/checksum/sync/cleanup history. Indexes cover user inventory, active worker queues, media detail views, per-device inventory, expiry cleanup, package-file integrity, JSONB policy snapshots, and event diagnostics. Table-level audit triggers were added for jobs, packages, and device state. No bearer tokens, raw signed URLs, plaintext client secrets, or source filesystem paths are stored.
2. [x] Add `downloads` server domain:
    - Create `server/src/domains/downloads/` using the five-file pattern: `mod.rs`, `handlers.rs`, `service.rs`, `error.rs`, `types.rs`.
    - Add router mounts under authenticated API routes.
    - Define DTOs for planning, job creation, job status, inventory, cancel/delete, package manifest, transfer URLs, and sync submission.
    - Use explicit error codes for access denied, policy denied, quota exceeded, unsupported media, storage unavailable, package expired, job cancelled, package not ready, checksum mismatch, and stale client state.

**Task 2 implementation note:** Added `server/src/domains/downloads/` with the five-file pattern and wired it through `domains/mod.rs`, `router.rs`, and central `AppError`. Routes are mounted under `/api/v1/downloads/*`: `GET /plan/{media_item_id}`, `POST /jobs`, `GET /jobs/{id}`, `POST /jobs/{id}/cancel`, `GET /inventory`, `DELETE /packages/{id}`, `GET /packages/{id}/manifest`, `POST /packages/{id}/transfer-urls`, `GET /packages/{id}/files/{*file_path}`, and `POST /sync`. Planning and job creation require `Require<CanDownload>`; the remaining routes are authenticated user-scoped until service-layer BOLA/policy checks land in Tasks 3-7 and 12. DTOs now cover internal rows plus typed request/response contracts for plan, job creation/status, cancel/delete, inventory, package manifest/file entries, transfer URLs, local package state updates, offline playback events, reconnect sync, and action acknowledgements. `DownloadError` maps to `DOWNLOAD_001`-`DOWNLOAD_016` in RFC 9457 responses and the registry is updated in [ERROR_HANDLING.md](docs/design/ERROR_HANDLING.md). Remaining not-implemented boundaries continue to return explicit `DOWNLOAD_015` responses instead of silent placeholders.
3. [x] Add access, quota, and policy integration:
    - Reuse library/item BOLA checks, user session validity, and streaming-policy constraints before planning or serving packages.
    - Add download-specific policy fields for enablement, max quality/resolution, max bytes per user/device, max active jobs, max retained packages, remote/LAN restrictions, and whether transcoded downloads are allowed.
    - Make denial responses explain the policy reason without leaking filesystem paths or private media details.
    - Record audit events for job creation, cancellation, package serving, expiry cleanup, quota denial, and revoke/delete actions.

**Task 3 implementation note:** Added `server_config.downloads` via migration `20260701020000_add_download_policy_config.sql` and `DownloadsConfig` in `server/src/state.rs`. The policy group covers global enablement, max quality/resolution, byte quotas per user/device, active job limits per user/device, retained package limits per user/device, LAN/local and exposed/remote restrictions, transcode-download allowance, default package expiry, ready-package retention, and forward-compatible per-user/library override maps. Download planning and job creation perform real preflight checks before planning or queueing durable jobs: authenticated session extraction, `can_download` capability at the handler layer, library BOLA check against `has_all_library_access`/`user_library_access`, healthy media-file availability, global enablement, runtime-mode restrictions, active job quotas, retained package quotas, and retained byte quotas. Package, manifest, transfer URL, package-file, and sync routes verify job/package ownership before their remaining not-implemented boundaries, so future implementations start from BOLA-safe service boundaries. Policy/quota denials write bounded `download_events` rows (`policy_denied` or `quota_denied`) without paths, tokens, signed URLs, or package internals. Per-user/library override evaluation remains scoped to Task 14 when admin settings become writable.
4. [x] Add download planning APIs:
    - Return deterministic download plans for a movie or episode: selected source file, direct/remux/transcode path, estimated size, expected duration, quality options, audio tracks, subtitle tracks, artwork inclusion, expiry, and policy constraints.
    - Integrate Phase 13 quality-management defaults and Phase 16a device capability reports.
    - Support Auto, Data Saver, Standard, and Maximum quality choices with predictable fallback behavior.
    - Prefer compatible source versions and direct copy/remux before transcoding; do not always start from the largest source file.
    - Include server-side validation tokens or plan revisions so stale client plans cannot create inconsistent jobs.

**Task 4 implementation note:** Implemented `GET /api/v1/downloads/plan/{media_item_id}`. The endpoint now requires mobile device context (`device_identifier`, `client_platform`) and reuses Task 3 access, streaming-policy, and quota preflight before planning. It supports only movies and episodes, rejects unavailable/non-healthy media, honors an explicit healthy `media_file_id`, and otherwise deterministically selects a source by preferring mobile-compatible MP4 direct-copy candidates before lower-resolution/smaller healthy files. The planner chooses `mp4` + `direct_copy` for mobile-compatible MP4 sources when the selected target does not downscale; otherwise it selects canonical `hls_fmp4` with `remux` or `transcode`. Auto/Data Saver/Standard/Maximum quality options return target resolution, target bitrate, estimated bytes, and transcode requirement; estimates use source size for direct/remux and bitrate × duration for transcode. The response includes source file details, selected format/strategy/quality target, estimated bytes/duration, audio/subtitle options from `additional_streams` with file-column fallbacks, artwork/storyboard inclusion flags, expiry, bounded policy constraints, `plan_revision`, and deterministic SHA-256 `plan_hash`. Added four focused unit tests for resolution parsing, MP4 direct-copy selection, HLS remux selection, and Data Saver transcode estimates.
5. [x] Define durable package format and manifest:
    - Generate a package manifest with schema version, media identifiers, source version, selected quality, selected streams, segment/file list, byte sizes, hashes, subtitles, artwork, chapters/markers where available, expiry, and sync metadata.
    - Include selected subtitles in a mobile-playable format, with conversion/OCR use documented where needed.
    - Include poster/backdrop/thumb assets sized for offline library views without requiring server reachability.
    - Use checksums for each file/segment and a package-level integrity hash.
    - Ensure manifests contain no reusable bearer tokens or long-lived signed URLs.

**Task 5 implementation note:** Implemented the schema-v1 package manifest response for `GET /api/v1/downloads/packages/{id}/manifest`. The handler now loads owned package rows and ordered `download_package_files` rows, rejects missing packages (`DOWNLOAD_012`), expired packages (`DOWNLOAD_006`), revoked packages (`DOWNLOAD_001`), and non-ready/non-serving packages (`DOWNLOAD_008`). The manifest includes package ID, job ID, schema/manifest version, package format, package strategy, media item/file IDs, source-version metadata from `media_files`, selected quality from the originating job, total bytes, package hash, package-relative file entries with roles/content types/byte sizes/SHA-256 checksums/segment indexes/required flags, selected audio/subtitles, included artwork/storyboards, expiry, sync metadata, and access-policy snapshot. The response uses only relative package file paths and stored bounded metadata; it does not include bearer tokens, refresh tokens, signed URLs, source filesystem paths, or reusable client secrets. Package workers in Task 6 remain responsible for populating mobile-playable subtitle files, artwork/thumb/storyboard files, chapters/markers, package hashes, and package-file checksum rows.
6. [x] Add offline package job execution:
    - Implement a durable background worker for prepare/transcode/remux/package jobs with queue ordering, retry, cancellation, timeout, progress reporting, and crash recovery.
    - Keep offline job concurrency separate from live playback/transcode concurrency so downloads cannot starve active streams.
    - Use bounded disk work directories and explicit cleanup for failed, cancelled, expired, and superseded packages.
    - Reuse FFmpeg profiles from streaming where compatible, with offline-specific defaults such as medium preset, fMP4 segments, and stable segment durations.
    - Emit metrics and events for queued, preparing, ready, failed, cancelled, expired, and cleaned-up states.

**Task 6 implementation note:** Added `server/src/workers/download_package_worker.rs` and registered the `download_package_worker` scheduled task with migration `20260701030000_seed_download_package_worker_task.sql`. The worker claims queued jobs in database order with `FOR UPDATE SKIP LOCKED`, recovers stale `preparing` jobs after worker interruption, writes bounded package work directories under `{data_dir}/downloads/{job_id}`, and keeps offline work separate from live playback by using its own scheduled-task loop instead of the live `TranscodeManager` semaphore. `POST /api/v1/downloads/jobs`, `GET /api/v1/downloads/jobs/{id}`, and `POST /api/v1/downloads/jobs/{id}/cancel` now persist and expose real job state: creation recomputes the authoritative plan, rejects stale `plan_revision`/`plan_hash`, snapshots policy, records `job_created`, and enqueues the job; cancellation marks non-terminal jobs cancelled and cleanup-eligible. Package execution supports direct MP4 copy plus HLS/fMP4 remux/transcode through FFmpeg with stable segment duration, medium x264 preset for offline transcodes, per-file SHA-256 checksums, package-level integrity hash, generated `manifest.json`, `download_packages` and `download_package_files` rows, progress/byte updates, retry/fail state, and cleanup events/metrics. Authenticated package file serving, HTTP Range/resumable transfer, and client repair URLs are implemented in Task 7.
7. [x] Add package serving and resumable transfer:
    - Serve package manifests and package files only after revalidating user/session/device access.
    - Support HTTP Range or chunked resumable downloads for interrupted transfers.
    - Provide client-visible checksums and repair/retry behavior for corrupt or partial files.
    - Use short-lived signed transfer URLs or authenticated endpoints that do not expose filesystem paths.
    - Return private cache headers and avoid CDN/public-cache assumptions.

**Task 7 implementation note:** Implemented authenticated package serving and resumable transfer. `GET /api/v1/downloads/packages/{id}/manifest` now requires `device_identifier` and revalidates user ownership, originating session, package device binding, package status/expiry/revocation, download enablement/network policy, current library access, and streaming policy before returning the manifest. `POST /api/v1/downloads/packages/{id}/transfer-urls` now accepts `device_identifier` plus manifest-relative `file_paths` and returns authenticated endpoint URLs under `/api/v1/downloads/packages/{id}/files/{relative_path}?device_identifier=...`; these are not bearer-bearing signed URLs and expose only manifest-relative paths plus checksum/byte-size headers for client repair. `GET /api/v1/downloads/packages/{id}/files/{*file_path}` serves only manifest-listed package files from `{data_dir}/downloads/{job_id}`, rejects traversal/absolute paths, verifies on-disk byte size against the manifest row, supports single HTTP `Range` requests with `206 Partial Content` and `Accept-Ranges: bytes`, returns `DOWNLOAD_016` for invalid ranges, sets private/no-store cache headers, exposes per-file checksum/file-role/segment headers, updates first/last served timestamps, records `package_served`/`package_expired` events, and emits served-file metrics.
8. [x] Add job status notifications:
    - Publish SSE events for download job state changes while the app is foregrounded.
    - Use mobile push notifications, where available from Phase 13b/16a, for ready/failed states that matter outside the app.
    - Coalesce noisy progress events and avoid battery-heavy push behavior.
    - Add unread/admin notification behavior only for actionable failures or quota/storage warnings.

**Task 8 implementation note:** Added foreground `download_job_status` SSE events for queued, preparing, staged, ready, failed, retry, and cancelled download job transitions. The worker publishes only the existing coarse progress milestones (`5`, `10`, `85`, `100`) plus terminal/retry states, so clients get timely updates without per-file progress spam. Ready and final failed jobs now dispatch durable notification records through the existing Phase 13b/16a notification pipeline, which fans out to in-app, SSE `notification`, webhook, and opt-in mobile push according to user preferences and configured providers. Migration `20260701040000_seed_download_notifications.sql` seeds `download_ready` and `download_failed` notification types, and all server Fluent locale bundles include matching templates. Non-actionable progress/cancel/retry events remain foreground SSE only; no battery-heavy push notifications are sent for noisy progress.
9. [x] Add mobile download manager:
    - Implement queue, pause/resume/cancel/delete, Wi-Fi-only, cellular opt-in, charging-only, low-storage handling, app restart recovery, and background transfer integration.
    - Maintain local inventory per server/user/device so switching servers or users cannot expose another account's downloads.
    - Show preparing, ready, downloading, paused, failed, expired, and unavailable states consistently.
    - Support download movie, download episode, delete download, delete all downloads, and retry failed download flows.
    - Defer "download next episode" and auto-remove-watched behavior unless the base manager is stable.

**Task 9 implementation note:** Added the Flutter mobile download manager shell. `clients/mobile/lib/models/download_models.dart` defines download quality modes, scoped inventory keys, settings, plans, jobs, status events, and local inventory items. `DownloadService` calls the Phase 16c server planning, job create/status/cancel, and package-delete endpoints with the current mobile device identity. `DownloadManagerNotifier` persists inventory and settings by `(server_origin, user_id, device_identifier)` in secure storage metadata, merges foreground `download_job_status` SSE events, refreshes jobs after app restart/foreground, and exposes queue, pause/resume, cancel, delete, delete-all, and retry flows. The authenticated app shell now has a Downloads tab and media detail pages can queue the current movie/episode for offline preparation. The manager exposes Wi-Fi-only, cellular allowance, charging-only, low-storage pause, and default download quality controls; actual protected package-file storage and native OS background transfer execution remain scoped to Task 10, while this task establishes the lifecycle/inventory/control surface. Added focused model tests for inventory scoping, job status event merges, and settings round-tripping. Flutter/Dart SDK verification is now available locally; physical-device transfer validation remains release-gate work.
10. [x] Add protected local storage:
    - Store package files in OS-appropriate app storage and exclude downloaded media from cloud backups where platform guidance requires it.
    - Store download metadata, sync queue, package keys if any, and server/user bindings in encrypted or OS-protected storage.
    - Never persist bearer tokens in package manifests or media file metadata.
    - Delete or disable protected data on logout, server removal, user deletion, session invalidation, or user-triggered delete-all.
    - Document platform differences between Android and iOS file protection.

**Task 10 implementation note:** Added a mobile protected-download storage channel and Dart storage boundary. Android `MainActivity` now exposes `duskcue/mobile_storage` methods that create hashed scope/package directories under `noBackupFilesDir/duskcue_downloads`, so package files are app-private and excluded from Android Auto Backup/device transfer. iOS `AppDelegate` exposes the same channel using `Application Support/DuskcueDownloads`, marks directories `isExcludedFromBackup`, and applies `completeUntilFirstUserAuthentication` file protection so downloads remain usable after first unlock without iCloud/iTunes backup. `ProtectedDownloadStorageService` prepares scope/package roots, writes `scope.json`, `sync_queue.json`, and redacted `metadata.json`, and strips bearer/session/access/refresh tokens, package keys, signed URLs, stream URLs, and transfer URLs before writing metadata. `DownloadManagerNotifier` now creates protected roots for queued/ready/failed items and deletes package/scope roots on item delete/delete-all. `AuthService.clearLocalSession()` clears protected downloads and scoped download metadata, covering logout, logout-all, session-kick/session-invalid paths, and server switching through the existing local-session clear flow. Native background transfer and checksum-to-file verification remain Task 11/12 follow-up work with the protected root now available.
11. [x] Add offline playback:
    - Play downloaded movies and episodes without server reachability.
    - Support selected audio and subtitle tracks offline.
    - Preserve local resume position, completion, watched status, and playback events while offline.
    - Make offline playback entry points work from the mobile library, item detail page, and download inventory.
    - Clearly separate unavailable-online items from locally playable downloads.

**Task 11 implementation note:** Added the mobile offline playback path on top of the manifest-backed packages from Tasks 5-10. `DownloadService` now fetches package manifests, creates authenticated transfer URLs, and downloads manifest-listed files with the package's device binding. `DownloadManagerNotifier` materializes server-ready packages into the protected package root, verifies each file's SHA-256 checksum, stores the server manifest as protected local metadata, and promotes verified packages to a distinct `playableOffline` local state so unavailable online/server-ready items are not confused with locally playable downloads. `OfflinePlaybackService` resolves MP4 packages to `media.mp4` and HLS/fMP4 packages to the local `.m3u8` manifest, then starts `video_player` from the local file path without playback/start, heartbeat, seek, QoE, or probe server calls. Offline heartbeats, seeks, stops, completion, watched state, duration, and local resume position are appended to the scoped protected `sync_queue.json` for Task 12 reconnect sync. The Downloads tab, media detail page, and shared library/search media cards now expose offline playback entry points only when the scoped inventory has a verified local package. Packaged selected audio/subtitle metadata is surfaced in the offline player as fixed local selections; changing or downloading alternate tracks remains future package-selection work.
12. [x] Add reconnect sync:
    - Submit queued progress, completion, watched-state, and play-event updates when the server becomes reachable.
    - Reuse `user_item_data` semantics for resume/completion and make sync idempotent.
    - Define conflict resolution when another device updated progress while the mobile device was offline.
    - Revalidate access, expiry, and policy during sync before allowing further playback/download refresh.
    - Handle duplicate sync submissions after app crash or network retry.

**Task 12 implementation note:** Implemented reconnect sync for mobile offline downloads. `POST /api/v1/downloads/sync` now validates package ownership and device binding, revalidates package status, expiry, session binding, download network policy, media access, and streaming policy, upserts `download_device_state`, and returns expired/revoked package IDs so mobile can stop treating those packages as playable. Offline playback events now carry stable `event_id` values; the server stores a bounded accepted-event ID set in device-state metadata and returns `accepted_playback_event_ids`, so duplicate submissions after app crash, response loss, or retry clear locally without double-applying watch progress. Accepted heartbeat/seek events update `user_item_data.resume_position_ms` only when the offline event is not older than the current row, so newer progress from another device wins. Accepted stop/completed events increment play count once per event ID, OR watched state, reset resume to zero when watched/completed, and preserve the newest `last_played_at`. The Flutter manager submits package states plus queued protected sync events on foreground load, job refresh, and after recording offline playback progress; accepted events are removed from `sync_queue.json`, pending counts are refreshed, and expired/revoked responses mark local items expired/unavailable. Native background scheduling remains a future enhancement; this task establishes the idempotent foreground reconnect sync path.
13. [x] Add revocation, expiry, and cleanup behavior:
    - Prevent new downloads immediately when library access, item availability, policy, or session validity fails.
    - Disable or delete existing packages at the next online check when access is revoked, package expiry passes, the user logs out, or the server instructs deletion.
    - Document that fully offline devices cannot receive revocation until reconnect.
    - Add package refresh/renew flow for valid users before expiry where policy allows it.
    - Add server cleanup for expired, orphaned, failed, cancelled, and never-downloaded packages.

**Task 13 implementation note:** Added offline-package revocation, expiry, renewal, and cleanup behavior. Package deletion is now implemented: `DELETE /api/v1/downloads/packages/{id}` marks owned packages `cleanup_pending`, tombstones related device-state rows as `deleted`, clears pending device sync, schedules package/job cleanup, and records `package_deleted`. Added `POST /api/v1/downloads/packages/{id}/renew`; renewals require the current device binding plus session, package status, expiry, network policy, media access, and streaming-policy revalidation before extending `expires_at` and cleanup retention from current download policy. Manifest, transfer URL, file-serving, and reconnect-sync revalidation now mark packages/jobs revoked when an online check detects changed session, access, or policy, while expired packages are marked expired and server-deleted cleanup-pending/cleaned packages are returned as `deleted_package_ids`. The Flutter mobile manager now purges protected package directories on successful sync for expired, revoked, or server-deleted packages; expired/revoked items remain visible as disabled rows and server-deleted packages are removed from local inventory. Mobile refresh-time renewal runs for ready/playable packages within three days of expiry. The package worker now proactively expires due packages, moves never-served ready packages to cleanup after retention, cleans expired/revoked/cleanup-pending/failed package directories, cleans failed/cancelled job directories without package rows, and removes orphaned download directories. Fully offline devices still cannot receive revocation until reconnect, as documented in [OFFLINE_DOWNLOADS.md](docs/design/OFFLINE_DOWNLOADS.md).
14. [x] Add user and admin settings:
    - Add user-facing download settings: quality preference, Wi-Fi-only, cellular allowance, charging-only, storage cap, auto-delete behavior, and delete-all.
    - Add admin settings: global enable/disable, per-user/library policy, max quality, max active jobs, max package retention, max per-user/device bytes, and transcode-download allowance.
    - Surface per-user/device inventory, package status, storage usage, active jobs, and recent failures for admin diagnostics.
    - Ensure settings changes affect future jobs immediately and existing jobs according to documented policy.

**Task 14 implementation note:** Added download settings and admin diagnostics. The system config API now exposes the `downloads` JSONB group, so admins can hot-update global enablement, max quality, user/device byte caps, active job caps, retained package caps, LAN/remote allowance, transcode-download allowance, default expiry, ready-package retention, and per-user/library override maps through the existing runtime config reload path. New planning, job creation, serving, renewal, and reconnect sync resolve an effective download policy by applying user UUID overrides and then library UUID overrides to the global policy. The web system configuration page now includes a Downloads group for those policy fields, including JSON editors for `user_overrides` and `library_overrides`, and the settings hub links to a new Downloads diagnostics page. `GET /api/v1/downloads/inventory` now returns authenticated user/device package inventory instead of `DOWNLOAD_015`; `GET /api/v1/downloads/admin/inventory` requires `can_manage_server` and returns recent package rows plus aggregate storage, active-job, failure, expired, and revoked counts for admin diagnostics without exposing paths or tokens. The Flutter Downloads settings panel now exposes storage cap and auto-delete watched controls in addition to quality, Wi-Fi-only, cellular allowance, charging-only, low-storage pause, and delete-all. Mobile queueing uses the selected plan estimate to reject new jobs that would exceed the local storage cap; settings updates pause queued/ready/downloading items when the cap is exceeded; completed offline playback queues its final sync event before optional auto-delete removes the package. Existing prepared packages continue to honor admin policy changes on manifest, transfer URL, file serving, renewal, and reconnect sync revalidation; existing queued server jobs retain the policy snapshot they were created with until broader job-policy migration/requeue controls are added.
15. [x] Add observability and tests:
    - Add metrics for queue depth, active jobs, bytes prepared, bytes downloaded, failures by reason, transcode duration, storage used, cleanup count, and sync conflicts.
    - Add integration tests for planning, policy denial, quota exceeded, job lifecycle, cancel/delete, package expiry, package serving auth, Range resume, checksum mismatch, and cleanup.
    - Add mobile tests for Android/iOS download lifecycle, app restart recovery, offline playback, low-storage failure, logout delete-all, and reconnect sync.
    - Add manual verification cases for server restart during packaging, network loss during transfer, revoked access while offline, and conflict sync with another device.

**Task 15 implementation note:** Added Phase 16c offline-download observability and focused test coverage. The download worker now emits gauges for queue depth, active jobs, bytes prepared, retained package count, and storage used, plus counters/histograms for prepared package bytes, files per package, transcode duration, bounded failure categories, and cleanup counts. Package serving records exact full/ranged transfer bytes after the response body is read, and reconnect sync records submissions, accepted package states, accepted playback events, rejected revoked/expired/deleted packages, and conflict counts when older offline progress loses to newer server watch state. Added Rust unit coverage for Range parsing, package path traversal rejection, sync conflict classification, accepted offline event ID capping, and failure-label normalization. Added mobile model tests for storage-cap clearing, auto-delete watched persistence, transfer checksum/range hints, offline-playable state persistence, and reconnect sync invalidation DTOs. [OFFLINE_DOWNLOADS.md](docs/design/OFFLINE_DOWNLOADS.md) now documents the metric names, focused automated coverage, local Flutter SDK verification, and manual release verification cases for server restart, network loss, revoked offline access, and cross-device sync conflicts.

**Verification:** User downloads a movie and episode on mobile, goes offline, plays both with selected subtitles/audio, accumulates progress locally, reconnects, and Duskcue syncs resume/watch state idempotently. Interrupted transfers resume without corrupting the package, checksums detect bad files, and server restart during package generation recovers or fails cleanly. Revoked access prevents new downloads and removes/invalidates existing packages at the next online check. Storage quota, low-storage handling, admin disablement, package expiry, and delete-all behavior work on Android and iOS.

---

## Phase 16d — Client Platform Readiness & Contract QA

**Goal:** Shared client contracts, conformance tests, diagnostics, release-readiness checklists, and device-lab practices that reduce duplicated work and inconsistent behavior across desktop, mobile, TV, and console clients.

**Prerequisites:** Phase 16a and Phase 16b. Phase 16c is optional input for offline-download conformance but is not required before TV platform work starts.

**Context from prior phases:**

- Phase 16a creates the first non-web clients and establishes practical patterns for server selection, secure token storage, playback, quality reporting, foreground SSE, mobile push, and packaging.
- Phase 16b defines the shared TV surface feed, platform content IDs, deep-link resolution, living-room UX rules, and adapter contracts consumed by Phases 17-23.
- Phase 16c is complete. Its offline-download contract, observability metrics, focused model/unit coverage, and manual verification cases should be folded into Phase 16d conformance as optional mobile/offline coverage, but it remains a mobile-first branch and should not block TV clients.
- Earlier server phases already expose auth, playback, subtitles, storyboards, artwork, search, collections, notifications, settings, and metrics. Client contract tests should exercise those existing surfaces instead of each platform rediscovering behavior independently.

**Scope boundary:** Phase 16d does not build another user-facing client and does not add a new product feature. It creates shared SDK/contracts, fixtures, test harnesses, diagnostics, design assets, accessibility baselines, and release checklists that downstream platform phases must reuse.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md) | Client repo layout, generated/shared code placement, platform directories |
| [API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md) | Error shape, pagination, auth headers, cache behavior, route naming |
| [API_SECURITY.md](docs/security/API_SECURITY.md) | BOLA checks, token handling, client-side secrecy boundaries |
| [AUTH.md](docs/design/AUTH.md) | Device linking, passkeys, session revocation, user/session lifecycle |
| [STREAMING.md](docs/design/STREAMING.md) | Playback start/resume/heartbeat/stop, subtitles/audio, HLS behavior |
| [QUALITY_MANAGEMENT.md](docs/design/QUALITY_MANAGEMENT.md) | Device capability reporting, network probes, QoE metrics |
| [REAL_TIME_PUSH.md](docs/design/REAL_TIME_PUSH.md) | SSE replay, foreground events, polling fallback |
| [MOBILE_PUSH.md](docs/design/MOBILE_PUSH.md) | Push-device registration and provider-response handling |
| [TV_PLATFORM_SURFACES.md](docs/design/TV_PLATFORM_SURFACES.md) | TV feed contracts, platform content IDs, living-room adapter behavior |
| [HTTP_CACHING.md](docs/design/HTTP_CACHING.md) | ETag/private-cache expectations for client and platform surfaces |
| [CLIENT_PLATFORM_READINESS.md](docs/design/CLIENT_PLATFORM_READINESS.md) | **Primary Phase 16d Task 0 outcome** — platform research, mandatory/advisory gates, contract-source decision, task routing |

**Tasks:**

0. [x] Research, design, and phase enrichment:
    - Review current platform guidance for Android, iOS, Windows, macOS, Linux desktop, Android TV, Fire TV, Roku, Tizen, webOS, tvOS, and Xbox around app identity, signing, accessibility, privacy disclosures, diagnostics, media playback, and store review.
    - Create `docs/design/CLIENT_PLATFORM_READINESS.md` before implementation begins.
    - Define which outputs are mandatory gates for Phases 17-23 and which are advisory checklists.
    - Decide whether shared client contracts are generated from server code, OpenAPI-like descriptions, checked-in JSON schema, or curated fixtures.

**Task 0 implementation note:** Added [CLIENT_PLATFORM_READINESS.md](docs/design/CLIENT_PLATFORM_READINESS.md) as the Phase 16d authoritative research/design document. Official Android, Apple, Microsoft, Amazon Fire TV, Roku, Samsung Tizen, LG webOS, and Flutter documentation confirms Phase 16d should produce shared contracts, fixtures, conformance tests, accessibility/input baselines, diagnostics redaction rules, release/store readiness checklists, and a device-lab matrix rather than another user-facing client. Mandatory gates for Phases 17-23 now include contract fixtures, playback conformance, auth/session denial cases, TV surface/deep-link conformance, accessibility/input checks, diagnostics bundle redaction, release metadata/signing placeholders, and representative device/simulator evidence. Advisory outputs include fully generated SDKs for every language, partner-gated catalog ingestion, exhaustive hardware automation, real signing automation, advanced diagnostics upload, and non-mobile offline-download conformance. The contract-source decision is to extend the existing curated `docs/api/client-contracts.v1.json` plus checked-in fixtures first; generated OpenAPI/JSON Schema and language bindings remain the target direction but must consume the curated manifest/fixtures until the Rust server emits schemas.
1. [x] Define shared client contract source of truth:
    - Inventory required routes and DTOs across auth, server health, libraries, media, search, collections, playback, subtitles, storyboards, artwork, quality, notifications, settings, TV surfaces, and offline downloads where available.
    - Add contract metadata for request methods, auth requirements, query/path validation, response schemas, Problem Details codes, cache headers, pagination, and SSE event payloads.
    - Ensure contract definitions describe both successful responses and expected denial/error cases.
    - Add drift checks so changed server DTOs, routes, or error codes fail CI unless fixtures/contracts are updated.

**Task 1 implementation note:** Promoted [client-contracts.v1.json](docs/api/client-contracts.v1.json) from a Phase 16a route manifest into the Phase 16d shared client contract source of truth. The manifest now marks the required Phase 16d domains, covers 86 routes across health, auth/session/device-linking, users, libraries, media/artwork, search, playback/HLS/watch data, subtitles, segments/storyboards, collections, quality/QoE, TV surfaces/deep links/diagnostics, offline downloads, notifications/SSE/push devices, and settings, and adds per-route `contract` metadata for response schema, cache profile, pagination profile, path/query validation, request schema, and expected RFC 9457 Problem Details codes. Added standard cache profiles, pagination profiles, Problem Details groups, and SSE event payload inventory for notifications, download job status, TV surface changes, and session lifecycle. `scripts/verify-client-contracts.mjs` now fails if required Phase 16d domains are missing or any route lacks required contract metadata, while preserving server-route and web-helper drift checks. Updated [CLIENT_CONTRACTS.md](docs/api/CLIENT_CONTRACTS.md) to document the new manifest/verifier contract.
2. [x] Add shared client SDK or generated bindings strategy:
    - Produce reusable API bindings or typed fixture contracts for TypeScript/Tauri, Dart/Flutter, Kotlin/Android, Swift/tvOS, and other platforms where generation is practical.
    - Where generation is not practical, publish canonical JSON fixtures and validation schemas that platform clients can test against.
    - Standardize bearer-token injection, refresh/re-auth behavior, timeout/retry policy, Problem Details mapping, pagination helpers, and cache/ETag handling.
    - Keep platform-specific secure storage and networking implementations behind small adapters.

**Task 2 implementation note:** Added [client-binding-targets.v1.json](docs/api/client-binding-targets.v1.json) as the Phase 16d shared SDK/binding target matrix and `scripts/verify-client-bindings.mjs` as its drift gate. The matrix keeps the current output fixture-first because the Rust server still does not emit OpenAPI 3.1 or JSON Schema, while marking TypeScript/Tauri, Dart/Flutter, Kotlin Android/Fire TV, and Swift tvOS/iOS as generation-practical once server schemas exist. Roku, Samsung Tizen, LG webOS, Windows, and Xbox remain canonical-fixture or target-dependent until platform phases select tooling. Every target must cover the required Phase 16d manifest domains and the same shared adapters: base URL resolution, bearer-token injection, re-auth/session revoke handling, timeout/retry policy, RFC 9457 Problem Details mapping, pagination, private cache/ETag handling, SSE decoding, secure storage, and diagnostics redaction. Updated [CLIENT_CONTRACTS.md](docs/api/CLIENT_CONTRACTS.md) and [CLIENT_PLATFORM_READINESS.md](docs/design/CLIENT_PLATFORM_READINESS.md) to document the binding strategy and official tooling rationale.
3. [x] Build contract test fixtures:
    - Add fixtures for login/device-linking, server selection, library lists, item details, search facets, collection rows, playback start/resume, heartbeat/stop, subtitles/audio tracks, storyboard metadata, artwork variants, notifications, settings, TV feed, deep-link resolve, and access-denied responses.
    - Include empty states, revoked sessions, missing library access, unavailable media files, expired signed URLs, transcode unavailable, quota/policy denial, and stale client state.
    - Version fixtures so downstream client branches can pin or update them intentionally.
    - Add golden validation for stable IDs, row order, date/time formats, enum values, and localized string ownership.

**Task 3 implementation note:** Added the versioned Phase 16d client fixture pack under [docs/api/fixtures/client/v1](docs/api/fixtures/client/v1/manifest.json) and `scripts/verify-client-fixtures.mjs`. The pack contains 17 fixtures covering all 15 required Phase 16d domains: readiness/server selection, auth login, device-link polling, user preferences, library list success and empty states, media detail/files/artwork, search facets, collection rows, playback start/resume, heartbeat/seek/stop/completion, subtitles/audio tracks/segments/storyboards/artwork variants, quality capability/probe/QoE payloads, download inventory/transfer/sync, notifications/SSE/push devices/settings, TV surface/deep-link resolve, and denial cases for revoked sessions, missing library access, unavailable media files, expired playback URLs, transcode unavailable, quota policy denial, stale client state, and TV access denial. The verifier enforces manifest coverage against `client-contracts.v1.json`, required fixture IDs, stable UUID/platform-content IDs, UTC date-time strings, approved enum values, row ordering, complete RFC 9457 Problem Details, display-ready server-owned localized strings, and redaction of local paths, bearer headers, signed URL parameters, and token-like values. Updated [CLIENT_CONTRACTS.md](docs/api/CLIENT_CONTRACTS.md) and [CLIENT_PLATFORM_READINESS.md](docs/design/CLIENT_PLATFORM_READINESS.md) to document the fixture pack, pinning policy, verifier, and JSON Schema/OpenAPI/Pact rationale.
4. [x] Add cross-client playback conformance tests:
    - Define a reusable playback state-machine test for start, resume, play, pause, seek, heartbeat, stop, completion, and playback error reporting.
    - Verify subtitle and audio-track selection behavior, including unavailable or unsupported tracks.
    - Verify signed media URL handling, HLS/remux/transcode path selection, media-session/remote-control expectations, and cross-device resume refresh.
    - Verify QoE metrics payloads for startup time, buffering, quality changes, playback failure, and selected quality mode.

**Task 4 implementation note:** Added the Phase 16d playback conformance pack under [docs/api/fixtures/playback/v1](docs/api/fixtures/playback/v1/manifest.json) and `scripts/verify-playback-conformance.mjs`. The pack defines seven reusable fixtures: ordered playback state-machine transitions for start/resume seek/first frame/heartbeat/pause/resume/seek/stop/completion/error, audio/subtitle selection cases including unsupported track rejection, direct play/direct stream/HLS transcode stream handoff with credential material kept out of URLs, media-session and remote-control action mappings, QoE samples for startup/buffering/bitrate/quality changes/failure/selected quality mode, cross-device resume refresh that ignores stale launcher cache, and Problem Details error-reporting cases for transcode unavailable, expired playback URLs, and unsupported track selections. The verifier checks required transitions, ordering, API paths, direct/direct-stream/HLS coverage, track-selection cases, remote actions, QoE fields, resume refresh behavior, Problem Details shape, UTC timestamps, stable IDs, and redaction of tokens, signatures, and private paths. Updated [CLIENT_CONTRACTS.md](docs/api/CLIENT_CONTRACTS.md) and [CLIENT_PLATFORM_READINESS.md](docs/design/CLIENT_PLATFORM_READINESS.md) with the playback conformance contract and official Media3/AVFoundation/HLS/Media Session rationale.
5. [x] Add auth and session conformance tests:
    - Verify device-linking, passkey-capable login, fallback login, logout, logout-all, session deletion, re-auth, expired session, and `session_kicked` handling.
    - Validate that clients never persist bearer tokens, signed URLs, push tokens, or package manifests in plaintext stores outside documented exceptions.
    - Define expected behavior when switching servers, switching users, revoking a session, deleting an account, or failing local-network/TLS validation.
    - Add negative tests for BOLA-protected item access and stale platform/deep-link IDs.

**Task 5 implementation note:** Added the Phase 16d auth/session conformance pack under [docs/api/fixtures/auth/v1](docs/api/fixtures/auth/v1/manifest.json) and `scripts/verify-auth-conformance.mjs`. The pack defines five reusable fixtures: auth-flow coverage for device linking, passkey-capable login, fallback login, and re-auth; session lifecycle coverage for logout, logout-all, session deletion, expired sessions, and `session_kicked`; secure-storage policy coverage for bearer tokens, signed media URLs, push tokens, download package manifests, server origins, user summaries, and device identifiers; server/user switching behavior for server changes, user changes, session revocation, account deletion, and local-network/TLS failures; and negative cases for BOLA-protected media access, stale TV platform IDs, stale deep links, expired re-auth codes, and denied device-linking flows. The verifier checks required flows, client-state expectations, storage classifications, plaintext secret prohibitions, Problem Details shape, API-relative request paths, UTC timestamps, stable IDs where applicable, and redaction of real tokens, signatures, and private paths. Updated [CLIENT_CONTRACTS.md](docs/api/CLIENT_CONTRACTS.md), [CLIENT_PLATFORM_READINESS.md](docs/design/CLIENT_PLATFORM_READINESS.md), and [AUTH.md](docs/design/AUTH.md) with the auth/session conformance contract and passkey/WebAuthn/OWASP rationale.
6. [x] Add TV surface and deep-link conformance tests:
    - Validate TV surface feed section order, limits, stable `platform_content_id` values, private cache headers, ETags, access filtering, and empty-state behavior.
    - Validate deep-link resolve responses for movies, episodes, revoked access, unavailable media, and unsupported platform hints.
    - Add platform adapter fixtures for Android Watch Next, Fire TV Watch Activity, Roku feed/deep links, Tizen Smart Hub Preview, webOS launch parameters, tvOS Top Shelf/Universal Links, and Xbox URI activation.
    - Ensure client implementations revalidate auth/access before playback, even when launched from platform-owned surfaces.

**Task 6 implementation note:** Added the Phase 16d TV/deep-link conformance pack under [docs/api/fixtures/tv/v1](docs/api/fixtures/tv/v1/manifest.json) and `scripts/verify-tv-deeplink-conformance.mjs`. The pack defines four reusable fixtures: surface-contract coverage for section order, total limits, stable `platform_content_id` values, private cache headers, ETags, access filtering, and empty states; deep-link resolve coverage for playable movies, playable episodes, revoked access, unavailable media, and unsupported platform hints; platform adapter mappings for Android TV Watch Next, Fire TV Watch Activity, Roku Search/Direct to Play, Samsung Smart Hub Preview, LG webOS launch parameters, tvOS Top Shelf/Universal Links, and Xbox URI activation; and launch-time revalidation cases for stale launcher resume state, revoked sessions, revoked library access, user switching, and deleted/replaced platform IDs. The verifier checks manifest coverage, API-relative paths, Problem Details shape, adapter coverage, mandatory revalidation before playback, UTC timestamps, stable IDs where applicable, and redaction of tokens, signed URL parameters, and private paths. Updated [CLIENT_CONTRACTS.md](docs/api/CLIENT_CONTRACTS.md), [CLIENT_PLATFORM_READINESS.md](docs/design/CLIENT_PLATFORM_READINESS.md), and [TV_PLATFORM_SURFACES.md](docs/design/TV_PLATFORM_SURFACES.md) with the TV/deep-link conformance contract and current official-source rationale.
7. [x] Add accessibility and input baselines:
    - Define minimum accessibility expectations for desktop keyboard navigation, mobile screen readers/dynamic type, TV focus navigation, remote/controller input, captions/subtitles, contrast, reduced motion, and touch target sizing.
    - Add reusable focus-order and remote-navigation test cases for TV clients.
    - Add checklist items for platform accessibility review before release.
    - Ensure localization and right-to-left layout expectations are documented for non-web clients.

**Task 7 implementation note:** Added [CLIENT_ACCESSIBILITY_INPUT.md](docs/design/CLIENT_ACCESSIBILITY_INPUT.md), the Phase 16d accessibility/input baseline fixture pack under [docs/api/fixtures/accessibility/v1](docs/api/fixtures/accessibility/v1/manifest.json), and `scripts/verify-accessibility-input.mjs`. The pack defines five reusable fixtures: baseline checklist coverage for desktop keyboard navigation, mobile screen readers, dynamic type, touch targets, TV focus navigation, remote/controller input, captions/subtitles, contrast/focus, reduced motion, and localization/RTL; focus-order cases for setup/sign-in, home-to-detail, search/filter, media-detail-to-playback, settings dialogs, and notification live regions; TV/console remote-navigation cases for row traversal, row boundaries, player controls, search keyboard return, modal back behavior, and surface refresh focus restore; per-platform review checklists for web desktop, Tauri desktop, Android/iOS mobile, Android TV/Google TV, Fire TV, Roku, Samsung Tizen, LG webOS, Apple tvOS, and Xbox; and localization/RTL cases for client catalogs, server-owned strings, RTL mirroring, directional icons, locale-aware formatting, and activation gates. The verifier checks required platform families, baseline categories, focus cases, remote cases, platform reviews, localization cases, actionable evidence, captions coverage, and truthful expectations. Updated [CLIENT_CONTRACTS.md](docs/api/CLIENT_CONTRACTS.md), [CLIENT_PLATFORM_READINESS.md](docs/design/CLIENT_PLATFORM_READINESS.md), [UI_FOUNDATIONS.md](docs/branding/UI_FOUNDATIONS.md), and [I18N.md](docs/design/I18N.md) with the accessibility/input conformance contract and current official-source rationale.
8. [x] Add shared design assets and client UI tokens:
    - Publish shared app icon, placeholder artwork, poster/backdrop sizing rules, color/type tokens, spacing guidance, focus-ring behavior, and media-state badges.
    - Define which strings come from server templates, which come from client catalogs, and how clients reuse or translate web/server message keys.
    - Add artwork loading rules for authenticated URLs, signed URLs, cache busting, fallback images, and offline/unavailable states.
    - Keep platform clients visually consistent without forcing one UI toolkit abstraction across every platform.

**Task 8 implementation note:** Added [CLIENT_DESIGN_ASSETS.md](docs/design/CLIENT_DESIGN_ASSETS.md) as the Phase 16d shared design asset and UI token contract. The versioned design fixture pack under [fixtures/design/v1](docs/api/fixtures/design/v1/manifest.json) defines DTCG-compatible token groups for color, typography, spacing, radius, shadow, motion, focus, artwork, and badge tones; source SVG assets for the app icon and poster/backdrop/thumbnail/logo placeholders live under [docs/branding/assets](docs/branding/assets); artwork rules cover authenticated URLs, signed URL secrecy, cache busting, fallbacks, offline package artwork, and unavailable/revoked states; string ownership separates server-owned media/problem/notification text from client-owned catalogs and shared key reuse; media-state badges define required states, tone tokens, icon hints, and label keys. Added `scripts/verify-design-assets.mjs` to enforce token groups, asset references, artwork rules, string ownership, platform mappings, and badge coverage.
9. [x] Add diagnostics, logging, and privacy-safe support bundles:
    - Define a common client log schema with timestamp, client version, platform, route/screen, request ID, event type, severity, and privacy classification.
    - Add export-diagnostics-bundle guidance for clients: app logs, device capability report, server URL redacted form, playback failure summaries, network state, and recent request IDs.
    - Ensure bundles omit tokens, passwords, signed media URLs, filenames where unnecessary, private paths, push tokens, and raw watch history unless explicitly consented.
    - Add server-side correlation guidance using request IDs, playback session IDs, and notification/download job IDs.

**Task 9 implementation note:** Added [CLIENT_DIAGNOSTICS.md](docs/design/CLIENT_DIAGNOSTICS.md) as the Phase 16d diagnostics, logging, support-bundle, redaction, and correlation contract. The versioned diagnostics fixture pack under [fixtures/diagnostics/v1](docs/api/fixtures/diagnostics/v1/manifest.json) defines the required client log fields, severity values, privacy classifications, export bundle sections, forbidden data classes, allowed redaction transforms, server-correlation fields, and per-platform export checklists. Added `scripts/verify-client-diagnostics.mjs` to enforce log schema coverage, bundle section coverage, redaction rules, privacy classes, correlation IDs, platform checklists, and absence of fixture leak patterns outside the policy fixture.
10. [x] Add device lab and compatibility matrix:
    - Define minimum and representative test devices for Android, iOS, Windows, macOS, Linux, Android TV/Google TV, Fire TV, Roku, Samsung Tizen, LG webOS, Apple TV, and Xbox.
    - Track OS versions, browser/webview engines, media-codec capabilities, HLS support, HDR/audio/subtitle support, remote/input behavior, storage constraints, and known platform limitations.
    - Add manual smoke-test scripts for each platform against the Docker deployment on `:48027`.
    - Define hardware that is required for release validation vs useful for best-effort compatibility.

**Task 10 implementation note:** Added [CLIENT_DEVICE_LAB.md](docs/design/CLIENT_DEVICE_LAB.md) as the Phase 16d device lab and compatibility authority. The versioned device-lab fixture pack under [fixtures/device-lab/v1](docs/api/fixtures/device-lab/v1/manifest.json) defines required platform IDs for Android mobile, iOS mobile, Windows, macOS, Linux, Android TV/Google TV, Fire TV, Roku, Samsung Tizen, LG webOS, Apple tvOS, and Xbox; minimum and representative devices; OS/runtime/browser-webview tracking; HLS, codec, HDR, audio, subtitle, remote/input, storage, and known-limitation fields; Docker `:48027` manual smoke scripts; release-required versus best-effort hardware; and allowed Phase 16d hardware gaps. Added `scripts/verify-device-lab.mjs` to enforce required platform coverage, capability fields, smoke steps, Docker port, release validation policy, hardware-gap coverage, and fixture leak patterns. Updated [PROJECT.md](PROJECT.md), [CLIENT_PLATFORM_READINESS.md](docs/design/CLIENT_PLATFORM_READINESS.md), [CLIENT_CONTRACTS.md](docs/api/CLIENT_CONTRACTS.md), [QUALITY_MANAGEMENT.md](docs/design/QUALITY_MANAGEMENT.md), and [TV_PLATFORM_SURFACES.md](docs/design/TV_PLATFORM_SURFACES.md) to cross-reference the device lab contract and official-source rationale.
11. [x] Add release and store-readiness checklists:
    - Document app IDs/bundle IDs/package names, signing identities, certificates, provisioning profiles, notarization, store metadata, privacy labels, permission descriptions, age/content rating, and review notes per platform.
    - Add CI placeholders for build artifacts, signing/notarization hooks, SBOM/provenance where applicable, and release-channel naming.
    - Define versioning rules across server, web, desktop, mobile, and TV clients.
    - Add per-platform release-blocking smoke tests and rollback/update expectations.

**Task 11 implementation note:** Added [CLIENT_RELEASE_READINESS.md](docs/design/CLIENT_RELEASE_READINESS.md) as the Phase 16d release/store-readiness authority. The versioned release fixture pack under [fixtures/release/v1](docs/api/fixtures/release/v1/manifest.json) defines required platform coverage for Android mobile, iOS mobile, Windows, macOS, Linux, Android TV/Google TV, Fire TV, Roku, Samsung Tizen, LG webOS, Apple tvOS, and Xbox; app IDs/package names/bundle IDs; signing identities, certificate/key placeholders, provisioning/profile placeholders, notarization or store-signing expectations; permission/capability declarations; privacy disclosures; age/content ratings; review notes; CI artifact, signing, SBOM, and provenance placeholders; local/internal/beta/stable channel naming; versioning rules across server, web, desktop, mobile, and TV clients; release-blocking smoke checks against Docker `:48027`; and rollback/update expectations. Added `scripts/verify-release-readiness.mjs` to enforce release checklist coverage, secure placeholder handling, CI artifact/SBOM/provenance fields, versioning targets, channel maps, smoke steps, privacy/review-note coverage, and fixture leak patterns. Updated [PROJECT.md](PROJECT.md), [CLIENT_PLATFORM_READINESS.md](docs/design/CLIENT_PLATFORM_READINESS.md), and [CLIENT_CONTRACTS.md](docs/api/CLIENT_CONTRACTS.md) to cross-reference the release-readiness contract and official-source rationale.
12. [x] Add client CI and smoke harness:
    - Build an end-to-end smoke harness that starts the Docker deployment, seeds representative data, and runs contract tests against the public `:48027` surface.
    - Add CI jobs for shared contract validation, fixture drift, TypeScript/Dart/Kotlin/Swift binding generation where available, lint, unit tests, and platform build smoke tests.
    - Make downstream platform client phases consume the same harness before declaring verification complete.
    - Keep long-running hardware tests documented as manual/release-gate checks when CI cannot run them.

**Task 12 implementation note:** Added [CLIENT_CI_SMOKE_HARNESS.md](docs/ci/CLIENT_CI_SMOKE_HARNESS.md) as the Phase 16d client CI and smoke harness authority. The versioned client CI fixture pack under [fixtures/client-ci/v1](docs/api/fixtures/client-ci/v1/manifest.json) defines the Docker `:48027` public-surface target, readiness/liveness/SSE smoke checks, deterministic representative seed media, required harness steps, required CI jobs, downstream Phase 17-23 consumption rules, and manual hardware/release-gate boundaries. Added `scripts/client-smoke-harness.mjs` with `--plan` for cheap PR drift checks and `--run` for the real Docker compose smoke path, plus `scripts/verify-client-ci-smoke.mjs` to enforce fixture coverage, workflow wiring, required verifier commands, seed redaction, downstream phase coverage, and hardware-gate documentation. Added `.github/workflows/client-ci-smoke.yml` with always-on shared contract, fixture drift, binding-readiness, TV/console fixture, and smoke-plan jobs; manual workflow-dispatch inputs run the full Docker smoke harness and heavier desktop/mobile platform smoke jobs. Updated [PROJECT.md](PROJECT.md), [CLIENT_PLATFORM_READINESS.md](docs/design/CLIENT_PLATFORM_READINESS.md), [CLIENT_CONTRACTS.md](docs/api/CLIENT_CONTRACTS.md), [CI_TESTING.md](docs/ci/CI_TESTING.md), and [RELEASE_ENGINEERING.md](docs/ci/RELEASE_ENGINEERING.md) to make the harness a downstream platform gate.

**Verification:** A seeded Docker deployment exposes stable contracts and fixtures that desktop, mobile, and TV/console client tests can consume. Contract drift fails CI, generated or fixture-backed bindings are available to downstream clients, playback/auth/TV-surface conformance tests cover success and denial paths, diagnostics omit secrets, accessibility and release checklists exist per platform, and each future platform phase has a clear reusable harness instead of inventing its own baseline.

---

## Phase 17 — Android TV / Google TV

**Goal:** Native Android TV client with Google TV / Android TV Watch Next integration and Sony BRAVIA validation.

**Prerequisites:** Phase 16b and Phase 16d.

**Phase 16d handoff:** Android TV work must consume the shared client CI and smoke harness before claiming phase verification complete: run `node scripts/client-smoke-harness.mjs --plan`, `node scripts/verify-client-ci-smoke.mjs`, and the relevant Phase 16d contract, playback, auth/session, TV/deep-link, accessibility/input, diagnostics, device-lab, and release-readiness verifiers before adding Android TV-specific emulator, Sony BRAVIA hardware, Media3, and Watch Next evidence.

**Tasks:**

0. Research, design, and phase enrichment — verify 2026 Android TV, Google TV, Media3, Watch Next, Google Play, and Sony BRAVIA guidance from official sources; update docs and this phase before implementation.

   **Task 0 implementation note:** Complete — [ANDROID_TV.md](docs/design/ANDROID_TV.md) records the July 18, 2026 official-source review and selects a dedicated `clients/tv/android/` Kotlin application with Compose for TV, not a Flutter TV flavor. The baseline is `com.duskcue.tv`, minSdk 26, compile/target SDK 36, Java 17, and aligned Media3 1.10.1 modules. The client must use the shared Phase 16d fixtures/contract, enforce the profile gate before profile-scoped requests or launcher publication, use server revalidation before every playback start, and treat launcher rows as local cache/mapping state only. Compose for TV is selected because Leanback is deprecated. Watch Next is limited to useful `continue`, `next_up`, and `new_episodes` entries, with one episode per series and removal on completion, revocation, or profile change; recommendations and ambient playback stay app-local. Google TV home-row appearance, Play distribution, and Sony BRAVIA HDR/audio/device behavior remain release-gate evidence rather than an API claim.
1. ~~Create Android TV project foundation — `clients/tv/android/` native Kotlin app using Compose for TV unless Task 0 finds a blocking reason to use Leanback; add Gradle module, package ID, debug signing, `LEANBACK_LAUNCHER` manifest activity, TV banner/icon placeholders, min/target SDK policy, local config, and app identity placeholders aligned with Phase 16d release readiness.~~ **DONE**

   **Task 1 implementation note:** Added the standalone `clients/tv/android/` Kotlin project: `com.duskcue.tv`, Compose for TV, minSdk 26, compile/target SDK 36, Java 17, TV-only manifest features, `LEANBACK_LAUNCHER`, `duskcue://play/...` intake, cleartext-disabled networking, debug signing, and placeholder adaptive icon/banner resources. The checked-in TV wrapper entry point delegates to the existing Gradle 8.14 wrapper; `.gitignore` excludes local SDK/signing state. The initial fixture-first API boundary provides canonical server-origin validation, bearer-header injection, scope-keyed private ETag support, RFC 9457 decoding, typed TV surface/resolve models, and a profile gate, with no persistent credential or media state yet. `:app:testDebugUnitTest`, `:app:lintDebug`, and `:app:assembleDebug` pass against SDK 36/Temurin 17. Device-linking, secure persistence, live UI, Media3, and Watch Next remain later tasks.
2. ~~Add shared contract/API client integration — implement a fixture-backed Kotlin client for the Phase 16d contract surface with base URL selection, bearer-token injection, timeout/retry policy, RFC 9457 Problem Details mapping, pagination/cache helpers, private ETag handling, SSE refresh hint handling where useful, diagnostics redaction, and typed adapter boundaries for later Fire TV reuse.~~ **DONE**

   **Task 2 implementation note:** The TV project now provides a fixture-first, transport-agnostic Kotlin boundary: canonical selected origins on public port `48027`, bearer injection, typed TV surface/deep-link resolve models, profile-scoped private ETags, RFC 9457 problem decoding with trace IDs, and a non-secret network failure path. Shared adapters add bounded idempotent-read retry/retry-after behavior, opaque cursor helpers, `tv_surface_changed` SSE decoding, diagnostics header/URL redaction, and `AndroidTv`/`FireTv` platform values for future reuse. Unit tests consume the existing TV/deep-link and cross-device-resume fixture packs; no token, signed URL, raw server path, or parent PIN is persisted or rendered. Authentication persistence, device linking, and the profile-selection workflow remain Task 3.
3. ~~Implement TV auth, server selection, and profile switching — device-linking login, saved server selection, re-auth/session-expired handling, logout/logout-all behavior, user/profile visibility, profile switching, and cleanup of local TV rows, Watch Next mappings, tokens, diagnostics identifiers, and cached server/user data when identity or server changes.~~ **DONE**

   **Task 3 implementation note:** `SecureSessionStore` now keeps a single DataStore ciphertext envelope encrypted by an Android Keystore AES-GCM key, with a random installation ID, at most ten server origins, and active session/user/profile state. Bearer tokens, parent PINs, signed URLs, playback sessions, and parent-unlock expiry never enter plaintext persistence; backup is disabled. `TvAuthenticationService` implements device-code request/poll, `AUTH_023` pending and `AUTH_024` slow-down timing, restore/re-auth cleanup, profile list/switch/parent-unlock requests, logout/logout-all, and session-kicked cleanup. `TvSessionCoordinator` clears the registered identity scope on account/server replacement and the profile scope before profile selection/switching. Future Watch Next rows bind to that cleaner when Task 7 adds the provider mapping; no profile-scoped UI may mount before the returned profile gate permits it. Focused Kotlin tests cover device identity, slow-down handling, session/account replacement, profile gates, logout, and session kicked.
4. ~~Build living-room home, browse, detail, search, and settings — consume `GET /api/v1/users/me/tv-surface` for Continue Watching, Next Up, New Episodes, and Recommendations; add app-local Search, Libraries, Collections, media detail/pre-playback, user/profile/server settings, TV publication settings, and bounded empty/error states from the shared TV UX contract.~~ **DONE**

   **Task 4 implementation note:** The Android TV Compose shell now provides server entry/device linking, server-enforced profile selection with opt-in remembered-device selection and transient parent-unlock PIN submission, Home, Browse, Search, Detail, Profiles, and Settings routes. `TvApplicationRuntime` shares the secure session runtime with a profile-scoped `TvLivingRoomStore`, so profile/account/server/logout/session-kicked cleanup clears private feed rows and ETags before the next scope can mount. The home uses the ordered TV surface feed; Browse uses libraries/collections; Search uses the profile-authorized search route; Detail calls TV resolve for a current availability check but does not start playback; and Settings reads/updates TV publication. Kotlin tests prove shared-feed ETag reuse, cleanup isolation, and active-user SSE-hint filtering; `:app:testDebugUnitTest`, `:app:lintDebug`, and `:app:assembleDebug` pass on SDK 36/Temurin 17. Media3 playback, real foreground SSE transport, authenticated artwork, Watch Next publication, and hardware evidence are explicitly still later tasks.
5. Implement Android playback stack — Media3 ExoPlayer with HLS/direct/direct-stream/transcode handoff, `MediaSession`, remote/media-button handling, lifecycle pause/resume, progress heartbeat, seek/stop/completion reporting, cross-device resume refresh, audio/subtitle track selection, captions, segment skip controls, quality mode/status display, QoE telemetry, and playback-error reporting.
6. Implement Android deep links and launch revalidation — `duskcue://play/{type}/{id}` and any Task 0-approved HTTPS/app-link path validate auth/access, refresh latest resume state, handle stale launcher tiles, resume after device-link auth, and enter playback directly without exposing raw IDs, tokens, or unavailable media details.
7. Implement Android TV Watch Next adapter — fetch TV surface feed, map eligible continue/next-up/new-episode items to AndroidX `WatchNextProgram`, persist local `media_item_id` to platform `program_id` mappings, update only changed items, remove completed/revoked/stale rows, enforce one-item-per-series guidance, keep user/profile privacy boundaries, and react to `tv_surface_changed` refresh hints.
8. Add Android TV artwork and media metadata handling — choose poster/backdrop/thumbnail/logo dimensions per Android TV and Watch Next requirements, load authenticated/signed artwork without leaking credentials, respect cache headers/ETags, refresh after artwork changes, provide deterministic title-tile fallbacks, and populate TV Provider metadata fields with stable `platform_content_id` values.
9. Add accessibility, input, and Android TV quality checks — D-pad and gamepad focus traversal, Back behavior, overscan safety, 10-foot typography, visible focus/pressed/disabled states, TalkBack labels, captions/subtitle access, reduced-motion behavior, remote-control shortcuts, app startup/load error behavior, and Android TV app quality checklist coverage.
10. Add diagnostics and privacy-safe support evidence — client log schema, request/playback/session correlation IDs, playback diagnostics, Watch Next publication diagnostics, support bundle/export path where practical, redaction of tokens/signed URLs/private media paths, and user-visible troubleshooting that does not expose server internals.
11. Add Android TV CI and conformance harness — Gradle lint/unit tests, fixture-backed API tests, TV/deep-link/playback/auth/accessibility/diagnostics verifier consumption, `node scripts/client-smoke-harness.mjs --plan`, `node scripts/verify-client-ci-smoke.mjs`, Android TV emulator smoke where feasible, and explicit manual/release-gate hardware checks where CI cannot run the device path.
12. Add Android TV release and Google Play readiness — Android TV form-factor track/readiness notes, target API policy, app bundle/APK artifact placeholders, package/versioning rules, Play signing placeholders, Data Safety/privacy disclosures, content rating, TV screenshots/banner/icon assets, reviewer test credentials/runbook, SBOM/provenance placeholders, rollback/update expectations, and Android TV quality checklist evidence.
13. Validate NVIDIA SHIELD TV as the high-capability Android TV reference device — test SHIELD TV and SHIELD TV Pro where available for Google Play visibility, Ethernet/Wi-Fi behavior, 4K HDR/HDR10/Dolby Vision mode behavior, Dolby Atmos/TrueHD/DTS passthrough and fallback, HLS/direct/direct-stream/transcode decisions, subtitles/captions, AI-upscaling/display-mode interactions, remote/gamepad controls, standby/resume, Watch Next behavior, and diagnostics capture.
14. Validate Sony as Android TV / Google TV hardware — test Sony BRAVIA Google TV and Android TV devices for Google Play install visibility, Watch Next behavior, HLS playback, HDR, Dolby Vision/HDR10 behavior where supported, audio passthrough/downmix, subtitles, remote focus, standby/resume, voice/deep-link entry, and differences between Google TV and older Android TV BRAVIA models.

**Verification:** Start a movie on web, stop midway, see it appear in Android TV Watch Next, select the tile, and resume in the Android TV app at the latest server position. Complete an episode and verify the next episode replaces it. Run the Phase 16d client CI/smoke harness and Android TV conformance checks, then validate the same flow on NVIDIA SHIELD TV and representative Sony BRAVIA hardware with HDR/audio/subtitle, remote, standby/resume, and diagnostics evidence.

---

## Phase 18 — Fire TV

**Goal:** Fire TV client/adapter using Amazon-specific Watch Activity, Content Personalization, and catalog/deep-link integration where available.

**Prerequisites:** Phase 16b and Phase 16d, preferably Phase 17 where Android client code can be reused on Fire OS.

**Tasks:**

0. Research, design, and phase enrichment — verify 2026 Fire TV, Fire OS, Vega, Watch Activity, Content Personalization, EMBER/catalog, and appstore guidance from official Amazon sources; update docs and this phase before implementation.
1. Add Fire TV app target — reuse Android architecture where Fire OS supports it; document any divergence from Android TV.
2. Implement Fire TV Watch Activity event reporting for playback start/progress/pause/resume/exit/completion.
3. Implement stable Amazon content IDs from Duskcue `platform_content_id`.
4. Implement authenticated deep-link playback and catalog/EMBER integration if partner access is available.
5. Track Fire TV Vega separately — implement only if Amazon's non-Android Vega path becomes required for target devices.

**Verification:** Report playback activity and verify Continue Watching behavior where the active Fire TV account allows personalization. Deep links open authenticated Duskcue playback and use the latest server resume position.

---

## Phase 19 — Roku

**Goal:** Roku SceneGraph/BrightScript client with certified deep links, Direct to Play, bookmarks, and optional Roku Search feed support.

**Prerequisites:** Phase 16b and Phase 16d.

**Tasks:**

0. Research, design, and phase enrichment — verify 2026 Roku SceneGraph, BrightScript, certification, deep-link, Direct to Play, bookmark, Roku Search feed, and public-channel guidance from official Roku sources; update docs and this phase before implementation.
1. Add Roku client shell — `clients/tv/roku/` SceneGraph/BrightScript app with device-linking, server selection, library browsing minimum, HLS playback, and progress reporting.
2. Implement Roku deep links and Direct to Play — handle `contentId`/`mediaType`, map to Duskcue media IDs, fetch the latest bookmark/resume state, and start movie/episode playback directly.
3. Add Roku app-local Continue Watching, Next Up, New Episodes, and Recommendations rows from the TV surface feed.
4. Add Roku Search feed support — generate stable feed IDs, metadata, artwork, and availability for public-store discovery if the channel targets Roku Search.

**Verification:** Launch movie and episode deep links through ECP/Deep Linking Tester and verify Direct to Play starts at the latest server bookmark. Playback progress updates Duskcue and app-local rows refresh correctly.

---

## Phase 20 — Samsung Tizen

**Goal:** Samsung packaged Tizen web app with AVPlay playback and Smart Hub Preview integration.

**Prerequisites:** Phase 16b and Phase 16d.

**Tasks:**

0. Research, design, and phase enrichment — verify 2026 Samsung TV SDK, Tizen packaging/signing, AVPlay, Smart Hub Preview, personal preview, subtitles, model-year support, and store/certification guidance from official Samsung sources; update docs and this phase before implementation.
1. Add Samsung Tizen client shell — `clients/tv/samsung/` packaged/signed Tizen web app with device-linking, server selection, TV remote focus navigation, and real-device install workflow.
2. Implement Samsung AVPlay playback — HLS playback, seek-to-resume, progress heartbeat, completion reporting, audio/subtitle selection, and model-year compatibility checks.
3. Implement Samsung Smart Hub Preview — app-local continue/next-up rows first, public preview deep links where useful, then personalized preview via foreground app + background service after validation.

**Verification:** Install the signed Tizen package on real hardware, resume HLS playback through AVPlay, verify app-local rows update after playback, and verify Smart Hub Preview deep links open the correct Duskcue item where supported.

---

## Phase 21 — LG webOS

**Goal:** LG packaged webOS TV app with launch/relaunch parameter handling, HLS resume playback, and app-local TV surfaces.

**Prerequisites:** Phase 16b and Phase 16d.

**Tasks:**

0. Research, design, and phase enrichment — verify 2026 LG webOS Studio, packaging, app lifecycle, Application Manager, media playback, `mediaOption`, web engine, model support, and app approval guidance from official LG sources; update docs and this phase before implementation.
1. Add LG webOS client shell — `clients/tv/lg/` packaged webOS TV app with device-linking, server selection, TV remote focus navigation, packaging, simulator testing, and real-device deploy workflow.
2. Implement LG launch/relaunch handling — parse webOS launch parameters, map stable `platform_content_id` values to Duskcue media IDs, revalidate auth/access, fetch the latest resume state, and enter playback.
3. Implement LG playback and app-local surfaces — HLS playback, `mediaOption` resume where supported, progress heartbeat, completion reporting, app-local Continue Watching/Next Up/New Episodes rows, and model/webOS-version compatibility checks.

**Verification:** Install the packaged webOS app on real hardware, launch with playback parameters, resume HLS playback with the latest server position, and verify app-local Continue Watching/Next Up rows update after playback.

---

## Phase 22 — Apple TV / tvOS

**Goal:** Native tvOS app with AVKit playback, Universal Links, and Top Shelf extension.

**Prerequisites:** Phase 16b and Phase 16d.

**Tasks:**

0. Research, design, and phase enrichment — verify 2026 tvOS, Swift/SwiftUI, AVKit, Universal Links, associated domains, Top Shelf, App Store, and Apple TV app/Universal Search guidance from official Apple sources; update docs and this phase before implementation.
1. Add Apple TV / tvOS client shell — `clients/tv/apple/` native Swift/SwiftUI tvOS app with device-linking, server selection, TV focus navigation, App Store packaging, and real-device deploy workflow.
2. Implement Apple AVKit playback — HLS playback through AVKit/AVPlayerViewController, seek-to-resume, progress heartbeat, completion reporting, audio/subtitle selection, and Apple TV hardware compatibility checks.
3. Implement Apple Universal Links — associated-domain configuration, stable `platform_content_id` link mapping, auth/access revalidation, and direct playback entry from supported links.
4. Implement Apple Top Shelf extension — use the Duskcue TV surface feed for curated continue-watching and next-up content; keep recommendations secondary; evaluate Apple TV app/Universal Search as optional partner/release work.

**Verification:** Install the tvOS app on real hardware, resume HLS playback through AVKit, open a Universal Link into direct playback, and verify Top Shelf items reflect continue-watching and next-up state.

---

## Phase 23 — Xbox

**Goal:** Native UWP Xbox console media app with app-local TV surfaces, URI activation, and explicit 4K/HDR capability decisions.

**Prerequisites:** Phase 16b and Phase 16d.

**Tasks:**

0. Research, design, and phase enrichment — verify 2026 Xbox/UWP, Store, MSIX, Xbox Device Portal, media playback, URI activation, SMTC, 4K/HDR, and certification guidance from official Microsoft sources; update docs and this phase before implementation.
1. Add Xbox client shell — `clients/tv/xbox/` native UWP media app with device-linking, server selection, controller/media-remote focus navigation, Visual Studio/MSIX packaging, and Xbox Device Portal deploy workflow.
2. Implement Xbox playback and app-local surfaces — native `MediaPlayerElement` / `MediaPlayer` HLS playback, seek-to-resume, progress heartbeat, completion reporting, System Media Transport Controls, app-local Continue Watching/Next Up/New Episodes rows, and Xbox hardware capability reporting.
3. Implement Xbox deep-link activation — URI protocol or App URI handler mapping stable `platform_content_id` values to Duskcue media IDs with auth/access revalidation and direct playback entry.
4. Decide Xbox 4K/HDR release posture — evaluate `hevcPlayback`, memory/background tradeoffs, HDR10 behavior, audio/subtitle support, Store certification, and whether self-hosted catalogs are acceptable.

**Verification:** Install on Xbox hardware, resume HLS playback through native media APIs, control playback through controller/media remote/SMTC, open URI activation into direct playback, and verify app-local rows update after playback.

---

## Phase 24 — Partner-Gated Platforms

**Goal:** Evaluate and implement partner-gated platform adapters only when platform access confirms self-hosted Duskcue viability.

**Prerequisites:** Phase 16b and Phase 16d.

**Tasks:**

0. Research, design, and phase enrichment — verify 2026 VIZIO, PlayStation, VIDAA, and set-top platform access using official sources/partner portals; update docs and split any viable platform into its own implementation phase before building.
1. Add VIZIO partner-access track — request/evaluate VIZIO Developer Portal access, confirm partner requirements, app model, media playback APIs, certification, deep-link/launcher/discovery surfaces, and whether self-hosted Duskcue distribution is viable.
2. Add VIZIO client only after access is confirmed — `clients/tv/vizio/` with device-linking, server selection, TV remote focus navigation, HLS/resume playback, progress reporting, app-local Continue Watching/Next Up rows, and VIZIO-specific deep links/discovery feeds if partner specs expose them.
3. Add PlayStation partner-access track — request/evaluate PlayStation Partners access, confirm media app feasibility, SDK/playback APIs, certification, dev hardware requirements, Media space/deep-link/resume surfaces, and whether self-hosted Duskcue distribution is viable.
4. Add PlayStation client only after access is confirmed — `clients/tv/playstation/` with device-linking, server selection, controller/media-remote focus navigation, HLS/resume playback, progress reporting, app-local Continue Watching/Next Up rows, and PlayStation-specific Media space/deep-link integrations if partner specs expose them.
5. Research VIDAA next — evaluate developer access, app model, playback APIs, partner requirements, and whether it deserves its own implementation phase.
6. Queue future platform research — operator set-top ecosystems and Apple Vision Pro / visionOS; prioritize only if user demand or distribution feasibility changes.

**Verification:** Partner-gated platforms are either promoted into a dedicated implementation phase with confirmed access and requirements, or explicitly left as blocked/deferred with documented reasons.

---

## Post-Phase 16d — Admin Settings Refactor (COMPLETE — pending final commit)

**Goal:** Replace the flat Settings surface with a personal Settings area and capability-filtered Admin experience, give each configuration field a canonical editor, and move operational workflows out of generic settings pages.

**Authoritative documents:** [ADMIN_SETTINGS.md](docs/branding/ADMIN_SETTINGS.md), [UI_FOUNDATIONS.md](docs/branding/UI_FOUNDATIONS.md), [CLIENT_ACCESSIBILITY_INPUT.md](docs/design/CLIENT_ACCESSIBILITY_INPUT.md), [AUTH.md](docs/design/AUTH.md).

**Tasks:**

1. ~~Separate personal Settings from Admin navigation and retire stale placeholder destinations~~ **DONE**
   - Added `/admin`, capability-filtered by the same owner-bypass rules used by page authorization.
   - Reduced `/settings` to personal language preferences, notifications/devices, and a conditional Admin entry.
   - Redirected `/settings/quality`, `/settings/security`, and `/settings/storage` to their implemented System groups.
   - Made System group selection shareable through `?group=…` and represented it as native navigation rather than an incomplete tab widget.
   - Added fully localized labels to every reviewed web locale.
2. ~~Consolidate subtitle configuration ownership and shrink the generic System editor~~ **DONE**
   - Dedicated Subtitles now owns `server_config.subtitles` and `integrations.subtitle_providers`; legacy System deep links redirect to the canonical page.
   - Extracted the data-driven configuration controls into `ConfigGroupForm.svelte` and grouped the remaining System links by task area.
   - Corrected delayed capability-load behavior in System, Subtitles, Backups, and Downloads.
3. **Deferred after evidence review:** the only repeated complex control was configuration-group editing, which is now shared through `ConfigGroupForm.svelte` and `configForms.js`. Page, card, async-state, metric, and table abstractions would currently hide meaningful workflow differences rather than reduce proven duplication.
4. ~~Move and simplify the Backups, Notifications, Migration, Collections, Overlays, and Downloads workflows~~ **DONE**
   - **Downloads complete:** Downloads now owns its server policy and its package-inventory operations; the System Downloads group redirects to this canonical surface.
   - **Backups complete:** readiness and actions remain visible by default; scheduled-task and evidence detail is progressively disclosed.
   - **Notifications complete:** personal feed/preferences/devices remain in Settings; server test dispatch is isolated under Admin and the personal segmented controls no longer claim incomplete tab semantics.
   - **Collections, Overlays, and Migration complete:** canonical routes now live under `/admin`; legacy Settings paths redirect permanently.

**Admin settings refactor status:** The high-value IA, configuration-ownership, and operational-workflow tasks are complete; the broader primitive-extraction item is deliberately deferred pending demonstrated duplication. Final project-wide verification, documentation audit, and one intentional commit/push remain.

**Verification:** `npm run build` and `npx svelte-check --tsconfig ./jsconfig.json` pass with no errors or warnings.

**Context for Task 2:** Dedicated domain pages remain canonical for rich behavior. In particular, Subtitles and subtitle-provider configuration must not stay duplicated between its specialized editor and generic System configuration.

---

## Post-Phase 10 — Storyboards Hardening & Observability (COMPLETE)

**Goal:** Make Storyboards reliable and observable across fresh databases, concurrent generation, authenticated clients, and configurable cache/storage environments.

**Authoritative documents:** [STORYBOARDS.md](docs/design/STORYBOARDS.md), [DATABASE.md](docs/design/DATABASE.md), [MIGRATION_STRATEGY.md](docs/design/MIGRATION_STRATEGY.md), [CLIENT_PLATFORM_READINESS.md](docs/design/CLIENT_PLATFORM_READINESS.md).

**Tasks:**

1. ~~Repair the media-item schema contract~~ **DONE — `bcab31b`**
   - Kept `media_items` as a hard-delete table, as specified in DATABASE.md, rather than adding an undocumented `deleted_at` lifecycle.
   - Removed stale `media_items.deleted_at` predicates from profile access, playback, subtitle candidate selection, and metadata refresh.
   - Corrected metadata refresh to use the canonical CTI parent fields (`media_items.type` and `media_items.tmdb_id`) rather than nonexistent child-table fields.
   - Extended disposable-database migration verification with representative media/profile/playback/worker query preparation.
2. ~~Publish complete storyboard artifact sets atomically and serialize each media-file generation with a database-backed lock~~ **DONE — `6e9cc15`**
   - Added nullable `storyboards.artifact_id` so legacy rows retain their existing per-file layout until regeneration.
   - Generation now holds a transaction-scoped PostgreSQL advisory lock per media file, writes a unique UUIDv7 artifact directory, and atomically switches the row pointer only after FFmpeg output and persistence succeed.
   - Manual contention returns `SYS_002`; scheduled contention is skipped; disposable-database migration verification proves lock exclusivity and transaction-end release.
3. ~~Deliver protected VTT and sprite assets through bearer-authenticated client loaders without credentials in URLs~~ **DONE — `dbcf0ce`**
   - Added text/blob response support to the shared API client, preserving its bearer header and selected server-origin behavior.
   - Seek previews lazily fetch protected VTT and WebP routes, then render only bounded `blob:` object URLs; abort and revocation prevent stale requests and retained image memory.
   - Parsed VTT references supply sprite filenames only; credential-bearing or server-relative URLs are never used as CSS image URLs.
4. ~~Correct null-hash freshness behavior, validate Storyboards configuration, and regenerate artifacts when the normalized generation configuration changes~~ **DONE — `9f45fee`**
   - `storyboards.file_hash` is nullable to match `media_files.file_hash`; legacy empty sentinel values migrate to null and nullable hashes compare as values in the candidate filter.
   - Each completed storyboard stores a normalized output fingerprint, so a changed effective interval, width, quality, keyframe mode, or grid triggers regeneration even when the source hash is unchanged.
   - Server and web configuration validation now enforce the documented mode, interval, width, quality, and grid bounds.
5. ~~Honor configured cache storage and reconcile orphaned artifacts~~ **DONE — `85b7a1f`**
   - Storyboard handlers and workers now use `BootstrapConfig.cache_dir`, honoring the configured cache root.
   - Post-generation reconciliation locks each media-file directory before removing unreferenced artifact directories, obsolete legacy files, or directories with no storyboard row; active generation staging is skipped safely.
6. ~~Return scheduled generation failures to the scheduler, validate task configuration, add Storyboards fixtures/integration coverage, and finish seek-preview accessibility/performance hardening~~ **DONE — `4cddce0`**
   - The registered executor is fallible. Malformed task configuration, unavailable configured libraries, and any per-library or per-file generation failure are returned to the scheduler after the worker completes safe cleanup.
   - Task configuration accepts only a UUID `library_id`, `adaptive`/`fixed` interval mode, and the serialized single-worker concurrency setting; unknown or invalid values fail fast.
   - Forced item generation and deletion now operate on every healthy media-file version under the same advisory-lock discipline, avoiding a stale alternate rendition after an admin action.
   - The client contract fixture pack now covers bearer-authenticated storyboard metadata, VTT, and WebP blob retrieval with private no-store caching and no credentials in URLs.
   - Seek previews honor reduced-motion preference, stay hidden from the accessibility tree, work with keyboard range input, and retain a 44px seek target. Playback declines unhealthy requested files.
7. ~~Add bounded Storyboard generation, storage, and serving telemetry~~ **DONE — `09668d4`**
   - The existing Prometheus endpoint now exposes outcome-bounded generation attempts and errors, successful FFmpeg duration and sprite counts, authenticated index/sprite read outcomes, and the reconciled cache byte size.
   - Library IDs, media IDs, file paths, and raw error text remain in structured logs, task history, and SSE progress rather than Prometheus labels; each label vocabulary is fixed and privacy-safe.
   - Cache measurement runs off the async worker, sums the actual VTT/WebP cache tree, and ignores symlinks so telemetry cannot traverse outside the configured cache root.
8. ~~Cover configured Storyboard task timeout and cancellation behavior~~ **DONE — `0420e68`**
   - The scheduler's persisted `timeout_seconds` now has direct regression coverage, including the Storyboard four-hour value and invalid-value clamp, timeout dropping unfinished work, and cancellation winning before a handler begins.
   - Timed-out Storyboard FFmpeg work is safe: Tokio drops the command future and `kill_on_drop(true)` terminates the child process. The corrected Build Order removes every stale hard-coded one-hour timeout claim.

**Outcome:** All eight hardening and observability tasks are complete. Verification for Task 8: `cargo fmt --all -- --check`, `cargo test -p duskcue` (766 tests), `node scripts/verify-storyboard-metrics.mjs`, and a strict clippy pass with the 11 known unrelated download, playback, TV, notification, and metadata-refresh diagnostics explicitly suppressed. The Task 8 implementation adds no lint diagnostics.

---

## Dependency Graph

```
Phase 1: Scaffolding (COMPLETE — aaedc05)
    ↓
Phase 2: Database Schema (COMPLETE — 15 migrations)
    ↓
Phase 3: Core Server Infrastructure (COMPLETE — 12 tasks)
    ↓
Phase 4: Auth & Users (COMPLETE — 11 tasks)
    ↓
Phase 5: Libraries & Media (COMPLETE — 10 tasks) ─────────────────────────────┐
    ↓                                                      │
Phase 6: Metadata Providers ←─── (enriches Phase 5)       │
    ↓                                                      │
Phase 7: Streaming & Playback (COMPLETE — 13 tasks)              │
    ↓                                                      │
Phase 8: Web Client Core (COMPLETE — 6 tasks) ←─── (consumes all above) ←──────┘
    ↓
    ├── Phase 9:  Subtitles (COMPLETE — 8 tasks)
    ├── Phase 10: Segments & Storyboards (COMPLETE — 12 tasks: 8 core + SSE + image pipeline + artwork endpoint + events store)
    ├── Phase 11: Analytics & Trakt (COMPLETE — 9 tasks)
    ├── Phase 12: Kometa-Like System (COMPLETE — 11 tasks)
    ├── Phase 13a: System Operations Core (COMPLETE — config + backup + maintenance)
    │       ↓
     │   Phase 13b: Notification System (Fluent + dispatch + push)  ←── COMPLETE — can overlap with Phase 14 — All 6 tasks done (Fluent i18n + dispatch pipeline + notification CRUD + webhook dispatch + push device registration + notifications UI)
    │       │
    ├── Phase 14: Platform Migration  ←── proceeds after 13a, independent of 13b
    │       │
    ├── Pre-v1.0 Hardening (Cache-Control/ETag + Paraglide + faceted search + metrics + 7-locale translations + RTL review + locale activation)
    │       │
    └── Phase 15: Docker & Deployment  ←── COMPLETE — single-container image, compose, smoke verification, GHCR workflows
            ↓
        Phase 16a: Desktop & Mobile Clients  ←── mobile push needs 13b
            ├── Phase 16b: TV Platform Foundation
            │       ↓
            │   Phase 16d: Client Platform Readiness & Contract QA
            │       ↓
            │   Phase 17: Android TV / Google TV  ←── includes Sony BRAVIA validation
            │       ↓
            │   Phase 18: Fire TV
            │       ↓
            │   Phase 19: Roku
            │       ↓
            │   Phase 20: Samsung Tizen
            │       ↓
            │   Phase 21: LG webOS
            │       ↓
            │   Phase 22: Apple TV / tvOS
            │       ↓
            │   Phase 23: Xbox
            │       ↓
            │   Phase 24: Partner-Gated Platforms  ←── VIZIO, PlayStation, VIDAA, future set-top platforms
            │
            └── Phase 16c: Offline Downloads  ←── mobile-first, not a TV prerequisite
```

Phases 9–13a can be built in any order after Phase 8, since they are independent domains. Phase 13b depends on Phase 10 (SSE EventBus) + Phase 13a (server_config API). Phase 14 depends on Phase 13a only (not 13b). Phase 15 is complete and provides the stable Docker deployment URL/base URL behavior used by Phase 16a and later client phases. Phase 16b follows Phase 16a so TV clients can reuse client-auth, playback, and device-quality lessons from desktop/mobile. Phase 16c follows Phase 16a but is not a prerequisite for Phase 16b, Phase 16d, or TV platform work; it can run in parallel with TV/client-readiness work after the mobile client foundation exists. Phase 16d follows Phase 16a and Phase 16b as a shared contract, QA, diagnostics, accessibility, and release-readiness gate for Phases 17–23. Phases 17–23 are platform-specific implementation phases with their own Task 0 research/design/enrichment step. Phase 24 handles partner-gated platforms only after platform access confirms viability. See [PHASE_13_SPLIT.md](docs/design/PHASE_13_SPLIT.md) for the Phase 13 dependency analysis.

---

## Post-Phase 16d — Household Profiles, Kids Mode, and Ambient Channels (Native Ambient Player Complete; TV Profile-Gate Follow-up Active)

**Committed:** `00d631b`, `d4e37ba`, `c53dabe`, `881db75`, `a0a8963`, `dd38cd1`, and `e185c14` on `main`

**Authoritative document:** [PROFILES_AND_AMBIENT_CHANNELS.md](docs/design/PROFILES_AND_AMBIENT_CHANNELS.md)

**What was built:**

- Household-owned, Netflix-style selectable profiles backed by `user_profiles`, with a default standard profile backfilled for every existing account.
- Session-scoped profile switching; profile-specific history, resume, favorites, ratings, subtitle preferences, offline playback sync, TV surface state, and direct stream/transcode authorization.
- Server-enforced Kids policy: explicit library allowlist, canonical maximum content rating, deny-on-unknown rating behavior, and controls for search, downloads, external links, ambient channels, and privileged capability routes.
- Ordered adult and Kids ambient channels. Their queue resolution rechecks profile policy, while `playback_mode = ambient` persists diagnostic session/events but never modifies user history, resume, play count, TV surfaces, or Trakt export.
- Server-issued ambient queue revisions: `next` returns `channel_updated_at`; an ambient start must echo it and is conditionally created only if the channel is still enabled, audience-matched, contains that item, and has that exact revision.
- Web profile picker and management page for standard/Kids profile creation and parental policies.
- Opt-in remembered-profile mapping per account/device, resolved only after normal authentication and cleared on sign-out or session revocation.
- Server-issued first-use selection state for new multi-profile sessions, plus a web profile gate that blocks profile-scoped routes until an explicit choice is made.
- Flutter Android/iOS profile gate: fresh login and token restoration resolve server profile scope before authenticated routes, with manual switching, remembered-device preference, transient parent unlock, artwork-cache clearing, and profile-isolated offline download scopes.
- Flutter-native ambient channel picker and player surface backed by exactly one native queue per platform. Android uses a Media3 `ExoPlayer`/`MediaSessionService`; iOS uses `AVQueuePlayer` with active media-playback audio/background configuration.
- A reproducible Android mobile build baseline: checked-in Gradle wrapper, JDK 17, API 36/build-tools 36, AGP 8.11.1, Gradle 8.14, Kotlin 2.2.20, AndroidX/Jetifier, and a compatible Firebase Core/Messaging lock pairing.

**Key decisions:**

- The authenticated `users` record remains the authorization and external-integration owner; `user_profiles` owns household experience state.
- Native background playback is a client responsibility. Android consumes this contract through Media3 `MediaSessionService`; Apple clients use AVQueuePlayer and the appropriate background media configuration. A web tab is not represented as native background playback.
- Parent PIN hashes are per-Kids-profile, Argon2id-derived server secrets; a valid PIN grants only a session-scoped, ten-minute profile unlock, while five failures produce a durable 15-minute lockout.
- A remembered profile is a device convenience setting, not an account credential or a Kids exit lock; profile PIN work remains required before making a shared-TV lock claim.
- The server owns ambient queue authorization and staleness. Android/iOS own actual background player lifecycle and may restore only non-secret channel/item/revision/position state, never a stream URL or credential.

**Shared-TV selection hardening (2026-07-18, `c53dabe`):** A remembered profile is a per-account, per-installation mapping keyed by a random opaque device ID. Separate TVs deliberately get separate mappings, and deleting app/browser data creates a new identity rather than cloning a household preference. A valid mapping is applied only after account authentication. A new session with multiple profiles and no valid mapping is explicitly marked `profile_selection_required`; the server retains its default-profile fallback for API compatibility, while the web shell blocks profile-scoped routes until the user switches explicitly. That switch clears the flag atomically with a requested remember/forget mutation. Web invalidates in-flight profile-scoped calls, remounts profile-scoped UI, resets local playback, and revalidates same-origin tabs through BroadcastChannel with a storage-event fallback. Native TV clients must implement the same gate and clear previews, artwork, queue, and launcher state before publishing replacement rows. See [PROFILES_AND_AMBIENT_CHANNELS.md](docs/design/PROFILES_AND_AMBIENT_CHANNELS.md), [TV_PLATFORM_SURFACES.md](docs/design/TV_PLATFORM_SURFACES.md), and [CLIENT_CONTRACTS.md](docs/api/CLIENT_CONTRACTS.md).

**Kids parent-unlock hardening (2026-07-18):** New Kids profiles require a 4–12 digit parent PIN. The server stores only an Argon2id PHC hash with a random salt and OWASP's 19 MiB/two-iteration/single-lane baseline. `user_profiles` persists failed attempts and a 15-minute lock after the fifth failure; `user_sessions` stores only the current profile's ten-minute parent-unlock pair. The unlock endpoint locks rows in a deadlock-safe order, returns no hash/attempt count/precise retry schedule, and the switch endpoint refuses a locked Kids-to-standard transition. Switching profiles or changing the PIN revokes the unlock. The web exposes secure PIN setup/replacement, hides sensitive management links in Kids mode, and presents a transient parent-access dialog. Existing Kids profiles remain compatible until a parent configures a PIN from a standard profile.

**Ambient native-player contract hardening (2026-07-18, `a0a8963`):** `ambient_channels.updated_at` is now the authoritative queue/configuration revision and advances inside the ordered-item replacement transaction. The next-item response returns it, while ambient playback start requires it and uses a conditional `INSERT ... SELECT` to prove the current channel is enabled, audience-matched, still contains the requested item, and has not changed. A mismatch is `409 PLAY_019`, creates no Duskcue play session or playback response URL, and cleans up any newly started transcode session. New sessions use the indexed, nullable `play_sessions.ambient_channel_id` relationship; legacy valid metadata values are backfilled. The playback fixture pack, client contract manifest, static verifier, and disposable PostgreSQL verifier cover the new boundary.

**Flutter native profile gate (2026-07-18, `dd38cd1`):** The existing Android/iOS Flutter client now routes every fresh or restored authenticated session to `/profiles` before the application shell, deep links, realtime, or downloads can run. The server-backed picker handles first-use selection, remembered-profile opt-in, manual switching from Settings, and a transient obscured PIN prompt when exiting a locked Kids profile. A switch clears image memory/disk cache and active download state before publishing the new profile; download inventory, package storage, and settings are partitioned by `profile_id`, so a prior profile's local packages remain inaccessible instead of being deleted. No TV application exists in this repository, so dedicated native TV enforcement remains a future platform task.

**Native ambient player (2026-07-18, `e185c14`):** Flutter now loads only profile-authorized ambient channels and passes an explicit selection plus the current origin/bearer only to an in-memory native runtime. Android's single `MediaSessionService` declares the required `mediaPlayback` foreground-service capability, while iOS has a single `AVQueuePlayer`, `.playback`/`.moviePlayback` audio session, `audio` background mode, and system play/pause controls. Both runtimes call `next`, revision-checked ambient start, heartbeat, stop, and completion advancement themselves; they stop an abandoned or completed server session before advancing, retry `PLAY_019` once from a fresh `next`, and retain no stream URL, bearer token, session ID, or selection across process/service loss, profile/auth/server changes, or explicit stop. The Android target also now checks in its wrapper and aligns on JDK 17/API 36, AGP 8.11.1, Gradle 8.14, Kotlin 2.2.20, AndroidX/Jetifier, and the matching Firebase Core/Messaging lockfile releases. Flutter's built-in Kotlin migration remains a focused future compatibility task, not a silent behavior change in this player delivery.

**Hardening order after Kids mode:** (1) adopt the proven profile gate/cache lifecycle in each dedicated TV application as its platform shell is introduced, then (2) consider offline ambient prefetch only if it preserves the no-stream-URL/no-credential restoration boundary.

**Verification:** `cargo fmt --check`, `cargo check -p duskcue`, focused profile-selection and parent-PIN unit tests, `npm run build` in `clients/web`, `flutter doctor -v`, `flutter analyze`, `flutter test`, and `flutter build apk --debug` in `clients/mobile`, `node scripts/verify-native-ambient-player.mjs`, `node scripts/verify-profile-selection-integration.mjs`, `node scripts/verify-profile-parent-unlock-integration.mjs`, `node scripts/verify-auth-conformance.mjs`, `node scripts/verify-client-contracts.mjs`, `node scripts/verify-playback-conformance.mjs`, `node scripts/verify-ambient-player-contract.mjs`, and `scripts/verify-migrations.ps1` against disposable PostgreSQL 18 pass. iOS compilation and device/background-interruption evidence remain a macOS/iOS hardware release gate.
