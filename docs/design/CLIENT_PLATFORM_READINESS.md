# Client Platform Readiness

## Purpose

This document is the authoritative Phase 16d research and design outcome for shared client contracts, conformance testing, diagnostics, accessibility, release readiness, and device-lab practices across desktop, mobile, TV, and console clients.

Phase 16d does not build another client. It creates the shared artifacts that Phases 17-23 must reuse so each platform does not redefine auth, playback, diagnostics, deep-link, signing, accessibility, or store-readiness behavior.

## Official Source Review

Reviewed July 1, 2026.

| Platform area | Official sources reviewed | Readiness impact |
|---|---|---|
| Android mobile and Android TV | Android Developers app signing, permissions, data safety, Media3 ExoPlayer, Android TV media app guidance, MediaSession/background playback | Android clients need Play/App Signing metadata, Data Safety/privacy declarations, least-privilege permissions, Media3 playback conformance, app links/deep links, foreground/background playback behavior, and Android TV focus/launcher validation. |
| iOS, macOS, and tvOS | Apple App Review Guidelines, Apple Human Interface Guidelines for privacy/accessibility/video/tvOS, tvOS developer pages, Flutter iOS release guidance | Apple clients need bundle IDs, signing/provisioning, privacy labels and purpose strings, accessibility/dynamic type/focus behavior, AVKit/tvOS playback expectations, TestFlight/App Store readiness, and clear local-network/privacy disclosures. |
| Windows desktop and Xbox | Microsoft Store Policies, app capability declarations, Windows media playback guidance, Xbox media app architecture and supported technologies | Windows/Xbox clients need package identity, declared capabilities, Store policy checks, media-control conformance, Xbox UWP media-app constraints, and explicit PlayReady/DRM non-goals for self-hosted Duskcue playback. |
| Flutter mobile | Flutter Android/iOS deployment and setup docs | Flutter remains the shared Android/iOS implementation surface; signing and store release remain platform-native responsibilities, and local SDK execution belongs in CI/release-gate environments. |
| Fire TV | Amazon Fire TV getting-started, Appstore submission, Watch Activity, Content Personalization, Vega deep-link/content-personalization docs | Fire TV clients need Appstore metadata, device testing, Android/Fire OS divergence notes, Watch Activity event mapping, and partner-gated Content Personalization/catalog handling where available. |
| Roku | Roku certification criteria, certification testing, deep linking, channel publishing, SceneGraph guidance, authenticated app testing | Roku clients need deep-link parameters for each media type, certification/pre-certification checks, static/app behavior analysis, app-local auth behavior, and Direct to Play validation. |
| Samsung Tizen | Samsung TV SDK install/download, quick start, application configuration, TV-device testing, accessibility guidance, Seller Office app information | Samsung clients need Tizen Studio/TV extension setup, certificate handling, config privileges, real-TV testing, accessibility behavior, app metadata/images, and AVPlay-specific conformance in the platform phase. |
| LG webOS | webOS TV app templates, `appinfo.json`, webOS CLI guidance, webOS Studio/simulator, app packaging/deploy docs | LG clients need `appinfo.json`, IPK packaging, webOS Studio/simulator and real-device deploy checks, launch/relaunch parameter handling, and media playback compatibility validation. |

## Research Findings

### Identity, Signing, and Store Metadata

Every target platform treats app identity and signing as a release gate, not an implementation detail. Android uses upload/app signing separation for Play distribution. Apple platforms require bundle IDs, signing identities, provisioning, App Store/TestFlight metadata, privacy labels, and purpose strings. Windows/Xbox require package identity and declared capabilities. Samsung, LG, Roku, and Fire TV each require platform-specific app metadata, icons/screenshots, package format, and submission checklist evidence.

**Decision:** Phase 16d must produce a release-readiness matrix with app IDs, package names, signing/certificate placeholders, privacy labels, capability declarations, store metadata, and rollback/update expectations per platform. Actual secrets and signing credentials stay out of the repo.

