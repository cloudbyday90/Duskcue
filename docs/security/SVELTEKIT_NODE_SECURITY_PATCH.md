# SvelteKit Node adapter security patch

## November research and decision

During actual installation of the [compatible Kit/static candidate](SVELTEKIT_2_SECURITY_PATCH.md), a fresh npm audit reports moderate [GHSA-rqw3-x7hx-mm3p](https://github.com/advisories/GHSA-rqw3-x7hx-mm3p) in installed Node adapter 5.5.7. The API's reviewed range recognizes 5.5.8, while the upstream advisory HTML still describes only 6.0.1. Official MCP retrieval and Astra's independent read-only review establish the maintenance backport from the [5.5.8 release](https://github.com/sveltejs/kit/releases/tag/@sveltejs/adapter-node@5.5.8), [tagged handler](https://raw.githubusercontent.com/sveltejs/kit/@sveltejs/adapter-node@5.5.8/packages/adapter-node/src/handler.js) and upstream regression. The advisory's request-header wording differs from the actual response-header sink; preserve that distinction.

The adapter tries to mutate a fetched SSE Response's immutable headers. The fix writes `x-accel-buffering` to Node's outgoing response instead. Tagged manifests retain consumer dependencies, exports and the Kit peer requirement. Duskcue's inspected API proxy creates fresh Response/Headers objects; package affected status does not establish application exploitability. A generated isolated production route will exercise the actual adapter without adding a product route.

| Option | Pros | Cons |
| --- | --- | --- |
| Raise only Node adapter to 5.5.8 | Compatible maintenance fix; keeps the Kit 2 configuration and current graph | Requires clean installation and fresh builds/tests |
| Adopt the pending major PR | Coordinates a future framework migration | Already fails current configuration; its proposed versions are affected |
| Keep 5.5.7 while qualifying the other patches | Avoids another installation | Leaves a known affected adapter and omits a repaired boundary |

Recommendation: retain Kit 2.70.4/static 3.0.11 and all supporting versions, raise the Node adapter floor/locked distribution to 5.5.8, and require its exact installed identity. Preserve unrelated lock nodes, consumer metadata, cookie override, application contracts, resource policies and production-off behavior. No release.

## Qualification contract

Extend the existing owned generated fixture with a finite nonce-bearing SSE endpoint returning bare global `fetch` of a data URL. Do not clone it or use a fresh Response in that triggering route. Require exact status, body, content type and buffering header; then verify a healthy response on the same server PID. Add a finite mutable SSE control and ordinary non-SSE control. Use the current HTTP/process owners, deadlines and cleanup. No additional network sink or dependency is needed.

First attempt the new case against installed 5.5.7 in the isolated build, retaining the original failure and actual cleanup. Then update only the selected adapter identity, clean-install and repeat. The ledger gains one mandatory case, making thirteen synthetic case identities. Passing harness units or audit metadata cannot qualify runtime remediation. Complete application and synthetic gates in the [reviewed output-consumption order](../ci/SVELTEKIT_SECURITY_BOUNDARIES.md#execution-gate-and-build-output-ordering); retain W3C focus/status and ordinary application behavior checks. Missing/refused gates remain explicit.

## Outcome

Verified source: `6835c88aeb44adb937db92ef3f64733465d05c37` on main; the later documentation-only attribution changes no executed source or package inputs.

The manifest floor is now `^5.5.8`; the lock node uses the published 5.5.8 distribution/integrity. Exact graph comparison preserves **189 other package nodes**, consumer dependency/peer metadata and all overrides. The guarded clean install confirms Kit 2.70.4/static 3.0.11/Node 5.5.8 and lock SHA-256 `64c1c98109286fc0064ff010042c95f88e7b79d7cbba35e87a8baa19fa7f2a80`.

The actual generated production baseline on Node adapter 5.5.7 crashes with `TypeError: immutable` in the adapter handler and the client observes ECONNRESET. After installation/rebuild with 5.5.8, the exact immutable Fetch SSE route returns 200, the nonce-bearing body and `x-accel-buffering: no`; the mutable and non-SSE controls and same-server healthy requests also pass. The final actual Windows matrix passes all thirteen cases and confirms owned cleanup. This is scoped runtime evidence for the adapter repair; it does not demonstrate that the inspected Duskcue proxy was exploitable.

The [execution authority](../ci/SVELTEKIT_SECURITY_BOUNDARIES.md#november-executed-qualification) retains the baseline, first harness-only navigation failure, corrected matrix and application receipts. Final-graph analysis has zero errors/warnings; 37 Node, 582 Vitest and 48 orchestration tests and both application builds pass. Actual built-Node proxy smoke passes before consuming static output, then a fresh Node rebuild and stronger SSR-reference smoke pass. Browser checks finish with 165 passes and the same three caption-artifact skips. No native-platform/backend-auth or complete accessibility qualification is inferred.

The final npm audit reports zero moderate/high/critical entries. Six low affected-package reports map to three advisory identities: enhanced-form redirect, cache-header metadata and deepmerge. The cache-header case passes against the maintenance backport despite stale advisory range metadata. No advisory is dismissed or forcibly overridden. Keep those low reports and a future patched-major migration separate; the already incompatible PR remains unadopted. No release or production-capability change.
