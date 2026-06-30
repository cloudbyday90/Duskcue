# HTTP Caching

## Overview

This document is the authoritative design for HTTP-layer response caching in Duskcue — the strategy by which the server emits cache directives (`ETag`, `Cache-Control`) and clients (browsers, Smart TVs, Tauri desktop, Flutter mobile) consume them. The goal is to minimize redundant network traffic and perceived latency without sacrificing correctness for personalized, frequently-changing data.

HTTP caching in Duskcue has two complementary layers, both documented here:

1. **Conditional requests** — `ETag` + `If-None-Match` + `304 Not Modified` for cheap revalidation
2. **Cache-Control directives** — `max-age`, `stale-while-revalidate`, `public`/`private`, `no-store`, `immutable` for fresh/stale lifetime control

A third layer — client-side SWR (stale-while-revalidate) via JavaScript data-fetching libraries — is also tracked here as a future evolution, with a documented decision to defer adoption.

## Scope — What This Document Is (and Isn't)

**This document covers:**

- HTTP response headers (`ETag`, `Cache-Control`, `Last-Modified`, `Vary`)
- Per-endpoint cache policy (the contract between server and API consumers)
- `stale-while-revalidate` (RFC 5861 §3) semantics and platform support
- Client-side SWR pattern strategy (TanStack Svelte Query evaluation)
- Implementation status of cache directives across the codebase

**This document does NOT cover:**

- Server-side on-disk cache storage tiers (metadata cache, transcode cache, LRU eviction, disk-space monitoring) — that is a separate operational concern documented in [CACHE_STORAGE.md](../operations/CACHE_STORAGE.md)
- HLS segment caching at the player layer — documented in [STREAMING.md](STREAMING.md)
- Artwork file storage on disk — documented in [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md)
- Browser service-worker caching (not adopted; see Key Decisions)

The clean separation: **CACHE_STORAGE.md is "what the server keeps on disk to avoid recompute"; HTTP_CACHING.md is "what the server tells clients to keep in their HTTP cache to avoid re-fetching."**

## Conditional Requests

### ETag for Cache Validation

Single-resource metadata endpoints return `ETag` headers so clients can revalidate cheaply:

```
GET /api/v1/media-items/01950abc...

HTTP/1.1 200 OK
ETag: "abc123def456"
Content-Type: application/json

{ "id": "01950abc...", "title": "The Matrix", ... }
```

Client revalidation:

```
GET /api/v1/media-items/01950abc...
If-None-Match: "abc123def456"

HTTP/1.1 304 Not Modified
ETag: "abc123def456"
```

The `304 Not Modified` response has no body — the client uses its cached copy. This saves bandwidth on unchanged resources but still requires a network round-trip.

