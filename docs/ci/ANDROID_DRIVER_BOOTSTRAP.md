# Android qualification driver bootstrap

## Status and problem

**Scoped qualification complete.** The operator confirmed the focused contract at `18570e8`, October 31, 2026, and the public-API logging refinement below. The modular implementation passes both actual scope journeys and all eight original journeys on the current inputs, with observed cleanup. The [scope-ownership implementation](../design/FLUTTER_SCOPE_CANCELLATION.md) now has its complete current-source native gate. Production attachment/caps and the remaining relay matrix stay separate; no release.

At review, the unchanged original suite reached seven passing journeys, then `flutter drive` lost its VM-service connection before the worker journey began. A previous attempt lost that connection before journey four. Those earlier results remain separately attributed; the passing controlled repeat below supersedes the pending gate.

The latest failure occurs with healthy host reserves: peak test-private allocation is 7.458 GiB and peak system commit is about 65.94/76.92 GiB. App-exit diagnostics show no current-app crash. The evidence does not establish whether ADB forwarding, the VM-service socket or Flutter-tool orchestration causes the disconnect. This proposal improves explicit ownership and diagnostics; it is not a claim that a root cause has already been proven. See [memory attribution](QUALIFICATION_MEMORY_ATTRIBUTION.md).

## Source-backed feasibility

