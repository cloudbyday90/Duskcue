# Android TV / Google TV

## Purpose

This document is the implementation authority for Phase 17. It turns the shared TV-surface and client-contract work into a native Android TV / Google TV client without making launcher state, credentials, or playback history a second source of truth.

## Status

Phase 17 Tasks 0–10 are complete as of July 25, 2026. `clients/tv/android/` is a buildable native Kotlin application with a TV launcher manifest, 16:9 placeholder banner/icon, Compose for TV entry point, device linking and profile lifecycle, profile-gated home/browse/detail/search/settings screens, an in-memory Media3 playback service, strict playback deep-link revalidation, Watch Next publication with authenticated local artwork delivery, explicit living-room input/accessibility policy, privacy-safe diagnostics export, debug APK build, fixture-backed contract unit tests, and Android lint verification. It is intentionally separate from the Flutter phone/tablet client.

## Research Outcome

| Area | Official guidance | Duskcue decision |
|---|---|---|
| TV UI | Android recommends Compose for TV; the Leanback UI toolkit is deprecated. TV Material supplies focus-aware, remote-oriented components. | Use Kotlin and Compose for TV with `androidx.tv:tv-material`; do not create a Flutter TV flavor or add new Leanback code. |
| Application shape | A TV application needs a TV launcher activity, `android.software.leanback`, a launcher icon, and a TV banner. | Create a dedicated `com.duskcue.tv` app with a `LEANBACK_LAUNCHER` entry point, banner/icon placeholders, and no touchscreen requirement. |
| Playback | Media3 ExoPlayer and `MediaSession` are Android's current playback stack. TV quality guidance requires video to pause when the user leaves the app. | Use one Media3 player/session authority. Video pauses when Duskcue loses foreground; service lifecycle and session state are still used for controls, diagnostics, and a clean release path. |
| Watch Next | Only useful unfinished movies, unfinished episodes, and an eligible next/new episode belong in the row. Items must remain fresh, use complete metadata, be removed when completed or ineligible, and never duplicate a series. | Publish only eligible server-feed `continue`, `next_up`, and `new_episodes` items. Never publish recommendations, ambient playback, denied Kids content, or a second episode from a series. |
| Deep links | Platform rows must enter the app through a playback intent, but playback state and entitlement can change after publication. | Every launcher/deep-link entry resolves with Duskcue before playback. The client never trusts a tile's resume position, stream URL, access state, or signed artwork URL. |
| Google TV | Android APIs make Watch Next implementation possible, but Google TV launcher visibility depends on device, Play distribution, and platform policy. | Treat Android TV publication as feature-complete only after app-level verification; treat Google TV home visibility as a hardware/store release gate. |
| Sony BRAVIA | Sony routes Google TV and Android TV apps through compatible Google Play distribution. Models and regions determine availability. | Test Sony as a priority Android TV / Google TV hardware profile, not as a separate client or service integration. |

## Architecture

### Dedicated Native Client

`clients/tv/android/` owns its Kotlin application module, native resources, emulator tests, and Play-ready metadata. Its checked-in wrapper entry point delegates to the repository's established Gradle 8.14 wrapper so the mobile and TV Android projects use the same verified Gradle distribution. It may reuse the versioned Duskcue HTTP fixtures and conceptual adapter boundaries from Phase 16d, but it shares no Flutter UI/runtime or phone-oriented navigation code.

| Setting | Value | Reason |
|---|---|---|
| Application/package ID | `com.duskcue.tv` | Stable, TV-specific Play identity; keep distinct from `com.duskcue.mobile`. |
| Language/UI | Kotlin + Compose for TV | Current Android TV UI path with visible focus and remote-oriented components. |
| Minimum SDK | 26 | Android TV Watch Next is an Android O-era capability; this avoids a nonfunctional compatibility path. |
| Compile/target SDK | 36 | Matches the configured Android SDK and exceeds Google Play's Android TV API 34 minimum. |
| JVM/toolchain | Java 17 | Aligns with the checked-in Android/Flutter toolchain. |
| Playback | Media3 1.10.1 ExoPlayer + MediaSession | Current version used by Duskcue's native Android playback bridge; all Media3 modules stay version-aligned. |

The app declares `android.software.leanback` as required, `android.hardware.touchscreen` as not required, a `LEANBACK_LAUNCHER` activity, and HTTPS networking. Later Media3 work may add only the playback permissions required by its active service behavior. It must not inherit the phone app's cleartext-by-default networking posture. Local development must use an explicit debug-only network-security configuration when a local HTTP server is genuinely required.

