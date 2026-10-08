# Tonight Web and Desktop Implementation Plan

## Status and purpose

Created October 3, 2026. Status: **In progress — resumed from preserved implementation; verification and remaining milestones are incomplete**.

Implement the reviewed Tonight experience in the actual SvelteKit client and its shared Tauri desktop build. This document is the execution plan for the Codex goal. [UI Foundations](UI_FOUNDATIONS.md#web-and-desktop-redesign--october-2026) remains the source of accepted visual and interaction decisions; existing domain documents remain authoritative for their contracts.

The HTML explorations under `.cache/ui-exploration/` are optional visual references. They are ignored local files, use illustrative data, and are not production code or a prerequisite for a fresh checkout. Do not port their mock authentication, sample PIN, timers, watch history, host APIs, or embedded artwork into the application.

The user has requested implementation planning and a Codex goal. Start execution with T01, then implement the milestones below. Use sub-agents for independent scopes where useful, with one integrator responsible for shared routes, contracts, and progress documentation.

### Mandatory goal constraints — updated October 3, 2026

- Research current official W3C/WAI guidance before implementing or revising an interaction, record the sources and review date in the relevant authoritative document, and apply it to the actual code and verification. Use current published WCAG recommendations and WAI-ARIA guidance; distinguish normative requirements, informative techniques/examples, and product choices. Draft standards are research context, not an unsupported conformance claim.
- Prefer semantic HTML and native controls. Verify keyboard operation, focus visibility/order/restoration, accessible names, status announcements, contrast, reflow/zoom, RTL, reduced motion, and adjustable timing where applicable. Autoplay must have a saved Off option before the time limit, persistent cancellation, and an untimed next-episode path. Automated checks supplement actual interaction and assistive-technology review; document unavailable qualification precisely.
- Prioritize small, composable service files over large singleton files. Give new modules one clear responsibility and explicit inputs, outputs, lifecycle, and scope. Keep API transport, profile preferences, artwork ownership, browsing queries, navigation guards, playback lifecycle, and autoplay timing in distinct services/components. Existing central stores and domain facades may delegate to focused modules; do not append unrelated features to them or create a replacement global singleton. Preserve the repository's domain interfaces and mature playback behavior.
- Review module boundaries and cleanup as part of each milestone. Test meaningful behavior through the appropriate service and integration boundary. A visually complete page with unverified keyboard behavior or a large new state singleton is not completion evidence.
- Monitor RAM and system commit during testing. Use the [Windows testing-memory workflow](../ci/TESTING_MEMORY.md) for remaining local qualification, one heavy job at a time, bounded workers/heaps/Cargo jobs, current headroom gates, sampled peak evidence and verified owned-process/container cleanup. Hold unsafe runs; existing user applications are outside test cleanup. The user's October 7 report concerns Windows sluggishness/freezes, and mitigation must not be described as a proven crash fix.

Primary references: [W3C accessibility standards](https://www.w3.org/WAI/standards-guidelines/wcag/), [WAI media player guidance](https://www.w3.org/WAI/media/av/player/), [APG patterns](https://www.w3.org/WAI/ARIA/apg/patterns/), [native dialog technique H102](https://www.w3.org/WAI/WCAG22/Techniques/html/H102), and [Timing Adjustable](https://www.w3.org/WAI/WCAG22/Understanding/timing-adjustable.html). Specific acceptance checks and sources remain in the milestones and verification sections below.

Standards reviewed October 3, 2026: use [WCAG 2.2](https://www.w3.org/WAI/standards-guidelines/wcag/) as the current published accessibility baseline. [WCAG 3](https://www.w3.org/WAI/standards-guidelines/wcag/wcag3-intro/) remains an incomplete draft. The [APG introduction](https://www.w3.org/WAI/ARIA/apg/about/introduction/) distinguishes its informative implementation guidance from normative WCAG and ARIA requirements. Recheck official sources when implementing each interaction and record any changed guidance.

Rechecked October 7, 2026 against the official WCAG overview, WAI media-player guidance, and WAI September 2026 WCAG 3 announcement: WCAG 2.2 remains the published baseline and WCAG 3 remains a working draft. The keyboard, focus, labeling, contrast, timing, and status checks below remain required. Final qualification must inspect the actual media and native application paths; fixture and source checks alone do not establish conformance.

## Scope and decisions

### Accepted requirements

| Surface | Required result |
|---|---|
| Visual foundation | Tonight charcoal/plum surfaces, restrained lavender accent, readable controls, editorial serif titles, cinematic artwork, and stable top navigation. |
| Home | Continue watching first, then a smaller featured title, then recently added content. |
| Movies and TV | Poster galleries with larger artwork and breathing room. |
| Title details | Dedicated full Title pages; TV Episode gallery with season selection, synopsis, watched state, and Play/Resume actions. |
| Search | Submit with Enter or the search action; dedicated results gallery. Preserve query, supported filters, sorting, and the origin when returning from details. |
| Player | Minimal transport controls, Episodes and Audio & subtitles actions, compact vertical popovers, and secondary Settings. |
| Visibility | Three-second idle delay as the initial product default. Reveal on pointer movement, tap, or keyboard interaction; retain controls while paused, interacting with controls, or using a popover. |
| Fullscreen and exit | Fullscreen toggle and top-right X. X stops playback and returns to the same complete Title page with season, episode, and position retained. Escape dismisses a popover first or exits fullscreen; it must not unexpectedly close the stream. |
| Next episode | Ten-second countdown after the actual episode-ended event; Play now and Cancel. Cancel leaves an untimed Play next action. Saved autoplay Off is available before the countdown. Retain playback preferences and fullscreen; do not wrap at a movie or the end of the selected season. |

### Implementation defaults for choices not separately selected

These are explicit working defaults for the requested implementation, not claims that each alternative received a separate design vote. Apply later user selections without restarting completed work.

| Choice | Default and boundary |
|---|---|
| Routine profile switching | Quick switch disclosure in the top-right profile area. Retain the server-required initial Who's watching? picker. |
| Viewing preferences | Quiet personal page with Save changes, Discard changes, and an unsaved-navigation warning. Editing a field does not save or navigate. |
| Playback-default ownership | Saved autoplay, audio, and subtitle defaults belong to the active profile. Interface locale stays account-owned. Streaming quality is a device preference. |
| Device volume | Preserve the existing `duskcue_player_volume` setting in local storage. It is shared within one browser origin or desktop installation, including its profiles/accounts, and is never uploaded as a profile preference. Streaming quality separately uses server/account/device scope. |
| Continue watching action | Implement direct resume using existing healthy-file selection and fresh watch data; ordinary catalog cards open the Title page. |
| Return from a collection or catalog | Preserve URL-backed filters, sort, pagination context, and the originating title focus where practical. |
| Technical media information | Retain existing information and controls, disclosed below primary title and playback content. |
| Loading, empty, and error layouts | Use the existing asset/accessibility contracts and the state requirements below; do not invent new visual directions. |

Keep existing profile creation, deletion, parental-policy management, ratings, notifications, and administrative workflows reachable. Rename is already supported by the profile API and should work in the profile management page. A new avatar editor, parental-policy redesign, cross-season autoplay, subtitle appearance editor, or replacement shortcut scheme is outside this goal.

### Scope limits

- Web and shared Tauri desktop UI; no Flutter or dedicated TV application redesign.
- Add only server contracts needed for correct browsing, episode data, and profile viewing preferences. Reuse existing authentication, media access, watch-state, streaming, quality, and session services.
- Preserve the personal Settings/Administration separation and capability checks. Personal quality and subtitle preferences must not navigate to administrative quality-policy or subtitle-provider editors.
- Do not replace the HLS stack, change deployment, publish a release, or deploy the application as part of this goal.
- Do not change ambient-channel semantics or expand Kids downloads/administrative permissions.

## Production findings and integration points

| Area | Existing code | Work needed |
|---|---|---|
| Shared shell and profile lifecycle | `clients/web/src/routes/+layout.svelte` | Preserve selection gating, PIN unlock, request cancellation, scope reset, SSE, notifications, locale, and desktop bridge while extracting presentation components. |
| Tokens and shared assets | `clients/web/src/app.css`, `docs/api/fixtures/design/v1/`, `clients/web/src/lib/utils/artwork.js` | Map Tonight into semantic tokens. Fix selected-server-aware desktop artwork loading; reserve aspect ratios and preserve private/authenticated artwork behavior. |
| Home | `clients/web/src/routes/dashboard/+page.svelte` | Replace the incomplete sequential check of a small recent-movie subset with an authoritative profile-scoped Continue watching query. |
| Catalog and search | `routes/media/+page.svelte`, `routes/libraries/[id]/+page.svelte`, `routes/search/+page.svelte`, `lib/components/SearchBar.svelte` | Reuse cursor pagination, library scope, query/facets, and stale-response protection. Close missing sort/watch/favorite contracts before exposing controls. |
| Title and episodes | `routes/media/[id]/+page.svelte`, `lib/api/media.js`, `server/src/domains/media/` | Keep file selection, favorites, ratings, and watch-data mutations. Add complete series/season/episode queries; a single paginated catalog page is not an episode source. |
| Collections | `routes/collections/+page.svelte`, `lib/api/collections.js`, `lib/components/CollectionGrid.svelte`, `lib/stores/collections.js` | The page is a placeholder and grid/store are scaffolds. Implement real accessible collection browsing using existing server APIs. |
| Playback | `routes/play/[id]/+page.svelte`, `lib/components/Player.svelte`, `lib/stores/player.js` | Retain direct/native HLS/hls.js playback, file health, resume, recovery, heartbeat/stop, segments, storyboards, and QoE. Add the reviewed presentation and real transition handling. |
| Preferences | `routes/settings/+page.svelte`, `routes/settings/profiles/+page.svelte`, `lib/stores/user.js`, `lib/api/profiles.js`, `server/src/domains/profiles/` | Locale-only account preferences and unscoped browser playback defaults are insufficient. Add typed active-profile viewing preferences without weakening profile-management authorization. |
| Tauri | `clients/desktop/scripts/build-web-static.mjs`, `clients/web/src/lib/desktop/tauri.js`, `clients/desktop/src-tauri/` | Keep static-adapter compatibility, selected-server origin handling, native navigation/tray/notifications, and bounded internal route allowlists. |
| Verification | `clients/web/package.json`, `clients/web/jsconfig.json`, `scripts/verify-*.mjs` | Unit/e2e scripts currently lack their dependencies and suites; there is no check/lint script. Establish a working harness and baseline before relying on these commands. |

Paths in the table are relative to the repository; abbreviated `routes/` and `lib/` paths are under `clients/web/src/`.

## Architecture and state rules

### Routing and shared components

Use actual SvelteKit links/routes for navigation and buttons for commands. Prefer existing `/media`, `/media/[id]`, `/search`, and `/play/[id]` routes, with URL parameters for media type, library, search, sort, and season as appropriate. Avoid introducing duplicate Movies/TV implementations solely for different navigation labels.

Introduce small shared components for the Tonight shell, poster/continue cards, artwork states, profile disclosure, episode gallery, preference form, and player controls when their reuse is demonstrated. Keep orchestration in existing composable stores/services; do not replace it with one large layout or player component. API calls belong in named `lib/api/` modules, not presentation components. New server browsing and viewing-preference logic should live in focused service modules behind existing domain facades rather than expanding an already large service file with another responsibility.

The playback route must carry a validated internal Title-page destination, including the series identity when an episode is played. Closing must persist the current position, stop the server session, release media/timers/listeners, leave fullscreen, and navigate to that title exactly once. Never use an arbitrary external return URL or a sparse intermediate summary page.

### Browsing contracts

Define profile-scoped, access-filtered queries for Continue watching and complete season/episode listings. Continue watching must include resumable movies and episodes across the accessible library, ordered by real activity; completed items must not appear as zero-minutes-left resumes.

Select endpoint names and response shapes in T02 after inspecting existing media/watch services. Reuse existing capabilities and pagination/error conventions. Return stable media/series/season identifiers, episode order, availability, and actual watch positions. Resolve the next episode from the selected season and current access scope; never guess from artwork or a partially loaded gallery.

Expose only supported filters and sorting. If an approved surface needs a missing query, implement its contract rather than presenting an inert control or filtering one loaded page as though it were the full collection.

### Preference contract and ownership

Use typed playback defaults, not display labels or media-specific track IDs. Audio preferences must express source default/preferred language and audio-description preference; subtitles must express Off/preferred language and SDH preference using the actual track metadata available in Duskcue. Normalize scanner language aliases such as `en`/`eng` before matching. Document deterministic fallback to source/default audio and supported subtitles when a preferred track is unavailable. The current scanner does not establish the original soundtrack and lacks structured audio-description flags: label the no-override choice Source default, and add structured stream metadata only where needed to support real audio-description selection. Missing description/SDH metadata must not be fabricated.

Prefer additive active-profile GET/PATCH endpoints under `/api/v1/profiles/current/viewing-preferences`, with typed request/response DTOs and a namespaced `viewing_preferences` key in existing profile metadata. GET returns `profile_id`, `has_saved_preferences`, and `viewing_preferences`; PATCH accepts `expected_profile_id` and the typed preferences object and returns the same response shape. Defaults include autoplay On and no requested audio-language override. A mandatory new table or backfill is not expected; add a migration only if the finalized schema needs one. Preserve unrelated metadata and existing profile-management PATCH guards.

The self-service endpoint may read/write only playback defaults for the authenticated account's active profile, including Kids. It must reject a selection-required session and accept no parental-policy, identity, account, role, or library-access fields. PATCH carries `expected_profile_id`; verify ownership and current session/profile inside a transaction before committing. Return `profile_id` so clients can reject stale replies. Use a consistent lock order with profile switching and parent unlock, and test concurrent operations.

Account locale stays on `/user/preferences`. Streaming quality is stored separately for the current server/account/device. Volume retains its existing local browser-origin or desktop-installation setting across profiles/accounts; this implementation default preserves the mature player contract and does not upload it to profile preferences. Other existing device settings remain separately owned. A title-specific track or speed change does not silently overwrite saved profile audio/subtitle defaults. The player's explicit autoplay setting updates the same saved preference used by Viewing preferences.

Legacy `duskcue_prefs` values are not attributable to a household profile. Do not silently copy them to every profile or server. Preserve unrelated theme/filter/segment-skip settings. Honor a valid legacy autoplay Off locally while the active profile has no explicitly saved defaults; explicit Save establishes that profile's defaults. Do not silently upload unscoped legacy audio/subtitle values. Cover unset, saved On, saved Off, malformed storage, and account/server changes in migration tests.

### Profile/session boundaries

Keep the existing account/device-to-profile remembered mapping. Checking Remember this profile on this device pins the chosen profile; a routine quick switch does not turn it into remember-last-profile behavior. Omitted, true, and false remember flags retain their existing server semantics.

Use the actual parent-unlock endpoint and its errors, expiry, and lockout behavior for protected Kids-to-standard exits. Never ship a fixture PIN. Unlock grants the existing session transition only; it does not turn Kids into an administrator. Profile management remains subject to its existing ownership and current-profile rules.

Switches, sign-out, session expiry, server changes, and cross-tab profile changes must stop incompatible playback, abort old-scope requests, clear profile-dependent stores/artwork, and revalidate the new scope. Late responses and preference mutations must not populate or update the new profile accidentally.

## Implementation milestones

All production tasks start unchecked. T01 can discover prerequisites, but must not mark later work complete merely because a prototype demonstrated it.

### T01 — Baseline, contracts, and working verification

- [x] Read `CONTEXT.md`, inspect the working tree, and preserve existing user changes.
- [x] Record baseline web build, Svelte check, desktop static build, and relevant server/contract checks, separating pre-existing failures from this work.
- [x] Add a working web check command using SvelteKit sync and `svelte-check --tsconfig ./jsconfig.json`. Establish Vitest and Playwright dependencies/configuration with compatible official versions and deterministic fixtures.
- [x] Define browser-test fixtures for two standard profiles and a protected Kids profile, movies, two seasons, resume/completed rows, missing artwork, unavailable files, and preference failures. Keep fixtures out of production UI/data.
- [x] Confirm the exact browsing/preference DTOs, routes, authorization, language/description metadata, and client/server bindings. Update the appropriate domain/contract documents before changing those contracts.

Completion evidence: reproducible baseline, executable harness, documented contracts, and a focused vertical smoke test through an existing real application route.

### T02 — Required server data and profile viewing preferences

- [x] Implement complete, paginated access-scoped Continue watching and series/season/episode data using existing watch/media services.
- [x] Add only the missing filters/sort/watch/favorite query support that the Tonight surfaces expose; preserve cursor and library scope.
- [x] Resolve available audio/subtitle streams from actual indices and metadata, including backward-compatible structured audio-description flags where supported. Never infer supported tracks from prototype labels.
- [x] Implement typed active-profile viewing preferences with defaults, validation, metadata preservation, scope checks, and explicit mutation-race handling.
- [x] Add API helpers/bindings and contract fixtures. Distinguish missing saved defaults from an explicitly saved default so legacy Off is preserved correctly.
- [x] Verify ownership, required selection, Kids self-service field boundaries, unrelated metadata, invalid input, profile switches during reads/writes, and concurrent switch/unlock/save behavior.

Completion evidence: targeted server tests plus database-backed query/mutation coverage and updated client-contract checks. Add migration verification against disposable PostgreSQL only if a migration is introduced.

### T03 — Tonight tokens, artwork, and shared shell

- [x] Introduce semantic Tonight colors, typography, spacing, artwork sizing, and control states in shared web styles, preserving other clients' asset/token contracts.
- [x] Implement responsive Home/Movies/TV/Collections navigation and submitted search. Keep notifications, locale, personal Settings, and capability-filtered Administration reachable.
- [x] Extract the quick profile disclosure and initial picker without changing existing profile/session orchestration.
- [x] Implement selected-server-aware authenticated artwork, stable poster/backdrop/still boxes, and quiet existing type-specific placeholders.
- [ ] Keep desktop static builds and internal navigation allowlists working; do not broaden native navigation to arbitrary paths or origins.

Completion evidence: real authenticated web/desktop shell, keyboard and responsive checks, artwork failure checks, and no regression in auth/profile gates or administrative access.

### T04 — Home, galleries, search, and Collections

- [x] Wire Continue watching, the smaller feature, and recently added content to real profile-scoped data. Omit an unavailable optional feature instead of inserting sample content.
- [x] Convert movie/series/library browsing to the poster gallery while retaining complete pagination, accessible titles, favorites, and supported filters/sorting.
- [x] Preserve submitted search, result facets, stale-request protection, URL state, and title-page return context.
- [x] Implement collection listing and collection item browsing with existing APIs, pagination, and scope-aware empty/error states.
- [x] Cover empty Continue watching, empty library, no search matches, pending metadata, request failures, and artwork failures independently.

Completion evidence: Home → catalog/search/collection → Title page → Back with correct profile, query, filters, sorting, and pagination context.

### T05 — Complete Title pages and Episode gallery

- [x] Render movie/series details with real artwork, synopsis, metadata, available playback actions, favorites, ratings, and secondary file information.
- [x] Render complete seasons and ordered episode galleries with real stills, synopsis, watched/resume state, duration, and clear availability.
- [x] Keep selected season and episode stable through playback, reload/navigation where represented in the URL, and return to details.
- [x] Use fresh watch data and healthy-file selection for Play/Resume; handle missing, revoked, deleted, and unplayable media without losing title context.
- [x] Keep each profile's watched labels, progress, favorites, and Continue watching independent; completed episodes derive from real watch state.

Completion evidence: movie and multi-season series journeys with resume, completion, unavailable episode, artwork failure, and profile-isolation coverage.

### T06 — Saved viewing preferences and profile management

- [x] Add the personal viewing-preferences page with native labeled controls, profile/device fieldsets, Save/Discard, and visible/polite result status.
- [x] Preserve drafts on save failure. Warn before all unsaved exits, including links, browser/back navigation where supported, profile switches, and search submitted by keyboard.
- [x] Load and reset preferences on profile/server/account transitions; ignore stale reads and reject writes against a switched profile.
- [ ] Implement the legacy-storage rules and connect profile defaults to actual media track selection and device quality limits.
- [ ] Retain existing profile creation/deletion/parental-policy workflows, wire name editing, and use actual remembered-profile/PIN endpoints with accessible error/focus behavior.

Completion evidence: Save/reload, Discard/Keep editing, failure recovery, separate profile defaults, legacy Off, one-off playback overrides, remembering, PIN errors/expiry, and required initial selection.

### T07 — Minimal player, fullscreen, and real autoplay

- [ ] Integrate minimal controls and vertical popovers with actual episodes, audio/subtitle tracks, quality, speed, transport, and volume; hide unavailable choices.
- [ ] Implement focus-aware idle visibility and touch/keyboard reveal while preserving seek previews, segment skipping, recovery, and telemetry.
- [ ] Implement native browser fullscreen and the supported Tauri equivalent/fallback with correct labels and recoverable failures. X closes playback to the Title page, never the desktop application window.
- [ ] Replace Escape-to-close behavior with popover dismissal/fullscreen exit priority. Stop/release the playback session once and retain resume data on every exit.
- [ ] Connect the ten-second next-episode state to the actual media-ended event. Read saved autoplay before starting it; pause while backgrounded, while a popover is open, or while keyboard focus is in the next-episode card.
- [ ] Cancel permanently ends that countdown until a new explicit playback/end cycle; leave Play next available. Use a polite status for transitions, not per-second announcements.
- [ ] Resolve and revalidate the next episode, stop the completed session, and start the new session once. Retain tracks when available, volume, quality, speed, and fullscreen. A failed/unavailable next start stays recoverable and must not skip forward or resume the timer.
- [ ] Stop at the selected season's final episode and after a movie. Clear timers/listeners on unmount, scope changes, seek away from end, or manual navigation; deduplicate repeated ended events.

Completion evidence: direct and HLS playback, real tracks, seek/transcode replacement, background/focus countdown pause, saved Off, Cancel/Play now, no wrapping, failed next start, fullscreen/Escape/X, and correct heartbeat/stop/resume behavior.

### T08 — Qualification and completion evidence

- [ ] Qualify the guarded testing workflow, record actual memory peaks for browser/build/native work, and preserve enough system headroom. Recheck memory before resuming heavy commands and document unavailable or held measurements accurately.
- [ ] Run the relevant checks below and resolve regressions introduced by this work. Do not substitute source-pattern verifiers for behavioral tests.
- [ ] Walk the full web journey and the shared Windows Tauri build/startup journey using real application data or isolated test fixtures.
- [ ] Verify keyboard-only navigation, focus return/visibility, dialogs, status speech with an available screen reader, contrast, reduced motion, RTL/localized labels, 320 CSS-pixel layout, and 400% zoom reflow.
- [ ] Capture review evidence for the actual Home, catalog/search, Title/Episode gallery, player, profile picker/menu, and preference page. Record native/assistive-technology checks that the environment cannot perform as specific release limitations rather than claiming they passed.
- [ ] Update this plan, UI Foundations, affected domain/contract docs, `BUILD_ORDER.md`, and `PROJECT.md` with implementation outcomes, commands/results, and commit references when available. Do not update CHANGELOG.

Completion evidence: actual production code implements the in-scope behavior, required automated checks pass, browser journeys are verified, desktop build/startup evidence is recorded, and any external device/assistive-technology qualification is precisely identified.

## State and accessibility acceptance criteria

Implement loading/empty/error behavior in each milestone rather than postponing it to a cosmetic final pass. An empty library, no resumable items, no search matches, unavailable media, and an API failure are different states. Retry the failed scope; keep usable content and drafts intact. Never show mock media as a fallback, expose private file paths, or persist signed stream/artwork URLs in navigation or saved preferences.

Use native links, buttons, form controls, and meaningful headings/landmarks. The profile popover is a disclosure with expanded state and ordinary Tab order, not a mixed-content ARIA application menu. Escape closes it and restores trigger focus; outside interaction preserves the user's new focus. Prefer native dialogs, with explicit cancellation focus fallback if the invoking popover item becomes hidden. These recommendations follow [WAI disclosure navigation](https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/examples/disclosure-navigation/) and [HTML dialog technique H102](https://www.w3.org/WAI/WCAG22/Techniques/html/H102); they are implementation techniques, not the complete conformance standard.

Player actions must work without a mouse, remain labeled, retain visible focus, and offer sufficient text/control contrast, following [W3C media-player guidance](https://www.w3.org/WAI/media/av/player/). Idle hiding must not strand focus or hide a focused control. Keep captions readable when controls or episode-end content appear. Live regions must remain inside the fullscreen player and survive rendering.

The three-second hiding delay and ten-second autoplay delay are product defaults. The saved Off path removes the content-imposed countdown before it is encountered; a Cancel button alone is not the basis for timing accessibility. Follow [Timing Adjustable](https://www.w3.org/WAI/WCAG22/Understanding/timing-adjustable). Aim for comfortable touch controls around 44px where practical, while separately checking [WCAG's minimum target requirements and exceptions](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum). Verify [reflow at 320 CSS pixels and 400% zoom](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html). Browser automation and fixture checks alone do not establish full WCAG conformance.

## Verification commands

Run commands from the repository root unless a working directory is shown. Discover and document prerequisites rather than inventing a pass. Keep package locks consistent; use the repository's established package manager. A documentation-only planning change needs link and diff checks, not application builds.

### Existing build and contract commands

```powershell
npm --prefix clients/web run build
npm --prefix clients/desktop run build
npm --prefix clients/desktop run tauri:build -- --debug
cargo fmt --all --check
cargo check -p duskcue
cargo clippy -p duskcue --all-targets -- -D warnings
node scripts/verify-client-contracts.mjs
node scripts/verify-client-fixtures.mjs
node scripts/verify-profile-selection-integration.mjs
node scripts/verify-profile-parent-unlock-integration.mjs
node scripts/verify-auth-conformance.mjs
node scripts/verify-playback-conformance.mjs
node scripts/verify-accessibility-input.mjs
node scripts/verify-design-assets.mjs
git diff --check
```

Run targeted Rust tests for the changed domains and database-backed tests for new queries/preferences. Document the exact test filters. Run `scripts/verify-migrations.ps1` against disposable PostgreSQL if schema work is introduced. Rust checks are required for Rust changes; broad checks should follow scoped passing checks when the impact warrants them.

### Web commands after T01 establishes the harness

```powershell
npm --prefix clients/web run check
npm --prefix clients/web run test:unit -- --run
npm --prefix clients/web run test:e2e
```

Before adding `check`, establish its baseline from `clients/web` with the installed SvelteKit/Svelte-check executables and `./jsconfig.json`. There is currently no standalone web lint script: add/document an appropriate scoped check if needed rather than claiming an unavailable command ran. The existing unit/e2e script names do not imply usable test suites until T01 adds their dependencies and configuration.

Behavioral coverage must exercise profile switching during pending work, concurrent preference saves/switches, query/season restoration, real player lifecycle, autoplay cancellation/deduplication, preference fallback, and recoverable failures. Avoid tests that only assert CSS class names or duplicate implementation logic.

## Execution and progress rules

Proceed milestone by milestone; T02 contracts precede dependent UI, and T06 preference persistence precedes final autoplay wiring. Shell work and isolated server/test work may proceed in parallel after T01. Give sub-agents explicit file ownership and integrate shared contract changes sequentially.

### Durable task checkpoints — user instruction October 7, 2026

Commit and push progress to `origin/main` at each coherent task boundary before starting the next task. The user explicitly authorized these recurring checkpoint pushes to protect work; no repeated approval is needed. Bring collaborating agents to a stable editing boundary, inspect the actual staged patch, run the relevant available lightweight checks, and include updated implementation/verification notes. Keep generated caches, local credentials, runtime data and build artifacts excluded. Use ordinary non-force pushes and retain the remote commit/branch evidence in the task report.

Checkpoint commits may preserve unfinished milestone source when the pending compiler/runtime/accessibility gates are stated accurately. They are durable work in progress, not completion, merge, deployment or release approval. Preserve the full T01–T08 objective, W3C/WAI requirements, composable service boundaries, current memory guards and the user's instruction to leave rust-analyzer running. Resume from the pushed checkpoint and push again after each subsequent coherent task, including fixes to failed verification. If a task cannot reach a coherent source boundary, preserve its patch locally and resolve that boundary before pushing it.

Keep the plan current after each milestone. Check a task only when its code and relevant verification are complete. Record the outcome and evidence in the log below, including remaining limitations. Do not mark historical build phases complete again or include unrelated working-tree changes in this work.

The goal is finished when the in-scope production implementation and required verification are complete, not when a screenshot resembles the prototype. Deployment and externally unavailable platform qualification remain separate work; report them accurately.

| Milestone | Status | Evidence / commit |
|---|---|---|
| Plan | Complete | Repository/code review and three sub-agent reviews; October 3, 2026. No production changes in this planning step. |
| T01 | Complete | Working Svelte-check/Vitest/Playwright harness, isolated multi-profile fixtures and seven browser smoke cases. Baseline builds/checks retained in `.cache/tonight-implementation/`; see [test harness](../ci/TONIGHT_WEB_TESTS.md). |
| T02 | In progress | Media/profile preference DB and race tests passed; library/collection/search PostgreSQL HTTP coverage passed. Search units: 6; structured-track fixtures: 6; contract manifest: 119 routes. HLS boundary: 9/9 and transcode-ID/profile binding: 1/1 passed on migrated PostgreSQL with actual FFmpeg fMP4 assets. Rust check/fmt pass; strict Clippy retains verified pre-existing diagnostics. Final integrated qualification remains open. |
| T03 | In progress | Tonight tokens, authenticated artwork, shell, profile disclosure and required picker implemented. Actual native primary-server startup, persistence, navigation and SSE passed before the strict zoom failure. Later new-navigation/header-Search/same-Title profile-switch locale repairs pass bounded source compilation but require fresh browser/check/build proof. Second-server replacement and final desktop qualification remain open. |
| T04 | Complete | Actual Home/catalog/library/collection/search services and galleries implemented. Fifteen browser journeys passed, including full URL/cursor return, failures, empty states and scope changes; see [browsing evidence](../design/TONIGHT_WEB_BROWSING.md). Final combined qualification remains T08. |
| T05 | Complete | Title/season/episode services: 15 unit tests; twelve Title browser cases passed across scoped runs, plus current canonical playback exit/episode return in the 13-case release/reflow rerun. Final combined/native qualification remains T08. |
| T06 | In progress | Save/Discard/draft/navigation guards: 26 unit and eight browser cases passed. Name management: five browser cases passed with exact name-only mutations, preserved Kids policy, shell reload and keyboard focus recovery. Actual player default/one-off/quality cases pass in the T07 scoped run. Native server switching and final lifecycle qualification remain open. |
| T07 | In progress | The integrated client passes 115 browser cases, including release/reflow, canonical X/Back, failed/lost-ACK retry, cross-tab cleanup, authentication recovery, actual burned captions and French/Arabic fallback language. Native HLS decoding, fullscreen fallback, manual next and complete Title return passed before the zoom failure. Focused real audio/selected-SRT output and four backend lifecycle checkpoint cases pass. Later stop/worker/mandatory Linux bootstrap changes require fresh compilation and actual DB/Linux launcher proof. |
| T08 | In progress | Verified checkpoints: zero check diagnostics, 372 units and 115 browser cases; later CSS Search correction passes 24 targeted cases/check/web build. The subsequent light locale-navigation repair has only bounded source compilation/syntax/catalog proof, requiring fresh shared checks and journeys. Earlier static/Tauri builds passed; native qualification failed Search occlusion at genuine 400% zoom. Corrected static build held before launch at 11.60 GiB commit headroom. Preserve editor/user workloads and normal gates; current native/DB/Linux/progressive-HLS/rendered qualification remains open. |

### Resume checkpoint — October 3, 2026

The Codex goal was paused with all three sub-agents stopped and changes preserved, then resumed. Work continues from the existing working tree rather than recreating completed changes. The mandatory W3C/WAI and modular-service constraints above apply to all remaining work. The following was the state at resume; subsequent milestone evidence replaces this checkpoint.

- T01/T03: the test harness and Tonight shell/Home exist; seven browser smoke tests passed. Final integration and milestone qualification remain open.
- T02: media and profile-preference database tests passed. The added library/collection HTTP test compiled but has not run against PostgreSQL; complete search contracts remain pending. The broader milestone is incomplete.
- T04: catalog services/components and collection detail exist. The latest patch is unverified; library route replacement, Collections list, and Search remain unfinished. Supply read-only active-profile context for Kids-aware command visibility.
- T05: Title services passed 15 unit tests; the first browser run had two passes and six failures. Pending work includes explicit label associations, fixture/locator corrections, Retry focus restoration, and file-loading error placement.
- T06: preference services and navigation guard passed 26 unit tests. Type-check fixes and explicit dialog focus were applied but need a check rerun; preference browser tests are written and unrun.
- T07/T08: production player/autoplay integration and final browser/desktop/accessibility qualification remain outstanding. Preserve the existing streaming lifecycle while implementing focused playback services.

### Earlier October 7 checkpoints — superseded by the current checkpoint below

- All current job handles are terminal; no active testing lock or owned browser listener remains. Preserve this working tree and the updated memory workflow on continuation.
- Player integration has real decoded/native-ended browser proof, valid VTT/sprite recovery, keyboard focus, safe canonical X return and cross-tab navigation gating. Full combined regression needs rerunning after the latest heartbeat/SSE integration.
- Heartbeats now have a focused per-store transport lifecycle; normal stop drains pending writes and repeated exit callers await the same cleanup before profile invalidation. Twenty-four integrated store/heartbeat cases pass.
- Selected-server desktop SSE now uses authenticated streaming fetch; ordinary browser cookie EventSource remains. Twenty-one parser/transport/store cases pass. Actual native SSE delivery and replay remain qualification gates.
- The current source check and 319-unit suite pass through the resource wrapper. Web/static/native builds are not claimed passed for this final implementation; the guarded bundling attempt was held before startup.
- Native runner and ignored isolated config are prepared at `.cache/tonight-desktop/fbb9e8ffc1564b2a9b7b33ab0fba18bb/`, including a QA-only actual WebView2 zoom permission. Build from current static assets, verify isolation/health/keyring/fullscreen/media and real 400% zoom before completion. No old native executable may substitute for this proof.
- Windows now has the explicitly approved temporary 8 GiB WSL cap; prior configuration/absence is backed up. Classifarr and Harmoniarr restarted with original IDs, running/healthy and no OOM flag. Recheck the cap after the new RAM arrives. Do not silently stop other user workloads to make a memory gate pass.
- The playback source audit found an unawaited physical FFmpeg release and retryable Stop failures being discarded. Focused backend worker/transaction helpers and client release recovery are being integrated; source preparation is not yet a compile/physical-exit/database proof. Do not mark T07 complete until those boundaries and retry behavior pass.
- Root tab coordination now uses an owned, current-server/account service with bounded channel/storage revision deduplication; seven new guarded units pass. A forced cross-tab profile change clears old playback while retaining same-account failed cleanup and a polite explicit retry. New browser failure/retry/focus coverage awaits the integrated client handoff.
- Short-height shell/header and preference-modal corrections are prepared alongside three 320×180 keyboard/reflow/representative computed-contrast journeys. Root Svelte source compilation passed with no warnings; actual web/native rendered checks remain required.
- The integrated release source check passed with zero errors/warnings; 28 focused release/store/control units and seven scoped tab-coordination units pass. The first 13-case targeted browser run passed eight: explicit failed-X retry, fixture lost-acknowledgement replay, untimed next after failed release, both successful/failed cross-tab switches, short-height basic player controls, protected PIN cancellation, and PIN expiry. Two new test cases omitted their API fixture; two actual short-height clipping/focus gaps and one real shallow-history Back restoration gap were found. Corrections are prepared and await rerun; backend idempotency/worker runtime proof, full regression and native packaging remain open.
- Those corrections now pass all 13 cases in 51.7s. Log: `.cache/tonight-implementation/release-reflow-browser-rerun.log`; output: `.cache/tonight-web/playwright/release-reflow-rerun-results`. Focused Episode/Cancel screenshots were inspected after the strict geometry checks passed. The latest full client check is clean and all 337 units pass in 28 files. The full browser suite is running; no web/static/native build or backend runtime pass is implied by these client results.
- The full browser suite is now terminal: 94/97 passed in 4.6 minutes. Three desktop cases used an outdated fake normalizer/command sequence; the strict fixture now accepts its known canonical origins and requires keyring read before native save. All five desktop cases passed in 19.9s on rerun. Logs: `.cache/tonight-implementation/tonight-current-full-browser.log` and `desktop-server-current-browser.log`. Production source did not change for those fixture corrections. This is complete current-case coverage across the full and targeted runs, not a claim that a single 97/97 full run occurred. Backend runtime and actual native builds/journeys remain required.
- Native preparation subsequently found the visible-controls transform could make a fixed popup relative to the wrong ancestor. Short-height placement now reserves the actual sticky heading/X; the popup owns final-frame focus scrolling and cancels its frames on close/unmount, while the outer player skips its descendants. Explicit sign-out remains possible after failed/expired playback release, checks captured server/account before clearing credentials, and discards old release state with authentication. All seven reflow/scope/401/503-sign-out cases pass in 35.7s; log `.cache/tonight-implementation/final-popover-scope-browser.log`. Source compilation of the current layout/Player/Popover has zero warnings. The final shared source-check attempt was held before startup at 3.48 GiB available RAM, commit 54.66/64.35 GiB; this is not a final check pass.
- Pinned Windows FFmpeg 8.1.2 is available only in ignored `.cache/tonight-tools/ffmpeg-8.1.2`, verified against the publisher checksum before extraction/execution. The current backend helpers also repair deleted-file FK recovery and the incompatible separate subtitle/scale filter paths. Those latest Rust changes and real DB/audio/caption/physical-exit tests remain uncompiled/unexecuted under the 12 GiB commit-headroom gate. No global PATH/install or further unrelated app changes occurred.

### Checkpoint — October 7, 2026, 18:31 UTC, superseded below

- Current shared source check passes with zero errors/warnings: `.cache/tonight-implementation/tonight-auth-final-check.log`. Current whole unit suite passes **358/358 in 30 files**: `.cache/tonight-implementation/tonight-auth-final-unit.log`. Both commands used the guarded workflow and finished without a memory stop.
- Authentication expiry on X now revalidates only the captured account/server. Confirmed expiry clears that scope; valid, unavailable and network-failed probes retain a focused explicit close retry. A newer account/server is protected from stale validation. The exact complete Title return survives normal sign-in and the required profile picker. Thirty focused authorization units and all five browser cases pass across their recorded runs.
- The latest whole 97-case browser run was 94 passing with three desktop mock-contract failures. Strict corrected fixtures passed all five desktop cases. The subsequent seven popup/scope/logout cases and five authorization cases pass across targeted runs. This evidence does not imply one green 104-case run; final combined qualification remains open.
- Current backend source compiled and all four worker lifecycle cases passed, including observed real FFmpeg progress followed by awaited termination before permit/cache release. The real PostgreSQL HTTP stop/heartbeat/seek and concurrency fixture is prepared but not yet compiled/executed.
- Actual cached-image output selected source-default audio index two and explicit index zero correctly, then exposed missing caption fonts. The runtime image has no system fonts; an explicit Inter diagnostic rendered the selected caption. The production Dockerfile now adds Noto providers. Default-fallback multilingual output and the unchanged production-argument fixture are being qualified against an isolated immutable derivative image; no deployment or application-service replacement is authorized.
- Native source and strict genuine 400% zoom runner are prepared. Current web/static/native builds and actual native startup, SSE, fullscreen, process cleanup and zoom remain required. Never substitute the old executable or narrow browser viewport for this proof.
- Final check sampled 1.132 GiB private memory and 1.140 GiB summed working set, with at least 5.225 GiB available host RAM. Whole units sampled 0.426/0.412 GiB respectively, with at least 5.269 GiB available. These are sampled workload observations, not proof that Windows freezes are fixed.
- The explicitly approved 8 GiB WSL cap remains in place; the previous configuration/absence is backed up and existing Docker services were restored with their original identities. Additional Tdarr and Ollama workloads were observed during qualification and are outside owned-test cleanup. Retain fresh resource gates and one heavy workflow; re-evaluate the temporary cap after the new RAM arrives.
- Goal status remains active. Remaining proof is actual backend database orchestration/caption output, current builds/native journeys, rendered contrast/caption review, and precise unavailable assistive-technology/direct-device observations. No deployment, commit or CHANGELOG update has occurred.

### Checkpoint — October 7, 2026, 19:35 UTC, superseded below

- Final current shared source check passes with zero errors/warnings, and **367/367 units pass in 31 files**. Logs: `.cache/tonight-implementation/tonight-current-final-check.log` and `tonight-current-final-unit.log`.
- The single current combined browser run passes **107/107 in 4.2 minutes**, including all 104 general cases and the three explicitly enabled actual-caption cases. Log: `.cache/tonight-implementation/tonight-final-full-browser.log`; output: `.cache/tonight-web/playwright/tonight-final-full-results`. This supersedes the earlier mixed full/targeted client checkpoint. Browser port 48037 was absent after terminal cleanup.
- Caption protection uses a per-Player layout service with explicit enablement, actual overlay bounds, fitted-picture aspect ratio, frame coalescing and complete listener/observer disposal. Video/source/session identity stays stable. Actual burned HLS glyphs remain visible and uncovered through controls, disclosures, fullscreen, autohide and real-ended Up next/Cancel. Short-height popovers at widths of at least 600 CSS pixels use 240 pixels, leaving a 380-pixel picture at 640×420 instead of 268. Low-resolution diagnostic text and offscreen/fully occupied extreme viewports remain explicit visual limits, not universal readability/conformance claims.
- The exact tested focused font configuration is now checked in with six Noto packages. Its immutable derivative passed inspected Latin/Arabic/CJK fallback and the unchanged production-argument audio/selected-SRT case, adding 255 MiB versus the broad candidate's 814 MiB. The existing local application image/services were not replaced. See [font evidence](../design/PLAYBACK_CAPTION_FONTS.md).
- Account-bound Stop now permits cleanup when an inactive original profile was deleted, preserves an existing frozen result and avoids history reassignment/recreation. Heartbeat/seek/stream restrictions remain. These nullable-profile and no-swap refinements, and their actual PG/HTTP/cache-failure/deletion fixture, require fresh current compilation/execution. The older cached manifest deliberately refuses their changed Rust bytes.
- The current production web build passes: `.cache/tonight-implementation/tonight-current-web-build.log`. The subsequent desktop static build was held before launch at 4.59 GiB available RAM and 10.81 GiB commit headroom; no native build/process or fresh manifest was created. Log: `tonight-current-desktop-build.log`; telemetry: `.cache/testing-memory/2026-10-07T19-43-38-668Z-85f7ecf9-55c1-4f87-8201-7441d8a787b8.jsonl`. Fresh current static build, isolated preparation, Tauri build and genuine native journeys still require normal admission. No old executable or October 3 static assets substitute for proof.
- The full browser run sampled 2.226 GiB owned private memory / 2.432 GiB summed working set, minimum host available RAM 5.451 GiB, and maximum system commit 53.763 GiB. Final source check sampled 1.192/1.200 GiB, minimum available 5.940 GiB; units 0.473/0.425 GiB, minimum available 6.637 GiB. All finished without a memory stop; Docker/WSL allocation is not separately attributed by owned Windows tree totals.
- The temporary 8 GiB WSL cap, 4 GiB swap and page reporting remain verified. Classifarr and Harmoniarr are currently healthy with no OOM flag. Classifarr has a newer externally replaced container identity; no service replacement was performed by this goal. After VS Code auto-restarted the approved stopped analyzer, the user chose to leave it running. Preserve the editor/user apps and use fresh gates; do not repeat process kills or change settings.
- Goal remains active. Current backend compilation/DB and actual native qualification are required; direct unavailable screen-reader/menu/tray/notification/display observations must be recorded precisely. No deployment, commit or CHANGELOG update has occurred.

### Verified checkpoint — October 7, 2026, 22:04 UTC

- After localization integration, the shared check passed with zero errors/warnings and **372/372 units in 33 files** passed. The complete browser run passed **115/115 in 4.5 minutes**, including three explicitly enabled actual-caption cases and eight French/Arabic preview journeys. Logs: `.cache/tonight-implementation/tonight-locale-combined-check.log`, `tonight-locale-combined-unit.log`, and `tonight-locale-full-browser.log`; results: `.cache/tonight-web/playwright/tonight-locale-full-results`. These are client fixture/encoded-media results, not database or native proof.
- Preview locales reuse 33 semantically matching existing catalog values per locale; unavailable new strings retain English with owning-element language metadata. Existing preview/review gates remain. The per-player choice parts preserve mixed language labels, and manual episode changes retain the localized playback URL. Both French/Arabic trusted-ended cases pass, including translated Cancel, English fallback status/action, exact next identity, matched start/stop counts and canonical complete Title return. Locale retention after leaving playback is outside those assertions. See [I18N evidence](../design/I18N.md).
- The current-at-21:52 ordinary web/static builds and isolated Tauri build passed. The actual native run then verified primary-server health/save, isolated native persistence, Windows keyring/device reload, SSE, navigation, HLS decoding, fullscreen/native fallback, trusted ended with saved Off, manual next and exact Title return. It **failed overall** at genuine engine zoom: the physical window remained 1280×720 while CSS became 320×180 at DPR 4, and Search's absolute submit button covered the focused input's two left corners. Strict center/four-corner checks were retained. Cleanup left zero native descendants and deleted only the verified generated test credential. See [native evidence and limits](../ci/TONIGHT_DESKTOP_TESTS.md).
- Search now places its 44-pixel submit button beside the flexible input. The subsequent accessibility/browsing/localization run passed **24/24**; current source check is **zero errors/warnings** and the ordinary web build passes. Logs: `.cache/tonight-implementation/tonight-search-focus-browser.log`, `tonight-search-focus-check.log`, and `tonight-search-focus-web-build.log`; results: `.cache/tonight-web/playwright/search-focus-results`. The preceding 372-unit/115-browser checkpoint predates this CSS-only correction; no later whole-suite run is claimed. Actual corrected native geometry and the new non-clipped CDP screenshot path remain unexecuted.
- The corrected desktop static build was held **before launch** at 5.57 GiB available RAM and commit 52.75/64.35 GiB: **11.60 GiB headroom**, below the unchanged 12 GiB reserve. Evidence: `.cache/tonight-implementation/tonight-search-focus-desktop-build.log` and `.cache/testing-memory/2026-10-07T22-04-18-294Z-d3e021a2-2e2f-433e-b533-5dcce1beb4b5.jsonl`. The next native run needs current static assets and a fresh isolated identifier/config/build; the previous executable cannot prove the correction.
- The 115-case browser run sampled owned private/working-set peaks **2.205/2.276 GiB**, minimum available **1.786 GiB**, maximum system commit **56.003 GiB**, without a memory stop. The isolated Tauri build sampled **3.092/2.840 GiB**, minimum available **2.157 GiB**, maximum commit **55.232 GiB**. These small reserves still support the user's resource concern; neither a passing job nor the cap proves Windows freezes are resolved. After the Search correction, the 24-case run sampled **2.065/2.138 GiB**, minimum available **3.876 GiB**. Telemetry is retained under `.cache/testing-memory/` with exact references in the test documents.
- Source review and a bounded exact-policy kernel reproduction confirmed that the former final pre-exec seccomp filter blocked initial execution with SIGSYS. The reproduction retained inherited filtering, no-new-privileges and **partial ABI-1 Landlock**, rather than claiming full ABI 3. Its Windows-bind single-file EACCES finding remains separate. A mandatory managed ELF bootstrap now supplies sealed policy and readiness acknowledgment for both playback and storyboards, with no exec/network allowance. Docker packaging and focused source tests are prepared; **C/Linux Rust compilation, actual managed launch/output/negative tests and current Windows/PG lifecycle tests remain unrun**. Whole Rust formatting passes. See [launcher authority](../ci/FFMPEG_LAUNCHER_QUALIFICATION.md).
- The temporary 8 GiB WSL cap remains. User instruction after the analyzer restarted is **leave rust-analyzer running; continue lighter work**. Preserve editor/user apps and do not retry process kills, lower compile gates or substitute stale cached Rust evidence. Reconcile source/documentation and prepare bounded qualification while heavier workflows are held. Goal remains active; no deployment, commit or CHANGELOG update occurred.

### Current light source work — October 7, 2026

- The first hosted runtime attempt at `04fea55` is terminal with successful Shared Web Checks and failed Linux/native qualification. Linux reached successful C/bootstrap/probe compilation, then the current test-helper compilation failed because `with_raw_output` followed `or_terminate`; locked API requires the opposite order. That order and a Linux `unused_mut` diagnostic are corrected in source, with formatting/diff checks passing. The retained Linux artifact reports no OOM and successful exact owned-container cleanup; runtime, SQL, producer and strict lint were not reached, and the dependent browser job was skipped.
- Hosted native installation, actual interactive prerequisite probes, normal-gated static/Tauri builds and refreshed prerequisites all passed. WebView2 attached at version `131.0.2903.86`, then the initial read-only isolation IPC lost its execution context during first document navigation. No isolation/credential/media/zoom journey check ran. The first-run Connect-to-server screenshot was inspected; native process-tree cleanup passed with zero remaining descendants and no page errors. Artifact `11520002970` is size/SHA verified in owned ignored cache. A bounded committed-document/first-run-gate/IPC readiness wait is being prepared; this is a test startup synchronization fix, not evidence that native journeys passed. No local compiler/app/runtime was launched.
- Both observed failures now have coherent source fixes: the local-managed output helper configures bounded raw output before termination, matching locked `tokio-process-tools`; a cfg-scoped Windows shadow avoids the Linux mutable-parameter warning. Native startup has one 30-second committed-app-document/visible-server-gate/IPC-bridge deadline before initial read-only isolation, with no mutation retry or assertion relaxation. Four startup tests pass at 64 MiB; whole Rust formatting and diff checks pass. Fresh hosted compilation/native/runtime/browser proof remains required; the previous failures are not relabeled as passing.
- Current-source hosted Shared Web Checks pass at `04fea552febbe0a3030f4b44954bd7abcab9603b`: Svelte checking reports zero errors/warnings, ten thin Node cases pass and **373/373 Vitest cases in 33 files** pass, for **383 total unit cases** across both runners. The ordinary production adapter-node build also passes. [Exact job](https://github.com/cloudbyday90/Duskcue/actions/runs/37706377561/job/113081617739). Managed Linux and actual native jobs remain live; the dependent full browser job has not started, so no new complete browser/caption/native pass is claimed.
- The hosted workflow is now source coherent for its first pushed run. Current helper execution passes **15 tests**, with two Linux-only group/drain cases correctly skipped on the Windows workstation and explicitly wired into the Linux job. Owned-container creation records pending identity before observation; cleanup recovers only matching names/labels/IDs. Command output honors backpressure and deadlines signal exact Linux process groups, including descendants holding pipes. The compiler produces current library/SQL artifacts, exact C/probe pairs and separately retained strict Clippy diagnostics; serial owned runtime cases and PostgreSQL exercise the actual managed launcher and HTTP lifecycle. Fresh caption production must succeed before browser artifact consumption. Hosted Windows prerequisites are re-probed after compilation without changing local guards. Rust formatting, YAML structure/pins, helper syntax and whitespace checks pass; actual hosted execution remains pending the push. See [web CI commands](../ci/TONIGHT_WEB_CI.md), [native qualification](../ci/TONIGHT_DESKTOP_TESTS.md) and [Linux readiness](../ci/FFMPEG_LAUNCH_READINESS.md).
- Remote packaging for `fbb3da1` now passes on Windows, macOS and Linux, including the current shared static frontend and normal bundle icons. [Packaging run](https://github.com/cloudbyday90/Duskcue/actions/runs/37701446159). Copyright and shared contract/binding/fixture jobs also pass; unrelated mobile/device-SDK failures remain separate from these successful desktop jobs. Current Linux server image compilation fails at the incomplete shutdown builder typestate; the corrected complete Unix/Windows builder is prepared and needs the next actual compile. Successful desktop packaging does not prove native UI journeys or server runtime.
- The source-prepared Tonight Qualification workflow now separates shared check/unit/build, bounded owned Linux compilation/managed cases/SQL/producer, full browser journeys with validated fresh caption artifacts, and actual isolated hosted Windows native journeys. Windows reuses the existing normal memory gates and strict zoom/cleanup tests. Host-only installer/probe proof is source/job/static-build scoped; no local download/install/build/runtime occurred. Four hosted prerequisite and five artifact/result helper tests pass at 64 MiB. Artifact consumption verifies unchanged committed source, exact producer and image identities, streamed executable hashes and nine bounded assets; the browser result gate requires actual caption, progressive and locale execution and reports retries/flakiness. The workflow is being reviewed before its first push/run; these are not runtime passes.
- The icon-only task checkpoint is pushed to `origin/main` as `c6dd202`. Subsequent task source now includes per-Player progressive HLS timeline ownership, known full runtime during partial publication, explicit stream-relative zero, final-manifest/natural-ended gating for MSE and disposal guards. Seven thin Node tests pass at 64 MiB, including the actual locked Hls configuration validator; Player's standalone compilation has zero warnings. One ordinary lifecycle case and three actual progressive-media browser cases are prepared but unrun. A controller reuses existing encoded segments for genuine growing EVENT/final ENDLIST responses; its three thin parser checks pass. Standard unit execution now includes the thin timing tests before Vitest. Production EVENT publication, native-HLS final metadata, full shared checks/builds and actual decoding remain separate open proof.
- Source-prepared `linux-cargo-check` uses the unchanged normal Cargo/12 GiB Windows gate and a separate fresh local WSL gate/boot identity. Fixed four-GiB/no-extra-swap/two-CPU/256-PID/offline two-job container compilation, source hashes, bounded logs/deadline and context-bound owned cleanup are prepared. Eleven compiler helpers plus two preset checks pass at 64 MiB; actual read-only engine/WSL sampling succeeded, but the prepared local compiler image remains absent and no compiler/container was launched. This does not replace real Linux compilation/runtime qualification.
- Required legal notices are being restored without explanatory code comments. The two unchanged existing October 3 SQL migrations are exempt only under exact checked hashes shared by copyright check/update/add tooling; the original legacy cutoff is retained, later migrations still require notices, and three thin tests reject mutation of the frozen pair. The combined lighter test run passes **26/26** (seven timing, three progressive fixture, eleven compiler, two presets, three migration policy) in 1.5 seconds, each Node process capped at 64 MiB. No complete Vitest/browser/build pass is implied.
- Docker CI for `36c9fd3` reached successful C bootstrap compilation, then failed actual Linux Rust compilation at `sandbox/owned_output.rs` because `GracefulShutdown::default()` is unavailable in the locked library. The real builder API repair is being integrated by the owning agent. The production image, bootstrap runtime/output/negative tests and the newer readiness source remain unqualified; checkpoint-specific compile output is evidence, not a current whole-source pass.
- The readiness task is now source coherent: start/transcode/remux returns and seek replacements await one 15-second bootstrap/first-playable deadline under owned cancellation. Bounded cache/playlist/MP4 inspection distinguishes natural success, failed execution, cancellation and physical cleanup; ten new Rust cases are prepared but uncompiled/unrun. The observed Linux `GracefulShutdown::default()` error now uses the actual builder with the existing ten-second resource default. Whole Rust formatting passes. Production VOD and the final syscall filter remain unchanged, so long progressive startup is still unfinished. Required legal notices and generated availability notices are restored; copyright checking passes on the currently tracked source, and frozen SQL bytes remain unchanged. Fresh complete checks/builds, the newer Rust/DB/Linux/native journeys and strict Clippy classification remain open.
- The full Tonight checkpoint `36c9fd3` is pushed to `origin/main`; recurring coherent task checkpoints now use that branch. Its GitHub Actions shared contract/binding/fixture jobs passed, while Linux/macOS Tauri compilation exposed missing `icons/icon.png`. The normal desktop icon assets are now generated from the existing approved app-icon SVG and all seven explicit bundle paths/PNG-alpha/ICO/ICNS bounds pass lightweight checks. The icon was inspected. Fresh compilation/native runtime after that asset repair remains pending; this is a distinct coherent checkpoint task. Copyright CI also found missing legal notices, including pre-existing Fire/SQL files; required notice repair is being reviewed without altering applied migration bytes.
- New Tonight Movies/TV/Collections navigation, the reachable header Search callback and Viewing preferences link now preserve the locale through existing `localizeUrl`. Same-Title profile switching validates the de-localized canonical destination, captures its localized form before asynchronous work, and retains the complete query. Two changed Svelte components pass standalone compilation with zero warnings at a 128 MiB heap; test syntax, catalog availability and scoped diff checks pass. Six existing French/Arabic case sources now cover these actual navigation paths, but **those expanded assertions and fresh complete check/build/runtime results are pending**. Earlier 372/115 and post-Search 24-case/check/build evidence predates these bytes. Legacy navigation and canonical player X exit keep their stated limits.
- Post-Search web Home and discard-dialog captures at 320×180 were directly inspected: the Search button sits beside the focused field and the focused discard action is exposed within the scrollable dialog. These ordinary web captures are not native engine-zoom or screen-reader speech proof. Updated documentation has valid local links, UTF-8/LF and no trailing whitespace.
- Fresh unlaunched native preparation is `.cache/tonight-desktop/faa19282bce74434a8a842c650b3642d/`, using its own identifier/config/manifest. Build current static assets first, then build/run that configuration through separate normal gates. Native screenshot capture and submit-button exposure assertions remain strict and unexecuted. No heavy job, testing lock or QA listener remains active in this interval.
- A further existing HLS startup gap is confirmed in production argument source and official [FFmpeg n8.1.2 HLS code](https://raw.githubusercontent.com/FFmpeg/FFmpeg/refs/tags/n8.1.2/libavformat/hlsenc.c): `hls_playlist_type=vod` postpones the playlist until trailer. This is a source finding, not an observed current managed-program failure. [FFmpeg EVENT mode](https://ffmpeg.org/ffmpeg-formats.html#hls-1) is the narrow progressive candidate, retaining all segments and final ENDLIST. Production arguments and the filter have not changed during this review.
- Required progressive-HLS work before T07 completion: qualify atomic manifest publication and its exact rename syscall within existing cache/path boundaries; retain exec/network denial; await the first playable playlist plus referenced init/complete segment under the owned pending guard before start/seek publication; preserve known item runtime rather than the growing produced window; start at stream-relative zero instead of live edge; and prove genuine final media end, seek offsets, original-profile binding and physical release. Do not retry arbitrary authorization 404s, weaken path grants or infer completion from a growing duration. Managed-output tests now split finite final-output from in-flight cancellation, with actual PID presence/absence assertions. The prepared set has ten units and five ignored Linux cases, all uncompiled/unrun. Actual manager/HTTP and progressive-HLS browser/native evidence are required; see [exact readiness commands](../ci/FFMPEG_LAUNCH_READINESS.md).
- There is currently no admitted Docker compiler preset: unit-policy font/kernel probes cannot be repurposed. Fresh Linux compilation needs a normal cargo/build-policy owned runner, explicit two-job limits inside the container, bounded RAM/no swap/CPU/PIDs and immutable image/source/artifact provenance. Dockerfile Cargo compilation now explicitly uses `-j 2`, a source setting still awaiting current-image qualification; independent frontend/Rust stages also need bounded sequential qualification. Whole `cargo fmt --all -- --check` passes after the test corrections. No Rust/C compiler, container or new heavy frontend workflow was launched during the lighter interval.

## Related authorities

- [Context](../../CONTEXT.md), [Build Order](../../BUILD_ORDER.md), and [Project](../../PROJECT.md).
- [Project Structure](../design/PROJECT_STRUCTURE.md) for SvelteKit, ES modules, shared services, and server domain conventions.
- [Client Contracts](../api/CLIENT_CONTRACTS.md) and [API Conventions](../design/API_CONVENTIONS.md) for response/binding and pagination/error contracts.
- [Profiles and Ambient Channels](../design/PROFILES_AND_AMBIENT_CHANNELS.md) and [Auth](../design/AUTH.md) for selection, ownership, PIN, remembering, and scope invalidation.
- [Streaming](../design/STREAMING.md) and [Quality Management](../design/QUALITY_MANAGEMENT.md) for playback lifecycle and quality boundaries.
- [Client Accessibility and Input](../design/CLIENT_ACCESSIBILITY_INPUT.md), [Client Design Assets](../design/CLIENT_DESIGN_ASSETS.md), and [I18N](../design/I18N.md).
- [Desktop and Mobile Clients](../design/DESKTOP_MOBILE_CLIENTS.md) and [Client CI Smoke Harness](../ci/CLIENT_CI_SMOKE_HARNESS.md) for shared desktop packaging and qualification.

## Codex goal brief

Implement the Tonight redesign in the actual Duskcue web and shared Tauri desktop code by completing T01–T08 in this plan. Apply current official W3C/WAI best practices, document sources and distinguish standards from product choices, and verify the resulting interaction and accessibility behavior. Prioritize modular, composable service files with explicit responsibilities and cleanup over large singleton files; review these boundaries at every milestone. Use UI Foundations for accepted behavior and this plan's explicitly labeled defaults for remaining reversible choices. Preserve existing auth, profile/Kids, access, streaming, resume, notification, localization, and desktop behavior; add the required browsing and typed active-profile viewing-preference contracts. Use sub-agents for independent work, keep milestone evidence current, and finish with relevant builds/tests and verified browser/desktop journeys. Commit and push coherent task checkpoints to `origin/main` between tasks under the user's standing authorization, with unfinished verification labeled accurately and the full goal retained. Do not ship prototype data or deploy/publish as part of the goal.
