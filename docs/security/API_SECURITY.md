# API Security Domain

## Overview

This document is the authoritative design for API-layer security. It covers: input validation, object-level authorization (BOLA prevention), SSRF prevention, response DTO separation, request payload limits, admin endpoint isolation, outbound API response validation, dependency auditing, secret scanning, and error response sanitization.

The design is mapped against two industry-standard frameworks:
- **OWASP Top 10:2025** — web application security risks
- **OWASP API Security Top 10 (2023)** — API-specific security risks

Transport security (TLS, signed URLs, security headers) is documented in [SECURITY.md](SECURITY.md). Authentication and authorization (passkeys, capabilities, sessions) is documented in [AUTH.md](../design/AUTH.md). Rate limiting and CORS conventions are documented in [API_CONVENTIONS.md](../design/API_CONVENTIONS.md). This document covers the application-layer security patterns that prevent the most common API vulnerabilities.

---

## Why This Document Exists

Research from multiple authoritative sources confirms that AI-assisted development ("vibe coding") systematically produces insecure code:

| Finding | Source |
|---|---|
| 62% of AI-built applications ship with critical security vulnerabilities | OX Security (May 2026) |
| Only 10.5% of AI-generated code passes security review | Carnegie Mellon University (2025) |
| 86% of AI-generated code failed XSS defense mechanisms | Georgetown CSET |
| 0 of 15 tested AI apps implemented CSRF protection; 0 set any security headers | Tenzai (December 2025) |
| 74 CVEs traced to AI coding tools (14 critical, 25 high); 35 in March 2026 alone | Georgia Tech SSLab Vibe Security Radar (April 2026) |
| Top 3 vulnerability classes: command injection, authentication bypass, SSRF | Georgia Tech SSLab |
| AI models selected insecure implementation options 45% of the time when both secure and insecure options existed | Veracode |
| Moltbook breach: 1.5M API authentication tokens exposed within 72 hours of launch due to missing Row Level Security policies | OX Security (January 2026) |

These findings directly inform the security controls in this document. Rust's type system eliminates entire vulnerability classes (buffer overflows, use-after-free, SQL injection via sqlx compile-time checking), but API-layer security — authorization checks, input validation, SSRF prevention, response filtering — must be explicitly designed and enforced.

---

## OWASP API Security Top 10 (2023) — Coverage Matrix

| # | Risk | Status | How We Address It | Document |
|---|---|---|---|---|
| API1 | Broken Object Level Authorization (BOLA) | **Covered** | Ownership validation in service layer | This document |
| API2 | Broken Authentication | **Covered** | Passkey-first + session tokens + mandatory auth in exposed mode | [AUTH.md](../design/AUTH.md) |
| API3 | Broken Object Property Level Authorization | **Covered** | Response DTO separation; serde ignores unknown fields by default | This document |
| API4 | Unrestricted Resource Consumption | **Covered** | 5-tier rate limiting + request body size limits | This document + [API_CONVENTIONS.md](../design/API_CONVENTIONS.md) |
| API5 | Broken Function Level Authorization | **Covered** | Capability-based access control + admin endpoint isolation | This document + [AUTH.md](../design/AUTH.md) |
| API6 | Unrestricted Access to Sensitive Business Flows | **Covered** | Invite code rate limits + device linking limits | This document |
| API7 | Server-Side Request Forgery | **Covered** | URL allowlisting + DNS pinning + redirect disabling | This document |
| API8 | Security Misconfiguration | **Covered** | Default-deny + no debug endpoints in production | [SECURITY.md](SECURITY.md) |
| API9 | Improper Inventory Management | **Covered** | URI versioning `/api/v1/` + single routing tree | [API_CONVENTIONS.md](../design/API_CONVENTIONS.md) |
| API10 | Unsafe Consumption of APIs | **Covered** | Outbound response validation against expected schemas | This document |

---

## 1. Input Validation

### Crate: `validator`

```toml
validator = { version = "0.20", features = ["derive"] }
```

All request DTOs use `#[derive(Validate)]` with declarative constraints. Validation runs at the deserialization boundary, before handler code executes.

### Pattern

```rust
use serde::Deserialize;
use validator::Validate;

#[derive(Deserialize, Validate)]
pub struct CreateLibraryRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: String,

    #[validate(length(min = 1, max = 500))]
    pub root_path: String,

    #[validate(regex(path = "crate::types::SLUG_REGEX"))]
    pub slug: String,

    pub media_type: String,
}
```