### Local Toolchain Verification

The workstation configuration uses a user-level `JAVA_HOME` pointing to Temurin 17, `ANDROID_HOME` and `ANDROID_SDK_ROOT` pointing to the installed SDK, and user PATH entries for that JDK's `bin` directory and Android `platform-tools`. The ignored `clients/tv/android/local.properties` provides Gradle's per-workstation `sdk.dir`; it is intentionally not committed. SDK platform/build tools 36 and their licenses are installed. A fresh terminal is required to inherit changed Windows user environment variables.

### Task 1 Implementation

The foundation is checked in at `clients/tv/android/` with AGP 8.11.1, Kotlin 2.2.20, Compose compiler support, Java 17, compile/target SDK 36, and minSdk 26. `MainActivity` starts a Compose for TV application; the initial server-selection state contains no media or profile data. The manifest accepts only `duskcue://play/...` routes through the TV activity and disallows cleartext networking. The project includes TV icon/banner placeholders and a wrapper entry point aligned to the existing Gradle 8.14 distribution.

`DuskcueApiClient` is deliberately transport-agnostic and has a real `HttpURLConnection` transport for future background dispatch. It establishes the Kotlin target's fixture-first boundary with canonical `:48027` origins, bearer-header injection, cache-scope-keyed private ETag revalidation, RFC 9457 decoding, typed TV surface/resolve models, and no credential-bearing URL construction. `DuskcueApiClientTest` consumes the shared `docs/api/fixtures/tv/v1` files directly. Verification: `:app:testDebugUnitTest`, `:app:lintDebug`, and `:app:assembleDebug` all pass using SDK 36 and Temurin 17.

### Task 2 Implementation

The shared Kotlin adapter boundary now includes a bounded retry wrapper for safe reads only, capped `Retry-After` handling, a cursor helper that preserves opaque cursors, profile-scope-keyed private ETag state, RFC 9457 decoding with trace IDs, and a transport-free network failure result. It exposes `AndroidTv` and `FireTv` platform values instead of hard-coding Android TV paths so Fire TV can reuse the typed client without leaking its different launcher behavior into the Android adapter.

`ServerSentEventDecoder` handles multi-line SSE data, comments, IDs, and unknown event names without dropping the stream. The typed `tv_surface_changed` hint is the only current TV refresh event consumer; profile, playback, and Watch Next work decide how to schedule the actual refresh. `DiagnosticsRedactor` removes authorization/cookie headers and query strings before support data can be emitted, while retaining status, route path, error code, and trace ID. Tests read both shared TV/deep-link and cross-device-resume fixtures, covering current server response shapes rather than a copied client fixture.

### Contract And Identity Boundary

The native client consumes the Phase 16d route manifest and the client, auth/session, playback, TV/deep-link, diagnostics, accessibility, device-lab, release, and client-CI fixture packs before it grows a production client. Its API layer owns canonical server-origin selection and bearer injection; RFC 9457 mapping, private ETag revalidation, bounded retry, and cancellation; redacted correlation identifiers; device-link authentication; secure token storage; session expiry/logout cleanup; and a server-backed profile gate.

An authenticated TV cannot request, render, cache, or publish profile-scoped media until `profile_selection_required` is false. On server, account, or profile change it cancels requests and clears in-memory feed/artwork/playback state, encrypted credentials, diagnostics scope, and every local Watch Next mapping/program before rendering the new scope. A remembered profile is an opt-in convenience only; it is not a separate credential and never bypasses a Kids parent-unlock boundary.

### Task 3 Storage And Session Decision

Task 3 uses one app-private Preferences DataStore file, created once at top level in the data layer. The persisted envelope is encrypted with an AES-GCM key generated and retained by Android Keystore. The DataStore contains only ciphertext; plaintext bearer tokens, parent PINs, signed URLs, playback sessions, and parent-unlock expiry never reach disk. The Keystore key is app-scoped and non-exportable; it is not a shared Android `KeyChain` credential.

The envelope can retain an opaque installation device ID, known server origins, the active session token/user/profile summary, and an explicit remembered-profile preference. Device-link code and parent PIN values are transient request inputs only. An unrecoverable decrypt failure is treated as signed out and deletes the ciphertext, rather than attempting a partial recovery.

