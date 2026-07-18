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

### Contract Fixture Pack

JSON Schema 2020-12 remains the target vocabulary for machine validation once server schemas exist, and OpenAPI components/examples remain the target packaging format for generated clients. Until then, Phase 16d uses a curated fixture pack with explicit verifier rules. Pact's consumer-contract guidance reinforces that fixtures should be shaped around real client expectations, not only provider implementation detail; Duskcue therefore includes success, empty, and denial fixtures for the client flows that downstream platform phases must implement.

**Decision:** Phase 16d Task 3 adds `docs/api/fixtures/client/v1/manifest.json` plus focused JSON fixtures for server selection, auth/device-linking, preferences, libraries, media detail, search, collections, playback, subtitles/audio/storyboards/artwork, quality, downloads, notifications/settings, TV surface/deep-link behavior, and common denial cases. `scripts/verify-client-fixtures.mjs` is the drift gate for coverage, stable IDs, row ordering, UTC date-times, enum values, Problem Details shape, localized string ownership, and secret/path redaction.

### Playback Conformance

Android Media3 exposes player events and track-selection APIs, Apple AVFoundation exposes media selection for subtitles and alternative audio tracks, HLS/WebVTT timing rules matter for subtitle synchronization, and web/desktop surfaces can receive remote actions through the W3C Media Session API. These platform APIs differ, but the Duskcue-facing lifecycle must be identical: the server owns start/resume, heartbeat, seek, stop/completion, selected track intent, stream decision, cross-device resume, and QoE/error reporting.

**Decision:** Phase 16d Task 4 adds `docs/api/fixtures/playback/v1/manifest.json` plus state-machine, track-selection, stream-path, media-session, QoE, cross-device resume, and error-reporting fixtures. `scripts/verify-playback-conformance.mjs` verifies required transitions, ordering, API paths, direct/direct-stream/HLS coverage, remote actions, QoE fields, resume refresh behavior, Problem Details shape, and redaction. Platform phases must map their native player callbacks into this pack before claiming playback conformance.

### Auth And Session Conformance

FIDO passkeys and WebAuthn keep passkey ceremonies bound to the selected relying-party/server origin, while OWASP API Security guidance keeps broken object-level authorization and broken authentication as core client/server contract risks. The OWASP Session Management guidance also reinforces that clients must handle expiry, revocation, logout, and secret storage consistently instead of treating session state as only a UI concern.

**Decision:** Phase 16d Task 5 adds `docs/api/fixtures/auth/v1/manifest.json` plus auth-flow, session-lifecycle, secure-storage, server/user-switching, and negative-case fixtures. `scripts/verify-auth-conformance.mjs` verifies required login and re-auth flows, logout/session revocation behavior, `session_kicked`, storage classifications, plaintext secret prohibitions, switching behavior, BOLA/stale-ID denial cases, Problem Details shape, stable IDs where applicable, UTC timestamps, and redaction. Platform phases must pass this pack before claiming sign-in/session conformance.

Household profiles add a device-scoped post-auth preference contract: a client with a random opaque per-installation device ID may opt in to a remembered profile, while first-use shared or incapable devices receive `profile_selection_required` and show profile selection before fetching or publishing profile-scoped rows. The device ID is non-secret and may be persisted only through platform-appropriate app storage; it must not be a hardware, advertising, or household identifier, and a client must never turn the preference into a profile credential or cache session/PIN material with it. A successful switch clears the flag for that session. Explicit sign-out and remote revocation clear the preference. Profile changes clear profile-scoped client caches and platform rows before the replacement profile renders.

For a PIN-protected active Kids profile, `parent_unlock_required` means the client must collect a transient 4–12 digit PIN and call `POST /api/v1/profiles/parent-unlock` before attempting a standard-profile switch. The server owns the ten-minute expiry and durable five-failure/15-minute throttle; a client must not implement a local unlock timer or retry loop, persist the PIN or expiry as authority, expose the PIN through accessibility text or diagnostics, or infer a retry time. Every client clears parent-unlock UI state on profile change, session loss, or app logout. Native TV clients must show the same parent-access boundary before they claim Kids-mode support.

