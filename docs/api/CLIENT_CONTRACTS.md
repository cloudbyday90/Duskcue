# Client Contracts

## Purpose

This document defines the Phase 16a desktop/mobile client contract strategy and its Phase 16d promotion into shared client contracts for desktop, mobile, TV, and console platforms. It supports the task list in [BUILD_ORDER.md](../../BUILD_ORDER.md).

The machine-readable starting point is [client-contracts.v1.json](client-contracts.v1.json).
The Phase 16d binding target matrix is [client-binding-targets.v1.json](client-binding-targets.v1.json).
The Phase 16d versioned fixture pack starts at [fixtures/client/v1/manifest.json](fixtures/client/v1/manifest.json).
The Phase 16d playback conformance pack starts at [fixtures/playback/v1/manifest.json](fixtures/playback/v1/manifest.json).

## Decision

Phase 16a uses a **curated checked-in contract manifest** as the immediate source of truth for desktop/mobile API consumption.

The longer-term direction is **generated OpenAPI or JSON Schema contracts**, but Duskcue does not yet emit OpenAPI or JSON Schema from the Rust server. Building that generator now would pull Phase 16d contract-QA work into Phase 16a. The pragmatic Phase 16a path is:

1. Curate the desktop/mobile route inventory in `docs/api/client-contracts.v1.json`.
2. Verify each route still exists in the Rust server.
3. Verify each route with an existing web helper keeps a matching helper name in `clients/web/src/lib/api`.
4. Use this manifest when writing mobile DTOs and typed errors.
5. Promote to generated schemas in Phase 16d when conformance fixtures and broader platform SDK generation are in scope.

## Research Summary

| Option | Pros | Cons | Phase 16a posture |
|---|---|---|---|
| OpenAPI 3.1 | Industry-standard HTTP contract, broad tooling, can generate Dart/TypeScript clients, uses JSON Schema vocabulary | Requires either manual spec maintenance or Rust schema generation work not currently present | Target for Phase 16d, not the first Phase 16a gate |
| JSON Schema 2020-12 fixtures | Strong response/request shape validation, language-neutral, good for fixture drift checks | Does not describe HTTP methods/auth/cache/SSE by itself | Use later for generated fixtures and DTO validation |
| Rust `crates/types` as direct source | Keeps DTOs near server code, avoids duplicate type names | Current server domains mostly define DTOs locally; Dart cannot consume Rust types directly without a generator | Future input, not current source |
| Existing web API helpers | Match implemented UI behavior and bearer-token semantics | JavaScript helpers are not schema-rich and do not cover every denial case | Use as compatibility evidence and helper-name drift check |
| Curated manifest | Fast, explicit, reviewable, works today, can cover auth/cache/error expectations | Requires discipline to update when routes change | Chosen for Phase 16a |

## Contract Scope

The manifest covers the route groups desktop/mobile need from:

- health and readiness
- auth, passkeys, device linking, sessions, and user preferences
- libraries, media, search, collections
- playback, stream/HLS URLs, watch data, subtitles, segments, storyboards
- quality capability/telemetry/QoE reporting
- notifications, foreground SSE, push-device lifecycle
- minimal server settings for admin-capable clients

The manifest intentionally does not make every current admin-only endpoint a Phase 16a mobile requirement. Desktop may keep admin-heavy workflows web-first.

## Error Contract

All JSON API errors use RFC 9457 Problem Details with Duskcue extensions:

```json
{
  "type": "/errors/auth_001",
  "title": "AUTH_001",
  "status": 401,
  "detail": "Passkey not found",
  "instance": "/api/v1/auth/webauthn/finish",
  "trace_id": "01972c00-...",
  "errors": null
}
```

Validation errors use `title = "VALID_001"` and include an `errors` array with per-field `field`, `code`, and `message`.

Mobile maps Problem Details into typed client error kinds:

| Kind | Rule |
|---|---|
| `network` | No HTTP response or transport failure |
| `authExpired` | HTTP 401 |
| `permissionDenied` | HTTP 403 |
| `rateLimited` | HTTP 429 |
| `notFound` | HTTP 404 |
| `conflict` | HTTP 409 |
| `validation` | `VALID_001` or HTTP 422 with field errors |
| `serverUnavailable` | HTTP 503 or 504 |
| `transcodeUnavailable` | playback/transcode-related unavailable errors |
| `playbackPolicy` | playback policy and streaming-policy denials |
| `unknown` | fallback |

The initial Dart implementation lives in `clients/mobile/lib/api/problem_detail.dart` and `clients/mobile/lib/api/client_error.dart`.

## Phase 16c Offline Downloads