All changes flow through a session coordinator. An account or server replacement clears identity-scoped state: encrypted token, diagnostics identifiers, profile-scoped cache, queued playback, artwork, and future Watch Next mappings. A profile replacement clears profile-scoped state before the new profile can render or publish a launcher row. Logout and session-kicked signals use the identity-clear path. This provides the cleanup boundary before Task 7 adds actual TV Provider rows.

### Task 3 Implementation

`SecureSessionStore` is the Android implementation of the encrypted envelope. It creates no duplicate DataStore instances, generates a random per-installation device ID, remembers at most ten non-secret server origins, and keeps the active bearer token/user/profile summary encrypted. Its keystore AES-GCM payload includes a versionless IV prefix and authenticates ciphertext before decode; invalid ciphertext is deleted and replaced with an unauthenticated state. `allowBackup=false` is set on the TV application, so neither the encrypted payload nor a stale server/profile selection participates in Android backup/restore.

`TvAuthenticationService` implements the Duskcue device-link route sequence: it requests a code with stable Android TV metadata, exposes the verification shortcut and user code, accepts a token only from a successful poll, keeps `AUTH_023` at the advertised interval, and honors `AUTH_024` using the server `Retry-After` or the required five-second increase capped at 60 seconds. It exposes profile listing, parent unlock, explicit profile switching with opt-in remembered-device choice, restore, logout, logout-all, and session-kicked paths. A 401 from profile restore/switch clears local identity state. Parent PIN input is sent only to the unlock endpoint and is not represented in `SecureTvState`.

The coordinator tests prove cross-server/account replacement clears the identity scope before installing a new token, profile selection/switch clears profile scope before the replacement profile is exposed, logout preserves only the non-secret saved-server choice, and session-kicked is equivalent to logout. The actual living-room controls consume this service in Task 4; no profile-scoped screen is allowed to mount until its returned `ProfileGateState` permits it.

### Task 4 Implementation

`TvApplicationRuntime` creates one secure session/authentication runtime, one non-persistent private ETag store, and one `TvLivingRoomStore`. The living-room store is registered as the session coordinator's local-state cleaner: profile, account, server, logout, and session-kicked cleanup clears cached home rows and ETags before any replacement scope can load. A `304 Not Modified` response is useful only when a row cache exists in the same `{origin, user, profile}` scope; after cleanup it produces a bounded refresh error instead of displaying a prior household member's rows. Its unit tests consume the shared `surface-contract.json` fixture and prove cache reuse, ETag removal, and active-user-only `tv_surface_changed` handling.

The Compose for TV shell now has server entry/device-link, profile picker, Home, Browse, Search, Detail, Profiles, and Settings states. Device linking polls the existing service flow; profile selection and the remembered-profile preference use the server switch endpoint. The parent PIN is a transient numeric input passed only to the existing parent-unlock call. No screen mounts the feed until `profile_selection_required` is false. A 401 from home, browse, search, detail availability, settings, or profile refresh clears the local identity scope and returns to device linking.

Home renders the ordered server sections directly from `GET /api/v1/users/me/tv-surface`; Browse calls the library/collection item endpoints; Search calls the profile-authorized search endpoint; and Detail performs a pre-playback `GET /api/v1/tv/resolve/{platform_content_id}` availability check without attempting to create a stream. Settings reads and updates the user's TV-publication preference and exposes server/profile/logout controls. UI cards use the shared 10-foot tokens, reserve deterministic 16:9 title-art space until Task 8 adds authenticated artwork, expose server media titles as accessibility labels, and provide visible D-pad focus treatment. The foreground SSE transport and physical remote/TalkBack evidence remain later verification work; the controller is prepared to refresh only after a decoded event for the active user.

### Living-Room UX

The app uses the server's ordered Continue Watching, Next Up, New Episodes, and Recommended sections for the home surface. It adds native Browse, Search, Details, Player, Profiles, Server, and Settings routes without reimplementing server ranking, watch history, availability, or content-policy decisions.

Every visible action must be reachable by a basic D-pad (up, down, left, right, select, Back, Home). Compose for TV focus is the default; explicit directional overrides are allowed only after emulator and hardware testing proves default focus incorrect. Primary content stays inside the TV safe area, starting from the Android 5% overscan guidance, with visibly distinct focus, pressed, disabled, loading, empty, and error states.

### Playback And Deep Links

#### Task 5 Research Decision