Ambient channels add a stale-selection boundary for native background players. `next` returns a server-issued `channel_updated_at`; an ambient start must echo it with the channel ID and is rejected with `PLAY_019` if the channel changed before start. Android keeps the ambient player and one `MediaSession` in `MediaSessionService`; Apple uses `AVQueuePlayer` with its media-playback/background lifecycle. Either platform may restore only non-secret queue identity and position, then must re-resolve the channel through Duskcue after service restart, profile change, session loss, or stale-revision conflict. Stream URLs and authorization material are never restoration state.

### TV Surface And Deep-Link Conformance

Android TV Watch Next, Fire TV featured-content/deep-link behavior, Roku deep linking and Direct to Play, Samsung Smart Hub Preview, LG webOS launch/relaunch parameters, tvOS Top Shelf/Universal Links, and Windows/Xbox URI activation all make launch-time content identity a contract boundary. The shared Duskcue requirement is that platform-owned surfaces never become sources of truth: clients publish stable Duskcue IDs, then resolve and revalidate against the server before playback.

**Decision:** Phase 16d Task 6 adds `docs/api/fixtures/tv/v1/manifest.json` plus surface-contract, deep-link resolve, platform-adapter mapping, and access-revalidation fixtures. `scripts/verify-tv-deeplink-conformance.mjs` verifies section order, limits, private cache and ETags, stable `platform_content_id` values, playable and denial resolve cases, adapter mappings for Phases 17-23, launch-time access revalidation, Problem Details shape, UTC timestamps, stable IDs where applicable, and redaction. Platform phases must pass this pack before claiming TV surface or platform deep-link conformance.

### Accessibility And Input Baselines

WCAG 2.2 provides the common baseline for focus order, keyboard access, contrast, target size, captions, visible focus, and reduced motion. Android, Apple, Roku, Samsung, Microsoft, and Xbox guidance then specialize that baseline into native screen readers, Dynamic Type/text scaling, TalkBack/VoiceOver/Narrator behavior, D-pad and remote/controller focus, caption setting expectations, and platform-specific review evidence.

**Decision:** Phase 16d Task 7 adds [CLIENT_ACCESSIBILITY_INPUT.md](CLIENT_ACCESSIBILITY_INPUT.md), `docs/api/fixtures/accessibility/v1/manifest.json`, and `scripts/verify-accessibility-input.mjs`. The pack covers desktop keyboard navigation, mobile screen readers and dynamic type, touch targets, TV focus navigation, remote/controller input, captions/subtitles, contrast/focus, reduced motion, focus-order cases, remote-navigation cases, per-platform accessibility review checklists, and localization/RTL activation cases. Platform phases must pass this pack or explicitly document non-applicable platform capabilities before claiming accessibility/input readiness.

### Shared Design Assets And UI Tokens

W3C Design Tokens Community Group work and the Design Tokens Format Module provide a practical exchange shape for named design decisions. Material Design 3 reinforces semantic tokens for color, type, spacing, shape, and state. Apple and Android guidance make app icons platform-specific release assets, so Duskcue needs shared source artwork rather than one exported icon file. WCAG focus and non-text contrast guidance keeps focus rings and badges from becoming decorative-only cues.

**Decision:** Phase 16d Task 8 adds [CLIENT_DESIGN_ASSETS.md](CLIENT_DESIGN_ASSETS.md), `docs/api/fixtures/design/v1/manifest.json`, source SVGs under `docs/branding/assets`, and `scripts/verify-design-assets.mjs`. The pack defines shared token groups, app-icon and placeholder sources, poster/backdrop/thumbnail/logo sizing, authenticated and signed artwork loading rules, fallback/offline/unavailable behavior, string ownership, media-state badges, and per-platform mapping guidance. Platform phases must consume these assets and rules while mapping them into native UI systems instead of sharing one toolkit abstraction.

### Diagnostics, Logging, And Support Bundles

OpenTelemetry's log data model provides a useful structure for timestamped records with severity, body, attributes, resource metadata, and trace correlation. OWASP logging guidance reinforces that logs must be useful for investigation while excluding credentials and sensitive personal data. Apple, Google Play, Microsoft, and TV platform guidance make diagnostics a privacy and release-disclosure concern, not just an engineering convenience.

