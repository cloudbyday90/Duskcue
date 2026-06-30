# Logging & Observability Strategy

## Overview

Structured logging, request tracing, metrics collection, and optional distributed tracing for the Rust Duskcue. All observability is built on the `tracing` ecosystem (tokio-rs), the `metrics` facade, and optional OpenTelemetry integration.

## Crate Selection

| Crate | Version | Maintainer | Role |
|---|---|---|---|
| `tracing` | 0.1.44 | tokio-rs | Instrumentation API — spans + events, async-aware |
| `tracing-subscriber` | 0.3.23 | tokio-rs | Layer-based subscriber: formatting, filtering, JSON output |
| `tracing-appender` | 0.2.5 | tokio-rs | Non-blocking rolling file appender |
| `tracing-error` | 0.2.1 | tokio-rs | Captures span context in error chains |
| `tower-http` (trace) | 0.6.11 | tower-rs | Axum-native HTTP request/response tracing middleware |
| `metrics` | 0.24.6 | metrics-rs | Lightweight metrics facade (counters, gauges, histograms) |
| `metrics-exporter-prometheus` | 0.18.3 | metrics-rs | Prometheus `/metrics` scrape endpoint |
| `tracing-opentelemetry` | 0.33.0 | tokio-rs | Bridge: tracing spans → OpenTelemetry (optional) |
| `opentelemetry` | 0.32.0 | open-telemetry | OTel API (optional) |
| `opentelemetry-otlp` | (via OTel SDK) | open-telemetry | OTLP exporter — Jaeger, Tempo, etc. (optional) |

### Why These Crates

| Crate | Strength | Limitation | Our Use |
|---|---|---|---|
| **tracing** | De facto standard; async-aware spans; `#[instrument]`; `log` compat | 0.1.x semver | All structured diagnostics |
| **tracing-subscriber** | Composable layers; `EnvFilter`; JSON output; `reload` | None significant | Subscriber setup and formatting |
| **tracing-appender** | Non-blocking writes; rolling files; `WorkerGuard` flush | None significant | File logging without blocking runtime |
| **tracing-error** | `SpanTrace` captures span context at error creation | Experimental | Error enrichment in development mode |
| **tower-http TraceLayer** | Axum-native; automatic request spans; customizable callbacks | None significant | HTTP request/response tracing |
| **metrics** | Facade pattern; zero-cost noop; labels | 0.2x semver | All metrics emission |
| **metrics-exporter-prometheus** | Native histograms; protobuf; push gateway; IP allowlist | Requires HTTP port | Prometheus scrape endpoint |
| **tracing-opentelemetry** | Vendor-neutral distributed tracing; same `tracing` instrumentation | Pre-1.0 OTel SDK; breaking changes | Optional feature flag |

### Rejected Alternatives

| Crate | Why Not |
|---|---|
| **log** | Superseded by `tracing` — no spans, no async awareness, no structured fields |
| **slog** | Predates `tracing`; less ecosystem support; no async spans |
| **fern** | `log`-based; no structured output; no spans |
| **opentelemetry-prometheus** (direct) | Redundant — `metrics` + `metrics-exporter-prometheus` is lighter and more flexible |
| **prometheus** (direct crate) | Direct Prometheus client bindings; `metrics` facade is more composable |
| **tracing-bunyan-formatter** | JSON output from `tracing-subscriber` covers this; Bunyan format is Node-centric |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Application Code                            │
│  tracing::info!(), #[instrument], metrics::counter!, etc.      │
└───────────┬─────────────────────────────┬───────────────────────┘
            │                             │
            ▼                             ▼
