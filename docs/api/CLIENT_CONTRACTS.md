# Client Contracts

## Purpose

This document defines the Phase 16a desktop/mobile client contract strategy and its Phase 16d promotion into shared client contracts for desktop, mobile, TV, and console platforms. It supports the task list in [BUILD_ORDER.md](../../BUILD_ORDER.md).

The machine-readable starting point is [client-contracts.v1.json](client-contracts.v1.json).
The Phase 16d binding target matrix is [client-binding-targets.v1.json](client-binding-targets.v1.json).
The Phase 16d versioned fixture pack starts at [fixtures/client/v1/manifest.json](fixtures/client/v1/manifest.json).
The Phase 16d playback conformance pack starts at [fixtures/playback/v1/manifest.json](fixtures/playback/v1/manifest.json).
The Phase 16d auth/session conformance pack starts at [fixtures/auth/v1/manifest.json](fixtures/auth/v1/manifest.json).
The Phase 16d TV/deep-link conformance pack starts at [fixtures/tv/v1/manifest.json](fixtures/tv/v1/manifest.json).
The Phase 16d accessibility/input baseline pack starts at [fixtures/accessibility/v1/manifest.json](fixtures/accessibility/v1/manifest.json).
The Phase 16d design asset/token pack starts at [fixtures/design/v1/manifest.json](fixtures/design/v1/manifest.json).
The Phase 16d diagnostics/support-bundle pack starts at [fixtures/diagnostics/v1/manifest.json](fixtures/diagnostics/v1/manifest.json).
The Phase 16d device lab and compatibility pack starts at [fixtures/device-lab/v1/manifest.json](fixtures/device-lab/v1/manifest.json).
The Phase 16d release/store-readiness pack starts at [fixtures/release/v1/manifest.json](fixtures/release/v1/manifest.json).
The Phase 16d client CI/smoke harness pack starts at [fixtures/client-ci/v1/manifest.json](fixtures/client-ci/v1/manifest.json).

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

The household-profile contract uses the same non-secret `device_id` for an opt-in remembered profile. Auth responses and `GET /api/v1/profiles` expose `profile_selection_required`; `GET /api/v1/profiles` also returns `remembered_profile_id` and `device_can_remember_profile`; `POST /api/v1/profiles/{id}/switch` accepts optional `remember_on_device` and clears the current session's selection-required state. Clients must treat this as a convenience preference only: they may store a random opaque per-installation device ID, but never a session token, password, parent PIN, hardware identifier, advertising identifier, or standalone profile-login credential.

Each `ProfileResponse` reports only `parent_pin_configured`, never its hash, attempt state, or secret. `GET /api/v1/profiles` and switch responses expose `parent_unlock_required` plus a current-session `parent_unlock_expires_at` only when it is valid for the active Kids profile. `POST /api/v1/profiles/parent-unlock` accepts a transient `{ "pin": "…" }` body for the active Kids profile and returns `{ "unlocked_until": "RFC3339 UTC" }`; clients must hold the entered PIN only long enough to submit it and must never place it in storage, logs, diagnostics, a URL, or same-origin profile-change signal. Leaving a PIN-protected Kids profile for a standard profile without a current matching unlock fails with `PROFILE_012`; invalid PIN is `PROFILE_010`, and durable throttling is `PROFILE_011` (`429` without a precise retry schedule).

### Ambient Channel Conformance

`POST /api/v1/ambient-channels/{id}/next` returns `{ channel_id, channel_name, media_item_id, playback_mode: "ambient", channel_updated_at }`. `channel_updated_at` is an RFC3339 UTC queue/configuration revision, not a client clock. A client must echo the exact value as `ambient_channel_updated_at` with `playback_mode: "ambient"` and `ambient_channel_id` in `POST /api/v1/playback/start`. The server rejects an incomplete ambient start as invalid and a stale revision as `409 PLAY_019`; it creates neither a playback session nor a stream URL in that case. Interactive playback must not send ambient fields.

