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
- `clients/desktop/` (Tauri) — Phase 16
- `clients/mobile/` (Flutter) — Phase 16
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
| `20260530_030000_create_core_media_tables.sql` | `libraries`, `library_paths`, `media_items`, `movies`, `series`, `seasons`, `episodes`, `media_files`, `subtitle_files`, `subtitle_ocr_cache`, `subtitle_sync_data`, `genres`, `media_genres`, `tags`, `media_tags`, `people`, `media_credits`, `artwork` |
| `20260530_030100_create_trakt_integration.sql` | `users` (stub), `trakt_accounts`, `trakt_sync_state` |
| `20260530_030200_create_activity_analytics.sql` | `play_sessions` (partitioned), `play_session_streams`, `play_events` (partitioned), `user_trust_events`, `user_trust_scores` |
| `20260530_030300_create_playback_domain.sql` | `user_item_data` (fillfactor=85), `bookmarks`, `playlists`, `playlist_items` |
| `20260530_040000_create_auth_domain.sql` | `streaming_policies`, `users` ALTER (13 columns added), `user_passkeys`, `user_totp`, `user_capabilities`, `user_library_access`, `user_sessions`, `api_keys`, `invitations`, `device_linking_codes`, `reauth_codes` |
| `20260530_050000_create_system_domain.sql` | `server_config`, `scheduled_tasks`, `scheduled_task_runs`, `notification_types`, `notifications`, `user_notification_preferences` |
| `20260530_060000_create_cross_cutting_concerns.sql` | `pg_trgm` + `pgstattuple` extensions, `audit_log` (partitioned) |
| `20260530_060100_create_audit_triggers.sql` | `audit_trigger_fn()` + 10 audit triggers |
| `20260530_060200_create_full_text_search.sql` | `rebuild_media_search_vector()` + 4 search triggers + trigram index (the PG FTS foundation for v1.0 search; see [SEARCH.md](docs/design/SEARCH.md) for the full search-engine decision and migration path to Meilisearch at scale) |
| `20260530_070000_seed_default_data.sql` | Default `server_config` row, 5 streaming policies, 11 notification types, 18 scheduled tasks |
| `20260530_070100_create_analytics_security.sql` | `user_location_history` + 6 per-table autovacuum overrides |
| `20260530_070200_create_migration_domain.sql` | `migration_sources`, `migration_user_mapping`, `migration_import_log` |
| `20260530_070300_create_quality_domain.sql` | `device_profiles`, `device_capability_tests`, `client_network_reports`, `qoe_reports` |
| `20260530_070400_create_overlays_collections.sql` | `overlay_definitions`, `artwork_overlay_state`, `artwork` ALTER (`is_locked`, `source_type`), `collections`, `collection_items`, `collection_templates` |
| `20260530_070500_create_segments_storyboards.sql` | `media_segments`, `media_fingerprints`, `storyboards` |

**Key decisions made during implementation:**

- All migrations use idempotent patterns (`IF NOT EXISTS`, `DO $$ ... $$`) per MIGRATION_STRATEGY.md
- `users` created as minimal stub in migration 2 (trakt dependency), expanded to full auth schema via idempotent `ALTER TABLE` in migration 5 — `DO $$` blocks check `information_schema.columns` before each ADD COLUMN
- `streaming_policies` created before `users` ALTER in migration 5 because `users.streaming_policy_id` references it
- `play_sessions` and `play_events` include June/July 2026 initial partitions; `audit_log` includes same; application-level partition management creates future partitions
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
| `server/src/domains/auth/service.rs` | Added 6 device linking functions: `generate_device_user_code`, `format_user_code`, `create_device_linking_code`, `poll_device_linking_token`, `verify_device_linking_code`; `CreateDeviceCodeParams` struct |
| `server/src/domains/auth/handlers.rs` | Replaced `todo!()` stubs with working handlers: `device_code`, `device_token`, `device_verify` |

**Key decisions from Task 6:**

