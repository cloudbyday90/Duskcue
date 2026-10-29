# Flutter playback journal storage qualification

## Contract and current outcome

The [Flutter migration](../design/VOD_FLUTTER_CLIENT_MIGRATION.md) requires one app-scoped playback reconciliation journal, at most 4 KiB of serialized command/scope identifiers. It contains no credentials, stream URLs, media metadata or command bodies. Android storage uses the dedicated `duskcue.vod.journal.v1` namespace and `duskcue_vod_journal_v1` key prefix. Known write/read-back precedes Start; pending platform operations retain ownership. Missing, corrupt or unreadable data must not silently authorize duplicate playback.

`resetOnError`, automatic algorithm migration and migration backup are currently disabled for this journal. In particular, storage failure cannot clear the record as an automatic recovery action. A replacement session or local journal deletion is not proof of server resource release. Read-back does not establish durability under arbitrary device power loss.

Actual API 36 testing on October 28, 2026 reached the installed Flutter application with the verified APK bytes, but locked `flutter_secure_storage` **10.3.1** reported an algorithm change on first access to a fresh namespace. With migration and reset disabled, it refused initialization. Playback did not initialize, and no decoded-media result is claimed. Unit mocks had not exercised this Android implementation defect.

## Publisher research and approved correction

The official [10.3.4 changelog](https://pub.dev/packages/flutter_secure_storage/versions/10.3.4/changelog), discovered through MCP search, identifies the fresh-install migration fix in 10.3.2, corrected saved-algorithm detection in 10.3.3, and additional namespace/key-preservation fixes through 10.3.4. The [package documentation](https://pub.dev/packages/flutter_secure_storage) explains its migration and reset controls.

Managed `flutter pub cache add flutter_secure_storage --version 10.3.4` first downloaded source for inspection. After explicit operator approval on October 29, the application now pins exactly 10.3.4. A complete lock-map comparison confirms that only this package's version and archive checksum changed; every other dependency and SDK constraint remains identical. The candidate's cached publisher archive SHA-256 is `fe638107c5f69119156ada2db5a57734385fac3f64430bd7252a00d3ead2ca4b`. Its source handles a genuinely empty store before deciding that algorithm migration is needed. The required Dart/Flutter minima and federated-package constraints remain compatible with the current stack.

The approved exception to the confirmed locked-stack contract is deliberately limited: pin this existing direct dependency to **10.3.4**, resolve and verify only its expected lock entry/checksum, retain other locked packages, keep the existing journal namespace and disabled reset/migration controls, then rebuild and qualify the current APK. No 11.x major migration or cipher downgrade is proposed. The operator explicitly confirmed this exception on October 29, 2026. Other dependencies and all reset/migration controls remain fixed; the native storage sequence described below passes.

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

## Verified Android storage sequence — October 29

The approved 10.3.4 patch passes actual API 36 tests against real PostgreSQL/HTTP/FFmpeg. One installed qualification APK runs six sequential stages on one owned emulator: decoded playback baseline; retain a readable bounded journal; force-stop and restore cleanup-only; retain another viewer and corrupt only its ciphertext; force-stop and refuse reads/new work while preserving ciphertext across Close; restore the backed-up ciphertext and explicitly settle that original viewer. Each process exit is observed. Default-store and separate-namespace sentinels remain readable across journal writes, deletion and both restarts. Sentinels contain only fixed test strings.

The corruption/recovery helper exists only in the qualification Android source set, accepts no path/key arguments, backs up at most 16 KiB of encrypted bytes in app-private no-backup storage and restores only that exact ciphertext for cleanup. Production contains no such channel. The normal Close path now preserves an unreadable journal when no parsed record is available; the regression test verifies repeated Open still refuses Start. It also retains late pre-dispatch journal writes for subsequent cleanup-only reconciliation.

The same run displays the authored grayscale/frame-ID picture near advancing native position, completes a paused same-viewer seek and server settlement, and confirms application/private-data, emulator/QEMU/ADB/listener and disposable PostgreSQL cleanup. The source inventory is `5626f705485fb574bbc1064ad17781a6a0f648584a6e4f78f74f12e2b3968ca0`; APK SHA-256 is `e2954e09c739d313f4bd7d69d6b2ee36b2b68c01cf15beda160711b3248f5bad`. The ignored runtime receipt is `2026-10-29T22-36-57-783Z-e6919ff0-6f67-4e5b-a2da-d3eb4621399e.jsonl`, resource `301adc13-1e89-415d-bab2-aa8f75ab543d`.

These results establish first use, ordinary process-death restoration, ciphertext-corruption refusal and current-namespace isolation for this runtime. They do not establish migration from every historical cipher/namespace arrangement, arbitrary power-loss durability or the remaining mobile fault/recovery matrix. Analysis passes. Full-suite and packaging outcomes are maintained in the [Android evidence](../ci/ANDROID_PLAYBACK_QUALIFICATION.md).

This document does not claim arbitrary storage-migration crash recovery, physical-device fidelity, iOS execution, spoken accessibility, ambient/offline migration or production shared-cap enforcement. Those remain governed by their respective qualification boundaries.


The final October 29 repeat again passes all native storage stages with exactly three viewer/history identities, the approved native-only thirty-second attachment observer and direct authenticated VM qualification. Storage/control observations remain five seconds; journal namespace and reset/migration flags are unchanged. [Final evidence](../ci/ANDROID_PLAYBACK_QUALIFICATION.md#final-approved-checkpoint) records current hashes, 56 passing Flutter tests, 24 host passes (two Linux-only skips), both APKs and complete owned cleanup. The broader Android fault/recovery matrix remains pending.
