# Tonight Web Verification

Created October 3, 2026. Scope: the production SvelteKit web client and the web code shared by Tauri. This document records the T01 test-harness decisions for [Tonight implementation](../branding/TONIGHT_IMPLEMENTATION_PLAN.md). Test data belongs under `clients/web/tests/`, never in application source or a production fallback.

## Research and recommendation

Svelte recommends Vitest for Vite/SvelteKit unit testing and Playwright for complete application journeys. Unit tests suit isolated request/state behavior; browser tests exercise actual SvelteKit routes, hydration, navigation, and controls. Component DOM tests require a browser or DOM environment; the initial harness does not need another simulated DOM dependency because route interactions run in Chromium. See [Svelte testing](https://svelte.dev/docs/svelte/testing).

Use stable Vitest 5.0.3, its matching `@vitest/coverage-v8` provider, and Playwright Test 1.63.0, confirmed through `npm view vitest version engines peerDependencies --json` and `npm view @playwright/test version engines --json` on October 3, 2026. Vitest 5 supports the existing Vite 8 and requires Node 22.12 or newer within its supported engine ranges; the verified workspace runs Node 24.18.1. Prefer Node 24 LTS for this harness. Lock resolved packages with the existing npm lockfile. See [Vitest installation](https://vitest.dev/guide/) and [Playwright installation](https://playwright.dev/docs/intro).

Use a separate `vitest.config.js` for Node tests of existing JavaScript modules, with an explicit unit-test include pattern. This avoids starting the production Paraglide/SvelteKit plugin pipeline for request-helper tests and prevents Playwright specs from being collected as Vitest tests. The tradeoff is that this configuration does not compile Svelte components; add a separate Svelte/browser project when a concrete component test needs it. See [Vitest configuration](https://vitest.dev/config/).

Use Playwright's managed `webServer` to run the actual Vite application on a dedicated loopback port with `--strictPort`. Do not reuse an unknown existing server. The browser context and fixture state are fresh for every test. Start with Chromium for a reproducible baseline; other browser engines and native Tauri qualification remain separate checks. See [Playwright web servers](https://playwright.dev/docs/test-webserver) and [test fixtures](https://playwright.dev/docs/test-fixtures).

Intercept client API requests before navigation with Playwright routing. This provides deterministic profile/catalog/error cases while the page itself remains a real production route. Unknown API requests must fail the test rather than reaching a developer's backend. Blocking service workers keeps interception predictable. This does not test Rust authorization, PostgreSQL transactions, server rendering of API data, or actual media decoding; those require separate server/integration/playback checks. See [Playwright API mocking](https://playwright.dev/docs/mock) and [network interception](https://playwright.dev/docs/network).

## Harness boundaries

- `check` runs SvelteKit sync, compiles Paraglide using the application's URL/cookie/preferred-language/base-locale strategies, then runs `svelte-check --tsconfig ./jsconfig.json`. This recognizes new message keys on a fresh checkout and exposes diagnostics rather than suppressing them.
- Vitest tests import the actual API core. Cover selected-server URL construction, RFC 9457 failures, profile-scope cancellation and stale-response rejection, and the explicitly unscoped profile-selection boundary.
- Playwright begins with actual `/dashboard` profile gating/navigation and `/media/[id]` rendering. Assert meaningful visible behavior and API requests, not CSS class names.
- Reusable test-only data includes two standard profiles, a protected Kids profile, movies, two seasons and episodes, resume/completed watch rows, unavailable files, missing artwork, and injectable API failures.
- Browser fixtures may seed the existing cached account storage to enter a selected/selection-required test session. They do not establish production authentication or grant actual privileges.
- Profile switch and parent-unlock fixtures reproduce the existing response shapes and request boundaries. Future viewing-preference/episode endpoints are added only after their production contracts exist.
- Unit/e2e discovery patterns remain separate. Browser reports, traces, screenshots, and result files go into ignored `.cache/` paths.
- Coverage measures the API core, artwork API, navigation helpers, and focused browsing services; it is not a repository-wide UI coverage claim. Follow the existing [CI testing](CI_TESTING.md) defaults of one CI retry and a trace on the first retry.
- Tests must retain original errors as useful diagnostics; do not alter production components merely to make a baseline assertion pass.

## Commands

From the repository root, after dependencies and Chromium are installed:

```powershell
npm --prefix clients/web ci
npm --prefix clients/web exec -- playwright install chromium
npm --prefix clients/web run check
npm --prefix clients/web run test:unit -- --run
npm --prefix clients/web run test:unit -- --run --coverage
npm --prefix clients/web run test:e2e
npm --prefix clients/web run build
npm --prefix clients/desktop run build
```

The check/unit/e2e commands are executable. Playwright downloads its matching browser separately from npm package installation. On Linux CI, install the supported browser system dependencies using Playwright's documented installation options. A browser-install failure is a missing prerequisite, not a passing browser test. In Windows PowerShell with the installed npm 12 wrapper, use `npm.cmd` when forwarding options, for example `npm.cmd --prefix clients/web run test:unit -- --run --coverage`; `npm.ps1` rejected the forwarded `--run` flag before the runner started.

These commands supplement the repository's contract, profile, auth, accessibility, design-asset, and playback fixture verifiers. Source-pattern and fixture checks do not substitute for route behavior or prove full accessibility conformance. Backend-changing milestones also require targeted Rust/database checks as specified in the implementation plan.

For the current Windows qualification, use the [testing-memory wrapper](TESTING_MEMORY.md) instead of launching overlapping raw checks/builds:

```powershell
node scripts/testing-memory/run.mjs web-check
node scripts/testing-memory/run.mjs web-unit
node scripts/testing-memory/run.mjs web-e2e
node scripts/testing-memory/run.mjs web-build
node scripts/testing-memory/run.mjs desktop-build
```

Run them sequentially. A held preflight or another live workflow is not a passing test. Native qualification also uses its guarded presets and [isolated runner](TONIGHT_DESKTOP_TESTS.md).

## Results

The integrator captured the original baseline before dependency/configuration edits: web build and desktop static build passed, and Svelte check returned zero errors/warnings.

On October 3, 2026, the harness installed Vitest 5.0.3, matching V8 coverage, Playwright 1.63.0, and Chromium. `npm run check` returned zero errors and warnings. The initial `npm.cmd run test:unit -- --run --coverage` passed 44 tests in three files, with scoped coverage of 80% statements and 79.85% lines; the desktop route helper had 100% line coverage. Six browser journeys passed in the first corrected full run; the anonymous case found an actual Svelte layout cleanup loop. After the shell owner fixed that loop, the targeted regression and a full seven-journey rerun passed.

The subsequent T04 integration expanded reusable fixtures to scoped library/collection/search reads, complete season/episode pagination, and active-profile preferences. A configurable test page cap forces real route continuation/replay without shipping fixture data. The suite retains API-error injection and fails unknown requests/runtime errors. `npm.cmd run test:unit -- --run` now passes 115 tests in ten files. The full check was clean after the initial T04 routes; final integrated check/build evidence follows remaining playback integration. The 44-test coverage percentages above remain historical baseline figures rather than a claim about later modules.

The final T04-inclusive `npm.cmd run test:e2e` executed 42 actual-route Chromium journeys: 41 passed, including all 15 browsing cases, all seven baseline cases, all eight viewing-preference cases, and 11 of 12 Title cases. The remaining Title profile-switch case exposed the shared shell's Home destination. After the shell retained the safe complete Title URL, the unchanged targeted regression passed. A subsequent targeted submitted-search run passed with ascending relevance represented by its native Sort option. These targeted checks do not claim a later all-green full-suite run. Final full-suite/check/build results after remaining playback integration belong in the implementation plan and relevant milestone evidence.

Initial harness-only failures were fixed: API interception accidentally matched Vite source-module paths, test fixtures needed TypeScript generics, and generated locale messages needed compilation before checking. Browser fixtures now match only the actual `/api/v1` prefix, and fail on unhandled API requests or page runtime errors.

The install audit reported four dependency advisories. The affected resolved packages were unchanged from the original lockfile: SvelteKit 2.69.1, devalue 5.8.1, nanoid 3.3.15, and PostCSS 8.5.16. This harness change does not claim remediation or run a broad audit fix.

### Short-height focus and contrast — October 7

Rechecked official [Reflow](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html), [Focus Not Obscured](https://www.w3.org/WAI/WCAG22/Understanding/focus-not-obscured-minimum.html) and [Contrast Minimum](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html) guidance. The surrounding interface must remain usable when magnified; the video aspect ratio does not exempt navigation, fields, disclosures or next actions. The Understanding documents are informative explanations of the published WCAG criteria.

The shell's sticky multi-row navigation could cover focused content in a short viewport. Its header now follows ordinary document flow at heights of 420 CSS pixels or less, and the preference discard modal explicitly scrolls within 90dvh. Three prepared `accessibility.spec.ts` journeys use a 320×180 CSS-pixel viewport to check Home/search, Title actions, preference fields and discard controls, every profile disclosure choice, and protected Kids PIN/error/cancellation. Native Tab and scroll must expose the actual focused control with its visible outline and unobscured center; document width must not overflow. Viewport screenshots are retained. These journeys are pending execution and do not replace the actual native 400% engine-zoom check.

The helper samples actual computed foreground/solid background layers for the focused search, preference language, Title Resume, discard action and PIN field. It alpha-composites supported RGB layers and compares the unrounded text contrast ratio to 4.5:1. It rejects gradients, group opacity or unsupported colors instead of inventing a result. These representative control samples do not qualify every image-backed text, icon, boundary, state or screen, and do not establish full conformance. Root source compilation of the changed layout/notice/preference files passed without Svelte warnings; source parsing does not prove rendered focus or contrast.

All three short-height journeys passed in the 13-case integrated release/reflow rerun, alongside all three short-height player cases, four explicit-release recovery cases, two cross-tab cases and PIN expiry. The first run exposed two omitted test fixtures and real menu/Cancel/Back gaps, all corrected before the rerun. Log: `.cache/tonight-implementation/release-reflow-browser-rerun.log`; screenshots: `.cache/tonight-web/playwright/release-reflow-rerun-results`.

The earlier caption/auth/popover/logout checkpoint passed its source check, 367 units and 107 browser cases. Localization integration subsequently passed **zero source errors/warnings, 372/372 units in 33 files and 115/115 browser cases in 4.5 minutes**, with actual-caption opt-in enabled. Logs: `.cache/tonight-implementation/tonight-locale-combined-check.log`, `tonight-locale-combined-unit.log`, and `tonight-locale-full-browser.log`; results: `.cache/tonight-web/playwright/tonight-locale-full-results`. The eight added French/Arabic journeys verify owning-element fallback language and real-ended/manual-next behavior; preview locales remain behind their existing review gate. These results prove client fixtures and actual encoded media, not database orchestration. The full browser memory log is `.cache/testing-memory/2026-10-07T21-46-01-640Z-ec11cc9f-fc8a-4e4b-b335-d98add038efb.jsonl`; its 1.786 GiB minimum host available memory is recorded in [resource evidence](TESTING_MEMORY.md).

The subsequent actual native 400% run found the Search field's left corners covered by its absolute submit button. Search now uses adjacent flexible input and 44-pixel button controls. **24/24 targeted accessibility/browsing/localization cases**, the current **zero-error/zero-warning source check** and ordinary **web build** pass after that CSS correction. Logs: `.cache/tonight-implementation/tonight-search-focus-browser.log`, `tonight-search-focus-check.log`, and `tonight-search-focus-web-build.log`; results: `.cache/tonight-web/playwright/search-focus-results`. No post-correction 115-case rerun is claimed. The corrected static build was held before launch at 11.60 GiB commit headroom; actual corrected native geometry, all remaining 400% journeys and the non-clipped native capture path remain separate [desktop qualification](TONIGHT_DESKTOP_TESTS.md) work.

Later lightweight source repairs localize reachable new Tonight navigation/header Search/preferences and retain the full Title query across preview-profile switching. Two changed Svelte components compile with zero warnings at a 128 MiB heap; expanded assertions remain in the existing six preview cases. Their runtime execution and complete source check/build remain pending; the 372/115 and post-Search 24-case/check/build evidence above predates these latest source bytes. The inspected post-Search Home/discard captures at 320×180 are ordinary web rendering evidence, not actual native zoom proof.

Current hosted source/unit/build checkpoint: [Shared Web Checks at `04fea55`](https://github.com/cloudbyday90/Duskcue/actions/runs/37706377561/job/113081617739) passes with zero Svelte-check errors/warnings, ten Node cases and 373 Vitest cases in 33 files, plus the ordinary production web build. This qualifies the current shared source after navigation/timing repairs. Full browser execution, genuine progressive decoding and fresh managed-caption rendering are still pending the dependent job; earlier browser checkpoint results are not relabeled as current.

The newer [shared job at `a37eec8`](https://github.com/cloudbyday90/Duskcue/actions/runs/37722919900/job/113134481477) supersedes shared-source currency: zero errors/warnings, 26 thin Node and 375 Vitest cases (401 total), and the ordinary production build pass. The prepared complete browser suite now contains 119 cases, including three progressive-HLS cases and one headed real-tab background case beyond the earlier 115. Hosted Linux uses Xvfb for the headed case; remaining tests retain their existing modes and serial worker count. The results gate requires all caption/progressive/background/locale scopes to execute and pass, reporting retries explicitly. Current complete browser runtime remains pending the failed managed-caption producer; the native client pass is separate [desktop evidence](TONIGHT_DESKTOP_TESTS.md).
