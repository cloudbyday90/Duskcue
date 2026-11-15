# Qualification memory attribution

## Resumed Android ambient execution

The final focused seven-stage resource `3278d13c-25ab-4631-a255-b0c510f0138d` and combined resource `d2ac3789-3010-42ea-acfd-acb1772152f6` both pass without a memory stop. Focused sampled peaks are **6.855 GiB owned private**, **74.232 GiB system commit**, **88.820% commit**, with **16.370 GiB minimum physical availability**. Combined peaks are **6.917 GiB owned private**, **73.643 GiB system commit**, **88.115% commit**, with **16.226 GiB minimum physical availability**. Their private UTC receipts are `2026-11-15T03-46-07-533Z-2a8fa2df-5dba-46bd-9850-04dcf834d56b.jsonl` and `2026-11-15T03-55-18-343Z-56c617c5-298a-40c6-863e-f9f0ba448153.jsonl`. These are sampled maxima, not additive whole-system attribution or a leak proof.

After root/process/listener/container completion and exact AVD maintenance, a snapshot observes **71,038,070,784 committed bytes** against **88,723,173,376 bytes** of commit capacity, with **24,514,215,936 physical bytes available**. No emulator, test backend or FFmpeg process remains. Shared host activity and changing system commit capacity prevent assigning the whole decline to this test. [Current qualification](ANDROID_AMBIENT_QUALIFICATION.md#combined-regression-and-cleanup) separates nineteen real journeys plus the worker guard from still-unqualified fault cases. No user app/model/service or host policy is changed to obtain these passes.

After operator memory cleanup, the existing guard admits actual compiler/emulator work. Four failed runtime attempts at private UTC receipts beginning `2026-11-15T02:05:50`, `02:17:24`, `02:24:31` and `02:31:33` reach application assertions; none is stopped by the memory guard. Their maximum sampled owned-process private bytes are respectively **6.74, 4.77, 5.52 and 5.53 GiB**; maximum Windows system commit percentages are **89.08%, 89.55%, 89.22% and 87.56%**. The first also compiles the Rust library; later attempts reuse Cargo's checked artifacts. These are observed process-tree maxima, not a hard memory ceiling, additive system-commit attribution or proof about shared WSL/Docker memory.

The [qualification record](ANDROID_AMBIENT_QUALIFICATION.md#resumed-compiler-and-native-qualification) distinguishes the actual source-factory exception, format refusal and seek/server refusals. Their driver/test failures must not be reported as host-memory or bootstrap failures. Each root result reports owned cleanup; emulator/ADB listeners and disposable AVD ownership are checked separately before manager retirement. Preserve one emulator, two CPUs, 2,560-MiB guest RAM, serial heavy execution and all current start/emergency/deadline rules. No app, model, unrelated container, service, page-file or OS setting is changed by this continuation.

## Scope and method — October 31, 2026

This investigation measures Duskcue development/qualification processes and the surrounding Windows host. It is not application-media cleanup or a change to service, compiler, VM, page-file or guard limits. The [scope-ownership increment](../design/FLUTTER_SCOPE_CANCELLATION.md) remains the active implementation task. Raw host process inventories stay ignored under `.cache/android-playback/`; this document publishes sanitized measurements and recommendations.

The existing Windows sampler provides system commit, physical availability and process private/working-set counters. A live snapshot walks the active guard PID's descendants, rather than classifying every Dart/Node/Java process as test-owned. All names are grouped only after retaining distinct PIDs; command lines, credentials and personal paths are not needed. Current native output and durable receipts identify each qualification phase. A post-cleanup snapshot verifies that the emulator/server/FFmpeg processes are gone. Docker statistics and Linux `/proc/meminfo` give additional VM context without stopping services.

Microsoft distinguishes process private commit from resident working-set pages; working sets can include shared pages, so their sum is not total physical usage. System commit also includes allocations beyond the measured process-private sum. A gap between these counters does not by itself establish a leak. [Microsoft memory counters](https://learn.microsoft.com/en-us/windows/win32/memory/memory-performance-information).

Free physical RAM and available commit are different budgets. A page file contributes to the commit limit; its current disk usage is not the same as system committed bytes. These measurements explain why a host with free RAM can still hit the unchanged commit guard. No page-file setting is modified. [Microsoft page-file guidance](https://learn.microsoft.com/en-us/troubleshoot/windows-client/performance/introduction-to-the-page-file).

## Measured consumers

The live snapshot at `2026-11-01T01:24:50Z` occurs between original-suite stages, after the decoded baseline and during the cold-journal series. It is one observation, not the peak:

| Consumer | Private allocation, GiB | Attribution |
| --- | ---: | --- |
| Whole guarded test tree | 4.672 | Includes the emulator below; do not add these two rows |
| Android emulator | 4.385 | 3.982 GiB resident; 2.5-GiB guest RAM does not cap total host-process memory |
| WSL host VM | 7.423 | Shared Docker/VM allocation, not solely this test |
| Rust language server | 4.303 | Host editor tooling, outside the test tree |
| Browser processes | 4.267 | Host applications, outside the test tree |
| Editor processes | 4.265 | Additional to the separately named Rust language server |
| Chat application processes | 2.963 | Host applications, outside the test tree |
| Background SQL processes | 1.752 | Outside the test tree; not the disposable PostgreSQL fixture |

At that instant, the host has 63.920 GiB physical RAM, 23.822 GiB available, 63.072 GiB system commit and a 76.920-GiB commit limit. The measured process-private sum is 52.743 GiB, with 2.467 GiB paged and 1.879 GiB nonpaged kernel pools. These fields overlap different accounting domains; do not add them into a purported complete reconciliation. The page file is allocated about 13 GiB, with approximately 425 MiB then in use. GPU telemetry separately reports about 2.3 GiB used; WDDM does not supply per-process usage through this query, so it cannot establish detailed GPU attribution.

Earlier observations identify two local model-server processes at approximately 6.26 and 6.22 GiB private allocation. Together, about 12.5 GiB is a substantial contributor to the earlier commit pressure. They are no longer present before the latest run. The operator unloads models; the agent does not terminate them or other services. Other host activity also changes, so the whole system difference is not attributed exclusively to those processes.

### WSL and containers

The inspected configuration sets an 8-GB WSL memory cap, 4-GB swap and `pageReporting=true`. It contains no explicit `autoMemoryReclaim` entry. Current Microsoft documentation lists `dropCache` as the default, but the file alone does not prove how the running VM applied defaults or its last restart's settings. Configuration changes require a WSL restart; do not perform one during qualification or without applicable interruption authorization. [Microsoft WSL configuration](https://learn.microsoft.com/en-us/windows/wsl/wsl-config).

During the live observation, the disposable PostgreSQL container uses about 113 MiB of its 512-MiB limit. The other running application container uses about 1.33 GiB. Linux reports about 7.76 GiB total, 5.16 GiB available, 4.67 GiB buffers/cache, 1.63 GiB anonymous pages and about 24 MiB swap use. These figures support substantial reclaimable cache, not a claim that the test database consumes the whole Windows WSL allocation. Docker documents automatic reclaim as a way to return unused Linux cache after builds; CPU activity, workload and runtime configuration need inspection before recommending a change. [Docker WSL backend guidance](https://docs.docker.com/desktop/features/wsl/).

Do not lower the WSL cap as an unmeasured shortcut: managed Linux compilation already requires 5.5 GiB available guest memory. The same VM also supports application services. Physical working-set reclamation and reduction in Windows commit must be measured separately; neither is guaranteed by deleting disk images.

## During-run peaks and observed release

| Native attempt | Peak test-private GiB | Peak system-commit GiB | Outcome |
| --- | ---: | ---: | --- |
| `89571162` | 6.91 | 74.27 | Original eight journeys pass; host commit guard later stops the additional scope VM at 96.55% |
| `afacd262` | 5.28 | 70.47 | Driver connection fails before the fourth original journey reports a result; no memory stop |
| `0cfca527` | 5.01 | 66.76 | Real profile journey passes; session journey exposes the now-fixed qualification-helper content-coding error |
| `2c3bb016` | 7.458 | 65.94 | Both new scope journeys and exact SQL checks pass, followed by seven original journeys; driver connection fails before the worker journey begins; no memory stop |

The last run's highest test-private sample is at `01:26:59.921Z`. At `01:33:39Z`, after owned process/container cleanup, system commit is 58.731/76.920 GiB and available physical RAM is 27.343 GiB. No test guard, emulator, fixture server or FFmpeg process is present. The earlier live snapshot's emulator allocation disappears. Shared WSL and host editor/browser allocations remain. This supports observed release for this attempt; it is not a general proof against every long-run leak.

The latest driver failure is separate from host exhaustion. Captured app-exit information contains previous expected force-stops but no exit for the current application PID; its VM service starts before the host driver loses its connection. That evidence does not establish an application memory crash. The precise ADB/VM-service disconnect cause remains unresolved. No command replay, timeout increase or dependency update is used to convert it into a pass.

## Options and recommendation

| Option | Benefit | Cost / decision |
| --- | --- | --- |
| Unload unused local models during heavy qualification | Removes an observed allocation of about 12.5 GiB without changing test limits | Model availability pauses; operator performs the action; effective first step |
| Close unused editor/browser windows or restart an idle language server | Can reduce additional measured host allocation | Interrupts user work; never stop them automatically |
| Reduce WSL memory or restart Docker | Could reduce some host allocation | Shared services and the 5.5-GiB Linux reserve constrain this; not recommended without a separate measured plan |
| Increase the page file | Could raise commit capacity | Adds disk/commit allowance, not physical RAM or a leak fix; requires a separate operator decision, not a test bypass |
| Persist bounded per-process attribution alongside guard totals | Makes future failures diagnosable by phase and owner | Needs a small reviewed telemetry/retention design; recommended next tooling improvement |

Recommended stack: retain the current Windows commit/physical guard, serial workloads, fixed guest/compiler budgets and verified compiler/APK reuse; unload unused models before admission; use bounded, private process-tree snapshots at admission, selected phase transitions, peaks and cleanup. Record system totals separately from test descendants and other host consumers. Persist no command lines, credentials or full environment. Keep all telemetry finite and preserve compact failure evidence.

At this investigation checkpoint, the immediate unresolved item was the native driver's ADB/VM-service bootstrap, not a demonstrated server/client memory leak. The subsequent qualified outcome below resolves that gate. No OS, WSL, Docker or application-memory setting is changed by this investigation.

## Bootstrap qualification follow-up

The [confirmed bootstrap increment](ANDROID_DRIVER_BOOTSTRAP.md#implementation-and-qualification-outcome) subsequently passes both real scope journeys and all eight original journeys on one fresh guarded resource, `bd8e217e-d2b7-4eea-a1d1-a6f3380966cb`. It uses the same guest, host, heap, encoder, root and deadline limits. SDK/package versions and page-file/WSL settings remain unchanged. Peak test-private allocation is **6.800 GiB**, peak system commit **68.665/76.920 GiB**, and minimum physical availability **20.188 GiB**. There is no memory stop. Receipt: `2026-11-01T02-33-26-484Z-2d99da46-f61d-4db6-9b20-2b5f01dc1707.jsonl`.

The new ignored process-tree snapshot during an intermediate bootstrap attempt again attributes the larger host allocations to shared WSL and rust-analyzer, while the owned QEMU is about 3.12 GiB private at that particular boot observation. It is neither the successful run's peak nor a complete attribution of system commit; raw process rows stay private in `.cache/android-playback/bootstrap-memory-scope.json`.

After observed app/SDK/log/ADB/VM/backend/container completion and native artifact maintenance, the `2026-11-01T02:46:08Z` snapshot records **61.204/76.920 GiB system commit** and **27.435 GiB available physical memory**. No guarded task, QEMU/emulator, test backend or FFmpeg process remains. Shared activity prevents attributing the entire system decline to the task. The unrelated ADB server retains its original PID, Classifarr remains healthy without interruption, and no user app/model is stopped by this increment. Exact post-cleanup evidence remains ignored in `bootstrap-memory-after-cleanup.json`.

One corrected run loses an ADB forward during a read-only VM probe, before the SDK driver is launched, at healthy memory levels. The new owner avoids default emulator discovery collisions without reconnecting a failed stage or changing resource limits. The exact actor for every historical disconnect remains unproven. Memory pressure and transport ownership are distinct failure classes; preserve that distinction in future reports.

The next platform item is the remaining relay adversarial/backpressure/output-format matrix. Bounded persisted phase attribution remains a proposed tooling improvement rather than an implemented sampler feature. Continue serial guarded work, verified artifact reuse, scoped native cleanup and private compact evidence.

## November 1 relay qualification

The [relay authority](ANDROID_RELAY_QUALIFICATION.md#native-attempts-and-remaining-acceptance) separates three resources. First full attempt `581a5334-8fa9-49c2-99f8-245a99460807` passes the worker guard, both scope journeys and seven original journeys, then the unchanged 94% commit guard stops worker recovery. Between the final two samples, committed bytes rise from 71,460,483,072 to 78,279,122,944, about 6.35 GiB. Observed test-private bytes remain near 7,284 million. System commit reaches 94.777% of 76.920 GiB while more than 19 GiB of physical RAM remains available. Physical free RAM alone would not make that admission safe.

This does not identify the actor: two-second sampling can miss an allocation, new child or shared/kernel change, and a post-stop process list cannot reconstruct its cause. Do not label the jump external to the test, a relay leak, or an application crash. A bounded private snapshot records current/peak process memory without command lines, credentials or environments. Large retained host consumers include shared WSL and rust-analyzer; their presence does not prove that either caused the jump. The operator frees host memory; the agent stops no user app, model or service and changes no limit.

Focused companion `78ed4855-7fbb-44b3-a94c-8afa013110dc` passes with peak private allocation 6.344 GiB, system commit 66.186/76.920 GiB and minimum physical availability 19.544 GiB. Final full resource **`a7e2c0c0-5466-4af2-a23e-ee59024a79a7`** passes all twelve native journeys plus the worker guard with **6.804 GiB** peak test-private, **65.893/76.920 GiB** peak system commit and **21.631 GiB** minimum physical availability. It has no guard stop. The earlier sharp jump is not reproduced. Bounded scope/worker snapshots supplement the guard totals; this remains one-off diagnosis, not a newly installed sampler or scheduled process.

Receipts are retained privately: failed `2026-11-01T11-15-28-907Z-f6edee5e-45b8-4a10-8a80-44af2849f784.jsonl`, focused `2026-11-01T11-31-25-184Z-08376ea7-7e29-4c50-bfc9-dda9c2cf4d8f.jsonl`, final `2026-11-01T11-44-21-122Z-7ccb874d-4f3a-4736-8d05-1d4a1cc36316.jsonl`. Exact owner/process/listener exit and named AVD cleanup are observed independently of application assertions. All six new AVDs are retired; current SDK/compiler/APKs and compact success/failure evidence remain. The older policy-blocked TEMP fixture remains untouched. Continue the existing guarded, serial workflow and diagnose a repeated spike before proposing memory-policy or allocator changes.

## November 8 ambient source work

The operator directs source work to continue and permits qualification to wait. Formatting/APK preflights refuse admission at the existing 85% check/build ceiling; neither workload runs. A short host tooling run passes 47 tests with two existing Linux skips under the unit policy. No user application/model/service is stopped and no memory, WSL, page-file, heap or deadline setting is changed.

Full Flutter regression receipt `2026-11-09T01-03-37-089Z-4ce52390-9675-4c34-b0c2-b4a46bef6687.jsonl` records an interrupted run, not a pass. The final triggering interval changes system commit from **84,174,909,440** to **90,692,165,632** bytes, about **6.07 GiB**, while observed test-private allocation falls from **965,812,224** to **959,602,688** bytes. Commit reaches **97.259%** of **93,248,184,320** bytes, with about **15.42 GiB physical RAM** still available. The existing 94% emergency guard terminates the owned workload; the next sample drops system commit substantially. Peak observed test-private allocation is approximately **0.925 GiB**.

These samples do not identify the allocating actor or prove an external cause: process sampling can miss transient/shared/kernel activity or a new child. The evidence does not establish an ambient/client leak or crash. Compact guard/test logs remain private. No emulator/PostgreSQL fixture is created by this interrupted unit run. Current-source compilation/native qualification remains pending under the [ambient authority](ANDROID_AMBIENT_QUALIFICATION.md#implementation-and-verification-checkpoint).

## November 14 ambient review admission

The operator's existing instruction permits source work while qualification waits. Current analysis and focused-test preflights refuse execution; they are neither passing nor interrupted test workloads. [The qualification record](ANDROID_AMBIENT_QUALIFICATION.md#november-14-astra-corrections--unqualified) lists all three receipts and source attribution. The last sample reports **77,024,378,880 committed bytes** against a **79,402,549,248-byte limit**, **97.005%**, leaving **2,378,170,368 bytes** of commit headroom. Available physical RAM is **21,619,290,112 bytes**. Free RAM does not restore the separate commit reserve required by the existing policy.

Only the preflight owner/sampler is observed: three processes totaling **91,303,936 private bytes**. No Flutter test child, Android build/emulator, backend/FFmpeg or PostgreSQL fixture is admitted by this attempt. This small sample does not attribute the rest of system commit or its growth to any specific application, model, kernel component or client leak. No unrelated process is stopped. All memory, emergency, deadline, guest and service settings stay unchanged. Retain compact refusal evidence and resume the serial guarded workflow when admission is available; do not repeatedly launch heavy work into the same refusal.

## November 14 web patch source candidate

The [compatible patch authority](../security/SVELTEKIT_2_SECURITY_PATCH.md#outcome) records two guarded `web-install` preflight refusals: baseline and newly prepared source candidate. Neither admits npm installation. In the latter sample, system commit is **79,324,229,632 / 84,971,442,176 bytes**, **93.354%**, leaving **5,647,212,544 bytes** headroom. Physical availability is **16,028,901,376 bytes**. The unchanged build policy requires twelve GiB headroom and at most 85% commit; physical RAM alone does not satisfy it.

The candidate preflight observes three owner/sampler processes totaling **91,336,704 private bytes**. No npm project installer, compiler, test server/browser, Android emulator, FFmpeg or database fixture is started by that attempt. Changes in total host commit/limit do not establish who allocated memory or changed the host's automatic allocation behavior. No page-file, WSL, heap, service, emergency or workload policy is changed by this work. Metadata-only source comparison is not a hidden installer or a runtime pass. Keep both compact refusals; updated-package verification remains blocked until normal admission is available.

## November 14 web boundary runner

The [runner authority](SVELTEKIT_SECURITY_BOUNDARIES.md#status-and-next-gate) records installation/build preflight refusals separately from three successful infrastructure-unit runs. The final corrected-source unit receipt is `2026-11-14T23-54-29-348Z-275c1639-a311-4360-8437-b6d6ab1fd90e.jsonl`: **30 pass**, zero failures/skips, no memory stop/interruption. Its peak test-private allocation is **214,663,168 bytes** across nine observed owned processes, with **81,586,065,408 / 90,250,170,368 bytes** system commit and minimum physical availability **18,065,608,704 bytes**. That fits the unchanged unit policy. Tiny Node/HTTP/filesystem helper cases run; no Svelte/Vite/compiler package executes.

Final runtime receipt `2026-11-14T23-55-25-273Z-e0c6dd6a-66b9-486f-81ea-8454b458e5f0.jsonl` refuses before the new script starts: **90.829%** commit, **81,973,403,648 / 90,250,170,368 bytes**, **8,276,766,720 bytes** headroom despite **19,280,728,064 bytes** physical availability. Build admission still requires twelve GiB headroom and at most 85%. The new unit preset is not used to compile or bypass this prerequisite. The installer refusal and earlier runtime refusal remain separate records; no incomplete framework attempt is promoted to a pass.

Owned unit processes/listeners, generated helper directories and the temporary PR-rebase index are gone; the active workflow marker is absent. No user app/model/service, page-file, WSL, heap, emergency limit or host configuration is changed. These observations do not attribute shared host allocation or growth to another actor. Retain compact evidence and continue the production matrix only when ordinary admission is available.