For restoration, native players may retain only the channel ID, selected media ID, returned revision, position, and transient player state. They must never persist a stream URL, bearer/signed token, transcode URL, parent PIN, or parent-unlock expiry. Process/service restoration, a profile change, session loss, and `PLAY_019` require a fresh `next` resolution before a start. Ambient heartbeats, seek, and stop remain required for operations, but do not produce history, resume, play count, TV rows, recommendations, or Trakt activity.

The Flutter ambient bridge treats the native player as the only queue owner. It passes server origin and bearer authorization only to the in-memory native runtime, which invokes `next`, ambient start, heartbeat, stop, and post-completion advancement. Android uses one Media3 `MediaSessionService`; iOS uses one `AVQueuePlayer` plus an active playback audio session. Flutter may attach a platform view to that player while the channel screen is visible, but it does not start a second `video_player` controller for the same session. Sign-out, profile switch, app process/service loss, or explicit stop releases the native player and clears all runtime values; platform playback resumption is intentionally disabled because neither a credential nor a reusable stream URL may be persisted.

After account authentication, the server activates a valid remembered profile or falls back to the account default profile. A new multi-profile session without a valid mapping is explicitly marked `profile_selection_required`; shared-TV clients must show a picker before requesting, rendering, or publishing profile-scoped media. The selected profile is remembered only after an explicit opt-in; `remember_on_device: false` removes the mapping. On every profile change, clients must abort profile-scoped requests and clear profile-scoped caches, previews/object URLs, artwork, queue state, launcher mappings, and user summaries before rendering the replacement profile. Browser clients must propagate only a same-origin minimal profile-change signal to other tabs, which revalidate their own session before rendering.

The Flutter Android/iOS client must route both fresh authentication and token restoration through a server-backed profile gate before exposing authenticated routes. It fetches `GET /api/v1/profiles`; `profile_selection_required` keeps the picker blocking, while a resolved profile releases navigation. It must clear the transient PIN controller on cancel, failure, success, profile change, session loss, and logout. Its local download inventory, package root, and settings scope include `profile_id` in addition to server origin, user ID, and device ID; image-memory/disk and route state are cleared after a successful profile switch and before the new scope renders. This is the first native implementation of the shared contract, not a substitute for a future dedicated TV picker.

### Profile Selection Conformance

The auth fixture includes a first-use multi-profile session, the `GET /api/v1/profiles` selection signal, and a switch that clears it. It also includes a locked Kids profile, failed parent PIN, valid time-bounded parent unlock, and subsequent adult-profile switch. `scripts/verify-profile-selection-integration.mjs` binds the migration, session creation, transactional switch, API helpers, browser gate, cache cancellation, and same-origin synchronization into one regression check. `scripts/verify-profile-parent-unlock-integration.mjs` binds Argon2id configuration, migration state, durable throttling, unlock revocation, fixture handling, and the web prompt into a second regression check.

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

## Phase 16d Auth And Session Conformance

Phase 16d Task 5 adds a reusable auth/session conformance pack under `docs/api/fixtures/auth/v1`. This pack separates protocol and client-state behavior from general API examples so platform clients can prove the same secure lifecycle before implementing platform-specific login UI.

The pack covers:

- device-linking, passkey-capable login, fallback login, and re-auth flows;
- logout, logout-all, session deletion, expired-session handling, and `session_kicked` handling;
- secure-storage expectations for bearer tokens, signed media URLs, push tokens, download package manifests, server origins, user summaries, and device identifiers;
- switching servers, switching users, session revocation, account deletion, and local-network/TLS validation failure behavior;
- negative cases for BOLA-protected item access, stale TV platform IDs, stale deep links, expired re-auth codes, and denied device-linking flows.

Run:

```bash
node scripts/verify-auth-conformance.mjs
```

The verifier checks required auth flows, required session lifecycle cases, storage classifications, plaintext-storage prohibitions, switching behavior, Problem Details shape, API-relative request paths, UTC timestamps, stable UUIDs where applicable, and redaction of real tokens, signatures, and private paths.

### Device-Linking Hardening Contract