**Decision:** Phase 16d Task 9 adds [CLIENT_DIAGNOSTICS.md](CLIENT_DIAGNOSTICS.md), `docs/api/fixtures/diagnostics/v1/manifest.json`, and `scripts/verify-client-diagnostics.mjs`. The pack defines required client log fields, privacy classifications, support bundle sections, forbidden data/redaction transforms, server-side correlation IDs, and platform export checklists. Platform phases must include request IDs and bounded domain IDs instead of raw request bodies, tokens, signed URLs, private paths, or raw watch history.

### Device Lab And Compatibility Matrix

Official platform guidance consistently distinguishes emulator/simulator usefulness from physical-device media and release evidence. Android and Media3 document format support as a combination of stream container and device codecs. Apple HLS/AVFoundation behavior depends on hardware, display chain, and AVPlayer/AVKit capabilities. Fire TV, Roku, Samsung Tizen, LG webOS, and Xbox all require device or certification-style testing before public platform claims.

**Decision:** Phase 16d Task 10 adds [CLIENT_DEVICE_LAB.md](CLIENT_DEVICE_LAB.md), `docs/api/fixtures/device-lab/v1/manifest.json`, and `scripts/verify-device-lab.mjs`. The pack defines required platform IDs, minimum and representative devices, OS/runtime tracking, media capability categories, HLS/HDR/audio/subtitle expectations, remote/input behavior, storage constraints, known limitations, Docker `:48027` smoke scripts, release-required hardware, best-effort hardware, and allowed Phase 16d hardware gaps. Platform phases must close or explicitly defer their hardware gaps before claiming release readiness.

### Release And Store Readiness

Official release guidance makes signing, app identity, store metadata, privacy disclosures, age/content ratings, review notes, versioning, and rollout/rollback behavior platform-specific release gates. Android requires app-signing and monotonically increasing version codes. Apple platforms require bundle IDs, distribution signing, provisioning profiles, App Store privacy details, unique build strings, and notarization for direct macOS distribution. Microsoft, Amazon, Roku, Samsung, and LG each require their own package identity, certification/review checklist, privacy/content metadata, and platform-specific release evidence.

**Decision:** Phase 16d Task 11 adds [CLIENT_RELEASE_READINESS.md](CLIENT_RELEASE_READINESS.md), `docs/api/fixtures/release/v1/manifest.json`, and `scripts/verify-release-readiness.mjs`. The pack defines per-platform app IDs/package names/bundle IDs, signing and certificate placeholders, provisioning/notarization/store-signing requirements, privacy labels/disclosures, permission/capability declarations, age/content rating expectations, review notes, CI artifact/signing/SBOM/provenance placeholders, release channel naming, versioning rules, release-blocking smoke tests, and rollback/update expectations. Platform phases must fill the placeholders with real secure-store/CI references and store evidence before beta or stable release claims.

## Client CI And Smoke Harness

GitHub Actions supports path-filtered PR and mainline workflows, manual `workflow_dispatch` inputs, job dependencies, conditional jobs, and least-privilege workflow permissions. Docker Compose supports detached deployment and healthcheck readiness patterns. Flutter and Tauri both document CI/build smoke paths, while GitHub artifact-attestation and SBOM guidance belongs to release artifact jobs with elevated permissions rather than every PR smoke check.

**Decision:** Phase 16d Task 12 adds [CLIENT_CI_SMOKE_HARNESS.md](../ci/CLIENT_CI_SMOKE_HARNESS.md), `.github/workflows/client-ci-smoke.yml`, `docs/api/fixtures/client-ci/v1/manifest.json`, `scripts/client-smoke-harness.mjs`, and `scripts/verify-client-ci-smoke.mjs`. Pull requests run deterministic contract, fixture, binding-readiness, TV/console fixture, and harness-plan checks. Maintainers can manually run the real Docker `:48027` smoke harness, which seeds representative media, starts `docker compose`, waits for readiness, probes the public surface, runs the Phase 16d conformance verifiers, and tears the deployment down. Platform build smoke lanes stay opt-in where they duplicate heavier packaging workflows, and hardware-only checks remain manual/release-gate evidence.

