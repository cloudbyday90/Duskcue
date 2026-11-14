# SvelteKit 2 security patch qualification

## Scope and current status

November 14, 2026 continuation from `c70b29a` on main. The selected maintenance item is the compatible `@sveltejs/kit` 2.70.2 → 2.70.4 and `@sveltejs/adapter-static` 3.0.10 → 3.0.11 pair. Raise their manifest minimums and exact lockfile nodes, retaining every other version, dependency range, override, configuration and platform contract. No framework major migration, Android change, production capability rollout or release is included.

Source candidate: `67be9361d67dff3bbe9760e6a3fc60e89cbf3a31` on main. Documentation-only successors preserve this source attribution. The candidate is uninstalled and behaviorally unqualified.

The operator requested Astra evaluation. Astra and a fresh independent read-only boundary investigator reviewed the application, upstream patches and compatibility. No product/API decision is reopened by these routine dependency patches. Current host commit pressure prevents the guarded clean-install prerequisite. Source preparation may continue, but security verification is **blocked** until the ordered checks below pass; installing a patched version alone is not a verified application fix.

## Official research

Primary sources were identified/read through MCP on November 14. Bounded read-only HTTPS to the registry provides the published version manifests where MCP cannot open registry metadata. No installation, resolver, lifecycle script or package code runs during that lookup.

