# Fire TV Implementation Design

## Outcome

This document is the authoritative Phase 18 design decision. Duskcue will build a dedicated Android-based Fire TV application that reuses platform-neutral Kotlin logic from the Android TV client, but has its own application identity, build target, launcher integration, release path, and Amazon adapter boundary. It will not ship AndroidX Watch Next APIs, Google Play assumptions, or a synthetic Amazon catalog into the Fire TV target.

The first deliverable is a fully usable Duskcue Fire OS app: profile-gated app-local browsing and Media3 playback, authenticated Duskcue deep-link handoff, remote/focus/accessibility support, and diagnostics. Amazon Content Personalization, Watch Activity, and catalog discovery are opt-in integrations with independent technical and partner gates. Vega is a separate, non-Android product track.

## Official Research Rechecked

Reviewed August 9, 2026. The implementation must be rechecked against these sources before an Amazon SDK or store submission upgrade.

| Topic | Official finding | Decision |
|---|---|---|
| Fire OS Android compatibility | [Fire TV differs from Android TV](https://developer.amazon.com/docs/fire-tv/differences-from-android-tv-development.html) confirms both are Android-based, but Google services are unavailable, Leanback support is incomplete, Amazon Appstore replaces Google Play, and testing requires physical Fire TV hardware. | Reuse only platform-neutral Duskcue Kotlin, Compose/TV UI patterns, Media3 playback, profile gate, deep-link resolver, diagnostics, and fixture tests. Build a separate Fire app target and Appstore release configuration. |
| Fire OS range | [Get started with Fire TV](https://developer.amazon.com/docs/fire-tv/get-started-with-fire-tv.html) and [Fire OS 16 guidance](https://developer.amazon.com/docs/fire-tv/fire-os-16.html) document multiple Fire OS generations. The current Android TV target already requires API 26, while Fire OS 6 is API 25 and Fire OS 7 is API 28. | Task 1 will support Fire OS 7/API 28 and newer with `minSdk = 28`, `compileSdk = 36`, and `targetSdk = 36`; Fire OS 6 and older are explicitly out of scope. Test at least one Fire OS 7/8/14 family device and one Fire OS 16 device before release claims. |
| Fire TV launcher and voice | Fire TV honors `LEANBACK_LAUNCHER`, but [does not support Leanback `SearchFragment`](https://developer.amazon.com/docs/fire-tv/differences-from-android-tv-development.html); Alexa/global results come from the Amazon Catalog rather than an in-app Android `ContentProvider`. | The Fire app will use `LEANBACK_LAUNCHER`, app-local search, and its own authenticated Duskcue routes. It will not add `SearchFragment` or claim Alexa/global discovery until catalog admission and launcher integration are complete. |
| Media focus and remote | Fire TV needs a manifest `MediaButtonReceiver` to take audio focus and adds Fast Forward, Rewind, and Menu keys. | Task 1 must include Media3 `MediaSessionService`/media-button integration and test D-pad, Back, Play/Pause, FF, Rewind, and Menu behavior on physical hardware. |
| Watch Activity | [Watch Activity](https://developer.amazon.com/docs/fire-tv/watch-activity.html) requires a catalog content ID, obfuscated app profile ID, duration, position, and state. Active events are current-device events; event triggers include state changes, seeking, and every 60 seconds. | Task 2 emits only after Amazon enablement, valid catalog mapping, customer opt-in, and an eligible standard Duskcue profile. It uses an opaque server-issued Fire profile key, not a Duskcue UUID, account ID, title, token, or PIN. |
| Content Personalization | [Content Personalization](https://developer.amazon.com/docs/fire-tv/introduction-content-personalization.html) is customer opt-in and uses Watch Activity and watchlist data. | Keep it disabled by default and feature-gated at runtime. A Kids profile never sends personalization activity in the first release; enabling it later needs an explicit parental/privacy review and a separately approved policy. |
| Catalog and EMBER | [EMBER Catalog Integration](https://developer.amazon.com/docs/catalog/ember-catalog-integration-overview.html) requires distribution rights, an Amazon-accepted catalog, a Fire TV playback app, AWS access, staging verification, and production admission. | Do not export private Duskcue libraries, ambient channels, or user-specific metadata. Task 4 is conditional on partner admission and may cover only distributable catalog content with accepted mapping and Amazon-approved deep links. |
| Vega | [Vega](https://developer.amazon.com/docs/vega/vega.html) is a separate developer stack in open beta, with its own tools and application model. | Do not treat Vega as a Fire OS Android build flavor. Task 5 starts only after target-device need, Amazon access, physical-device test capability, and an approved independent product/design decision. |

## Alternatives and Recommendation

| Option | Advantages | Costs and risks | Recommendation |
|---|---|---|---|
| One Android TV APK with Fire-specific branches | Lowest initial file count; shares existing code. | Couples Appstore identity, Firebase/Google assumptions, Watch Next provider, Amazon SDK, release policy, and test matrix to a Google TV target. A Fire-only regression can affect Android TV. | Rejected. |
| Dedicated Fire OS Android application target with shared neutral code | Clear Appstore boundary; Fire-only permissions/receivers; independent versioning and physical-device gates; keeps Android TV Watch Next out of Fire builds. | Requires deliberate extraction of shared Kotlin components and duplicate app packaging configuration. | Selected. |
| Fire TV web wrapper | Could reuse web views quickly. | Does not provide a native Media3/service path or the focused TV experience required by the roadmap; makes Amazon integration and hardware behavior less dependable. | Rejected. |
| Start by implementing EMBER/global discovery | Potential launcher discovery if accepted. | Partner-only, rights-bound, catalog-dependent, and unsuitable for a user's private library; no useful baseline if admission is unavailable. | Deferred behind a usable Fire OS app. |
| Treat Vega as the Fire TV client | Covers a newer Amazon stack. | Non-Android technology and release model; no guarantee that its device/app availability meets Duskcue's needs. | Deferred as an independent track. |

## Architecture Boundary

### Target layout

Task 1 creates `clients/tv/fire/` with package identity separate from `com.duskcue.tv`. It may consume an extracted neutral Kotlin module for:

- server origin selection, authenticated API client, and fixture DTOs;
- server-authoritative profile picker, remembered-device preference, switch invalidation, and Kids parent-unlock flow;
- playback state translation, Media3 session coordination, app-local TV feed rendering, accessibility/focus behavior, diagnostics redaction, and Duskcue deep-link resolution.

It must keep these items Fire-only:

- Amazon Appstore identity, store assets, signing configuration, and dependency repository/SDK wiring;
- `amazon.hardware.fire_tv` detection, Fire manifest declarations, media-button handling, and Fire remote semantics;
- Fire TV Integration SDK feature flag, Watch Activity adapter, Amazon profile-key adapter, catalog mapping, and catalog-approved launch entry;
- Fire-specific capability and physical-device validation evidence.

The Fire target must not depend on AndroidX `tvprovider`, `WatchNextProgram`, `PreviewProgram`, or Android TV Watch Next artwork/provider code. It must also not rely on Google Play services, Play billing, Play Store links, or Firebase APIs that require Google services. No native Amazon source, account token, catalog credential, or Appstore signing secret belongs in the repository.

### Shared-device and Kids boundary

The profile contract in [PROFILES_AND_AMBIENT_CHANNELS.md](PROFILES_AND_AMBIENT_CHANNELS.md) applies unchanged. The Fire app creates a random per-installation Duskcue `device_id`, honors `profile_selection_required` before loading profile-scoped rows, and presents “Remember on this TV” only after normal account authentication. It stores no profile credential, raw profile ID, session token, parent PIN, parent-unlock state, hardware ID, advertising ID, or Amazon account identity in that preference.

On profile switch, logout, account change, or session revocation, it must stop/pause sensitive playback as appropriate, abort profile-scoped requests, clear artwork/feed/ambient/runtime launcher state, clear any queued Fire Watch Activity identity, and revalidate the new server session before rendering or reporting anything. An ambient channel remains Duskcue-diagnostic-only and never produces personal history or Fire Watch Activity.

Kids profiles retain the server's library, rating, search, external-link, download, and ambient policy checks. The initial Fire release must not report Kids Watch Activity or watchlist data to Amazon. A Fire TV profile is not assumed to represent a Duskcue household profile; the privacy-safe default is app-local only until a standard profile has both Duskcue authorization and Fire TV customer opt-in.

## Amazon Integration Gates

### Watch Activity and Content Personalization — Task 2

The adapter remains unavailable unless all of these conditions are true:

1. Amazon has granted the app/SDK account the required integration access and the integration SDK reports availability on the device.
2. The active Duskcue profile is standard, not ambient, and has passed server authorization for the selected content.
3. The active Fire TV customer has opted in to Content Personalization.
4. The media item has an exact, currently accepted Amazon catalog content ID.
5. The server has provided the profile-specific opaque Fire profile key and the app has a truthful duration/position/state.

When enabled, the adapter reports an active event on start, pause, resume, seek, exit/completion, and at the documented 60-second cadence. It sends off-device events only as accurately timestamped historical data after a server sync; it never fabricates a current-device active event. The app must never send a zero-position state merely because its resume state has not loaded, nor infer an Amazon row, ordering, or cross-device refresh as a guaranteed user-visible outcome. Amazon documents that Continue Watching may refresh asynchronously and catalog IDs must exactly match the accepted catalog.

### Stable ID strategy — Task 3

`platform_content_id` remains Duskcue's stable internal cross-platform identifier. It is neither an Amazon CDF/catalog ID nor permission to publish a user's private catalog. Task 3 adds a versioned, server-owned mapping for distributable items only:

| Value | Owner | Permitted use |
|---|---|---|
| `duskcue:movie:<uuid>` / `duskcue:episode:<uuid>` | Duskcue | Authenticated in-app routing and local adapter correlation. |
| Opaque Fire profile key | Server | Fire SDK `app_internal` profile namespace after the integration gates pass. It has no reversibility to a Duskcue profile ID. |
| Exact Amazon catalog/CDF ID | Amazon-accepted catalog mapping | Watch Activity and Amazon catalog launch only. No mapping means no Amazon event. |

The mapping will have no fallback derived from a title, path, raw UUID, or private source. It is versioned/auditable, withdrawn when catalog eligibility or rights change, and never exposed to an unauthorized client. The shared TV adapter fixture expresses this `not yet cataloged` baseline so future code cannot confuse the two identifier domains.

### Catalog, deep links, and voice — Task 4

An Amazon catalog launch must enter the Fire app, require or resume normal Duskcue account authentication, select the correct Duskcue profile under the existing server rules, resolve the original content through `/api/v1/tv/resolve/...`, and start only after current authorization, library/rating policy, availability, and resume position are rechecked. It must not trust an inbound catalog ID, cached URL, prior permission, or launcher-provided position.

Task 4 is blocked until Amazon partner/onboarding approval, valid distribution rights, accepted staging catalog, production admission, and Amazon-provided launcher/deep-link configuration exist. If those gates are absent, the Fire app remains fully supported with app-local search/browse and Duskcue-owned deep links; it makes no Alexa/global-search or Fire home-row promise.

## Delivery Sequence and Evidence

| Task | Deliverable | Must not claim before evidence |
|---|---|---|
| 0 | This design, corrected shared fixture, CI drift coverage, and project tracking. | Fire app availability, Amazon SDK availability, catalog admission, or hardware support. |
| 1 | Fire OS app target, shared-neutral extraction, Appstore-ready identity placeholders, Media3/media-button behavior, profile gate, app-local feed, physical Fire TV validation plan. | Compatibility with every Fire device or Fire OS release. |
| 2 | Feature-gated Watch Activity adapter and fixture/unit coverage. | Personalization participation or visible Continue Watching until a customer-opted-in physical-device validation passes. |
| 3 | Auditable accepted-catalog mapping and eligibility/revocation tests. | That arbitrary/private Duskcue media can appear in Amazon Catalog. |
| 4 | Partner-approved EMBER export and authenticated Amazon launch path, if admitted. | Public catalog/global voice/search availability in unapproved territories. |
| 5 | Separate Vega proposal/target only if triggered. | Android/Firebase code compatibility or coverage of all Fire TV hardware. |

Required physical evidence before a Fire TV release includes Appstore install/update, Fire OS version/device family, profile-picker and Kids boundary, local playback/resume, audio focus/media keys, D-pad/Back/Menu/FF/Rewind, VoiceView/captions, standby/resume, codec/HDR/audio behavior claimed by the release, diagnostics redaction, and (only if enabled) customer-opted-in Watch Activity and catalog launch behavior. No Fire TV emulator substitutes for this evidence.

## Related Contracts

- [TV_PLATFORM_SURFACES.md](TV_PLATFORM_SURFACES.md) defines the server-owned feed and launch-time authorization boundary.
- [PROFILES_AND_AMBIENT_CHANNELS.md](PROFILES_AND_AMBIENT_CHANNELS.md) defines profile selection, remembered-device, Kids, and ambient rules.
- [CLIENT_PLATFORM_READINESS.md](CLIENT_PLATFORM_READINESS.md) defines Phase 16d shared contract/release practices.
- [CLIENT_DEVICE_LAB.md](CLIENT_DEVICE_LAB.md) defines the shared device-lab evidence model.
- [platform-adapter-mappings.json](../api/fixtures/tv/v1/platform-adapter-mappings.json) is the fixture-checked pre-catalog Fire adapter baseline.