- **RFC 8628 device code flow** — Three endpoints: `POST /api/v1/device/code` (device initiates), `POST /api/v1/device/token` (device polls), `POST /api/v1/device/verify` (user approves). All routes and DTOs already existed from Task 1.
- **Device code hashed at rest** — The internal `device_code` (32 random bytes, hex-encoded, 256-bit) is SHA-256 hashed before storage in the `device_linking_codes.device_code` column, consistent with session token pattern. Raw code sent to device once, never stored.
- **User code stored raw** — The 8-char base-20 user code is stored without formatting in `device_linking_codes.user_code`. Dashes are stripped from user input before lookup, so `WDJBMJHT` and `WDJB-MJHT` both match.
- **Verification URI from config** — Built from `RuntimeConfig.base_url + "/link"`, falling back to request `Host` header, then `http://localhost:48027/link`.
- **No explicit denial** — The schema has no `is_denied` column. Users deny by simply not approving; the code expires after `device_linking_code_expiry_seconds` (default 900 = 15 minutes). `DeviceLinkingDenied` and `DeviceLinkingSlowDown` error variants exist for future use.
- **Token exchange cleanup** — After successful token exchange, the `device_linking_codes` row is deleted. This prevents reuse and serves as implicit cleanup. Expired codes are also cleaned on access (delete on poll if expired).
- **Session creation uses stored device metadata** — When the device polls and finds `is_approved = true`, a session is created for `approved_by_user_id` using the `client_name`, `client_platform`, `client_version`, `ip_address`, and `user_agent` from the original device code request.
- **`CreateDeviceCodeParams` struct** — Introduced to satisfy clippy `too_many_arguments` (8 params → 3 params); same pattern as `CreateInvitationParams` from Task 3.
- **No new workspace dependencies** — device code generation uses existing `rand` 0.9 and `BASE20_CHARS`; hashing uses existing `sha256_hex`.
- **Configurable parameters** — `AuthConfig.device_linking_code_length` (default 8), `device_linking_code_expiry_seconds` (default 900), `device_linking_poll_interval_seconds` (default 5) — all from `AuthConfig` defaults in `state.rs`.

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
- Artwork delivery endpoint and WebP variant generation — deferred to Phase 8 follow-up or alongside Phase 10 storyboards. **Note (June 2026):** Full image format policy documented in [IMAGE_FORMATS.md](docs/design/IMAGE_FORMATS.md) — WebP as primary delivery format (AVIF rejected for encode cost on NAS hardware; JPEG XL rejected for browser support). `MediaCard.svelte` currently renders a gradient placeholder because no artwork serving endpoint exists yet; the endpoint will serve WebP variants at standard sizes (w185/w342/w500/original) with `<picture>` JPEG fallback.
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
- **No task timeout in executor wrapper** — individual task timeout handled inside the spawned `JoinHandle` via `tokio::time::timeout`; timeout value comes from `scheduled_tasks.timeout_seconds` (default: 3600) — but currently the inner handler's own timeout (3600s hardcoded) applies; the outer spawn just awaits the `JoinHandle`

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
| `server/migrations/20260617_080000_add_user_item_data_metadata.sql` | Adds `metadata JSONB NOT NULL DEFAULT '{}'` column to `user_item_data` for per-user per-item subtitle offset storage (`metadata->>'subtitle_offset_ms'`) |
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
 | `server/migrations/20260619_080000_seed_subtitle_auto_fetch_task.sql` | Seeds `subtitle_auto_fetch` scheduled task (1800s interval, `is_enabled = false`, 1800s timeout) for existing deployments |

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
- **Validation statics match DB CHECK constraints exactly** — `VALID_SEGMENT_TYPES = ["intro", "credits", "recap", "preview", "outro"]` and `VALID_SEGMENT_SOURCES = ["chapter", "chromaprint", "blackframe", "silence", "manual", "combined"]` mirror the `media_segments.segment_type` and `media_segments.source` CHECK constraints from migration `20260530_070500_create_segments_storyboards.sql`. Service-layer validation (Task 2) will catch invalid values before they hit the DB constraint, returning VALID_001 instead of INTERNAL
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

3. Create `server/src/domains/storyboards/` — five-file pattern
4. Implement `server/src/services/storyboards.rs`:
   - FFmpeg thumbnail extraction at adaptive intervals
   - WebP spritesheet generation
   - WebVTT seek file generation
5. Implement `server/src/workers/segment_detector.rs` — background segment detection
6. Implement `server/src/workers/storyboard_generator.rs` — background thumbnail generation
7. Implement skip button in web client player — `SkipButton.svelte`
8. Implement seek preview in web client player — `SeekPreview.svelte`

**Strategic implementation debt absorbed into Phase 10** (per [IMPLEMENTATION_DEBT.md](docs/design/IMPLEMENTATION_DEBT.md) — storyboards are the first consumer of these items):

9. Implement `server/src/services/image_pipeline.rs` — WebP encode, resize, variant generation (debt from [IMAGE_FORMATS.md](docs/design/IMAGE_FORMATS.md); storyboards produce WebP sprites via this same service)
10. Implement artwork delivery endpoint `GET /api/v1/items/{id}/artwork/{type}?size={size}` — serves WebP variants from `image_pipeline.rs` (debt from [IMAGE_FORMATS.md](docs/design/IMAGE_FORMATS.md); web client currently renders gradient placeholders)
11. Implement SSE endpoint `GET /api/v1/events` + `EventBus` in AppState — `DashMap<Uuid, broadcast::Sender>` per user (debt from [REAL_TIME_PUSH.md](docs/design/REAL_TIME_PUSH.md); storyboard generation progress is the first SSE consumer)
12. Implement `clients/web/src/lib/stores/events.js` — Svelte store managing `EventSource` lifecycle; dispatches to domain stores (debt from [REAL_TIME_PUSH.md](docs/design/REAL_TIME_PUSH.md); player subscribes to storyboard-ready events)

