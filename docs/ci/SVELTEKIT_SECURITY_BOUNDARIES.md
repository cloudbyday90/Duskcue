# SvelteKit security boundary runner

## Scope

November 2026 continuation of the [compatible patch qualification](../security/SVELTEKIT_2_SECURITY_PATCH.md). Astra's renewed read-only review recommends real installed-package and production-adapter evidence with independent orchestration tests. Routine test implementation needs no new product/API decision. Clean installation is still the first package prerequisite; an old installed graph must refuse before package imports, fixture writes, listeners or compilation.

Implement a bounded first runner slice: actual built adapter-node malformed-to-valid reroute handling and healthy/localized controls; actual installed static-generator patterns for redirects and both trailing-slash page rules; and the actual prerendered cache attribute. Keep synthetic routes entirely inside an owned ignored fixture, with no application route, dependency/version change, installer, symlink or network fallback.

This slice does **not** complete the security matrix. Browser execution of the cache artifact, real client server-resolution origin confinement, Windows page/data preview containment with real outside-root sentinels, the five self-fetch fallback variants, existing application checks/builds/browser flows and provider status remain mandatory. No final security result can be inferred from a partial-run exit code or passing orchestration tests.

## Research and recommendations

Official MCP research on November 14 rechecks the [upstream backport](https://github.com/sveltejs/kit/pull/17620/files), the [SvelteKit adapter distinction](https://svelte.dev/docs/kit/adapters), [Vite configuration loading](https://vite.dev/config/) and [Node child completion](https://nodejs.org/download/release/v24.21.0/docs/api/child_process.html). Use the existing Kit 2 configuration and installed versions, rather than adopting the current documentation's Kit 3 examples. The installed Vite CLI labels native loading experimental; the fixture uses plain ESM and retains real compilation as a required gate. Native loading avoids the default shared `.vite-temp` bundle. A child's `exit` or kill request does not establish closed stdio. Existing Vite-dev Playwright responses can precede Kit and cannot prove production reroute behavior.

| Choice | Pros | Cons | Decision |
| --- | --- | --- | --- |
| Small generated production fixture using installed packages | Exercises the actual adapter/framework boundary without adding app routes | Needs compilation; does not establish the app's full legitimate flows | Implement behind existing build admission |
| Unit tests with injected lifecycle operations and tiny owned HTTP/process helpers | Can verify cancellation and retained cleanup separately from compiler availability | Cannot qualify framework vulnerabilities | Implement with explicit attribution |
| Copying patched implementations or accepting any malformed-path error | Cheap to run | Can pass without executing the repaired boundary or preserving service health | Reject |
| Treating partial fixture results as complete security verification | Simple status | Hides browser/platform/backport proof gaps | Reject |

Stack: existing Node 24/npm, selected Kit 2.70.4/static 3.0.11, unchanged Node adapter and supporting graph; direct installed Vite CLI for fixture build; builtin Node HTTP/process owners; existing guarded workflow. [W3C status-message guidance](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html?trk=article-ssr-frontend-pulse_little-text-block), focus and localized-navigation acceptance remain in the existing browser matrix; no application UI or accessibility certification is introduced.

## Ownership and budgets

Use focused prerequisite, lifecycle, process, HTTP, fixture and case modules plus a thin driver. The actual runner uses the unchanged build policy and web-build 3,072-MiB heap ceiling; pure orchestration tests use unit/128 MiB, one test worker. Neither lane can install dependencies or lower admission thresholds.

Allow one fixture, one build/server/static-generator child and one probe request at a time. Use a ten-minute execution observer, three minutes per build, fifteen seconds for startup, five seconds and 64 KiB per HTTP response, sixteen MiB combined child output and a 64-KiB diagnostic tail. Use literal loopback and OS-assigned ports with exact child/PID/nonce readiness binding. Refuse redirects for probe requests. Abort outstanding owned requests and observe request/socket closure.

Register child ownership before startup, retain it until `close`, and use the existing Windows creation-time/executable revalidation/tree-stop helper. Invoke Node/Vite directly with the native configuration loader, without npm/shell intermediaries or shared configuration-bundle output. Process/HTTP cleanup observers have ten seconds; timeout/failure remains a failure and preserves the generated directory/ownership evidence. Filesystem completion is awaited. Ownership may outlive an observer deadline; no hard native-completion deadline or successful disposal is inferred. A late completion may permit cleanup, but cannot erase the failed attempt.

Generate below `clients/web/.cache` so package resolution uses the existing web installation naturally. Resolve and verify each generated root and refuse reparse-point traversal before cleanup. Do not delete through a junction or computed path outside that exact root. Retain compact evidence while removing only verified disposable generated files after confirmed cleanup.

## Status and next gate

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

Next: obtain unchanged build admission, complete clean installation and exact installed-graph validation, execute the named boundary subset, then complete the remaining browser/Windows/origin/self-fetch and application matrix above. Until then the dependency fix and new fixture compilation remain locally unqualified. Work stays on main with no release.
