# Fire TV Implementation Design

## Outcome

This document is the authoritative Phase 18 design decision. Duskcue will build a dedicated Android-based Fire TV application that reuses platform-neutral Kotlin logic from the Android TV client, but has its own application identity, build target, launcher integration, release path, and Amazon adapter boundary. It will not ship AndroidX Watch Next APIs, Google Play assumptions, or a synthetic Amazon catalog into the Fire TV target.

Tasks 0–3 are complete: the first deliverable is a fully usable Duskcue Fire OS app at `clients/tv/fire/`, with profile-gated app-local browsing and Media3 playback, authenticated Duskcue deep-link handoff, remote/focus/accessibility support, diagnostics, a Fire-specific server playback profile, CI build evidence, and a default-disabled Watch Activity adapter. Amazon Content Personalization, Watch Activity, and catalog discovery are opt-in integrations with independent technical and partner gates. Vega is a separate, non-Android product track.

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

Task 1 creates `clients/tv/fire/` with the independent Appstore application ID `com.duskcue.firetv`. It initially consumes selected `com.duskcue.tv` source directories through explicit Gradle source sets so the existing neutral Kotlin path is compiled by both targets while Android TV-only files remain out of the Fire build. The Kotlin namespace does not determine the Appstore application identity; extract a standalone shared module only when the first divergent implementation makes the ownership boundary clearer. It may reuse:

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

### Task 2 research and implementation decision — October 3, 2026