### Accessibility and Input

Accessibility is platform-specific enough that a single UI test cannot cover every surface. Desktop needs keyboard/focus order. Mobile needs screen reader, dynamic type, touch target, reduced motion, and captions behavior. TV/console clients need remote/controller focus order, visible focus, back/menu semantics, captions/subtitles, and non-pointer navigation. Roku, Samsung, Apple, Android TV, and Xbox all make remote/focus behavior part of certification-quality UX.

**Decision:** Phase 16d must define baseline accessibility and input conformance cases that every downstream platform phase must run or document as not applicable.

### Media Playback

The shared server playback contract is stable enough to test centrally: playback start/resume, signed HLS URL handling, heartbeat/stop/completion, selected audio/subtitle tracks, errors, QoE reports, and cross-device resume. Platform media stacks remain native: Media3/ExoPlayer on Android/Fire TV, AVKit/AVPlayer on Apple platforms, Roku SceneGraph video nodes, Samsung AVPlay, LG web media APIs, and Windows/Xbox MediaPlayer/MediaPlayerElement.

**Decision:** Phase 16d must define a platform-neutral playback state-machine conformance suite plus fixtures. Platform phases implement adapters against their native media stack.

### TV Surfaces and Deep Links

Phase 16b already established a server-owned TV surface feed and deep-link resolver. Official platform guidance reinforces that TV/launcher surfaces require stable content IDs, deep links that resolve directly to the appropriate playback or detail behavior, authenticated-user filtering, and platform-specific submission evidence. Roku and Fire TV explicitly tie certification/discovery behavior to deep-link and activity/feed correctness.

**Decision:** Phase 16d must promote the existing TV fixtures into a shared conformance pack covering feed ordering, ETags/private cache behavior, stable `platform_content_id` values, denial paths, and platform adapter mappings.

### Diagnostics and Privacy

Support bundles are useful only if they can be collected without leaking tokens, signed URLs, private filesystem paths, push tokens, or raw watch history. The server already has request IDs, playback sessions, notification IDs, and download job/package IDs that clients can cite without exposing secrets.

**Decision:** Phase 16d must define a common client log schema and diagnostics bundle format with privacy classifications and redaction rules. Exported bundles are advisory for early platform work but mandatory before release claims.

### Contract Source of Truth

Phase 16a created a curated checked-in contract manifest at `docs/api/client-contracts.v1.json`, verified by `scripts/verify-client-contracts.mjs`. That remains the only working route source today because the Rust server does not emit OpenAPI or JSON Schema.

Options considered:

| Option | Pros | Cons | Phase 16d decision |
|---|---|---|---|
| Generate OpenAPI 3.1 from server code | Broad tooling and SDK generation | Requires schema derivation across existing local DTOs and error types | Target direction, not Task 0 assumption |
| Generate JSON Schema from shared Rust DTO crate | Strong fixture/schema validation | Current DTOs are domain-local, not centralized in `crates/types` | Future refactor input |
| Continue curated manifest plus fixture schemas | Works now, reviewable, can cover denial/cache/SSE behavior | Requires discipline and drift checks | Chosen Phase 16d starting point |
| Per-client handwritten contracts | Flexible per platform | High drift risk and repeated bugs | Not allowed as source of truth |

**Decision:** Phase 16d starts from the curated manifest and checked-in fixtures. Task 1 extends metadata for auth, validation, cache, errors, pagination, SSE, and offline-download routes. Task 2 may add generated bindings where practical, but generation must consume the curated manifest/fixtures until a server-emitted schema exists.

### SDK and Binding Strategy

OpenAPI Generator currently lists broad client generator coverage, including TypeScript, Dart, Kotlin, and Swift families. Dart's `json_serializable`, Kotlin serialization, and Swift OpenAPI Generator each provide practical typed-model or client-generation paths once Duskcue has a real OpenAPI 3.1 or JSON Schema source. JSON Schema also remains useful for validating canonical fixtures even on platforms where full client generation is not practical.

