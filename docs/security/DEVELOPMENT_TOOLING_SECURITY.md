# Development tooling security

## October 24, 2026 dependency maintenance

This document owns the development-tool dependency decision and its qualification. [IPTV.md](../design/IPTV.md) owns the subsequent IPTV transport milestone. This is a targeted response to six supplied Dependabot alerts, not a repository security audit or a release.

The authenticated GitHub REST read returned six open general alerts and zero malware alerts. The GitHub connector had no Dependabot reader. With the user's permission, the request process used the saved `cloudbyday90` login instead of the invalid environment token. No credential was printed, persisted or changed. [GitHub CLI documents environment-token precedence](https://cli.github.com/manual/gh_help_environment); [GitHub REST documents alert classification, state and pagination](https://docs.github.com/en/rest/dependabot/alerts).

## Static triage

Compared with local revision `848f5157eb222121ccd669f915acb9b74187722c`, preserving provider input order:

| Alert | Manifest | Advisory | Verdict | Review priority |
|---|---|---|---|---|
| 31 | Web lockfile | GHSA-r825-w8mm-5pxw, WSL editor flags | Needs review | 5 |
| 30 | Desktop lockfile | GHSA-r825-w8mm-5pxw, WSL editor flags | Needs review | 6 |
| 29 | Web lockfile | GHSA-587h-7wmj-h5pm, Windows existence check | Needs review | 1 |
| 28 | Desktop lockfile | GHSA-587h-7wmj-h5pm, Windows existence check | Needs review | 2 |
| 27 | Web lockfile | GHSA-698v-4r3x-3vcv, Windows percent expansion | Needs review | 3 |
| 26 | Desktop lockfile | GHSA-698v-4r3x-3vcv, Windows percent expansion | Needs review | 4 |

Both root manifests declare Vite as a development dependency; both lockfiles and installed copies resolve 8.1.3. Dependabot labels the web copy runtime, which does not by itself establish production exposure. Both Vite distributions bundle `launch-editor` and `launch-editor-middleware` 2.14.1.

The development route `/__open-in-editor` reads the `file` query, resolves a path, removes line/column suffixes and checks existence. On Windows, the launcher constructs a shell command with incomplete escaping; on WSL it converts an absolute filename back to a relative one. Host validation and literal-fragment rejection do not establish authorization for this route. Windows is a supported development host. Exact attacker access, editor choice, WSL use, environment control and workspace-write prerequisites are unproven, so static triage does not claim confirmed workstation exploitation.

The web development command runs Vite; desktop development delegates to that same web command. The separate desktop Vite dependency must also be updated. Docker copies the adapter-node build and runs its server, while Tauri uses generated static assets. Neither production entry point runs this editor middleware. No product-runtime exploit path was identified in this targeted inspection.

No root or manifest-scoped `SECURITY.md` was resolved. The standing instructions designate [product security](SECURITY.md), [API security](API_SECURITY.md), and [vulnerability disclosure](VULNERABILITY_DISCLOSURE.md); those documents informed this assessment. Development-only scope is not grounds to dismiss an affected tool.

## Official research and alternatives

Sources were discovered through the web-search MCP service and opened October 24, 2026. Package registry metadata independently identifies the selected release and integrity hash.

| Option | Benefit | Cost / decision |
|---|---|---|
| Update both Vite copies to 8.1.6 | Upstream's compatible patched release for the existing 8.1 line; replaces the vulnerable bundled launcher | Requires lockfile, build and development-workflow qualification. **Selected.** |
| Update to latest 8.3.x | Includes newer features and fixes | Broadens dependency and compatibility changes beyond these alerts. Defer to separate maintenance. |
| Override only `launch-editor` | Small-looking dependency override | Does not replace the code already bundled inside Vite. Incomplete remediation. |
| Disable editor middleware locally | Removes this convenience endpoint if enforced consistently | Creates application-specific maintenance and leaves vulnerable distribution copies. Prefer the upstream patch. |
| Dismiss because dependencies are development-only | No code churn | Leaves developer workstation risk and misstates the evidence. Rejected. |

