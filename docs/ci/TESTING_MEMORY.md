# Testing memory on Windows

## Findings — October 7, 2026

The user reported system-wide sluggishness and freezes during development/testing. Read-only samples found 31.92 GiB installed RAM, available physical memory falling from 6.35 to 2.96 GiB, and system commit rising from 56.8 to 59.16 GiB against a 64.53 GiB limit (88–91%). Substantial paging was observed. These samples establish memory pressure; they do not establish the cause of every freeze.

Largest private allocations included Docker's WSL VM (~12.56 GiB), rust-analyzer (~3.62 GiB), and the existing Chrome browser group (~3.91 GiB at the later sample). Working set and private allocation are different measures. No running Duskcue compiler/browser test or confirmed orphan testing process was found. Existing user applications were preserved.

Readable System logs had no matching resource-exhaustion, unexpected-reboot or bugcheck events in seven days. Application reports included a VS Code hang and graphics live-kernel events. Those reports do not attribute the freezes to RAM or testing. The sanitized inspection is in ignored `.cache/tonight-implementation/windows-memory-oct7.json`.

Read-only sample October 8, 06:46:52 UTC during lighter work: available physical RAM **4.779 GiB**, system commit **50.002/64.351 GiB**, commit headroom **14.349 GiB**. The three rust-analyzer processes together hold **4.252 GiB private allocation**; this is not resident RAM or an exclusive physical-memory total. Raw counters are retained in ignored `.cache/tonight-implementation/memory-light-work-20261008-064654.json`. This single sample is not a workload peak, a freeze diagnosis or proof of improvement caused by the WSL cap. The user's instruction to leave the editor running and continue lighter work still applies; no local heavy test, service restart or hardware/settings change accompanied the observation. Current complete qualification remains on bounded hosted runners.

## Research and decisions

Reviewed official sources October 7, 2026:

- [Microsoft page-file guidance](https://learn.microsoft.com/en-us/troubleshoot/windows-client/performance/introduction-to-the-page-file): system commit needs backing from RAM/page files; exhausting the commit limit can cause failures. Free RAM alone is insufficient evidence of headroom.
- [Microsoft process counters](https://learn.microsoft.com/en-us/previous-versions/aa394323(v=vs.85)): private bytes are distinct from resident/shared working set, and a recorded parent PID can be reused. Process-tree totals are sampled attribution, not exclusive physical memory.
- [Node command-line limits](https://nodejs.org/api/cli.html): old-space limits constrain a V8 heap region; they do not cap total process RSS, native allocations, browsers, or a VM.
- [Cargo jobs](https://doc.rust-lang.org/cargo/commands/cargo-build.html): default parallelism follows logical CPU count. Bound local job count rather than assuming available RAM scales with cores.
- [Playwright workers](https://playwright.dev/docs/test-parallel): workers run independently. One worker lowers overlapping browser/test allocations while retaining each test's isolation.
- [Docker constraints](https://docs.docker.com/engine/containers/resource_constraints/) and [FFmpeg threading](https://ffmpeg.org/ffmpeg.html): bound disposable encoder containers and decoder/encoder/filter threads. These limits do not cap Docker Desktop's entire VM or native FFmpeg RSS.

The repository's Playwright and Vitest configurations now default to one worker. The Windows resource runner serializes heavy workflows, checks current host headroom before startup, limits Node old-space and Cargo/test concurrency, and monitors actual system commit during the owned job. This is mitigation and measurement, not a claim that the freezes are resolved.

## Guarded commands

Run from the repository root. Use the resource wrapper for remaining Windows qualification; do not overlap raw builds or browser runs with it.

```powershell
node scripts/testing-memory/run.mjs web-unit
node scripts/testing-memory/run.mjs web-e2e playback-annotations.spec.ts playback-recovery.spec.ts playback-scope.spec.ts
node scripts/testing-memory/run.mjs web-check
node scripts/testing-memory/run.mjs web-build
node scripts/testing-memory/run.mjs desktop-build
node scripts/testing-memory/run.mjs cargo-check
node scripts/testing-memory/run.mjs cargo-clippy
node scripts/testing-memory/run.mjs tauri-build --debug --no-bundle --config '<isolated qualification config>'
node scripts/testing-memory/run.mjs native-qa --run '<isolated qualification manifest>'
```

The native workflow retains the [isolation requirements](TONIGHT_DESKTOP_TESTS.md). Other platforms use their normal repository commands; this wrapper measures Windows counters. Extra arguments cannot override its worker/job/heap flags.

These reserves are explicit local implementation defaults, not standards or guaranteed maximum workload consumption:

| Workload | Minimum available RAM | Minimum commit headroom | Maximum preflight commit usage |
|---|---|---|---|
| Unit | 2 GiB | 2 GiB | 92% |
| Browser | 3 GiB | 8 GiB | 85% |
| Sequential web source/type check | 4 GiB | 8 GiB | 85% |
| Build / Cargo / native | 4 GiB | 12 GiB | 85% |

Source checking has a separate policy because its JavaScript stages run sequentially with a 3 GiB old-space limit and no native linker/browser. The larger build/Cargo/native reserve remains unchanged. Measure each workload's actual process-tree peak before further calibration; a marginal refusal is not evidence that a check passed.

An owned running job is stopped if available RAM falls below 1.5 GiB, commit headroom falls below 2 GiB, commit usage reaches 94%, or measurement becomes invalid/stale. This polling is best effort; it cannot prevent a sudden unrelated allocation, kernel fault or every freeze. Measurements older than ten seconds fail closed. Logging is bounded and a stalled evidence writer stops authorizing the job.

Child environments set Cargo jobs to two, Rust test threads to one, and the libuv thread pool to two. Node old-space defaults are 1536 MiB for unit/native-driver commands and 3072 MiB for check/build/browser commands. Conflicting inherited percentage/old-space flags are removed only from that child environment. Existing global configuration remains intact.

## Ownership and cleanup

`policy.mjs` owns validation/reserves; `windows-sampler.ps1` owns sanitized measurement; `presets.mjs` owns command/environment defaults; the workflow lock owns single-job serialization; process/container helpers own verified cleanup; `run.mjs` orchestrates their lifecycle. No new global application singleton is introduced.

The lock records the actual wrapper and child PIDs. Recovery rereads stale state under an exclusive recovery lock and conservatively rejects live/reused PIDs or incomplete recovery. A failed process cleanup preserves the active lock. A lock file alone is never evidence that a job is running; check its recorded process and creation identity.

Before stopping a Windows tree, the helper checks the actual spawned child's current executable name, parent PID and creation-time window. It targets only that owned command and its descendants. Existing browser/editor/VM applications are not terminated. Process-tree telemetry may omit brief exited children and summed working set can count shared pages more than once.

Cold fixture generation uses uniquely named Docker containers with an exact run ownership label, 512 MiB memory and swap ceiling, two CPUs and two FFmpeg decoder/encoder/filter threads. Marker files let the wrapper inspect that run's immutable container IDs and labels before cleanup if a killed Docker client cannot clean itself. Cached encoded clips are preserved and bypass generation. An interrupted legacy media-generation lock can still require explicit verified recovery; an opaque lock is not automatically deleted.

## Evidence and remaining work

Ten policy cases pass, including invalid/stale measurements and each reserve boundary. Five ownership/lock cases pass, including actual minimal helper-tree cleanup, rejection of mismatched identity, one stale-lock contender winning, atomic child metadata and incomplete recovery. The live sampler emits sanitized counters and fails with structured errors instead of inventing zeros.

A guarded web-build attempt was held before any compiler process started: available RAM 2.67 GiB, commit 56.18/64.53 GiB. Its log is under `.cache/testing-memory/`. This is a successful refusal under pressure, not a build pass. Each subsequent run records sampled system/owned-tree peaks; short bursts between samples can be missed.

The guarded one-worker unit run passed all 280 cases in 23 files. Sampled owned-tree private allocation peaked at 431.9 MiB, summed working set at 421.1 MiB, minimum host available RAM at 4.85 GiB and maximum system commit at 55.62 GiB. The tree includes the wrapper, sampler and test descendants. These are sampled values, not exact instantaneous peaks or a comparison against an unmeasured previous run. Log: `.cache/testing-memory/2026-10-07T12-52-54-555Z-a306b6e5-78a9-45f0-b39e-968e0b2182ba.jsonl`. Seventeen tiny policy/preset/ownership checks also passed; test isolation remains enabled.

Actual constrained web/browser/build/native peaks, cold-generation limits and the remaining Tonight qualification are pending. Leave the goal active. Before resuming heavy work, obtain sufficient current headroom by finishing or closing unused user workloads; do not silently shut down Docker/WSL, editors or browser sessions. Do not change page-file, GPU, global WSL or driver settings based only on these samples.

## Approved temporary WSL cap — applied October 7

The user has bought additional RAM, arriving in a few days. A read-only follow-up found only `docker-desktop` registered in WSL. Its two running containers were `classifarr` (~822 MiB of its existing 2 GiB limit) and `harmoniarr-walkthrough-harmoniarr-1` (~125 MiB). Linux reported ~15.58 GiB total and ~12.62 GiB available, while Windows reported a much larger VM private allocation. This supports investigating VM allocation/cache behavior; it does not mean all those private bytes are physically resident or predict an exact amount reclaimed.

No user `.wslconfig` existed when inspected. The user explicitly approved this temporary configuration and the Docker restart:

```ini
[wsl2]
memory=8GB
swap=4GB
pageReporting=true
```

[Microsoft WSL configuration](https://learn.microsoft.com/en-us/windows/wsl/wsl-config) documents these settings and states that shutdown affects all running distributions. Current documentation already defaults automatic memory reclaim to `dropCache`; do not claim it is disabled or change it without evidence. The proposal therefore leaves that mode unchanged.

The prior absence/configuration is recorded in ignored `.cache/testing-memory/wsl-cap-original.json`. Docker Desktop was stopped gracefully, the registered WSL VM shut down, then Docker Desktop restarted. Both original container IDs returned running/healthy, with no OOM-killed flag; their existing restart policies and data were preserved. Linux now reports 8,134,992 KiB total (~7.76 GiB after kernel reservations), confirming the new limit.

The immediately preceding Windows sample was available RAM 1.32 GiB and commit 72.86/74.73 GiB; after services restarted it was 5.86 GiB and 52.55/64.36 GiB. The system-managed commit limit also changed during this interval. This is observed headroom improvement, not controlled attribution of the entire difference to WSL or proof that freezes are fixed. Global page-file/GPU/driver settings were not changed.

The next guarded six-case browser regression run reached a sampled owned-tree peak of 1.600 GiB private and 1.822 GiB summed working set. Minimum host available RAM was 6.284 GiB and peak system commit was 53.869/64.363 GiB. No emergency stop or new Docker encoder occurred; existing clips were reused. Five tests passed; a real cross-tab playback replay defect was found and corrected, with its rerun pending. Log: `.cache/testing-memory/2026-10-07T13-14-54-695Z-e54ccd0c-1d2c-45c5-a99b-9b5eaa0b884c.jsonl`.

The unchanged cross-tab regression subsequently passed through the wrapper, with no new playback start under the switched profile. Its sampled private peak was 1.591 GiB, working set 1.819 GiB, and minimum available RAM 5.134 GiB. The full current unit suite later passed 319 cases in 26 files: sampled owned private peak 440.7 MiB, working set 427.6 MiB, minimum available RAM 5.16 GiB. Source/type checking also passed with zero errors/warnings and a sampled private peak of 1162.3 MiB, keeping at least 8.45 GiB available. These results support the constrained workflows; browser/build/native totals and brief peaks remain distinct.

The web bundling attempt remained held at 6.63 GiB available, commit 55.09/64.35 GiB, below the build's 12 GiB commit reserve and over its 85% preflight limit. No build was started by that refusal. Current native/static builds remain pending until their fresh resource gate allows them; the goal remains active.

Review the cap after the new RAM is installed. Eight GiB is an interim observed-workload limit, not a permanent hardware recommendation. If Linux services exhibit allocation failures or degraded behavior, restore the prior configuration with an approved restart. On rollback, first verify this file still matches the applied configuration; do not remove subsequent user edits. Further unrelated service/editor/browser restarts need separate scope/approval.

The later 13-case release/reflow browser run finished with eight passing and five functional/fixture failures, without a memory stop. Sampled owned private allocation peaked at 1.636 GiB, summed working set at 1.848 GiB, and host available RAM stayed at or above 4.719 GiB; maximum commit was 54.229 GiB. Telemetry: `.cache/testing-memory/2026-10-07T15-26-12-817Z-cdcdc285-11ff-4c87-a6d4-35449aa4d784.jsonl`. The passing cases and newly exposed UI/navigation gaps are recorded in the [Tonight plan](../branding/TONIGHT_IMPLEMENTATION_PLAN.md); this is workload evidence rather than an all-green suite or freeze-resolution claim.

The earlier full client unit suite passed **337/337 in 28 files**; sampled private peak was 447.4 MiB, working set 428.0 MiB, minimum available RAM 4.719 GiB. The full **97-case browser** run finished 94 passing/three mocked-fixture failures, without a memory stop. Its sampled peaks were **2.080 GiB private / 2.255 GiB working set**, minimum host available RAM **2.685 GiB**, and maximum commit **57.326 GiB**. The three fixture failures were corrected and the five-case desktop rerun passed. The browser's lower minimum shows why later builds cannot assume the earlier six-case headroom. Logs: `.cache/testing-memory/2026-10-07T15-40-23-918Z-2b48c085-7d7b-4739-a882-36271ee85dfb.jsonl` and `2026-10-07T15-45-18-218Z-97c82801-8e44-4d79-bb09-ec7af07c940c.jsonl`. Retain isolation, one worker and fresh gates; these sampled results do not guarantee the absence of paging or freezes.

After authentication/popover/logout integration, the final shared source check passes with zero errors/warnings. Sampled owned private memory was 1.132 GiB, working set 1.140 GiB, minimum available host RAM 5.225 GiB, and maximum system commit 55.287 GiB. The current whole unit suite passes **358/358 in 30 files**; sampled private memory was 0.426 GiB, working set 0.412 GiB, minimum available RAM 5.269 GiB, and maximum commit 54.978 GiB. Both finished without an emergency stop. Evidence: `.cache/testing-memory/2026-10-07T18-23-32-960Z-bbbe2975-b578-466b-9ba4-57d09fa51bbe.jsonl` and `2026-10-07T18-31-03-605Z-bf997c92-fdf2-497f-8bff-1c961f5f1c1c.jsonl`. The earlier source-check hold is superseded by this admitted pass; current build/native qualification still requires its distinct 12 GiB reserve.

The user then explicitly approved temporarily stopping the existing VS Code Rust language server. Its exact PID, parent, executable path and creation time were verified before stopping PID 85684; evidence is `.cache/testing-memory/rust-analyzer-approved-stop.json`. Immediately afterward, available RAM was 6.45 GiB and commit headroom 14.51 GiB. VS Code automatically spawned a new language server and background Rust checks. The subsequent guarded web build was held before compiler startup at 2.58 GiB available RAM and 55.77/64.35 GiB committed; it did not pass. Evidence: `.cache/testing-memory/2026-10-07T18-44-29-949Z-4ed560d9-a811-45ef-9545-27586c53865e.jsonl`. The installed extension exposes `rust-analyzer: Stop server`; native editor control is unavailable. After being offered that command, the user explicitly chose to leave the server running and continue lighter work. Preserve that choice: do not repeatedly kill newly spawned editor processes or silently change extension/workspace configuration.

The earlier 367-unit/107-browser checkpoint is retained in `.cache/testing-memory/2026-10-07T19-24-53-632Z-5daf3788-e479-4432-a7a9-bf11bad798ed.jsonl`. The later localized client check passed with zero diagnostics; **372 units in 33 files** passed. The complete **115/115 browser** run, with actual-caption cases explicitly enabled, finished in 4.5 minutes without a memory stop. Its sampled owned private/working-set peaks were **2.205/2.276 GiB**, minimum host available RAM **1.786 GiB**, maximum system commit **56.003 GiB**. Evidence: `.cache/testing-memory/2026-10-07T21-46-01-640Z-ec11cc9f-fc8a-4e4b-b335-d98add038efb.jsonl`. The small available reserve remains relevant to the user's Windows freezing concern; successful sampling does not prove that concern is resolved.

The isolated current-at-21:54 Tauri build passed in 2m04s, sampling **3.092/2.840 GiB** private/working set, minimum available **2.157 GiB**, maximum commit **55.232 GiB**: `.cache/testing-memory/2026-10-07T21-54-47-410Z-206f4827-3508-4fca-bba1-954c3bfefeab.jsonl`. The actual native run failed a strict Search focus exposure check at engine zoom 4; owned process/keyring/listener cleanup passed. Functional failure is distinct from memory failure.

After the CSS Search correction, 24 targeted browser cases passed with sampled peaks **2.065/2.138 GiB**, minimum available **3.876 GiB**, maximum commit **53.876 GiB**: `.cache/testing-memory/2026-10-07T22-00-47-999Z-995e46ab-00a1-403e-9a0f-04662de6b490.jsonl`. Current source check and web build passed. The corrected static build was held before launch at available **5.57 GiB**, commit **52.75/64.35 GiB**, headroom **11.60 GiB** below the unchanged 12 GiB requirement: `.cache/testing-memory/2026-10-07T22-04-18-294Z-d3e021a2-2e2f-433e-b533-5dcce1beb4b5.jsonl`. The latest user instruction is to leave rust-analyzer running and continue lighter work. Do not stop it again, lower the gate or count the held build as a pass; prepare source/documentation and isolated qualification without a new heavy launch.

Fresh reserve then admitted the production web build, which passed without changing user apps or global settings. Sampled private memory was 1.934 GiB, working set 1.182 GiB, minimum available RAM 6.905 GiB, and maximum system commit 52.944 GiB. Evidence: `.cache/testing-memory/2026-10-07T19-33-01-553Z-41af5f04-2bd4-4320-8508-b6facf3acf5d.jsonl`. The subsequent desktop static build was held before launch at **4.59 GiB available / 10.81 GiB commit headroom**, below its 12 GiB reserve. Evidence: `2026-10-07T19-43-38-668Z-85f7ecf9-55c1-4f87-8201-7441d8a787b8.jsonl`. Dynamic admission of one build does not authorize assuming later headroom; current native/backend qualification remains pending fresh gates.

## Source-prepared Linux compiler qualification

Prepared October 7, 2026; **no Linux compiler container, image pull or image build has run through this workflow**. The `linux-cargo-check` preset uses the existing normal Cargo policy: at least 4 GiB available Windows RAM, 12 GiB commit headroom and at most 85% commit. Its Node driver receives a 128 MiB heap, and the existing Windows sampler/emergency guard remains active. The driver also verifies that its PID is the registered child of the `linux-cargo-check` workflow lock, so a diagnostic or unit preset cannot substitute for compiler admission.

Windows headroom alone does not establish spare memory inside the capped WSL VM. The separate Linux sampler requires the current local Docker Desktop named-pipe engine, a matching WSL2 kernel and an already-running `docker-desktop` distribution. It reads `/proc/meminfo`, `/proc/version` and the kernel boot ID without restarting Docker/WSL or changing global settings. [Microsoft's WSL command reference](https://learn.microsoft.com/en-us/windows/wsl/basic-commands) documents listing running distributions; [the kernel's `/proc` reference](https://www.kernel.org/doc/html/latest/filesystems/proc.html) documents `MemAvailable`. Before creating a compiler container, the runner requires fresh matching evidence with **5.5 GiB MemAvailable**, covering its fixed 4 GiB container allowance plus a 1.5 GiB reserve. During compilation, it samples every two seconds and stops only the owned container/process if that reserve falls below 1.5 GiB, evidence is older than ten seconds, sampling fails or engine/boot identity changes. Read-only CLI metadata has a five-second deadline and a 64 KiB output limit. Existing user workloads, the 8 GiB VM cap and editor processes remain unchanged.

Prerequisites are an explicit **already-local immutable image SHA-256** and a prepared offline Cargo registry containing nonempty `cache`, `index` and `src` directories. The compiler image must match the engine architecture and carry `duskcue.qualification.compiler=linux-cargo-check-v1` plus `duskcue.qualification.rust-toolchain=<installed stable version>` labels. It must contain that Linux Rust toolchain and the repository's native compiler prerequisites, with the toolchain readable by UID/GID 65532. No such prepared local compiler image was found in the October 7 inventory. Image and offline dependency preparation therefore remain a separate future task through normal build admission and owned resource limits; this runner never pulls, installs packages or invents support for BuildKit limit flags. Missing/incompatible image or missing registry fails before registering or creating a container. An incomplete dependency cache fails the actual offline Cargo command, with no network fallback.

When these prerequisites and both current gates are satisfied, use this command from the repository root:

```powershell
node scripts/testing-memory/run.mjs linux-cargo-check --image '<local prepared sha256:...>' --registry '<prepared offline Cargo registry directory>'
```

The fixed command is `cargo check -p duskcue --tests --locked --offline -j 2`, preceded by actual `rustc -vV`/Cargo version output. It checks backend and test compilation rather than running tests. Its owned container uses `--pull=never`, **4 GiB memory with equal memory-swap (zero extra swap), two CPUs and 256 PIDs**. [Docker's resource documentation](https://docs.docker.com/engine/containers/resource_constraints/) explains the equal memory/swap setting. The root filesystem and source/registry mounts are read-only; isolated target/Cargo-home directories are writable. Network is disabled, all capabilities are dropped, no-new-privileges is enabled, and Docker's default seccomp remains in place. There is no Docker socket mount, privileged mode, unconfined seccomp or production data mount. The existing 512 MiB diagnostic containers retain their limits unchanged.

Source inputs, including Rust code/tests, manifests/lockfile, migrations, embedded assets, locale files and applicable repository toolchain/config files, are hashed with bounded reads, copied into an isolated snapshot and verified before compilation. The read-only snapshot and current source hashes are checked again afterward; changed input invalidates the result. Artifacts remain under `.cache/testing-memory/linux-cargo-<UUID>/`: `source-manifest.json`, `compiler.log`, `linux-memory.jsonl`, `result.json` and, on an owned emergency stop, `stop-evidence.json`. Logs stream with 64 KiB backpressure and a combined 32 MiB ceiling; the command deadline is 30 minutes. The marker records the exact UUID/name and validated Docker context, and cleanup rechecks the ownership label before removing that container by ID. The wrapper uses the same recorded context for cleanup if the driver is interrupted.

Eleven deterministic compiler-helper checks plus the two existing preset checks pass under a standalone 64 MiB Node heap. They cover Linux reserve boundaries, stale/changed provenance, rejecting remote/non-WSL engines, immutable image declarations, fixed container limits, registered workflow identity, context-bound ownership, offline-registry prerequisites, copied-source changes and bounded output. A read-only real sampler probe at 22:48:11 UTC matched engine `fd1881c2-e019-4b2f-aadc-1aab914aa87a`, WSL boot `c1c5d40c-9fdf-422b-9d70-e1a6bc5e0d67` and kernel `5.15.167.4-microsoft-standard-WSL2`; it observed 5.736 GiB MemAvailable with 225 ms sample age. This qualifies the sampler's read-only path at that instant. Container launch, stream/deadline/emergency cleanup behavior and Linux compilation remain unexecuted and require their own actual evidence; helper tests and a spare-memory sample are not a compiler pass.