┌───────────────────────┐     ┌───────────────────────┐
│   tracing Subscriber  │     │    metrics Recorder   │
│   (Layer Registry)    │     │  (PrometheusExporter) │
├───────────────────────┤     ├───────────────────────┤
│ Layer 1: ErrorLayer   │     │  Counters             │
│   (tracing-error)     │     │  Gauges               │
│ Layer 2: fmt Layer    │     │  Histograms           │
│   (console: pretty)   │     └───────────┬───────────┘
│ Layer 3: fmt Layer    │                 │
│   (file: JSON)        │                 ▼
│ Layer 4: OTel Layer   │     ┌───────────────────────┐
│   (optional)          │     │  /metrics endpoint    │
└───────────────────────┘     │  (embedded in router) │
                              └───────────────────────┘
```

---

## Structured Logging

### Output Format

| Target | Format | Use Case |
|---|---|---|
| **Console** | Pretty (colored, human-readable) | Development, `docker logs` |
| **File** | JSON (structured, machine-parseable) | Production, log aggregation (Loki, Elastic) |

JSON output uses `tracing-subscriber`'s `json` feature. Each log line is a self-contained JSON object.

Example JSON output:

```json
{
  "timestamp": "2026-05-30T14:23:01.456789Z",
  "level": "INFO",
  "target": "server::api::libraries",
  "span": {"name": "HTTP request", "method": "GET", "path": "/api/v1/libraries"},
  "fields": {"message": "Library scanned", "library_id": "01912abc-def4-7xyz...", "item_count": 142},
  "threadId": 1
}
```

### Log Levels

| Level | Use | Example |
|---|---|---|
| `ERROR` | Unrecoverable failures requiring attention | Database connection lost, WAL-G backup failed |
| `WARN` | Degraded behavior, recoverable issues | Transcode fallback to software, token refresh failed |
| `INFO` | Significant business events | Library scan complete, user login, playback started |
| `DEBUG` | Diagnostic information for troubleshooting | SQL query executed, cache hit/miss, middleware chain |
| `TRACE` | Very verbose internals | HTTP headers, frame-by-frame transcode progress |

### Filtering

`EnvFilter` with `RUST_LOG` support for fine-grained control:

```bash
RUST_LOG=info
RUST_LOG=server=debug,sqlx=warn
RUST_LOG=server::api::media=trace
```

Default: `info` (from `BootstrapConfig.log_level`, see CONFIGURATION.md).

The filter can be reloaded at runtime via `tracing-subscriber::reload` — the admin API exposes an endpoint to change log level without restart.

---

## File Logging

### Configuration

Stored in `server_config.logging` JSONB (see DATABASE.md):

```json
{
  "level": "info",
  "max_file_size_mb": 10,
  "max_files": 5,
  "format": "json"
}
```

| Field | Type | Default | Description |
|---|---|---|---|
| `level` | String | `"info"` | Minimum log level (trace/debug/info/warn/error) |
| `max_file_size_mb` | u32 | `10` | Max size per log file before rotation |
| `max_files` | u32 | `5` | Number of rotated log files to retain |
| `format` | String | `"json"` | File output format: `json` or `text` |

### Rust Struct

```rust
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct LoggingConfig {
    pub level: String,
    pub max_file_size_mb: u32,
    pub max_files: u32,
    pub format: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            max_file_size_mb: 10,
            max_files: 5,
            format: "json".to_string(),
        }
    }
}
```

### Rolling File Appender

```rust
use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::layer::SubscriberExt;

let file_appender = RollingFileAppender::builder()
    .rotation(tracing_appender::rolling::Rotation::DAILY)
    .filename_prefix("server")
    .filename_suffix("log")
    .max_log_files(config.logging.max_files)
    .build(log_dir)
    .unwrap();

let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
```

`WorkerGuard` (`guard`) is held for the lifetime of the application — dropped on shutdown to flush buffered logs.

Log files are written to `{data_dir}/logs/`.

---

## HTTP Request Tracing

### tower-http TraceLayer

```rust
use axum::Router;
use tower_http::trace::{TraceLayer, DefaultMakeSpan, DefaultOnResponse};
use tracing::Level;