The maintainer advisories identify the [Windows existence-check bypass](https://github.com/vitejs/launch-editor/security/advisories/GHSA-587h-7wmj-h5pm), [percent expansion](https://github.com/vitejs/launch-editor/security/advisories/GHSA-698v-4r3x-3vcv), and [editor-flag interpretation](https://github.com/vitejs/launch-editor/security/advisories/GHSA-r825-w8mm-5pxw). The first can remove a filesystem prerequisite for the Windows command-injection chain. All list Vite 8.1.6 as patched on this release line. [Vite's immutable 8.1.6 release](https://github.com/vitejs/vite/releases/tag/v8.1.6) is the selected upstream artifact; this repository creates no release.

The recommendation stack is the patched Vite distribution in both clients, committed integrity-bound lockfiles, lifecycle-disabled clean dependency installation, controlled bundled-code regression checks, and existing guarded type/unit/browser/build checks. Keep development servers on trusted local interfaces and retain the production adapter-node/static deployment paths.

The intentional upstream compatibility restriction is that Windows editor opening refuses every literal `%` in a filename or parent directory, including legal names such as `100%`. File URLs decode to the same refusal. Ordinary SvelteKit `+page` and `[id]` filenames and editor positions remain supported. Preserve the guard.

## Qualification contract

Focused checks must execute the launcher code bundled in each installed Vite copy with inert filesystem/process substitutes. They must cover Windows path normalization after position parsing, percent-bearing inputs and WSL flag-like filenames, plus ordinary filenames and line/column controls. No real editor or attacker command may execute. A failure on the affected baseline followed by a pass on the updated distribution supplies stronger evidence than a version-only assertion. These checks do not emulate the Windows shell or a full WSL editor.

Run installations and tests serially through [the Windows resource workflow](../ci/TESTING_MEMORY.md). Preserve the existing budgets and unrelated applications. Web type/unit checks, representative browser accessibility/playback/profile journeys, and both adapter-node and desktop static builds qualify compatibility. Native Rust code is unchanged; the prior [Rust SDK qualification](../operations/RUST_TOOLCHAIN.md) remains separately recorded.

[W3C's WCAG 2.2 reference](https://www.w3.org/WAI/WCAG22/quickref/) remains the UI baseline: keyboard access, visible/unobscured focus, reflow and programmatically available status messages. The dependency patch adds no UI contract. Existing accessibility journeys are regression evidence, not a claim of complete WCAG conformance or IPTV accessibility qualification.

## Random PR assessment

GitHub MCP again returned only [open PR #5](https://github.com/cloudbyday90/Duskcue/pull/5), head `c018d77b7f09543e5960a571dc43e2f061cedf0a`; random selection from that one-entry set chose #5. Its requested PostCSS 8.5.23 and Nano ID 3.3.16 are older than main's 8.5.30 and 3.3.20 in both locks. No local PR patch applies. Preserve the newer versions; do not merge or otherwise change the PR.

## Implementation outcome

Both manifests now require `vite: ^8.1.6`; both lockfiles and clean installed trees resolve exactly 8.1.6 with the registry integrity. Vite's required Rolldown minimum moves 1.1.4 to 1.1.5, together with its platform bindings and supporting Oxc/Wasm dependencies. All dependency changes remain in that closure. PostCSS 8.5.30 and Nano ID 3.3.20 are preserved. No Cargo dependency or runtime IPTV code changes.

The small `clients/web/tests/fixtures/vite-editor.mjs` loader executes the installed bundle's launcher helpers with controlled filesystem, OS, path and process substitutes; `tests/unit/vite-editor-security.test.mjs` owns twelve cases across both clients. The `vite-editor-qualify` preset retains ordinary unit-memory policy and a 128-MiB heap. The existing shared web CI job now installs both client trees and runs the same serial check on Node 24. A changed/unrecognized bundled layout fails instead of silently skipping checks. This fixture is not a sandbox for untrusted code.

The affected 8.1.3 baseline produced six expected security failures and six passing compatibility controls. Updated bundles pass all twelve cases, including encoded percent file URLs and WSL file URLs with positions. The original unsafe helper behavior no longer reproduces under the inert sink substitute. No real editor, shell command, Windows shell expansion or WSL editor was executed. Normal Svelte paths, file URLs, editor line/column arguments and ordinary Linux paths retain their expected behavior; missing files and Windows UNC paths still refuse launch.

Independent read-only investigation and one independent candidate review found no concrete surviving route or compatibility blocker for these three advisory classes. Both installed bundle hashes are `9d9d6a26374aa5495b099ba77c234c8131b477aaeaafed8e1f3c9ed6fd10d4fc`. The persistent normalized `triage-finding/v0` artifact has SHA-256 `3debcca1864b1b2039eddfb1fa3f9f7b0fea27d03e53050ab93e59e7be2aa7fd`; the Codex Security target-bound collection retains the ordered six-input assessment. This does not resolve the earlier proof gaps about actual attacker access.

| Ordered check | Result and evidence |
|---|---|
| Syntax, lock identity and diff | Node syntax checks, both integrity-bound lock identities, workflow YAML/install ordering and `git diff --check` pass. |
| Controlled affected baseline | Six expected security failures, six passing compatibility controls on Vite 8.1.3. Memory evidence: `.cache/testing-memory/2026-10-24T14-28-23-731Z-307dbce5-dec9-4d54-bb3c-8567c7184c50.jsonl`. No real process sink executes. |
| Clean installations | Guarded `web-install` and `desktop-install` pass with lifecycle scripts disabled. |
| Patched bundle behavior | `node scripts/testing-memory/run.mjs vite-editor-qualify`: all twelve cases pass, including the added parser-transition cases. Final evidence: `.cache/testing-memory/2026-10-24T14-39-49-119Z-81993e24-d3e5-40ff-97aa-99cdb39419f3.jsonl`. |
| Types and units | Guarded `web-check`: zero errors/warnings. Guarded `web-unit`: 37 Node cases plus 496 Vitest cases pass. Evidence: `2026-10-24T14-30-15-959Z-01ebec0b-894d-4e6e-8a58-3378d29f8907.jsonl` and `2026-10-24T14-31-18-408Z-1cd65580-38be-47c9-ae18-86d15e64a43e.jsonl` under `.cache/testing-memory/`. |
| Initial full browser run | 126 pass, three existing caption cases skipped, one failure loading SvelteKit's generated `app.js` before playback. The retained first error-context artifact has SHA-256 `cf18a2c2429cd125f3cd9113359b822d28a9fd4b5932167eafee559e6229fc1e`. Memory evidence: `2026-10-24T14-31-44-485Z-930a6b08-cac2-4525-ab85-f4a788492af4.jsonl`. |
| Traced focused repeat | The same native-fullscreen-recovery case passes; observed generated-module HTTP status is 200. Memory evidence: `2026-10-24T14-38-19-302Z-87bdc94c-3cce-4a01-9ed9-7c0ef1493586.jsonl`. |
| Complete browser repeat | Guarded `web-e2e`: 127 pass, three existing caption skips, zero failures in 5.4 minutes. Evidence: `.cache/testing-memory/2026-10-24T14-40-02-219Z-1314cc81-9624-42a0-805b-f96a24aa66aa.jsonl`. No test assertion, timeout or retry policy changed. The initial module-fetch failure was not reproduced; its root cause remains unproven. |
| Production builds | Guarded `web-build` passes with adapter-node; guarded `desktop-build` passes with adapter-static. Evidence: `.cache/testing-memory/2026-10-24T14-45-55-053Z-a0d31547-f9a6-44fc-90f6-12824e4835b5.jsonl` and `2026-10-24T14-46-55-826Z-f8447d85-2160-4ffb-b906-f0431e6988e1.jsonl`. Both report plugin-timing information; no build failure. |
| Legal headers and formatting | The user explicitly approved only the standard legal headers on the seven existing Live TV Rust files after the copyright check identified them. Those files change only by that header. Copyright now passes all 702 checked files; `cargo fmt --all --check` passes. No executable Rust change requires rebuilding the already-qualified SDK/backend. |

Every guarded workflow completed without a memory stop or interruption, including the intentionally failing baseline and first browser run. Heavy installations, tests and builds ran serially under the existing policies; no memory threshold, unrelated process or system setting changed. The local SDK remains Rust/Cargo 1.99.1 with rustup 1.29.2. Existing strict Clippy findings on unchanged backend code remain as recorded in [Rust toolchain maintenance](../operations/RUST_TOOLCHAIN.md); no new strict-Clippy pass is claimed.

Browser checks exercise real UI and media fixtures with mocked backend/native contracts. They do not establish real Tauri native execution, a live backend contract or complete WCAG conformance. Hosted CI execution is separate from the locally parsed workflow and completed local checks.

### Remaining dependency maintenance

The post-update web `npm audit --json --ignore-scripts` exits 1 with three low-severity package entries and no moderate/high/critical entries. Desktop clean installation reports zero audit vulnerabilities. The three web entries are SvelteKit and its two adapters; they represent two upstream advisories, not three independent exploits:

- [Enhanced-form open redirect](https://github.com/advisories/GHSA-pfhg-q3qv-9q6r) requires enhanced form actions and qualifying routes.
- [Prerendered cache-control XSS](https://github.com/advisories/GHSA-27mw-82g8-phjj) requires untrusted values supplied through `setHeaders` at prerender time.

Targeted source search found no `use:enhance`, `$app/forms`, `setHeaders`, prerender declaration or server action in the current web source. The backend proxy returns responses directly; it does not feed remote cache policy into prerendered markup. These observations narrow current applicability but are not a formal dismissal or a complete framework audit. Upstream fixes require SvelteKit 3; npm proposes adapter major upgrades too. Record a separate compatibility assessment before those upgrades or before introducing either affected feature. No `npm audit fix --force`, major migration, alert dismissal or audit suppression was applied in this patch.

The next feature item remains IPTV milestone 2: supervised Dispatcharr HTTP MPEG-TS fetching into restricted FFmpeg input, bounded rolling output, readiness and confirmed cleanup. The owner/delegation, guide import, Kids enforcement and shared admission milestones retain their order. Native Tauri execution and a real Dispatcharr stream are not qualified by these dependency checks.