The July 18, 2026 official Android review confirms that Media3 `ExoPlayer` is the supported player implementation and that a player which must be controllable by TV remotes/system media controls belongs with its `MediaSession` in a `MediaSessionService`. Android's service guidance creates both in `onCreate`, releases both in `onDestroy`, and requires the foreground-service/media-playback permissions plus the service action declaration. Android TV control guidance maps D-pad center to play/pause, left/right to seek, and up/down to showing controls without stopping video.

| Option | Benefits | Costs | Decision |
|---|---|---|---|
| Activity-owned `ExoPlayer` | Smallest implementation | Loses a single player/session authority when UI lifecycle changes; weaker remote/system control path | Reject. |
| `MediaSessionService` + `ExoPlayer` | One player authority, system/remote integration, supported Media3 lifecycle | Requires foreground-service declaration and explicit service cleanup | Use for interactive TV playback. |
| `MediaLibraryService` | Can expose an Android media catalog to external browsers | Would duplicate Duskcue's private/profile-scoped catalog and exposes an unnecessary browse surface | Reject for v1. |
| Android system playback resumption | Can show a post-reboot resumption entry | Requires a locally restorable playlist; Duskcue must re-resolve profile access and never retains stream URLs or token-bearing runtime state | Do not opt in. Return through Duskcue's normal resolve/start flow instead. |

The service keeps the bearer token, resolved stream URL, server session ID, and selected media metadata only in its active in-memory runtime. UI code resolves the platform content ID immediately before each `POST /api/v1/playback/start`, passes the fresh server resume position to the player, and clears the service runtime on logout, server switch, profile switch, session expiration, player error, completion, and explicit exit. The service uses `DefaultHttpDataSource` bearer headers for direct and HLS requests; it never adds credentials to stream URLs, local state, media metadata, or logs. It emits an immediate heartbeat on first rendered frame, periodic state heartbeats, seek/stop transitions, and a final QoE report. Activity/task removal pauses and releases interactive video rather than silently continuing it in the background. The service is app-internal (`exported=false`): Android TV's in-app D-pad and MediaSession behavior remain available, while assistant/third-party controller discovery is intentionally deferred until a controller authorization policy exists.

The first player surface binds a native `PlayerView` to the service; default Media3/TV remote behavior remains authoritative until device testing identifies an accessibility or focus defect. The initial device profile is deliberately conservative (H.264/AAC, 1080p, stereo, SDR, MP4/Matroska, WebVTT/SRT); richer codecs, HDR, bit depth, passthrough, and display-mode claims require device capability evidence rather than optimistic constants. Track selection, captions, quality mode, and QoE are derived from the existing playback/quality contracts rather than a client-owned policy. The service does not implement Watch Next, system playback resumption, background ambient channels, or deep-link routing in this task.

The player resolves a current Duskcue item immediately before `POST /api/v1/playback/start`, then uses only the returned runtime stream state. It reports heartbeat, seek, stop, completion, selected audio/subtitle tracks, playback errors, and QoE through the shared contract. It refreshes the latest server resume state after returning from launcher/deep-link entry and before it creates the `MediaItem`.

#### Task 5 Implementation

The TV player is now an app-internal `MediaSessionService` owning one ExoPlayer, MediaSession, bearer-authenticated direct/HLS data source, and temporary `PlayerView` attachment. It has no persisted playback token, URL, session ID, or media state. It sends first-frame/periodic heartbeats, seek, stop/completion, and final QoE signals; the QoE payload records startup, rebuffer count/duration/ratio, bitrate/rung, quality changes/drops, selected quality mode, and a distinct playback failure code/message. Media3 handles play/pause/media buttons through the session and uses the shared 10-second backward / 30-second forward seek increments; a player error is reported and displayed as a safe return-to-details state rather than a blank playback surface.

Audio and caption choices are fetched from the profile-authorized media-file endpoint while a detail view is active. The scanner now stores every source audio stream in `media_files.additional_streams.audio` alongside subtitle streams, so a library rescan upgrades existing media metadata. `POST /api/v1/playback/start` validates an explicitly selected source index and returns `VALID_001` with a field-level `unavailable` error for a missing audio or subtitle stream. The selected audio stream drives the decision engine and FFmpeg map; a selected subtitle is retained in direct play or rendered into the HLS conversion when container conversion is required. The player observes profile-authorized segment windows once per second and exposes a focusable Skip Intro/Credits action that uses the segment's server-provided `skip_to_ms`. The native direct-play preference is language-based, so exact selection among duplicate same-language streams remains a physical-device validation item rather than a completed release claim.

