# Android interactive playback qualification

## Status and authority

Concrete qualification proposal, October 28, 2026, for the seven confirmed choices in [Flutter migration](../design/VOD_FLUTTER_CLIENT_MIGRATION.md) and its [owned HLS relay](../security/FLUTTER_HLS_RELAY.md). Final concrete-contract confirmation is pending. This document claims no newly installed SDK component, application build, native test result or OS change.

Qualification covers the existing Flutter interactive Android player and bounded video-encoded HLS lane. Native ambient remains independent, offline remains separate and iOS scaffold/runtime, typed decoder probes, direct/copied video and broader devices/formats remain deferred. Normal production routes/owner/advertisement/caps remain off.

## Verified starting environment

Read-only inspection found Flutter 3.44.4 stable/Dart 3.12.2, JDK 17.0.19, existing Android API/build-tools 36, NDK 28.2.13676358 and platform-tools 37.0.0. The project SDK and environment SDK variables disagree, so every helper must capture/use explicit verified executable/SDK paths. No emulator executable, installed system image or attached device was present; the AVD directory was empty.

Windows Hypervisor Platform (`HypervisorPlatform`, used for WHPX) is disabled. Hyper-V, Virtual Machine Platform and WSL are enabled; HypervisorPresent/firmware virtualization do not prove WHPX readiness. Enabling the missing feature changes the OS and may require restarting Windows. Prepare the application/tests/APK and exact prerequisite handoff first; request separate operator action before that OS change/restart. No silent feature change, reboot, service interruption or unsupported acceleration claim follows from approval of this implementation proposal.

Existing accepted SDK licenses match cached stable emulator 37.1.11 and API 36 image terms. Cached repository metadata is dated August 19, 2026. Installation must retrieve official package metadata, verify actual revision/archive/checksum/accepted-license identity and stop rather than accept changed terms or silently substitute another release. Reusing existing accepted terms does not authorize accepting new terms. No application dependency upgrade is required for emulator provisioning.

The iOS project has no native target or complete build scaffold; its CI skip is not compilation evidence. No Windows-host test can qualify iOS native playback.

