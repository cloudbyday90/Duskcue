# Playback dependency review

## Design and provenance — October 17, 2026

This dependency review accompanies the [playback compatibility foundation](../design/PLAYBACK_COMPATIBILITY.md). The user requested a random open PR implemented and tested locally without merging it.

GitHub MCP pagination returned the complete open pool `[5]` and an empty second page. A cryptographically generated uniform index selected PR [#5](https://github.com/cloudbyday90/Duskcue/pull/5), head `c018d77b7f09543e5960a571dc43e2f061cedf0a`. Its two-lockfile patch upgrades PostCSS 8.5.16 to 8.5.23 and nanoid 3.3.15 to 3.3.16. The exact patch was applied, installed and tested locally. The remote PR remains open and unmerged; no review comment, closure or release was created.

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
