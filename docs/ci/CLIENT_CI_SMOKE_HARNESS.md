# Client CI Smoke Harness

## Overview

Phase 16d Task 12 defines the reusable client CI and smoke harness that downstream desktop, mobile, TV, and console phases must consume before claiming verification complete.

The machine-readable fixture pack starts at [manifest.json](../api/fixtures/client-ci/v1/manifest.json). The executable harness is [client-smoke-harness.mjs](../../scripts/client-smoke-harness.mjs), and the drift gate is [verify-client-ci-smoke.mjs](../../scripts/verify-client-ci-smoke.mjs).

## Harness Modes

Use the plan mode for normal pull-request and local drift checks:

```bash
node scripts/client-smoke-harness.mjs --plan
node scripts/verify-client-ci-smoke.mjs
```

Use the run mode for release-gate or maintainer-triggered evidence:

```bash
node scripts/client-smoke-harness.mjs --run
```

The run mode creates deterministic representative media under `media/`, starts `docker compose up -d` with `DUSKCUE_HOST_BIND=127.0.0.1` and `DUSKCUE_PORT=48027`, waits for `/health/ready`, probes `/health/live` and `/api/v1/events`, runs the shared Phase 16d contract/conformance verifiers, and tears the compose deployment down unless `--keep` is passed.

## CI Lanes

[client-ci-smoke.yml](../../.github/workflows/client-ci-smoke.yml) provides the shared lanes:

| Job | Default trigger | Purpose |
|---|---|---|
| `shared_contract_validation` | PR and `main` | Route manifest, helper, adapter, and binding-target drift |
| `fixture_drift` | PR and `main` | Client, playback, auth, TV, accessibility, design, diagnostics, device-lab, release, and CI fixture drift |
| `binding_generation_readiness` | PR and `main` | TypeScript/Tauri, Dart/Flutter, Kotlin Android/Fire TV, and Swift iOS/tvOS target coverage |
| `tv_console_fixture_smoke` | PR and `main` | TV/deep-link, TV-surface, and device-lab baseline |
| `android_tv_conformance` | PR and `main` when Android TV inputs change | Android TV contract/conformance, release-readiness, and NVIDIA SHIELD evidence-harness verification, Kotlin unit tests, lint, debug APK, and debug evidence artifact |
| `android_tv_emulator_smoke` | `workflow_dispatch` with `run_android_tv_emulator_smoke=true` | API 36 Android TV AVD installation, Leanback launcher, and custom deep-link handoff smoke |
| `docker_smoke_plan` | PR and `main` | Cheap validation that the Docker smoke harness still has the expected steps |
| `docker_smoke_run_manual` | `workflow_dispatch` with `run_docker_smoke=true` | Real Docker `:48027` deployment smoke |
| `desktop_tauri_smoke` | `workflow_dispatch` with `run_platform_smoke=true` | Tauri/web build smoke when maintainers request heavier evidence |
| `mobile_flutter_smoke` | `workflow_dispatch` with `run_platform_smoke=true` | Flutter analyze, test, and Android debug build smoke |

The workflow uses `contents: read` by default and keeps publish/signing/provenance permissions out of the smoke workflow. Release artifact jobs that emit publishable packages must follow the release-readiness placeholders for SBOM and provenance evidence.

## Downstream Consumption

Phases 17-23 must run or explicitly cite:

```bash
node scripts/client-smoke-harness.mjs --plan
node scripts/verify-client-ci-smoke.mjs
node scripts/verify-client-contracts.mjs
node scripts/verify-client-fixtures.mjs
node scripts/verify-client-bindings.mjs
```

Platform phases must then add their platform-specific build, emulator, simulator, or hardware checks. Long-running playback, HDR, passthrough audio, remote-control, store review, signing, wake/resume, and physical-device checks remain manual or release-gate checks when GitHub-hosted CI cannot run them truthfully.

### Android TV Consumption

Phase 17 adds an automatic `android_tv_conformance` job to this workflow. It is deliberately a debug-build and contract lane: its uploaded APK, lint report, and unit-test output are troubleshooting artifacts rather than publishable release evidence. The job consumes the normal shared checks, the Android TV / Google TV release-readiness verifier, the NVIDIA SHIELD fixture/capability-capture verifier, and then runs the native Android TV Gradle test/lint/assembly gate. The static SHIELD verifier validates the physical evidence contract and never converts CI into a hardware, AVR, HDR, Play, or Watch Next visibility claim.

Maintainers can request `android_tv_emulator_smoke` from `workflow_dispatch`. It creates an Android TV API 36 `tv_1080p` AVD, installs the debug APK, verifies the Leanback feature/launcher, and starts a valid-shape custom playback URI. The portable `node scripts/android-tv-emulator-smoke.mjs --apk clients/tv/android/app/build/outputs/apk/debug/app-debug.apk` command may also be run against one already-booted local Android TV AVD. This is installation/intent evidence, not playback, Watch Next, hardware remote, HDR/audio, accessibility, document-picker, or release readiness evidence.

## Research Basis

- GitHub Actions workflow syntax supports event filters, manual `workflow_dispatch` inputs, job dependencies, job conditions, and explicit permissions.
- Docker Compose supports `up -d` for detached deployments and healthcheck-driven readiness patterns.
- Flutter documents integration tests and deployment build flows for Android and iOS, so the shared harness keeps Flutter unit/build smoke separate from hardware release gates.
- Tauri documents GitHub Actions distribution pipelines, so the desktop smoke lane validates the shared web/Tauri build path without implying signed release readiness.
- GitHub artifact attestations and SBOM guidance are release concerns; the smoke harness records placeholders and keeps elevated attestation permissions out of PR workflows.

## Sources

- GitHub Actions workflow syntax: https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax
- GitHub Actions job variations: https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/run-job-variations
- GitHub artifact attestations: https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations
- Docker Compose startup order: https://docs.docker.com/compose/how-tos/startup-order/
- Docker Compose up: https://docs.docker.com/reference/cli/docker/compose/up/
- Flutter integration tests: https://docs.flutter.dev/testing/integration-tests
- Flutter Android deployment: https://docs.flutter.dev/deployment/android
- Flutter iOS deployment: https://docs.flutter.dev/deployment/ios
- Tauri GitHub distribution pipeline: https://v2.tauri.app/distribute/pipelines/github/