The device-code response uses a canonical `/auth/link` `verification_uri`, plus `verification_uri_complete` with `?code=` for QR/NFC handoff. Clients must continue to display the text URI and user code, and must not display the internal `device_code` after issuance. `POST /api/v1/device/token` returns `AUTH_023` while authorization is pending, `AUTH_024` with `Retry-After` when polled too quickly, `AUTH_014` after an explicit denial, and `AUTH_013` after expiry or one-time consumption.

An authenticated browser first uses `GET /api/v1/device/verify?user_code=…` to display only the pending device's non-secret metadata. It then sends `POST /api/v1/device/verify` with an explicit `approve` boolean. If sign-in is required, web clients preserve only a local `/auth/link?...` return target and return to the review step; they never auto-approve a prefilled code.

Run:

```bash
node scripts/verify-device-linking-integration.mjs
```

The verifier binds the Rust route/service/migration safeguards, web handoff, client helper, versioned contract, and auth fixture into one compatibility check.

## Phase 16d TV And Deep-Link Conformance

Phase 16d Task 6 promotes TV surface and deep-link behavior into a versioned conformance pack under `docs/api/fixtures/tv/v1`. This pack complements the earlier unversioned Phase 16b TV surface examples by adding a manifest, platform adapter mappings, and launch-time revalidation cases.

The pack covers:

- TV surface section order, limits, stable `platform_content_id` values, nullable movie/required-episode `series_id` values, private cache headers, ETags, access filtering, and empty states;
- deep-link resolve behavior for playable movies, playable episodes, revoked access, unavailable media, and unsupported platform hints;
- adapter mappings for Android TV Watch Next, Fire TV Watch Activity, Roku Search/Direct to Play, Samsung Smart Hub Preview, LG webOS launch parameters, tvOS Top Shelf/Universal Links, and Xbox URI activation;
- revalidation when launcher caches are stale, sessions are revoked, library access changes, users switch, or platform IDs are deleted/replaced.

Run:

```bash
node scripts/verify-tv-deeplink-conformance.mjs
```

The verifier checks the manifest coverage, section ordering, cache policy, stable platform IDs, episode-series identity, Android Watch Next one-episode/change-only mapping requirements, API-relative paths, Problem Details shape, adapter coverage, mandatory access revalidation, UTC timestamps, stable UUIDs where applicable, and redaction of tokens, signed URL parameters, and private paths.

## Phase 16d Accessibility And Input Baselines

Phase 16d Task 7 adds [CLIENT_ACCESSIBILITY_INPUT.md](../design/CLIENT_ACCESSIBILITY_INPUT.md) plus a reusable accessibility/input fixture pack under `docs/api/fixtures/accessibility/v1`. The pack gives downstream clients a common set of release-gate checks instead of leaving accessibility to each platform phase.

The pack covers:

- minimum baselines for desktop keyboard navigation, mobile screen readers, Dynamic Type/text scaling, mobile touch targets, TV focus navigation, remote/controller input, captions/subtitles, contrast/focus, reduced motion, and localization/RTL;
- focus-order cases for setup/sign-in, home-to-media-detail, search/filter, media-detail-to-playback, settings dialogs, and notification live regions;
- TV/console remote-navigation cases for row traversal, row boundaries, player controls, search keyboard return, modal back behavior, and surface-refresh focus restore;
- per-platform review checklists for web desktop, Tauri desktop, Android/iOS mobile, Android TV/Google TV, Fire TV, Roku, Samsung Tizen, LG webOS, Apple tvOS, and Xbox;
- localization/RTL cases for client catalogs, server-owned strings, layout mirroring, directional icons, locale-aware formatting, and activation gates.

Run:

```bash
node scripts/verify-accessibility-input.mjs
```

The verifier checks required platform families, baseline categories, focus cases, remote cases, platform reviews, localization cases, actionable evidence, captions coverage, and non-empty expectations.

## Phase 16d Design Assets And UI Tokens

