# Analytics Security Domain

## Overview

This document is the authoritative design for security-focused analytics — detecting suspicious account activity using IP geolocation, impossible travel detection, and behavioral analysis. It covers: IP geolocation enrichment, the impossible travel detection algorithm, false positive suppression, automated responses, the GeoIP database update pipeline, and the trusted IP management system.

The database schema for play sessions, trust events, and trust scores is documented in [DATABASE.md](../design/DATABASE.md). This document describes the application-layer engine that populates geolocation data, detects anomalies, and triggers trust events. The HTTP API surface for the analytics dashboard (routes, DTOs, query parameters) is documented in [ANALYTICS.md](../design/ANALYTICS.md).

The design is **hands-off by default** — it runs automatically in the background, surfaces alerts in the admin dashboard when something looks wrong, and never blocks a user without the admin choosing to act. For a personal or family Duskcue, this means the system quietly watches for problems and tells you when it sees one.

---

## What "Impossible Travel" Means

If someone streams a movie from your server in Indiana at 8:00 PM, and the same account starts another stream from India at 8:30 PM, that's physically impossible — no one can fly from Indiana to India in 30 minutes. This usually means someone else has gotten hold of the account's credentials.

The system detects this automatically by comparing the geographic distance between two streaming sessions against the time between them. If the implied travel speed exceeds what a commercial airplane can achieve, it flags the event as suspicious.

This is the same approach used by Microsoft, Google, and other major platforms — it's a well-established security technique that catches credential theft, session hijacking, and account sharing early.

---

## IP Geolocation

### How It Works

When someone starts a streaming session, the server sees their IP address. An IP address doesn't directly tell you where someone is — it's just a number like `203.0.113.42`. The server uses a geolocation database to look up that IP address and find the approximate city, region, and country it belongs to.

This lookup happens entirely on the server — no data is sent to any external service during the lookup. The geolocation database file lives on the server's disk.

### Database: MaxMind GeoLite2 City

| Aspect | Detail |
|---|---|
| **Database** | MaxMind GeoLite2 City (free) |
| **File format** | MMDB (MaxMind binary database, ~70 MB) |
| **Accuracy** | 95-99% for country, 55-80% for city |
| **License** | CC BY-SA 4.0 (free for internal use, attribution required) |
| **Update frequency** | MaxMind publishes updates weekly; server downloads automatically |
| **Storage location** | `{data_dir}/geoip/GeoLite2-City.mmdb` |

### Why MaxMind GeoLite2

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **MaxMind GeoLite2 (offline MMDB)** | Free; highest accuracy among free options; fully offline (no API calls during lookups); no latency per lookup; works without internet after initial download; industry standard; Rust library available | Requires free MaxMind account to download; city accuracy varies by region (55-80%); license requires attribution | **Selected** |
| IP2Location Lite | Free; downloadable | Lower accuracy; no mature Rust reader | Rejected |
| Online API (per-request lookup) | Always current | Adds latency to every session; requires internet; rate limits; sends user IPs to third party | Rejected |

### Rust Library: maxminddb

```toml
maxminddb = { version = "0.28", features = ["mmap"] }
```

- **`maxminddb` 0.28** — reads MMDB files; thread-safe (`Send + Sync`); `mmap` feature memory-maps the file for fast lookups without loading the entire 70 MB into RAM
- Looked up once per new play session (not per segment), so performance impact is negligible
- Reader is held in application state as `Arc<Reader>` and hot-reloaded when the database file is updated

### Enrichment Flow

When a new `play_sessions` row is created:

1. Extract the client IP address from the HTTP request
2. Classify the location type:
   - **LAN** — IP is in a private range (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `fd00::/8`) or matches the server's own subnet
   - **WAN** — IP is a public address
   - **Relay** — IP belongs to a known relay/proxy service
3. If the IP is public, look it up in the GeoLite2 MMDB
4. Populate `geo_city`, `geo_region`, `geo_country`, `geo_lat`, `geo_lon` on the `play_sessions` row
5. If the IP is private (LAN), leave the geo columns null — local connections don't need geolocation

