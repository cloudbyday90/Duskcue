# Android qualification driver bootstrap

## Status and problem

Proposed focused contract, October 31, 2026. No replacement runner is implemented. The [scope-ownership implementation](../design/FLUTTER_SCOPE_CANCELLATION.md) is implemented, has 117 passing Flutter tests and passing real provider/scope journeys with exact four-viewer/four-history/zero-unsettled SQL evidence. Its current-source overall native gate remains pending: the unchanged original suite reaches seven passing journeys, then `flutter drive` loses its VM-service connection before the worker journey begins. A previous attempt loses that connection before journey four. The earlier original-suite pass remains separately attributed.

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

## Complete proposed contract

1. Replace only host drive orchestration. Reuse the SHA-verified qualification APK and preserve app data across the existing cold/corruption journeys. Validate current Dart, SDK driver sources, locked/generated host package mapping and artifact identities. Add no package, SDK update or SDK source patch.
2. Use only this run's ADB server and exact serial, explicit `-P`, current boot identity and private four-KiB bootstrap. Preserve locked-SDK-equivalent paused/debug launch flags. Resolve a unique current package PID and process-start identity; refuse ambiguous or replaced ownership.
3. Own one fresh loopback forward with `--no-rebind`. Verify exact serial/host/guest mapping. Obtain a VM-service authentication URI only from the current owned process's fresh bounded logs; validate expected literal loopback port and bounded capability path. Keep capabilities in memory/child environment and redact them before persistence. Disable neither authentication nor origin checks.
4. Allow at most thirty seconds for bootstrap/read-only readiness within the existing 240-second outer stage ceiling and shared thirty-minute suite deadline. Readiness may recognize the paused isolate; it must not wait for the integration-test extension or resume application work itself. Revalidate PID, boot and forward ownership at the handoff.
5. Start one unchanged SDK Dart integration-driver process, with current package configuration, existing 1,536-MiB heap cap and three-minute test/result behavior. Preserve its existing initial SDK connection behavior under the outer deadline; add no wrapper driver reconnect/relaunch after failure and no application-command replay. A failed or uncertain attempt stays failed.
6. Own a PID-filtered logcat producer before the SDK driver can resume the app. Retain actual process/stream/file completion and backpressure; share the existing sixteen-MiB stage output budget rather than granting each producer another full budget. Reject oversized/incomplete framing. Preserve both `ANDROID_PHASE background` and `ANDROID_PHASE renew-loss` so genuine Home/resume and renewal-loss actions still occur once.
7. Keep every original eight-stage assertion and the separately attributed two scope journeys. Preserve exact viewer/history/recovery/zero-unsettled SQL assertions, authored frame checks, private namespace/cold restart/corruption behavior and real backend/FFmpeg evidence. Run fresh runtime cases under one shared deadline, one unchanged API-36/2,560-MiB/two-CPU emulator at a time.
8. On success, error, cancellation or timeout, retain and observe driver/logcat child exit, stream/file completion and exact forward cleanup before claiming closure. Unknown cleanup remains a failure. Preserve existing VM/ADB/app cleanup, thirty-second observation and root budgets; do not stop unrelated apps, models, services or ADB servers. Capture bounded failure diagnostics without calling them successful app completion.
9. First qualify parser/identity/refusal, process/stream backpressure, phase attribution, timeout/cancellation and redaction with meaningful host tests and owned loopback fixtures. Then run both real scope journeys and all eight original journeys against current PostgreSQL/HTTP/FFmpeg. No test is weakened or automatically retried to obtain a pass. Apply routine owned artifact cleanup and preserve compact failed/passing evidence.
10. Update current outcomes and phase status only from actual results, commit and push on main without a release. Keep production protocol-two advertisement/attachment/shared caps, relay/output-format expansion, ambient/offline/iOS and other product work separate. Memory guard, WSL/page-file configuration, application budgets and locked dependencies remain unchanged.

The repository's design review requires shared-understanding confirmation before implementing this new qualification ownership boundary. Once this complete contract is confirmed, ordinary module placement and implementation choices need no further interview unless new evidence changes its behavior or limits.