Phase 16d Task 8 adds [CLIENT_DESIGN_ASSETS.md](../design/CLIENT_DESIGN_ASSETS.md) plus a reusable design asset/token fixture pack under `docs/api/fixtures/design/v1`. The pack keeps visual consistency machine-checkable without forcing all clients into one UI toolkit.

The pack covers:

- DTCG-compatible token groups for color, typography, spacing, radius, shadow, motion, focus, artwork, and media-state badge tones;
- source SVG assets for the app icon and poster/backdrop/thumbnail/logo placeholders;
- app icon derivation rules for Android adaptive/themed icons, Apple app icons, desktop icons, TV banners, and store assets;
- poster, backdrop, thumbnail, and logo aspect-ratio and size rules;
- authenticated artwork URLs, signed URL secrecy, ETag/revision cache busting, fallback placeholders, offline package artwork, and unavailable/revoked states;
- string ownership across server-rendered media/problem/notification text, client catalogs, and shared message-key reuse;
- media-state badges for playable, downloading, offline-ready, unavailable, missing-file, metadata-incomplete, access-revoked, expired, transcode-unavailable, syncing, live, and upcoming states.

Run:

```bash
node scripts/verify-design-assets.mjs
```

The verifier checks required token groups, hex color values, CSS mappings, source asset paths, required artwork rules, string ownership sections, platform mappings, badge states, label keys, and color-alone accessibility requirements.

## Phase 16d Diagnostics And Support Bundles

Phase 16d Task 9 adds [CLIENT_DIAGNOSTICS.md](../design/CLIENT_DIAGNOSTICS.md) plus a reusable diagnostics fixture pack under `docs/api/fixtures/diagnostics/v1`. The pack defines how platform clients should log and export troubleshooting evidence without leaking secrets or private media context.

The pack covers:

- required client log fields: timestamp, client version, platform, route/screen, request ID, event type, severity, and privacy classification;
- privacy classes for public, operational, user-private, secret, and consent-required data;
- support bundle sections for app logs, device capability report, redacted server URL, playback failure summaries, network state, and recent request IDs;
- forbidden data and redaction transforms for bearer/session tokens, passwords, signed media URLs, private paths, push tokens, raw watch history, and filenames;
- server-side correlation fields for `x-request-id`, Problem Details `trace_id`, playback sessions, notifications, downloads, packages, and TV surface events;
- platform export checklists for web, Tauri, Flutter mobile, Android TV, Fire TV, Roku, Tizen, webOS, tvOS, Windows, and Xbox.

Run:

```bash
node scripts/verify-client-diagnostics.mjs
```

The verifier checks log schema coverage, bundle section coverage, redaction rules, privacy classes, correlation IDs, platform checklists, and absence of fixture leak patterns outside the redaction policy fixture.

## Phase 16d Device Lab And Compatibility Matrix

Phase 16d Task 10 adds [CLIENT_DEVICE_LAB.md](../design/CLIENT_DEVICE_LAB.md) plus a reusable device lab fixture pack under `docs/api/fixtures/device-lab/v1`. The pack defines the compatibility evidence downstream platform phases need before claiming release readiness.

The pack covers:

- required platform IDs for Android mobile, iOS mobile, Windows, macOS, Linux, Android TV/Google TV, Fire TV, Roku, Samsung Tizen, LG webOS, Apple tvOS, and Xbox;
- minimum and representative devices, including release-required and best-effort hardware;
- OS/runtime, browser/webview engine, codec, HLS, HDR, audio, subtitle, remote/input, storage, and known-limitation fields;
- manual smoke scripts against the Docker deployment on `http://<server>:48027`;
- release validation rules for simulator/emulator evidence versus physical device evidence;
- known platform limitations and Phase 16d hardware-gap tracking.

Run:

```bash
node scripts/verify-device-lab.mjs
```

The verifier checks required platform coverage, required capability fields, Docker port `48027`, smoke-step coverage, release-required/best-effort classifications, hardware-gap coverage, and fixture leak patterns.

## Phase 16d Release And Store Readiness