### Location Type Classification

```rust
pub fn classify_location(ip: &IpAddr, server_subnets: &[IpNetwork]) -> LocationType {
    if is_private_ip(ip) || server_subnets.iter().any(|s| s.contains(ip)) {
        LocationType::Lan
    } else if is_relay_ip(ip) {
        LocationType::Relay
    } else {
        LocationType::Wan
    }
}
```

LAN and VPN connections (Tailscale `100.x.x.x`, WireGuard `10.x.x.x`) are classified as LAN — they don't trigger impossible travel detection because both sides of the "travel" are on the same trusted network.

---

## Impossible Travel Detection

### The Algorithm

The detection runs after each new `play_sessions` row is created with geolocation data. It follows the standard approach used by Microsoft Defender, WorkOS Radar, and CrowdSec:

1. **Find the previous session** — look up the user's most recent play session (different IP, different country) from the last 24 hours
2. **Calculate distance** — use the Haversine formula to compute the great-circle distance between the two geographic coordinates
3. **Calculate implied velocity** — divide distance by time elapsed between the two sessions
4. **Apply suppression rules** — check if the event should be suppressed (VPN usage, same country, known device)
5. **Determine severity** — based on whether the destination is new for this user
6. **Create trust event** — if the event survives suppression, insert a `user_trust_events` row

### Haversine Formula

The great-circle distance between two points on Earth, given their latitude and longitude:

```
d = 2r · arcsin(√(sin²((φ₂-φ₁)/2) + cos(φ₁)·cos(φ₂)·sin²((λ₂-λ₁)/2)))
```