| Source | Consequence for this patch |
| --- | --- |
| [Kit 2.70.4 release](https://github.com/sveltejs/kit/releases/tag/@sveltejs%2Fkit@2.70.4), [upstream backport](https://github.com/sveltejs/kit/pull/17620) | The November 5 backport also covers cache-header escaping, Windows preview containment, route-origin confinement and self-fetch redirects. Verification must consider more than the two moderate alert titles. |
| [Malformed reroute advisory](https://github.com/sveltejs/kit/security/advisories/GHSA-mw6w-2fmv-pvw3) | Kit 2.70.4 repairs the original-path decode/error boundary; 3.1.0 is also affected. |
| [Static route-pattern advisory](https://github.com/sveltejs/kit/security/advisories/GHSA-jf9x-2fg2-hx4w) | Adapter-static 3.0.11 escapes generated Vercel route patterns; 4.0.0 is also affected. |
| [Cache-header XSS advisory](https://github.com/sveltejs/kit/security/advisories/GHSA-27mw-82g8-phjj) | Its current version table recognizes 2.70.4, while prose still says 3.0.0. Tagged 2.70.4 code confirms attribute escaping; provider alert metadata still reports 3.0.0. Preserve that disagreement rather than claiming alert closure. |
| [npm install](https://docs.npmjs.com/cli/install/), [npm ci](https://docs.npmjs.com/cli/commands/npm-ci/) | Use a locked clean install with lifecycle scripts disabled and explicit required build commands. Do not use a force audit update or unconstrained dependency refresh. |
| [W3C focus order](https://www.w3.org/WAI/WCAG22/Understanding/focus-order.html), [status messages](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html?trk=article-ssr-frontend-pulse_little-text-block) | Preserve localized navigation, meaningful focus and accessible status during browser regression. This package patch adds no UI and makes no new WCAG conformance claim. |

## Boundary and application evidence

### Malformed pathname

The public Node deployment uses adapter-node by default in `clients/web/svelte.config.js`. `clients/web/src/hooks.js` supplies Paraglide's `deLocalizeUrl` as a real reroute. In installed Kit 2.70.2, a reroute that repairs malformed input can reach an original-path decode outside the protected error boundary. The shared framework boundary is therefore the appropriate patch location; adding a route-specific filter or changing the application's locale hook would not be equivalent.

The current generated Paraglide default implementation removes a recognized literal locale prefix and retains the remaining percent-encoded pathname. No malformed-to-valid transformation satisfying the advisory's exact prerequisite has been established in this application. `hooks.server.js` runs later and cannot prove protection before reroute decoding. Package presence and a real reroute do not alone establish an exploitable deployment. The update protects the framework boundary without changing locale or backend-proxy behavior.

### Generated route patterns and other backports

The static desktop build always passes explicit adapter options, including its fallback and output paths. Installed adapter-static invokes Vercel zero-configuration output only when options are absent. No tracked Vercel deployment configuration or route prerender export is found. Consequently the affected deployed Vercel rule-generation path is not established in Duskcue. Still patch the shared package rather than weakening validation or inventing app-side regex filtering.

No application enhanced form action, `$app/forms`, `setHeaders` or explicit route prerender is found in the inspected source. This bounds evidence for the low advisories; it is not a universal non-exposure claim. Tagged Kit 2.70.4 escapes the prerender cache attribute, unlike installed 2.70.2. The enhanced-form advisory's 3.0.0 requirement and the unpatched `deepmerge` report remain separate review items; do not dismiss either from this inventory.

## Astra recommendations and alternatives

| Choice | Pros | Cons | Recommendation |
| --- | --- | --- | --- |
| Compatible patch pair | Repairs upstream boundaries without a configuration/runtime migration; consumer metadata is unchanged | Needs real installed-package and application checks; does not resolve every reported alert | Adopt as the bounded candidate |
| PR #6 as written | Existing automated patch | Known `config_file_unsupported` failure; proposed 3.1.0/4.0.0 are themselves affected | Do not adopt or merge |
| Coordinated patched major migration | Can address changed major APIs and remaining framework concerns together | Requires separate runtime/configuration/cookie and web/Tauri qualification | Later reviewed increment |
| App-specific input filtering or unsupported overrides | Small apparent source change | Does not repair every shared framework path; can reject legitimate locale/proxy behavior | Reject |

Final stack: existing Node 24.18.1/npm 12.0.2, Svelte 5, Vite 8 and TypeScript 6; Kit 2.70.4; adapter-node 5.5.7; adapter-static 3.0.11 for the existing static Tauri build; existing cookie override and remaining locked dependencies. Android and Rust/PostgreSQL stay unchanged. The same upstream release also provides adapter-node 5.5.8 for immutable SSE response-header handling; assess that separate boundary before a later adapter update. This pair is not described as updating the whole framework ecosystem.

## Exact source preparation

The two published version manifests are read from the existing lockfile's verified `https://registry.npmjs.org` origin. Reads refuse redirects, require matching package/version identity and JSON, impose a ten-second deadline and 256-KiB body ceiling, and require same-origin distribution URLs with SHA-512 integrity. Compare dependencies, optional/peer dependencies, optional peer metadata, engines, bin and license with the old lock nodes; all are identical. Tagged-source review independently agrees. No package code or registry-provided script is executed.

When guarded resolution cannot run, source preparation may replace only each node's `version`, `resolved` and `integrity`, plus the two existing root manifest floors. Preserve all other fields and every other lock node unchanged. This is an explicit, narrowly reviewed source edit, **not** an npm-generated or installed lockfile claim. Refuse the edit if identity or consumer metadata differs. The installed graph and previous generated builds remain old/stale until guarded `npm ci` and fresh builds complete.

## Ordered qualification

1. Inspect the exact manifest/lock diff and published metadata. Confirm only two root floors and two package nodes change; preserve all peer, override, runtime and adapter requirements. Validate JSON and unchanged-node identity.
2. Run `node scripts/testing-memory/run.mjs web-install`, then check actual installed package versions and the exact locked graph. Run guarded `web-check`, `web-unit`, `web-build` and `desktop-build` serially. Missing installation blocks meaningful updated-package tests; an old installed graph cannot validate the candidate.
3. Exercise actual built adapter-node malformed paths followed by healthy requests. Separately use a minimal isolated production fixture with a deliberate malformed-to-valid reroute so the vulnerable branch is actually exercised. Require a controlled 400 and continued service. Add alternate malformed encodings and an ordinary localized route. Existing Playwright launches Vite dev, which can reject before Kit, so its pass alone cannot prove this fix.
4. Exercise the installed static generator's literal-path patterns with metacharacters/trailing slashes and negative sibling matches. Include cache-header escaping, Windows preview path containment and route/self-fetch origin/redirect behavior relevant to the complete backport. Keep synthetic fixtures out of production routes.
5. Preserve existing browser localization/navigation, auth/session-cookie, proxy/redirect, household/profile, desktop startup and playback smoke behavior. Focus/status checks supplement source inspection; no full native Tauri or accessibility certification is inferred.
6. Use one fresh read-only candidate review, address only confirmed regressions, retain actual failed/refused evidence, and rerun relevant gates if source changes. Observe owned process/listener/file completion and apply the repository cleanup skill; keep current reusable outputs and compact evidence.

All existing memory, emergency, worker and deadline limits remain. No user app/model/service is stopped. Do not replay an identical known-incompatible major trial, suppress validation or weaken a guard to produce a pass.

## Outcome

Research and independent pre-patch review are complete. The candidate changes `clients/web/package.json` and `clients/web/package-lock.json`: two root minimums and exactly two nodes' version/distribution/integrity fields, preserving 188 other package nodes, all consumer metadata and the override. JSON/graph comparison against `c70b29a` and fresh published-registry comparison pass. Actual installed packages still report Kit 2.70.2 and static 3.0.10. No application runtime code, fixture route or new regression test is introduced; the existing checks and the required focused production fixtures remain to run after installation.

| Gate | Actual outcome |
| --- | --- |
| Exact source/registry identity and compatibility | Pass for both version manifests and lockfile diff; no resolver or package execution is inferred. |
| Baseline clean installation | Preflight refused before execution: `2026-11-14T23-04-20-061Z-bcc7fa5f-1159-4997-8eb6-e08dc0da6ae2.jsonl`, 71.48/73.95 GiB commit despite 19.58 GiB available physical RAM. |
| Candidate clean installation | Preflight refused before execution: `2026-11-14T23-08-53-342Z-062cbe2a-6971-4a78-bd4a-b9bfb10799ea.jsonl`, **93.354%** commit, **73.88/79.14 GiB**, about **5.26 GiB** headroom and **14.93 GiB** available physical RAM. The build policy requires twelve GiB and at most 85%. |
| Original/alternate security triggers and legitimate controls | Not executed against updated packages: the clean-install prerequisite is blocked. The original issue has **not** been shown to stop reproducing locally. |
| Analysis, units, Node/static builds and browser/native flows | Not run against the candidate. Running them on the old installed graph would not establish the update or preserved legitimate behavior. |

The two refusal receipts remain private under the existing guarded workflow. No resource policy, unrelated process or OS setting changes. Commit/push records an uninstalled source candidate, **not** a fixed finding, clean build or completed security qualification. Final candidate review, narrow text checks and provider-status evidence are recorded separately below; none can replace the blocked behavioral gates.

One fresh read-only candidate review finds no concrete surviving bypass, invalid lock metadata or application compatibility regression. It independently retrieves both exact tarballs into memory, verifies SHA-512/package identity and inspects the actual repaired boundaries: protected original/rerouted decoding, all three generated static pattern sources, cache escaping, preview containment and route/self-fetch confinement. This is source/integrity evidence, not package execution or verification of legitimate workflows. The target-bound private Codex Security `report_validation.md` records the blocked outcome and compact maintenance facts; it does not start or represent a full repository scan.

Narrow closing checks pass: staged JSON/semantic graph validation, registry and independent tarball identity/integrity, whitespace, copyright compliance over 1,070 files, ten added local Markdown links/anchors and private-LAN/cache exclusion. No updated-package test is reported as passing. Current clean-install, original/alternate trigger, legitimate-control, analysis/build/browser and provider-closure gaps remain explicit.

The [PR maintenance record](../ci/SVELTEKIT_3_UPGRADE_REVIEW.md) owns the exact open-PR head/rebase outcome. [Android ambient qualification](../ci/ANDROID_AMBIENT_QUALIFICATION.md#november-14-astra-corrections--unqualified) remains a separate unfinished gate. No release or production enablement.