Phase 16c Task 9 extends the curated manifest with the mobile-only `/api/v1/downloads/*` route set. These routes are not Phase 16a online-client requirements, but they are now consumed by `clients/mobile/lib/services/download_service.dart` for the offline download manager shell and are verified by `scripts/verify-client-contracts.mjs` against the Rust server route table.

## Auth Session Metadata

Phase 16a mobile/desktop auth requests include stable client metadata where the server DTO supports it:

- `device_id`
- `device_name`
- `client_name`
- `client_version`
- `client_platform`

Mobile generates and secure-stores a stable `device_id` through `flutter_secure_storage`. Desktop uses the selected server origin as the key for OS-keyring token storage and sends client metadata from the reused web/native bridge flows as those screens are wired.

## Bearer Token Semantics

Desktop/Tauri webview keeps compatibility with `clients/web/src/lib/api/core.js`:

- API base path is `/api/v1`.
- Web and Tauri webview calls use same-origin credentials.
- Native desktop or mobile calls inject `Authorization: Bearer <token>` from OS-backed secure storage.
- Bearer tokens are never put in query strings, logs, crash reports, or diagnostics bundles.
- Desktop stores bearer tokens in the OS keyring through Tauri commands; mobile stores them through `flutter_secure_storage`.
- `204 No Content` and `304 Not Modified` map to `null`/empty success.
- Media and HLS URL builders return URLs for platform media components rather than JSON-fetching binary streams.

## Drift Control

Run:

```bash
node scripts/verify-client-contracts.mjs
```

The verifier currently checks:

- each route path in `client-contracts.v1.json` exists in `server/src`;
- each declared web helper name exists under `clients/web/src/lib/api`;
- duplicate method/path pairs are rejected.

Phase 16d Task 1 extends the manifest into the shared client contract source of truth for desktop, mobile, TV, and console phases. `client-contracts.v1.json` now includes:

- the required Phase 16d domain list;
- standard Problem Details expectations by route class;
- cache profiles for health, authenticated JSON, private ETag JSON, binary/media bytes, and mutation responses;
- pagination profile names;
- foreground SSE event payload inventory for notifications, download jobs, TV surface changes, and session lifecycle events;
- a `contract` block on every route covering response schema, cache profile, pagination profile, path/query validation metadata, request schema, and expected Problem Details codes.

The verifier now fails when a required Phase 16d domain is missing or a route lacks the required contract metadata. Later Phase 16d tasks extend this into response fixtures, generated bindings, and broader CI conformance tests.

## Phase 16d Binding Targets

Phase 16d Task 2 adds [client-binding-targets.v1.json](client-binding-targets.v1.json) as the machine-readable SDK and binding strategy. The current source remains the curated manifest plus checked-in fixtures. The immediate output is typed fixture contracts and a common adapter contract rather than fully generated SDKs, because the Rust server still does not emit OpenAPI 3.1 or JSON Schema.

The matrix defines target strategies for:

- TypeScript/Tauri webview helpers;
- Dart/Flutter mobile DTOs and services;
- Kotlin Android TV / Fire TV clients;
- Swift tvOS / iOS clients;
- Roku BrightScript clients;
- Samsung Tizen and LG webOS JavaScript clients;
- Windows/Xbox clients once their app shell is selected.

Every target must cover the required Phase 16d domains from the manifest and must implement or explicitly adapt the same shared concerns: base URL resolution, bearer-token injection, re-auth/session revoke handling, timeout/retry behavior, Problem Details mapping, pagination helpers, cache/ETag handling, SSE decoding, secure storage, and diagnostics redaction.

Run:

```bash
node scripts/verify-client-bindings.mjs
```

The verifier fails if a required target, required shared adapter, required manifest domain, or fixture requirement is missing. Generated clients remain the target direction for TypeScript, Dart, Kotlin, and Swift once server-emitted schemas exist; Roku and some TV web targets stay fixture-first unless platform tooling makes full client generation practical.

## Phase 16d Client Fixtures

Phase 16d Task 3 adds a versioned client fixture pack under `docs/api/fixtures/client/v1`. It is intentionally broader than the earlier TV-only fixtures and gives downstream clients a stable response corpus before generated schemas exist. The pack includes:

- server selection and readiness;
- auth login and device-link polling;
- user preferences and reviewed locale metadata;
- library success and empty states;
- media detail, artwork URLs, search facets, and collection rows;
- playback start/resume plus heartbeat/seek/stop sequence examples;
- subtitles, audio tracks, segments, storyboard metadata, and artwork variants;
- device quality, bandwidth, and QoE payloads;
- download inventory, transfer URL, and reconnect-sync examples;
- notifications, SSE, push-device, and settings examples;
- TV surface and deep-link resolve examples;
- denial cases for revoked sessions, missing library access, unavailable media, expired playback URLs, transcode unavailable, quota denial, stale client state, and TV access denial.