### Enforcement Rule

Every handler that accepts a request body MUST use `Json<T>` where `T: Validate`. The `AuthenticatedUser` extractor or a validation middleware calls `request.validate()?` before the handler runs. Handlers never receive unvalidated input.

### What This Prevents

| Attack | Prevention |
|---|---|
| SQL injection via user input | `sqlx` parameterized queries prevent this at the DB layer; `validator` prevents it at the API layer |
| XSS via stored input | Length limits, regex constraints, and type checking prevent arbitrary script injection |
| Path traversal in `root_path` | Regex validation restricts to safe path characters; service layer validates canonical path |
| Enum injection in `media_type` | Rust enum deserialization rejects invalid values at compile time |

### Why Not Custom Validation

| Approach | Pros | Cons | Verdict |
|---|---|---|---|
| **`validator` crate** | Declarative, derive-based, composable, well-maintained | One more dependency | **Selected** |
| Hand-written validation in handlers | No extra dependency | Scattered, inconsistent, easy to miss fields | Rejected |
| Custom middleware | Centralized | Cannot express per-field rules; inflexible | Rejected |
| JSON Schema validation | Framework-agnostic | Runtime overhead; requires schema files | Rejected |

---

## 2. Broken Object Level Authorization (BOLA) Prevention

### The Problem