## Mandatory Gates for Phases 17-23

The following outputs are mandatory before a downstream platform phase can claim implementation complete:

- Client contract fixture coverage for auth, libraries/media, playback, subtitles/audio, artwork, notifications/SSE where applicable, TV feed, deep-link resolve, and denial/error cases.
- Playback conformance cases for start, resume, heartbeat, pause/seek, stop, completion, failure reporting, subtitle/audio selection, and cross-device resume refresh.
- Auth/session conformance for device linking or equivalent sign-in, logout, logout-all/session revoke handling, expired session behavior, and BOLA denial.
- TV surface/deep-link conformance for platform-owned launcher/search/top-shelf/watch-next surfaces where applicable.
- Accessibility/input checklist for the target device family.
- Shared design asset/token pack for icon sources, placeholder artwork, artwork loading behavior, string ownership, and media-state badges.
- Diagnostics bundle redaction checklist and request/playback/session correlation fields.
- Release/store readiness checklist from [CLIENT_RELEASE_READINESS.md](CLIENT_RELEASE_READINESS.md) for app identity, package ID, signing placeholder, metadata, privacy labels/disclosures, permissions/capabilities, platform smoke tests, CI artifact/SBOM/provenance placeholders, versioning rules, and rollback/update expectations.
- Device-lab entry from [CLIENT_DEVICE_LAB.md](CLIENT_DEVICE_LAB.md) for at least one representative target device or simulator/emulator, with release-required hardware, best-effort hardware, and documented hardware gaps.
- Client CI/smoke harness baseline from [CLIENT_CI_SMOKE_HARNESS.md](../ci/CLIENT_CI_SMOKE_HARNESS.md), including `node scripts/client-smoke-harness.mjs --plan`, `node scripts/verify-client-ci-smoke.mjs`, and the relevant Phase 16d contract/fixture verifiers before platform-specific verification is declared complete.

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
| 3. Contract test fixtures | `docs/api/fixtures/client/v1`, client fixture manifest, fixture verifier |
| 4. Playback conformance | `docs/api/fixtures/playback/v1`, state-machine/QoE fixtures, playback verifier |
| 5. Auth/session conformance | `docs/api/fixtures/auth/v1`, auth/session verifier, negative auth fixtures |
| 6. TV/deep-link conformance | `docs/api/fixtures/tv/v1`, TV/deep-link verifier, adapter mapping expectations |
| 7. Accessibility/input baselines | `docs/design/CLIENT_ACCESSIBILITY_INPUT.md`, `docs/api/fixtures/accessibility/v1`, accessibility/input verifier |
| 8. Shared design assets/tokens | `docs/design/CLIENT_DESIGN_ASSETS.md`, `docs/api/fixtures/design/v1`, source SVG assets, design verifier |
| 9. Diagnostics/logging bundles | `docs/design/CLIENT_DIAGNOSTICS.md`, `docs/api/fixtures/diagnostics/v1`, diagnostics verifier |
| 10. Device lab matrix | `docs/design/CLIENT_DEVICE_LAB.md`, `docs/api/fixtures/device-lab/v1`, device lab verifier |
| 11. Release/store readiness | `docs/design/CLIENT_RELEASE_READINESS.md`, `docs/api/fixtures/release/v1`, release readiness verifier |
| 12. Client CI/smoke harness | `docs/ci/CLIENT_CI_SMOKE_HARNESS.md`, `docs/api/fixtures/client-ci/v1`, `.github/workflows/client-ci-smoke.yml`, smoke harness, CI verifier |

## Open Follow-Ups

- Decide in Task 1 whether to add JSON Schema files beside the manifest or embed schema snippets in the manifest first.

## Task 10 Research Sources