Where `r` = 6,371 km (Earth's mean radius), `φ` = latitude, `λ` = longitude.

This is the standard formula for computing distances between GPS coordinates. It accounts for the curvature of the Earth, which matters for long distances (the straight-line map distance can be significantly wrong for intercontinental travel).

### Velocity Threshold

| Parameter | Default | Why |
|---|---|---|
| `velocity_threshold_kmh` | 1,000 | A commercial airplane flies at ~900 km/h. Setting the threshold at 1,000 km/h provides a small buffer for geolocation inaccuracy without generating false positives. If the implied speed between two sessions exceeds this, something is wrong. |

### Configuration

All detection parameters are stored in `server_config.analytics` JSONB (see [DATABASE.md](../design/DATABASE.md)):

```json
{
    "geoip_enabled": true,
    "geoip_license_key": "",
    "geoip_update_schedule": "0 3 * * 1",
    "impossible_travel_enabled": true,
    "velocity_threshold_kmh": 1000,
    "min_distance_km": 500,
    "lookback_hours": 24,
    "same_country_suppress": true,
    "trusted_ips": [],
    "trusted_cidrs": []
}
```

| Field | Type | Default | Description |
|---|---|---|---|
| `geoip_enabled` | bool | `true` | Master toggle for geolocation enrichment |
| `geoip_license_key` | string | `""` | MaxMind license key for downloading GeoLite2 updates (stored in bootstrap config, not here) |
| `geoip_update_schedule` | string | `"0 3 * * 1"` | Cron expression for MMDB updates (default: weekly Monday 03:00) |
| `impossible_travel_enabled` | bool | `true` | Master toggle for impossible travel detection |
| `velocity_threshold_kmh` | int | `1000` | Speed threshold in km/h — above this is flagged as impossible |
| `min_distance_km` | int | `500` | Minimum distance to consider — shorter distances are skipped because city-level GeoIP accuracy is too unreliable |
| `lookback_hours` | int | `24` | How far back to look for the user's previous session |
| `same_country_suppress` | bool | `true` | If both sessions are in the same country, don't flag — same-country jumps are almost always VPN or carrier artifacts |
| `trusted_ips` | string[] | `[]` | Individual IP addresses to ignore (e.g., VPN exit nodes) |
| `trusted_cidrs` | string[] | `[]` | IP ranges to ignore in CIDR notation (e.g., `["203.0.113.0/24"]` for a corporate network) |

---

## False Positive Suppression

The biggest challenge with impossible travel detection is false positives — alerts that look like attacks but are actually normal behavior. Without suppression, VPN users would trigger alerts every time they connect.

The system uses five layers of suppression, evaluated in order:

### Layer 1: LAN/VPN Connection Suppression

If either session is from a LAN or VPN connection (location_type = `lan`), the event is suppressed entirely. This is the most important suppression — users connecting through Tailscale, WireGuard, or a home network should never trigger impossible travel alerts.

### Layer 2: Trusted IP List

The admin can mark specific IP addresses or IP ranges as "trusted" in the analytics settings. Common use cases:

- VPN exit node IPs (e.g., your Tailscale exit node at work)
- Corporate office IP ranges
- A cloud VPS used as a relay

If one side of the travel is a trusted IP, the event's severity is reduced to "low" (logged but not notified).

### Layer 3: Same-Country Suppression

When `same_country_suppress` is enabled (default), any travel within the same country is skipped entirely. This is because:

- IP geolocation at the city level is only 55-80% accurate
- Mobile carriers often route traffic through gateways in different cities within the same country
- Corporate networks may route through a central office in another state
- Same-country "travel" is almost never an actual security concern

### Layer 4: User Location Baseline

The system tracks which countries each user has streamed from in the last 90 days (stored in `user_location_history`). If the destination country is already in the user's normal set:

- Severity is reduced to "low" (dashboard only, no notification)
- The event is still recorded for audit purposes

This prevents repeated alerts for a family member who genuinely travels internationally.

### Layer 5: Same-Device Detection

If both sessions came from the same `client_device` identifier, the event is suppressed. When the same device "travels" between countries in minutes, it's almost certainly a VPN switch — the user turned on or off a VPN, not physically moved.

### Suppression Decision Flow

```
New session with geo data
    │
    ├─ Is either session LAN/VPN? ──── Yes ──→ Suppress entirely
    │
    ├─ Is either IP in trusted list? ── Yes ──→ Reduce to "low"
    │
    ├─ Same country? ──────────────── Yes ──→ Suppress entirely
    │
    ├─ Same device? ───────────────── Yes ──→ Suppress entirely
    │
    ├─ Distance < min_distance_km? ── Yes ──→ Suppress (too inaccurate)
    │
    ├─ Velocity > threshold? ──────── No ──→ Normal travel, skip
    │
    └─ Velocity > threshold ──────── Yes ──→ Check user baseline
         │
         ├─ Destination in 90-day history? ──→ Severity: low
         │
         └─ Destination is new ─────────────→ Severity: medium or high
              │
              ├─ New continent ──→ Severity: high
              │
              └─ Same continent ──→ Severity: medium
```

---

## Automated Response

The system is **notification-first** — it tells you something looks wrong, and you decide what to do. For a personal or family server, automatically blocking users is too disruptive (false positives would lock out legitimate family members).

### Severity Levels

| Severity | What Happens | Trust Score Impact |
|---|---|---|
| **Low** | Recorded in trust events; visible in admin dashboard; no notification sent | -2 |
| **Medium** | Recorded + admin notification sent (email or dashboard) | -5 |
| **High** | Recorded + admin notification + dashboard security alert | -10 |

### Trust Score Decay

Trust scores recover automatically when the user has normal sessions:

- **+1 per day** of normal activity (no trust events)
- **Minimum score: 0** (never goes below 0)
- **Starting score: 100** (every user starts with full trust)

The trust score is informational — it does not block or restrict any functionality. It's a signal for the admin to investigate, not an automated enforcement mechanism.

### Admin Actions (Manual)

When the admin sees a trust event in the dashboard, they can:

| Action | What It Does |
|---|---|
| **Acknowledge** | Marks the event as reviewed; no further notifications for this event |
| **Revoke all sessions** | Deletes all rows in `user_sessions`; every user must re-authenticate |
| **Lock user account** | Sets `locked_until` on the user; prevents login until admin unlocks |
| **Mark IP as trusted** | Adds the suspicious IP to the trusted list so it doesn't trigger future alerts |
| **Dismiss** | Records that the admin reviewed and decided no action is needed |

### Other Trust Rules (Already in Schema)

The `user_trust_events.rule_type` CHECK constraint includes six rule types. Only `impossible_travel` is fully designed in this document. The remaining rules are reserved for future implementation:

| Rule Type | Description | Status |
|---|---|---|
| `impossible_travel` | Velocity between two sessions exceeds threshold | **Designed (this document)** |
| `simultaneous_locations` | Two active sessions from different countries at the same time | Future |
| `device_velocity` | Same device appears from two distant locations quickly | Future |
| `concurrent_streams` | More simultaneous streams than expected for one account | Future |
| `geo_restriction` | Stream attempted from a blocked geographic region | Future |
| `account_inactivity` | Account inactive for extended period, then suddenly active | Future |

---

## GeoIP Database Updates

### Update Pipeline

The GeoLite2 City MMDB file must be kept current because IP address assignments change over time — new blocks are allocated to different countries and ISPs regularly. A stale database means incorrect geolocation, which means missed detections or false positives.

### Scheduled Task: `geoip_database_update`

| Property | Value |
|---|---|
| **Task type** | `geoip_database_update` (added to `scheduled_tasks` CHECK constraint) |
| **Schedule** | Weekly, Monday 03:00 (configurable via `server_config.analytics.geoip_update_schedule`) |
| **What it does** | Downloads the latest GeoLite2-City.mmdb from MaxMind using the license key |
| **Storage** | `{data_dir}/geoip/GeoLite2-City.mmdb` |
| **Hot-reload** | Downloads to a temp file, then atomically renames over the existing file; the `maxminddb` Reader re-opens on next lookup |
| **Fallback** | If the download fails, the existing file continues to work; the task logs a warning and retries next week |
| **No MMDB present** | If the MMDB file is missing at startup, geolocation enrichment is skipped; impossible travel detection is disabled; a warning appears in the admin dashboard |

### First-Run Setup

During the first-run setup wizard, an optional step is presented:

> **"Enable IP Geolocation"**
>
> This feature detects suspicious account activity by looking up the approximate location of streaming sessions. It's recommended for servers that are accessible over the internet.
>
> To enable it, you'll need a free MaxMind account and license key. [Sign up here] (link to MaxMind signup page)
>
> License key: `[________________]`
>
> This is optional — you can skip this and add a license key later in Settings.

The license key itself is stored in the bootstrap configuration (`config.toml`), not in the database, because it's a secret needed before the database is available.

### MaxMind Attribution

The GeoLite2 EULA (CC BY-SA 4.0) requires attribution. The server's About page includes:

> "This product includes GeoLite2 data created by MaxMind, available from https://www.maxmind.com"

### EULA Compliance

| Requirement | How We Comply |
|---|---|
| Attribution | About page + startup log message |
| Internal use only | Server uses the database for its own session enrichment, not exposed as a service |
| Destroy old versions | Update task deletes the previous MMDB file after successful download and validation |
| No redistribution | MMDB file is never served to clients; only used server-side |

---

## User Location History

### Purpose

To power the "user baseline" suppression layer, the server tracks which countries each user has streamed from. This allows the system to distinguish between a family member who regularly travels to Canada (normal) and someone streaming from a country the account has never used before (suspicious).

### Table: `user_location_history`

```sql
CREATE TABLE user_location_history (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    country_code TEXT NOT NULL,

    first_seen_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    session_count INT NOT NULL DEFAULT 1,

    CONSTRAINT uq_user_location_country UNIQUE (user_id, country_code)
);

CREATE INDEX idx_user_location_history_user_id ON user_location_history (user_id);
CREATE INDEX idx_user_location_history_last_seen ON user_location_history (last_seen_at DESC);
```

| Column | Purpose |
|---|---|
| `country_code` | ISO 3166-1 alpha-2 country code (e.g., `US`, `GB`, `JP`) |
| `first_seen_at` | When this country was first seen for this user |
| `last_seen_at` | When this country was most recently seen |
| `session_count` | How many sessions from this country (helps distinguish regular travelers from one-offs) |

This table is updated as a side effect of play session enrichment — no separate scheduled task needed.

### Baseline Window

The suppression layer considers a country "in the user's baseline" if `last_seen_at` is within the last 90 days. Countries not seen in 90 days are still in the table but are treated as "not recent" for suppression purposes. Old rows are never deleted — the full history is valuable for the admin analytics dashboard.

---

## Database Schema Changes

### New JSONB Column: `server_config.analytics`

Added to the `server_config` table:

```sql
ALTER TABLE server_config ADD COLUMN analytics JSONB NOT NULL DEFAULT '{}';
```

Default value (seeded on first run):

```json
{
    "geoip_enabled": true,
    "geoip_update_schedule": "0 3 * * 1",
    "impossible_travel_enabled": true,
    "velocity_threshold_kmh": 1000,
    "min_distance_km": 500,
    "lookback_hours": 24,
    "same_country_suppress": true,
    "trusted_ips": [],
    "trusted_cidrs": []
}
```

### New Scheduled Task Type

`geoip_database_update` added to the `scheduled_tasks.task_type` CHECK constraint.

### New Table: `user_location_history`

See the User Location History section above for full DDL.

---

## Rust Implementation

### New Workspace Dependencies

```toml
maxminddb = { version = "0.28", features = ["mmap"] }
```

| Crate | Version | Purpose |
|---|---|---|
| `maxminddb` | 0.28 | Reads MaxMind MMDB files; `mmap` feature for memory-mapped file access |

### Source Module

```
server/src/
├── domains/
│   └── analytics/
│       ├── mod.rs           # Module registration, enrichment pipeline
│       ├── handlers.rs      # Admin analytics API endpoints
│       ├── service.rs       # Trust engine, impossible travel detection, location history
│       ├── geolocation.rs   # MMDB reader, IP lookup, location classification
│       ├── error.rs         # Analytics domain errors
│       └── types.rs         # AnalyticsConfig, LocationType, TrustEvent payloads
```

The `geolocation.rs` module holds the `maxminddb` Reader and provides the enrichment API. The `service.rs` module contains the trust engine that evaluates impossible travel rules.

### GeoIP Reader Lifecycle

```rust
pub struct GeoIpService {
    reader: ArcSwap<maxminddb::Reader<Vec<u8>>>,
    db_path: PathBuf,
}

impl GeoIpService {
    pub fn lookup(&self, ip: IpAddr) -> Option<GeoLocation> {
        let reader = self.reader.load();
        let result = reader.lookup(ip).ok()?;
        let city: geoip2::City = result.decode().ok()?;
        Some(GeoLocation {
            city: city.city?.name?.get("en").map(String::from),
            region: city.subdivisions?.first()?.name?.get("en").map(String::from),
            country: city.country?.iso_code.map(String::from),
            latitude: city.location?.latitude,
            longitude: city.location?.longitude,
        })
    }

    pub fn reload(&self) -> Result<(), AnalyticsError> {
        let new_reader = maxminddb::Reader::open_readfile(&self.db_path)?;
        self.reader.store(Arc::new(new_reader));
        Ok(())
    }
}
```

`ArcSwap` allows hot-reloading the reader without locking — lookups continue using the old reader until the new one is fully loaded and swapped in atomically.

---

## Task 7 Implementation Notes (GeoIP Service)

### Module Location: `services/geoip.rs` (cross-cutting service)

The Rust Implementation section above suggested `domains/analytics/geolocation.rs`. The actual implementation lives at `server/src/services/geoip.rs` per [BUILD_ORDER.md](../design/BUILD_ORDER.md) Task 7. Rationale:

- GeoIP is **cross-cutting** — consumed by both the analytics domain (impossible travel detection, Task 8) and the playback domain (play-session geolocation enrichment at session start). Placing it in `services/` follows the established convention for cross-cutting infrastructure (`encryption.rs`, `event_bus.rs`, `artwork_delivery.rs`, `decision_engine.rs`).
- The impossible-travel trust engine (Haversine, 5-layer suppression) remains in `domains/analytics/service.rs` (Task 8). Only the MMDB reader + IP lookup + location classification live in the shared service.
- The `AnalyticsConfig` (trusted IPs, velocity thresholds, toggles) lives in `RuntimeConfig.analytics` in `state.rs`, not in the service module — keeping configuration with the rest of the config tree.

### `maxminddb` 0.28 API (verified against docs.rs, June 2026)

The 0.28 release reworked the lookup API. The pseudocode above (`result.decode()`, `city.city?.name?`) reflects the older 0.27 API. The implemented code uses the 0.28 API:

| 0.27 API (pseudocode above) | 0.28 API (implemented) |
|---|---|
| `reader.lookup::<geoip2::City>(ip)` → `Result<geoip2::City>` | `reader.lookup(ip)` → `Result<LookupResult>`; then `result.decode::<geoip2::City>()` → `Result<Option<geoip2::City>>` |
| `city.city?.names?.get("en")` (Option-chained) | `city.city.names.english` (direct `Option<&str>` field — 0.28 structs default-deserialize missing fields to `None`, no `Option` unwrap needed) |

- **`geoip2` is a module *within* the `maxminddb` crate** (`use maxminddb::{Reader, geoip2}`), not a separate dependency. No extra crate needed.
- **`geoip2::City<'a>` borrows from the reader's buffer** — the decoded struct's lifetime `'a` is tied to the `&'a Reader`. Lookups must extract owned data (`String`, `f64`) within the scope where the ArcSwap guard lives. The `GeoLocation` return struct is fully owned, sidestepping the lifetime constraint.

### Buffer Type: `Reader<Vec<u8>>` (not `Reader<Mmap>`)

The `mmap` feature is enabled (per the workspace `Cargo.toml` directive) but `Reader::open_readfile()` is used for actual loading, returning `Reader<Vec<u8>>`:

- **Owned, `'static`, `Send + Sync`** — the simplest type that composes cleanly with `ArcSwap<Reader<Vec<u8>>>`. No file-handle or `Mmap`-borrows-from-`File` lifetime entanglement.
- **70 MB memory cost is negligible** for a media server (FFmpeg transcoding uses GBs). The mmap benefit (OS can page out cold regions) is marginal at this size.
- The `mmap` feature remains enabled for future flexibility — switching to `Reader::from_source(mmap)` is a one-line change if memory pressure ever warrants it.

### Graceful Degradation: `ArcSwap<Option<Reader<Vec<u8>>>>`

When the MMDB file is absent or corrupt at startup, the service initializes with `None` stored in the ArcSwap:

- `lookup()` returns `None` → callers (playback enrichment, impossible travel) silently skip geo columns; the server runs normally without geolocation.
- `is_available()` returns `false` → the `GET /api/v1/analytics/geoip/status` endpoint surfaces "GeoIP not configured" in the admin dashboard.
- `reload()` (called by the Task 9 updater after a successful download) populates the `Some(reader)` — lookups begin working without a restart.

This matches the design's "If the MMDB file is missing at startup, geolocation enrichment is skipped; impossible travel detection is disabled; a warning appears in the admin dashboard."

### Location Classification

`classify_location(ip, server_subnets) -> LocationType` lives in the geoip service (not the analytics domain) because it is a pure IP-classification concern consumed by both the playback enrichment path and the impossible-travel suppression layer. It uses `ipnet::IpNet::contains()` (already a workspace dependency, used by `middleware.rs` metrics-subnet guarding):

- **Lan** — RFC 1918 private ranges (`10/8`, `172.16/12`, `192.168/16`), IPv6 ULA (`fc00::/7`), link-local (`169.254/16`, `fe80::/10`), loopback (`127/8`, `::1`), or any CIDR matching the server's own subnet.
- **Wan** — any public IP not matching Lan/Relay.
- **Relay** — reserved for future known-relay/proxy IP list matching (Tailscale `100.64/10` CGNAT range is classified as Lan since it represents the operator's own mesh).

---

## New Error Codes

Analytics security uses the existing trust event system. No new API error codes are needed — trust events are created in the background and surfaced via the admin dashboard, not as API errors.

| Failure | Handling |
|---|---|
| MMDB file missing | Skip geolocation enrichment; log warning at startup; admin dashboard shows "GeoIP not configured" |
| MMDB lookup fails | Skip geo columns for this session; impossible travel detection skipped for this event |
| MaxMind download fails | Keep existing MMDB file; scheduled task logs failure; retry next week |
| Invalid license key | Download fails with 401; admin dashboard shows "GeoIP license key invalid" |

---

## Admin Dashboard: Security Analytics

### Analytics Page

The admin dashboard includes a **Security Analytics** section with:

| Section | Content |
|---|---|
| **Trust Overview** | Per-user trust scores; color-coded (green/yellow/red); click to see event history |
| **Recent Trust Events** | Timeline of impossible travel and other trust events; most recent first; unacknowledged highlighted |
| **User Location Map** | World map showing where each user has streamed from; hover for session count and last seen date |
| **GeoIP Status** | Database file age, size, next update time; warning if stale (>14 days old) |
| **Trusted IPs** | List of trusted IPs/CIDRs with "Add" and "Remove" buttons |

### Event Detail View

Clicking a trust event shows:

| Field | Example |
|---|---|
| User | `John` |
| Rule | Impossible Travel |
| Severity | High |
| Previous location | Chicago, Illinois, US |
| New location | Mumbai, Maharashtra, IN |
| Distance | 12,950 km |
| Time between sessions | 2 hours 14 minutes |
| Implied speed | 5,800 km/h |
| Threshold | 1,000 km/h |
| Same device? | No |
| Country in user history? | No |
| Actions | [Acknowledge] [Revoke Sessions] [Lock Account] [Trust IP] |

---

## Privacy Considerations

### What We Store

The server records the approximate location (country, region, city) of each streaming session. This is derived from the IP address using an offline database — no data is sent to any external service.

### What We Don't Do

- **No precise tracking** — IP geolocation identifies the city at best, and often only the region. It cannot identify a street address or household.
- **No third-party sharing** — IP addresses and location data never leave the server.
- **No cross-user correlation** — each user's location history is independent.
- **Admin visibility** — the server admin (the person who set up the server) can see location data for all users. This is by design — the admin is responsible for the server's security. In a family setting, this is the person who manages the home network.

### Log Handling

IP addresses in logs are masked per the rules in [LOGGING_OBSERVABILITY.md](../operations/LOGGING_OBSERVABILITY.md) — the last octet is hidden at `info` level. Full IP addresses appear only at `debug`/`trace` level. Trust event details (which contain location information) are logged at `info` level with country names only, not coordinates.

---

## Task 8 Implementation Notes (Impossible Travel Detection)

### Trust Engine Location

The trust engine (Haversine + velocity + 5-layer suppression) lives in `domains/analytics/service.rs` per BUILD_ORDER.md Task 8 — not in a separate `services/` module. Rationale:

- The trust engine is tightly coupled to the analytics domain's DB tables (`user_trust_events`, `user_trust_scores`, `user_location_history`, `play_sessions`). Placing it alongside the trust event CRUD functions keeps the detection logic with the data it reads and writes.
- The pure math functions (`haversine_distance`, `implied_velocity_kmh`) are private functions in the same module, tested via unit tests without a DB.
- The cross-cutting `GeoIpService` remains in `services/geoip.rs` (Task 7); the analytics domain consumes it.

### Fire-and-Forget Enrichment

Play-session geo enrichment and impossible travel detection run asynchronously after `start_playback` returns:

1. The `start_playback` handler extracts the client IP via `middleware::extract_client_ip(&headers, Some(&connect_info))`.
2. After `PlaybackStartResponse` is ready, the handler spawns `tokio::spawn(analytics::service::enrich_and_detect(...))`.
3. The spawned task updates `play_sessions` geo columns, upserts `user_location_history`, and runs the detection engine.
4. All errors are logged at `WARN` — enrichment never blocks playback.

This matches the design's "notification-first, never blocks" philosophy. A crashed enrichment task leaves the session without geo data (graceful degradation — the session is valid, just untracked).

### `INET` Column Handling

PostgreSQL `INET` columns (`play_sessions.ip_address`) are bound and decoded as strings, not `std::net::IpAddr`:

- **Bind**: `ip.to_string()` with SQL cast `$N::inet` (PostgreSQL implicitly converts text to INET)
- **Decode**: `row.try_get::<String, _>("ip_address")` then `.parse::<IpAddr>()`

sqlx 0.9 requires the `ipnetwork` feature for native `IpAddr` support. Adding it was rejected to avoid pulling in the `ipnetwork` crate for a single column. String conversion is zero-cost for the typical use case (one IP per session).

### Severity Determination

The design specifies "new continent → high, same continent → medium". The `play_sessions` table has no `geo_continent` column. Severity is determined by Haversine distance instead:

| Condition | Severity | Score Impact |
|---|---|---|
| Either IP is trusted (trusted_ips/trusted_cidrs) | low | -2 |
| Destination country in user's 90-day baseline | low | -2 |
| New country, distance > 4000 km (intercontinental) | high | -10 |
| New country, distance ≤ 4000 km (regional) | medium | -5 |

4000 km approximates the width of a continent or a transatlantic hop. This is a distance-based proxy for the continent comparison.

### `ConnectInfo` Availability

Task 8 added `into_make_service_with_connect_info::<SocketAddr>()` to the `axum::serve` call in `main.rs`. Previously, `ConnectInfo<SocketAddr>` was not available in request extensions, causing the rate limiter and metrics subnet guard to always fall back to `0.0.0.1` for direct connections. Now:

- Handlers can use `ConnectInfo<SocketAddr>` as an axum extractor.
- `extract_client_ip` gets the real socket address as a fallback after X-Forwarded-For and X-Real-IP headers.
- Both proxy-header and direct-connection modes work correctly.

---

## Research Sources

### Impossible Travel Detection
- WorkOS — Impossible Travel: What It Is, How It Works (March 2026): https://workos.com/blog/impossible-travel
- Microsoft — Defender for Cloud Apps Anomaly Detection Policies (November 2025): https://learn.microsoft.com/en-us/defender-cloud-apps/anomaly-detection-policy
- Huntress — Time Travelers Busted: How to Detect Impossible Travel (March 2024): https://www.huntress.com/blog/time-travelers-busted-how-to-detect-impossible-travel-
- CrowdSec — Detecting Suspicious IP Behavior and Impossible Travel (June 2023): https://www.crowdsec.net/blog/detect-suspicious-ip-behavior-impossible-travel
- BlueVoyant — The Impossible Travel Alert: Friend or Foe? (May 2021): https://www.bluevoyant.com/blog/the-impossible-travel-alert-friend-or-foe
- Adaptist Consulting — How to Prevent Hackers from Hijacking Your Employee Accounts (March 2026): https://adaptistconsulting.com/blog/impossible-travel-is/

### IP Geolocation
- Linkly — 7 Best Free GeoIP Databases (2026): https://linklyhq.com/blog/free-geoip-databases
- IPLocate.io — 5 Best MaxMind Alternatives in 2026: https://www.iplocate.io/blog/maxmind-alternatives-2025
- MaxMind GeoLite2 End User License Agreement (February 2026): https://www.maxmind.com/en/geolite/eula

### Rust Integration
- maxminddb crate: https://crates.io/crates/maxminddb
- maxminddb documentation: https://docs.rs/maxminddb