**Verification:** After detection runs, media items have intro/credit markers. Skip button appears during intros in player. Seek bar shows thumbnail previews. Artwork renders on MediaCard (no more gradient placeholders). Storyboard generation progress streams via SSE.

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
5. Implement backup coordination — WAL-G status check, pg_dump trigger, verification
6. Implement `server/src/workers/backup_runner.rs` — scheduled backup execution
7. Implement `server/src/workers/reindex_maintenance.rs` — weekly REINDEX CONCURRENTLY
8. Implement `server/src/workers/disk_space_check.rs` — 30-minute disk monitoring
9. Build admin settings UI — all `server_config` JSONB fields as toggles, sliders, dropdowns; push/webhook config fields visible but annotated "Activation requires Phase 13b — notification dispatch"

**Verification:** Admin can configure all settings via UI. Backups run on schedule. Disk space alerts trigger when thresholds are exceeded. Scheduled tasks are visible and triggerable.

---

## Phase 13b — Notification System

**Goal:** Notification dispatch with multi-channel delivery (in-app + SSE + webhook), localized templates via Fluent, and push device registration for future mobile push.

**Prerequisites:** Phase 10 (SSE EventBus), Phase 13a Task 2 (server_config API for push/webhook config). Can overlap with Phase 14 if capacity allows.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [I18N.md](docs/design/I18N.md) | **i18n prerequisite** — Fluent server-side infrastructure; notification templates use Fluent message IDs, not English template strings |
| [MOBILE_PUSH.md](docs/design/MOBILE_PUSH.md) | **Multi-channel dispatch architecture** — in-app + SSE + webhook always available; mobile push via FCM/APNs/UnifiedPush opt-in |
| [PHASE_13_SPLIT.md](docs/design/PHASE_13_SPLIT.md) | **Phase split rationale** — Phase 13b's 6 tasks; dependency analysis; MVP fallback |

**Tasks:**

