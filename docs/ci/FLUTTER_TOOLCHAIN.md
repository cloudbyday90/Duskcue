# Flutter and Dart toolchain

## Selected release

On October 28, 2026 the operator requested updating Dart and Flutter during the Android interactive-player implementation. This explicitly supersedes that increment's original Flutter 3.44.4/Dart 3.12.2 tooling freeze. Application dependencies remain locked apart from SDK-required transitive changes and the previously approved direct uuid declaration.

The official Windows stable manifest selects **Flutter 3.47.8 / Dart 3.13.7**, released October 23, 2026, framework revision `737ba8f885dc774b53b442759999a312c9461e4e`. The Windows x64 archive SHA-256 is `11e19ea3409808fe3b3765ea8149640c299e58cce6eda7c9f38fc1b4a193eb6e`. The reviewed tuple is committed in [flutter-toolchain.json](../../scripts/testing-memory/flutter-toolchain.json).

Sources were discovered through MCP search, official Flutter SDK publisher code and the published release manifest: [Dart installation](https://dart.dev/get-dart), [Flutter upgrade guidance](https://docs.flutter.dev/install/upgrade), [Flutter archive](https://docs.flutter.dev/install/archive), and [Windows release metadata](https://storage.googleapis.com/flutter_infra_release/releases/releases_windows.json). Dart is bundled with Flutter; substituting an unrelated standalone Dart executable would not update Flutter's compiler/engine toolchain.

## Recommendation and tradeoffs

| Choice | Benefits | Cost | Decision |
| --- | --- | --- | --- |
| Current stable Flutter and its bundled Dart | Matched framework/compiler/engine; current supported stable toolchain | Requires fresh package resolution, tests and APK/native qualification | Selected by the operator |
| Standalone Dart alongside the previous Flutter | Useful for independent Dart command-line projects | Does not update the Dart used by this Flutter application | Not the application toolchain |
| Beta/main Flutter | Earlier features | Additional compatibility risk outside this qualification | Deferred |

The recommendation stack is Flutter 3.47.8, Dart 3.13.7, existing Riverpod/Dio/secure-storage/video_player components, the owned bearer-free relay and the qualified Rust/PostgreSQL/FFmpeg backend. Keep Android production attachment disabled until actual native qualification succeeds. SDK availability or a passing widget test does not establish native decoder behavior.

## Installation and ownership

`node scripts/testing-memory/run.mjs flutter-sdk-update` streams the reviewed official archive with finite size/deadline limits, verifies its checksum and selects a separate versioned SDK installation. It checks extracted framework/revision metadata and the actual Dart executable. The existing SDK remains available to other processes; no IDE process, unrelated service, OS feature or global device is stopped.

Project-local Android properties and the ignored VS Code SDK selection point to the new installation. No machine-specific SDK path or downloaded archive belongs in Git. Future terminals/IDE SDK selection may require their usual reload; managed commands use the explicit project SDK directly. The actual Dart executable SHA-256 is `dbc2da722f24a786daa23c053ef61f04e52d6675c43f572bf36f4289971e6d46`.

The managed runner enforces the selected framework/Dart/revision tuple and records executable/source hashes. Completed APK reuse requires matching inputs, tool identities, APK bytes and dependency-graph evidence; runtime tests remain fresh. Generated Android properties/registrants are derived inputs, while source, lockfile and analyzer configuration are inventoried. Existing memory/deadline limits remain in force.

CI uses the same reviewed tuple through the modular release selector and verifies the official per-platform archive checksum before extraction. Three selector tests pass, and actual official Linux x64/macOS x64/macOS arm64 manifests select 3.47.8 successfully. A second managed SDK-update execution reused the selected installation successfully, confirming idempotent project selection. These tooling checks do not claim execution of GitHub-hosted packaging jobs.

## Dependency and validation outcome

Offline resolution changed only these SDK-required transitive versions: intl 0.20.3, matcher 0.12.20, meta 1.19.0, test 1.31.1, test_api 0.7.12, test_core 0.6.18 and vector_math 2.4.3. Direct application versions did not change. Flutter added analyzer exclusions for generated build/platform directories; application, unit and integration-test Dart remain analyzed.

The first resolution was recorded as changed-input evidence, then rerun against the reviewed lockfile. On the new SDK, static analysis passes and all **52 Flutter tests pass**. A new large-text/RTL cleanup test required waiting for scroll layout before tapping; status now stays visible while controls scroll. Its corrected focused run and the complete suite both pass. This is widget/HTTP/socket evidence, not spoken screen-reader or Android-device qualification.

Managed receipts under ignored `.cache/testing-memory/`:

| Check | Receipt |
| --- | --- |
| SDK installation/checksum/executable | `2026-10-28T20-44-40-775Z-9a8bf746-214e-43e5-835c-64d35437f19f.jsonl` |
| Stable offline resolution | `2026-10-28T20-49-35-372Z-a7e0e094-b946-4705-9916-39061182d961.jsonl` |
| Analysis, no diagnostics | `2026-10-28T21-04-06-188Z-d43494e5-1e1d-4fc4-b279-567b126bd496.jsonl` |
| Complete 52-test pass | `2026-10-28T21-04-44-123Z-fcdd5152-2a58-4c08-ad73-4021f79023e7.jsonl` |

Fresh qualification and normal Android debug APKs now build, with package/launcher identities verified; see [Android evidence](ANDROID_PLAYBACK_QUALIFICATION.md). WHPX enablement and restart were separately authorized and completed; the actual emulator now confirms WHPX is usable. Stable emulator 37.2.12/API 36 r2 are installed. Its first boot attempt increased the agreed guest budget from 2,048 to 2,560 MiB and was stopped; that adjustment is now explicitly confirmed, with all other limits unchanged; actual native qualification remains required. Fresh post-reboot analysis, 52 Flutter tests and qualification APK pass; the backend fixture compiles. See the linked Android evidence for exact limits, corrected attempts and remaining gates. No release is created.

The subsequently approved 2,560-MiB emulator now boots and shuts down, and on-device APK SHA-256 matches the prior verified artifact. Actual app first use exposed a secure-storage 10.3.1 defect. At `73a3798`, candidate 10.3.4 source was downloaded with the managed cache-only action for review; dependency selection was unchanged pending the [minimal patch exception](../security/FLUTTER_JOURNAL_STORAGE.md). Current analysis and 53 Flutter tests pass. Native storage/playback and rebuilt-APK acceptance remain required; Flutter/Dart and the other locked packages are unchanged.


October 29: the operator approved exactly `flutter_secure_storage` 10.3.1 → 10.3.4. Full lock-map comparison confirms only that package version/archive checksum changed; all SDK constraints and other dependency entries remain identical. The existing disabled reset/migration controls are preserved. Fresh qualification APK/native storage, cold restart, corruption refusal, namespace isolation and decoded playback/paused-seek cleanup pass, as attributed in [Android evidence](ANDROID_PLAYBACK_QUALIFICATION.md#approved-storage-patch-and-decoded-playback--october-29). Broader native gates remain pending; no 11.x migration or Android build-stack upgrade is included.


Final October 29 verification: 56 Flutter tests, clean analysis, 24 host checks (two Linux-only skips), both debug APK flavors and the six-stage real Android/backend journey pass. The app lockfile still changes only the approved secure-storage patch from the prior toolchain baseline. The explicitly approved thirty-second native attachment observer is separate from SDK/dependency selection; control/storage/relay/lease/root limits remain unchanged. See [final attribution and remaining native gates](ANDROID_PLAYBACK_QUALIFICATION.md#final-approved-checkpoint).
