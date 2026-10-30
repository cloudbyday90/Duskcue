# SvelteKit 3 dependency PR review

## Scope and evidence

On October 28, 2026, GitHub MCP identified one open candidate, [Dependabot PR #6](https://github.com/cloudbyday90/Duskcue/pull/6). Its base was the current main checkpoint `08052e5fff05c24c07ae4ff1064bbbb2babe75fa`; head `b14d4aeaf556bf657b0627f91d208bc9c320ccb0` was open, mergeable and unmerged. It was fresh rather than a stale PR requiring rebase. Previous Dependabot #5 cleanup remains separate and complete.

The complete patch updates SvelteKit to 3.1.0, adapter-node to 6.0.0 and adapter-static to 4.0.0, with their lockfile changes. The title alone understates those adapter updates. The patch was fetched at its verified immutable head and applied locally on main, without creating a branch or merging the PR.

## Local outcome

Managed `npm ci --ignore-scripts` succeeded. Managed `web-check` failed immediately with `config_file_unsupported`: SvelteKit 3 rejects the existing `svelte.config.js` and requires configuration in the Vite plugin. This occurred before application checking; it is not a passing migration or a comparison with the repository's existing application diagnostics.

Only the trial's two dependency files were restored from byte-preserving starting copies after verifying they still matched the fetched patch. A subsequent managed install restored the original lockfile graph; the actual installed SvelteKit is again 2.70.2. No forced dependency resolution, validation suppression, PR merge, remote comment or release occurred. The corrected local patch-file attempt and logs remain ignored.

| Check | Outcome / ignored memory receipt |
| --- | --- |
| Exact candidate installation | Pass; `2026-10-29T01-17-14-749Z-5c2a8891-2a79-4d7c-9bab-e4f30189b036.jsonl` |
| Candidate application check | Failed at configuration; `2026-10-29T01-17-46-320Z-c7663fae-634a-43f0-af3e-fa2d57b96266.jsonl` |
| Original installation restoration | Pass; `2026-10-29T01-18-01-456Z-1e8f72a6-bbd0-497e-853a-15f04659fc83.jsonl` |

The restored install reports six low-severity npm audit findings; the candidate install reported five. These counts are installation diagnostics, not validated vulnerability assessments or a claim that this PR fixes the remaining findings.

## Research, choices and recommendation

The official [SvelteKit 3 migration guide](https://svelte.dev/docs/kit/migrating-to-sveltekit-3), found through MCP search, requires configuration migration and newer framework/runtime prerequisites. It also describes cookie v2 and changed cookie defaults. The [official changelog](https://github.com/sveltejs/kit/blob/main/packages/kit/CHANGELOG.md) records additional framework changes. The currently broad `cookie: ^0.7.0` override must be reviewed against the new major API; adapter installation alone does not qualify that boundary.

| Approach | Pros | Cons |
| --- | --- | --- |
| Accept the dependency-only patch | Small package-file change | Demonstrably fails current application validation; does not complete the documented migration |
| Coordinate the framework migration | Preserves both web and static-desktop configurations while testing changed behavior | Requires source/tooling changes and browser/desktop verification beyond this package-only PR |
| Retain the existing graph temporarily | Keeps the verified application stack while Android work continues | Defers framework fixes and retains current audit findings |

Recommendation: retain the current graph for this Android checkpoint, then implement a separately reviewed coordinated migration. Move the existing adapter/preprocessing configuration into Vite, reconcile cookie/runtime/type requirements and affected scripts/imports, compare application diagnostics, and test both Node web and static Tauri builds plus relevant auth/navigation/playback behavior. Rebase the bot PR only if it actually becomes stale; do not confuse an application migration failure with a merge conflict.

The [Android runtime gate](ANDROID_PLAYBACK_QUALIFICATION.md) remains the immediate playback prerequisite. No security, accessibility or native-runtime success is inferred from this dependency trial.

## October 30 retrial

GitHub MCP again returned only PR #6, at the same immutable head, open, mergeable and unmerged. Its exact patch applied on current main. Managed install passed (`2026-10-30T09-07-44-425Z-273d77c0-7a08-481f-adee-7e0c3dbdeb3c.jsonl`); application check again failed at `config_file_unsupported` (`2026-10-30T09-08-17-998Z-e215747b-61ef-4fc5-afb3-6b62de7bb694.jsonl`). Byte-preserved dependency files and the original installed graph were restored successfully (`2026-10-30T09-13-43-322Z-47476763-0d6a-4bf4-9927-5ba0e002a8f5.jsonl`). No conflict/stale rebase or merge applies. The coordinated migration recommendation stands.


The subsequent Windows-worker review on October 30 again finds only PR #6, open/unmerged/mergeable at the same head `b14d4aeaf556bf657b0627f91d208bc9c320ccb0`. Web source/dependency inputs relevant to the already executed trial are unchanged, so its documented configuration failure remains applicable; the dependency-only patch is not adopted. No conflicting stale rebase, merge, remote message or release applies.