The Android test/lint/debug-build gate and Rust `cargo check -p duskcue` pass after this implementation. Watch Next, artwork, CI, and device evidence remain separate tasks.

`duskcue://play/{type}/{id}` is an app route, not a capability URL. The activity accepts only recognized type/ID paths, shows no private item details until server resolution succeeds, and falls back to device linking or a bounded unavailable/access-denied state. It never persists signed stream URLs, bearer tokens, parent PINs, or parent-unlock expiry. Future Google TV Live integrations must additionally respect the documented `exit_on_back` direct-back behavior; it is not claimed by the initial on-demand client.

#### Task 6 Implementation

`MainActivity` converts both the initial activity intent and every `onNewIntent` delivery into an in-memory launch request. The parser accepts exactly `duskcue://play/{movie|episode}/{canonical UUID}`; it rejects a different authority, type, path shape, query, fragment, user-info, or port before any API call. The raw URI is never written to storage, rendered, logged, or included in an error message. Task 0 approved this custom-scheme route only, so this implementation intentionally does not claim an HTTPS App Link or register an unverified deployment-specific host.

A valid request remains pending through device linking and required profile selection. Once the authenticated profile gate opens, the client converts it to the canonical `duskcue:{type}:{id}` server identity, calls TV resolve, requires a fresh playable/access-revalidated response, and only then starts a new server playback session at the returned current resume position. It does not trust the launcher’s title, availability, prior resume position, stream URL, or track state. Revoked, deleted, malformed, unavailable, or otherwise stale entries produce the same bounded `This item is unavailable.` outcome; a 401 clears the local session and returns to device linking while retaining only the in-memory non-secret target for the renewed session. This is direct-to-play once access is established, with no title or resume/start-over interstitial.

### Watch Next Adapter

The adapter reads the active profile's TV feed and writes only local platform records through AndroidX TV Provider. A local mapping contains an opaque profile scope, stable `surface_item_id`/`platform_content_id`, platform `program_id`, content fingerprint, and last publication state. It does not contain a bearer token, signed URL, raw file path, parent PIN, or a cross-profile resume copy.

Publication rules:

- map unfinished movies and episodes to `WATCH_NEXT_TYPE_CONTINUE`, including duration, last playback position, and last engagement time;
- map one eligible episode per started series to `WATCH_NEXT_TYPE_NEXT` or `WATCH_NEXT_TYPE_NEW`;
- use the stable Duskcue platform content ID as the provider-facing identity where Android requires an internal ID;
- update only the item whose state changed, reconcile stale mappings from a new feed, and delete completed, revoked, denied, disabled, or profile-switched rows;
- refresh after active `tv_surface_changed` hints, launch/resume, playback completion, and exit; the release path must measure the Android guideline's five-second update expectation;
- never publish `recommended`, ambient items, unplayable media, duplicate series entries, or content the current profile cannot see.

If the system reports a program as no longer browsable, the adapter removes its local mapping and does not recreate it until the next legitimate Duskcue state change. Google TV's actual home-row display is not inferred from successful provider writes.

#### Task 7 Implementation

The Android application now uses `androidx.tvprovider:tvprovider:1.1.0` to publish `WatchNextProgram` rows from the active profile's fresh `android_tv` surface. The documented builder operations in that artifact carry an over-broad library-only lint annotation, so the single provider-builder function has a scoped `RestrictedApi` suppression rather than disabling lint project-wide. The publisher accepts only `continue`, `next_up`, and `new_episodes`; `recommended`, ambient playback, stale cached feed data, unplayable items, malformed platform IDs, and malformed Duskcue deep links are excluded before the provider API is reached. Movies and episodes must have a positive duration and engagement time. Continue items must exceed the Android start threshold and retain a three-minute end-credits fallback; next/new items require a stable episode `series_id`. Source priority is Continue, Next, then New, so a stable series ID produces one episode per series even when multiple feed sections contain it.

`TvSurfaceItemResponse` now exposes nullable `series_id`. It is null for movies and required for episodes in the versioned and legacy TV fixtures; a client connected to an older server without this field safely declines episode publication instead of risking duplicate-series rows. `platform_content_id` is used as the content ID and internal provider ID, while stable Duskcue series IDs and season/episode numbers populate the episode metadata. Task 8 adds a local authenticated poster URI without putting a bearer or signed URL into TV Provider.