Phase 16d Task 11 adds [CLIENT_RELEASE_READINESS.md](../design/CLIENT_RELEASE_READINESS.md) plus a reusable release fixture pack under `docs/api/fixtures/release/v1`. The pack gives downstream platform phases a shared release checklist instead of leaving signing, store metadata, privacy labels, CI artifacts, and rollback posture to each client implementation.

The pack covers:

- app IDs, package names, bundle IDs, display names, store/distribution channels, signing identity placeholders, certificate/key placeholders, provisioning/profile placeholders, notarization or store-signing expectations, permission/capability declarations, privacy disclosures, age/content ratings, and review notes for all required Phase 16d platforms;
- CI release placeholders for named artifacts, build commands, signing hooks, notarization/store-processing hooks, SBOM files, provenance/attestation outputs, and release-channel defaults;
- versioning rules across server, web, desktop, Android, Apple, and TV clients;
- local, internal, beta, and stable release-channel mapping;
- release-blocking smoke tests against the Docker `:48027` deployment and per-platform rollback/update expectations;
- privacy, permission, and review-note expectations for Duskcue's self-hosted/no-bundled-catalog posture.

Run:

```bash
node scripts/verify-release-readiness.mjs
```

The verifier checks required platform coverage, identity/signing fields, certificate placeholder handling, CI artifact/SBOM/provenance fields, versioning targets, release-channel mapping, Docker smoke steps, rollback/update expectations, privacy/review-note coverage, and fixture leak patterns.

## Phase 16d Client CI And Smoke Harness

Phase 16d Task 12 adds [CLIENT_CI_SMOKE_HARNESS.md](../ci/CLIENT_CI_SMOKE_HARNESS.md), a reusable client CI fixture pack under `docs/api/fixtures/client-ci/v1`, the executable `scripts/client-smoke-harness.mjs`, the drift gate `scripts/verify-client-ci-smoke.mjs`, and `.github/workflows/client-ci-smoke.yml`.

The pack covers:

- the Docker `:48027` public-surface target, readiness and liveness probes, SSE non-5xx probe, deterministic representative seed media, and the required harness steps;
- always-on PR/main CI jobs for shared contract validation, fixture drift, binding-generation readiness, TV/console fixture smoke, and harness-plan validation;
- manual workflow-dispatch jobs for the full Docker smoke run and heavier desktop/mobile platform smoke checks;
- downstream Phase 17-23 consumption requirements so platform phases reuse the same baseline before declaring verification complete;
- manual hardware/release-gate boundaries for checks that hosted CI cannot run truthfully.

Run:

```bash
node scripts/client-smoke-harness.mjs --plan
node scripts/verify-client-ci-smoke.mjs
```

Use `node scripts/client-smoke-harness.mjs --run` only when Docker is available and a maintainer wants release-gate evidence against the public `:48027` deployment.