Official prerequisite/tooling basis: [Android emulator command line](https://developer.android.com/studio/run/emulator-commandline) and [acceleration](https://developer.android.com/studio/run/emulator-acceleration), discovered through MCP search and followed official links. The documented CPU software-emulation mode is unsupported and very slow; it does not silently replace the supported emulator gate. Supported software graphics rendering is a distinct setting and must be recorded. Existing accepted-license matching and disabled feature state come from local read-only inspection, not an assumption from public guidance.

## Source, tool and application isolation

Create modular managed helpers for SDK discovery/acquisition, bounded build, AVD/ADB ownership, fixture transport, native scenarios, evidence and cleanup. Reuse existing memory sampling/guarded PostgreSQL/FFmpeg/source-inventory primitives. Heavy phases remain serial through the managed runner and use its unchanged admission/emergency policies. Do not weaken Linux/compiler or root/pool limits to accommodate mobile tooling.

Keep pubspec/lock and native build versions fixed, except the direct declaration of already locked uuid 4.5.3. Capture the actual resolved Media3/plugin/AAR graph: current app and plugin declarations differ, so a guessed version is insufficient. Record Flutter/Dart/JDK/Gradle/AGP/Kotlin/SDK/image versions, artifact hashes, application/package/AVD/device identities, source hashes before/after execution and all corrected attempts. No stale APK or cached test result qualifies current source. Build caches may reuse matching inputs; runtime tests execute fresh.

Use a qualification flavor/package distinct from `com.duskcue.mobile`, and a task-owned AVD/data directory/ports. Existing user devices, AVDs, emulator/adb processes and global Gradle daemons are not test resources. Use an owned ADB server/socket and exact owned emulator serial for installation, reverse forwarding, force-stop, screenshots and cleanup. A conflict or uncertain identity refuses execution; do not fall back to the default connected device or restart a global ADB daemon. Do not operate a physical device without separate authorization.

Pass ephemeral fixture credentials at runtime through an owned app-private test bootstrap, not source constants, public URLs, package metadata or committed files. Keep fixture material under ignored private paths, redact it before log persistence and clear exact app credential/journal/bootstrap state at verified cleanup. The normal application contains no production bypass endpoint/trust configuration. The controlled backend mounts only existing qualified routes/owner and uses disposable real accounts, sessions, PostgreSQL and authored media. No supplied Dispatcharr feed or identifying value is required.

## Initial local resource budgets

| Phase/resource | Limit and ownership |
|---|---|
| Managed admission | Existing check/build/native thresholds and active emergency reserves; no parallel heavy workloads or guard bypass. Account for owned child processes and VM/runtime memory. |
| Flutter/Dart | One tool/test owner at a time; use a verified 1,536-MiB Dart old-generation cap where the actual CLI supports it, plus managed measured process limits. Do not claim an unverified environment variable as enforcement. |
| Gradle/Kotlin | Maximum two workers; task-owned single-use build JVM with at most 3,072-MiB heap, 1,024-MiB metaspace and 256-MiB code cache; in-process Kotlin compilation, no surviving shared daemon. Override the current large project defaults only in this owned qualification/build path. |
| Emulator | One instance, two virtual CPUs and 2,048-MiB guest RAM; isolated cold AVD, headless execution, no saved snapshots or account restoration. Record actual acceleration/rendering mode. VM/native/graphics overhead remains measured by the host guard, not called part of guest RAM. |
| Phase deadlines | SDK download/acquisition at most fifteen minutes; application build at most thirty minutes; emulator boot at most ten minutes; native scenario at most three minutes; complete native suite at most thirty minutes. No deadline extension disguised as a retry; matching completed build checkpoints may be reused. |
| Evidence | Bounded metadata-only journals and logs, at most sixteen MiB per tool/scenario log and a finite capture list; redact bearer/bootstrap/relay capabilities before persistence. Do not collect arbitrary host/device logs or packet captures. |
| Cleanup | Stop new work and join exact owned app/ADB/emulator/tool/HTTP/process resources. A thirty-second observation deadline marks unknown cleanup as failed and retained; it never licenses global termination or a success claim. |

These are initial application/tool/VM budgets, not a proof of total process RSS or disk byte usage. Verified CLI flags, child ownership, sampling and actual results must agree. If a tool cannot honor the selected controls or acceleration/package/license prerequisites are unavailable, retain the gate and report it; do not downgrade to source-only native qualification or alter OS/services implicitly.

## Required tests

1. **Pure/state/HTTP/socket behavior:** typed strict schemas, exact origin/session/profile/credential/transport/context capture, server-time UUIDv7/CSPRNG, stable immutable intent and no duplicate/revival, complete acknowledgement/no downgrade, returned resource/ETag/revision rejection, stale-response/401 fencing, one in-flight lease/checkpoint/status, output-epoch sequencing, positive worker recovery only and explicit correction. Test before/after-dispatch/response-loss paths; do not mirror implementation branches as assertions.
2. **Journal/native ownership:** known write/read-back before mutation, serialized late storage operations, unreadable/corrupt/missing state, scope invalidation despite storage failure, controlled process death and cold restart without automatic Start/URL reattachment, profile-only cleanup and stronger-boundary clearing, native initialization/disposal failure and quarantine. A read-back does not establish physical persistence under arbitrary power loss.
3. **Relay adversarial cases:** all admission/parser/URI/manifest/redirect/coding/byte/range/backpressure/cancellation bounds from the [relay authority](../security/FLUTTER_HLS_RELAY.md). Include actual loopback socket tests and a redirect canary; no bearer may reach the native player or an unapproved destination.
4. **Widget/navigation/accessibility:** preparing/changing/uncertain/expired/withdrawing/cleanup states, explicit Check status/Play/correction/Stop, Android Back during Start or source replacement, failed Close remaining truthful, focus/semantics, large text and RTL/narrow layout. Preserve legacy/offline and ambient bridge regressions. Semantics tests are not spoken screen-reader qualification.
5. **Actual native media and backend:** fresh isolated APK/AVD against real PostgreSQL/HTTP/FFmpeg. Native player consumes local bearer-free HLS; relay authenticates exact scoped upstream manifest/init/segment requests. Require decoded authored-frame capture plus advancing native presentation/position, actual output codec/dimensions/timing, paused seek and track/quality replacement, one viewer/history, correct absolute source checkpoints and positive server worker-recovery adoption. Metadata alone does not prove decoded playback.
6. **Lost outcomes and lifecycle:** lost accepted Start/change/renewal/withdrawal observations; explicit reconciliation without mutation replay; cold restart cleanup; pause/background/resume with mandatory explicit Play; real missed lease after suspension/process stop; scope changes while media/control/storage is pending; old-output request rejection and actual socket/controller cleanup. An expected failed attempt remains failed evidence until corrected and rerun with a fresh verified identity.
7. **Final attribution and hygiene:** SQL verifies exact viewer/history/revisions, retained receipts, no unresolved preparation/resource release claims and actual cleanup. Verify exact credential/bootstrap clearing, owned listeners gone and no owned adb/emulator/FFmpeg/test-server process remaining. Run Flutter analysis/tests, isolated APK plus supported normal packaging, relevant backend/workflow regressions, format/copyright/link/privacy checks. Record existing unrelated diagnostics separately; no blanket suppression.

Authored fixture images and known codec metadata can establish this emulator lane, not arbitrary physical devices, audible fidelity, HDR, other codecs or iOS. Capture limitations separately from passing cases. Service disruption is unnecessary unless an actual measured gate later warrants a separately authorized handoff.

## Outcome and next gate

This is the concrete design/acceptance document. Implementation, SDK acquisition, APK/native execution and all result receipts are pending final shared-understanding confirmation. After code/tests/APK are ready, request the operator-controlled WHPX enable/restart handoff; actual native qualification remains required before marking this increment complete. Keep all source changes on main and commit/push reviewed task checkpoints without creating a release. Record final identities, results, failures and limitations here after execution.