The existing Android Keystore AES-GCM DataStore envelope persists the program mapping. Each entry has only a SHA-256 hash of the origin/user/profile scope, Duskcue media and platform IDs, a platform program ID, and a content fingerprint. There is no raw profile scope, title, bearer token, stream URL, file path, parent PIN, unlock state, or copied resume point. Reconciliation serializes provider access, drains failed cleanup IDs first, inserts missing eligible rows, updates only a changed fingerprint, and deletes mappings absent from the new fresh surface. Profile, identity, server, session-kicked, and logout cleanup stop playback and delete all provider rows before a replacement scope can publish; unresolved deletions retain only a numeric pending ID for retry.

`MainActivity` refreshes publication on resume. The home refresh path also publishes after an active `tv_surface_changed` hint, and the player schedules a fresh feed after completion or explicit exit and after a five-minute paused interval. Android's `ACTION_WATCH_NEXT_PROGRAM_BROWSABLE_DISABLED` receiver removes the provider row and adds a matching fingerprint suppression; unchanged Duskcue state cannot recreate a row the user removed, while a later changed source fingerprint can. Provider success is not a claim that Google TV renders the row: launcher visibility remains an emulator/device and Play/certification gate.

#### Task 8 Implementation

The Watch Next adapter fetches a poster only from the canonical API-relative path `/api/v1/items/{canonical-media-uuid}/artwork/poster?size=w500`. It rejects absolute URLs, authority/query/fragment-bearing feed paths, and paths for another media item before any connection is opened. `UrlConnectionWatchNextArtworkFetcher` makes the same-origin HTTPS request with the active bearer token in memory, disables redirects, accepts only WebP responses up to five megabytes, and conditionally revalidates with the previous ETag. Neither the TV Provider row, its URI, encrypted state, logs, nor diagnostics receives that token, the Duskcue origin, a raw artwork path, or a signed URL.

Downloaded bytes live only in app-private cache files. The provider receives an opaque local URI of the form `content://com.duskcue.tv.watchnext-artwork/poster/{opaque-uuid}`. Its tiny `ContentProvider` exposes only a canonical UUID-backed poster file; it has no listing, mutation, remote-fetch, or lookup behavior. The encrypted Keystore/DataStore metadata is similarly bounded to a hashed user/profile/server scope, content ID, source hash, opaque cache UUID, and ETag. Successful artwork byte changes receive a new local URI and update the existing `WatchNextProgram`; `304` leaves the URI and row untouched. Old rows and their artwork are removed with normal scope cleanup. If no valid poster can be delivered, a deterministic 2:3 WebP title tile is generated locally from the stable platform content ID and display title.

Android TV sizing is explicit: Watch Next poster is `w500`; app-local backdrops, thumbnails, and logos use `w1280`, `w300`, and `original` respectively when their later UI surfaces consume the typed `TvArtworkHints` response. `setPosterArtUri` is now populated alongside the existing stable content/internal-provider IDs and episode metadata. The reconciler distinguishes a source fingerprint from a rendering fingerprint: artwork changes can update an existing row, but an Android-disabled row stays suppressed until the underlying Duskcue content state changes. The current `tv_surface_changed` publication path conditionally revalidates the poster as well as the row metadata.

#### Task 9 Implementation

`TvQualityPolicy` defines the conservative 58dp horizontal and 28dp vertical safe area, a 20sp minimum client-owned supporting-text size, logical Back destinations, and player remote shortcuts. Setup starts on the server field, linking starts on Cancel, search starts on its input, detail and playback return to a focused primary action, and all other authenticated routes return to Home before the activity can exit to the launcher. Focused, pressed, and disabled controls use a border plus distinct surfaces; the UI has no animated route or focus transition, so reduced-motion operation remains fully legible without a motion-dependent cue.

The player intercepts D-pad left/right, gamepad A, media transport buttons, Menu, and Captions only for the documented playback operations. Its visible remote controls expose play/pause, current captions, and current audio tracks; in-playback track changes update the active Media3 track-selection parameters rather than only changing the displayed label. Async messages use polite accessibility live regions, failures use assertive live regions and a focused retry action, and client-owned labels identify cards, actions, inputs, and disabled controls to TalkBack.

