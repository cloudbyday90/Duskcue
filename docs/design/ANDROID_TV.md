# Android TV / Google TV

## Purpose

This document is the implementation authority for Phase 17. It turns the shared TV-surface and client-contract work into a native Android TV / Google TV client without making launcher state, credentials, or playback history a second source of truth.

## Status

Phase 17 Tasks 0–2 are complete as of July 18, 2026. `clients/tv/android/` is a buildable native Kotlin application with a TV launcher manifest, 16:9 placeholder banner/icon, Compose for TV entry point, debug APK build, fixture-backed contract unit tests, and Android lint verification. It is intentionally separate from the Flutter phone/tablet client.

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

### Task 1 Implementation

The foundation is checked in at `clients/tv/android/` with AGP 8.11.1, Kotlin 2.2.20, Compose compiler support, Java 17, compile/target SDK 36, and minSdk 26. `MainActivity` starts a Compose for TV application; the initial server-selection state contains no media or profile data. The manifest accepts only `duskcue://play/...` routes through the TV activity and disallows cleartext networking. The project includes TV icon/banner placeholders and a wrapper entry point aligned to the existing Gradle 8.14 distribution.

`DuskcueApiClient` is deliberately transport-agnostic and has a real `HttpURLConnection` transport for future background dispatch. It establishes the Kotlin target's fixture-first boundary with canonical `:48027` origins, bearer-header injection, cache-scope-keyed private ETag revalidation, RFC 9457 decoding, typed TV surface/resolve models, and no credential-bearing URL construction. `DuskcueApiClientTest` consumes the shared `docs/api/fixtures/tv/v1` files directly. Verification: `:app:testDebugUnitTest`, `:app:lintDebug`, and `:app:assembleDebug` all pass using SDK 36 and Temurin 17.

### Task 2 Implementation

The shared Kotlin adapter boundary now includes a bounded retry wrapper for safe reads only, capped `Retry-After` handling, a cursor helper that preserves opaque cursors, profile-scope-keyed private ETag state, RFC 9457 decoding with trace IDs, and a transport-free network failure result. It exposes `AndroidTv` and `FireTv` platform values instead of hard-coding Android TV paths so Fire TV can reuse the typed client without leaking its different launcher behavior into the Android adapter.

`ServerSentEventDecoder` handles multi-line SSE data, comments, IDs, and unknown event names without dropping the stream. The typed `tv_surface_changed` hint is the only current TV refresh event consumer; profile, playback, and Watch Next work decide how to schedule the actual refresh. `DiagnosticsRedactor` removes authorization/cookie headers and query strings before support data can be emitted, while retaining status, route path, error code, and trace ID. Tests read both shared TV/deep-link and cross-device-resume fixtures, covering current server response shapes rather than a copied client fixture.

### Contract And Identity Boundary

The native client consumes the Phase 16d route manifest and the client, auth/session, playback, TV/deep-link, diagnostics, accessibility, device-lab, release, and client-CI fixture packs before it grows a production client. Its API layer owns canonical server-origin selection and bearer injection; RFC 9457 mapping, private ETag revalidation, bounded retry, and cancellation; redacted correlation identifiers; device-link authentication; secure token storage; session expiry/logout cleanup; and a server-backed profile gate.

An authenticated TV cannot request, render, cache, or publish profile-scoped media until `profile_selection_required` is false. On server, account, or profile change it cancels requests and clears in-memory feed/artwork/playback state, encrypted credentials, diagnostics scope, and every local Watch Next mapping/program before rendering the new scope. A remembered profile is an opt-in convenience only; it is not a separate credential and never bypasses a Kids parent-unlock boundary.

### Living-Room UX

The app uses the server's ordered Continue Watching, Next Up, New Episodes, and Recommended sections for the home surface. It adds native Browse, Search, Details, Player, Profiles, Server, and Settings routes without reimplementing server ranking, watch history, availability, or content-policy decisions.