Run:

```bash
node scripts/verify-client-fixtures.mjs
```

The verifier checks that the fixture manifest covers every required Phase 16d domain, every required fixture exists, rows marked with ordering rules are stable, UUIDs and platform content IDs have canonical shapes, timestamps use UTC RFC3339 strings, enum values stay inside approved sets, Problem Details denial fixtures are complete, server-owned localized strings are display-ready, and fixtures do not leak local paths, bearer headers, signed URLs, or package signatures.

## Phase 16d Playback Conformance

Phase 16d Task 4 adds a reusable playback conformance pack under `docs/api/fixtures/playback/v1`. This pack is separate from the general client fixture pack because playback clients need an ordered state-machine contract, not only request/response examples.

The pack covers:

- start, resume seek, first frame, heartbeat, pause, resume, seek, stop, completion, and playback-error transitions;
- supported and unsupported audio/subtitle selection, including downmix/transcode and image-subtitle burn-in cases;
- direct play, direct stream, and HLS transcode handoff paths with credential material kept out of URLs;
- remote/media-session actions for play, pause, seek backward/forward, seek-to, and stop;
- QoE payloads for startup, buffering, bitrate, quality changes, selected quality mode, and playback failure;
- cross-device resume refresh, including TV-surface refresh events and stale launcher-cache avoidance;
- Problem Details examples for transcode unavailable, expired media URL, and unavailable track selections.

Run:

```bash
node scripts/verify-playback-conformance.mjs
```

The verifier checks required state events, event ordering, playback API paths, stream decision coverage, track-selection cases, media-session action mappings, QoE field coverage, cross-device resume expectations, Problem Details error shape, UTC timestamps, stable UUIDs, and redaction of tokens, signatures, and private paths.

Phase 16b added reusable TV surface fixtures ahead of full Phase 16d generation:

```bash
node scripts/verify-tv-surface-fixtures.mjs
```

The TV verifier checks `docs/api/fixtures/tv` feed, resolve, diagnostics, unavailable, and golden-render fixtures for section order, stable IDs, cache/ETag expectations, access-revoked behavior, and privacy-safe content.

## Relationship to Other Docs

| Document | Relationship |
|---|---|
| [DESKTOP_MOBILE_CLIENTS.md](../design/DESKTOP_MOBILE_CLIENTS.md) | Phase 16a client strategy and task implementation notes |
| [API_CONVENTIONS.md](../design/API_CONVENTIONS.md) | REST naming, auth headers, pagination, async behavior, cache behavior |
| [ERROR_HANDLING.md](../design/ERROR_HANDLING.md) | RFC 9457 Problem Details and Duskcue error-code registry |
| [REAL_TIME_PUSH.md](../design/REAL_TIME_PUSH.md) | SSE event contract and mobile foreground behavior |
| [MOBILE_PUSH.md](../design/MOBILE_PUSH.md) | Push-device registration and provider-token lifecycle |

## Research Sources

- OpenAPI Specification: https://spec.openapis.org/oas/latest.html
- OpenAPI Generator generators: https://openapi-generator.tech/docs/generators/
- JSON Schema 2020-12: https://json-schema.org/draft/2020-12
- JSON Schema reference: https://json-schema.org/understanding-json-schema/reference
- RFC 9457 Problem Details: https://www.rfc-editor.org/rfc/rfc9457
- RFC 8216 HTTP Live Streaming: https://datatracker.ietf.org/doc/html/rfc8216
- Pact consumer testing guidance: https://docs.pact.io/consumer
- Ajv JSON Schema validation: https://ajv.js.org/
- Android Media3 player events: https://developer.android.com/media/media3/exoplayer/listening-to-player-events
- Android Media3 track selection: https://developer.android.com/media/media3/exoplayer/track-selection
- Apple AVFoundation media selection: https://developer.apple.com/documentation/avfoundation/selecting-subtitles-and-alternative-audio-tracks
- Apple HLS authoring: https://developer.apple.com/documentation/http-live-streaming/hls-authoring-specification-for-apple-devices
- W3C Media Session: https://www.w3.org/TR/mediasession/
- Dart JSON serialization: https://docs.flutter.dev/data-and-backend/serialization/json
- Dart json_serializable: https://pub.dev/packages/json_serializable
- Kotlin serialization: https://kotlinlang.org/docs/serialization.html
- Swift OpenAPI Generator: https://swift.org/blog/introducing-swift-openapi-generator/