let app = Router::new()
    .layer(
        TraceLayer::new_for_http()
            .make_span_with(
                DefaultMakeSpan::new()
                    .level(Level::INFO)
                    .include_headers(false)
            )
            .on_response(
                DefaultOnResponse::new()
                    .level(Level::INFO)
                    .latency_unit(tower_http::LatencyUnit::Millis)
            )
    );
```

### Request Span Fields

Every HTTP request automatically gets a tracing span with:

| Field | Source | Example |
|---|---|---|
| `method` | HTTP method | `GET`, `POST` |
| `uri` | Request URI | `/api/v1/libraries` |
| `status` | Response status code | `200`, `404`, `500` |
| `latency` | Request duration | `12ms` |
| `request_id` | `X-Request-ID` header (if present) | `abc123` |

### Request ID Propagation

A separate tower middleware generates or propagates `X-Request-ID`:

- If incoming request has `X-Request-ID` header → use it (for distributed tracing)
- If not → generate UUIDv7 and set on both request context and response header
- Request ID is recorded as a field on the tracing span
- Included in error responses (see ERROR_HANDLING.md `trace_id` field)

---

## Error Enrichment

### tracing-error Integration

`tracing-error::ErrorLayer` captures the current span context when errors are created:

```rust
use tracing_subscriber::prelude::*;

let subscriber = tracing_subscriber::Registry::default()
    .with(tracing_error::ErrorLayer::default());
```

Errors created with `.in_current_span()` carry a `SpanTrace`:

```rust
use tracing_error::prelude::*;

let result = some_fallible_operation().in_current_span()?;
```

In development mode (`environment: development`), error responses include the span trace for debugging. In production, only the error code is returned (see ERROR_HANDLING.md).

### thiserror Integration

Domain error types include a `trace_id` field populated from the request context:

```rust
#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("Library not found: {library_id}")]
    NotFound {
        library_id: uuid::Uuid,
        trace_id: Option<String>,
    },
}
```

---

## Metrics

### metrics Facade

The `metrics` crate provides a lightweight API that libraries emit to without knowing the exporter:

```rust
use metrics::{counter, histogram, gauge};

counter!("http.requests.total", "method" => "GET", "status" => "200").increment(1);
histogram!("http.request.duration", "method" => "GET").record(duration);
gauge!("transcode.active_sessions").set(active_count as f64);
```

### Prometheus Exporter

Embedded in the existing axum router — no separate HTTP port needed:

```rust
use metrics_exporter_prometheus::PrometheusBuilder;
use axum::Router;

let recorder = PrometheusBuilder::new()
    .install_recorder()
    .unwrap();

let metrics_route = axum::routing::get(move || {
    let handle = recorder.clone();
    async move { handle.render() }
});

let app = Router::new()
    .route("/metrics", metrics_route)
    // ... other routes