BOLA (OWASP API1, ranked #1 for two consecutive editions) occurs when an API uses object IDs from client requests without verifying the authenticated user has access to that specific object. UUIDv7 makes enumeration harder, but does not prevent authorized-user-A from accessing authorized-user-B's data.

### Prevention Pattern

Every service-layer function that reads or mutates data by ID MUST validate that the requesting user has access to the target object.

#### Ownership Validation Trait

```rust
pub trait Authorize {
    fn require_owner(&self, user_id: &Uuid) -> Result<(), AppError>;
}
```

#### Service-Layer Enforcement

```rust
pub async fn get_playlist(
    db: &PgPool,
    user: &AuthenticatedUser,
    playlist_id: Uuid,
) -> Result<PlaylistResponse, AppError> {
    let playlist = sqlx::query_as!(
        PlaylistRow,
        "SELECT * FROM playlists WHERE id = $1 AND deleted_at IS NULL",
        playlist_id
    )
    .fetch_optional(db)
    .await?
    .ok_or(PlaylistError::NotFound)?;

    if playlist.user_id != user.id {
        return Err(AppError::Forbidden(AUTH_004));
    }

    Ok(playlist.into_response())
}
```

### Admin Bypass

Users with `can_manage_server` or `can_manage_users` capabilities can bypass ownership checks for admin operations. This is explicit — every admin-accessible endpoint has a separate capability check, not a blanket bypass.

### Tables Requiring Ownership Checks

| Table | Ownership Column | Scope |
|---|---|---|
| `user_item_data` | `user_id` | Per-user watch state |
| `bookmarks` | `user_id` | Per-user bookmarks |
| `playlists` | `user_id` | Per-user playlists |
| `user_sessions` | `user_id` | Per-user sessions |
| `api_keys` | `user_id` | Per-user API keys |
| `trakt_accounts` | `user_id` | Per-user Trakt link |
| `notifications` | `user_id` | Per-user notifications |
| `play_sessions` | `user_id` | Per-user analytics (admin can view all) |
| `libraries` | Capability check | `can_manage_libraries` |
| `media_items` | Library access check | Via `user_library_access` |
| `invitations` | `created_by_user_id` | Per-admin invitations |

### Why Not Middleware-Based Authorization

Middleware can verify authentication (who are you?) and capabilities (what can you do?), but cannot verify object-level authorization (do you own this specific row?) because the object ID is in the request path/body and the owner check requires a database query. Object-level authorization MUST happen in the service layer.

---

## 3. Response DTO Separation

### The Problem

OWASP API3 (Broken Object Property Level Authorization) covers two patterns:
- **Excessive Data Exposure** — returning full database rows when only a subset of fields is needed
- **Mass Assignment** — accepting client-supplied fields that should be read-only (e.g., `is_admin`)

### Prevention: Three-Type Pattern

Every domain uses three distinct types for each entity:

| Type | Purpose | Derived Traits |
|---|---|---|
| **`XxxRow`** | Database model (maps to SQL columns) | `FromRow`, no `Serialize` |
| **`XxxRequest`** | Inbound request body (accepts only user-writable fields) | `Deserialize`, `Validate` |
| **`XxxResponse`** | Outbound response body (exposes only safe fields) | `Serialize` |

```rust
// DB model — internal only, never serialized to clients
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub password_hash: Option<String>,       // never sent to clients
    pub role: String,                         // never sent to clients directly
    pub failed_login_attempts: i32,           // never sent to clients
    pub locked_until: Option<DateTime<Utc>>,  // never sent to clients
    pub metadata: JsonValue,
}

// Request DTO — only fields the client may set
#[derive(Deserialize, Validate)]
pub struct UpdateUserRequest {
    #[validate(length(min = 1, max = 100))]
    pub display_name: Option<String>,
    pub email: Option<String>,
}

// Response DTO — only fields safe to expose
#[derive(Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub role: String,
}
```

### Why Rust/Serde Makes Mass Assignment Harder

Rust's type system provides built-in mass assignment defense:
- `#[derive(Deserialize)]` only maps fields that exist in the struct — unknown fields are ignored by default
- Extra fields in the request body (e.g., `"role": "admin"`) are silently discarded, not mapped
- This is structurally different from dynamic languages where arbitrary keys can be bound to model attributes

### Enforcement Rule

- `FromRow` structs (`XxxRow`) MUST NOT implement `Serialize`
- `XxxResponse` structs MUST NOT contain sensitive fields (password hashes, tokens, internal metadata)
- Conversion from `XxxRow` to `XxxResponse` MUST be explicit (via `From`/`Into` or a dedicated method)

---

## 4. Request Payload Limits

### The Problem

OWASP API4 (Unrestricted Resource Consumption) covers denial-of-service via large payloads, unlimited requests, or expensive operations.

### Configuration

| Limit | Default | Configuration |
|---|---|---|
| **Request body size** | 1 MB | `tower-http::limit::RequestBodyLimitLayer` |
| **File upload size** | 50 MB | Separate limit for upload endpoints |
| **Request timeout** | 30 seconds | `tower::timeout::TimeoutLayer` |
| **Max page size** | 100 items | Enforced in `PaginationParams` validator |
| **Max items per bulk operation** | 50 items | Per-endpoint validation |

### Implementation

```rust
use tower_http::limit::RequestBodyLimitLayer;
use tower::ServiceBuilder;
use std::time::Duration;

let api_layers = ServiceBuilder::new()
    .layer(RequestBodyLimitLayer::new(1024 * 1024))        // 1 MB default
    .timeout(Duration::from_secs(30));

let upload_layers = ServiceBuilder::new()
    .layer(RequestBodyLimitLayer::new(50 * 1024 * 1024))   // 50 MB for uploads
    .timeout(Duration::from_secs(300));
```

### Existing Rate Limiting

Rate limiting is already designed with 5 tiers via `governor` v0.6. Full design in [API_CONVENTIONS.md](../design/API_CONVENTIONS.md).

---

## 5. Admin Endpoint Isolation

### The Problem

OWASP API5 (Broken Function Level Authorization) occurs when admin endpoints are accessible to regular users because authorization checks are inconsistent or rely on UI-based hiding.

### Design

Admin endpoints are grouped under `/api/v1/admin/*` with a dedicated router that applies stricter middleware:

```rust
pub fn admin_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/admin/users", get(list_users).post(create_user))
        .route("/api/v1/admin/users/{id}", get(get_user).delete(delete_user))
        .route("/api/v1/admin/libraries", post(create_library))
        .route("/api/v1/admin/config", get(get_config).put(update_config))
        .route("/api/v1/admin/tasks", get(list_tasks).post(trigger_task))
        .route("/api/v1/admin/backups", get(list_backups).post(create_backup))
        .layer(require_capability("can_manage_server"))
        .with_state(state)
}
```

> **Implementation note:** The actual implementation uses `Require<CanManageServer>` as an `FromRequestParts` extractor (see `server/src/extractors.rs`) rather than a `.layer()` middleware. This avoids the double-extraction problem and is more ergonomic. `AdminOnly` is a type alias for `Require<CanManageServer>`. See [AUTH.md](../design/AUTH.md) Task 11 for details.

### Admin Capability Requirements

| Endpoint Group | Required Capability | Additional Checks |
|---|---|---|
| User management | `can_manage_users` | Owner account cannot be demoted |
| Library management | `can_manage_libraries` | Library access validation |
| Server configuration | `can_manage_server` | Owner-only for critical changes |
| Scheduled tasks | `can_manage_scheduled_tasks` | Task type validation |
| Analytics dashboard | `can_view_analytics` | Scope to permitted libraries |
| Backup management | `can_manage_server` | Backup path validation |

### Default-Deny Router

The router is default-deny — no route is accessible without explicit authentication and authorization. The `AuthenticatedUser` extractor rejects unauthenticated requests with `401 Unauthorized`. Capability checks return `403 Forbidden` with `AUTH_004` error code.

---

## 6. Server-Side Request Forgery (SSRF) Prevention

### The Problem

OWASP API7 (SSRF) occurs when an API fetches a remote resource using a user-supplied URL without validation. In our platform, the primary SSRF vectors are:
- **Metadata fetching** — TMDb, TVDb API calls
- **Artwork downloading** — poster/backdrop URLs from metadata providers
- **Subtitle fetching** — OpenSubtitles, SubDL API calls
- **Trakt.tv API** — OAuth callbacks, sync operations
- **ACME challenges** — Let's Encrypt HTTP-01 challenge fetches (only when exposed)

### Prevention: URL Allowlisting

```rust
pub struct OutboundUrlValidator {
    allowed_hosts: Vec<String>,
    blocked_cidrs: Vec<IpAddr>,
}

impl OutboundUrlValidator {
    pub fn validate(&self, url: &str) -> Result<Url, SsrfError> {
        let parsed = Url::parse(url).map_err(|_| SsrfError::InvalidUrl)?;

        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(SsrfError::BlockedScheme);
        }

        let host = parsed.host_str().ok_or(SsrfError::MissingHost)?;

        if !self.allowed_hosts.iter().any(|allowed| {
            host.eq_case_insensitive(allowed) ||
            host.ends_with(&format!(".{}", allowed))
        }) {
            return Err(SsrfError::HostNotAllowed);
        }

        Ok(parsed)
    }
}
```

### Allowed Hosts by Function

| Function | Allowed Hosts | Configurable |
|---|---|---|
| TMDb metadata + images + exports | `api.themoviedb.org`, `image.tmdb.org`, `files.tmdb.org` | Yes — `server_config.metadata.providers` |
| TVDb metadata | `api4.thetvdb.com` | Yes — `server_config.metadata.providers` |
| Fanart.tv artwork | `webservice.fanart.tv` | Yes — `server_config.metadata.providers` |
| OMDb ratings | `www.omdbapi.com` | Yes — `server_config.metadata.providers` |
| Trakt.tv | `api.trakt.tv`, `trakt.tv` | Yes — Trakt account link |
| SubDL subtitles | `api.subdl.com`, `dl.subdl.com` | Yes — `server_config.integrations` |
| OpenSubtitles subtitles | `api.opensubtitles.com` | Yes — `server_config.integrations` |
| Migration sources | Admin-entered Jellyfin/Emby base URLs | Yes — Phase 14 migration setup, guarded by network-mode policy |
| Artwork sources | Provider domains above | Yes |
| ACME (exposed mode only) | Let's Encrypt ACME directory | Yes — `server_config.security.tls` |

### Hardening Rules

1. **Only `http://` and `https://` schemes allowed** — block `file://`, `gopher://`, `dict://`, `ftp://`
2. **DNS resolution is pinned** — resolve DNS once, validate the IP against blocked ranges, then connect to the pinned IP (prevents DNS rebinding)
3. **HTTP redirects are disabled** — `reqwest` client configured with `redirect(Policy::none())`. If a redirect is required, the target URL goes through the same validation pipeline
4. **Private IP ranges are blocked** — `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `169.254.0.0/16`, `127.0.0.0/8`, `::1/128`, `fc00::/7`
5. **Cloud metadata endpoints are blocked** — `169.254.169.254` (AWS/Azure/GCP metadata)

### Why Allowlisting Over Denylisting

| Approach | Pros | Cons | Verdict |
|---|---|---|---|
| **Allowlisting** (selected) | Only known-safe destinations; impossible to bypass with encoding tricks | Must update when adding providers | **Selected** |
| Denylisting | No maintenance when adding providers | Fragile — bypassed via IP encoding (decimal, octal, hex, IPv6-mapped) | Rejected |
| No outbound restrictions | Simplest | Fully vulnerable to SSRF | Rejected |

### reqwest Client Configuration

```rust
let outbound_client = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .timeout(Duration::from_secs(30))
    .connect_timeout(Duration::from_secs(10))
    .no_proxy()
    .build()?;
```

**Consumers of this hardened config:** all metadata/artwork/subtitle/Trakt outbound clients (Phase 6+), and the notification webhook dispatch client (`services::notification_dispatch::build_webhook_client`, Phase 13b Task 4). The webhook URL is operator-configured (trusted), but defense-in-depth applies — `no_proxy()` prevents a malicious `HTTP_PROXY` env var from redirecting notification traffic, and `redirect(Policy::none())` blocks SSRF via redirect chains.

**Migration source exception:** Jellyfin and Emby migration sources are admin-entered and often legitimately live on private LAN addresses. Phase 14 Task 3 applies network-mode policy instead of a fixed public allowlist: local mode permits LAN/loopback targets after URL/DNS validation, while exposed mode rejects private, loopback, link-local, unique-local, reserved, and cloud metadata addresses. Stored migration configs also record redirect blocking, 10-second timeout, and 1 MiB response-size policy for the REST clients. Phase 14 Task 6 keeps raw Jellyfin/Emby API keys session-only for `/connect` and `/discover`: the supplied key must hash to the stored `api_key_hash` before any source API request is sent, and the raw key is not persisted.

**Plex upload exception:** Phase 14 Task 7 allows a large multipart body only on `POST /api/v1/migrations/{id}/upload`; the route streams the upload to a per-migration directory, enforces the 10 GiB Plex DB cap while writing, validates the SQLite header and required tables before accepting the file, and canonicalizes the stored path before read-only extraction.

---

## 7. Outbound API Response Validation

### The Problem

OWASP API10 (Unsafe Consumption of APIs) — developers trust data from third-party APIs more than user input. In our platform, responses from TMDb, TVDb, Trakt.tv, OpenSubtitles, and SubDL are consumed and stored. A compromised or malicious provider response could inject malicious content into our database.

### Prevention: Schema Validation

All outbound API responses are validated against expected schemas before processing:

```rust
#[derive(Deserialize, Validate)]
pub struct TmdbMovieResponse {
    #[validate(length(max = 500))]
    pub title: String,
    pub id: i64,
    #[validate(range(min = 1800, max = 2030))]
    pub release_date: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub vote_average: Option<f32>,
    pub genres: Vec<TmdbGenre>,
}
```

### Rules

1. **Every outbound response type implements `Deserialize` + `Validate`** — malformed responses are rejected, not stored
2. **String fields have maximum length constraints** — prevents storing arbitrarily large payloads
3. **URL fields from providers go through the SSRF validator** — artwork URLs are validated before download
4. **Error responses are handled gracefully** — a failed metadata fetch does not crash the server or corrupt data
5. **Trakt.tv responses are rate-limit-aware** — 429 responses trigger backoff, not retry storms

---

## 8. Business Flow Abuse Prevention

### The Problem

OWASP API6 (Unrestricted Access to Sensitive Business Flows) — automated abuse of legitimate business operations.

### Protected Flows

| Flow | Rate Limit | Additional Protection |
|---|---|---|
| **Invite code verification** | 5 per IP per 15 min | Account lockout after 5 failures for 30 min |
| **Device linking code creation** | 10 per IP per hour | 15-minute code expiry |
| **Device linking code approval** | 10 per user per hour | Must be authenticated |
| **Re-auth code requests** | 3 per user per 24 hours | Email delivery required |
| **Passkey registration** | 5 per user per hour | Requires active session |
| **Password change** | 3 per user per hour | Requires current password |
| **Library scan trigger** | 1 per user per 5 min | Requires `can_manage_libraries` |
| **Backup creation** | 1 per user per hour | Requires `can_manage_server` |

### Implementation

These are enforced at the application layer using a token-bucket rate limiter stored in memory (not in `governor`, which handles HTTP-level rate limiting). Uses a simple `DashMap<String, TokenBucket>` keyed by user ID or IP.

---

## 9. Dependency Auditing

### The Problem

OWASP Top 10:2025 A03 (Software Supply Chain Failures) — a new entry in 2025, reflecting the growing risk of compromised dependencies. AI coding tools frequently recommend outdated or vulnerable packages.

### Prevention

| Tool | Purpose | When |
|---|---|---|
| **`cargo audit`** | Checks `Cargo.lock` against the RustSec Advisory Database for known CVEs | CI pipeline (every build) |
| **`cargo deny`** | License compliance, duplicate dependency detection, banned crates | CI pipeline (every build) |
| **`cargo vet`** | Human-reviewed audit trail for each crate in the dependency tree | CI pipeline (every build) + ongoing review |
| **`cargo cyclonedx`** | Generates a CycloneDX Software Bill of Materials (SBOM) for every release | CI pipeline (releases only) |
| **Dependabot / Renovate** | Automated dependency update PRs | Continuous |

### CI Integration

```yaml
- name: Security audit
  run: cargo audit

- name: Dependency check
  run: cargo deny check

- name: Supply chain vet
  run: cargo vet

- name: Generate SBOM
  if: github.ref == 'refs/heads/main'
  run: cargo cyclonedx --format json --output sbom.json
```

### What Each Tool Does

| Tool | What It Catches | Why It's Needed |
|---|---|---|
| `cargo audit` | Known CVEs in dependencies | Alerts when a crate we depend on has a published vulnerability |
| `cargo deny` | License violations, duplicate crate versions, banned crates, advisory violations | Catches legal issues and structural problems that `audit` misses |
| `cargo vet` | Unreviewed or untrusted crates in the dependency tree | Requires human review before new crates are accepted; Google-sponsored; creates a trusted audit trail |
| `cargo cyclonedx` | Generates a machine-readable list of every dependency and its version | An SBOM lets users and security researchers verify exactly what's in a release binary |

### Why `cargo vet`

`cargo audit` and `cargo deny` check for **known problems** — vulnerabilities that have already been discovered and reported. `cargo vet` goes further: it requires that a human has actually read and approved the code for each crate before it's trusted. This catches:

- **Typosquatting** — crates with names similar to popular crates, published by attackers
- **Dependency confusion** — malicious crates that exploit package resolution order
- **Supply chain takeover** — a legitimate crate's maintainer account is compromised and a malicious version is published
- **Subtle backdoors** — code that passes automated checks but would not pass human review

The vet process is simple: when a new dependency is added, a team member reviews it and records their approval in `supply-chain/audits.toml`. CI enforces that all dependencies are vetted before merging.

### Why an SBOM

A Software Bill of Materials (SBOM) is a complete, machine-readable list of every piece of software bundled into a release. It answers the question: "what exactly is in this binary?" Without an SBOM, users have no way to verify that a release contains only the expected dependencies.

The SBOM is published alongside each release as `sbom.json` in CycloneDX format (industry standard, NIST-endorsed).

### Reproducible Builds (Deferred)

Reproducible builds — where anyone can verify that the published binary was compiled from the exact source code — are a valuable security measure but are deferred because:

- Rust cross-compilation with Alpine musl targets requires strict environment control
- Build path normalization (`--remap-path-prefix`) adds complexity to the CI pipeline
- Docker image reproducibility requires layer-by-layer pinning
- The SBOM, `cargo vet`, and signed releases provide strong supply chain assurance without reproducible builds

This may be revisited in a future phase when the CI pipeline is more mature.

### Known Dismissed Vulnerabilities

Vulnerabilities that are flagged but cannot or should not be fixed, with documented rationale. Each entry corresponds to a dismissed Dependabot/RustSec alert.

#### GHSA-wrw7-89jp-8q8g — `glib` unsoundness in `VariantStrIter` (medium)

| Field | Value |
|---|---|
| Advisory | [GHSA-wrw7-89jp-8q8g](https://github.com/advisories/GHSA-wrw7-89jp-8q8g) |
| Package | `glib` (Rust GLib bindings) |
| Affected range | `>= 0.15.0, < 0.20.0` (Duskcue pins `0.18.5`) |
| Fixed in | `0.20.0` |
| Severity | medium |
| Dependabot alert | #1 |
| Dismissal reason | `not_used` |
| Date dismissed | 2026-06-23 |

**Source:** Transitive dependency via `tauri 2.11.2` (Phase 16 desktop client stub) → `gtk 0.18` → `glib 0.18.5`. The chain is Linux-only (`gtk`/`webkit2gtk` are `cfg(target_os = "linux")` windowing crates); it does not appear in the server or web-client build graphs.

**Why it cannot be fixed:**

1. **gtk3-rs (gtk 0.18) is EOL/unmaintained** — the gtk-rs project has moved to GTK4 bindings only; no gtk 0.19+ will ever release, so glib 0.20+ (which requires the gtk-rs 0.20 series) is unreachable through this chain.
2. **`[patch.crates-io]` is not viable** — gtk 0.18 declares `glib = "^0.18"`; forcing glib 0.20 via a patch breaks the Linux build because the 0.18→0.20 API is incompatible (trait re-exports, `glib::Object` changes).
3. **No Tauri upgrade resolves it** — every Tauri 2.x release uses gtk 0.18 on Linux.
4. **Upstream Tauri explicitly wontfixed it** ([tauri-apps/tauri#12048](https://github.com/tauri-apps/tauri/issues/12048)) — maintainer FabianLars: "Since the gtk3 bindings are unmaintained I think this is a wontfix sadly. (We don't use glib directly ourselves)" and "this unsound issue doesn't seem to affect us".

**Why it does not affect Duskcue:**

- The vulnerability is in `glib::VariantStrIter::impl_get`, which passes an immutable `&*mut c_char` to a C function that mutates in place — a soundness bug that can cause NULL-pointer dereferences **only when iterating a `GVariant` string array**. Neither Tauri nor Duskcue calls this iterator; Duskcue's desktop crate is an empty stub (`fn main() {}`) with zero Tauri API usage.
- The affected code is Linux-only and does not ship in the server or web-client artifacts.
- The desktop client is Phase 16 (future) — when it is built, the Tauri→GTK4 migration (tracked upstream in tauri-apps/tauri#7335) is the path that will retire gtk3-rs and this transitive advisory.

**Re-evaluation trigger:** Revisit if (a) a fix lands in gtk3-rs or a Tauri release adopts glib 0.20+, (b) the desktop client moves out of stub status (Phase 16), or (c) a Duskcue code path is added that iterates `GVariant` strings.

---

## 10. Secret Scanning

### The Problem

AI models insert hardcoded secrets (API keys, tokens, passwords) from training data. GitHub reports millions of leaked credentials annually.

### Prevention

1. **No secrets in source code** — all secrets come from environment variables or the `server_config` table
2. **`server_config` values are never returned to clients** — the admin config endpoint returns masked values (e.g., `"api_key": "mv_****"`)
3. **Pre-commit hook** — optional `gitleaks` pre-commit hook for developers
4. **`.gitignore` covers secrets** — `.env`, `*.pem`, `*.key`, `config.toml` are gitignored
5. **Bootstrap config uses env vars** — `DUSKCUE_DATABASE_URL` and other sensitive values are environment-only, never in files

### Application-Level Secret Handling

```rust
pub fn mask_secret(value: &str) -> String {
    if value.len() <= 8 {
        return "****".to_string();
    }
    format!("{}****", &value[..4])
}
```

The first concrete admin settings endpoints implementing this masking pattern were Phase 9 Task 8's subtitle settings (`GET /api/v1/settings/subtitles` returns `api_key_masked` + `has_api_key`, never raw keys; `PUT /api/v1/settings/subtitles/providers` encrypts SubDL/OpenSubtitles keys at rest via `EncryptionKey` AES-256-GCM and only overwrites when a new value is provided). Phase 13a Task 2 extends the same rule to the generic `GET/PUT /api/v1/server/config` and `GET/PUT /api/v1/server/config/{group}` endpoints: sensitive JSON keys are masked in responses, preserved when masked placeholders are round-tripped, and encrypted before storage when changed. See [CONFIGURATION.md](../operations/CONFIGURATION.md).

---

## 11. Error Response Sanitization

### The Problem

Leaking internal error details (stack traces, SQL errors, file paths) gives attackers information about the system's internals. AI-generated code frequently returns raw errors to clients.

### Enforcement Rule

The `AppError::IntoResponse` implementation (already designed in [ERROR_HANDLING.md](../design/ERROR_HANDLING.md)) MUST:

1. **Never return raw `sqlx::Error` messages** — map to generic `SYS_001` with `"Internal server error"`
2. **Never return file paths** — map to `LIB_003` or `MEDIA_002` without the path
3. **Never return stack traces** — all errors are structured RFC 9457 Problem Details
4. **Log internally, not externally** — the full error is logged at `error!` level; the client receives the sanitized version

### RFC 9457 Error Response Format

```json
{
    "type": "https://duskcue.example.com/errors/PLAY_005",
    "title": "Stream not authorized",
    "status": 403,
    "detail": "The streaming URL signature is invalid or expired.",
    "request_id": "01H5..."
}
```

No `stack`, no `file`, no `line`, no `sql`, no `query`.

---

## New Workspace Dependencies

```toml
validator = { version = "0.20", features = ["derive"] }
```

| Crate | Version | Purpose |
|---|---|---|
| `validator` | 0.20 | Declarative input validation with `#[derive(Validate)]` |

All other security measures use existing workspace dependencies (`axum`, `tower-http`, `governor`, `ring`, `rustls`, `sqlx`, `serde`).

---

## No New Tables

All security measures are application-layer patterns enforced in code. No database schema changes are required.

---

## No New Error Codes

SSRF violations, validation failures, and authorization denials map to existing error codes:

| Failure | Mapped Code | Domain |
|---|---|---|
| Input validation failure | `VALID_001` (Validation error) | Validation |
| Object-level authorization failure | `AUTH_004` (Forbidden) | Auth |
| Admin capability check failure | `AUTH_004` (Forbidden) | Auth |
| Request body too large | `SYS_001` (Configuration error) | System |
| SSRF blocked URL | `SYS_001` (Configuration error) | System |
| Outbound API response invalid | Domain-specific (e.g., `LIB_003` for metadata) | Per domain |

---

## Research Sources

### OWASP Standards
- OWASP Top 10:2025: https://owasp.org/Top10/2025/en/
- OWASP API Security Top 10 (2023): https://owasp.org/API-Security/editions/2023/en/0x11-t10/
- OWASP API1:2023 — Broken Object Level Authorization: https://owasp.org/API-Security/editions/2023/en/0xa1-broken-object-level-authorization/

### Vibe Coding Security Research
- OX Security — Vibe Coding Security: Why 62% of AI-Generated Code Is Vulnerable (May 2026): https://www.ox.security/blog/vibe-coding-security/
- Georgia Tech SSLab — Bad Vibes: AI-Generated Code is Vulnerable, Researchers Warn (April 2026): https://research.gatech.edu/bad-vibes-ai-generated-code-vulnerable-researchers-warn
- Martin Fowler — The VibeSec Reckoning (May 2026): https://martinfowler.com/articles/vibesec-reckoning.html
- Axway — OWASP API Security: Top 10 Security Risks & Remedies for 2026 (2026): https://blog.axway.com/learning-center/digital-security/risk-management/owasps-api-security

### API Security Analysis
- APIsec — 2023 OWASP API Top Ten Analysis (2024): https://www.apisec.ai/blog/2023-owasp-api-top-ten
- Salt Security — State of API Security Report (2024): https://salt.security/api-security-trends

### Rust/Axum Security
- bulletproof-rust-web — Production-grade Rust/Axum guide with security hardening: https://github.com/gruberb/bulletproof-rust-web
- OneUptime — How to Secure Rust APIs Against Common Vulnerabilities (January 2026): https://oneuptime.com/blog/post/2026-01-07-rust-api-security/view
- Rustify — Rust for Backend Development: Complete Axum Guide 2026 (February 2026): https://rustify.rs/articles/rust-backend-development-axum-2026

### SSRF Prevention
- SSRF Prevention Guide 2026 (enhanced May 2026, includes AI/MCP risks): https://chs.us/guides/ssrf/

### Supply Chain Security
- Mozilla — `cargo vet` documentation and rationale: https://mozilla.github.io/cargo-vet/
- CycloneDX — SBOM standard for software supply chain: https://cyclonedx.org/
- Google Open Source Security — Supply chain security for Rust: https://opensource.googleblog.com/
- NIST — Software Bill of Materials (SBOM) guidance: https://www.nist.gov/itl/executive-order-safe-software/sbom
- RustSec Advisory Database: https://rustsec.org/

### NIST Framework
- NIST Cybersecurity Framework (CSF) 2.0: https://www.nist.gov/cyberframework