The verifier checks fixture coverage, required CI jobs, required contract/conformance verifier commands, downstream phase consumption, manual hardware gate coverage, seed-data redaction rules, workflow wiring, and harness script drift.

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
- FIDO passkeys: https://fidoalliance.org/passkeys/
- W3C WebAuthn Level 3: https://www.w3.org/TR/webauthn-3/
- OWASP API1 Broken Object Level Authorization: https://owasp.org/API-Security/editions/2023/en/0xa1-broken-object-level-authorization/
- OWASP API2 Broken Authentication: https://owasp.org/API-Security/editions/2023/en/0xa2-broken-authentication/
- OWASP Session Management Cheat Sheet: https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html
- Android TV Watch Next: https://developer.android.com/training/tv/discovery/watch-next-add-programs
- Fire TV featured content deep links: https://developer.amazon.com/docs/fire-tv/deep-linking-featured-content.html
- Fire TV Content Personalization: https://developer.amazon.com/docs/fire-tv/introduction-content-personalization.html
- Roku deep linking: https://developer.roku.com/dev/docs/implementing-deep-linking
- Roku Direct to Play: https://developer.roku.com/dev/docs/direct-to-play
- Samsung Smart Hub Preview: https://developer.samsung.com/smarttv/develop/guides/smart-hub-preview/smart-hub-preview.html
- LG webOS app lifecycle: https://webostv.developer.lge.com/develop/guides/app-lifecycle-management
- Apple Top Shelf: https://developer.apple.com/design/human-interface-guidelines/top-shelf
- Microsoft URI activation: https://learn.microsoft.com/en-us/windows/apps/develop/launch/handle-uri-activation
- WCAG 2.2: https://www.w3.org/TR/WCAG22/
- WCAG 2.2 Quick Reference: https://www.w3.org/WAI/WCAG22/quickref/
- Android accessibility testing: https://developer.android.com/guide/topics/ui/accessibility/testing
- Android TV navigation: https://developer.android.com/training/tv/get-started/navigation
- Android TV focus system: https://developer.android.com/design/ui/tv/guides/styles/focus-system
- Apple accessibility HIG: https://developer.apple.com/design/human-interface-guidelines/accessibility
- Apple VoiceOver HIG: https://developer.apple.com/design/human-interface-guidelines/voiceover
- Roku certification criteria: https://developer.roku.com/dev/docs/certification
- Roku text to speech: https://developer.roku.com/dev/docs/text-to-speech
- Samsung TV accessibility guide: https://developer.samsung.com/smarttv/develop/guides/fundamentals/accessibility.html
- Microsoft Narrator: https://support.microsoft.com/en-us/accessibility/windows/narrator/complete-guide-to-narrator
- Xbox Accessibility Guideline 106: https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/106
- W3C Design Tokens Community Group: https://www.w3.org/community/design-tokens/
- Design Tokens Format Module: https://www.designtokens.org/tr/drafts/format/
- Material Design 3 design tokens: https://m3.material.io/foundations/design-tokens
- Material Design 3 typography: https://m3.material.io/styles/typography/overview
- Material Design 3 spacing tokens: https://m3.material.io/styles/spacing/tokens
- Material Design 3 states: https://m3.material.io/foundations/interaction/states
- Apple app icons: https://developer.apple.com/design/human-interface-guidelines/app-icons
- Apple SF Symbols: https://developer.apple.com/sf-symbols/
- Apple focus and selection: https://developer.apple.com/design/human-interface-guidelines/focus-and-selection
- Apple designing for tvOS: https://developer.apple.com/design/human-interface-guidelines/designing-for-tvos
- Android adaptive icons: https://developer.android.com/develop/ui/compose/system/icon_design_adaptive
- Android launcher icon codelab: https://codelabs.developers.google.com/design-android-launcher
- Google Play icon design specifications: https://developer.android.com/distribute/google-play/resources/icon-design-specifications
- WCAG non-text contrast: https://www.w3.org/WAI/WCAG21/Understanding/non-text-contrast.html
- WCAG focus appearance: https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance.html
- OpenTelemetry Logs Data Model: https://opentelemetry.io/docs/specs/otel/logs/data-model/
- OWASP Logging Cheat Sheet: https://cheatsheetseries.owasp.org/cheatsheets/Logging_Cheat_Sheet.html
- OWASP Secrets Management Cheat Sheet: https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html
- Android declare your app's data use: https://developer.android.com/privacy-and-security/declare-data-use
- Google Play Data safety section: https://support.google.com/googleplay/android-developer/answer/10787469
- Apple App Privacy Details: https://developer.apple.com/app-store/app-privacy-details/
- Apple User Privacy and Data Use: https://developer.apple.com/app-store/user-privacy-and-data-use/
- Microsoft Privacy Statement: https://www.microsoft.com/en-us/privacy/privacystatement
- Windows privacy compliance guide: https://learn.microsoft.com/en-us/windows/privacy/windows-privacy-compliance-guide
- Roku certification criteria: https://developer.roku.com/dev/docs/certification
- Dart JSON serialization: https://docs.flutter.dev/data-and-backend/serialization/json
- Dart json_serializable: https://pub.dev/packages/json_serializable
- Kotlin serialization: https://kotlinlang.org/docs/serialization.html
- Swift OpenAPI Generator: https://swift.org/blog/introducing-swift-openapi-generator/
