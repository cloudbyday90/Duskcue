# SvelteKit security boundary runner

## Scope

November 2026 continuation of the [compatible patch qualification](../security/SVELTEKIT_2_SECURITY_PATCH.md). Astra's renewed read-only review recommends real installed-package and production-adapter evidence with independent orchestration tests. Routine test implementation needs no new product/API decision. Clean installation is still the first package prerequisite; an old installed graph must refuse before package imports, fixture writes, listeners or compilation.

Implement bounded synthetic cases: actual built adapter-node malformed-to-valid reroute handling and healthy/localized controls; installed static-generator literal patterns; prerendered cache attributes and browser execution; server-resolution client origin confinement; Windows page/data preview containment; all five server-fetch fallbacks plus an external-follow control; and the [Node adapter immutable-SSE regression](../security/SVELTEKIT_NODE_SECURITY_PATCH.md). Keep synthetic routes entirely inside an owned ignored fixture, with no application route, installer, symlink or network fallback. The selected installed stack is now Kit 2.70.4/static 3.0.11/Node 5.5.8.

The synthetic driver always retains `completeSecurityMatrix: false`: it does not aggregate application, native-platform or provider gates. Current executed outcomes are recorded below; no final security result can be inferred from its exit code or passing orchestration tests alone.

## November executed qualification

Verified source: `6835c88aeb44adb937db92ef3f64733465d05c37` on main. Documentation-only attribution preserves the exact tested implementation and installed lock identity.

Source `ddbd7ac` had 46 passing orchestration tests but no actual framework execution. Initial install admission again refuses, then the operator frees host memory and the unchanged guard admits a clean install. Exact installed metadata confirms the selected Kit/static graph. Analysis passes with zero diagnostics and web units pass 37 Node plus 582 Vitest cases on the preceding Node 5.5.7 graph.

Fresh npm audit identifies the additional moderate Node adapter advisory. Astra independently confirms its 5.5.8 backport and the precise immutable-response regression; the [Node patch authority](../security/SVELTEKIT_NODE_SECURITY_PATCH.md) owns research, tradeoffs and scope. A generated-only baseline SSE request crashes the actual 5.5.7 server with `TypeError: immutable`/ECONNRESET. After the narrow adapter update and clean installation, that case and controls pass. The first browser run exposes a harness-only execution-context race during safe same-origin full navigation. Starting the exact URL wait before scheduling real `goto` fixes the observer without relaxing the foreign-request or positive-resolution controls; Astra reviews the correction.