`TvQualityPolicyTest` verifies Back destinations, D-pad/gamepad/media/caption mappings, safe-area and type minima, and the Android TV entries in the shared accessibility fixture pack. The Android test source set consumes that fixture pack directly, while `node scripts/verify-accessibility-input.mjs` verifies its cross-client schema.

### Task 9 Manual Release Evidence

| Check | Required evidence | Automation boundary |
|---|---|---|
| D-pad, gamepad, and remote traversal | Emulator/device walkthrough of home rows, search keyboard return, details, player, Settings, profile picker, and error retry. | Policy mappings and shared remote cases are unit/fixture checked; physical traversal is not emulated here. |
| TalkBack and captions | TalkBack labels/roles/state walkthrough and a real subtitle/audio-track change during playback. | Compose semantics and active Media3 track updates are code covered; screen-reader output requires a device. |
| Overscan, focus, and reduced motion | Screenshots on representative Android TV/Google TV hardware, including player chrome and a reduced-motion system setting. | Conservative layout constants and no animated focus/route transitions are verified in code. |
| Android TV quality checklist | Record app-quality criteria, Watch Next launch focus, lifecycle pause, media buttons, and Back-to-launcher behavior in release evidence. | Task 11/12 add CI/release artifact collection; Google TV launcher visibility remains hardware/store evidence. |

#### Task 10 Implementation

`TvDiagnostics` is an application-memory-only, 1,000-record/24-hour bounded ledger. Each record uses the shared client log fields: RFC3339 timestamp, generated app version, `android_tv`, bounded route/screen, opaque request ID or `unavailable`, stable event type, severity, and operational privacy classification. The ledger never accepts a request body, header, bearer token, signed URL, title, profile, private path, or media ID. Profile/account/server cleanup clears the entire ledger before replacement identity data can appear.

`DuskcueApiClient` records successful HTTP status/request IDs, network failures, and RFC 9457 failures using only route templates, `x-request-id`, `trace_id`, and a validated error code. Player startup and failure records retain only the playback session ID, stream decision, and bounded failure code. Watch Next refresh records use aggregate insert/update/delete/failure counts, and active `tv_surface_changed` messages retain their opaque event ID without the SSE payload. The support bundle uses host-only server origin, explicit `unknown` network/capability values where the client has not observed the fact, bounded playback-failure/request-ID summaries, and no automatic upload.

Settings now offers a user-initiated **Export support bundle** action. Android's document creator receives one redacted JSON file only after the user selects a destination; Duskcue tells the user to share it manually with their server administrator. `TvDiagnosticsTest` covers bundle redaction, retention, and the Android TV export checklist, while `DuskcueApiClientTest` proves request/trace correlation does not export a raw media identifier or bearer token. `node scripts/verify-client-diagnostics.mjs` verifies the shared fixture schema.

#### Task 11 Implementation

The shared client smoke workflow now has an automatic `android_tv_conformance` lane for Android TV source, contract, fixture, workflow, and documentation changes. It installs Android SDK Platform/Build-Tools 36 and Temurin 17, runs the TV/client/playback/auth/deep-link/accessibility/diagnostics verifiers plus the shared smoke-harness plan and CI-fixture verifier, then runs `:app:testDebugUnitTest`, `:app:lintDebug`, and `:app:assembleDebug`. The job uploads the unsigned debug APK, lint HTML report, and unit-test results as short-retention debugging evidence only; it performs no signing, publishing, SBOM, provenance, or release assertion.

`android_tv_emulator_smoke` is an opt-in `workflow_dispatch` job. It boots an API 36 `android-tv`/`tv_1080p` AVD, installs the debug APK, confirms the target exposes `android.software.leanback`, starts the `LEANBACK_LAUNCHER` activity, and exercises an intentionally non-secret valid-shape `duskcue://play/...` handoff. The same portable [android-tv-emulator-smoke.mjs](../../scripts/android-tv-emulator-smoke.mjs) script can run against a locally booted Android TV AVD or a USB/Wi-Fi-debugged TV. It checks installation and intent handling only; it does not claim authenticated browse/playback, Watch Next launcher visibility, TalkBack, captions, HDR, audio, remote hardware, standby/resume, document-picker sharing, or Google Play behavior.

The static [verify-android-tv-ci.mjs](../../scripts/verify-android-tv-ci.mjs) gate keeps the workflow, fixture matrix, manual AVD job, required conformance commands, and debug evidence paths connected. It is included in the shared client-CI verifier list so a Docker smoke-plan check cannot silently omit the Android TV lane.