```

Access control: the `/metrics` endpoint is **not** behind auth (Prometheus scrapers can't authenticate). Instead, `server_config.network` JSONB includes an `allowed_metrics_subnets` field (default: `["127.0.0.1/32", "::1/128"]`) enforced by a tower middleware layer. Only local and explicitly allowed subnets can scrape metrics.

### Metric Categories

| Category | Examples | Type |
|---|---|---|
| **HTTP** | `http.requests.total`, `http.request.duration`, `http.responses.total` | Counter + Histogram |
| **Playback** | `playback.sessions.active`, `playback.session.duration`, `playback.transcode.duration` | Gauge + Histogram |
| **Library** | `library.items.total`, `library.scan.duration`, `library.files.total_size_bytes` | Gauge + Histogram + Counter |
| **Database** | `db.pool.connections.active`, `db.pool.connections.idle`, `db.query.duration` | Gauge + Histogram |
| **System** | `system.uptime_seconds`, `system.memory.usage_bytes`, `system.memory.total_bytes`, `system.memory.usage_percent`, `system.memory.pressure_events`, `system.memory.pressure_stall_percent`, `system.cpu.usage_percent`, `system.cpu.usage_average_percent`, `system.cpu.pressure_events`, `system.cpu.thermal_celsius`, `system.cpu.cores_total`, `system.cpu.big_cores`, `system.cpu.hw_accel`, `transcode.rejections_total`, `transcode.ffmpeg_threads` | Gauge + Counter |
| **Trakt** | `trakt.sync.operations.total`, `trakt.sync.duration`, `trakt.sync.errors.total` | Counter + Histogram |
| **Transcode** | `transcode.jobs.active`, `transcode.jobs.duration`, `transcode.hardware.accel.used`, `transcode.rejections_total`, `transcode.kill_total`, `transcode.zombie_reaped_total` | Gauge + Histogram + Counter |
| **Analytics** | `analytics.geoip.lookups_total`, `analytics.geoip.lookup_duration`, `analytics.geoip.database_age_hours`, `analytics.trust.events_total`, `analytics.trust.events_suppressed_total`, `analytics.trust.score_average`, `analytics.trust.score_minimum` | Counter + Histogram + Gauge |
| **Real-time events** | `sse_connections`, `sse_connected_users`, `sse_events_published_total` | Gauge + Counter |
| **Image variants** | `image_variant_requests_total`, `image_variant_generations_total`, `image_variant_generation_duration_seconds` | Counter + Histogram |
| **Search** | `search_queries_total`, `search_query_duration_seconds` | Counter + Histogram |
| **Notifications** | `notification_delivery_total` | Counter |

### Standard Labels

All metrics include consistent labels:

| Label | Example | Description |
|---|---|---|
| `method` | `GET` | HTTP method |
| `status` | `200` | HTTP status code |
| `library_id` | `01912abc...` | Library UUID (library metrics) |
| `media_type` | `movie` | Media item type |
| `hardware_accel` | `nvenc` | Hardware acceleration method |
| `channel` | `webhook` | Notification delivery channel |
| `result` | `cache_hit` | Bounded operation result |
| `has_filters` | `true` | Whether a search query used filters |

---

## OpenTelemetry (Optional)

### Feature Flag

OpenTelemetry is opt-in via a Cargo feature flag:

```toml
[features]
default = []
otel = ["tracing-opentelemetry", "opentelemetry", "opentelemetry-otlp"]
```

When enabled, an additional subscriber layer bridges `tracing` spans to OTel:

```rust
#[cfg(feature = "otel")]
{
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(opentelemetry_otlp::new_exporter().tonic())
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    subscriber = subscriber.with(otel_layer);
}
```

### Configuration

OTel settings stored in `server_config.integrations` JSONB:

```json
{
  "classifarr_enabled": false,
  "otel_enabled": false,
  "otel_endpoint": "http://localhost:4317",
  "otel_service_name": "media-server"
}
```

When `otel_enabled` is `true` and the `otel` feature is compiled in, distributed tracing is activated. Otherwise, zero overhead.

### Why Optional

- Most self-hosted users won't run a Jaeger/Tempo collector
- OTel SDK is pre-1.0 — breaking changes between minor versions
- Adds build complexity and binary size
- Feature flag means zero cost when disabled

---

## Subscriber Initialization

The full subscriber is built at startup in the logging module:

```rust
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};
use tracing_error::ErrorLayer;