Every visible action must be reachable by a basic D-pad (up, down, left, right, select, Back, Home). Compose for TV focus is the default; explicit directional overrides are allowed only after emulator and hardware testing proves default focus incorrect. Primary content stays inside the TV safe area, starting from the Android 5% overscan guidance, with visibly distinct focus, pressed, disabled, loading, empty, and error states.

### Playback And Deep Links

The player resolves a current Duskcue item immediately before `POST /api/v1/playback/start`, then uses only the returned runtime stream state. It reports heartbeat, seek, stop, completion, selected audio/subtitle tracks, playback errors, and QoE through the shared contract. It refreshes the latest server resume state after returning from launcher/deep-link entry and before it creates the `MediaItem`.

`duskcue://play/{type}/{id}` is an app route, not a capability URL. The activity accepts only recognized type/ID paths, shows no private item details until server resolution succeeds, and falls back to device linking or a bounded unavailable/access-denied state. It never persists signed stream URLs, bearer tokens, parent PINs, or parent-unlock expiry. Future Google TV Live integrations must additionally respect the documented `exit_on_back` direct-back behavior; it is not claimed by the initial on-demand client.

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

## Delivery Order

1. Create the standalone Gradle/Kotlin/Compose-for-TV app and prove a TV emulator build/launch with the correct manifest and placeholder assets.
2. Add fixture-backed Kotlin HTTP models/client, secure local state, server selection, device linking, and profile gate.
3. Build the home/browse/details/search/settings surfaces from the scoped feed before native playback.
4. Add the Media3 playback/session lifecycle and strict deep-link revalidation.
5. Add the Watch Next mapping store/reconciler and artwork handling.
6. Consume all Phase 16d verifiers, then add Android lint/unit/emulator checks and physical NVIDIA SHIELD/Sony BRAVIA evidence.
7. Complete Play artifacts, signing slots, Data Safety, content rating, TV banner/screenshots, support runbook, and staged-release evidence before a public claim.

## Deferred Release Gates

- Google Play account, upload/app-signing keys, Data Safety declarations, content rating, store listing, and reviewer credentials remain external secrets/release work.
- Google TV launcher visibility, certification, and region/device availability need empirical hardware and store evidence.
- NVIDIA SHIELD and Sony BRAVIA HDR, Dolby Vision, audio passthrough/downmix, subtitles, standby/resume, and remote/gamepad behavior require real devices.
- A future Fire TV client may reuse non-UI Kotlin API, profile, playback, and diagnostics abstractions only after its Android/Fire OS divergence is evaluated.

## Official Sources

- Android TV app creation: https://developer.android.com/training/tv/get-started/create
- Compose for TV: https://developer.android.com/training/tv/playback/compose
- Android TV playback overview: https://developer.android.com/training/tv/playback
- Media3 ExoPlayer setup: https://developer.android.com/media/media3/exoplayer/hello-world
- Media3 session playback control: https://developer.android.com/media/media3/session/control-playback
- Media3 background playback service: https://developer.android.com/media/media3/session/background-playback
- Android TV Watch Next guidelines: https://developer.android.com/training/tv/discovery/guidelines-app-developers
- Android TV Watch Next attributes: https://developer.android.com/training/tv/discovery/watch-next-programs
- Android TV app-quality criteria: https://developer.android.com/docs/quality-guidelines/tv-app-quality
- Android TV navigation: https://developer.android.com/training/tv/get-started/navigation
- Android TV layouts and overscan: https://developer.android.com/design/ui/tv/guides/styles/layouts
- Google Play target API policy: https://support.google.com/googleplay/android-developer/answer/11926878
- Google Play Android TV preview assets: https://support.google.com/googleplay/android-developer/answer/9866151
- Sony Google TV / Android TV application availability: https://www.sony.com/electronics/support/televisions-projectors-lcd-tvs-android-/xr-55x90k/articles/00114472
