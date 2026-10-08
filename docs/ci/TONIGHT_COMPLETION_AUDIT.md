# Tonight completion audit — October 8, 2026 UTC

The in-scope T01–T08 implementation and available qualification are complete at code checkpoint `c1859eb8ecea45c34c75f665e15615d2ecb8cf4d`. This audit covers the full [implementation plan](../branding/TONIGHT_IMPLEMENTATION_PLAN.md), accepted [UI Foundations](../branding/UI_FOUNDATIONS.md), current source and retained runtime evidence. Documentation-only descendants preserve this tested code. Historical failures remain in Git history, the plan and domain qualification notes.

## Current verification

[Run 37748877546](https://github.com/cloudbyday90/Duskcue/actions/runs/37748877546) completes all five qualification jobs successfully:

| Boundary | Actual result |
|---|---|
| Shared checking and production web build | Zero Svelte errors/warnings; ordinary adapter-node build passes |
| Shared units | 37 thin Node plus 375 Vitest = **412/412**, no skipped thin cases |
| Complete browser journeys | **120/120 cases and 120 attempts**, zero unexpected, skipped, retried or flaky cases |
| Required browser scopes | Captions 3, progressive 3, locales 8, real background 1, remembering 1; all first attempt |
| Actual Windows Tauri | All twelve groups, both prerequisite probes, static/debug-native builds and owned cleanup pass |
| Native rendering | 37 strict focus checks, 42 matching PNG dimensions, genuine 400% engine zoom; supported contrast samples pass |
| AMD64 / ARM64 backend | **48/48 / 47/47**, all seven managed cases, fresh caption/audio producer and mandatory real PostgreSQL progressive HTTP pass |
| Strict Clippy | Exit **101**, thirteen independently classified baseline diagnostics; zero introduced helper diagnostics |
| Final lighter checks | All eight plan-listed verifiers, whole Rust formatting and whitespace pass |

A green qualification job does not turn strict Clippy into a clean pass. Its thirteen diagnostics remain visible and match the verified baseline; the two introduced borrowed-path diagnostics are removed without allowances.

## Every milestone task

The identifiers below follow each milestone's checkbox order. Browser names refer to files under `clients/web/tests/e2e/`; unit names refer to `clients/web/tests/unit/`.

| Task | Implementation and authoritative evidence |
|---|---|
| T01.1 | `CONTEXT.md` and initial tree inspected; user changes preserved. Final code checkpoint and ordinary main/origin pushes are recorded; generated caches remain ignored. |
| T01.2 | Original web/check/static/server baselines and pre-existing lint distinctions are retained in [web harness](TONIGHT_WEB_TESTS.md), the plan and qualification notes. |
| T01.3 | Locked Svelte-check/Vitest/Playwright harness runs from a fresh hosted checkout: zero diagnostics, 412 units and 120 journeys. |
| T01.4 | Test-only account/profile/catalog/media/error fixtures cover standard/Kids profiles, seasons, watch/resume/completion, failures and artwork. Unexpected API requests/page errors fail actual-route journeys. |
| T01.5 | Typed contracts, bindings and domain notes are implemented; final verifier admits 119 routes and 20 fixtures across 17 domains. |
| T02.1 | Profile/access-scoped Continue and complete ordered season/episode queries in `domains/media/browsing.rs`; retained real PostgreSQL browsing tests and current browsing/Title journeys. |
| T02.2 | Complete search/catalog/collection filters, facets, sorting and cursor scope; actual PostgreSQL navigation test traverses ties, null years, full collections and Kids restrictions. Current `browsing.spec.ts` passes. |
| T02.3 | Actual stream indices/defaults and known description/SDH metadata; current production-argument producer decodes default index two at 880 Hz, explicit zero near 220 Hz and selected SRT ordinal one. Unknown metadata is not invented. |
| T02.4 | Focused `profiles/viewing_preferences.rs`, typed DTOs, metadata siblings, active expected-profile checks and consistent transactional locking; retained real database/race proof against unchanged domain/test inputs. |
| T02.5 | Namespaced profile API and saved-versus-unset model, explicit bindings and device ownership; `viewing-preferences`, runtime/track units and current preference journeys pass. |
| T02.6 | Real database ownership/selection/Kids/unknown-field/stale-save/unlock/switch races retain their exact source boundary. Current profile/PIN/scope journeys supplement this server proof. |
| T03.1 | Actual `app.css` semantic charcoal/plum/lavender tokens and serif headings; current inspected native/browser surfaces. Shared mobile/TV token contracts are preserved. |
| T03.2 | Responsive top navigation, submitted Search, notifications, locale, Settings and capability-filtered administration; current routes/browsing/localization/native navigation and events evidence. |
| T03.3 | Focused picker/disclosure components retain required selection, quick-switch omission and real parent unlock. Current selection/PIN/Remember cases and native picker/focus checks pass. |
| T03.4 | Authenticated selected-origin artwork service/component owns requests/object URLs and fixed aspect ratios; artwork units, failure/RTL/title journeys and native server switching pass. |
| T03.5 | Current guarded static/debug-Tauri build/startup and bounded internal navigation pass; native journey rejects external navigation and preserves selected-origin authentication. |
| T04.1 | Actual scoped Continue, smaller optional feature and recent data; Home/browsing journeys and inspected native Home. No sample fallback is in application source. |
| T04.2 | Poster galleries with complete pagination, titles, favorites and supported filters/sort; current browsing cases and native catalog capture. |
| T04.3 | Submitted query/facets, stale-response ownership and complete URL/cursor return; current search/catalog restoration journeys. |
| T04.4 | Real collection listing/membership, scoped reads and pagination; current collection journeys and retained PostgreSQL coverage. |
| T04.5 | Independent empty/failed/pending/unavailable/artwork states and explicit retry; current route/browsing/Title failure cases. |
| T05.1 | Full movie/series metadata, artwork, synopsis, watch/favorite/rating and file disclosure; current Title journeys and inspected captures. |
| T05.2 | Complete season traversal and ordered episode gallery with actual availability/watch/resume; Title units and multi-season journeys. |
| T05.3 | Canonical series/season/episode/origin survives playback and reload; exact query assertions pass in all progressive and ordinary exit cases. |
| T05.4 | Fresh watch data, healthy-file selection and unavailable/unknown/deleted targets remain recoverable; current Title/playback entry and failure cases. |
| T05.5 | Scope-owned watch/favorite/progress state and actual completion; current profile-switch/Title/Continue cases plus real original-profile Stop database proof. |
| T06.1 | Labeled profile/device fieldsets, Save/Discard and stable result status; all preference journeys and current native preferences group. |
| T06.2 | Draft failures and guarded links/Search/history/profile exits; navigation/preference units and actual discard/keep-editing cases. |
| T06.3 | Scope reset, stale read rejection and expected-profile writes; units, current server/profile switching and retained transactional races. |
| T06.4 | Legacy Off/unset/saved rules, real semantic track matching and separate device quality; current runtime/track/storage units and actual-media retained-settings cases. Volume keeps its documented local ownership. |
| T06.5 | Existing create/delete/parental guards, name-only editing, PIN recovery and explicit remembering preserved. Current Remember journey proves true, omitted pin preservation and false removal with exact payloads; five name cases and PIN/selection cases pass. |
| T07.1 | Minimal transport and vertical Episodes/Audio/Settings disclosures with real supported choices; current controls, actual caption/media and native HLS groups. |
| T07.2 | Per-player idle visibility, focus/hover/seek/reveal, annotations/storyboards and telemetry ownership; current controls/reflow/annotations/recovery and units. |
| T07.3 | Browser fullscreen and supported native fallback; actual Escape/X/Title return and app-window retention pass, including genuine native zoom. |
| T07.4 | Disclosure then fullscreen Escape priority; one durable release, drained heartbeat and exact Title/resume context. Browser recovery/lost-ACK cases and real concurrent/idempotent Stop/physical cleanup pass. |
| T07.5 | Trusted natural media end, saved Off before timing, focus/menu holds and actual tab backgrounding. Current trusted Hidden/Visible evidence freezes seven seconds for 11,001.647649 ms and restores seven without catch-up. Native minimization separately freezes nine for 11,008 ms. |
| T07.6 | Permanent Cancel, untimed Play next and polite transition-only status; current Cancel held untimed for eleven seconds with no new start, plus current native/DOM/focus evidence. Spoken AT remains unavailable. |
| T07.7 | Complete selected-season next resolution/revalidation, previous release before one new start, retained settings/fullscreen and untimed failure recovery; current next/autoplay/release cases and runtime units. |
| T07.8 | No movie/season wrap, seek/manual/scope disposal and repeated-ended deduplication; current browser and per-instance generation units. Real backend observer/seek/cache/permit cleanup passes. |
| T08.1 | Guarded local admission/ownership, bounded hosted workers/heaps/containers, actual retained browser and current static/native memory samples; user editor/services preserved. See [memory evidence](TESTING_MEMORY.md). |
| T08.2 | Current builds/checks, 412 units, 120 journeys, both native backend lanes and eight final verifiers/formatting pass; introduced regressions resolved. Baseline strict lint is classified, not hidden. |
| T08.3 | Full fresh web journey and actual isolated Windows Tauri build/startup/navigation/media lifecycle pass. Fixtures are explicitly allowed by the plan; live-server coupling is not implied. |
| T08.4 | Actual keyboard/focus/dialog/status DOM, supported contrast, reduced motion, French/Arabic RTL, 320-pixel and native 400% checks pass. Unavailable speech/physical OS observations are recorded below. |
| T08.5 | Current native Home/catalog/Search/Title/Episode gallery/player/picker/disclosure/preferences and browser caption/RTL/reflow captures inspected; dimensions/identity match recorded artifacts. |
| T08.6 | Final plan, UI Foundations, domain qualification, project/build-order and this audit are reconciled; ordinary main/origin checkpoint required before goal completion. No CHANGELOG or deployment. |

## Real browser ownership and timing proof

The exact report has 120 cases and 120 attempts, with zero skips, retries or flaky results. The same-window headed Chromium 153 case uses the public existing default context with `noDefaults`; actual child arguments contain no headless flag. Trusted Hidden occurs at 7088.8 ms and Visible at 18118.4 ms. Seven seconds remains unchanged while hidden for 11,001.647649 ms, with one start, unchanged source and no focused card/open menu. Restore begins at seven then decrements normally. Cancel remains untimed after another eleven seconds; X returns to canonical Title and produces exactly one matching Stop.

The original owned Chrome PID 6822 exits zero, without a signal or kill fallback; its fixture teardown records `cleanup.passed=true` and `exited=true`. This was inspected directly from the retained report, not inferred from test intent. Browser archive `11537792213` is 1,643,873 bytes, SHA256 `3bf72d41773147878aaea8d8260e49612a3cb4c20330ada48fbc08e4f23316ad`. [Web CI](TONIGHT_WEB_CI.md) records the original-session capture cause and public owned lifecycle.

## Backend, native and resource evidence

Both architectures bind the exact code commit and complete 406-file backend input inventory, aggregate `d9277be6f6d658643312fee592a6d14ff08a3a34c0e0ff5cca74545e156759c8`. Four native artifact hashes/ELF headers and nine generated media hashes are verified. First complete EVENT frames decode while the physical child runs; independent managed completion reaches real ENDLIST/Succeeded. Mandatory real PostgreSQL/HTTP start/seek/profile/capacity/physical cleanup passes with the actual 600.554-second source and four relative-zero samples. See [launch readiness](FFMPEG_LAUNCH_READINESS.md) and [HTTP qualification](TONIGHT_PROGRESSIVE_HTTP_TESTS.md).

[Current desktop proof](TONIGHT_DESKTOP_TESTS.md) records twelve actual groups, 37 focus checks, 42 captures, both prerequisite probes, generated credential/session/process cleanup and current measured peaks. Hosted static/Tauri/native-QA private peaks are 1.314/3.175/0.568 GiB; working peaks 1.124/2.822/0.987; minimum available RAM 12.569/10.502/12.886. All exit zero without a memory stop.

The historical guarded 115-case browser workload measured 2.205 GiB private/2.276 working peak and minimum 1.786 GiB available. The current hosted 120-case job provides one-worker/heap bounds and cleanup, not a fresh browser RSS peak. Linux compiler/runtime 4-GiB/512-MiB limits and no-OOM results are limits, not measured peaks. The temporary WSL cap and read-only local snapshots do not prove a Windows-freeze fix.

## Explicit qualification limits

- Spoken screen-reader output, physical OS scaling and direct OS menu/tray/notification interactions were unavailable. DOM/keyboard/engine-zoom checks are not claimed as those observations or universal WCAG conformance.
- Browser/native client journeys use actual decoded media and isolated API fixtures. Real Rust/PostgreSQL/manager/component proof is separate; browser-to-live-server coupling remains unqualified.
- The HTTP fixture explicitly does not observe final encoder status zero. Independent managed worker Succeeded proof is separate and does not fill that HTTP field.
- Actual selected-caption/font/track samples do not establish universal script, ASS layout, every codec or legacy-file reprobe conformance.
- Native preview speech availability, authenticated server event replay and external mobile/TV/controller qualification retain their existing platform/release boundaries.

These limits are the plan's specified available-environment reporting boundaries, not a reduction of implementation scope. No required implementation or available automated/native verification remains unfinished.