**ETag generation:** SHA-256 hash of the JSON response body, computed after serialization, wrapped in quotes per [RFC 9110 §8.8.3](https://httpwg.org/spec/rfc9110.html#field.etag). Strong validators only (no `W/` weak prefix) — Duskcue's responses are byte-exact across requests when semantically equal.

**Endpoints with ETag:**

| Endpoint | ETag Scope |
|---|---|
| `GET /api/v1/media-items/{id}` | Per-item metadata |
| `GET /api/v1/libraries/{id}` | Library config |
| `GET /api/v1/users/me/tv-surface` | Per-user TV surface feed |
| `GET /api/v1/server/config` | Full server config |

Paginated collection list endpoints do NOT use ETag — paginated responses change frequently (new items shift cursors), so the validation cost exceeds the savings. Bounded personalized feeds such as the TV surface feed may use ETags when the response body is stable for unchanged source data.

### `Last-Modified` — Not Used

Duskcue uses `ETag` exclusively for conditional requests; `Last-Modified` / `If-Modified-Since` is intentionally not emitted. `ETag` is strictly more expressive (sub-second precision, content-hash semantics), and emitting both creates ambiguity about which validator the cache should prefer. Single-validator policy keeps the cache behavior predictable.

## Cache-Control Strategy

### Per-Endpoint Policy

The authoritative per-endpoint table lives in [API_CONVENTIONS.md](API_CONVENTIONS.md) because it is part of the API contract that API consumers depend on. Summary:

| Endpoint | Cache-Control | Rationale |
|---|---|---|
| Media item metadata | `private, max-age=300, stale-while-revalidate=600` | 5 min fresh; 10 min stale-serve; per-user due to watch status |
| Library config | `private, max-age=60, stale-while-revalidate=300` | 1 min fresh; 5 min stale-serve; changes are rare but visible |
| TV surface feed | `private, max-age=60, stale-while-revalidate=300` | 1 min fresh; 5 min stale-serve; personalized launcher rows and resume state |
| Static artwork URLs | `public, max-age=86400, stale-while-revalidate=604800, immutable` | 24 hr fresh; 7 day stale-serve; artwork rarely changes |
| HLS segments | `no-cache` | Always revalidate for streaming session validity |
| HLS manifest / playlist | `no-cache, no-store, must-revalidate` | Live transcode state changes; immutable segments |
| Subtitle content | `no-cache` | Subtitle content may change (offset updated, file re-fetched) |
| Search results | `no-store` | Dynamic, personalized |
| Settings/health/metrics | `no-store` | Operational data; never cached |

### Directive Reference

| Directive | Purpose | Duskcue Usage |
|---|---|---|
| `max-age=N` | Seconds the response is fresh | Used everywhere a cache is permitted |
| `s-maxage=N` | Freshness for shared caches (CDNs/proxies) | **Not used** — Duskcue is self-hosted, no shared cache layer between server and browser |
| `public` | Allows shared-cache storage (overrides `Authorization` default) | Static artwork only |
| `private` | Browser-only cache (never shared) | All authenticated endpoints — responses contain user-scoped data (watch state, capabilities) |
| `no-cache` | Cache may store, must revalidate before reuse | HLS, subtitle content (changing resources) |
| `no-store` | Never store | Search, settings, health (dynamic/operational) |
| `must-revalidate` | Never serve stale once expired (without revalidation) | HLS manifest |
| `immutable` | Client may skip revalidation on reload (cache-busting URLs) | Artwork (fingerprinted URLs) |
| `stale-while-revalidate=N` | Serve stale while background-revalidating | See dedicated section below |
| `stale-if-error=N` | Serve stale on origin error | **Not used** — see exclusion rationale below |

### Per-User Data and the `private` Directive

Almost every Duskcue API response is scoped to the authenticated user — even "metadata" responses embed `is_favorite`, `user_rating`, `resume_position_ms` from `user_item_data`. The default for any new authenticated endpoint is `private, max-age=...` so that responses are never stored in a hypothetical shared cache between users. The only `public` endpoints are unauthenticated static assets (artwork, spritesheets).

## `stale-while-revalidate` (RFC 5861 §3)

### How It Works

The directive tells HTTP caches: "after the response becomes stale, you may continue serving it for N additional seconds, provided you trigger a background revalidation at the same time." This hides revalidation latency from the user — they see instant content from cache, and the cache is silently refreshed for next time.

Three time windows:

```
[0 ─────── max-age ─────── max-age + stale-while-revalidate ─────── ∞)
   FRESH              STALE-AND-SERVE                  MUST-REVALIDATE
   (use cache)        (use cache + background refresh) (network required)
```

### Platform Support (as of June 2026)

| Platform | Support | Behavior when unsupported |
|---|---|---|
| Chrome, Edge, Firefox (desktop) | ✅ Since 2019 (Chrome 75, Firefox 68) | — |
| Samsung Tizen TV | ✅ Tizen 6.0+ (2021, Chromium M76+) | Older Tizen WebKit (pre-2021) silently falls back to `max-age` |
| LG webOS TV | ✅ webOS 5.x+ (2019, Chromium 79+) | Older webOS WebKit silently falls back to `max-age` |
| Tauri desktop (Windows, Linux) | ✅ WebView2 / WebKitGTK are Chromium-based | — |
| Safari (desktop + iOS) | ❌ Not supported (June 2026) | **Silently ignored** — Safari uses `max-age` only |
| Tauri desktop (macOS) | ❌ WKWebView is WebKit-based | Silently ignored — uses `max-age` only |
| Flutter mobile | N/A | Uses Dart HTTP client, separate caching layer |

### Safety Property — Why This Is Zero-Risk

Per [RFC 9111 §5.2](https://httpwg.org/spec/rfc9111.html#field.cache-control), caches MUST ignore cache directives they don't recognize. `stale-while-revalidate` therefore always degrades gracefully to `max-age` behavior on unsupported clients. **There is no failure mode — only a performance mode.** Adding it to responses benefits every Chromium-based client (the realistic Duskcue target: desktop browsers, modern Smart TVs, Tauri-on-Windows) without harming Safari, older TV WebKit, or any other client.

### `stale-if-error` Exclusion Rationale

The companion directive `stale-if-error` (RFC 5861 §4) — "serve stale when the origin errors" — is **intentionally not used**:

1. **No browser support.** `stale-if-error` is not implemented by any major browser (Chrome, Firefox, Safari all ignore it). It's only honored by some CDNs (Fastly, KeyCDN).
2. **No CDN in self-hosted deployment.** Duskcue is self-hosted by operators directly — there is no shared cache layer between the Duskcue server and the user's browser where `stale-if-error` would apply.
3. **The browser HTTP cache is always the cache.** With no intermediary, the only HTTP cache in the request path is the end-user's browser, which doesn't implement this directive anyway.

Adding `stale-if-error` would be cargo-cult header noise with no behavioral effect.

### ETag Interaction

`stale-while-revalidate` and `ETag` conditional requests are **complementary, not alternatives**. When a stale response is served from cache, the background revalidation is a normal HTTP request — it includes `If-None-Match` with the cached ETag. If the content hasn't changed, the server returns `304 Not Modified` (no body), and the cache resets its freshness timer without re-sending the response body. This minimizes both perceived latency AND bandwidth.

Implementation rule: apply `ETag` + `stale-while-revalidate` together on the same endpoints (single-resource metadata endpoints and explicitly bounded personalized feeds). Paginated collection endpoints use neither.

## Client-Side SWR Pattern

Beyond the HTTP directive, the **SWR data-fetching pattern** — popularized by Vercel's `swr` React library and `@tanstack/query` — provides application-level cache management: in-memory deduplication of concurrent requests, programmatic invalidation on mutations, optimistic updates, and background refresh independent of HTTP cache headers.

The Svelte-compatible implementation is [`@tanstack/svelte-query`](https://tanstack.com/query/latest/docs/framework/svelte/overview), which supports Svelte 5 runes. It is pure JavaScript and works on every browser that runs JS — no platform concerns.

### Two-Layer Comparison

| Concern | HTTP directive (Layer 1) | TanStack Svelte Query (Layer 2) |
|---|---|---|
| Where it runs | Browser HTTP cache (below the app) | App JS runtime (above the HTTP cache) |
| Works on all Duskcue targets | ✅ (graceful degradation) | ✅ (pure JS) |
| Bundle cost | Zero JS, server header only | Adds ~13 KB minified + dependency |
| Best for | Read-heavy metadata, artwork | Search, dashboards, in-page mutations |
| Deduplication of concurrent requests | Via HTTP cache lookup | Yes (query-key dedup) |
| Mutation invalidation | No (cache expiry only) | Yes (programmatic `invalidateQueries`) |
| Optimistic updates | No | Yes |
| Background refresh on focus/reconnect | No (only when cache entry is read) | Yes |

### Decision — Adopt Layer 1 Now, Defer Layer 2

**Layer 1 (HTTP `stale-while-revalidate`): Adopt.** This is the existing-but-unimplemented Cache-Control design in [API_CONVENTIONS.md](API_CONVENTIONS.md). Implementation belongs as a Phase 8 follow-up. Zero JavaScript, zero dependencies, zero risk (graceful degradation), benefits every Chromium-based client including modern Smart TVs.

**Layer 2 (TanStack Svelte Query): Defer to Phase 11+.** The existing `svelte/store` pattern (5 stores: auth, user, libraries, player, notifications) handles current UI complexity adequately. TanStack Query shines for highly-interactive surfaces where:

- Multiple components need the same data (dedup)
- Mutations need to invalidate related queries (admin CRUD)
- Background refresh on window focus matters (long-lived dashboard sessions)
- Optimistic updates smooth perceived latency

These conditions appear in Phase 11 (analytics dashboards), Phase 12 (overlay editor, collection builder UI), and Phase 13 (admin settings). Revisit adoption when those surfaces land. Migrating earlier would add a dependency and refactor cost without proportional benefit.

When Layer 2 is adopted, the two layers compose: TanStack Query reads from the JS layer on cache hit, falls through to `fetch()` which hits the HTTP cache (Layer 1) before going to the network. The `staleTime` option on TanStack Query should be set to the same value as the corresponding endpoint's `max-age` to avoid surprising divergence between the two caches.

## Service Workers — Not Adopted

Service workers could provide fine-grained cache control (Workbox recipes, LRU policies, broadcast updates). Duskcue does not use service workers:

1. **Self-hosted, low-latency origin.** Service-worker caching's main win is offline support and resilient handling of flaky/slow origins. Duskcue's origin is on the user's own LAN (or their own VPS) — network failures are operator-visible problems to fix, not user-experience problems to paper over.
2. **Smart TV support is poor.** Tizen and webOS TV browsers have limited or no service worker support across model years. The HTTP `Cache-Control` directive works universally on Chromium-based TVs; service workers do not.
3. **Complexity cost.** Service workers add a second cache layer that must be kept coherent with the HTTP cache and the JS data layer — invalidation logic, version skew on deploys, debugging complexity. The benefit does not justify the cost for Duskcue's deployment model.

The HTTP `Cache-Control` directive + eventual TanStack Query covers the legitimate use cases without service workers.

## Implementation Status

| Component | Status | Notes |
|---|---|---|
| `ETag` / `If-None-Match` / `304` | ✅ Implemented | `cache::conditional_etag` computes SHA-256 ETags for JSON single-resource routes and honors existing artwork ETags. Web client `core.js` already has `options.ifNoneMatch` plumbing. |
| `Cache-Control: no-cache` on subtitle content | ✅ Implemented | `domains/subtitles/handlers.rs::get_subtitle_content` |
| `Cache-Control` per-endpoint table | ✅ Implemented for available routes | `SetResponseHeaderLayer::if_not_present` applies media metadata, library config, artwork, server config/config groups, health, metrics, and search policies at route level. |
| `stale-while-revalidate` directives | ✅ Implemented | Emitted on media metadata, library config, and static artwork routes per the API contract. |
| TanStack Svelte Query | Not adopted | Deferred to Phase 11+ per decision above |

### Pre-v1.0 Task 1 Implementation Notes

Pre-v1.0 Hardening Task 1 wires the HTTP caching contract without changing handler response DTOs:

- `server/src/cache.rs` owns the cache policy constants, `SetResponseHeaderLayer::if_not_present` helper, SHA-256 ETag generation, and conditional request handling.
- `GET /api/v1/media-items/{id}` and `GET /api/v1/libraries/{id}` emit private `Cache-Control` headers with `stale-while-revalidate` and SHA-256 ETags over the serialized JSON body.
- `GET /api/v1/users/me/tv-surface` emits private `Cache-Control` headers with `stale-while-revalidate` and SHA-256 ETags over a stable data-derived feed body.
- `GET /api/v1/server/config` emits `Cache-Control: no-store` plus a SHA-256 ETag for explicit client revalidation. `GET /api/v1/server/config/{group}` emits `no-store` without ETag because it is not listed in the ETag contract.
- `GET /api/v1/items/{id}/artwork/{type}` emits the public immutable artwork cache policy via route middleware and continues using the existing strong artwork variant ETag.
- `/health` and `/metrics` emit `Cache-Control: no-store`.
- Cache layers are attached before mutation methods are chained on mixed-method paths, so PATCH/PUT/DELETE handlers are not cacheable.
- ETag-bearing responses are excluded from the global gzip compression layer so strong validators are computed over the same bytes that are delivered.

## Key Decisions

1. **HTTP directive over client library for the primary layer** — `Cache-Control: stale-while-revalidate` works at the HTTP layer with zero JavaScript, zero dependencies, and universal Chromium support (including Smart TVs). Client-side SWR (TanStack) is a complement for later, not a replacement.
2. **`stale-while-revalidate` is safe to add unconditionally** — RFC 9111 §5.2 mandates that unknown directives be ignored, so Safari and older TV WebKit simply fall back to `max-age`. There is no downside.
3. **No service workers** — the self-hosted LAN deployment model and poor Smart TV support make the complexity unjustiated. HTTP `Cache-Control` + future TanStack Query covers the real needs.
4. **No `stale-if-error`** — no browser implements it; Duskcue has no CDN in front; it would be cargo-cult header noise.
5. **No `Last-Modified`** — `ETag` (strong, content-hash) is strictly more expressive. Single-validator policy avoids cache ambiguity.
6. **`private` by default on authenticated endpoints** — Duskcue responses embed user-scoped data (watch state, ratings, capabilities); never allow shared-cache storage of authenticated responses.
7. **`immutable` only on fingerprinted URLs** — artwork URLs include content hashes (TMDB file path + ID), so they're truly immutable across versions and skip revalidation even on reload.
8. **`s-maxage` never used** — Duskcue is self-hosted with no shared cache; the browser is always the only HTTP cache.
9. **ETag and `stale-while-revalidate` together** — applied jointly on single-resource metadata endpoints and explicitly bounded personalized feeds. Paginated collection endpoints use neither.

## Relationship to Other Domains

| Document | Relationship |
|---|---|
| [API_CONVENTIONS.md](API_CONVENTIONS.md) | Authoritative API contract — the per-endpoint Cache-Control/ETag table lives there; this document explains the strategy and semantics behind those values |
| [CACHE_STORAGE.md](../operations/CACHE_STORAGE.md) | Server-side on-disk cache tiers (metadata, transcode, artwork). Complementary — that's "what the server keeps"; this is "what the server tells clients to keep" |
| [STREAMING.md](STREAMING.md) | HLS segment and manifest cache semantics are defined there; this document cross-references but does not redefine streaming-specific cache rules |
| [SUBTITLES.md](SUBTITLES.md) | Subtitle content delivery uses `Cache-Control: no-cache` (content may change with offset/fetch); the rationale is documented there, the directive semantics here |
| [SECURITY.md](../security/SECURITY.md) | The `private` directive is a security control preventing cross-user cache leakage in hypothetical shared-cache deployments |
| [BUILD_ORDER.md](../../BUILD_ORDER.md) | Phase 8 follow-up tracks Cache-Control/ETag implementation; Phase 11+ tracks TanStack Svelte Query adoption |

## Research Sources

- **[RFC 5861](https://datatracker.ietf.org/doc/html/rfc5861)** — Cache Control Extensions for Stale Content (defines `stale-while-revalidate` and `stale-if-error`)
- **[RFC 9110 §8.8.3](https://httpwg.org/spec/rfc9110.html#field.etag)** — HTTP ETag field semantics
- **[RFC 9111](https://httpwg.org/spec/rfc9111.html)** — HTTP Caching (successor to RFC 7234); §5.2 defines the "unknown directives MUST be ignored" safety property
- **[web.dev: Keeping things fresh with stale-while-revalidate](https://web.dev/articles/stale-while-revalidate)** — Chrome 75 / Firefox 68 ship reference; use-case patterns
- **[MDN: Cache-Control header](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Cache-Control)** — Directive reference, browser compatibility notes
- **[tower-http `SetResponseHeaderLayer`](https://docs.rs/tower-http/latest/tower_http/set_header/response/struct.SetResponseHeaderLayer.html)** — Route-layer response header insertion used for Cache-Control policies
- **[Samsung Tizen Web Engine Specifications](https://developer.samsung.com/smarttv/develop/specifications/web-engine-specifications.html)** — Per-year Tizen / Chromium version mapping (Tizen 6.0 = M76, Tizen 10.0 = M130)
- **[LG webOS Web API and Web Engine](https://webostv.developer.lge.com/develop/specifications/web-api-and-web-engine)** — webOS TV Chromium version history (webOS 4.x = Chromium 68, webOS 5.x = Chromium 79, webOS 26 = Chromium 132)
- **[TanStack Query Svelte docs](https://tanstack.com/query/latest/docs/framework/svelte/overview)** — `@tanstack/svelte-query` API and SSR patterns
