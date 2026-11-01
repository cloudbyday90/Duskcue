# Qualification memory attribution

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

The platform implementation's immediate unresolved item is the native driver's ADB/VM-service bootstrap, not a demonstrated server/client memory leak. Qualify that boundary before repeating the overall gate. The existing relay adversarial/backpressure/output-format milestone remains next after the scope/native gate. No OS, WSL, Docker or application-memory setting changes are made by this investigation.