Official URLs were discovered through MCP search. [FlutterDriver.connect](https://api.flutter.dev/flutter/flutter_driver/FlutterDriver/connect.html) accepts `VM_SERVICE_URL`, resumes a paused application and requires an external bound on connection time. The [integration driver](https://api.flutter.dev/flutter/package-integration_test_integration_test_driver/integrationDriver.html) connects, requests results and closes the driver; its test timeout does not cover the preceding connection wait. [ADB documentation](https://android.googlesource.com/platform/packages/modules/adb/%2B/refs/heads/master/docs/user/adb.1.md) supports explicit server/serial selection and non-rebinding forwards.

Inspection of the locked Flutter 3.47.8 SDK confirms:

- `flutter_tools/lib/src/drive/drive_service.dart:265–269` already runs the SDK Dart binary with the driver file and `VM_SERVICE_URL`.
- `flutter_tools/lib/src/commands/drive.dart:241` defaults to paused startup; `android/android_device.dart:678–685` supplies checked mode, entry-point verification and `start-paused` launch options.
- `flutter_driver/lib/src/driver/vmservice_driver.dart:92–239` discovers the isolate, resumes it and subsequently waits for the driver extension. Waiting for that extension before starting the SDK driver would deadlock paused startup.
- The current drive service creates an additional Flutter-tool VM-service client/log reader before launching the driver. That is a confirmed architectural fact, not proof that this extra client causes the failure.
- The resolved host `vm_service` is 15.2.0. Connect/isolate waits and initial connection behavior require the retained outer process deadline.

## Options

| Option | Pros | Cons / recommendation |
| --- | --- | --- |
| Keep current `flutter drive` orchestration and diagnostics | Smallest change; familiar SDK workflow | Repeated unexplained pre-test disconnects keep the gate pending |
| Owned launch plus `flutter drive --use-existing-app` | Removes install/start orchestration while keeping the rest | Retains the extra client/log reader; useful narrower experiment, not a demonstrated fix |
| Owned ADB bootstrap plus the unchanged SDK Dart driver | Explicit PID/URI/forward/log/process ownership; reuses the actual SDK test implementation and locked stack | Adds a qualification service requiring adversarial and real native validation; recommended |
| Change SDK/dependencies or fork the driver | Might use an upstream fix if one is established | No confirmed matching fix; broadens the locked qualification scope; defer |

Recommended stack: current verified APK/SDK/package mapping; small composable Node owners using built-in process/HTTP APIs; existing isolated ADB server/serial; paused Android launch; bounded read-only VM readiness; current SDK Dart integration driver; retained bounded app logs; existing actual backend/media scenarios. No application playback or server protocol changes are proposed.

### Confirmed public-API logging refinement

The locked high-level `integrationDriver` calls `FlutterDriver.connect()` with default raw communication-file logging and exposes no override. The official [public connection API](https://api.flutter.dev/flutter/flutter_driver/FlutterDriver/connect.html) exposes `logCommunicationToFile:false`; the [high-level implementation](https://api.flutter.dev/flutter/package-integration_test_integration_test_driver/integrationDriver.html) confirms the unconditional default. Inspection of the locked SDK agrees. Raw serialized results could otherwise escape the shared output/redaction owner.

The operator explicitly confirmed using the repository's small host wrapper with that public flag. It retains the same SDK connection implementation, three-minute `requestData`, SDK `Response` parsing, result assertions, screenshot callback, driver close and exit behavior. `flutter_driver`, already locked through `integration_test`, receives a direct SDK dev-dependency declaration for that API; its version and source stay unchanged. No SDK file is patched, no extended-driver protocol is substituted and no mutation is replayed. The wrapper gains responsibility for this small result sequence; that is the tradeoff for enforcing bounded, redacted evidence without forking the SDK.

### Owned ADB discovery

A native repeat passes both scope journeys, then loses its original-suite forward during a read-only VM probe before the SDK driver starts. Guest logs show an ADB transport closing and another opening. An unrelated host ADB server is listening on 5037. This identifies an ownership hazard, not a proven attribution of every earlier disconnect.

[Android's ADB documentation](https://developer.android.com/tools/adb) describes default emulator discovery on odd ports 5555–5585; the previous console/ADB pair 5580/5581 overlaps it. The [ADB transport implementation](https://android.googlesource.com/platform/packages/modules/adb/%2B/290ccb5dfe946d4dd39a6b8e584dd11ecd61779e/transport_local.cpp) explicitly supports disabling broad scans with `ADB_LOCAL_TRANSPORT_MAX_PORT` below 5555. [Emulator source](https://android.googlesource.com/platform/external/qemu/%2B/e6aef36e024c3265ff8103f8d2265dd235851ef4/android/qemu-setup.c) uses `ANDROID_ADB_SERVER_PORT` for its host-server notification. The [ADB manual](https://android.googlesource.com/platform/packages/modules/adb/%2B/HEAD/docs/user/adb.1.md) also provides a server-only USB-device selector.

Within the confirmed exact-owner contract, select one free even console port from 5620–5680, preserve the private server's explicit socket/port, disable its broad scan in its child environment and limit its USB selector to the owned emulator serial. Rely on the emulator's private-server registration. No global server is stopped, reconfigured or disconnected. This avoids default discovery collisions; it is not a firewall against another host process explicitly connecting. A lost transport/forward still fails the stage without reconnection or replay. Guest, process, memory and deadline budgets remain unchanged.

## Confirmed contract

1. Replace only host drive orchestration. Reuse the SHA-verified qualification APK and preserve app data across the existing cold/corruption journeys. Validate current Dart, SDK driver sources, locked/generated host package mapping and artifact identities. Add no resolved package, SDK update or SDK source patch; the confirmed public API uses a direct declaration of the existing SDK test dependency.
2. Use only this run's ADB server and exact serial, explicit `-P`, current boot identity and private four-KiB bootstrap. Preserve locked-SDK-equivalent paused/debug launch flags. Resolve a unique current package PID and process-start identity; refuse ambiguous or replaced ownership.
3. Own one fresh loopback forward with `--no-rebind`. Verify exact serial/host/guest mapping. Obtain a VM-service authentication URI only from the current owned process's fresh bounded logs; validate expected literal loopback port and bounded capability path. Keep capabilities in memory/child environment and redact them before persistence. Disable neither authentication nor origin checks.
4. Allow at most thirty seconds for bootstrap/read-only readiness within the existing 240-second outer stage ceiling and shared thirty-minute suite deadline. Readiness may recognize the paused isolate; it must not wait for the integration-test extension or resume application work itself. Revalidate PID, boot and forward ownership at the handoff.
5. Start one SDK Dart host-driver process using the confirmed public-API logging refinement, with current package configuration, existing 1,536-MiB heap cap and three-minute test/result behavior. Preserve its existing initial SDK connection behavior under the outer deadline; add no wrapper driver reconnect/relaunch after failure and no application-command replay. A failed or uncertain attempt stays failed.
6. Own a PID-filtered logcat producer before the SDK driver can resume the app. Retain actual process/stream/file completion and backpressure; share the existing sixteen-MiB stage output budget rather than granting each producer another full budget. Reject oversized/incomplete framing. Preserve both `ANDROID_PHASE background` and `ANDROID_PHASE renew-loss` so genuine Home/resume and renewal-loss actions still occur once.
7. Keep every original eight-stage assertion and the separately attributed two scope journeys. Preserve exact viewer/history/recovery/zero-unsettled SQL assertions, authored frame checks, private namespace/cold restart/corruption behavior and real backend/FFmpeg evidence. Run fresh runtime cases under one shared deadline, one unchanged API-36/2,560-MiB/two-CPU emulator at a time.
8. On success, error, cancellation or timeout, retain and observe driver/logcat child exit, stream/file completion and exact forward cleanup before claiming closure. Unknown cleanup remains a failure. Preserve existing VM/ADB/app cleanup, thirty-second observation and root budgets; do not stop unrelated apps, models, services or ADB servers. Capture bounded failure diagnostics without calling them successful app completion.
9. First qualify parser/identity/refusal, process/stream backpressure, phase attribution, timeout/cancellation and redaction with meaningful host tests and owned loopback fixtures. Then run both real scope journeys and all eight original journeys against current PostgreSQL/HTTP/FFmpeg. No test is weakened or automatically retried to obtain a pass. Apply routine owned artifact cleanup and preserve compact failed/passing evidence.
10. Update current outcomes and phase status only from actual results, commit and push on main without a release. Keep production protocol-two advertisement/attachment/shared caps, relay/output-format expansion, ambient/offline/iOS and other product work separate. Memory guard, WSL/page-file configuration, application budgets and locked dependencies remain unchanged.

The operator confirmed this ownership boundary and its logging refinement before dependent implementation. Ordinary placement and format corrections remained within that contract. The implementation does not change application playback, credentials, owner grants or production rollout.

## Implementation and qualification outcome

Small Node modules own stage accounting/framing, PID/start and VM identity, read-only HTTP completion, retained app logs, SDK/package inputs, bootstrap and ADB discovery policy. `android-stage.mjs` composes them. The existing bounded-command helper accepts optional shared accounting/chunk observers; unchanged callers retain their existing behavior. The repository Dart wrapper uses the SDK's public connection/result APIs with raw communication logging disabled and closes the driver even when result reading/parsing fails. The SDK source is unchanged.

Host package identity covers 393 transitive source/metadata inputs, explicit SDK package roots, the locked VM-service version, package configuration, lockfile and Dart executable. It is verified before each stage and after the suite. Actual Android PID identity accepts only the exact package argument plus NUL padding; a unique PID/start identity, boot ID and forward are checked again before handoff. Readiness accepts the SDK's transient `None` state with either boolean runnable value, polls read-only at 100 ms within the original thirty seconds, and requires `PauseStart` before handoff. Resume and driver-extension registration remain SDK responsibilities. Only a bounded eight-state readiness trace is retained.

Verification:

- **120 Flutter tests pass**, including the three new public-driver result/close/evidence tests. Clean analysis and two-file formatting pass. After the direct SDK declaration and its metadata-only lockfile change, offline resolution and the three affected tests pass again. The first offline resolution correctly invalidates its source receipt because it updates that lockfile classification; the stable repeat passes. No package version changes.
- **44 host checks pass; two existing Linux-only skips.** Actual child-process and loopback fixtures exercise shared overflow, deadlines/cancellation, file completion, split/redacted output, incomplete/invalid frames, phase attribution, refused coding/redirects/bodies, uncertain forward acceptance, process/boot/forward substitution and transitive package-map/source changes. Discovery policy masks inherited broad-scan configuration without modifying its parent environment.
- Both APK builds pass actual package/activity checks. App input digest: `428e1e9842bc1c3d0bb4d3dd2aab78c6dd675325ad7d6d35ffd01d8a5ff7e10d`. Qualification APK: `8b3cbc31f542e9b7309c21522d736e7978588edbc4deb85e587b451ce731ea4a`; production APK: `03c28ef8d7f42bdb1d5dc26c1c1c7d11454c078dfff935afcba442bd31aa76a1`. APK bytes are unchanged from the preceding application checkpoint; host-driver/test/dependency-classification inputs changed.
- Resource **`bd8e217e-d2b7-4eea-a1d1-a6f3380966cb` passes the standalone worker prerequisite, both real provider/scope journeys and all eight original journeys** against actual PostgreSQL/HTTP/FFmpeg. Scope SQL proves four released viewers, four histories and zero unsettled Starts. Original SQL preserves its five-viewer/five-history/zero-unsettled assertions. Authored frame 19 matches the 949–1,355-ms presentation interval; baseline native attachment peaks at 2,522 ms. Genuine Home/resume and renewal-loss phases execute. Cold restart, namespace integrity, corruption refusal/restoration and accepted-response losses pass without replay.
- Worker recovery proves a 531-ms checkpoint at sequence three, 18 successful negative reads then positive evidence, one same-viewer Recover, preserved configuration, 47 stalled threads and physical exit at 39,930 ms. No wrapper reconnection or application-command replay obtains the pass.
- Every stage, app/private-state, forward/reverse, VM/ADB, backend/root and fixture-container cleanup completes. Shared stage output ranges from **307,012 to 1,221,431 bytes**, below sixteen MiB. Existing three-minute results, 240-second stages, shared thirty-minute native suite, process/root/memory/guest/encoder limits remain unchanged. The old raw SDK communication file's timestamp remains at the preceding checkpoint; this runner does not append to it.

Exact passing input evidence: backend/qualification aggregate `0ac871dc07e5874f1af87dc373565c239253d1afe152054078f9466e85db8a00`; Android tooling `967ffe4e9360b01ae088d9562d73add693bdb593bc8f6298ddb199b4fb10a2f0`; host driver graph/executable `5244e32dcfd9ff3fe6b2e45f6ac066ed8ae39588355001495bb1401fc1f46ac9`. Documentation-only follow-ups do not alter these runtime inputs.

Guard receipts under ignored `.cache/testing-memory/`:

| Check | Receipt |
| --- | --- |
| Full 120 Flutter tests | `2026-11-01T02-03-18-552Z-ef6d471f-b5a4-4f32-adc0-ba951c8a8858.jsonl` |
| Clean analysis | `2026-11-01T02-05-25-608Z-59b8e409-62e2-4d39-ab9a-552369acd314.jsonl` |
| Current three driver tests | `2026-11-01T02-08-30-571Z-1b175517-7d75-495e-a149-bec5902d9b2c.jsonl` |
| Final host checks | `2026-11-01T02-33-19-432Z-e8aff514-3f1b-41e9-bc3a-9895669073ac.jsonl` |
| Complete real native repeat | `2026-11-01T02-33-26-484Z-2d99da46-f61d-4db6-9b20-2b5f01dc1707.jsonl` |

### Corrected attempts

Every failure remains a failure, with compact diagnostics retained. These are fresh manual runs after source corrections, not automatic retries or resumed application commands. All below complete owned cleanup and record no memory stop.

| Resource | Result / correction |
| --- | --- |
| `d7993757-b675-46cd-a864-b32aa80e3af7` | First shell refusal before app-log/driver ownership. Immediate post-launch PID lookup has a race; bounded empty-PID observation is added, without relaunch. The exact failed shell subcommand was not captured in that first attempt. |
| `330efe0f-c435-4d54-a611-5336db1bbf5d` | Exact command-vector check refuses Android's NUL padding; accept that padding while refusing any extra nonzero argument. |
| `c8d06487-2d30-43d0-970d-a7dc32f81fe0` | First readiness check refuses the not-yet-paused state. Add the bounded SDK-style `None` wait and compact state trace. |
| `6f70b914-d79a-4284-8e55-6526ca5a57d8` | Trace proves `runnable:true` with pause kind `None`; do not use that bit to reject the SDK's transient state. Require `PauseStart` at handoff. |
| `5794425a-23e6-4f7d-a489-52fe1b510230` | Both scope journeys/SQL pass; original baseline loses its forward during read-only probing, before SDK-driver startup. Preserve failure and cleanup evidence; isolate discovery from the default scan range. |

The last observation supports a transport-ownership problem independent of the SDK driver. A separate host server and default scanning are a credible collision path; the exact actor behind every historical disconnect is not proven. The successful isolated repeat is qualification evidence, not a complete causal reconstruction.

### Memory, storage and next item

The successful repeat records **6.800 GiB peak test-private allocation**, **68.665/76.920 GiB peak system commit** and **20.188 GiB minimum available physical memory**, with no guard stop. After owned cleanup, no guard or test-native process remains; commit is 61.204/76.920 GiB and available physical memory 27.435 GiB. Shared host activity prevents attributing the entire system change to the test. [Memory attribution](QUALIFICATION_MEMORY_ATTRIBUTION.md) retains the original investigation and new measurements.

Native manager cleanup retires all **eight** AVDs from corrected/passing attempts: **28.037 GiB logical**, **27.562 GiB observed host recovery**, final C: free **971.518 GiB**. Current compiler/SDK/APK inputs and compact evidence remain. The unrelated default ADB server keeps the same PID; no other service is restarted. The earlier policy-blocked private TEMP fixture remains retained. [Storage outcome](../operations/DEVELOPMENT_STORAGE.md) owns the cleanup record.

MCP still identifies only [Dependabot PR #6](https://github.com/cloudbyday90/Duskcue/pull/6). Its exact dependency patch already received the documented local failed trial and stale-branch rebase in this session; unchanged inputs do not justify another install or chasing documentation commits with repeated rebases. No merge or dependency adoption. See [upgrade review](SVELTEKIT_3_UPGRADE_REVIEW.md).

**Next:** finish the Android relay adversarial/backpressure and remaining output-format matrix under the [relay authority](../security/FLUTTER_HLS_RELAY.md), before native ambient. This checkpoint does not complete that matrix or enable normal production protocol-two attachment/shared caps, provider VOD, recordings, offline or iOS playback.