- Android supported media formats: https://developer.android.com/media/platform/supported-formats
- Android Media3 ExoPlayer supported formats: https://developer.android.com/media/media3/exoplayer/supported-formats
- Android TV app creation: https://developer.android.com/training/tv/get-started/create
- Apple HTTP Live Streaming: https://developer.apple.com/streaming/
- Apple HLS authoring specification: https://developer.apple.com/documentation/http-live-streaming/hls-authoring-specification-for-apple-devices
- Fire TV ADB device testing: https://developer.amazon.com/docs/fire-tv/connecting-adb-to-device.html
- Fire OS 14+ TV device testing: https://developer.amazon.com/docs/app-testing/test-on-fire-os-14.html
- Roku certification criteria: https://developer.roku.com/dev/docs/certification
- Roku deep linking: https://developer.roku.com/dev/docs/implementing-deep-linking
- Samsung TV media specifications: https://developer.samsung.com/smarttv/develop/specifications/media-specifications.html
- Samsung Remote Test Lab: https://developer.samsung.com/remote-test-lab
- LG webOS streaming protocol and DRM: https://webostv.developer.lge.com/develop/specifications/streaming-protocol-drm
- Flutter integration testing: https://docs.flutter.dev/testing/integration-tests
- Windows Device Portal: https://learn.microsoft.com/en-us/windows/uwp/debug-test-perf/device-portal
- Xbox Device Portal: https://learn.microsoft.com/en-us/xbox/gdk/docs/tools/tools-console/wdp/wdp

## Task 11 Research Sources

- Android app signing: https://developer.android.com/studio/publish/app-signing
- Android app versioning: https://developer.android.com/studio/publish/versioning
- Flutter Android deployment: https://docs.flutter.dev/deployment/android
- Flutter iOS deployment: https://docs.flutter.dev/deployment/ios
- Apple App Store provisioning profile: https://developer.apple.com/help/account/provisioning-profiles/create-an-app-store-provisioning-profile/
- Apple Xcode distribution/versioning: https://developer.apple.com/documentation/xcode/preparing-your-app-for-distribution/
- Apple App Store Connect app information: https://developer.apple.com/help/app-store-connect/reference/app-information/app-information/
- Apple App Privacy Details: https://developer.apple.com/app-store/app-privacy-details/
- Tauri macOS signing: https://v2.tauri.app/distribute/sign/macos/
- Tauri Windows signing: https://v2.tauri.app/distribute/sign/windows/
- Microsoft Store policies: https://learn.microsoft.com/en-us/windows/apps/publish/store-policies
- Microsoft app certification process: https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-certification-process
- Microsoft privacy/support info: https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/support-info
- Microsoft age ratings: https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/age-ratings
- Amazon Appstore submission FAQ: https://developer.amazon.com/docs/app-submission/faq-submission.html
- Roku certification criteria: https://developer.roku.com/dev/docs/certification
- Roku channel publishing: https://developer.roku.com/dev/docs/channel-publishing-guide
- Roku deep linking: https://developer.roku.com/dev/docs/implementing-deep-linking
- Samsung application publication process: https://developer.samsung.com/tv-seller-office/application-publication-process.html
- Samsung launch checklist: https://developer.samsung.com/tv-seller-office/checklists-for-distribution/launch-checklist.html
- LG app approval process: https://webostv.developer.lge.com/distribute/app-approval-process
- LG app self checklist: https://webostv.developer.lge.com/distribute/app-self-checklist
- GitHub artifact attestations: https://docs.github.com/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-to-establish-provenance-for-builds
- GitHub workflow artifacts: https://docs.github.com/en/actions/tutorials/store-and-share-data
- GitHub SBOM export: https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/establish-provenance-and-integrity/export-dependencies-as-sbom
- CycloneDX tool center: https://cyclonedx.org/tool-center/

## Task 12 Research Sources

- GitHub Actions workflow syntax: https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax
- GitHub Actions job variations: https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/run-job-variations
- GitHub artifact attestations: https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations
- Docker Compose startup order: https://docs.docker.com/compose/how-tos/startup-order/
- Docker Compose up: https://docs.docker.com/reference/cli/docker/compose/up/
- Flutter integration tests: https://docs.flutter.dev/testing/integration-tests
- Flutter Android deployment: https://docs.flutter.dev/deployment/android
- Flutter iOS deployment: https://docs.flutter.dev/deployment/ios
- Android command-line tests: https://developer.android.com/studio/test/command-line
- Tauri GitHub distribution pipeline: https://v2.tauri.app/distribute/pipelines/github/
