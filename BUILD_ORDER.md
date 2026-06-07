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
| `20260530_060200_create_full_text_search.sql` | `rebuild_media_search_vector()` + 4 search triggers + trigram index |
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
Phase 1: Scaffolding (COMPLETE — aaedc05)
    ↓
Phase 2: Database Schema (COMPLETE — 15 migrations)
    ↓
Phase 3: Core Server Infrastructure (COMPLETE — 12 tasks)
    ↓
Phase 4: Auth & Users (NEXT)
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
