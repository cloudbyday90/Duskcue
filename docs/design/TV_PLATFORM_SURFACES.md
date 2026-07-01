# TV Platform Surfaces

## Purpose

Duskcue should publish continue-watching, next-episode, and recommendation items into TV operating-system surfaces where the platform allows it, then deep-link the user back into Duskcue playback at the correct item and resume position.

This document defines the cross-platform design. Each TV platform is an adapter over the same Duskcue server API, not a separate source of truth.

## Research Summary

### Phase 16b Task 0 Refresh — June 30, 2026

The Phase 16b research refresh rechecked current official platform documentation for Android TV / Google TV, Fire TV, Roku, Samsung Tizen, LG webOS, Apple TV / tvOS, Xbox/UWP, and partner-gated ecosystems before implementation starts.

The core finding is unchanged: Duskcue needs a platform-neutral, user-scoped TV surface feed and deep-link resolver owned by the server, with adapters translating that feed into platform-specific launcher, search, catalog, or app-local surfaces.

Updated platform posture:

| Platform class | Official-source signal | Duskcue decision |
|---|---|---|
| Row-owned launcher surfaces | Android TV Watch Next and tvOS Top Shelf expose app-owned publication surfaces. | Build stable server feed IDs, fresh resume data, artwork URLs, and small curated sections; clients publish only useful continue/next-up rows and remove stale entries. |
| Event-driven activity surfaces | Fire TV Watch Activity and Content Personalization depend on app-reported playback events, with catalog integration gated by partner access. | Treat Fire TV as playback-event reporting first, not as a direct list-publishing surface. Server feed still powers app-local rows and playback resume. |
| Feed plus deep-link surfaces | Roku Search and Direct to Play require stable content IDs, correct deep-link handling, and bookmark/resume behavior for certification. | Make `platform_content_id` reversible and platform-safe; direct-to-play resolve must fetch current Duskcue resume before playback. |
| Packaged web TV apps | Samsung Tizen and LG webOS provide packaged app models, native media APIs/lifecycle hooks, and model-year/web-engine constraints. | Do not reuse the generic browser UI as-is. Build platform packages with TV focus behavior, native playback wrappers, app-local rows, and launch/relaunch handling. |
| Console media apps | Xbox UWP media-app docs provide native media APIs, URI activation, SMTC, and explicit 4K/HDR tradeoffs. | Treat Xbox as a console adapter over the same TV feed, with native playback and capability reporting rather than a web wrapper by default. |
| Partner-gated platforms | VIZIO and PlayStation public materials route developers through partner portals. | Prepare stable IDs and app-local contracts now; implement only after portal access confirms self-hosted media-app viability. |

Pros and cons of the selected server-feed approach:

| Option | Pros | Cons | Decision |
|---|---|---|---|
| Server-owned TV surface feed plus adapter contracts | Keeps resume/access/recommendation truth in one place; supports every platform class; lets future clients share fixtures and contract tests. | Requires careful shaping for platform-specific IDs, cache headers, and privacy. | Selected for Phase 16b. |
| Platform-by-platform bespoke APIs | Each client could exactly match its vendor API. | Duplicates logic, risks inconsistent resume state, and makes BOLA/cache/test coverage harder. | Rejected. |
| Client-only launcher state | Fast to prototype on Android TV or tvOS. | Cannot reliably reflect web/mobile playback, access revocation, metadata refreshes, or cross-device completion. | Rejected except for platform-local ID mappings. |

Final Phase 16b recommendation:

1. Build the server domain, stable platform IDs, user-scoped feed, deep-link resolver, cache/private ETag behavior, diagnostics, settings, and `tv_surface_changed` events before any platform app.
2. Add fixtures and a reference harness so Android TV, Fire TV, Roku, Tizen, webOS, tvOS, Xbox, and partner-gated clients consume the same contract.
3. Keep launcher publication local to native clients. The Rust server emits facts and refresh hints; it does not call TV operating-system APIs directly.
4. Treat certification, store visibility, partner feeds, signing, and physical hardware validation as platform-phase and release-gate work, not Phase 16b server prerequisites.

### Official Sources