| Gate / private memory receipt | Actual outcome |
| --- | --- |
| Initial admitted install, Node 5.5.7; `2026-11-15T01-04-04-527Z-8de781e0-5ecc-4e5b-88b4-59cc206fc4d0.jsonl` | Pass; exact metadata and unchanged manifest/lock identity. |
| Baseline production fixture; `2026-11-15T01-08-10-865Z-de475f94-0bab-495b-92d5-ba93295d5a38.jsonl` | Build succeeds; actual SSE trigger crashes affected 5.5.7. Owned processes/files settle and are cleaned. This is expected baseline failure, not a passing matrix. |
| Final-graph clean install; `2026-11-15T01-09-06-644Z-7b630bc3-e11c-4acf-9578-0a8a77e20533.jsonl` | Pass; Kit 2.70.4/static 3.0.11/Node 5.5.8. One additional root floor/lock distribution changes; 189 unrelated nodes preserved. |
| First patched matrix; `2026-11-15T01-09-48-211Z-cd854663-9c5c-4b3d-9db8-f243962698ed.jsonl` | SSE/other earlier cases pass; full run fails on the browser observer's destroyed evaluation context. Cleanup completes. |
| Corrected Windows matrix; `2026-11-15T01-10-48-154Z-ce8ff51b-1fd6-460f-b432-158f68469afa.jsonl` | **All thirteen case identities pass**, actual Windows preview/browser/fetch execution, zero foreign requests and confirmed process/socket/fixture cleanup. |
| Final-graph `web-check`; `2026-11-15T01-13-12-891Z-9a704228-7aec-4ae0-a474-eba38e2c8ee3.jsonl` | **Zero errors, zero warnings.** |
| Final-graph `web-unit`; `2026-11-15T01-13-59-934Z-4cc7c16c-a7ff-4408-aa4e-f58fbec4ea4d.jsonl` | **37 Node and 582 Vitest tests pass**, 44 Vitest files. |
| Node application build; `2026-11-15T01-14-25-399Z-5d3f5864-23f6-47e3-b24f-43c813a28c1e.jsonl` | Pass. |
| Initial application smoke; `2026-11-15T01-15-11-834Z-4cfe9523-8bd5-4447-9a02-be3e5ccb4711.jsonl` | Actual Node SSR shell, entry files and real proxy-hook JSON/cookie/query/redirect/SSE checks pass. This earlier version did not tie entry filenames to returned HTML. |
| Static desktop build; `2026-11-15T01-15-26-986Z-dc9da369-08af-4f1c-811e-15238c5f530e.jsonl` | Pass after consuming Node output; static fallback and actual entry files verified. |
| Application browser suite; `2026-11-15T01-16-06-450Z-8c45fc1d-2d2d-4136-80b8-b4b18883743e.jsonl` | **165 pass, three unchanged burned-caption skips**, 6.2 minutes. Those skips require a separately supplied qualified caption-HLS artifact. |
| Fresh Node rebuild; `2026-11-15T01-22-50-110Z-38751710-6b70-485f-b1e7-e5e385527f24.jsonl` | Pass; required because the static build replaced shared client assets. |
| Strengthened application smoke; `2026-11-15T01-23-15-369Z-ad797e4a-cf19-4be7-ac49-ead10fa18ead.jsonl` | **Pass**, actual SSR shell paired to both referenced app/start assets; JSON/query/cookie/redirect/SSE hook controls and cleanup confirmed. |
| Final orchestration units; `2026-11-15T01-23-29-532Z-c35f9033-2528-4cf6-8390-b2972f4feb69.jsonl` | **48 pass**, zero skipped/failed/cancelled. Includes SSE, exact SSR-asset pairing and application-lane policy checks. |

The final lock SHA-256 is `64c1c98109286fc0064ff010042c95f88e7b79d7cbba35e87a8baa19fa7f2a80`. Final npm audit has zero moderate/high/critical reports and six low affected-package reports spanning three advisory identities; cache-header metadata still disagrees with its tested backport. No alert is dismissed and no force update occurs.

Astra correctly distinguishes the actual application's SSR/loading shell from client-rendered login/hydration. The strengthened smoke result uses `ssrShell` and fetches the exact app/start import paths referenced by its returned HTML, requiring the same owned origin. Its final rebuild/repeat and browser-suite outcomes pass as recorded above. Existing browser tests use Vite dev, mocked API/device boundaries and real browser/media paths; they are not a fresh native Tauri, real authenticated Rust-backend or Android ambient qualification. W3C keyboard/focus/reflow/status regressions remain scoped checks, not a complete accessibility certification.

