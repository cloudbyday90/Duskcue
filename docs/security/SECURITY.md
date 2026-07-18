# Security & Remote Access Domain

## Overview

This document is the authoritative design for the security and remote access domain. It covers: network security tiers, TLS configuration, streaming URL authentication, HTTP security hardening, HTTP compression and BREACH mitigation, timing attack resistance, real-time event security (SSE), security event monitoring, remote access patterns, Cloudflare TOS constraints, and FFmpeg per-process sandboxing (Landlock + seccomp).

The platform is **local-first** — security features are **opt-in**, not opt-out. A server on a trusted LAN needs no TLS, no signed URLs, and minimal auth friction. When the admin enables remote access, security hardening activates progressively.

Authentication and user management are documented in [AUTH.md](../design/AUTH.md). API-level conventions (rate limiting, CORS, session cookies) are documented in [API_CONVENTIONS.md](../design/API_CONVENTIONS.md). Application-layer API security (input validation, BOLA prevention, SSRF prevention, response DTO separation, admin endpoint isolation) is documented in [API_SECURITY.md](API_SECURITY.md). OS hardening, Docker Engine requirements, and platform compatibility are documented in [OS_HARDENING.md](../operations/OS_HARDENING.md). This document covers the network, transport, and streaming security layers that wrap around those systems.

---

## Network Security Tiers

### Three-Tier Model

| Tier | Network Scope | TLS | Auth | Streaming Security | Default |
|---|---|---|---|---|---|
| **1 — Local** | localhost, LAN, VPN | Optional | Passkey optional | None (direct URLs) | Yes |
| **2 — Remote** | VPN tunnel (WireGuard, Tailscale, Pangolin) | Optional | Required | None (VPN = LAN) | Opt-in |
| **3 — Exposed** | Public internet | Required | Required | HMAC signed URLs | Opt-in |

### Tier 1 — Local Network (Default)

The server binds to `0.0.0.0:48027` (HTTP) by default. Native IPv6 support is planned for Phase 15 through a configurable bind address (`DUSKCUE_BIND_ADDRESS`, with `::` for IPv6/dual-stack where supported by the host). No TLS, no signed streaming URLs. Auth is controlled by `server_config.auth.auth_required` — in local mode, the admin can disable auth entirely for a single-user setup.

- HTTP on port 48027 (configurable)
- No TLS termination
- Passkey auth optional (can be disabled for single-user LAN)
- Direct HLS URLs (no signing)
- Intended for: LAN, localhost, Docker internal network

### Native IPv6 Security Requirements

IPv6 support must preserve the same security model as IPv4:

- Public IPv6 addresses are treated as remote/exposed addresses, not as LAN by default.
- Loopback `::1/128`, ULA `fc00::/7`, link-local `fe80::/10`, and IPv4-mapped IPv6 addresses are classified explicitly by the network and analytics layers.
- Forwarded client-IP headers are accepted only when the immediate peer is the loopback internal proxy; direct peers cannot select their own rate-limit or audit IP with `X-Forwarded-For` or `X-Real-IP`. A configurable non-loopback trusted-proxy CIDR allowlist remains deferred, so external reverse proxies must forward through the local Duskcue web proxy.
- Generated URLs containing IPv6 literals must use bracket notation, for example `https://[2001:db8::10]:48027`.
- Exposed IPv6 deployments require the same controls as exposed IPv4 deployments: TLS, authentication, signed streaming URLs, strict security headers, and correctly configured trusted proxies.

### Tier 2 — Remote via VPN Tunnel (Opt-In)

The admin connects the server to a VPN (WireGuard, Tailscale, Pangolin, Headscale). Clients connect through the VPN tunnel, making the server behave as if it were on the LAN. No application-level changes required — the VPN handles encryption and authentication.

- HTTP on VPN IP (e.g., Tailscale 100.x.x.x, WireGuard 10.x.x.x)
- No TLS needed at application layer (VPN provides encryption)
- Auth required (enforced when VPN users are not trusted LAN users)
- Direct HLS URLs (VPN = trusted network)
- Platform does not embed WireGuard — documented setup guides for external VPNs

### Tier 3 — Public HTTPS Exposure (Opt-In)

The admin configures `server_config.auth.network_mode = "exposed"` and provides a domain name. The server activates TLS, signed streaming URLs, strict security headers, and mandatory authentication.

