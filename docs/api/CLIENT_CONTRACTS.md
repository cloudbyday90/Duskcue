# Client Contracts

## Purpose

This document defines the Phase 16a desktop/mobile client contract strategy. It is scoped to the online desktop and mobile MVP and supports the task list in [BUILD_ORDER.md](../../BUILD_ORDER.md).

The machine-readable starting point is [client-contracts.v1.json](client-contracts.v1.json).

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

Phase 16d extends this into response fixtures, generated bindings, and CI conformance tests.

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
- JSON Schema 2020-12: https://json-schema.org/draft/2020-12
- RFC 9457 Problem Details: https://www.rfc-editor.org/rfc/rfc9457
- Dart JSON serialization: https://docs.flutter.dev/data-and-backend/serialization/json