**Decision:** Phase 16d Task 2 adds `docs/api/client-binding-targets.v1.json` as the binding target matrix and `scripts/verify-client-bindings.mjs` as the drift gate. The current output is typed fixture contracts and shared adapter requirements, not generated SDK source. TypeScript/Tauri, Dart/Flutter, Kotlin Android/Fire TV, and Swift tvOS/iOS are marked generation-practical once server schemas exist. Roku, Samsung Tizen, LG webOS, Windows, and Xbox remain fixture-first or target-dependent until their platform phase selects tooling and packaging constraints.

All downstream clients must keep the following behind small adapters: base URL resolution, bearer-token injection, session refresh/re-auth handling, timeout/retry policy, RFC 9457 Problem Details mapping, pagination helpers, private cache/ETag storage, SSE event decoding, secure storage, and diagnostics redaction. Platform-specific keychain, keystore, credential locker, app-private storage, and networking APIs must not leak into shared DTO or fixture logic.

## Mandatory Gates for Phases 17-23

The following outputs are mandatory before a downstream platform phase can claim implementation complete:

- Client contract fixture coverage for auth, libraries/media, playback, subtitles/audio, artwork, notifications/SSE where applicable, TV feed, deep-link resolve, and denial/error cases.
- Playback conformance cases for start, resume, heartbeat, pause/seek, stop, completion, failure reporting, subtitle/audio selection, and cross-device resume refresh.
- Auth/session conformance for device linking or equivalent sign-in, logout, logout-all/session revoke handling, expired session behavior, and BOLA denial.
- TV surface/deep-link conformance for platform-owned launcher/search/top-shelf/watch-next surfaces where applicable.
- Accessibility/input checklist for the target device family.
- Diagnostics bundle redaction checklist and request/playback/session correlation fields.
- Release/store readiness checklist for app identity, package ID, signing placeholder, metadata, privacy labels/disclosures, permissions/capabilities, and platform smoke tests.
- Device-lab entry for at least one representative target device or simulator/emulator plus documented hardware gaps.

## Advisory Outputs

These are recommended but not release-blocking for the first platform implementation unless the platform requires them:

- Fully generated SDKs for every language.
- Partner-gated catalog ingestion or discovery feeds.
- Hardware test automation for every model-year variant.
- Store submission automation with real signing/notarization credentials.
- Advanced diagnostics upload workflows beyond local export.
- Offline-download conformance for TV/desktop/web clients, because Phase 16c is mobile-first.

## Phase 16d Task Routing

| Task | Primary artifact |
|---|---|
| 1. Shared client contract source of truth | Expanded `docs/api/client-contracts.v1.json`, schemas/fixtures, verifier updates |
| 2. SDK/generated bindings strategy | `docs/api/client-binding-targets.v1.json`, shared adapter contracts, binding verifier |
| 3. Contract test fixtures | Versioned fixtures under `docs/api/fixtures/` |
| 4. Playback conformance | Playback state-machine fixtures and expected event/QoE payloads |
| 5. Auth/session conformance | Auth/session fixtures and negative cases |
| 6. TV/deep-link conformance | TV fixture pack and adapter mapping expectations |
| 7. Accessibility/input baselines | Accessibility/input checklist and test cases |
| 8. Shared design assets/tokens | Shared assets, visual tokens, artwork/fallback rules |
| 9. Diagnostics/logging bundles | Log schema, bundle manifest, redaction rules |
| 10. Device lab matrix | Device/OS/media capability matrix and manual smoke scripts |
| 11. Release/store readiness | Per-platform release checklist and CI placeholders |
| 12. Client CI/smoke harness | Docker-backed seeded smoke harness and CI jobs |

## Open Follow-Ups

- Decide in Task 1 whether to add JSON Schema files beside the manifest or embed schema snippets in the manifest first.
- Decide in Task 12 whether the seeded Docker smoke harness owns its own fixture database or reuses migration/test seed scripts from existing server verification.