Rechecked the official [SDK setup](https://developer.amazon.com/docs/fire-tv/get-started-with-firetv-integration-sdk.html), [Watch Activity](https://developer.amazon.com/docs/fire-tv/watch-activity.html), and [customer opt-in guidance](https://developer.amazon.com/docs/fire-tv/introduction-content-personalization.html). The downloaded SDK API was inspected before choosing builder methods; the setup page's Kotlin example differs from the published JAR, so the implementation uses the JAR's verified fluent builders.

| Option | Benefit | Cost | Decision |
|---|---|---|---|
| Compile-only official SDK with an optional system library | Typed calls, no Amazon implementation in the APK, consistent with Amazon's setup guidance | Requires a checksum-pinned build download and a feature check before SDK access | Selected |
| Reflection against device classes | No compile dependency | Weak API checking and harder failure diagnosis | Rejected |
| Package the SDK implementation in the application | Direct class availability | Conflicts with Amazon's system-library integration model | Rejected |

The build downloads the official SDK archive into ignored build storage, verifies SHA-256 `f3094973bbb18b5a58807ad043d31055ade6a2f6646bcae54a4d1022b9cdf593`, and exposes only its compile-time JAR. The manifest declares the optional `com.amazon.tv.developer.sdk.content` library and `USE_SDK` permission. Builds default `duskcueFireWatchActivityEnabled` to false. Enabling that flag alone never authorizes an event.

A composable playback observer receives loaded content position/duration, play/pause changes, seeks, periodic samples, and exit before Media3 is cleared. A Fire-only reporter converts eligible samples to active SDK events with `cdf_id` content and `app_internal` profile namespaces. It waits for the first rendered frame and a valid content duration, sends the truthful loaded resume position, uses content position during interstitials, and reports every 60 seconds while the player is open even when paused. Completion and explicit/error/service exit use `EXIT`; no active event is queued or replayed after a failure.

Authorization is an in-memory, expiring playback-session binding supplied by the server integration boundary. Feature enablement, Amazon integration/device availability, customer opt-in, standard profile, completed profile selection, interactive playback, current content authorization, an exact accepted catalog ID, and an opaque Fire profile key must all pass. Unknown consent is denied. Amazon's public SDK does not document a customer-opt-in query; the provider must obtain positive consent through the approved integration, and Amazon's service retains the platform opt-out boundary. Do not infer consent from library availability or TV publication settings.

Profile/account/server/session cleanup clears the reporter before stopping playback, preventing an old profile's exit event after its scope is revoked. Reporter bindings, payloads, catalog IDs, and profile keys never enter preferences, logs, diagnostics, or a retry queue. Task 3 supplies the auditable accepted-catalog and profile-key source. The production source queries it only after positive device consent; unknown consent remains denied. Off-device historical synchronization requires a real timestamped server history source and is deferred rather than represented as active local playback.

Task 2 verification: Fire `:app:testDebugUnitTest :app:lintDebug :app:assembleDebug` passes with 10 tests, including nine reporter tests; the same Android TV commands pass with 42 tests. Both lint reports have zero errors and only existing dependency/resource/manifest/API-style warnings. Shared client-contract, CI/smoke-plan, playback, auth, TV/deep-link/surface, accessibility, and diagnostics checks pass. Inspection of the generated Fire APK's DEX class definitions confirms the Duskcue SDK bridge is present while Amazon implementation classes and AndroidX `tvprovider` are absent. Physical customer-opted-in Continue Watching validation remains open.

### Watch Activity and Content Personalization — Task 2

The adapter remains unavailable unless all of these conditions are true:

1. Amazon has granted the app/SDK account the required integration access and the integration SDK reports availability on the device.
2. The active Duskcue profile is standard, not ambient, and has passed server authorization for the selected content.
3. The active Fire TV customer has opted in to Content Personalization.
4. The media item has an exact, currently accepted Amazon catalog content ID.
5. The server has provided the profile-specific opaque Fire profile key and the app has a truthful duration/position/state.

When enabled, the adapter reports an active event on start, pause, resume, seek, exit/completion, and at the documented 60-second cadence. It sends off-device events only as accurately timestamped historical data after a server sync; it never fabricates a current-device active event. The app must never send a zero-position state merely because its resume state has not loaded, nor infer an Amazon row, ordering, or cross-device refresh as a guaranteed user-visible outcome. Amazon documents that Continue Watching may refresh asynchronously and catalog IDs must exactly match the accepted catalog.

### Stable ID strategy — Task 3

Task 3 research rechecked October 3, 2026: [EMBER best practices](https://developer.amazon.com/docs/catalog/ember-best-practices.html) require durable identifiers, permanent retirement rather than reuse, and staging/acceptance before production. The [SDK identifier contract](https://developer.amazon.com/docs/fire-tv/get-started-with-firetv-integration-sdk.html) uses the exact accepted program ID with `cdf_id`, and a consistent, non-identifying profile key with `app_internal`.

| Option | Benefit | Cost | Decision |
|---|---|---|---|
| Derive an Amazon ID from a Duskcue UUID/title | No registry | Cannot prove catalog acceptance and can confuse private library identity with Amazon identity | Rejected; remove the legacy synthetic Amazon encoder |
| Store accepted mappings in media metadata | Small schema change | Weak uniqueness, permanent retirement, concurrency, and audit semantics | Rejected |
| Dedicated versioned registry and random per-profile keys | Exact identity, immutable binding, retirement, revision checks, auditable rights/acceptance changes | Adds a migration and scoped API | Selected |

The registry binds a catalog reference and exact accepted content ID to the original movie/episode identity. Deleting local media leaves a retired tombstone; a content ID cannot be rebound to another item. Admin writes require current media access, explicit acceptance/distribution-rights references, a future rights expiry, and the current revision for updates. Each successful registration/update records a transactional revision snapshot. Disabling a mapping, expiry, deleted media, missing catalog admission, user TV-publication opt-out, profile-selection state, Kids scope, ambient/stopped playback, or revoked media access prevents authorization.

`integrations.fire_tv` defaults disabled and carries only the selected catalog reference and an operator-owned partner-approval reference. Those references are evidence locators, not credentials or a claim that Amazon has admitted Duskcue. Admin registration uses `/api/v1/tv/fire/catalog/{platform_content_id}`; playback reads use `/api/v1/tv/fire/playback/{session_id}/authorization`. The latter is private/no-store, checks the authenticated owner and active profile against an active interactive play session and current media policy, and returns an opaque random 32-byte profile key only with an eligible mapping. Keys are stable across that profile's devices and unrelated to account/profile UUIDs or titles. Positive customer consent remains a separate device integration boundary; the server never infers it from publication preferences.

Playback authorization is short-lived and must be refreshed while playback continues. Client refreshes must discard replies from an old session/profile scope, clear a denied binding immediately, and sample current Media3 position when sending rather than replay an older event after network completion. The physical Amazon admission/consent path remains a release gate, while registry identity, concurrency, access denial, expiry, withdrawal, and tombstone behavior are repository-verifiable.

Task 3 verification: disposable PostgreSQL 18 applies every migration and passes the registry contract for default/disabled integration, missing/stopped/ambient playback, publication opt-out, unhealthy files, library denial, profile-selection/Kids denial, different profile keys, admin restrictions, immutable accepted identity, competing revision writes, withdrawal, rights expiry, and deletion tombstones that cannot be reused. Fire's 15 tests and Android TV's 42 tests, both lint/debug APK builds, `cargo fmt --all -- --check`, `cargo clippy -p duskcue --all-targets`, and the shared API/auth/playback/TV conformance checks pass. Clippy reports only existing unrelated diagnostics. The full server suite passed with 776 tests; the separately enabled database contract also passes. The verification uncovered and corrected pre-seed audit partitions and the default-language full-text trigger, documented in [DATABASE.md](DATABASE.md) and [SEARCH.md](SEARCH.md).

`platform_content_id` remains Duskcue's stable internal cross-platform identifier. It is neither an Amazon CDF/catalog ID nor permission to publish a user's private catalog. Task 3 adds a versioned, server-owned mapping for distributable items only:

| Value | Owner | Permitted use |
|---|---|---|
| `duskcue:movie:<uuid>` / `duskcue:episode:<uuid>` | Duskcue | Authenticated in-app routing and local adapter correlation. |
| Opaque Fire profile key | Server | Fire SDK `app_internal` profile namespace after the integration gates pass. It has no reversibility to a Duskcue profile ID. |
| Exact Amazon catalog/CDF ID | Amazon-accepted catalog mapping | Watch Activity and Amazon catalog launch only. No mapping means no Amazon event. |

The implemented mapping has no fallback derived from a title, path, raw UUID, or private source. It is versioned/auditable, withdrawn when catalog eligibility or rights change, and never exposed to an unauthorized client. The shared TV adapter fixture expresses this `not yet cataloged` baseline so future code cannot confuse the two identifier domains.

### Catalog, deep links, and voice — Task 4

Task 4 research rechecked October 3, 2026: Amazon's [launcher integration](https://developer.amazon.com/docs/catalog/integrate-with-launcher.html) distinguishes sign-in and playback intents and requires current entitlement handling. [EMBER onboarding](https://developer.amazon.com/docs/catalog/ember-catalog-integration-overview.html) and [production admission](https://developer.amazon.com/docs/catalog/upload-your-catalog-production.html) remain limited to approved partners. No approval, accepted staging catalog, or approved launcher configuration has been verified in this workspace.

The Duskcue-owned authenticated launch path is independently deliverable. Review found that its resolve/start requests could complete after a profile switch or a replacement intent. A shared composable launch coordinator binds each attempt to a monotonically versioned account/profile scope and request generation, rechecks after each server call, discards known abandoned server sessions, and uses only the newly resolved server resume position. The runtime checks the same expected scope across Fire authorization preparation before handing a stream to Media3. No inbound URL carries a bearer, stream URL, PIN, or resume position.

An abandoned launch uses the optional `cancelled_before_start` flag on the authenticated playback-stop API. The server accepts it only for the caller's session with no heartbeat and zero recorded position, or for an already-cancelled session. Cancellation records a stopped diagnostic session and releases any transcode, while leaving history, play count, resume, and TV surfaces unchanged. A normally stopped or already-progressed session cannot be relabeled as an unplayed cancellation. This preserves existing stop behavior for all requests that omit the flag.

The ordinary-stop comparison in the disposable test also exposed three stale parameter references in the profile-scoped watch-history upsert: the conflict branch used the media UUID as a Boolean, the watched flag as a position, and the position as a media-file UUID. The correction binds watched state, resume, and file identity to the same parameters as the insert branch. The database test verifies both history-preserving cancellation and ordinary stop persistence.

| Option | Benefit | Cost | Decision |
|---|---|---|---|
| Keep launch orchestration inside the UI controller | Small local edits | Hard to exercise real suspended-response and scope-change behavior | Rejected |
| Shared launch coordinator with explicit server/runtime boundaries | Testable authentication, profile gating, resume, revocation, and replacement semantics in both targets | Adds a small shared service and scope version | Selected |
| Publish an EMBER feed before admission is confirmed | Early export code | Cannot validate partner schema/configuration, distribution rights, ingestion, or private-library exposure | Deferred until the required evidence exists |

An Amazon catalog launch must enter the Fire app, require or resume normal Duskcue account authentication, select the correct Duskcue profile under the existing server rules, resolve the original content through `/api/v1/tv/resolve/...`, and start only after current authorization, library/rating policy, availability, and resume position are rechecked. It must not trust an inbound catalog ID, cached URL, prior permission, or launcher-provided position.

The Amazon-specific exporter and launcher portion of Task 4 awaits confirmed partner onboarding, distributable content with valid rights, and the applicable schema/launcher requirements. Accepted staging results and production admission are subsequent publication/release gates. Without those prerequisites, the Fire app supports app-local search/browse and authenticated Duskcue-owned deep links; it makes no Alexa/global-search or Fire home-row promise.

Task 4 repository verification: Fire has 37 passing tests and Android TV has 55, including the shared eleven authenticated-launch tests and serialized profile-snapshot tests. Both lint/debug APK builds pass with zero lint errors and existing warnings. The full server suite passed with 776 tests; the separately enabled PostgreSQL contract passes current cancellation ownership, repeat cancellation, cancellation after progress/seek/normal stop denial, cancelled-session ordinary-stop denial, unchanged resume/play count, and ordinary-stop media-file persistence. Formatting, clippy, and shared contract/conformance checks pass; clippy retains existing unrelated diagnostics. A local ADB inventory found zero connected ready devices, so no physical launch, opted-in Watch Activity, or Continue Watching result is claimed.

## Separate Vega Track — Task 5

Research refreshed October 3, 2026. Amazon's [Vega SDK overview](https://www.developer.amazon.com/apps-and-games/sdks) identifies Vega as a React Native platform for a separate generation of Fire TV devices. The [Vega build guide](https://www.developer.amazon.com/docs/vega/0.23/build-an-app) supports a virtual device for initial development and requires physical Fire TV testing before Appstore submission. Amazon's [native Fire OS porting workflow](https://developer.amazon.com/docs/adbt/port-fire-os-app-to-vega), updated September 30, describes Kotlin/Java to React Native migration as a rewrite requiring manual review; generated output is not proof of API compatibility.

**Tracking complete; implementation not triggered.** The current release target is Android-based Fire OS 7+/API 28. No release requirement for a Vega device has been established. The Fire APK cannot be counted as a Vega deliverable. A future Vega project is reserved separately at `clients/tv/fire-vega/`; no scaffold, SDK credential, build lane, package identity, or hardware-support claim is created by this tracking decision.

| Option | Benefit | Cost | Recommendation |
|---|---|---|---|
| Separate React Native for Vega client | Native platform lifecycle, playback, and remote integration can be validated directly | New UI/runtime implementation and SDK-specific dependencies | Preferred if Vega becomes required |
| Vega WebView wrapper | Greater reuse of existing web views | Native playback, credential storage, focus, lifecycle, and platform integration still need proof | Evaluate only against the same functional gates |
| Reuse the Fire OS APK | Existing binary | Android build/runtime APIs do not constitute a Vega application | Unsupported |

Trigger implementation when the release explicitly includes a device whose current official model/OS specification requires Vega, with a supported SDK/toolchain and access to physical validation. Recheck the model, territory, SDK version, and Appstore distribution requirements at that point; an Amazon model name alone does not prove its operating system.

The separate implementation backlog is:

1. Confirm target devices, SDK/toolchain versions, distribution feasibility, and current official API contracts; record the decision before generating a project.
2. Create the independent Vega target and implement server selection, secure credentials, device linking, profile selection/remembering, parent unlock, and account/profile/session cleanup using Vega APIs.
3. Implement native playback, fresh server resume, audio/caption selection, remote focus/media controls, heartbeat/stop/cancellation, lifecycle interruption, and diagnostics redaction. Android Keystore, Compose, Media3, and the Android Amazon SDK JAR are not reused as Vega implementations.
4. Reuse server-owned API contracts, fixtures, exact accepted catalog mappings, and eligibility rules. Implement Vega launcher/personalization adapters only against the approved Vega interface, keeping customer consent, Kids/ambient exclusion, and distribution-rights checks intact.
5. Add a distinct build/test lane and virtual-device smoke checks, then physical playback/remote/accessibility/standby and Appstore evidence before claiming support. Pin actual SDK/package versions when implementation begins.

## Delivery Sequence and Evidence

Current completion audit: Tasks 0–3 and Task 5 tracking are complete. Task 4's authenticated Duskcue-owned path is implemented and repository-verified; Amazon catalog/EMBER integration awaits partner requirements. The phase's live Watch Activity/Continue Watching and deep-link playback evidence is missing. No repository test substitutes for those physical observations, so the phase is not marked complete. The final Fire APK class-definition audit confirms the SDK bridge and launch coordinator are present with no Amazon implementation or Android TV provider classes packaged.

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