- HTTPS via rustls with ACME auto-cert (Let's Encrypt)
- Auth mandatory (forced to `auth_required = true`)
- HMAC-SHA256 signed streaming URLs
- Full security headers (HSTS, CSP, X-Frame-Options, etc.)
- Intended for: direct internet exposure, reverse proxy fronting

### Why No Embedded WireGuard

Embedding a WireGuard server into the platform creates cross-platform issues:

- Requires TUN device access (platform-specific: `/dev/net/tun` on Linux, utun on macOS, wintun on Windows)
- Kernel module differences across Linux, macOS, Windows, Synology NAS
- Docker networking complexity (requires `--cap-add=NET_ADMIN`, `--device /dev/net/tun`)
- Synology NAS may not expose TUN devices to Docker containers
- NAT traversal (hole punching) requires a relay/coordination server
- Permissions vary wildly: root on Linux, admin on macOS, service on Windows

Instead, the platform provides **first-class setup guides and UI integration** for external VPN solutions. The admin UI includes a "Remote Access" section with guided setup for Tailscale, Headscale, WireGuard (manual), and Pangolin.

---

## Cloudflare TOS and Video Streaming

### The Problem

Cloudflare's CDN-specific terms (as of the September 2025 TOS update) prohibit using the CDN to serve video and large files hosted outside Cloudflare's own storage products (R2, Stream, Images). While the old Section 2.8 was removed in May 2023, the restriction moved to CDN-specific service terms.

In practice:
- Cloudflare Tunnel routes all public HTTP traffic through the CDN layer
- Streaming personal video through a Cloudflare Tunnel violates CDN terms
- Multiple community reports of account warnings after heavy video streaming
- The 100MB upload limit also blocks large file uploads through tunnels
- No public UDP support (limits WireGuard through tunnels)

**Conclusion: Cloudflare CDN/Tunnel cannot be used for video streaming.**

### Recommended Alternatives for Remote Access

| Solution | Type | Works Behind CGNAT | Open Ports | Self-Hosted | Streaming OK |
|---|---|---|---|---|---|
| **WireGuard (self-hosted)** | VPN | No (needs public IP) | 1 UDP | Yes | Yes |
| **Tailscale** | Mesh VPN | Yes | None | No (cloud control plane) | Yes |
| **Headscale** | Mesh VPN (Tailscale-compatible) | Yes | None | Yes | Yes |
| **Pangolin** | Identity-based reverse proxy + WireGuard | Yes | None (VPS receives) | Yes (AGPLv3) | Yes |
| **Rathole** | Reverse proxy tunnel (Rust) | Yes | 1 TCP on VPS | Yes (Apache 2.0) | Yes |
| **Frp** | Reverse proxy tunnel | Yes | 1 TCP on VPS | Yes (Apache 2.0) | Yes |
| **Caddy/Nginx + port forward** | Reverse proxy | No (needs public IP) | 80, 443 | Yes | Yes |

### Platform Guidance (Not Built-In)

The admin UI provides a **Remote Access Setup** page with:

1. **Detect network type** — check if behind CGNAT (connect to STUN server or check if port forwarding works)
2. **Recommend based on network type:**
   - Public IP → Caddy/Nginx reverse proxy + ACME TLS
   - CGNAT → Tailscale (easy) or Pangolin (self-hosted) or Rathole (Rust-native)
3. **Provide setup guides** — step-by-step for each recommended solution
4. **Detect when server is behind VPN** — check if default route goes through VPN interface (tun/wg/utun)

The platform does not install, configure, or manage any VPN or tunnel software. It provides documentation and detection only.

---

## TLS Configuration

### Library: rustls

**rustls** is the TLS library — pure Rust, memory-safe, no OpenSSL dependency.

| Aspect | rustls | OpenSSL |
|---|---|---|
| Memory safety | Yes (Rust) | No (C, history of CVEs) |
| Static linking | Yes | No (system lib) |
| Cross-compilation | Simple | Complex (requires sysroots) |
| Docker image size | No change | +20MB for libssl |
| Performance | Excellent (TLS 1.3) | Excellent |
| TLS 1.3 | Default | Supported |
| FIPS 140-3 | Via aws-lc-rs backend | Native |
| Maintenance | Ironbound (well-funded) | OpenSSL Foundation |

### TLS Requirements

| Parameter | Value |
|---|---|
| Minimum TLS version | TLS 1.2 |
| Preferred TLS version | TLS 1.3 |
| Cipher suites (TLS 1.3) | `TLS_AES_256_GCM_SHA384`, `TLS_AES_128_GCM_SHA256`, `TLS_CHACHA20_POLY1305_SHA256` |
| Cipher suites (TLS 1.2) | `ECDHE-ECDSA-AES256-GCM-SHA384`, `ECDHE-ECDSA-CHACHA20-POLY1305`, `ECDHE-ECDSA-AES128-GCM-SHA256` |
| Key exchange | ECDHE only (forward secrecy mandatory) |
| Certificate type | ECDSA (P-256) preferred; RSA (2048+) accepted |
| HSTS | `max-age=63072000; includeSubDomains; preload` (2 years) |

### ACME Certificate Management

When `network_mode = "exposed"`, the server automatically provisions TLS certificates via ACME (Let's Encrypt):

- **Challenge type:** HTTP-01 (default) or DNS-01 (for behind-reverse-proxy setups)
- **Challenge directory:** `data_dir` + `/acme-challenges/`
- **Certificate storage:** `data_dir` + `/tls/`
- **Auto-renewal:** checked every 24 hours; renews at 30 days before expiry
- **Key rotation:** new key pair on each renewal
- **Staging support:** Let's Encrypt staging directory for initial setup testing
- **Custom CA:** configurable for enterprise environments

### TLS in Docker

In the Docker deployment, TLS termination can be handled by:
1. **The platform directly** — rustls binds port 443, manages ACME certs
2. **A reverse proxy** — Caddy/Traefik/Nginx in front, handling TLS and forwarding to the platform on HTTP

The platform's single-container model supports both patterns. When a reverse proxy is detected (via `X-Forwarded-Proto` header), the platform trusts the proxy's TLS termination and does not duplicate it.

---

## Streaming URL Authentication

### HMAC-SHA256 Signed URLs

When `network_mode = "exposed"`, all HLS manifests and segments require signed URLs. This prevents unauthorized users from accessing media content even if they discover a streaming URL.

### Signing Scheme

```
GET /api/v1/stream/{media_item_id}/{variant}/index.m3u8
    ?token=<HMAC-SHA256 signature>
    &expires=<Unix epoch seconds>

GET /api/v1/stream/{media_item_id}/{variant}/seg-{number}.m4s
    ?token=<HMAC-SHA256 signature>
    &expires=<Unix epoch seconds>
```

### Signing Parameters

| Parameter | Manifest (`.m3u8`) | Segment (`.m4s`) |
|---|---|---|
| **TTL** | 60 seconds | 300 seconds |
| **Bound to** | User session ID | User session ID (via wildcard path) |
| **Signing key** | HMAC-SHA256, rotated every 24h | Same key |
| **Path scope** | Exact manifest path | Wildcard: `/api/v1/stream/{media_item_id}/{variant}/*` |

### Signing Flow

1. Client requests manifest URL via authenticated API (`GET /api/v1/media/{id}/play`)
2. Server validates user is authorized for this media item
3. Server generates a signed manifest URL: `path + expires + HMAC(session_id + path + expires)`
4. Client fetches manifest — server validates signature + expiry
5. Manifest contains relative segment URLs — client requests segments
6. Segment requests validated against same session via wildcard path signature

### Key Management

- **Signing key:** 256-bit random key, stored in `server_config.security` JSONB
- **Rotation:** every 24 hours via scheduled task; old key accepted for 2x TTL during rotation
- **Dual-key validation:** both current and previous key accepted during rotation window
- **Key generation:** `ring::hmac::Key::generate(HMAC_SHA256, SystemRandom::new())`

### Why Session-Bound, Not IP-Bound

Mobile users switch between Wi-Fi and cellular mid-session, changing their IP address. IP-bound tokens cause false 403s on every network transition. Session binding ties the token to the server-side session ID, which is stable regardless of network changes.

### Cache Key Isolation

Signed URL parameters (`token`, `expires`) must be excluded from the HTTP cache key. This prevents every user from generating a separate cache entry for the same segment. The cache layer strips query parameters when computing cache keys for streaming endpoints.

---

## HTTP Security Headers

When `network_mode = "exposed"`, the following security headers are applied as a Tower middleware layer:

| Header | Value | Purpose |
|---|---|---|
| `Strict-Transport-Security` | `max-age=63072000; includeSubDomains; preload` | Forces HTTPS for 2 years |
| `X-Content-Type-Options` | `nosniff` | Prevents MIME sniffing |
| `X-Frame-Options` | `DENY` | Prevents clickjacking via iframes |
| `Referrer-Policy` | `strict-origin-when-cross-origin` | Limits referrer information leakage |
| `Permissions-Policy` | `geolocation=(), microphone=(), camera=()` | Disables unnecessary browser APIs |
| `Content-Security-Policy` | `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob:; object-src 'none'; base-uri 'self'; frame-ancestors 'none'` | Prevents XSS, clickjacking, injection |

### CSP Details

The CSP is designed for a media streaming application:
- `media-src 'self' blob:` — allows streaming from signed URLs and blob URLs (hls.js uses blob for MSE)
- `img-src 'self' data: blob:` — allows artwork from server, inline data URIs, and blob URLs
- `style-src 'self' 'unsafe-inline'` — `unsafe-inline` for styles is standard for Svelte-generated CSS; no security risk for styles
- `script-src 'self'` — no inline scripts, no eval, no external CDNs
- `object-src 'none'` — no Flash, no Java, no plugins
- `frame-ancestors 'none'` — supersedes X-Frame-Options for CSP-aware browsers

In local mode, CSP is relaxed to `default-src 'self' 'unsafe-inline' 'unsafe-eval' blob: data: media:` for development convenience. The admin UI shows a warning when running in local mode with relaxed CSP.

### Header Behavior by Tier

| Header | Local | Remote (VPN) | Exposed |
|---|---|---|---|
| HSTS | Off | Off | On |
| X-Content-Type-Options | On | On | On |
| X-Frame-Options | Off | Off | On (DENY) |
| Referrer-Policy | Off | Off | On |
| Permissions-Policy | Off | Off | On |
| CSP | Relaxed | Relaxed | Strict |

---

## HTTP Compression and BREACH Mitigation

### The Problem

The BREACH attack exploits HTTP compression when both attacker-controlled data and a secret (like a session token) appear in the same compressed response. By observing the compressed response size, an attacker can gradually extract the secret byte-by-byte.

This is relevant because `tower-http` includes `compression-gzip` for response compression. If a page reflects user input and also includes a session token, compression creates a side channel.

### Our Approach

Compression is selectively disabled based on endpoint sensitivity. This is the standard industry mitigation — there is no known way to make compression safe alongside secrets in the same response.

| Endpoint Type | Compression | Why |
|---|---|---|
| **Static assets** (CSS, JS, images, fonts) | **Enabled** | No secrets in static files; compression saves bandwidth |
| **Artwork** (posters, backdrops, thumbnails) | **Enabled** | No secrets in image data; large files benefit from compression |
| **Authentication endpoints** (login, tokens) | **Disabled** | Responses contain session tokens; BREACH risk |
| **Admin API** (config, user management) | **Disabled** | Responses may contain sensitive config values |
| **Streaming manifests and segments** | **Disabled** | Already binary-encoded; compression provides minimal benefit and adds latency |
| **Public API** (library listings, metadata) | **Enabled** | No secrets in responses; compression saves bandwidth |

### Implementation

```rust
use tower_http::compression::CompressionLayer;

let compression_layer = CompressionLayer::new()
    .compressible_content_types(STATIC_CONTENT_TYPES);

let static_content_types: [&[u8]; 6] = [
    b"text/css",
    b"text/javascript",
    b"application/javascript",
    b"image/svg+xml",
    b"application/json",
    b"text/html",
];
```

The compression layer is only applied to the general API router. The admin router and auth router do not include the compression layer.

### Why Not Disable All Compression

Disabling compression everywhere wastes bandwidth on static assets and large metadata responses. A typical library listing can be 50-200 KB of JSON — compression reduces this by 70-80%. The selective approach protects sensitive endpoints without penalizing normal browsing.

### Why Not Random Padding

Random padding (adding noise to responses to mask compression ratios) is theoretical and adds complexity. Selective disabling is the proven, simple approach used by major web frameworks.

---

## Timing Attack Resistance

### What Timing Attacks Are

A timing attack tries to guess a secret by measuring how long a comparison takes. If the code checks characters one by one and returns `false` at the first mismatch, an attacker can measure the response time to determine how many characters they guessed correctly.

### Our Protection

The critical operations that compare secrets all use constant-time comparison from the `ring` cryptography library:

| Operation | Comparison Method | Constant-Time? |
|---|---|---|
| Streaming URL HMAC signature validation | `ring::hmac::verify_with_own_key` | Yes — `ring` uses constant-time comparison internally |
| Session token validation | Session ID is a UUIDv7; looked up in DB, not compared character-by-character | N/A — database lookup, not string comparison |
| Password hashing | `argon2` (or `bcrypt`) | Yes — password hashing algorithms are inherently constant-time |
| TOTP code verification | `ring::hmac` or constant-time compare | Yes |

### Why Standard `==` Is Acceptable for Non-Secrets

Some comparisons use Rust's standard `==` operator:

- **UUIDv7 identifiers** — these are not secrets. They appear in URLs, API responses, and logs. Guessing a UUIDv7 by timing `==` gives no advantage because the value is already public.
- **Display names, library names, media titles** — not secrets; no timing protection needed.
- **Email addresses** — compared during login lookup (find the user by email, then verify the passkey). The email itself is not the secret being verified.

The only place where constant-time comparison matters is when the compared value is a secret that an attacker is trying to guess. All such operations use `ring`.

### Decision

No additional action is needed. The `ring` library handles all secret comparisons correctly. This section exists to document that this was evaluated, not overlooked.

---

## Real-Time Event Security (SSE)

### When This Applies

Server-Sent Events (SSE) carries server→client push: transcode progress, scan progress, notifications, session kicks, playback commands. The transport decision is documented in [REAL_TIME_PUSH.md](../design/REAL_TIME_PUSH.md); this section defines the security posture. SSE is **unidirectional server→client over standard HTTP** — most of the WebSocket-specific attack surface (client-injected frames, subprotocol abuse, binary message handling) does not apply.

### Requirements

| Requirement | Implementation |
|---|---|
| **Authenticated connection** | SSE `GET /api/v1/events` requires a valid session cookie, identical to any authenticated REST endpoint. No unauthenticated streams. Revalidated on every reconnect. |
| **Same-origin enforcement** | `Origin` header checked against `server_config.security.allowed_origins` in exposed mode (same CORS policy as REST endpoints — SSE is just HTTP) |
| **Per-user connection limit** | 5 concurrent SSE connections per user (covers multi-tab + multi-device); excess connections rejected with HTTP 429 rate-limit response. Prevents SSE-based DoS. |
| **Per-user event rate limit** | Inherits the authenticated-user rate-limit tier (300 req/min). Publishing beyond the limit triggers backpressure, not connection drop. |
| **Idle timeout / heartbeat** | 15-second KeepAlive comment frames (`:keep-alive\n\n`) detect dead connections without waiting for TCP keepalive; dead connections free their subscriber slot promptly. |
| **Authorization scoped at publish time** | The server publishes events only to the owning user's stream; admins receive their own events plus events for users they can manage (`can_manage_users`). No cross-user event leakage even if a connection is hijacked. |
| **No client→server payload over SSE** | SSE is server→client only. Client→server actions (e.g., remote-control commands from a phone) go via authenticated REST POST endpoints with full input validation. There is no attack surface for client-injected event-stream data. |
| **Event payloads are server-authored JSON** | All event payloads originate from typed Rust structs serialized via `serde_json` — no string interpolation, no SQL, no shell arguments. Same DTO/validator pattern as REST responses per [API_SECURITY.md](API_SECURITY.md). |
| **No `Last-Event-ID` injection** | The `Last-Event-ID` header on reconnect is parsed as a UUID; malformed values are ignored (full ring-buffer replay is skipped, client falls through to live events). The header never reaches SQL or command layers. |

### Why SSE Reduces Attack Surface vs WebSocket

The prior design (WebSocket, see git history) required dedicated security controls that SSE obviates:

| WebSocket risk | SSE status |
|---|---|
| Authenticated handshake via query-string token (token leaks in URLs/logs) | ✅ Eliminated — SSE uses session cookies like every other HTTP request |
| Client message rate limiting (30 msg/sec) | ✅ Eliminated — clients cannot send messages over SSE |
| Binary frame injection (potential deserialization bugs) | ✅ Eliminated — SSE is text-only with a fixed wire format |
| Subprotocol negotiation abuse | ✅ Eliminated — no subprotocol negotiation in SSE |
| Cross-origin WebSocket smuggling via crafted `Upgrade` headers | ✅ Eliminated — no protocol upgrade; standard CORS applies |
| Ping/pong spoofing for keepalive bypass | ✅ Eliminated — KeepAlive is server→client only |

### Out of Scope

- **Mobile OS push notifications (FCM/APNs)** — separate transport with its own security profile; documented in Phase 16
- **Event payload schemas per event type** — defined in their domain docs (transcode events in [STREAMING.md](../design/STREAMING.md), scan events in [MEDIA_SCANNING.md](../design/MEDIA_SCANNING.md), notifications in Phase 13)

---

## Security Event Monitoring

### What We Monitor

The server tracks security-relevant events and shows them in the admin dashboard. This is not a full security information and event management (SIEM) system — it is a simple, built-in view that helps the server owner notice problems without installing additional software.

### Events Shown in Admin Dashboard

| Event | Where It Appears | Alert Threshold |
|---|---|---|
| Failed login attempts | Security panel | 5+ failures from same IP in 15 minutes → notification |
| Rate limit triggers | Security panel | Logged individually; summary notification at 10+/hour |
| Invalid streaming URL signatures | Security panel | Logged individually; summary notification at 20+/hour |
| New device connections | Security panel | Each new device shown; no alert threshold |
| Session revocations | Security panel | Logged individually |
| Backup encryption status | System health panel | Warning if S3 backup has no encryption |
| TLS certificate expiry | System health panel | Warning at 30 days; error at 7 days |

### Admin Quick Actions

The admin dashboard includes one-click actions for common security responses:

| Action | What It Does |
|---|---|
| **Revoke all sessions** | Deletes all rows in `user_sessions`; every user must re-authenticate |
| **Rotate streaming signing keys** | Generates new HMAC key immediately; old key accepted for 2x TTL during transition |
| **Lock user account** | Sets `locked_until` on the user; prevents login until admin unlocks |
| **Generate new invite code** | Creates a new admin invite code; old codes remain valid until expired |
| **Export security log** | Downloads security events as JSON for the selected time range |

### Why Not a Full Incident Response Plan

This is a self-hosted personal and family Duskcue, not a business system. Full incident response procedures (severity classification, escalation paths, communication templates, post-incident reviews) are designed for organizations with teams, compliance requirements, and legal obligations. For a home server, the most important things are:

1. **See the problem** — the admin dashboard shows security events clearly
2. **Fix it quickly** — one-click actions handle the common responses
3. **Prevent recurrence** — rate limiting, account lockout, and encryption are always active

If the server is exposed to the internet and experiences a real attack, the owner should revoke all sessions, rotate keys, and check the audit log for unauthorized access — all achievable from the admin dashboard in under a minute.

---

## Session Security

### Cookie Configuration

Session cookies are configured differently based on network mode:

| Attribute | Local | Exposed |
|---|---|---|
| `HttpOnly` | Yes | Yes |
| `Secure` | No (HTTP) | Yes (HTTPS) |
| `SameSite` | `Lax` | `Strict` |
| `Path` | `/` | `/` |
| `Max-Age` | 90 days | 90 days |
| `Domain` | Not set | Not set |

### Bearer Token Configuration

Bearer tokens (for mobile, desktop, API clients) follow the same tiered approach:

| Attribute | Local | Exposed |
|---|---|---|
| Transport | HTTP | HTTPS |
| Lifetime | 1 hour (access), 30 days (refresh) | 1 hour (access), 30 days (refresh) |
| Storage | Client-managed | Client-managed |
| Revocation | Server-side session store | Server-side session store |

Bearer tokens are never stored in URLs, query parameters, or logs. The `Authorization: Bearer mv_...` header is stripped from log output by the tracing middleware.

Phase 16a client storage rules are defined in [DESKTOP_MOBILE_CLIENTS.md](../design/DESKTOP_MOBILE_CLIENTS.md): desktop uses Tauri Stronghold or OS-backed secure storage, and mobile uses Android Keystore/iOS Keychain through a vetted plugin or platform channel. Plaintext app preferences, browser localStorage, logs, diagnostics bundles, and crash reports must not contain bearer tokens, refresh tokens, push tokens, signed media URLs, or future offline-download package secrets.

Phase 16c offline-download storage rules are defined in [OFFLINE_DOWNLOADS.md](../design/OFFLINE_DOWNLOADS.md). Download manifests and package files must not contain bearer tokens, refresh tokens, raw signed URLs, source filesystem paths, or reusable package secrets. Android package files live under app-private no-backup storage; iOS package files live under Application Support with backup exclusion and first-unlock file protection. Sensitive metadata, sync queues, server/user/device bindings, and future package keys use OS-protected or encrypted storage. Short-lived package transfer URLs are used only for foreground transfer and are not persisted in local metadata. Offline playback events store event IDs, package IDs, bounded event types, positions, completion/watched flags, and timestamps in the protected sync queue for reconnect sync; the server stores only bounded accepted-event IDs in device-state metadata to prevent duplicate replay. The server can revoke new downloads and online package serving immediately, but fully offline devices can only disable/delete already-downloaded packages after local expiry enforcement or reconnect policy sync.

Phase 16a server selection stores only non-secret server origins and labels in the client saved-server list. Clients canonicalize origins to `http(s)://<server>:48027`, reject Docker's internal `48028` API port, and test `/health/ready` before selecting a server. Local and Remote VPN modes may use HTTP because the deployment is LAN/VPN-scoped; Exposed mode requires HTTPS with a certificate trusted by the client OS. Self-signed and private-CA certificates are not silently trusted by mobile clients and must be installed/trusted through Android or iOS device management before connection.

Phase 16a auth/session clients separate non-secret connection state from bearer-token state. Desktop stores bearer tokens through the OS credential store via the Rust `keyring` crate and keys entries by normalized server origin. Mobile stores the session token, cached user summary, and stable client device identifier through `flutter_secure_storage`, backed by Android Keystore/iOS Keychain. When the API reports `401`, mobile clears the bearer token and cached user before returning to the auth flow.

### Remembered Household Profiles

A remembered household profile is a non-secret, server-side preference keyed by the authenticated account and a stable device ID. It can select the active profile for a newly authenticated session, but it never authenticates a request, extends a session, or replaces a password, passkey, bearer token, cookie, or parental PIN. The server verifies the profile/account relationship on every lookup and removes the preference on explicit sign-out, remote session revocation, or sign-out everywhere. Browser clients may persist only the opaque device ID needed for this preference; bearer tokens and parental secrets remain prohibited from browser storage.

### Kids Profile Parent Unlock

Each PIN-protected Kids profile stores only a salted Argon2id PHC hash. Duskcue uses the OWASP minimum baseline of 19 MiB memory, two iterations, and one lane; the raw 4–12 digit PIN is accepted only by the create, update, or active-profile unlock request and is never returned, logged, cached, embedded in a URL, or sent through the browser's profile-change signal. The profile row persists a five-attempt/15-minute lockout so restarts, new tabs, and session refreshes cannot reset brute-force protection. A valid PIN grants a ten-minute unlock only for that Kids profile on the current server-side session. Changing the PIN or changing to another profile revokes the unlock. The endpoint returns generic invalid/locked Problem Details and deliberately omits an exact retry schedule.

This is a shared-display profile boundary, not a replacement for the account's normal password/passkey/session authorization or MFA. It prevents profile-picker escape while a child uses a shared authenticated TV, but cannot secure credentials a parent intentionally discloses.

---

## CORS Configuration

| Mode | `Access-Control-Allow-Origin` | Credentials |
|---|---|---|
| Local | `*` (or specific LAN origins) | Yes |
| Exposed | Exact origin(s) from `server_config.security.allowed_origins` | Yes |

In exposed mode, CORS is strict — only explicitly listed origins are allowed. Wildcard `*` is never used with `Access-Control-Allow-Credentials: true` (browser security model forbids it).

Full CORS design in [API_CONVENTIONS.md](../design/API_CONVENTIONS.md).

---

## Rust Implementation

### New Workspace Dependencies

```toml
rustls = "0.23"
tokio-rustls = "0.26"
ring = "0.17"
tower-http = { version = "0.6", features = ["cors", "trace", "compression-gzip", "set-header"] }
```

- **rustls 0.23** — pure Rust TLS; memory-safe; no OpenSSL
- **tokio-rustls 0.26** — async TLS streams for Tokio (server-side acceptor)
- **ring 0.17** — HMAC-SHA256 signing key generation; cryptographic randomness; same library used by rustls internally
- **tower-http 0.6** — add `set-header` feature for security header middleware

### Security Header Middleware

```rust
use axum::http::{HeaderValue, header};
use tower_http::set_header::SetResponseHeaderLayer;

fn security_headers_layer(exposed: bool) -> Vec<SetResponseHeaderLayer> {
    let mut layers = vec![
        SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ),
    ];

    if exposed {
        layers.push(SetResponseHeaderLayer::if_not_present(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=63072000; includeSubDomains; preload"),
        ));
        layers.push(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ));
        layers.push(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ));
    }

    layers
}
```

### HMAC Signing Service

```rust
use ring::hmac;

pub struct StreamSigner {
    current_key: hmac::Key,
    previous_key: hmac::Key,
    key_rotated_at: chrono::DateTime<chrono::Utc>,
}

impl StreamSigner {
    pub fn sign_manifest(
        &self,
        session_id: &str,
        path: &str,
        ttl_seconds: u64,
    ) -> String {
        let expires = chrono::Utc::now().timestamp() + ttl_seconds as i64;
        let payload = format!("{session_id}:{path}:{expires}");
        let signature = hmac::sign(&self.current_key, payload.as_bytes());
        format!("token={}&expires={expires}", hex::encode(signature.as_ref()))
    }

    pub fn validate(
        &self,
        session_id: &str,
        path: &str,
        token: &str,
        expires: i64,
    ) -> bool {
        if chrono::Utc::now().timestamp() > expires {
            return false;
        }
        let payload = format!("{session_id}:{path}:{expires}");
        let Ok(sig_bytes) = hex::decode(token) else {
            return false;
        };
        hmac::verify_with_own_key(&self.current_key, payload.as_bytes(), &sig_bytes)
            .or_else(|_| {
                hmac::verify_with_own_key(&self.previous_key, payload.as_bytes(), &sig_bytes)
            })
            .is_ok()
    }
}
```

### Source Module

```
server/src/
├── domains/
│   └── security/
│       ├── mod.rs           # Module registration, tier detection
│       ├── tls.rs           # rustls configuration, ACME, cert management
│       ├── signing.rs       # HMAC-SHA256 streaming URL signing
│       ├── headers.rs       # Security header middleware
│       └── types.rs         # SecurityConfig, NetworkTier, etc.
```

This follows a slightly different pattern than the five-file domain pattern (mod/handlers/service/error/types) because the security domain is infrastructure, not business logic — it has no API handlers or error codes of its own. It wraps around all other domains.

---

## Database Schema

### server_config.security JSONB

```json
{
    "network_mode": "local",
    "allowed_origins": [],
    "tls": {
        "enabled": false,
        "port": 443,
        "acme_directory": "https://acme-v02.api.letsencrypt.org/directory",
        "acme_email": "",
        "challenge_type": "http-01",
        "cert_path": "",
        "key_path": "",
        "hsts_max_age_seconds": 63072000,
        "min_tls_version": "1.2"
    },
    "stream_signing": {
        "enabled": false,
        "manifest_ttl_seconds": 60,
        "segment_ttl_seconds": 300,
        "key_rotation_hours": 24
    },
    "vpn_detection": {
        "auto_detect": true,
        "vpn_interfaces": ["tun0", "wg0", "utun", "tailscale0"]
    }
}
```

No new tables are needed. The `server_config.security` JSONB column stores all security configuration. The `server_config.auth` JSONB column (defined in [AUTH.md](../design/AUTH.md)) already contains `network_mode`, `require_https`, and `auth_required` fields — the security JSONB extends this with TLS, signing, and VPN detection configuration.

The `security` column is added to the existing `server_config` table.

---

## Error Codes

No dedicated error codes for the security domain. Security failures map to existing codes:

| Failure | Mapped Code | Domain |
|---|---|---|
| Invalid streaming signature | `PLAY_005` (Stream not authorized) | Playback |
| Expired streaming signature | `PLAY_005` (Stream not authorized) | Playback |
| TLS cert acquisition failure | `SYS_001` (Configuration error) | System |
| TLS handshake failure | `SYS_001` (Configuration error) | System |
| CSP violation report | Logged only, no error response | — |

---

## Admin UI: Remote Access Setup

### Setup Wizard Page

The admin UI includes a **Remote Access** page under Settings:

1. **Current Status** — shows detected network mode (local/VPN/exposed)
2. **Network Detection** — auto-detect:
   - Check for VPN interfaces (`tun0`, `wg0`, `utun`, `tailscale0`)
   - Check if behind CGNAT (connect to STUN server)
   - Check if ports are reachable from external IP
3. **Setup Guides** — expandable sections with step-by-step instructions:
   - Tailscale (recommended for ease)
   - Headscale (self-hosted Tailscale control plane)
   - WireGuard (manual setup)
   - Pangolin (identity-based reverse proxy)
   - Rathole (Rust-native tunnel)
   - Caddy/Nginx (reverse proxy with ACME)
4. **Expose Directly** — if admin wants direct HTTPS:
   - Enter domain name
   - Choose ACME challenge type (HTTP-01 or DNS-01)
   - Enter email for Let's Encrypt
   - Server provisions cert and enables TLS

### Security Dashboard

When exposed, the admin UI shows a security dashboard with:
- TLS certificate status (expiry, issuer, SANs)
- Active streaming sessions with signed URL status
- Security header compliance check
- Rate limiting status per tier
- Failed authentication attempts (from audit log)

---

## FFmpeg Per-Process Sandboxing

FFmpeg is a large, complex C application that processes untrusted input (user media files). Any vulnerability in FFmpeg's decoders could lead to arbitrary code execution in the context of the running process. To mitigate this, FFmpeg child processes are sandboxed with two complementary Linux security mechanisms, applied in the child process via `Command::pre_exec()` (between fork and exec). Both gracefully degrade on unsupported platforms.

Process lifecycle management (spawn, graceful shutdown, zombie prevention, bounded output) uses `tokio-process-tools` v0.11.2. See [MEMORY.md](../design/MEMORY.md) for full lifecycle design.

### Landlock LSM — Filesystem Sandboxing

**Crate**: `landlock` (actively maintained, unprivileged, Rust-first API)

**Kernel requirement**: Linux 5.13+ with `CONFIG_SECURITY_LANDLOCK=y`. The Alpine container baseline runs on the host kernel; supported Docker hosts in [OS_HARDENING.md](../operations/OS_HARDENING.md) satisfy this requirement.

**Graceful degradation**: If the kernel does not support Landlock, enforcement is silently skipped with a `tracing::warn!` log. Protection falls back to DAC (file ownership and permissions) only.

**Policy — what FFmpeg can access:**

| Path | Access | Rationale |
|---|---|---|
| `/data/media/{library}/` | Read-only | Read source media files for transcoding |
| `/cache/transcodes/{session_id}/` | Read-write | Write HLS segments and manifest for this session only |
| `/usr/lib/`, `/lib/` | Read-only | Shared libraries (codec implementations, fontconfig) |
| `/usr/share/`, `/etc/` | Read-only | Fonts, codec configurations, locale data |
| `/dev/dri/` | Read-only | Hardware acceleration devices (NVENC, VAAPI, QSV) |
| `/tmp/` | Read-write | FFmpeg temporary files during transcode |
| Everything else | Denied | No access to database, config, secrets, user data, other sessions |

**How it works:**

```rust
#[cfg(target_os = "linux")]
fn apply_landlock(session_id: &str, media_path: &Path, transcode_dir: &Path) {
    use landlock::{Access, AccessFs, Ruleset, RulesetAttr, PathBeneath, PathFd};

    let access_ro = AccessFs::from_bits(Access::ReadFile | Access::ReadDir)
        .unwrap();
    let access_rw = access_ro | AccessFs::from_bits(Access::WriteFile | Access::MakeDir).unwrap();

    let ruleset = Ruleset::new()
        .handle(access_ro | access_rw)?
        .add_rule(PathBeneath::new(PathFd::new(media_path)?, access_ro))?
        .add_rule(PathBeneath::new(PathFd::new(transcode_dir)?, access_rw))?
        .add_rule(PathBeneath::new(PathFd::new("/usr")?, access_ro))?
        .add_rule(PathBeneath::new(PathFd::new("/lib")?, access_ro))?
        .add_rule(PathBeneath::new(PathFd::new("/etc")?, access_ro))?
        .add_rule(PathBeneath::new(PathFd::new("/dev/dri")?, access_ro))?
        .add_rule(PathBeneath::new(PathFd::new("/tmp")?, access_rw))?
        .restrict_self()?;

    Ok(())
}
```

**Key properties:**
- **Unprivileged** — no root, no SUID binary, no external daemon. Any process can restrict itself
- **Stacks with SELinux/AppArmor** — Landlock is a stackable LSM; no conflict with existing MAC
- **Per-session isolation** — each FFmpeg process can only write to its own transcode directory, not other sessions'
- **Irreversible** — once `landlock_restrict_self()` is called, the restrictions cannot be loosened. Even if FFmpeg is compromised, it cannot escape

### Seccomp-BPF — Syscall Filtering

**Crate**: `seccompiler` (from rust-vmm, used by Firecracker and production VMMs)

**Approach**: Allow-list — only explicitly permitted syscalls pass; everything else triggers `SIGSYS` (process killed). This is the safest approach: deny-lists must be updated whenever a new dangerous syscall is added to the kernel.

**Installation**: Applied via `seccompiler::apply_filter()` in `Command::pre_exec()` — between fork and exec. Only FFmpeg gets the filter; the parent server is unrestricted.

**Allow-list profile (approximate — determined via `strace -fc ffmpeg [typical transcode command]`):**

| Syscall | Category | Rationale |
|---|---|---|
| `read`, `write`, `close`, `lseek` | I/O | Basic file operations |
| `openat`, `fstat`, `fstatfs`, `statx` | File metadata | Open files, stat paths |
| `mmap`, `munmap`, `mprotect`, `madvise`, `brk` | Memory | Memory management (FFmpeg uses mmap extensively) |
| `poll`, `ppoll`, `epoll_create1`, `epoll_ctl`, `epoll_wait` | Event loop | Event polling for I/O |
| `futex`, `clock_gettime`, `clock_nanosleep`, `nanosleep` | Threading/time | Thread synchronization, timing |
| `ioctl` | Device control | HW acceleration (NVENC/VAAPI/QSV ioctls) |
| `dup`, `dup2`, `dup3`, `pipe2` | FD management | File descriptor operations |
| `fcntl`, `fcntl64` | File control | File locking, FD flags |
| `getdents64`, `access`, `faccessat2` | Directory/permission | Directory listing, access checks |
| `readlink`, `readlinkat` | Symlinks | Resolve symlinks |
| `uname`, `sysinfo`, `getrandom` | System info | System call info, random numbers |
| `sigaction`, `sigprocmask`, `rt_sigreturn` | Signals | Signal handling |
| `exit_group`, `clone` (with restrictions) | Process | Exit; cloning for FFmpeg threading |
| `arch_prctl` (x86_64 only) | Architecture | Thread-local storage setup |
| `set_tid_address`, `fadvise64`, `rseq` | Thread/perf | Thread registration, read-ahead hints |
| `prctl` (restricted) | Process control | Limited to specific operations |

**Blocked dangerous syscalls:**

| Syscall | Why blocked |
|---|---|
| `execve`, `execveat` | No spawning new processes |
| `fork`, `vfork` | No forking (FFmpeg uses `clone` for threads) |
| `ptrace` | No process inspection/debugging |
| `mount`, `umount2` | No filesystem manipulation |
| `chroot`, `pivot_root` | no filesystem namespace escape |
| `connect`, `bind`, `listen`, `accept` | No network (FFmpeg should not access network) |
| `socket`, `socketpair` | No socket creation |
| `keyctl`, `add_key`, `request_key` | No kernel keyring access |
| `perf_event_open` | No performance monitoring |
| `kcmp`, `process_vm_readv`, `process_vm_writev` | No cross-process memory access |

**How it works:**

```rust
#[cfg(target_os = "linux")]
fn apply_seccomp() -> Result<(), seccompiler::Error> {
    use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, SeccompRule};
    use std::convert::TryInto;

    let allowed_syscalls: Vec<(i64, Vec<SeccompRule>)> = vec![
        (libc::SYS_read, vec![]),
        (libc::SYS_write, vec![]),
        (libc::SYS_openat, vec![]),
        (libc::SYS_close, vec![]),
        (libc::SYS_fstat, vec![]),
        (libc::SYS_mmap, vec![]),
        (libc::SYS_munmap, vec![]),
        (libc::SYS_mprotect, vec![]),
        (libc::SYS_ioctl, vec![]),
        (libc::SYS_futex, vec![]),
        (libc::SYS_clock_gettime, vec![]),
        (libc::SYS_exit_group, vec![]),
        // ... remaining allow-list entries
    ];

    let filter: BpfProgram = SeccompFilter::new(
        allowed_syscalls.into_iter().collect(),
        SeccompAction::KillProcess,
        SeccompAction::Allow,
        std::env::consts::ARCH.try_into().unwrap(),
    )?
    .try_into()?;

    seccompiler::apply_filter(&filter)?;
    Ok(())
}
```

**Applied in `pre_exec`:**

```rust
#[cfg(target_os = "linux")]
{
    command = command.pre_exec(|| {
        apply_landlock(&session_id, &media_path, &transcode_dir)?;
        apply_seccomp()?;
        Ok(())
    });
}
```

**Key properties:**
- **Inherited by threads** — seccomp filters apply to the calling thread and all threads it spawns. FFmpeg's worker threads inherit the filter
- **Irreversible** — filters cannot be removed once installed. Compromised FFmpeg cannot escape
- **Low overhead** — BPF programs are JIT-compiled to native instructions by the kernel. Negligible per-syscall cost
- **Allow-list approach** — only known-safe syscalls pass. New kernel syscalls are denied by default until explicitly allowed
- **x86_64 + aarch64** — `seccompiler` supports both our target architectures
- **Feature-gated** — `#[cfg(target_os = "linux")]`; not compiled on Windows/macOS

### Defense-in-Depth Summary

FFmpeg processes are protected by multiple independent layers. Compromise of one layer does not compromise the others:

| Layer | What it restricts | Mechanism | Applied by |
|---|---|---|---|
| Container isolation | Process/filesystem/network namespace | Docker `read_only`, `cap_drop ALL`, `no-new-privileges` | Docker runtime |
| Landlock LSM | Filesystem paths | `landlock` crate, `landlock_restrict_self()` | Server (child pre_exec) |
| Seccomp-BPF | System calls | `seccompiler` crate, `apply_filter()` | Server (child pre_exec) |
| Process groups | Signal isolation | `tokio-process-tools` (`process_group(0)`) | Library (automatic) |
| Bounded output | Memory consumption | `tokio-process-tools` (bounded buffers) | Library (configured) |
| Priority | CPU/I/O scheduling | `nice` / `ionice` | Server (command args) |
| File permissions | POSIX DAC | `PUID`/`PGID` non-root user | Docker runtime |

### Platform Compatibility

| Platform | Landlock | Seccomp | tokio-process-tools | Notes |
|---|---|---|---|---|
| Linux x86_64 (Docker) | Yes (kernel 6.x) | Yes | Full (SIGTERM) | All layers active |
| Linux ARM64 (Docker) | Yes (kernel 6.x) | Yes | Full (SIGTERM) | All layers active |
| Linux bare metal | If kernel ≥ 5.13 | Yes | Full (SIGTERM) | All layers active |
| macOS (Apple Silicon) | No | No | Full (SIGTERM) | Sandbox falls back to DAC |
| Windows | No | No | Full (CTRL_BREAK) | Sandbox falls back to DAC |

---

## Research Sources

### Cloudflare TOS and Video Streaming
- Cloudflare Self-Serve Subscription Agreement (Updated September 12, 2025): https://www.cloudflare.com/terms/
- Cloudflare Blog — Goodbye, section 2.8 and hello to Cloudflare's new terms of service (May 2023): https://blog.cloudflare.com/updated-tos/
- Localtonet — Cloudflare Tunnel Alternative: When You Need TCP, UDP, or No Domain (March 2026): https://localtonet.com/blog/cloudflare-tunnel-alternative
- Hacker News — State of Homelab 2026 (April 2026): https://news.ycombinator.com/item?id=47746577

### Remote Access Patterns
- JellyWatch — Jellyfin Remote Access via VPN: Tailscale and WireGuard (March 2026): https://jellywatch.app/blog/jellyfin-vpn-wireguard-tailscale-remote-access-2026
- XDA Developers — Pangolin: Self-hosted reverse proxy management server (April 2026): https://www.xda-developers.com/i-dont-use-tailscale-or-nginx-to-access-my-home-lab-remotely-heres-what-i-use-instead/
- Pangolin — Pangolin vs. Tailscale Comparison (January 2026): https://pangolin.net/news/pangolin-v-tailscale
- Pinggy — Top 10 Cloudflare Tunnel Alternatives in 2026 (May 2026): https://pinggy.io/blog/best_cloudflare_tunnel_alternatives/
- Reddit r/selfhosted — Minimum security steps every self-hosted server should have (February 2026): https://www.reddit.com/r/selfhosted/comments/1r4lpld/

### Streaming Security
- BlazingCDN — CDN Signed URLs and Token Authentication Explained (November 2025): https://blog.blazingcdn.com/en-us/cdn-signed-urls-and-token-authentication-explained
- Fora Soft — Live Streaming Security: The 2026 Playbook (April 2025): https://www.forasoft.com/blog/article/security-considerations-live-streaming-protecting-content

### Application Security
- Redfox Cybersecurity — Web Application Security Best Practices: 2026 Checklist (May 2026): https://www.redfoxsec.com/blog/web-application-security-best-practices-a-developers-checklist-for-2026
- Reddit r/homelab — Reverse Proxy Security Best Practices (December 2025): https://www.reddit.com/r/homelab/comments/1pwzhwh/reverse_proxy_security_best_practices/
- Reddit r/webdev — Reasonable security baseline for self-hosted services 2026 (February 2026): https://www.reddit.com/r/webdev/comments/1qvtnja/

### Authentication (2026)
- dev.to — 5 Authentication Patterns Every Web Developer Should Know in 2026 (March 2026): https://dev.to/alanwest/5-authentication-patterns-every-web-developer-should-know-in-2026-50ol

### BREACH/CRIME Compression Attacks
- OWASP — BREACH Attack: https://owasp.org/www-community/attacks/BREACH_attack
- Qualys — SSL/TLS Compression and CRIME/BREACH: https://blog.qualys.com/category/threat-research
- Mozilla MDN — HTTP Compression Security Considerations: https://developer.mozilla.org/en-US/docs/Web/HTTP/Compression
- Reddit r/netsec — BREACH Mitigation in Practice 2026 (January 2026): https://www.reddit.com/r/netsec/comments/1h7g2jx/

### Timing Attacks
- Coda Hale — A Lesson in Timing Attacks (2013, still authoritative): https://codahale.com/a-lesson-in-timing-attacks/
- ring cryptography library — constant-time operations: https://github.com/briansmith/ring

### FFmpeg Sandboxing
- StackExchange Security — Security risks of using FFmpeg as part of web service (December 2018): https://security.stackexchange.com/questions/200487/
- PeerTube GitHub — Run FFmpeg with reduced privileges Issue #1371 (November 2018): https://github.com/Chocobozzz/PeerTube/issues/1371
- rust-vmm seccompiler — seccomp-BPF jailing library for Rust: https://github.com/rust-vmm/seccompiler
- landlock-lsm rust-landlock — Rust library for Linux Landlock sandboxing: https://github.com/landlock-lsm/rust-landlock
- Linux Kernel Documentation — Landlock: unprivileged access control: https://docs.kernel.org/userspace-api/landlock.html
- HardenedLinux — GNU/Linux Sandboxing: A Brief Review (August 2024): https://hardenedlinux.org/blog/2024-08-20-gnu/linux-sandboxing-a-brief-review/
- pelagos-containers — Landlock LSM integration Issue #51 (March 2026): https://github.com/pelagos-containers/pelagos/issues/51
- NVIDIA NemoClaw — Sandbox Image Hardening (Landlock + seccomp): https://docs.nvidia.com/nemoclaw/deployment/sandbox-hardening

### Implementation Status

**Implemented** in `server/src/services/sandbox.rs` (Phase 7, Task 3). Key implementation decisions:

- `landlock` v0.4 with `ABI::V3` for access flag computation; `AccessFs::from_read()` for RO paths, `AccessFs::from_all()` for RW paths
- `seccompiler` v0.4 with 62-syscall allow-list; `SeccompAction::KillProcess` on mismatch, `SeccompAction::Allow` on match
- Platform-gated to Linux via `#[cfg(target_os = "linux")]`; no-op on Windows/macOS
- `libc` v0.2 for `SYS_*` constants (unconditional dep)
- `SandboxConfig` borrows paths as `&Path`; closures capture `PathBuf` clones for `'static + Send` in `pre_exec`
- Graceful degradation: sandbox failure logs warning but returns `Ok(())` so FFmpeg still starts
- `apply_landlock()` silently skips non-existent paths (e.g., `/dev/dri` on headless systems)
- `target_arch()` returns `seccompiler::TargetArch` based on compile-time `cfg`; `arch_prctl` gated to `x86_64`
- Phase 15 container-build verification fixed Linux-only compile requirements: Landlock `Access` trait import for `AccessFs::from_all()`, `seccompiler` BPF conversion errors mapped to `std::io::Error`, and `pre_exec` registration wrapped in explicit Linux-only `unsafe` blocks

### Self-Hosted Security Monitoring
- Reddit r/selfhosted — Minimum Security Steps (February 2026): https://www.reddit.com/r/selfhosted/comments/1r4lpld/
- Tenzai — Security Dashboard Design for Self-Hosted Applications (December 2025): https://tenzai.com/blog/self-hosted-security-dashboard/
