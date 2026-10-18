# Playback dependency review

## Format qualification recheck — October 18, 2026

GitHub MCP returned the complete current open pool `[5]`. A fresh `crypto.randomInt` draw at `2026-10-18T18:39:52.538Z` selected index zero and [PR #5](https://github.com/cloudbyday90/Duskcue/pull/5), open and unmerged at head `c018d77b7f09543e5960a571dc43e2f061cedf0a`. Both lockfile patches were inspected. Their PostCSS 8.5.23/nanoid 3.3.16 delta is already implemented and superseded locally by 8.5.30/3.3.20. Retain those newer fixes and regression-test the current graphs; there is no new PR code delta or downgrade. Selection evidence is `.cache/local-evaluation/format-pr-selection.json`. No merge, comment, closure or release is performed.

Current audit and qualification results will be recorded in [Format qualification](../design/PLAYBACK_FORMAT_QUALIFICATION.md).

## Native worker-failure qualification recheck — October 18, 2026

The complete current open PR pool remained `[5]`. A fresh cryptographic draw at `2026-10-18T13:51:41.610Z` selected index `0`, [PR #5](https://github.com/cloudbyday90/Duskcue/pull/5), still open and unmerged at head `c018d77b7f09543e5960a571dc43e2f061cedf0a`. The complete two-lockfile patch was reviewed again. Its PostCSS 8.5.23/nanoid 3.3.16 changes remain superseded by the local PostCSS 8.5.30/nanoid 3.3.20 versions; this iteration introduces no dependency delta, downgrade or remote PR mutation. Selection evidence is retained in ignored `.cache/local-evaluation/native-failure-pr-selection.json`.

Fresh web and desktop npm audits both returned zero advisories, recorded in ignored `.cache/local-evaluation/native-failure-web-audit.json` and `.cache/local-evaluation/native-failure-desktop-audit.json`. The web client passed 476 unit tests and its source/type check. The initial and final-image paced recovery gates, eight Linux feature cases, 62 browser cases, twelve native fixture groups, no-cache Compose rebuild and restored runtime evaluation also passed. Exact execution identities, stopped attempts and remaining format boundaries belong to [Native worker-failure qualification](../design/PLAYBACK_NATIVE_FAILURE_QUALIFICATION.md). GitHub MCP confirmed that #5 remains open and unmerged after the recheck; no PR mutation was performed.

## Live-client qualification recheck — October 18, 2026

GitHub MCP returned the current complete open pool containing only [PR #5](https://github.com/cloudbyday90/Duskcue/pull/5). A fresh cryptographic draw at `2026-10-18T11:59:12.093Z` selected index `0`; metadata still reports open/unmerged head `c018d77b7f09543e5960a571dc43e2f061cedf0a`. The complete two-lockfile patch was inspected again. Its PostCSS 8.5.23/nanoid 3.3.16 delta is already locally implemented and superseded by PostCSS 8.5.30/nanoid 3.3.20. This iteration retains those stronger versions and re-tests their functional coverage; it creates no new dependency delta or remote PR mutation. Current audit and runtime results are recorded in [Live playback client qualification](../design/PLAYBACK_LIVE_CLIENT_QUALIFICATION.md).

## Design and provenance — October 17, 2026

This dependency review accompanies the [playback compatibility foundation](../design/PLAYBACK_COMPATIBILITY.md). The user requested a random open PR implemented and tested locally without merging it.

GitHub MCP pagination returned the complete open pool `[5]` and an empty second page. A cryptographically generated uniform index selected PR [#5](https://github.com/cloudbyday90/Duskcue/pull/5), head `c018d77b7f09543e5960a571dc43e2f061cedf0a`. Its two-lockfile patch upgrades PostCSS 8.5.16 to 8.5.23 and nanoid 3.3.15 to 3.3.16. The exact patch was applied, installed and tested locally. The remote PR remains open and unmerged; no review comment, closure or release was created.

The bounded playback-recovery iteration repeated the complete open-PR discovery through GitHub MCP: page one contained only PR #5, and page two was empty. A new uniform cryptographic draw over that one-entry pool selected index `0`, again PR #5 at the same head. Its functional dependency delta is already included and superseded by the patched versions below, so this iteration requires no additional PR patch and does not downgrade either lockfile. The canonical PR remains open and unmerged; this recheck made no remote mutation.

On 18 October 2026, the authenticated direct-fallback iteration searched the current open PRs through GitHub MCP and again found only #5. A fresh cryptographic draw selected index `0`; the metadata and complete two-file patch still identify head `c018d77b7f09543e5960a571dc43e2f061cedf0a`. The current lockfiles already supersede that locally applied patch. No additional dependency delta or downgrade is introduced, and no remote merge, comment or closure is performed. Both current npm graphs were audited again and returned zero advisories; the shared client tests were also rerun. Final application and image results are owned by [Direct-play fallback](../design/PLAYBACK_DIRECT_FALLBACK.md).

The selected patch addresses [PostCSS source-map handling](https://github.com/postcss/postcss/security/advisories/GHSA-fxqj-rqcc-2cmp) and the nanoid negative-size loop described in its [official changelog](https://github.com/ai/nanoid/blob/main/CHANGELOG.md). The exact PR versions did not clear every current advisory, so the final lockfiles include a separate compatible security follow-up.

## Final recommendation and tradeoffs

| Choice | Benefit | Cost or limit |
|---|---|---|
| Apply and qualify the selected PR before follow-up fixes | Preserves evidence that the requested PR was exercised locally | Final lockfiles intentionally extend beyond that historical patch |
| Update affected packages within existing manifest and peer ranges | Clears reported advisories without a major migration | Requires fresh installs, source checks and runtime bundling |
| Verify archive SHA-512 independently and initially disable install scripts | Checks that reviewed archives match registry integrity values | Integrity does not establish that package behavior is safe |

The recommended stack is the selected PR, compatible advisory fixes, fresh installation and client checks, then production-image qualification. These npm updates stay within the existing package manifests. The subsequent Rust dependency migration and its audit limits are recorded in [Playback dependency security follow-up](../security/PLAYBACK_DEPENDENCY_FOLLOWUP.md).

| Package | Final version | Official npm publication date (UTC) | Clients |
|---|---|---|---|
| SvelteKit | 2.70.2 | July 29, 2026 | Web |
| devalue | 5.9.4 | September 18, 2026 | Web |
| nanoid | 3.3.20 | October 5, 2026 | Web, desktop |
| source-map-js | 1.2.2 | September 30, 2026 | Web, desktop |
| PostCSS | 8.5.30 | October 12, 2026 | Web, desktop |

These versions address [SvelteKit Accept-header processing](https://github.com/sveltejs/kit/security/advisories/GHSA-29g2-3rmr-qm68), [devalue serialization](https://github.com/sveltejs/devalue/security/advisories/GHSA-j22f-vq7h-c4qm), [nanoid size handling](https://github.com/ai/nanoid/blob/3.3.20/CHANGELOG.md), [source-map-js indexed section offsets](https://github.com/7rulnik/source-map-js/pull/79) and later PostCSS source-map hardening. URLs were obtained through connected research services and maintainer sources. Registry publication timestamps and archive integrity evidence are retained in ignored `.cache/local-evaluation/dependency-security-patch-metadata.json`.

## Verification and outcome

Both guarded fresh installations with `npm ci --ignore-scripts` passed, and both npm projects report **zero advisories**. Independent lock-only audits agree. This describes the checked npm dependency graphs; it does not characterize Rust dependencies or prove application-level exploitability.

Final client checks and production-image evaluation are recorded in the [foundation outcome](../design/PLAYBACK_COMPATIBILITY.md#verification-and-outcome). Audits and installation logs are under ignored `.cache/local-evaluation/playback-*`; resource evidence is under `.cache/testing-memory/`.

The later bounded-recovery iteration retains these lockfiles and rechecks the shared client with 410 Vitest cases, 37 Node cases, zero source/type diagnostics and four browser recovery cases. Its final image and backend results belong to [Playback recovery](../design/PLAYBACK_RECOVERY.md#acceptance-and-outcome). This is regression evidence for the already superseded PR delta, not a new application or remote merge of PR #5.