fn init_logging(config: &LoggingConfig, log_dir: &Path) -> WorkerGuard {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));

    let file_appender = RollingFileAppender::builder()
        .rotation(RollingFileAppender::Rotation::DAILY)
        .filename_prefix("server")
        .filename_suffix("log")
        .max_log_files(config.max_files)
        .build(log_dir)
        .expect("failed to create log directory");

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let console_layer = tracing_subscriber::fmt::layer()
        .pretty();

    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(non_blocking);

    let subscriber = Registry::default()
        .with(env_filter)
        .with(ErrorLayer::default())
        .with(console_layer)
        .with(file_layer);

    #[cfg(feature = "otel")]
    if config.otel_enabled {
        let otel_layer = build_otel_layer(&config.otel_endpoint, &config.otel_service_name);
        subscriber = subscriber.with(otel_layer);
    }

    subscriber.init();

    guard
}
```

The `WorkerGuard` is returned and held in main for the lifetime of the process. On drop (shutdown), it flushes all buffered log writes.

---

## Integration with Existing Systems

### Configuration (CONFIGURATION.md)

Bootstrap `log_level` initializes logging before the database is available. After `server_config` loads, the `logging` JSONB config takes over — including file rotation settings and format. The filter can be hot-reloaded via the admin API.

### Error Handling (ERROR_HANDLING.md)

- `trace_id` field in RFC 9457 Problem Details comes from the request span's `request_id` field
- `tracing-error` enriches error types with `SpanTrace`
- In `development` environment, error responses include span trace for debugging
- Error events are emitted at appropriate levels: `ERROR` for 5xx, `WARN` for 4xx

### Database (DATABASE.md)

- `server_config.logging` JSONB stores all logging configuration
- SQL query durations recorded via `histogram!("db.query.duration")`
- Connection pool stats recorded via `gauge!("db.pool.connections.active")`
- Migration output logged at `INFO` level during startup

### Backup & Recovery (BACKUP_RECOVERY.md)

- WAL-G operations logged at `INFO` (success) and `ERROR` (failure)
- Backup verification results logged with full context
- `tracing` spans wrap backup operations for timing and error tracking

### Project Structure (PROJECT_STRUCTURE.md)

Logging initialization lives in `server/src/logging.rs` (or `server/src/observability.rs`), not in a domain module — it's cross-cutting infrastructure, not a business domain.

---

## Log File Management

### Rotation

- **Rotation**: Daily (via `tracing-appender`)
- **Retention**: Configured by `max_files` (default: 5 files, ~5 days)
- **Location**: `{data_dir}/logs/server.log.YYYY-MM-DD`

### Log Sanitization

The following are never written to log files:

- Database connection strings (passwords)
- User passwords or password hashes
- API keys and tokens (only last 4 characters logged: `...abcd`)
- Session tokens
- Full request bodies (only method, path, status, latency)

This is enforced via a custom `tracing-subscriber` formatter that redacts sensitive fields.

---

## Personal Information in Logs

### What Counts as Personal Information

In a self-hosted Duskcue, "personal information" means data that identifies a real person or reveals their viewing habits. The server handles very little sensitive personal data — no financial information, no health records, no government IDs. The primary concerns are:

- **Email addresses** — used for account setup and notifications
- **IP addresses** — visible in HTTP request logs when the server is exposed
- **Viewing history** — what a user watches and when (stored in the database, not in logs)

### Handling Rules by Data Type

| Data Type | Example | Logging Rule | Why |
|---|---|---|---|
| Email address | `user@example.com` | Never logged at `info` level or below. At `debug`/`trace`, first 3 characters + `***@***` (e.g., `use***@***`) | Email identifies a real person; prevents email harvesting from log files |
| Display name | `John Smith` | Never logged at `info` level or below. At `debug`/`trace`, logged fully | Display names are chosen by the user and may be their real name |
| IP address | `192.168.1.42` | Mask last octet in `info`+ level: `192.168.1.xxx`. Full IP at `debug`/`trace` level | IP addresses reveal network topology and location; masking protects privacy in normal logs |
| User ID (UUIDv7) | `01912abc-...` | Full UUID allowed at all levels | UUIDs are internal identifiers; they do not identify a person without database access |
| Session ID | `mv_sess_abc123...` | First 8 characters only at all levels | Session IDs are bearer tokens; full session ID in logs enables session hijacking if log files are compromised |
| Device ID | `dev_abc123...` | First 8 characters only at all levels | Same rationale as session IDs |
| Media item ID (UUIDv7) | `01912def-...` | Full UUID allowed at all levels | Media IDs are not personal data; they identify a movie or episode |
| Invite codes | `INV-abc123` | Never logged | Invite codes grant account creation access; logging them allows unauthorized account creation |
| Watch history | "User played Movie X" | Never include media title + user in the same log line at `info`+ | Combining user identity with viewing history reveals personal habits |
| Library paths | `/media/TV Shows/Kids TV/` | Logged at `info` level during scans | File paths are server configuration, not personal data |

### Implementation

The sanitization is implemented as a `tracing-subscriber` layer that transforms fields before they reach the writer:

```rust
pub struct PiiSanitizerLayer;

