# Flutter playback journal storage qualification

## Contract and current outcome

The [Flutter migration](../design/VOD_FLUTTER_CLIENT_MIGRATION.md) requires one app-scoped playback reconciliation journal, at most 4 KiB of serialized command/scope identifiers. It contains no credentials, stream URLs, media metadata or command bodies. Android storage uses the dedicated `duskcue.vod.journal.v1` namespace and `duskcue_vod_journal_v1` key prefix. Known write/read-back precedes Start; pending platform operations retain ownership. Missing, corrupt or unreadable data must not silently authorize duplicate playback.

`resetOnError`, automatic algorithm migration and migration backup are currently disabled for this journal. In particular, storage failure cannot clear the record as an automatic recovery action. A replacement session or local journal deletion is not proof of server resource release. Read-back does not establish durability under arbitrary device power loss.

Actual API 36 testing on October 28, 2026 reached the installed Flutter application with the verified APK bytes, but locked `flutter_secure_storage` **10.3.1** reported an algorithm change on first access to a fresh namespace. With migration and reset disabled, it refused initialization. Playback did not initialize, and no decoded-media result is claimed. Unit mocks had not exercised this Android implementation defect.

## Publisher research and minimal proposed correction

The official [10.3.4 changelog](https://pub.dev/packages/flutter_secure_storage/versions/10.3.4/changelog), discovered through MCP search, identifies the fresh-install migration fix in 10.3.2, corrected saved-algorithm detection in 10.3.3, and additional namespace/key-preservation fixes through 10.3.4. The [package documentation](https://pub.dev/packages/flutter_secure_storage) explains its migration and reset controls.

Managed `flutter pub cache add flutter_secure_storage --version 10.3.4` downloaded source for inspection only. The application still selects 10.3.1; neither pubspec nor lockfile was changed. The candidate's cached publisher archive SHA-256 is `fe638107c5f69119156ada2db5a57734385fac3f64430bd7252a00d3ead2ca4b`. Its source handles a genuinely empty store before deciding that algorithm migration is needed. The required Dart/Flutter minima and federated-package constraints remain compatible with the current stack.

The proposed exception to the confirmed locked-stack contract is deliberately limited: pin this existing direct dependency to **10.3.4**, resolve and verify only its expected lock entry/checksum, retain other locked packages, keep the existing journal namespace and disabled reset/migration controls, then rebuild and qualify the current APK. No 11.x major migration or cipher downgrade is proposed. Operator confirmation of that exception is pending.

| Choice | Benefit | Cost / limitation |
| --- | --- | --- |
| Keep 10.3.1 and the current strict controls | Preserves the original lockfile | Reproduced fresh-store refusal prevents first-use playback qualification |
| Pin and qualify 10.3.4 | Addresses the publisher-identified initialization and namespace/key defects without changing the journal's failure policy | Changes Android vendor code and requires fresh native storage and playback qualification |

Recommendation: the compatible 10.3.4 patch, followed by all acceptance gates below. Do not hide the defect with an automatic retry or enable reset-on-error to make the test pass.

## Required native acceptance

1. Fresh journal initialization, bounded write/read-back before admission and one real server Start. Verify the selected cipher and namespace configuration remains unchanged.
2. A known journal survives actual application process death/restart. The returning screen offers status/cleanup only, creates no second Start and does not attach old media automatically.
3. Corrupt ciphertext, unavailable keys or failed storage operations refuse new work without deleting the evidence or treating it as an empty journal. Confirm server ownership separately.
4. Journal read/write/delete and restart preserve a distinct default-store sentinel and another isolated namespace. No fixture bearer is persisted as a sentinel or copied into evidence.
5. Observe pending native/storage completion, exact bootstrap/journal clearing and actual app/process/listener cleanup. Run the remaining real PostgreSQL/HTTP/FFmpeg playback, reconfiguration, renewal, recovery and lost-outcome scenarios from [Android qualification](../ci/ANDROID_PLAYBACK_QUALIFICATION.md).

The native test now checks the authored grayscale patches and encoded frame ID against observed presentation time, rather than only detecting a non-black picture. Its pure pixel-verifier tests pass; that is not yet a decoded Android frame result. The full Flutter suite passes **53 tests** and analysis reports no issues. Host regression checks pass **22 tests**, with two Linux-only skips. Exact receipts and failed attempts are recorded in the Android authority.

This document does not claim arbitrary storage-migration crash recovery, physical-device fidelity, iOS execution, spoken accessibility, ambient/offline migration or production shared-cap enforcement. Those remain governed by their respective qualification boundaries.