## Delivery Order

1. Create the standalone Gradle/Kotlin/Compose-for-TV app and prove a TV emulator build/launch with the correct manifest and placeholder assets.
2. Add fixture-backed Kotlin HTTP models/client, secure local state, server selection, device linking, and profile gate.
3. Build the home/browse/details/search/settings surfaces from the scoped feed before native playback.
4. Add the Media3 playback/session lifecycle and strict deep-link revalidation.
5. Add the Watch Next mapping store/reconciler and artwork handling.
6. Consume all Phase 16d verifiers and run Android lint/unit/debug-build checks; use the opt-in Android TV AVD smoke for installation, Leanback launcher, and deep-link handoff evidence.
7. Complete Play artifacts, signing slots, Data Safety, content rating, TV banner/screenshots, support runbook, and staged-release evidence before a public claim.

## Deferred Release Gates

- Google Play account, upload/app-signing keys, Data Safety declarations, content rating, store listing, and reviewer credentials remain external secrets/release work.
- Google TV launcher visibility, certification, and region/device availability need empirical hardware and store evidence.
- NVIDIA SHIELD and Sony BRAVIA HDR, Dolby Vision, audio passthrough/downmix, subtitles, standby/resume, and remote/gamepad behavior require real devices.
- TalkBack behavior, actual overscan screenshots, reduced-motion settings, physical remote/gamepad traversal, and platform caption-preference behavior require dedicated emulator/device evidence; the Task 11 AVD smoke intentionally does not claim those observations.
- The system document-picker interaction, selected-destination behavior, and manual support-bundle sharing require emulator/device evidence. Support exports remain local and manual; no diagnostics upload endpoint is implemented or implied.
- A future Fire TV client may reuse non-UI Kotlin API, profile, playback, and diagnostics abstractions only after its Android/Fire OS divergence is evaluated.

## Official Sources

- Android TV app creation: https://developer.android.com/training/tv/get-started/create
- Android Virtual Device command-line management: https://developer.android.com/tools/avdmanager
- Android Emulator command-line usage: https://developer.android.com/studio/run/emulator-commandline
- Android Debug Bridge: https://developer.android.com/tools/adb
- Compose for TV: https://developer.android.com/training/tv/playback/compose
- Media3 background playback service: https://developer.android.com/media/media3/session/background-playback
- Media3 playback control/session: https://developer.android.com/media/media3/session/control-playback
- Android TV playback controls: https://developer.android.com/training/tv/playback/controls
- Android TV playback guidance: https://developer.android.com/training/tv/playback/
- Android TV playback overview: https://developer.android.com/training/tv/playback
- Media3 ExoPlayer setup: https://developer.android.com/media/media3/exoplayer/hello-world
- Media3 session playback control: https://developer.android.com/media/media3/session/control-playback
- Media3 background playback service: https://developer.android.com/media/media3/session/background-playback
- Android TV Watch Next guidelines: https://developer.android.com/training/tv/discovery/guidelines-app-developers
- Android TV Watch Next attributes: https://developer.android.com/training/tv/discovery/watch-next-programs
- Android TV Watch Next provider operations: https://developer.android.com/training/tv/discovery/watch-next-add-programs
- AndroidX TV Provider disabled-program broadcast: https://developer.android.com/reference/androidx/tvprovider/media/tv/TvContractCompat
- Android TV app-quality criteria: https://developer.android.com/docs/quality-guidelines/tv-app-quality
- Android TV navigation: https://developer.android.com/training/tv/get-started/navigation
- Android TV focus system: https://developer.android.com/design/ui/tv/guides/styles/focus-system
- Android TV layouts and overscan: https://developer.android.com/design/ui/tv/guides/styles/layouts
- Android Keystore: https://developer.android.com/privacy-and-security/keystore
- Android cryptography: https://developer.android.com/privacy-and-security/cryptography
- Android Preferences DataStore: https://developer.android.com/topic/libraries/architecture/datastore
- Google Play target API policy: https://support.google.com/googleplay/android-developer/answer/11926878
- Google Play Android TV preview assets: https://support.google.com/googleplay/android-developer/answer/9866151
- Sony Google TV / Android TV application availability: https://www.sony.com/electronics/support/televisions-projectors-lcd-tvs-android-/xr-55x90k/articles/00114472