impl<S: Subscriber> Layer<S> for PiiSanitizerLayer {
    fn on_event(&self, event: &Event, ctx: Context<S>) {
        let mut visitor = PiiVisitor::new();
        event.record(&mut visitor);
    }
}

struct PiiVisitor {
    fields: BTreeMap<String, String>,
}

impl Visit for PiiVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        let sanitized = match field.name() {
            "email" => mask_email(value),
            "ip" | "remote_addr" => mask_ip(value),
            "session_id" | "device_id" => truncate_id(value),
            "invite_code" => "***".to_string(),
            _ => value.to_string(),
        };
        self.fields.insert(field.name().to_string(), sanitized);
    }
}
```

### Default Behavior

The default log level is `info`. At this level, all personal information is sanitized according to the rules above. The admin can enable `debug` or `trace` for troubleshooting, which exposes more detail but still redacts the most sensitive fields (invite codes, full session IDs, passwords).

The admin UI displays a notice when debug logging is active: "Debug logging is enabled. Log files may contain email addresses and IP addresses. Disable debug logging when troubleshooting is complete."

---

## Observability Dashboard

While not part of the server binary, the recommended stack for self-hosted observability:

| Component | Purpose | Recommended |
|---|---|---|
| **Metrics storage** | Time-series database | Prometheus (via `/metrics` endpoint) |
| **Log aggregation** | Centralized log search | Grafana Loki (JSON logs are Loki-compatible) |
| **Visualization** | Dashboards | Grafana |
| **Distributed tracing** | Request flow visualization | Grafana Tempo (optional, via OTel feature) |
| **Alerting** | Threshold-based notifications | Grafana Alerting → our notification system |

The server exposes the data (metrics endpoint, JSON logs, optional OTel traces). Admins choose whether to consume it with Grafana, use the built-in Tautulli-style analytics, or both.

---

## Implementation Notes

### Phase 3 (Task 9) — Tracing Subscriber

**Module:** `server/src/logging.rs`

**What was implemented:**

- `init_logging(log_level: &str, data_dir: &Path) -> WorkerGuard` — builds and installs the layered tracing subscriber at startup step 3
- Layer stack (outermost → innermost): `EnvFilter` → `ErrorLayer` (tracing-error) → console fmt layer (pretty) → file fmt layer (JSON)
- `RollingFileAppender::builder()` with `Rotation::DAILY`, prefix `server`, suffix `log`, `max_log_files(5)` — writes to `{data_dir}/logs/`
- `WorkerGuard` returned and held in `main()` as `_log_guard` — dropped on shutdown to flush buffered file writes
- `EnvFilter` reads `RUST_LOG` env var first, falls back to bootstrap `log_level`
- `LoggingConfig` in `state.rs` expanded from empty placeholder to full struct with `level`, `max_file_size_mb`, `max_files`, `format` fields
- `tracing-error` v0.2.1 added to workspace dependencies

### Phase 3 (Task 10) — Prometheus Metrics Endpoint

**Module:** `server/src/logging.rs` (init_metrics), `server/src/middleware.rs` (track_http_metrics, metrics_subnet_guard), `server/src/router.rs` (/metrics route)

**What was implemented:**

- `init_metrics() -> PrometheusHandle` in `logging.rs` — installs the global `metrics` recorder via `PrometheusBuilder::new().install_recorder()`; returns a `PrometheusHandle` for the `/metrics` route handler; custom histogram buckets for `http_request_duration` (5ms to 10s)
- `/metrics` GET endpoint in `router.rs` — returns Prometheus text format via `state.metrics_handle.render()`; subnet-guarded by `metrics_subnet_guard` middleware
- `track_http_metrics` middleware in `middleware.rs` — records `http_requests_total` (counter, labels: method, status) and `http_request_duration` (histogram, label: method) for all requests except `/metrics` (avoids self-referential metrics)
- `metrics_subnet_guard` middleware in `middleware.rs` — checks client IP against `state.metrics_allowed_subnets` (parsed `IpNet` CIDRs); returns 403 Forbidden for disallowed IPs; uses same `extract_client_ip` logic as rate limiter (X-Forwarded-For → X-Real-IP → ConnectInfo → fallback)
- `NetworkConfig` expanded from empty placeholder to include `allowed_metrics_subnets: Vec<String>` — defaults to `["127.0.0.1/32", "::1/128"]` (localhost only); parsed to `Vec<IpNet>` at startup and stored as `Arc<Vec<IpNet>>` in `AppState`
- `PrometheusHandle` added to `AppState` — `Clone`-able handle for rendering metrics
- `metrics` v0.24, `metrics-exporter-prometheus` v0.18, `ipnet` v2 added to workspace dependencies
- Metrics recorder installed at startup step 3 (after logging init, before router build) — ensures all subsequent operations emit metrics
- HTTP metrics middleware placed between TraceLayer and CorsLayer in the middleware stack — all requests tracked (including rate-limited 429s), within trace span context

### Pre-v1.0 Task 4 — Cross-Cutting Infrastructure Metrics

**Module:** `server/src/logging.rs`, `server/src/services/event_bus.rs`, `server/src/services/artwork_delivery.rs`, `server/src/domains/search/service.rs`, `server/src/services/notification_dispatch.rs`

**What was implemented:**

- `init_metrics()` now registers histogram buckets for `search_query_duration_seconds` and `image_variant_generation_duration_seconds` in addition to the existing HTTP duration buckets.
- SSE metrics: `sse_connections_opened_total`, `sse_connections_rejected_total`, `sse_connections`, `sse_connected_users`, and `sse_events_published_total{event_type,delivered}`.
- Image variant metrics: `image_variant_requests_total{category,variant,result}` for cache-hit vs generated request accounting, `image_variant_generations_total{category,variant,status}`, and `image_variant_generation_duration_seconds{category,variant,status}`.
- Search metrics: `search_queries_total{status,has_filters}` and `search_query_duration_seconds{status,has_filters}`. Admins can calculate p50/p95/p99 with Prometheus `histogram_quantile()` and compare p95 against SEARCH.md's 200ms soft and 500ms hard migration triggers.
- Notification delivery metrics: `notification_delivery_total{channel,status}` for in-app, SSE, webhook, and push channels. Webhook dispatch records both scheduled `pending` and background terminal `delivered`/`failed` statuses.
- Labels intentionally exclude user IDs, query strings, media IDs, notification IDs, webhook URLs, and other unbounded or sensitive values.

**Deferred:**

- Runtime config hot-reload — `LoggingConfig` fields (level, max_files, format) loaded from DB at step 9 but not yet wired to subscriber reload; requires `tracing-subscriber::reload` layer
- `max_file_size_mb` — stored in config but `tracing-appender` only supports time-based rotation (DAILY); retained for future custom rotation or documentation accuracy
- PII sanitization layer — custom `tracing-subscriber` layer for email masking, IP truncation, session ID truncation; deferred to Phase 13
- OpenTelemetry layer — `otel` Cargo feature flag; deferred until admin demand warrants it
- Additional metric categories (playback, library, database, system, transcode, analytics, trakt) — added incrementally in each domain phase
- `allowed_metrics_subnets` admin API for runtime updates — subnet list is fixed at startup; reload on `reload_runtime_config()` deferred to admin API implementation
- Response body size histogram — `http_response_body_size` metric deferred to Phase 7 (streaming) when body sizes become relevant