1. Set up Fluent server-side i18n — `fluent-i18n` crate, `server/locales/en/notifications.ftl`, migrate `notification_types.in_app_template` from English strings to Fluent message IDs (debt item #5 from [IMPLEMENTATION_DEBT.md](docs/design/IMPLEMENTATION_DEBT.md))
2. Implement multi-channel dispatch pipeline — notification record always in DB; fan-out to in-app + SSE + webhook simultaneously; mobile push channel included in fan-out but client implementation deferred to Phase 16 (debt item #6 from [IMPLEMENTATION_DEBT.md](docs/design/IMPLEMENTATION_DEBT.md))
3. Implement notification CRUD — create, list, mark-as-read, delete; notification types and user preferences from Phase 2 tables
4. Implement webhook dispatch — HTTP POST to operator-configured URL with ntfy/Gotify/Discord/Slack/generic formats; HMAC signing; retry with backoff (debt item #8 from [IMPLEMENTATION_DEBT.md](docs/design/IMPLEMENTATION_DEBT.md))
5. Create `user_push_devices` table + `POST /api/v1/user/push-devices` API — device registration for FCM/APNs/UnifiedPush tokens; token lifecycle (heartbeat, auto-invalidation, manual revoke) (debt item #7 from [IMPLEMENTATION_DEBT.md](docs/design/IMPLEMENTATION_DEBT.md))
6. Build notifications UI — notification center, preferences, push device management, per-channel opt-in per notification type

**Verification:** Admin triggers a test notification. Notification appears in-app (notification center), via SSE (live update if web client is open), and via webhook (operator-configured endpoint). Notification templates render in the user's preferred locale via Fluent. Push devices register and display in user settings.

**MVP fallback:** If Phase 13b takes longer than estimated, ship in-app + SSE + webhook only. Defer FCM/APNs/UnifiedPush client implementations to Phase 16. The `user_push_devices` table and API still ship (schema-only) to avoid Phase 16 schema migration. See [PHASE_13_SPLIT.md](docs/design/PHASE_13_SPLIT.md) for details.

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

## Pre-v1.0 Hardening

**Goal:** Close implementation debt from strategic design decisions that doesn't block features but improves v1.0 quality. Per [IMPLEMENTATION_DEBT.md](docs/design/IMPLEMENTATION_DEBT.md).

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [HTTP_CACHING.md](docs/design/HTTP_CACHING.md) | Cache-Control + ETag response headers on metadata/artwork endpoints |
| [I18N.md](docs/design/I18N.md) | Paraglide JS adoption — extract web client strings to `en.json` |
| [SEARCH.md](docs/design/SEARCH.md) | Faceted search UI (genre/year/rating filter pills) |
| [IMPLEMENTATION_DEBT.md](docs/design/IMPLEMENTATION_DEBT.md) | Full debt catalog and scheduling rationale |

**Tasks:**

1. Implement Cache-Control + ETag response headers — `SetResponseHeaderLayer` per resource group in `router.rs`; SHA-256 ETag on single-item metadata endpoints; per-endpoint `max-age` + `stale-while-revalidate` per [HTTP_CACHING.md](docs/design/HTTP_CACHING.md) table
2. Adopt Paraglide JS — `@inlang/paraglide-js` Vite plugin; extract existing web client UI strings to `clients/web/messages/en.json`; configure URL prefix + cookie locale strategy per [I18N.md](docs/design/I18N.md)
3. Build faceted search UI — genre/year/rating/type filter pills on search results page; uses existing PG FTS with parallel GROUP BY queries per [SEARCH.md](docs/design/SEARCH.md)
4. Add Prometheus metrics for new infrastructure — SSE connection count, event publish rate, image variant generation throughput + cache hit rate, search query latency p50/p95/p99, push delivery success rate per channel; extends existing `init_metrics()` from Phase 3

**Verification:** Metadata endpoints return ETag headers; conditional requests return 304. Web client strings are in `en.json` and wrapped in Paraglide `m.*` calls. Search results page has genre/year/rating filters. Grafana dashboard shows SSE connections and search latency.

---

## Phase 15 — Docker & Deployment

**Goal:** Production-ready Docker image with embedded PostgreSQL.

**Authoritative docs:**

| Doc | What to build from it |
|---|---|
| [DOCKER_DEPLOYMENT.md](docs/operations/DOCKER_DEPLOYMENT.md) | **Primary** — hybrid embedded/external PG, volume strategy, security hardening |
| [OS_HARDENING.md](docs/operations/OS_HARDENING.md) | Docker Engine version minimums, Alpine 3.22 pinning |
| [CACHE_STORAGE.md](docs/operations/CACHE_STORAGE.md) | Docker volumes: `duskcue-data`, `duskcue-cache`, tmpfs for transcode |
| [SEARCH.md](docs/design/SEARCH.md) | **Post-v1.0 follow-up** — optional Meilisearch sidecar (loopback, same container) for libraries exceeding 50k items; default v1.0 ships PG FTS only, no sidecar |
| [MULTI_INSTANCE.md](docs/design/MULTI_INSTANCE.md) | **Deployment topology constraint** — Duskcue is single-instance by design. Phase 15 must ship single-container as the canonical topology (`replicas: 1` for any Kubernetes examples). No load-balancer-multi-instance pattern; HA via container `restart: unless-stopped` + embedded PG crash recovery. |
| [REVERSE_PROXY.md](docs/design/REVERSE_PROXY.md) | **Exposed-mode operator guidance** — built-in rustls TLS handles simple single-domain exposed mode (no proxy needed); Caddy is the recommended external proxy for multi-service routing. Canonical Caddyfile + docker-compose.yml + Nginx/Traefik alternatives documented. Critical `DUSKCUE_TRUSTED_PROXIES` config for client IP detection. |

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
    ├── Phase 10: Segments & Storyboards (+ SSE + image pipeline + artwork endpoint)
    ├── Phase 11: Analytics & Trakt
    ├── Phase 12: Kometa-Like System
    ├── Phase 13a: System Operations Core (config + backup + maintenance)
    │       ↓
    │   Phase 13b: Notification System (Fluent + dispatch + push)  ←── can overlap with Phase 14
    │       │
    ├── Phase 14: Platform Migration  ←── proceeds after 13a, independent of 13b
    │       │
    ├── Pre-v1.0 Hardening (Cache-Control/ETag + Paraglide + faceted search + metrics)
    │       │
    └── Phase 15: Docker & Deployment  ←── needs 13a + 13b + 14 for complete v1.0 image
            ↓
        Phase 16: Desktop & Mobile Clients  ←── mobile push needs 13b
```

Phases 9–13a can be built in any order after Phase 8, since they are independent domains. Phase 13b depends on Phase 10 (SSE EventBus) + Phase 13a (server_config API). Phase 14 depends on Phase 13a only (not 13b). Phase 15 needs all prior phases for a complete v1.0 image. See [PHASE_13_SPLIT.md](docs/design/PHASE_13_SPLIT.md) for the full dependency analysis.