| Source | Finding |
|---|---|
| [Android TV Watch Next](https://developer.android.com/training/tv/discovery/watch-next-add-programs) | Android TV apps can add, update, and remove programs in the system Watch Next channel for unfinished content, next episodes, new episodes, and watchlist items. |
| [Android TV channel guidelines](https://developer.android.com/training/tv/discovery/guidelines-app-developers) | Watch Next behavior is constrained: add only useful continuation items, keep items fresh, remove completed items, avoid clutter, and use correct program types. |
| [AndroidX `WatchNextProgram`](https://developer.android.com/reference/androidx/tvprovider/media/tv/WatchNextProgram) | `WatchNextProgram` is the AndroidX builder/API surface for publishing Watch Next rows from a TV app. |
| [Media3 session playback control](https://developer.android.com/media/media3/session/control-playback) | Android media apps expose playback to system and external controls through `MediaSession`; TV clients should not treat playback as an isolated in-app-only concern. |
| [Fire TV Watch Activity](https://developer.amazon.com/docs/fire-tv/watch-activity.html) | Fire TV Continue Watching is driven by playback events from the app: start/progress/pause/resume/exit, seek updates, and 60-second in-player progress reports. |
| [Fire TV Content Personalization](https://developer.amazon.com/docs/fire-tv/introduction-content-personalization.html) | Fire TV uses shared watch activity for Continue Watching and may use it for personalized recommendation rows. |
| [Fire TV EMBER catalog integration](https://developer.amazon.com/docs/catalog/ember-catalog-integration-overview.html) | New Fire TV catalog integrations use EMBER metadata and are available to select partners; catalog IDs must match playback/deep-link IDs. |
| [Fire TV deep-link verification](https://developer.amazon.com/docs/catalog/verify-deep-links-from-the-catalog.html) | Fire TV catalog deep links should start playback directly for authenticated users and route unauthenticated users through sign-in before playback. |
| [Vega Content Personalization](https://developer.amazon.com/docs/vega/0.21/intro-to-vega-content-personalization.html) | Amazon's newer Vega path exposes similar Continue Watching and recommendation concepts through Vega Content Personalization; documentation is still marked open beta. |
| [Roku deep linking](https://developer.roku.com/dev/docs/implementing-deep-linking) | Public Roku video apps must support `contentId` and `mediaType` deep links for certification; movies and episodes launch directly into playback. |
| [Roku Search feed](https://developer.roku.com/dev/docs/search-feed) | Roku Search uses a JSON catalog feed containing metadata, artwork, IDs, availability, and other content fields. |
| [Roku Direct to Play](https://developer.roku.com/dev/docs/direct-to-play) | Public Roku apps participating in Roku Search must support Direct to Play, bookmarks, auth status events, and no resume/start-over interstitial for voice-launched playback. |
| [Samsung Smart Hub Preview](https://developer.samsung.com/smarttv/develop/guides/smart-hub-preview/smart-hub-preview.html) | Samsung Tizen can show public or personalized preview tiles with deep links when the user focuses the app icon in Smart Hub. |
| [Samsung Personal Preview](https://developer.samsung.com/smarttv/develop/guides/smart-hub-preview/implementing-personal-preview.html) | Personalized Smart Hub Preview content uses a foreground application plus a background service application that provides personalized preview data. |
| [Samsung AVPlay](https://developer.samsung.com/smarttv/develop/guides/multimedia/media-playback/using-avplay.html) | Samsung TV web apps use AVPlay for native media playback controls such as seek, rate, and track switching. |
| [Samsung Adaptive Streaming](https://developer.samsung.com/smarttv/develop/guides/multimedia/adaptive-streaming.html) | AVPlay supports adaptive streaming engines including HLS `.m3u8`, which matches Duskcue's streaming output. |
| [Samsung Subtitles](https://developer.samsung.com/smarttv/develop/guides/multimedia/subtitles.html) | AVPlay subtitle support has platform-specific constraints; remote external subtitles may need local download before playback. |
| [Samsung TV quick-start](https://developer.samsung.com/smarttv/develop/getting-started/quick-start-guide.html) | Samsung TV apps are packaged/signed Tizen web applications developed with the Samsung TV SDK and tested on emulator or real devices. |
| [LG webOS app lifecycle](https://webostv.developer.lge.com/develop/guides/app-lifecycle-management) | webOS TV apps receive launch and relaunch events with parameters; deep-link-like behavior should be handled through app launch state. |
| [LG webOS Application Manager](https://webostv.developer.lge.com/develop/references/application-manager) | webOS apps can be launched with JSON parameters, and apps should handle repeated launch requests through relaunch handling. |
| [LG webOS Web App Types](https://webostv.developer.lge.com/develop/getting-started/web-app-types) | webOS TV apps can be packaged basic web apps or hosted web apps; Duskcue should use a packaged shell with server-backed content. |
| [LG webOS mediaOption resume](https://webostv.developer.lge.com/develop/guides/resuming-media-with-mediaoption) | webOS media elements support `mediaOption` playback data such as start position, which maps directly to Duskcue resume state. |
| [LG webOS streaming protocol and DRM](https://webostv.developer.lge.com/develop/specifications/streaming-protocol-drm) | webOS TV support is model/platform dependent; unlisted streaming and DRM formats are unsupported or not recommended. |
| [LG webOS Web API and Web Engine](https://webostv.developer.lge.com/develop/specifications/web-api-and-web-engine) | webOS TV web engine versions vary significantly by platform release, so Duskcue must define minimum supported versions and test older engines. |
| [LG webOS Studio](https://webostv.developer.lge.com/develop/tools/webos-studio-dev-guide) | LG recommends webOS Studio for current development tooling. |
| [LG webOS App Approval Process](https://webostv.developer.lge.com/distribute/app-approval-process) | Public distribution requires LG app approval; certification and device compatibility testing are release tasks. |
| [Sony Google TV / Android TV support](https://www.sony.com/electronics/support/articles/00200248) | Sony classifies BRAVIA software by Google TV / Android TV / other TV; Sony should be treated as Android TV / Google TV hardware validation unless a model-specific issue requires special handling. |
| [Sony app installation](https://www.sony.com/electronics/support/articles/00147386) | Sony Google TV / Android TV app installation goes through Google Play on the TV, reinforcing that Duskcue's Android TV distribution path covers supported Sony BRAVIA models. |
| [Sony supported apps](https://www.sony.com/electronics/support/articles/00114472) | Sony notes that Google Play on Android TV only displays apps supported by the TV; Duskcue must meet Android TV compatibility and store listing requirements for Sony visibility. |
| [Apple TV Services / Top Shelf](https://developer.apple.com/documentation/tvservices) | tvOS apps can provide Top Shelf content through an app extension, making Apple TV the next platform with a meaningful launcher-like surface to research. |
| [Apple Top Shelf extension](https://developer.apple.com/documentation/TVServices/building-a-full-screen-top-shelf-extension) | A tvOS app can provide full-screen Top Shelf content and a description through a Top Shelf app extension. |
| [Apple `TVTopShelfAction`](https://developer.apple.com/documentation/tvservices/tvtopshelfaction) | Top Shelf items can provide actions with URLs for tvOS to open when the user selects an item. |
| [Apple AVKit tvOS playback](https://developer.apple.com/documentation/avkit/customizing-the-tvos-playback-experience) | AVKit is Apple's native playback UI path for tvOS video apps and supports customization of the tvOS playback experience. |
| [Apple tvOS Universal Links](https://developer.apple.com/documentation/xcode/allowing-apps-and-websites-to-link-to-your-content) | tvOS can deep-link into app content through Universal Links, relevant to Duskcue playback entry from platform surfaces. |
| [Apple associated domains](https://developer.apple.com/documentation/xcode/supporting-associated-domains) | Universal Links require associated domains, so Duskcue deployments need a stable HTTPS base URL and associated-domain configuration for tvOS deep links. |
| [Apple TV app and Universal Search integration](https://developer.apple.com/videos/play/tech-talks/508/) | Apple TV app / Universal Search integration uses metadata feeds and service presentation controls; this is separate from the app-owned Top Shelf surface. |
| [Apple Video Partner Program](https://developer.apple.com/programs/video-partner/resources/) | Apple Video Partner Program participants integrate technologies such as Universal Search and AirPlay 2; Duskcue should treat Apple TV app/Universal Search as release-gated partner work. |
| [VIZIO content partners](https://www.vizio.com/en/content-partners) | VIZIO app distribution is partner-led; Duskcue should evaluate it after open or better-documented platforms. |
| [VIZIO Developer Portal](https://developer.vizio.com/) | VIZIO's developer documentation is portal-gated, so exact app, playback, and launcher requirements require partner/developer access. |
| [VIZIO Partner Portal APIs](https://api.developer.external.plat.vizio.com/apis/) | VIZIO exposes partner APIs for subscription/entitlement style integrations through a key-request portal, reinforcing the partner-gated integration model. |
| [VIZIO Platform+ developer program](https://platformplus.vizio.com/news/vizio-welcomes-developers-through-first-ever-conference-and-preferred-developer-program) | VIZIO describes a Preferred Developer Program and a Developer Portal for app partners, including APIs/documentation and onboarding support. |
| [PlayStation Partners](https://partners.playstation.net/) | PlayStation development is partner-gated; Duskcue should treat PlayStation as a later partnership/platform-access investigation. |
| [PS5 entertainment](https://www.playstation.com/en-us/ps5/ps5-entertainment/) | PlayStation positions PS5 as a media device with a dedicated Media home and major entertainment apps, confirming user demand but not public self-service app development. |
| [PS5 media experience launch post](https://blog.playstation.com/2020/10/22/new-media-experience-and-top-entertainment-streaming-apps-coming-to-ps5/) | Sony describes a dedicated PS5 Media space, entertainment apps, and a Media Remote; Duskcue should treat media UX and remote behavior as first-class if a partner path is available. |
| [Xbox media app development](https://learn.microsoft.com/en-us/windows/uwp/apps-for-xbox/development-options) | Xbox media apps are built and packaged through UWP tooling, can be deployed through Visual Studio/Device Portal, and can be manually side-loaded for testing. |
| [Xbox media app architecture](https://learn.microsoft.com/en-us/windows/uwp/apps-for-xbox/application-architecture) | Microsoft documents two Xbox media-app patterns: a WebView-hosted app or a native UWP app using XAML/C#/C++ with MediaElement/MediaPlayer APIs. |
| [Xbox 4K video playback](https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/hevc-xbox) | Xbox UWP apps can enable 4K/HDR10 playback with the `hevcPlayback` capability; this changes memory allocation and background behavior. |
| [Xbox supported technologies](https://learn.microsoft.com/en-us/windows/uwp/apps-for-xbox/supported-technologies) | Xbox UWP media apps have platform-specific support constraints, including PlayReady as the DRM path. |
| [Windows media playback controls](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/media-playback) | `MediaPlayerElement` exposes programmatic media controls and events for Windows/UWP-style media playback. |
| [Windows URI activation](https://learn.microsoft.com/en-us/windows/apps/develop/launch/handle-uri-activation) | UWP apps can register URI protocols and handle protocol activation, which maps to Duskcue deep-link playback. |
| [VIDAA homepage](https://www.vidaa.com/) | VIDAA has meaningful Hisense/Toshiba/Sharp reach, but public developer documentation appears limited; Duskcue should treat it as a partnership research target. |

### Recommendation

Build a **Duskcue TV surface feed API** plus platform-specific TV adapters.

The server decides which media items belong in continue-watching, next-up, and recommendation surfaces. Native TV clients decide how to publish those items to their platform launcher and how to deep-link back into Duskcue playback.

This keeps resume state synchronized across web, mobile, desktop, and TV while respecting vendor-specific launcher APIs.

## Cross-Platform Consistency Contract

Duskcue should feel like one product across every living-room platform, even when the implementation language, playback engine, packaging workflow, certification path, and platform surface APIs differ.

Every TV/console client must preserve:

- the same information architecture: Home, Search, Libraries, Collections, Settings, user/profile controls, and playback
- the same row semantics and priority order: Continue Watching, Next Up, New Episodes, Recommendations
- the same resume behavior: always fetch the latest server resume state before playback
- the same device-linking and server-selection model
- the same core playback controls: play/pause, seek, skip segment where available, audio tracks, subtitles, quality/status, and stop/back behavior
- the same access-control behavior: revalidate auth and library access before playback, including deep-link launches
- the same visual content hierarchy: title, subtitle, poster/backdrop, progress, runtime, season/episode metadata, and availability state
- the same error vocabulary and recovery paths, adapted to platform UI conventions
- the same privacy posture: no platform launcher item should leak another Duskcue user's private watch state

Platform implementations may differ in:

- client technology and language
- packaging, signing, store, or partner approval process
- playback API and codec/HDR/audio capability profile
- deep-link format
- launcher or home-screen surface support
- local cache and platform content ID storage
- certification test process and required hardware matrix

### Living-Room UX Contract

All TV and console clients must open into the product experience, not a marketing screen. The first authenticated view is a living-room home surface backed by `GET /api/v1/users/me/tv-surface`, with app-local navigation to Search, Libraries, Collections, Settings, and profile/server controls.

Row order is fixed unless a platform certification rule requires otherwise:

1. Continue Watching
2. Next Up
3. New Episodes
4. Recommendations

Rows with no items should keep their position only when the platform UI benefits from stable landmarks; otherwise they may be omitted. Empty messages must use bounded client strings keyed by server `empty_reason` values such as `no_matching_items`, `limit_reached`, `tv_publication_disabled`, `tv_platform_disabled`, and `tv_section_disabled`. Do not show raw server errors, SQL details, file paths, or provider payloads in empty states.

Row labels are client-localized strings. Server section identifiers remain stable API keys. Recommended default English labels are "Continue Watching", "Next Up", "New Episodes", and "Recommended"; platform clients may use shorter vendor-native labels only where launcher space is constrained.

Focus behavior:

- D-pad/remote navigation must have one predictable focused element at all times.
- Horizontal rows move left/right within a row and up/down between rows without trapping focus.
- Focus, selected, pressed, disabled, and loading states must be visually distinct at 10-foot viewing distance.
- Back exits transient panels first, then playback, then the current page, and never silently logs out.
- Long-press, media remote, controller shoulder buttons, and voice-entry shortcuts may be added per platform, but every action must also be reachable through basic D-pad/select/back input.

Layout and typography:

- Prefer poster cards for movie, episode, and recommendation rows; prefer wide backdrop cards only where the platform launcher expects them.
- Use title, subtitle, season/episode metadata, progress, runtime, and availability in that priority order.
- Keep TV text short, avoid paragraphs in browse rows, and reserve detailed descriptions for detail/pre-playback screens.
- Maintain readable text and focus rings on both SDR and HDR displays; do not depend on subtle gradients or low-contrast overlays.

Artwork requirements:

- Minimum useful assets are poster and backdrop for movies/series, thumbnail for episodes, and logo where available.
- If an artwork URL fails, fall back in this order: server-provided alternate category, deterministic title tile, then platform-native placeholder.
- Fallback title tiles must not expose filesystem names or unmatched provider IDs.
- Clients should cache artwork using server cache headers and ETags, but must refetch after `tv_surface_changed` events that include `artwork_changed`, `metadata_changed`, or `settings_changed` when affected rows are visible.

Playback controls:

- Required controls are play/pause, seek, skip forward/back where platform conventions support it, stop/back, audio track selection, subtitle selection, quality/status display, and segment skip where the server marks a skippable intro/credits/recap window.
- Transport overlays should auto-hide during playback and reappear on remote input, pause, buffering, seek, errors, or track/quality changes.
- TV clients must expose enough status to explain Direct Play, Remux, Transcode, unavailable media, buffering, and playback errors without dumping diagnostics into the normal player UI.
- Audio/subtitle selectors must write preferences back through existing watch-data/playback APIs when the server exposes the relevant track indices.

Profile, server, and device-linking behavior:

- A TV device is commonly shared. Clients must make the active Duskcue user/profile visible in Settings and any profile switcher.
- Launcher, Top Shelf, Watch Next, Smart Hub Preview, and app-local rows are scoped to the authenticated Duskcue user whose settings enabled publication.
- Device-linking must resume the original deep link or platform tile after authentication succeeds.
- Switching user/profile or server must clear platform-local launcher mappings and cached TV rows for the old identity before publishing new rows.

Privacy rules:

- Never publish another Duskcue user's resume position, watched state, collection membership, or private recommendation into the active platform profile.
- If a TV OS has its own household/profile model, Duskcue must map rows only into the platform profile that authenticated the Duskcue user, or fall back to app-local rows when that guarantee is not available.
- Notification, migration, admin, and diagnostic events must not appear on TV launcher surfaces.
- Error messages should say what the user can do next: sign in, refresh, check library access, choose another version, or contact the server admin.

Localization:

- API enum values, reasons, and IDs are not display strings.
- Row labels, empty states, errors, settings labels, and playback controls are localized in the client.
- Server-provided media titles, episode names, collection names, and artwork remain content data and should be displayed as returned.
- Clients must preserve layout under longer translated strings by truncating non-critical browse metadata before truncating the primary title.

Each platform phase starts with Task 0: current online research, design refresh, and phase enrichment. Task 0 must use official platform documentation current to 2026, update this document, update [BUILD_ORDER.md](../../BUILD_ORDER.md), and record any changed implementation constraints before code is written.

## Goals

- Continue an unfinished movie from the TV home screen.
- Continue an unfinished episode from the TV home screen.
- Show the next episode for an in-progress series after an episode completes.
- Open Duskcue directly into playback from a platform tile.
- Resume at the server's current resume position, not stale local state.
- Remove or update stale TV launcher items quickly when the user watches or completes content on any client.

## Non-Goals

- No third-party cloud recommendation service for v1.0.
- No separate recommendation model per TV platform.
- No launcher publication from the Rust server directly; launcher APIs live inside native TV apps.
- No assumption that every TV platform exposes the same surface or certification path.

## Architecture

```
Duskcue server
  -> TV surface API
  -> Native TV client adapter
  -> Platform launcher surface
  -> Deep link back into Duskcue playback
  -> Playback progress API updates server resume state
```

The server remains authoritative for:

- watch progress
- next episode selection
- user library access
- artwork URLs
- recommendation eligibility
- freshness and removal rules

The TV client remains authoritative for:

- launcher API calls
- platform content IDs
- deep-link registration
- local cache of platform item IDs
- platform-specific certification requirements

## Server API

Add a platform-neutral TV surface endpoint:

`GET /api/v1/users/me/tv-surface`

Phase 16b Task 1 added the initial authenticated TV domain shell under `server/src/domains/tv/` and registered the first contract routes:

| Route | Current Phase 16b behavior | Later task |
|---|---|---|
| `GET /api/v1/users/me/tv-surface` | Validates `platform`, `limit`, and `sections`; returns accessible, healthy user-scoped Continue Watching, Next Up, New Episodes, and deterministic Recommendation sections with private cache headers, ETags, bounded availability states, and privacy-safe availability details. Per-user TV publication settings can return explicit empty sections for disabled publication/platform/section states. | Implemented. |
| `GET /api/v1/tv/resolve/{platform_content_id}` | Validates canonical `duskcue:{movie|episode}:{uuid}` IDs, performs inverse lookup through the shared TV access scope, reloads current resume/media-file state, and returns playback-start hints for accessible items. Returns unavailable content when the authenticated user has disabled TV publication. | Implemented. |
| `GET /api/v1/tv/settings` | Returns persisted per-user TV publication settings plus integration status: enabled platforms, diagnostics availability, last feed generation, last TV surface event, and last resolve failure. | Implemented. |
| `PUT /api/v1/tv/settings` | Updates per-user TV publication settings stored at `users.metadata.tv_surface_settings`; accepts partial publication/platform/section toggles and emits `tv_surface_changed` with `settings_changed` when affected sections change. | Implemented. |
| `GET /api/v1/tv/diagnostics` | Requires `can_manage_server`; validates feed query parameters and returns candidate counts, included section counts, aggregate exclusion reasons, and a bounded privacy-safe exclusion sample. | Implemented. |

The registered TV error codes are `TV_001` invalid platform, `TV_002` unavailable content, `TV_003` access denied, `TV_004` unsupported platform hint, `TV_005` invalid platform content ID, `TV_006` invalid section, `TV_007` invalid limit, and `TV_008` diagnostics unavailable.

Phase 16b Task 2 added platform content ID utility targets:

| Target | Shape |
|---|---|
| Canonical | `duskcue:{movie|episode}:{uuid}` |
| Roku/Amazon-style strict IDs | `duskcue_{movie|episode}_{uuid_without_dashes}` |
| URL path/query IDs | Percent-encoded canonical IDs |

Optional query parameters:

| Parameter | Purpose |
|---|---|
| `platform` | `android_tv`, `google_tv`, `fire_tv`, `roku`, `tvos`, `tizen`, `webos`, `xbox`; used only for shaping optional hints, not authorization. |
| `limit` | Maximum total items. Default 30, maximum 100. |
| `sections` | Comma-separated list: `continue`, `next_up`, `recommended`, `new_episodes`. |

Response shape:

```json
{
  "generated_at": "2026-06-27T00:00:00Z",
  "platform": "android_tv",
  "limit": 30,
  "sections": [
    {
      "section_type": "next_up",
      "title": "Next Up",
      "empty_reason": null,
      "items": [
        {
          "surface_item_id": "tv:next_up:duskcue_episode_019...",
          "media_item_id": "019...",
          "platform_content_id": "duskcue:episode:019...",
          "media_type": "episode",
          "section_type": "next_up",
          "title": "Episode Title",
          "subtitle": "Show Name S02E04",
          "description": "Episode overview...",
          "season_number": 2,
          "episode_number": 4,
          "duration_ms": 2640000,
          "resume_position_ms": 0,
          "progress_percent": 0,
          "last_engaged_at": "2026-06-26T23:00:00Z",
          "poster_url": "/api/v1/items/019.../artwork/poster",
          "backdrop_url": "/api/v1/items/019.../artwork/backdrop",
          "deep_link": "duskcue://play/episode/019...",
          "web_url": "/media/019...",
          "availability": "playable",
          "availability_detail": null
        }
      ]
    }
  ]
}
```

Task 3 implementation details:

- Continue Watching uses unfinished movie/episode `user_item_data` rows with meaningful resume progress.
- Next Up returns one next unwatched episode per watched series.
- New Episodes returns the newest unwatched episode per started series.
- Recommended is deterministic and scores enabled collection membership, recent genre/tag/person overlap, rating, date, and title.
- Normal feed sections exclude inaccessible libraries, deleted libraries, and items without a healthy media file.
- `generated_at` is derived from feed data rather than wall-clock time so unchanged responses can reuse ETags.

Task 6 implementation details:

- Feed items expose bounded availability states and optional user-facing detail strings; these strings never include file paths, library paths, tokens, signed URLs, SQL errors, or server internals.
- Admin diagnostics classify non-included candidates as `library_offline`, `access_revoked`, `missing_file`, `metadata_incomplete`, or `not_selected`.
- Diagnostics return `candidate_count`, `included_count`, `section_counts`, `reason_counts`, and a bounded `excluded` sample so support tooling can explain feed composition without exporting private filesystem data.
- TV surface observability uses bounded Prometheus labels only: platform, status, section, and reason.

Resolve response shape:

```json
{
  "platform_content_id": "duskcue:episode:019...",
  "media_item_id": "019...",
  "media_type": "episode",
  "title": "Episode Title",
  "subtitle": "Show Name S02E04",
  "description": "Episode overview...",
  "duration_ms": 2640000,
  "resume_position_ms": 615000,
  "availability": "playable",
  "availability_detail": null,
  "playback_action": "start_playback",
  "playback_start_path": "/api/v1/playback/start",
  "playback_start": {
    "method": "POST",
    "path": "/api/v1/playback/start",
    "media_item_id": "019...",
    "media_file_id": "019...",
    "start_position_ms": 615000,
    "force_transcode": false,
    "device_profile_required": false
  },
  "deep_link": "duskcue://play/episode/019...",
  "web_url": "/media/019...",
  "artwork": {
    "poster_url": "/api/v1/items/019.../artwork/poster",
    "backdrop_url": "/api/v1/items/019.../artwork/backdrop",
    "logo_url": "/api/v1/items/019.../artwork/logo",
    "thumbnail_url": "/api/v1/items/019.../artwork/thumbnail"
  },
  "requires_auth": true
}
```

Task 7 implementation details:

- Resolve never trusts resume state embedded in a platform tile. It reloads `user_item_data` for the authenticated Duskcue user on every request.
- Watched items resolve with `resume_position_ms = 0`; unfinished items resolve with the latest stored resume position.
- The preferred `media_file_id` is the current largest healthy media file for the item, matching the playback domain's default file selection.
- Inaccessible, deleted-library, missing, malformed, and cross-type IDs remain BOLA-safe and return unavailable content rather than revealing library membership.
- Accessible items with no healthy file return a bounded unavailable action and availability detail instead of a filesystem path or internal error.

## Playback Entry Contract

Every TV and console client must treat the Duskcue server as the authority for playback entry. A platform row, launcher tile, voice result, Top Shelf item, URI activation, or app-local row may carry a cached `platform_content_id`, but it must not carry authoritative resume position, access state, or media-file selection.

Required flow:

1. Ensure the local app has a valid Duskcue session. If not, route through device linking or sign-in, then retry the original platform entry.
2. Call `GET /api/v1/tv/resolve/{platform_content_id}` with the current session token.
3. If resolve returns `TV_002`, `TV_005`, or an availability/action other than `start_playback`, show a platform-native unavailable message and refresh the local TV surface feed.
4. Call `POST /api/v1/playback/start` with the resolved `media_item_id`, preferred `media_file_id`, `force_transcode`, `quality_mode`, `max_streaming_bitrate`, and the platform's current `device_profile`.
5. If the resolved `start_position_ms` is greater than 0, seek to that position before visible playback where the platform API allows it. For direct play, native player seek/range behavior is acceptable. For remux/transcode sessions, call `POST /api/v1/playback/seek` when the returned stream URL needs a server-side seeked manifest.
6. Attach the returned `stream_url` to the platform player and begin playback.
7. Send an immediate heartbeat after the first successful seek/start, then send heartbeats every 15 seconds while playback is active.
8. Send an immediate heartbeat on pause, resume, buffering start, buffering end, and large seek.
9. On user back/exit, app background without platform media continuation, player error, or natural completion, call `POST /api/v1/playback/stop` with the best known position.
10. Refresh the TV surface after stop/completion, after a playback error, and after receiving a future `tv_surface_changed` event.

Playback start request shape for TV clients:

```json
{
  "media_item_id": "019...",
  "media_file_id": "019...",
  "force_transcode": false,
  "quality_mode": "auto",
  "max_streaming_bitrate": 25000000,
  "device_profile": {
    "platform": "roku",
    "app_version": "1.0.0",
    "model": "4802X",
    "supported_containers": ["mp4", "mkv"],
    "supported_video_codecs": ["h264", "hevc"],
    "supported_audio_codecs": ["aac", "ac3", "eac3"],
    "supported_subtitle_formats": ["webvtt", "srt"],
    "max_video_resolution": "3840x2160",
    "supports_hdr": true,
    "supports_dolby_vision": false
  }
}
```

TV heartbeat contract:

| Event | Endpoint | Required fields | Notes |
|---|---|---|---|
| Start confirmed | `POST /api/v1/playback/heartbeat` | `session_id`, `position_ms`, `state: "playing"` | Send after player starts and after any startup seek. |
| Cadence update | `POST /api/v1/playback/heartbeat` | `session_id`, `position_ms`, `state` | Every 15 seconds during active playback; this fits STREAMING.md's 10-30 second range. |
| Pause | `POST /api/v1/playback/heartbeat` | `session_id`, `position_ms`, `state: "paused"`, `is_paused: true` | Send immediately, not at the next cadence tick. |
| Buffering | `POST /api/v1/playback/heartbeat` | `session_id`, `position_ms`, `state: "buffering"`, `is_buffering: true` | Send start and end transitions when the platform exposes them. |
| Seek | `POST /api/v1/playback/seek` then heartbeat | `session_id`, `position_ms` | Use server seek when transcode/remux manifests must restart at the new position. |
| Exit or completion | `POST /api/v1/playback/stop` | `session_id`, `position_ms` | Server applies the existing 90% watched threshold and clears resume when watched. |

Playback error reporting:

- If the player fails before a session starts, show the resolve availability detail where present and refresh the TV surface.
- If the player fails after a session starts, send `POST /api/v1/playback/stop` with the best known position and submit a QoE report through `POST /api/v1/playback/qoe` when the platform has useful error/buffering context.
- TV clients should not include filenames, local paths, bearer tokens, signed URLs, or raw server URLs in user-visible errors or diagnostic bundles.
- Repeated codec or subtitle failures should update the platform's device profile or prompt capability testing rather than forcing every future item through platform-specific special cases.

Direct-to-play requirements:

- Roku Search/voice, Fire TV catalog/deep links, webOS launch parameters, tvOS Universal Links, Xbox URI activation, and similar direct entries must call resolve and then start playback directly.
- These paths must not show a resume/start-over interstitial when the platform certification path expects Direct to Play. The resolved `start_position_ms` is the bookmark.
- If `start_position_ms` is 0, start from the beginning without asking.
- If auth is missing, route to sign-in/device-linking and resume the original direct entry after auth succeeds.

### Selection Rules

Platform content IDs:

- derive from stable Duskcue IDs, for example `duskcue:episode:{media_item_id}`
- must not expose library paths or file paths
- remain stable across metadata refresh, artwork refresh, and file moves that preserve the same media item
- map cleanly to Fire TV catalog IDs and Roku `contentId` / feed IDs
- can be shaped by the optional `platform` query parameter if a platform requires stricter characters

Continue-watching items:

- include movies and episodes with meaningful progress but not completed
- sort by `user_item_data.last_played_at DESC`
- exclude items the user can no longer access
- include the server resume position

Next-up items:

- include at most one episode per series
- choose the next unwatched episode after the latest completed episode
- prefer chronological season/episode order
- exclude specials unless the series ordering marks them as part of the normal sequence

New-episode items:

- include new episodes from series the user has started or follows
- avoid showing multiple episodes from the same series on constrained launcher surfaces

Recommended items:

- v1.0 can start with deterministic recommendations from existing collections and related metadata
- later versions can add stronger personalized ranking without changing platform adapters

### Update Triggers

TV clients should refresh the surface feed after:

- playback start
- playback pause/stop
- meaningful heartbeat/seek resume-position updates
- playback completion
- library scan completion
- metadata/artwork refresh
- poster/overlay changes
- collection changes
- user library-access changes

The server should also emit an SSE event when TV surface state changes:

`tv_surface_changed`

Payload:

```json
{
  "user_id": "01J...",
  "reason": "playback_completed",
  "changed_sections": ["continue", "next_up", "new_episodes", "recommended"],
  "media_item_id": "01J...",
  "series_id": null,
  "library_id": null,
  "generated_after": "2026-06-30T18:00:00Z",
  "debounce_until": null
}
```

Implemented reasons are bounded to `playback_started`, `resume_position_changed`, `playback_paused`, `playback_stopped`, `playback_completed`, `watch_data_updated`, `library_changed`, `library_scan_completed`, `metadata_changed`, `artwork_changed`, `collection_changed`, `access_changed`, `settings_changed`, or `other`.

Server producers:

- playback handlers emit user-scoped events for start, resume movement, seek, stop/completion, and explicit watch-data changes
- library handlers, scheduled scans, and filesystem-triggered scans emit library-scoped events only to active users who can access that library
- metadata refresh emits `metadata_changed` after changed items are re-enriched
- poster/artwork and overlay handlers emit `artwork_changed`
- collection handlers emit `collection_changed`
- user status, library-access, and capability changes emit `access_changed`
- TV publication settings emit `settings_changed`

Native TV clients can subscribe while running and schedule a refresh after receiving the event. If `debounce_until` is present, clients should avoid immediately refetching more than once for the same user/reason/item window; the server also coalesces heartbeat-heavy resume updates per user so TV launchers do not thrash during active playback.

## Shared Platform Adapter Contract

Every platform adapter translates the same server-owned facts into the platform's native launcher/search/deep-link model. Adapter code must keep platform-specific persistence and API calls at the edge; Duskcue server state remains authoritative.

### Required Inputs

All adapters consume:

- `GET /api/v1/users/me/tv-surface` for app-local rows and row-owned platform surfaces
- `GET /api/v1/tv/resolve/{platform_content_id}` before playback from any row, search result, voice result, URI, Universal Link, launch parameter, or platform tile
- `GET /api/v1/tv/settings` to decide whether the authenticated user has enabled publication for the current platform and sections
- `tv_surface_changed` SSE events while the app is foregrounded, with REST refresh on app resume
- existing playback start/heartbeat/seek/stop/QoE APIs for playback state and telemetry

### Identifier Mapping

- `platform_content_id` is the only portable content identity crossing Duskcue and platform surfaces.
- Platform-owned IDs must be deterministic transforms or local mappings back to `platform_content_id` or `media_item_id`.
- Deep links must include only stable IDs and action hints, not bearer tokens, signed stream URLs, local file paths, or resume positions.
- If a platform requires separate movie/episode/series/feed IDs, derive them from Duskcue media IDs and document the mapping in that platform phase.
- If a platform exposes opaque row IDs, program IDs, or tile IDs, store those locally unless the same mapping must be shared across multiple devices for the same Duskcue user.

### Surface Classes

| Surface class | Platforms | Adapter behavior |
|---|---|---|
| Row-owned launcher surfaces | Android TV Watch Next, Apple tvOS Top Shelf | Client publishes, updates, and removes a small curated set from the Duskcue TV feed. Continue Watching and Next Up take priority over broad recommendations. |
| Event-driven activity surfaces | Fire TV Watch Activity / Content Personalization | Client reports playback activity and completion accurately. The server feed still powers app-local rows; platform home behavior may lag or depend on catalog eligibility. |
| Catalog/feed plus deep-link surfaces | Roku Search, Fire TV EMBER/catalog, optional Apple TV app/Universal Search, partner-gated feeds | Client or release tooling exports stable metadata IDs and deep links. Every launch still resolves through Duskcue before playback. |
| App-local-only surfaces | LG webOS v1, Xbox, PlayStation/VIZIO before partner surface access, fallback paths on any platform | Client renders rows inside the app from the TV surface feed and does not promise home-screen publication. |
| Partner-gated surfaces | VIZIO, PlayStation, some Fire TV/Apple/Roku discovery features | Prepare stable IDs and app-local behavior now; implement platform publication only after portal specs confirm a self-hosted/private-library app is allowed. |

### Refresh and Removal Rules

- On `tv_surface_changed`, refetch the TV surface after `debounce_until` when present, then update app-local rows and platform rows from the fresh feed.
- Remove platform-local rows when the refreshed feed no longer contains the item, when the item resolves unavailable, when the user disables publication/settings for that platform, or when the active Duskcue user/server changes.
- Completion events should remove finished continue-watching entries and allow next-up entries to appear after the next feed refresh.
- Metadata/artwork changes should update titles, subtitles, artwork URLs, and platform metadata without changing stable IDs.
- Access-control changes must be treated as high priority: remove inaccessible platform tiles before adding new ones.

### Playback Progress and Capability Reporting

- All adapters send playback start, heartbeat, seek, stop, and QoE reports through Duskcue APIs even when the platform also receives native watch-activity events.
- Device capability reports should include platform name, app version, model where available, container/codecs, subtitle support, HDR/Dolby Vision support, max resolution, and audio capabilities.
- Platform playback events must never replace Duskcue heartbeat/stop calls; they are platform integration outputs, not the source of truth.
- If a platform activity API requires completion or resume percentages, derive them from the current player position and Duskcue playback lifecycle state, not from stale launcher cache.

### Storage Rules

- Local storage is allowed for server list, selected server, active user summary, device ID, last fetched TV feed, artwork cache, platform row/program/tile IDs, and platform-specific content ID mappings.
- Durable server-side adapter tables are required only when a mapping must be shared across devices, reconciled by background server jobs, audited centrally, or used by a server-hosted metadata feed.
- Platform-local caches must be cleared on logout, server switch, user/profile switch, publication opt-out, or access revocation.
- Cached TV feed data must be treated as private per Duskcue user and platform profile.

### Token and Secret Handling

- TV clients must use device linking or normal auth to obtain credentials; platform deep links must never contain bearer tokens.
- Bearer/session tokens must be stored only in platform secure storage where available.
- If a platform lacks secure token storage, prefer short-lived device sessions and require re-linking over plaintext persistent tokens.
- Logs, crash reports, platform analytics, catalog feeds, and launcher metadata must not include bearer tokens, signed URLs, local network credentials, media paths, or server-internal diagnostics.

### Acceptance Checklist for Each Platform Phase

Before a platform adapter is considered complete, it must prove:

- App-local rows render from the same feed fixtures as other TV clients.
- Deep links call TV resolve and start playback without stale resume state.
- Publication settings disable feed rows and platform publication for that user/platform.
- Playback progress updates Duskcue resume/completion state.
- `tv_surface_changed` or foreground refresh updates/removes stale rows.
- Logout/profile/server switching clears local platform mappings and private cached rows.
- The platform's secure storage story is documented and tested.

## Android TV / Google TV Adapter

Android TV / Google TV is the first target because it provides a documented Watch Next API through AndroidX TV Provider.

### Client Responsibilities

- Build a native Android TV client in Kotlin.
- Use Media3 ExoPlayer for playback.
- Expose playback through Media3 `MediaSession`.
- Register deep links for Duskcue playback routes.
- Fetch `GET /api/v1/users/me/tv-surface`.
- Translate eligible Duskcue items into `WatchNextProgram` entries.
- Store `media_item_id` to platform `program_id` mappings locally.
- Update or remove stale `WatchNextProgram` entries after playback state changes.

### Android Watch Next Mapping

| Duskcue surface type | Android Watch Next usage |
|---|---|
| `continue` movie | Unfinished movie resume item. |
| `continue` episode | Unfinished episode resume item. |
| `next_up` | Next episode after completion. |
| `new_episodes` | New episode for a started/followed series, bounded to one per series. |
| `recommended` | Not Watch Next by default; use app UI first unless platform guidelines permit. |

### Google TV Constraint

Google TV's visible home-screen behavior may require platform approval, certification, or store compliance beyond simply calling Android APIs. Duskcue should implement the Android TV Watch Next adapter first and treat Google TV launcher visibility as a release/certification task.

## Fire TV Adapter

Fire TV is the second target because it can reuse much of the Android client architecture on Fire OS devices, while adding Amazon-specific catalog, launcher, and personalization integrations.

Amazon now has two relevant implementation paths:

- **Fire OS Android app path** — Android-based Fire TV app using Amazon Fire TV Integration SDK, Watch Activity, launcher integration, and EMBER catalog integration.
- **Vega path** — Amazon's newer Vega app stack and Vega Content Personalization APIs. As of the researched docs, Vega Content Personalization is open beta and should be tracked separately from the Android Fire OS client.

### Client Responsibilities

- Reuse the Android TV client where Fire OS remains Android-based.
- Publish playback events through Fire TV Watch Activity during active playback.
- Implement Amazon catalog/launcher deep links where eligible.
- Keep Amazon content IDs stable and aligned with Duskcue media IDs.
- Support authenticated and unauthenticated deep-link flows.
- Route authenticated deep links directly to playback.
- Report accurate playback exit/completion events so Fire TV can remove completed items.
- Respect the customer's Fire TV privacy and sharing settings.

### Fire TV Mapping

| Duskcue surface type | Fire TV usage |
|---|---|
| `continue` movie | Watch Activity event stream drives Continue Watching. |
| `continue` episode | Watch Activity event stream drives Continue Watching after meaningful progress. |
| `next_up` | Completion/exit events let Fire TV show the next episode when catalog metadata supports it. |
| `new_episodes` | Amazon recommendation/catalog behavior, not a direct app-owned row by default. |
| `recommended` | Content Personalization / recommendation rows where available; app-local recommendations remain the fallback. |

### Fire TV Design Decisions

- **Event-driven, not row-owned** — Unlike Android TV Watch Next, Fire TV Continue Watching is primarily driven by app-reported playback activity. Duskcue should not model Fire TV as a direct list of launcher entries.
- **Catalog IDs matter** — Amazon's watch activity and deep-link flows depend on content IDs matching catalog integration. Duskcue should generate stable, platform-safe content IDs from `media_item_id`.
- **Off-device progress is limited** — Amazon documentation says current Fire TV services are not using off-device activity to influence the Continue Watching row. Duskcue should still sync server progress into the Fire TV app before playback starts, but should not promise immediate home-row updates from web/mobile playback alone.
- **EMBER is partner-gated** — Fire TV catalog integration is available to select partners. Self-hosted Duskcue should implement local app playback and active Watch Activity first; catalog/search integration is a release-channel task.
- **Vega is tracked separately** — Vega support should be a second Fire TV adapter if Amazon's non-Android Fire TV platform becomes required for broad device coverage.

## Roku Adapter

Roku is a separate native client, not a variant of the web or Android TV app.

Roku discovery is centered on:

- a Roku app built with SceneGraph/BrightScript
- a Roku Search JSON feed
- required deep-link handling
- Direct to Play for public apps
- bookmarks for automatic resume

### Client Responsibilities

- Build a Roku SceneGraph/BrightScript client.
- Implement Duskcue login/device-linking.
- Fetch Duskcue playback URLs and resume state from the server.
- Handle `contentId` and `mediaType` launch parameters.
- Map `contentId` to a Duskcue media item and validate access.
- Start playback directly for movie and episode deep links.
- Use server resume position as the bookmark; do not show a resume/start-over screen for Direct to Play.
- Report playback progress and completion back to Duskcue.
- Generate and host a Roku Search feed if public-store discovery is pursued.

### Roku Mapping

| Duskcue surface type | Roku usage |
|---|---|
| `continue` movie | App-local Continue Watching row; Direct to Play resumes using bookmark position when launched from Roku Search/voice. |
| `continue` episode | App-local Continue Watching row; deep-linked episode resumes from server bookmark. |
| `next_up` | App-local Next Up row; Roku Search feed exposes series/season/episode IDs for platform discovery. |
| `new_episodes` | App-local row first; Roku Search/My Feed behavior may surface catalog changes after feed integration. |
| `recommended` | App-local recommendations first; Roku platform discovery depends on feed eligibility and certification. |

### Roku Design Decisions

- **Roku is feed plus deep link, not a direct Watch Next equivalent** — Duskcue should implement in-app continue/next-up rows and platform deep links before promising home-screen recommendations.
- **Deep links must autoplay for movies/episodes** — Public Roku video apps must handle deep links correctly for certification. Movie and episode links should start playback directly.
- **Server resume state becomes the bookmark** — Roku Direct to Play forbids resume/start-over interstitials for voice-launched playback; Duskcue must resolve the bookmark before playback begins.
- **Search feed IDs must be stable** — Roku `PlayID`/`contentId` values should map to durable Duskcue IDs and remain synchronized with the Roku feed.
- **Certification is a first-class task** — Roku testing should include Deep Linking Tester, ECP launch commands, and Roku certification checks before public release.

## Samsung Tizen Adapter

Samsung Tizen is the fourth target because it has a documented packaged TV web-app model, native Samsung Product APIs, AVPlay media playback, and Smart Hub Preview for launcher-facing content.

This is not just the shared browser client served inside the TV browser. It should be a Samsung TV application package with Tizen configuration, certificates, remote-control focus handling, device testing, and Samsung-specific playback and preview integration.

### Client Responsibilities

- Build a packaged Samsung Tizen web app under `clients/tv/samsung/`.
- Use the Samsung TV SDK/Tizen tooling for packaging, signing, emulator testing, and real-device installation.
- Use AVPlay for playback instead of relying only on the browser `<video>` element.
- Fetch Duskcue HLS playback URLs and start position from the server before playback.
- Use AVPlay `seekTo()` to enter playback at the latest server resume position.
- Report playback progress, pause/exit, and completion back to Duskcue.
- Implement remote-control focus navigation suitable for Samsung TV remotes.
- Use Smart Hub Preview for launcher-facing continue/next-up tiles where supported.
- Prefer personalized Smart Hub Preview for signed-in users, backed by the Duskcue TV surface feed.
- Fall back to public preview or app-local continue-watching if personalized preview is unavailable.

### Samsung Mapping

| Duskcue surface type | Samsung usage |
|---|---|
| `continue` movie | Personalized Smart Hub Preview tile where supported; app-local Continue Watching row otherwise. |
| `continue` episode | Personalized Smart Hub Preview tile where supported; app-local Continue Watching row otherwise. |
| `next_up` | Personalized Smart Hub Preview tile and app-local Next Up row. |
| `new_episodes` | App-local row first; preview tile only when it does not displace active resume/next-up items. |
| `recommended` | Public or personalized Smart Hub Preview candidates, bounded aggressively; app-local recommendations remain the primary surface. |

### Samsung Design Decisions

- **Tizen web app, not generic hosted web** — Samsung TV support should ship as a signed Tizen package so it can use Samsung Product APIs, AVPlay, and Smart Hub Preview.
- **AVPlay is the playback default** — AVPlay exposes TV-native media controls, adaptive streaming, seeking, and track features that are more appropriate for Samsung TVs than a plain web player.
- **Smart Hub Preview is the launcher surface** — Samsung does not map cleanly to Android Watch Next. Duskcue should use Smart Hub Preview for deep-linkable tiles shown when the user focuses the Duskcue app icon.
- **Personal preview requires extra app structure** — Personalized preview requires a foreground app plus a background service app. The first Samsung release can ship app-local rows plus public preview, then enable personalized preview after background-service behavior is validated.
- **Subtitle handling needs platform testing** — AVPlay subtitle behavior differs from the web client; remote external subtitles may need local download or server-side WebVTT/HLS subtitle delivery.
- **Model coverage must be explicit** — Samsung media support varies by model year. Phase 20 should define a minimum supported Samsung model group and test real hardware, not only the emulator.

## LG webOS Adapter

LG webOS is the fifth target. Public LG documentation supports standards-based web apps with platform APIs, launch/relaunch parameters, packaged or hosted app models, HLS playback support, and `mediaOption` resume hints. It does not expose a public Watch Next-equivalent row in the same way Android TV does, so Duskcue should implement app-local continue-watching and deep-link-style launch handling first.

### Client Responsibilities

- Build a webOS TV app under `clients/tv/lg/`.
- Use a packaged web app shell for store/device integration, with server-backed content and artwork loaded from Duskcue.
- Use webOS Studio for current development workflow, packaging, simulator testing, and device deployment.
- Implement device-linking, server selection, and TV remote focus navigation.
- Handle `webOSLaunch` and `webOSRelaunch` events.
- Parse launch parameters into Duskcue playback targets, for example `{ "action": "play", "contentId": "duskcue:episode:01J..." }`.
- Fetch the latest Duskcue resume state before playback.
- Use `mediaOption` start-position data where supported to avoid unnecessary preload before resume.
- Play Duskcue HLS streams through the webOS media element path supported by the target model.
- Report playback progress, pause/exit, and completion back to Duskcue.
- Keep continue-watching, next-up, new-episodes, and recommendation rows inside the app unless LG exposes a public launcher surface during release work.

### LG Mapping

| Duskcue surface type | LG webOS usage |
|---|---|
| `continue` movie | App-local Continue Watching row; launch parameters can open and resume directly when invoked externally. |
| `continue` episode | App-local Continue Watching row; launch parameters can open and resume directly when invoked externally. |
| `next_up` | App-local Next Up row. |
| `new_episodes` | App-local New Episodes row for started/followed series. |
| `recommended` | App-local recommendations from the Duskcue TV surface feed. |

### LG Design Decisions

- **App-local surface first** — Public LG docs support launch/relaunch parameters and app lifecycle handling, but do not provide a clear public Watch Next-equivalent home row API. Duskcue should not promise LG home-screen recommendations for v1.0.
- **Packaged shell over generic browser** — Ship a webOS TV package so Duskcue can use platform lifecycle behavior, app metadata, remote handling, and release tooling. The shared web client can inform UI, but the TV app needs its own focus and playback layer.
- **Use launch parameters as deep links** — External launches should pass stable `platform_content_id` values; the app maps them back to Duskcue IDs and revalidates auth/access before playback.
- **Use `mediaOption` for resume where available** — Resume should be supplied before media pipeline preload when supported, then verified by player progress events after playback starts.
- **HLS remains the primary stream** — LG's streaming protocol table lists HLS support on webOS TV devices; Duskcue should still use device capability reports because emulator and model behavior differ.
- **Minimum webOS version must be explicit** — web engine versions vary widely across webOS releases. Phase 21 should define a minimum supported webOS version and use server-side transcoding/fallback decisions for older devices.
- **Distribution is a release task** — LG app approval, store metadata, model compatibility, and real-device testing are required before public distribution.

## Sony BRAVIA Validation

Sony is not a separate TV operating-system adapter for the current roadmap. Modern Sony BRAVIA sets are primarily Google TV / Android TV devices, and Sony's own app-installation documentation routes users through Google Play on the TV.

Duskcue should handle Sony as a priority hardware validation profile for the Android TV / Google TV adapter.

### Validation Responsibilities

- Test the Android TV client on representative Sony BRAVIA Google TV and Android TV models.
- Confirm Google Play listing compatibility and install visibility on Sony devices.
- Validate Watch Next / Continue Watching behavior on Sony Google TV models.
- Validate remote-control keys, long-press behavior, voice search entry, and focus navigation.
- Validate HLS playback, direct play, direct stream, and transcode decisions on Sony hardware.
- Validate HDR behavior across HDR10, HLG, and Dolby Vision source material where device support exists.
- Validate audio passthrough/downmix behavior for common home-theater setups.
- Validate subtitle rendering, track switching, and server-side WebVTT delivery.
- Validate standby/resume behavior when launching from the Sony home screen or Google TV app.
- Document model-specific issues and decide whether any require Android adapter conditionals.

### Sony Mapping

| Duskcue surface type | Sony usage |
|---|---|
| `continue` movie | Android TV / Google TV Watch Next through the Android adapter. |
| `continue` episode | Android TV / Google TV Watch Next through the Android adapter. |
| `next_up` | Android TV / Google TV Watch Next through the Android adapter. |
| `new_episodes` | Android adapter behavior; Google TV launcher visibility depends on platform policy. |
| `recommended` | App-local recommendations first; launcher exposure follows Android/Google TV rules. |

### Sony Design Decisions

- **No Sony-specific client** — Sony BRAVIA Google TV / Android TV uses the Android TV adapter and Google Play distribution path.
- **Sony is a compatibility gate** — BRAVIA hardware should be part of the Android adapter acceptance matrix because Sony devices are common in home theaters and expose real HDR/audio edge cases.
- **Other Sony TVs are out of scope** — Sony models not running Google TV or Android TV should not drive a separate Duskcue app unless future research identifies a viable public app platform.
- **Hardware behavior matters more than launcher API behavior** — The main Sony risk is playback capability, HDR/audio/subtitle behavior, and remote ergonomics, not a distinct recommendation API.

## Apple TV / tvOS Adapter

Apple TV / tvOS is the sixth full platform target. It is a separate native client, not a web app wrapper. The core path is a Swift/SwiftUI tvOS app using AVKit for playback, Universal Links for deep entry, and a Top Shelf extension for app-owned launcher content.

Apple TV app / Universal Search integration should be tracked separately because Apple's public material frames it as a metadata-feed and video-partner integration. Duskcue should not block the tvOS client on Apple TV app / Universal Search access.

### Client Responsibilities

- Build a tvOS app under `clients/tv/apple/`.
- Use Swift/SwiftUI or UIKit/TVUIKit where tvOS focus behavior requires it.
- Use AVKit / `AVPlayerViewController` for playback rather than a web player.
- Support Duskcue login/device-linking and server selection.
- Fetch Duskcue HLS playback URLs and resume state before playback.
- Use AVPlayer seeking to start at the latest server resume position.
- Report playback progress, pause/exit, and completion back to Duskcue.
- Implement Universal Links with associated domains for playback entry.
- Use stable `platform_content_id` values in Universal Link paths or query parameters.
- Implement a Top Shelf extension backed by the Duskcue TV surface feed.
- Keep Top Shelf content small, fresh, and useful: resume and next-up first, recommendations only when space allows.
- Evaluate Apple TV app / Universal Search metadata feeds and playback reporting as a release-gated partner integration.

### Apple Mapping

| Duskcue surface type | Apple TV / tvOS usage |
|---|---|
| `continue` movie | Top Shelf item and app-local Continue Watching row; Universal Link opens direct playback at server resume position. |
| `continue` episode | Top Shelf item and app-local Continue Watching row; Universal Link opens direct playback at server resume position. |
| `next_up` | Top Shelf item and app-local Next Up row. |
| `new_episodes` | App-local row first; Top Shelf only when it does not displace resume/next-up items. |
| `recommended` | App-local recommendations first; Top Shelf recommendations only after resume/next-up coverage is correct. |

### Apple Design Decisions

- **Native tvOS client** — Apple TV support should be a native Swift/SwiftUI tvOS app using AVKit, not a hosted web client.
- **Top Shelf is the app-owned platform surface** — Duskcue controls Top Shelf content through a bundled extension when the app appears in the Apple TV top row.
- **Universal Links over custom URL schemes** — Universal Links are preferred because Apple treats them as the standard content-linking path and they align with secure HTTPS deployments.
- **Stable HTTPS base URL is required** — Universal Links and associated domains require a stable HTTPS domain. Local-only deployments can still use in-app browsing and app-local resume rows, but full Universal Link behavior requires exposed or locally trusted HTTPS.
- **Apple TV app / Universal Search is optional** — It uses metadata feeds and partner-style integration. Duskcue should document and prepare metadata IDs, but not require this path for the first tvOS client.
- **Top Shelf must be curated** — Continue-watching and next-up are the correct first items. Broad recommendations should stay in-app until the basic resume loop is reliable.
- **Profile mapping needs attention** — Apple TV can have multiple users; Duskcue should map tvOS app identity to the selected Duskcue user and avoid leaking one user's Top Shelf items to another user.

## VIZIO Adapter

VIZIO is the seventh platform target, but it should be treated as **partner-gated** until Duskcue has access to the VIZIO Developer Portal specifications. Public VIZIO material confirms a content/app partner path, a developer portal, partner APIs, onboarding through partner management, and platform support for 4K UHD, HDR10, Dolby Vision, Alexa, and Google Assistant. It does not expose enough public technical detail to commit to a specific open implementation shape.

The practical design posture is: prepare Duskcue's server-side IDs, feeds, and playback contracts now; implement the VIZIO client only after portal access confirms app packaging, playback APIs, deep-link requirements, certification, and whether self-hosted/personal media apps are allowed.

### Partner Access Questions

- Is a self-hosted personal media server app acceptable on VIZIO's platform, or is the partner path intended only for commercial content services?
- Is the VIZIO app model a hosted HTML5/Chromium-style app, a packaged app, or a VIZIO-specific hybrid model?
- Which media APIs are required for HLS, seeking, subtitles, HDR10, Dolby Vision, audio track selection, and error reporting?
- Does VIZIO expose app launch parameters or deep links into specific content?
- Does VIZIO expose a home-screen continue-watching/recommendation surface, or only editorial/search/discovery placement through partner feeds?
- Are metadata feeds required for search/discovery, and what stable ID format is expected?
- Can app-local user profiles drive personalized rows, or does personalization require VIZIO Account / partner APIs?
- What certification devices, model years, memory budgets, web engine versions, and remote-control behaviors must be supported?

### Client Responsibilities After Access

- Build a VIZIO client under `clients/tv/vizio/` only after portal access confirms the app model.
- Use Duskcue `platform_content_id` values as the stable VIZIO content/deep-link IDs where allowed.
- Implement device-linking, server selection, TV remote focus navigation, and app-local Continue Watching/Next Up rows.
- Fetch the latest Duskcue resume state before playback.
- Report playback progress, pause/exit, and completion back to Duskcue.
- Implement VIZIO launch/deep-link behavior if portal specs expose it.
- Implement metadata/search/discovery feeds only if VIZIO's partner specs require them and permit self-hosted media catalogs.
- Use VIZIO partner APIs only for platform-required integration; do not couple Duskcue's core auth or entitlements to VIZIO Account.

### VIZIO Mapping

| Duskcue surface type | VIZIO usage |
|---|---|
| `continue` movie | App-local Continue Watching row first; platform surface only if partner specs expose it. |
| `continue` episode | App-local Continue Watching row first; platform surface only if partner specs expose it. |
| `next_up` | App-local Next Up row. |
| `new_episodes` | App-local New Episodes row. |
| `recommended` | App-local recommendations first; platform discovery feed only if partner specs allow it. |

### VIZIO Design Decisions

- **Partner-gated until proven otherwise** — Public VIZIO pages point developers to partner management, developer portal login, and API key requests. Duskcue should not assume a self-service sideload/store path.
- **No launcher promises before specs** — VIZIO may support discovery, search, editorial rows, voice, or deep links through partner integrations, but Duskcue should not promise a home-screen continue-watching surface until portal docs confirm it.
- **App-local rows are the fallback** — The Duskcue TV surface feed still powers continue-watching, next-up, new episodes, and recommendations inside the app.
- **Stable IDs are still useful** — `platform_content_id` values prepare Duskcue for VIZIO metadata, search, and deep-link specs if partner access is approved.
- **Playback capability is attractive but must be verified** — VIZIO publicly advertises support for 4K UHD, HDR10, Dolby Vision, and voice assistants, but Duskcue must validate actual HLS, subtitles, audio, HDR, and resume behavior against the partner technical specs and real hardware.
- **Core Duskcue remains independent** — VIZIO Account and entitlement APIs should not become required for a self-hosted Duskcue deployment unless VIZIO certification requires an adapter-specific bridge.

## PlayStation Adapter

PlayStation is the eighth platform target, but it is **partner-gated**. Public Sony/PlayStation material confirms that PS5 has a dedicated Media space, a media remote experience, and major streaming apps, but public technical docs for third-party media app implementation are not available outside the PlayStation Partners path.

Duskcue should not assume that a self-hosted personal media server app is acceptable on PlayStation, nor that a PS5 app can be built without partner approval, SDK access, dev hardware, certification, and store/media-space review.

### Partner Access Questions

- Does PlayStation accept self-hosted personal media server apps, or only commercial entertainment services?
- Is the relevant path a media/entertainment app, a general application, or a game-style package with media functionality?
- Which SDK APIs are available for HLS playback, seek-to-resume, audio/subtitle tracks, HDR, controller/media remote input, and background/resume behavior?
- Are deep links, activity cards, Media space tiles, resume surfaces, or search/discovery integrations available to third-party media apps?
- Does PlayStation require metadata feeds, content ratings, regional availability, or entitlement integration for media apps?
- Can the app authenticate directly against a local/self-hosted Duskcue server, or must platform account/entitlement systems be integrated?
- What are the certification rules for LAN-only servers, private libraries, user-generated libraries, and local network discovery?
- What dev hardware, SDK agreements, legal entity status, privacy policy, support, and age-rating obligations are required?

### Client Responsibilities After Access

- Build a PlayStation client under `clients/tv/playstation/` only after partner access confirms app feasibility.
- Implement device-linking and server selection without exposing local server credentials through platform logs or crash reports.
- Use Duskcue `platform_content_id` values as stable content identifiers where PlayStation specs allow.
- Fetch the latest Duskcue resume state before playback.
- Implement HLS playback, seek-to-resume, progress heartbeat, pause/exit reporting, and completion reporting.
- Support DualSense/DualShock controller input and the PS5 Media Remote.
- Implement app-local Continue Watching, Next Up, New Episodes, and Recommendations rows.
- Add Media space, activity/resume, search, or deep-link integration only if PlayStation partner docs expose supported APIs for media apps.
- Validate local network, remote HTTPS, NAT, and certificate behavior because Duskcue deployments are often self-hosted.

### PlayStation Mapping

| Duskcue surface type | PlayStation usage |
|---|---|
| `continue` movie | App-local Continue Watching row first; platform Media space/resume surface only if partner docs expose it. |
| `continue` episode | App-local Continue Watching row first; platform Media space/resume surface only if partner docs expose it. |
| `next_up` | App-local Next Up row. |
| `new_episodes` | App-local New Episodes row. |
| `recommended` | App-local recommendations first; platform discovery only if partner docs allow self-hosted catalogs. |

### PlayStation Design Decisions

- **Partner-gated until proven otherwise** — PlayStation Partners is the only credible development path. Duskcue should not plan public implementation from unofficial SDKs or reverse-engineered APIs.
- **Console client, not TV OS adapter** — PlayStation work is a living-room console media client. It should reuse the TV surface feed, but its distribution, certification, input, and playback constraints differ from TV platforms.
- **App-local surfaces first** — There is no public Watch Next-equivalent API in the researched PlayStation material. Continue-watching and next-up should live inside the Duskcue app unless partner docs expose Media space resume/discovery hooks.
- **Media Remote matters** — PS5 has a media remote path, so playback controls, focus behavior, and transport keys should be verified alongside normal controller input.
- **Self-hosted viability is the blocking question** — Before implementation, confirm that PlayStation permits a client whose content catalog is user-provided and local/private rather than a commercial streaming catalog.
- **Core Duskcue remains independent** — PlayStation account, entitlement, or store systems should be adapter-specific bridges, not dependencies for Duskcue server auth or library access.

## Xbox Adapter

Xbox is the ninth platform target and is more actionable than PlayStation because Microsoft publishes an Xbox media app path through UWP documentation. It is still a living-room console client rather than a TV operating-system launcher adapter.

The recommended first implementation is a native UWP media app using XAML/C# with `MediaPlayerElement` / `MediaPlayer`, not a WebView-hosted version. A WebView shell would reduce UI reuse friction, but the native path gives better performance, better media integration, and avoids tying playback to an older WebView stack.

### Client Responsibilities

- Build an Xbox client under `clients/tv/xbox/`.
- Use UWP packaging with Visual Studio/MSIX and Xbox Device Portal for development deployment.
- Implement device-linking, server selection, controller navigation, and media remote behavior.
- Use native UWP media playback APIs (`MediaPlayerElement` / `MediaPlayer`) for HLS/resume playback.
- Fetch the latest Duskcue resume state before playback.
- Seek to the latest server resume position before or immediately after playback starts.
- Report progress heartbeat, pause/exit, completion, and playback errors back to Duskcue.
- Integrate System Media Transport Controls for media remote and Xbox Guide playback controls.
- Use URI protocol activation or App URI handlers for Duskcue deep-link playback where supported.
- Implement app-local Continue Watching, Next Up, New Episodes, and Recommendations rows.
- Add 4K/HDR support through `hevcPlayback` only after memory/background tradeoffs are accepted.
- Validate on Xbox Series X/S and Xbox One S/X class hardware; original Xbox One should be treated as a reduced-capability profile.

### Xbox Mapping

| Duskcue surface type | Xbox usage |
|---|---|
| `continue` movie | App-local Continue Watching row; URI activation can enter playback at the latest server resume position. |
| `continue` episode | App-local Continue Watching row; URI activation can enter playback at the latest server resume position. |
| `next_up` | App-local Next Up row. |
| `new_episodes` | App-local New Episodes row. |
| `recommended` | App-local recommendations from the Duskcue TV surface feed. |

### Xbox Design Decisions

- **Native UWP media app first** — Use native XAML/C# plus `MediaPlayerElement` / `MediaPlayer` unless later prototyping proves a WebView-hosted UI is good enough.
- **Console adapter, not TV launcher adapter** — Xbox should reuse the TV surface feed and living-room UX patterns, but there is no researched Xbox equivalent to Android Watch Next or Apple Top Shelf.
- **URI activation is the deep-link path** — Duskcue can register a protocol or app URI handler and map stable `platform_content_id` values back to Duskcue media IDs.
- **SMTC is required for a proper media experience** — System Media Transport Controls should drive Xbox Guide and media remote playback control.
- **4K/HDR has a real resource tradeoff** — Enabling `hevcPlayback` grants 4K/HDR10 support and more memory but changes app concurrency/background behavior. Duskcue should treat this as a release decision, not a default assumed capability.
- **Device capability reporting matters** — Xbox hardware spans original Xbox One, Xbox One S/X, and Series S/X. The client must report capabilities so the server can choose direct play, remux, or transcode correctly.
- **Store policy still needs validation** — UWP media apps are documented, but Store submission, certification, self-hosted catalog acceptability, and current Xbox app policy need a release-phase check.

## Future Platform Research Queue

After Xbox, these platforms are worth evaluating in order:

1. **VIDAA** — Meaningful Hisense/Toshiba/Sharp reach, but public developer access is less clear than Samsung/LG/Roku. Treat as a partnership and documentation-availability investigation.
2. **Set-top ecosystems** — Xfinity/Xumo, Sky, and other operator boxes are valuable only if Duskcue later pursues managed distribution partnerships.
3. **Apple Vision Pro / visionOS** — Possible future media client using Apple media frameworks, but it is not a TV-surface target and should not compete with living-room platform work.

## Other Platforms

tvOS and future platform behavior should be handled over the same server API.

| Platform | v1.0 posture |
|---|---|
| Fire TV | Android-based Fire OS adapter after Android TV; add Watch Activity first, then catalog/deep-link integration if partner access is available. Track Vega separately. |
| Roku | Separate SceneGraph/BrightScript app; app-local continue/next-up first, then Roku Search feed, deep links, Direct to Play, and certification. |
| Samsung Tizen | Packaged Tizen web app with AVPlay, app-local continue/next-up, Smart Hub Preview deep links, and personalized preview after background-service validation. |
| LG webOS | Packaged webOS TV app with app-local continue/next-up, launch/relaunch parameter handling, `mediaOption` resume, HLS playback, and LG app approval/device compatibility testing. |
| Sony Google TV / Android TV | Covered by the Android TV / Google TV adapter. Sony should be treated as a priority hardware test target, not a separate platform adapter, unless Sony-specific APIs become necessary. |
| Apple TV / tvOS | Native Swift/SwiftUI client using AVKit, Universal Links, app-local rows, and Top Shelf; Apple TV app / Universal Search remains optional partner/release work. |
| VIZIO | Partner-gated adapter. Prepare stable IDs and app-local surfaces now; implement client only after Developer Portal access confirms app model, media APIs, deep links, discovery feeds, and self-hosted-app viability. |
| PlayStation | Partner-gated console adapter. Prepare stable IDs and app-local surfaces now; implement client only after PlayStation Partners access confirms self-hosted media-app viability. |
| Xbox | UWP console adapter using native media APIs, URI activation, SMTC, app-local TV surfaces, and explicit 4K/HDR capability checks. |
| VIDAA | Future research. Evaluate public developer access and partnership requirements for Hisense/Toshiba/Sharp devices. |

## Data Model Impact

No schema change is required for the first server feed because Duskcue already has:

- `user_item_data` for resume and watched state
- `play_sessions` and `play_events` for playback activity
- `series`, `seasons`, and `episodes` for next-up resolution
- `artwork` for posters and backdrops
- user/library access tables for authorization

Add tables only when a platform adapter needs durable server-side synchronization state. Android Watch Next program IDs should remain local to the Android client unless multi-device Android TV synchronization proves necessary. Fire TV and Roku content IDs should be deterministic strings derived from Duskcue IDs, so they do not need a database table for the first implementation.

## Security

- TV surface endpoint requires normal user authentication.
- Every item must pass the same BOLA checks as media detail/playback APIs.
- Deep links must revalidate auth and access before playback.
- Platform clients must not cache bearer tokens in plaintext.
- Artwork URLs should use existing authenticated or signed artwork delivery rules.

## Phase Placement

Implement this after the core desktop/mobile clients are in place:

1. Phase 16a: Desktop and mobile clients.
2. Phase 16b: TV platform foundation.
3. Phase 17: Android TV / Google TV, including Sony BRAVIA validation.
4. Phase 18: Fire TV.
5. Phase 19: Roku.
6. Phase 20: Samsung Tizen.
7. Phase 21: LG webOS.
8. Phase 22: Apple TV / tvOS.
9. Phase 23: Xbox.
10. Phase 24: partner-gated platforms such as VIZIO, PlayStation, VIDAA, and future set-top ecosystems.

Every platform phase has Task 0 for 2026-current official-source research, design refresh, and phase enrichment before implementation starts.