All admitted workflows finish without a memory stop or interruption under unchanged policies. The Windows synthetic matrix peaks at **1,222,774,784 bytes** observed owned private allocation; the browser suite peaks at **3,275,694,080 bytes**. These are sampled workflow allocations, not whole-host commit attribution. No user model/service is stopped by the agent. [Maintenance](../operations/DEVELOPMENT_STORAGE.md#qualified-web-security-maintenance) confirms the active marker, owned runner processes, fixture runs and test listener are gone; current outputs and compact evidence remain. The final shared application output is Node, because the stronger smoke required a rebuild after static verification. Rebuild static before a later native desktop consumer needs that artifact.

The scoped web qualification is complete. Keep the three low advisory identities, unchanged caption-fixture skips and native-platform/backend limits explicit. Next resume the confirmed Android ambient compile/analysis/test/native gates under their existing contract; no production attachment, shared cap, provider/DVR or release is enabled here.

Closing source checks pass: syntax for twenty runner/test modules and both preset files; copyright compliance over 1,090 files; 23 added local Markdown links/anchors; exact published lock delta and installed graph; staged whitespace and private-LAN/cache exclusion. Application source is unchanged apart from the selected package files. These checks supplement the actual execution above.

## Research and recommendations

Official MCP research on November 14 rechecks the [upstream backport](https://github.com/sveltejs/kit/pull/17620/files), the [SvelteKit adapter distinction](https://svelte.dev/docs/kit/adapters), [Vite configuration loading](https://vite.dev/config/) and [Node child completion](https://nodejs.org/download/release/v24.21.0/docs/api/child_process.html). Use the existing Kit 2 configuration and installed versions, rather than adopting the current documentation's Kit 3 examples. The installed Vite CLI labels native loading experimental; the fixture uses plain ESM and retains real compilation as a required gate. Native loading avoids the default shared `.vite-temp` bundle. A child's `exit` or kill request does not establish closed stdio. Existing Vite-dev Playwright responses can precede Kit and cannot prove production reroute behavior.

| Choice | Pros | Cons | Decision |
| --- | --- | --- | --- |
| Small generated production fixture using installed packages | Exercises the actual adapter/framework boundary without adding app routes | Needs compilation; does not establish the app's full legitimate flows | Implement behind existing build admission |
| Unit tests with injected lifecycle operations and tiny owned HTTP/process helpers | Can verify cancellation and retained cleanup separately from compiler availability | Cannot qualify framework vulnerabilities | Implement with explicit attribution |
| Copying patched implementations or accepting any malformed-path error | Cheap to run | Can pass without executing the repaired boundary or preserving service health | Reject |
| Treating partial fixture results as complete security verification | Simple status | Hides browser/platform/backport proof gaps | Reject |

Stack: existing Node 24/npm, selected Kit 2.70.4/static 3.0.11/Node 5.5.8 and unchanged supporting graph; direct installed Vite CLI for fixture build; builtin Node HTTP/process owners; existing guarded workflow. [W3C status-message guidance](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html?trk=article-ssr-frontend-pulse_little-text-block), focus and localized-navigation acceptance remain in the existing browser matrix; no application UI or accessibility certification is introduced.

## Ownership and budgets

Use focused prerequisite, lifecycle, process, HTTP, fixture and case modules plus a thin driver. The actual runner uses the unchanged build policy and web-build 3,072-MiB heap ceiling; pure orchestration tests use unit/128 MiB, one test worker. Neither lane can install dependencies or lower admission thresholds.

Allow one fixture, one build/server/static-generator child and one probe request at a time. Use a ten-minute execution observer, three minutes per build, fifteen seconds for startup, five seconds and 64 KiB per HTTP response, sixteen MiB combined child output and a 64-KiB diagnostic tail. Use literal loopback and OS-assigned ports with exact child/PID/nonce readiness binding. Refuse redirects for probe requests. Abort outstanding owned requests and observe request/socket closure.

Register child ownership before startup, retain it until `close`, and use the existing Windows creation-time/executable revalidation/tree-stop helper. Invoke Node/Vite directly with the native configuration loader, without npm/shell intermediaries or shared configuration-bundle output. Process/HTTP cleanup observers have ten seconds; timeout/failure remains a failure and preserves the generated directory/ownership evidence. Filesystem completion is awaited. Ownership may outlive an observer deadline; no hard native-completion deadline or successful disposal is inferred. A late completion may permit cleanup, but cannot erase the failed attempt.

Generate below `clients/web/.cache` so package resolution uses the existing web installation naturally. Resolve and verify each generated root and refuse reparse-point traversal before cleanup. Do not delete through a junction or computed path outside that exact root. Retain compact evidence while removing only verified disposable generated files after confirmed cleanup.

## Remaining synthetic matrix — November continuation

Astra's renewed official-source review recommends completing the synthetic matrix while build admission is unavailable. Add separate preview, browser-owner/browser-case, self-fetch and evidence-validator modules, retaining the current prerequisite and all resource policies. These are routine qualification choices under the existing contract, not new application features.

Use actual Vite preview for Windows page/data traversal with nonce-bearing outside-root sentinels and in-root positive controls. Use the installed Playwright BrowserServer, one browser/context/page, with retained launch/connect/close and actual child completion. Browser cases must show ordinary inline script execution, real hydrated server-resolution navigation, zero foreign requests and no injected marker. Block foreign traffic locally. Missing Chromium fails without download. Extend installed metadata checks to the already locked Playwright packages.

Generate a server-only fixture helper importing the exact installed Kit fetch module through a computed relative source path, letting the existing build resolve its generated aliases. Invoke the actual five fallback branches with a real owned loopback redirect sink and a positive external-follow control. Do not copy the repaired implementation, replace global fetch or add an application endpoint. Require exact per-case identities, status/body/location/request sequences and observed sink/socket cleanup.

Keep one Node command and one browser at most, with no overlapping build, under unchanged build admission, heap, output and execution budgets. Browser shutdown failures retain their cleanup barrier and prevent directory deletion/result publication. The matrix ledger must distinguish actual Windows execution from a non-Windows limitation. Even a complete synthetic ledger leaves `completeSecurityMatrix: false` until ordinary application builds/flows and the other authority gates pass.

The tradeoff is additional test-owner code in exchange for actual browser/platform/runtime boundary evidence. Prefer these isolated, locked-stack fixtures over source-copy tests, vacuous 404 checks, generic browser errors or a new major framework migration. Record current execution and remaining gaps below before claiming progress beyond source preparation.

### Official-source review and implemented stack

MCP retrieves the [tagged preview implementation](https://raw.githubusercontent.com/sveltejs/kit/@sveltejs/kit@2.70.4/packages/kit/src/exports/vite/preview/index.js), [tagged fetch tests](https://raw.githubusercontent.com/sveltejs/kit/@sveltejs/kit@2.70.4/packages/kit/src/runtime/server/fetch.spec.js), [upstream client cases](https://raw.githubusercontent.com/sveltejs/kit/@sveltejs/kit@2.70.4/packages/kit/test/apps/basics/test/client.test.js) and [Playwright BrowserServer contract](https://playwright.dev/docs/api/class-browserserver). Astra checks these against the installed locked tooling, then independently reviews the implementation. Public browser connection closure is distinct from owned process termination; the owner keeps the latter until actual child closure. A timeout remains a failed attempt even if late cleanup eventually settles.

| Recommendation | Benefit | Cost / limit |
| --- | --- | --- |
| Keep Kit 2.70.4/static 3.0.11 and existing Node adapter/Playwright/Vite versions | Avoids an unrelated major migration and exercises the selected backport | Requires a clean installation before runtime evidence is valid |
| Actual Vite preview, browser navigation and installed fetch helper in an isolated build | Positive controls establish that the repaired boundary is reached | Additional bounded fixture/owner code; application compatibility remains separate |
| Twelve exact case identities and strict evidence fields | Missing branches, skips, stale nonces and absent controls cannot become a matrix pass | Non-Windows execution remains explicitly unqualified for the Windows case |
| Retain cleanup futures; terminate the owned browser after stalled graceful cleanup | Prevents an observer timeout from abandoning a live process | Failed or unreturned public operations retain the fixture barrier |

Final stack: unchanged Node 24/npm and selected same-major packages; direct installed Vite build/preview; installed Playwright launching the exact prechecked full Chromium executable in headless mode; builtin Node HTTP/process owners; one generated fixture and explicit evidence validation. There is no browser download or new dependency. The generated server-only endpoint imports the installed fetch module; it does not copy the patch or replace global fetch. External test traffic is confined to an owned literal-loopback sink, and foreign browser requests are intercepted locally.

The existing [W3C status-message guidance](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html?trk=article-ssr-frontend-pulse_little-text-block) still informs the later application flow checks: waiting/error states must remain programmatically available without unnecessary focus changes. These synthetic security cases add no application accessibility claim.

### Current-source outcome

This is the earlier source-only checkpoint at `ddbd7ac`, superseded by the executed qualification above. All then-planned synthetic lanes have source implementation. Astra finds two concrete issues: stalled graceful browser closure could prevent native termination, and the default headless launch could use a different executable from admission. The corrections observe one retained graceful chain, independently request owned termination on failure/timeout, join actual child closure and late public completion, preserve sticky failure, and forward the exact validated executable. Browser child environment selection excludes inherited credentials. New tests cover held launch/context creation, stalled context/server closure, connection failure, actual-close ownership, executable forwarding, shared output bounds and strict evidence refusal.

| Command / private memory receipt (UTC November 15) | Actual outcome |
| --- | --- |
| `web-install`; `2026-11-15T00-33-22-921Z-784c8457-6bd2-4b7c-9049-ea151e371bfd.jsonl` | Refused before execution. Installed Kit/static remain 2.70.2/3.0.10. |
| Initial expanded units; `2026-11-15T00-49-38-003Z-a9272f4b-d6fa-4d71-9555-d06ee5ba1674.jsonl` | 42 pass/one failed: an older test omitted the fixture's new explicit web input. Corrected test input; no framework executes. |
| Expanded units; `2026-11-15T00-49-58-944Z-9c526507-41ac-4a39-b20f-920224a4832e.jsonl` | 43 pass. Predates Astra's two browser corrections. |
| Cleanup-correction units; `2026-11-15T00-51-28-095Z-2a4ae2b9-e701-4fb0-ac1c-e4054b4e068f.jsonl` | 44 pass/one failed: the earlier late-context test waited beyond the newly sticky graceful deadline, then expected success. Separate bounded late creation from the explicit stalled-cleanup cases. |
| Final `web-security-unit`; `2026-11-15T00-52-13-999Z-86fcfd11-f4fe-4ad8-b7b4-01e0911413d4.jsonl` | **46 pass**, zero failed/skipped/cancelled. Peak observed owned private allocation **220,315,648 bytes**; no memory stop or interruption. No framework or Chromium execution. |
| Final `web-security-boundaries`; `2026-11-15T00-52-28-809Z-41831c32-167c-4364-9a88-7b66b9e2f285.jsonl` | Refused before runner execution at **90.951%** commit, **76.45/84.05 GiB**, about **7.61 GiB** commit headroom despite **18.37 GiB** physical availability. Build admission still requires twelve GiB and at most 85%. |

No installed-package fixture build, actual browser/preview/fetch case, ordinary updated-graph application flow or Android ambient qualification runs in this increment. Syntax checks and the independent units qualify orchestration only. The complete synthetic ledger always retains `completeSecurityMatrix: false`; it cannot substitute for the remaining application gates. [PR maintenance](SVELTEKIT_3_UPGRADE_REVIEW.md#november-matrix-continuation) and [owned storage maintenance](../operations/DEVELOPMENT_STORAGE.md#web-matrix-source-maintenance--november-continuation) remain separately attributed.

Closing checks pass: syntax for all seventeen runner/test modules plus the preset, copyright compliance over 1,087 files, nine added local Markdown links/anchors, staged whitespace and private-LAN/cache exclusion. Application runtime files and package manifests/lockfile are unchanged by this increment. These checks do not execute the patched framework.

Next: follow the serial execution order below once unchanged build admission is available. Resume Android ambient compilation/native qualification only after the web gates pass. Main only; no release or production enablement.

## Execution gate and build-output ordering

Renewed November MCP research confirms the [npm clean-install contract](https://docs.npmjs.com/cli/commands/npm-ci/), [Playwright browser identity/connection contract](https://playwright.dev/docs/api/class-browsertype) and [Windows commit/physical-memory distinction](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-memorystatus). Frozen `npm ci --ignore-scripts` preserves the lockfile and suppresses install scripts; it also replaces `node_modules`. Do not purge the current installation before guarded admission or substitute `npm install`/an old graph for the required clean install. The current Windows sampler already uses the newer memory APIs; this research does not change it or authorize page-file/OS changes.

Astra finds no additional definite harness source blocker after the prior browser corrections. It finds one qualification-order hazard in repository configuration: both Node and desktop builds write `clients/web/build/client`. `clients/desktop/scripts/build-web-static.mjs` runs the web build with `DUSKCUE_WEB_ADAPTER=static`; `clients/web/svelte.config.js` selects static pages/assets in that same client directory. Static assets can therefore replace a Node build's client files while an earlier `build/index.js` remains. This is a source-based finding, not an observed mixed-output runtime failure.

Use this serial order:

1. Guarded `web-install`; confirm the actual installed graph and unchanged manifest/lock identities before package execution.
2. Guarded `web-check` and `web-unit` on that graph.
3. Guarded `web-build`, then immediately consume that exact output in actual built-Duskcue Node application smoke checks. Observe server/reader completion before another build writes the shared directory.
4. Guarded `desktop-build`, then its static-output checks. If Node smoke must run afterward, rebuild Node first; never combine an old server entry with later static client output.
5. Guarded `web-security-boundaries` on Windows: all twelve exact cases, positive controls and owned cleanup are required. Its generated build is isolated from application output.
6. Existing localization/navigation, authentication/session-cookie, proxy/redirect, household/profile, desktop and playback browser regressions. The existing Playwright configuration starts Vite dev; it supplements, but cannot replace, actual production-application smoke.

| Approach | Pros | Cons / qualification limit |
| --- | --- | --- |
| Consume each application's build before another adapter writes its client output | Avoids stale mixed-adapter evidence without product/configuration changes | Requires explicit serial execution; a later Node repeat needs a rebuild |
| Keep the locked stack, isolated security fixture and current guards | Preserves the selected backport and finite owned resources | Requires both synthetic boundary execution and ordinary application checks |
| Expand source-only harnesses while runtime admission is refused | Could add more source assertions | Does not resolve installation/compilation uncertainty; no additional expansion is warranted by this review |

Recommendation: retain the existing Node/npm, selected Kit/static maintenance patches, unchanged Node adapter/Vite/Playwright and resource policies; correct execution order and perform real qualification when admitted. The internal fetch import/generated aliases still need their first actual compilation. Missing Chromium, refused admission or compilation failure remains a failed/blocked gate. Preserve the existing W3C focus/status acceptance without claiming new UI/accessibility results.

The actual-application smoke driver will reuse the existing installed-identity, direct-Node process and bounded HTTP owners under unchanged build admission. Consume `clients/web/build/index.js` immediately after the Node build, with a single owned literal-loopback backend fixture and sixty-second observer. Check login HTML/locale placeholders, two actual built entry assets, malformed-path refusal and continued service, and the real application hook's finite JSON/cookie/redirect/SSE proxy behavior. No real backend credential, production fixture route, database or application source change is needed. Join owned listener/socket/process completion before publishing; no synthetic proxy fixture is an authenticated Rust/backend or full accessibility qualification.

Initial `web-install` receipt **`2026-11-15T00-58-45-791Z-ceb9f8cc-887d-4332-a055-8b68c1377d3f.jsonl`** refuses before execution at **91.180%**, **83.18/91.23 GiB** system commit, **8.05 GiB** headroom and **13.73 GiB** physical availability. The next operator-requested check, `2026-11-15T01-03-19-003Z-3c36d6e3-99bb-4762-a1e0-b78a4e15e213.jsonl`, meets the twelve-GiB reserve but still refuses at 86.366%. A subsequent fresh sample drops below 85% and the guard admits the successful installs/qualification above. No reserve, percentage, OS or service setting is changed. [Maintenance](../operations/DEVELOPMENT_STORAGE.md#web-execution-gate-maintenance) records targeted resource inventory and preservation of running services/models.

## Status and next gate

Source checkpoint: `6bf5cb038e340493d2874b8039fc73b17c615754` on main. Documentation-only successors preserve this unit-tested infrastructure attribution; the actual framework fixture remains uncompiled and unqualified.

Implemented focused prerequisite, lifecycle, process, HTTP, fixture, pattern, worker and case modules with a thin driver. Prerequisite validation covers the exact three framework/adapter identities and locked supporting packages. Fixture imports resolve through the existing web installation; generation uses no installer or symlink. The driver rechecks input identity after cases, preserves retained cleanup, and always returns `completeSecurityMatrix: false`. A successful exit can qualify only this named subset.

Astra's implementation review found one concrete race: cancellation during held final disposal could be promoted to a partial pass after the last child closed. The lifecycle now checks cancellation after disposal settles, preserving cleanup and refusing that result. A new held-disposal regression covers the correction. Other reviewed ownership, fixture-root, adapter-export, HTTP and pattern/cache boundaries have no additional definite source blocker; this is not a compiled-framework or complete security pass.

Final source inspection also prevents a flooded output producer from allocating another cleanup observer for every discarded chunk. Only the first failure initiates the observed stop; subsequent output is discarded. An injected burst test checks the observer count while preserving actual close ownership and failed status.

| Command / private memory receipt | Actual result |
| --- | --- |
| `web-install`; `2026-11-14T23-24-03-614Z-8ccc400c-cd7f-49b8-8d24-fe59a4b39ee7.jsonl` | Refused before execution: 78.70/84.05 GiB commit despite 12.60 GiB available physical RAM. Installed graph remains old. |
| Initial `web-security-unit`; `2026-11-14T23-41-43-141Z-eeb22c82-64a8-4d29-82ba-525305f501c1.jsonl` | 28 pass, no skips. Earlier source; it did not yet cover the review-discovered disposal race. |
| `web-security-boundaries`; `2026-11-14T23-42-47-925Z-2bda1c0b-c4e8-44ad-9a46-23cd4dbb17dd.jsonl` | Build preflight refused before script execution at 89.718% commit and about 8.64 GiB headroom. No package import, fixture/build, static-generator or production-server case runs. |
| Corrected `web-security-unit`; `2026-11-14T23-46-29-852Z-02df5b8c-6699-41c1-8d56-1f4a267dc41c.jsonl` | **29 pass**, zero skipped/failed. Includes actual owned direct-Node tree termination, real loopback overflow/truncation/stall/redirect/cancellation, retained close/observer cases, held cleanup cancellation, exact version refusal, and owner/junction cleanup refusal. No patched framework package executes. |
| Final `web-security-unit`; `2026-11-14T23-54-29-348Z-275c1639-a311-4360-8437-b6d6ab1fd90e.jsonl` | **30 pass**, zero skipped/failed. Adds the bounded output-observer regression; all earlier assertions remain. No memory stop/interruption or framework-package execution. |
| Final `web-security-boundaries`; `2026-11-14T23-55-25-273Z-e0c6dd6a-66b9-486f-81ea-8454b458e5f0.jsonl` | Current-source build preflight refuses before execution at **90.829%** commit, **76.34/84.05 GiB**, about **7.71 GiB** headroom despite **17.96 GiB** physical availability. The unchanged policy requires twelve GiB and at most 85%. |

The unit lane meets the unchanged unit policy while build admission remains unavailable; it is not a downgraded compiler policy. The final repeat's peak observed test-private allocation is 214,663,168 bytes, with no guard stop or interruption. Owned helper processes/listeners and generated unit directories are gone; no production fixture, VM/database, user app/model/service, SDK/dependency version or OS/guard setting changes. Current syntax/metadata checks supplement these tests without replacing engine cases.

GitHub now reports alerts 37/36 fixed at `2026-11-14T23:16:04Z`, with low alerts 34/33/32 still open. This is provider dependency-graph status, not local behavioral verification. [PR maintenance](SVELTEKIT_3_UPGRADE_REVIEW.md#november-14-boundary-runner-continuation) records conflict resolution and the existing unmerged PR head. [Storage maintenance](../operations/DEVELOPMENT_STORAGE.md#web-boundary-runner-maintenance--november-14) owns the disposal record.

Closing source checks pass: syntax for thirteen changed modules, copyright compliance over 1,081 files, eleven added local Markdown links/anchors, staged whitespace and private-LAN/cache exclusion. The web package manifests/lockfile and all application runtime files are unchanged in this runner increment. These checks add no engine, application, native Tauri or complete accessibility result.

At that checkpoint the remaining source lanes were still to be implemented. The current continuation above supplies those lanes, with actual framework execution and ordinary application gates still pending. Until those pass the dependency fix and fixture compilation remain locally unqualified. Work stays on main with no release.
