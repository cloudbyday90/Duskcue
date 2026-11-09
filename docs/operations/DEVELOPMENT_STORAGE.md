# Development storage maintenance

## Purpose and authority

Use the [Duskcue repository-cleanup skill](../../.agents/skills/duskcue-repository-cleanup/SKILL.md) to maintain checkout disk usage, development caches, qualification artifacts and Duskcue-owned build/test images. Inspect at the start/end of substantial build or qualification series and when growth is reported. Retire disposable outputs promptly after their owners actually finish, while retaining compact evidence. The operator explicitly scoped this skill to the repository cleanup process. It creates no runtime cleanup service, background scheduler or application retention feature. Media, recordings, application data and the playback reservation/receipt policies remain separate.

The October 31 operator decision limits image cleanup to Duskcue. Other-project images and all application volumes remain excluded. Preserve every container-referenced image, the two newest distinct images per Duskcue repository and separately required pinned artifacts. The operator approved one brief Docker Desktop restart with exact Harmoniarr/Classifarr restoration and health verification; future interruptions need their own applicable authorization. Existing authorization does not require asking again for every identified disposable cache directory.

## Official research — October 31, 2026

Sources were discovered and read through MCP search, then checked against installed CLI help.

- [Docker pruning](https://docs.docker.com/engine/manage-resources/pruning) distinguishes images, containers, volumes and build cache. Unused volumes may still contain important data. Default image pruning is narrower than `-a`; global system pruning also affects stopped containers, networks and BuildKit cache mounts. Use exact owned objects on this shared engine.
- [Buildx prune](https://docs.docker.com/reference/cli/docker/buildx/prune/) supports filters for identity, description, type, activity, age and sharing; multiple filters are combined. [BuildKit garbage collection](https://docs.docker.com/build/cache/garbage-collection/) supports periodic age/space policies. A shared-engine setting is a separate operator decision, not a project-local cleanup side effect.
- [Cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html) supports package/profile selection and dry runs. A bare clean removes the whole target directory. [Cargo build-cache documentation](https://doc.rust-lang.org/stable/cargo/reference/build-cache.html) distinguishes final artifacts, dependencies, incremental output and configurable target/build directories; internal layouts can change. Prefer supported scoped cleanup and preserve expensive dependencies when useful.
- [Android AVD removal](https://developer.android.com/tools/agents/android-cli/commands/emulator_remove?authuser=90) removes a named device and its disks. The [installed-manager reference](https://developer.android.com/tools/avdmanager?authuser=303) retains the legacy deletion command while documenting its deprecation. Inspect actual installed behavior and explicit AVD-home/name/path identity; do not substitute a global personal device for an unrecognized qualification AVD.
- [Microsoft WSL disk guidance](https://learn.microsoft.com/en-us/windows/wsl/disk-space) distinguishes guest filesystem capacity from Windows storage. [Docker's WSL reclamation explanation](https://www.docker.com/blog/docker-desktop-4-34/) describes reclaiming zeroed blocks when Desktop quits on the managed-data-disk architecture. Object reclamation and physical host-space recovery must be measured separately. Do not delete a distribution or its data disk to reclaim cache.

## Retention choices

| Choice | Benefit | Cost / boundary |
| --- | --- | --- |
| Prompt per-run VM disk retirement; retain logs/results/screenshots | Prevents multi-GiB growth on every native attempt without erasing failure evidence | Disk-level replay requires a new fixture; preserve an explicit investigation hold when needed |
| Package-scoped Cargo cleanup after inventory/dry run | Reclaims obsolete server artifacts while preserving third-party dependencies | Server executables and incremental state may require rebuilding; old cached-runner manifests cannot authorize missing binaries |
| Exact Duskcue image references with two recent distinct images retained | Removes redundant qualification images while keeping recent reuse/rollback | Shared layers/build records may retain most bytes; never promise the sum of image sizes |
| Preserve unknown volumes and other projects | Avoids deleting application data or unrelated rollback history | Some reported reclaimable storage remains deliberately untouched |
| Supported Desktop reclamation during an approved restart | Can return freed guest blocks to Windows without hand-editing the VHD | Temporarily interrupts Docker services; physical compaction may reclaim less than expected |

Recommendation stack: native Cargo/Docker/installed Android management tools; exact project ownership and bounded candidate manifests; retained source/tool/result evidence; observed completion; explicit service-outage authority; supported Desktop reclamation and measured host free space. Do not add a global cleanup daemon, change compilation profiles, disable incremental compilation, or impose a shared-engine GC cap as an incidental fix.

## October 31 inventory and execution

Initial workspace inventory: `target` 263.33 GiB, including 169.98 GiB incremental data and 84.27 GiB dependencies; `.cache` 104.76 GiB, including 97.74 GiB Android resources; mobile build outputs 7.45 GiB. Docker reports 775 images (376.4 GB), 6,699 build records (316.6 GB) and 306 volumes (27.96 GB). These figures use different units and share storage; they must not be added as independently reclaimable bytes. Docker Desktop is the only installed WSL distribution; its managed data file has logical length 694.89 GiB.

The guarded-work marker is absent and no Cargo/rustc/emulator process is active. A reviewed workspace manifest identifies 22 stopped owned Android runs and two unknown/insufficiently attributed boot directories to retain. All logs/results and current SDK/Gradle/APK inputs are protected. Application containers, images, mounts and volumes were inventoried before any cleanup. Full inventories remain ignored under `.cache/disk-cleanup/`.

Automatic review rejected the general bounded recursive file-deletion command with “blocked by policy”; no deletion ran through that command. Supported native cleanup operations were then evaluated independently. Cargo's dry run estimated 219.4 GiB; `cargo clean --manifest-path server/Cargo.toml -p duskcue --locked --offline` completed, reporting 66,934 files / 218.0 GiB removed. Actual host recovery is measured separately. Third-party dependencies, receipts and APKs remain; the server package must rebuild before its removed executables can be used again.

The newer Android CLI did not enumerate these externally created scoped AVDs, so it was not used to delete an unlisted device. The installed legacy manager positively identified the exact scoped name/path. Native deletion removed 22 owned AVDs (86.251 GiB of inventoried logical disk files), preserving the run logs/results and current SDK, Gradle cache and APKs. Two boot directories without sufficient ownership evidence remain. No application volume was deleted.

### Verified outcome

Maintenance/skill checkpoint: `501dd4e` on main. Documentation-only successors preserve its instructions and evidence attribution. No application code, dependency upgrade or release is included.

| Resource | Before | After / action |
| --- | --- | --- |
| Cargo target directory | 263.33 GiB | 46.060 GiB; scoped Cargo cleanup reports 218.0 GiB / 66,934 files removed |
| Repository cache | 104.76 GiB | 18.515 GiB; receipts/logs retained |
| Android development cache | 97.74 GiB | 11.486 GiB; 22 stopped per-run AVDs removed through the installed manager |
| Duskcue image references | 142 | 12; 130 exact stale references removed, preserving container references and two recent distinct images per repository |
| Duskcue build-cache selection | 87 aged, unshared, reclaimable regular records allowlisted | Native pruning reports four removed records / 136.9 MB; no global builder prune or cache-mount deletion |
| Application volumes | 306 total volumes | 306 remain; none removed |
| C: host free space | 700.071 GiB at the reviewed cleanup baseline | 984.730 GiB at final measurement; observed increase 284.658 GiB |
| Docker managed VHD logical length | 694.893 GiB | 694.893 GiB after supported restart; no physical VHD shrink is claimed |

The approved `docker desktop restart --timeout 240` completed. The exact original Harmoniarr/Classifarr container IDs, immutable image IDs, mounts, restart policies and prior running set were checked before/after; both are healthy. Previously stopped containers remain stopped. The current qualification APK still matches its recorded SHA-256, and SDK metadata plus the failed worker-run result remain present. Removed Rust test executables require a later guarded rebuild; validation must not treat an old cached-runner manifest as evidence that the executable remains available. No application code or dependency graph changed, and no application regression pass is claimed from maintenance.

The shared Docker engine also exhibited changes to twelve named Classifarr image references during this interval, outside this task's Duskcue-only mutation journal. Aggregate engine changes are therefore not attributed wholly to this task. The observed post-restart engine summary is 699 images / 351.3 GB, 6,629 cache records / 313.7 GB and unchanged 306 volumes / 27.96 GB. Shared layers, retained records and other-project storage explain why object sizes are not host bytes recovered. Further reclamation must preserve the user-selected project boundary; this task neither changes shared GC settings nor performs manual VHD compaction.

The installed skill is `$duskcue-repository-cleanup`, discovered through a local directory link to the versioned repository skill. Its old name was removed from repository instructions and metadata. Automatic review rejected the operation that would remove the old installed directory link with “blocked by policy”; that command made no changes. Explicit file patches renamed the two repository files and a new discovery link was created. The retained old empty link has no `SKILL.md`; the renamed skill is available through the new entry. No alternate unlink or permission bypass was attempted.

Private inventories, exact removal journals, Cargo/native-manager results, cache allowlists, service restoration evidence and before/after measurements remain ignored under `.cache/disk-cleanup/`. Only sanitized outcomes are published here. The skill/frontmatter, repository links, whitespace and current APK identity are verified separately from the live cleanup operations.

## Pending development work

The subsequent [observer increment](../design/FLUTTER_WORKER_RECOVERY_OBSERVER.md) is explicitly confirmed, implemented and scoped-qualified, including all eight real Android stages. Next: Android scope/control/read cancellation and remaining relay/output-format evidence before ambient. The earlier repository cleanup itself did not authorize or qualify that behavior.

## Observer qualification maintenance — October 31

The observer series uses the routine skill at start/end. A Windows commit-limit interruption ends its first attempt during stage eight; its seven completed stages and interrupted app-level cleanup remain distinct from the passing repeat. Independent inspection confirms all owned processes/listeners gone and the wrapper removes its PostgreSQL container. The successful repeat confirms complete app/private-state/uninstall, mapping, VM/ADB, backend/root and database cleanup before storage retirement. No service restart, unrelated process termination, application-data cleanup or shared Docker pruning occurs.

The installed AVD manager positively identifies each exact scoped name/path; its list formatter reports a system-image metadata error, which is not treated as device absence. After path/reparse, process and listener revalidation, native named deletion succeeds for both new AVDs. The interrupted disk is 4.294 GiB logical (observed host recovery 4.234 GiB); the passing disk is 5.099 GiB logical (observed recovery 5.040 GiB). Total removed logical AVD storage: 9.393 GiB. Source/tool hashes, logs, screenshots/results, both current APKs, SDK and Gradle inputs remain.

The earlier package cleanup requires a real Rust rebuild; Cargo reuses dependencies and rebuilds the test package in 3m42s. The passing repeat validates/reuses that current compiler checkpoint and reruns both runtime fixtures. Preserve it for the pending Android matrix instead of immediately cleaning useful outputs. Initial series free space is 981.207 GiB; final post-AVD measurement is 973.323 GiB. This net reduction includes rebuilt reusable artifacts and shared host activity and is not a failed-cleanup or isolated allocation measurement. Exact private before/after manifests and manager results live under each ignored native-run directory, with the series baseline under `.cache/android-playback/`. No private feed value or data is published.

## Scope qualification maintenance — October 31

The [scope/cancellation increment](../design/FLUTTER_SCOPE_CANCELLATION.md#native-attempts-and-remaining-acceptance) retains current dependencies, compiler checkpoint, SDK, both verified APKs and compact source/tool/test evidence. The first native attempt passes the original eight journeys before a host commit interruption stops the additional VM. A fresh repeat loses its Flutter-driver connection during the fourth original journey; VM/ADB/backend/container cleanup is observed, while app-level cleanup remains unconfirmed. These failures do not establish native scope acceptance.

After exact native identity/path/reparse, process and listener revalidation, the installed manager deletes all three new AVDs: 7.297 GiB logical across the first attempt's two disks and 4.445 GiB in the repeat. Total removed logical storage is 11.742 GiB; combined observed host free-space recovery during those deletions is 11.559 GiB. Logs, screenshots, source/artifact identities and current reusable builds remain. Manager list formatting still reports its known metadata-rendering error after identifying the exact registrations; successful named deletion and observed absence establish removal. No service restart, application data/volume deletion, unrelated-project image cleanup or global pruning occurs.

An interrupted Rust case also leaves a small private temporary fixture outside the repository. Its exact marker and owner interval are identified, but automatic approval review rejects the recursive deletion as “blocked by policy.” That operation makes no changes; it is not retried through another mechanism. The fixture remains, and its PostgreSQL server/container and owned processes/listeners are gone. Do not claim complete temporary-file removal. Exact native-manager receipts and interrupted-run evidence remain ignored under `.cache/android-playback/` and `.cache/testing-memory/`.

An interim native hold is caused by host commit headroom, independently of physical free RAM. The operator unloads local models; the agent leaves model and other service processes untouched. A later controlled run passes both new scope journeys and seven original journeys before a driver connection failure at healthy memory levels. [Attribution evidence](../ci/QUALIFICATION_MEMORY_ATTRIBUTION.md) records that distinction and observed release of test-owned allocations. AVD cleanup itself is not an application-test pass.

Final cleanup retires six AVDs across all attempts, totaling 23.840 GiB logical storage and 23.486 GiB combined observed host recovery during deletion. The final measured C: free space is 967.148 GiB; net series change includes preserved rebuilt compiler outputs and other host activity, so it is not attributed solely to these removals. All current SDK/compiler/APK inputs and compact success/failure evidence remain. One saved emulator PID has already been reused by an unrelated console process; image-identity, named-QEMU and listener checks establish that it is not the old owner. That process is left running. Native named deletion succeeds only after the actual owner absence is verified. The policy-blocked private temporary fixture remains as stated above.

## Driver bootstrap maintenance — October 31

The [bootstrap increment](../ci/ANDROID_DRIVER_BOOTSTRAP.md#implementation-and-qualification-outcome) corrects host orchestration and passes both real scope journeys plus all eight original journeys. Failed attempts remain failed, with their compact log/result/identity/cleanup records preserved. The series reuses the current Rust compiler checkpoint, SDK and byte-identical verified APKs; it does not rebuild caches merely to replace what maintenance deleted.

After all owners report closed, exact path/reparse, image/PID, named-QEMU and listener checks precede installed-manager named deletion. All eight new AVDs are removed: **28.037 GiB logical storage**, with **27.562 GiB observed host free-space recovery during deletion**. C: free space rises from 943.957 to **971.518 GiB** across those cleanup operations; shared host activity and sparse allocation prevent equating logical size with physical recovery. Per-run native-manager receipts and `.cache/android-playback/bootstrap-cleanup-receipt.json` retain exact measurements and names.

Current compiler/dependency/SDK/APK checkpoints, authored frame evidence, compact private fixtures and all corrected/passing diagnostics remain. The old SDK raw communication log is retained as private historical evidence, not appended by the new public-API wrapper. The previously policy-blocked TEMP fixture remains untouched. No application media/database/volume, unrelated-project image, global builder cache or WSL disk is deleted or compacted. No service restart occurs; the unrelated default ADB server retains its PID, and Classifarr remains healthy. Ownership exit and AVD cleanup do not substitute for the separate successful native test receipts.

## Relay qualification maintenance — November 1

The [relay increment](../ci/ANDROID_RELAY_QUALIFICATION.md#final-complete-regression) maintains artifacts after each bounded series. The interrupted full run's two AVDs, focused companion's one AVD and passing final run's three AVDs are removed only after exact owner-image/PID, named-QEMU, listener, path and reparse checks. Installed-manager registration/name/path checks and successful named deletion establish removal; per-run manager logs and compact cleanup receipts remain ignored.

| Series | AVDs | Removed logical GiB | Observed host recovery GiB |
| --- | ---: | ---: | ---: |
| Guard-interrupted full run | 2 | 7.060 | 6.991 |
| Focused companion | 1 | 3.526 | 3.467 |
| Passing complete regression | 3 | 10.543 | 10.364 |
| Total | 6 | 21.129 | 20.822 |

The final deletion interval leaves **989.326 GiB** free on C:. These are deletion-interval observations, not attribution of every host disk change or an equation between sparse logical bytes and physical recovery. Reusable Cargo/SDK/dependency/APK checkpoints, current authored media, generated-output probes, screenshots and compact success/failure/guard records remain. The earlier policy-blocked TEMP fixture is untouched. No other-project image, application database/volume/media, global cache or WSL disk is removed or compacted; no Docker/service restart occurs. Physical cleanup of an interrupted test does not turn it into a passing test. The complete final regression has separate successful application/backend/host evidence.

## Ambient source-work maintenance — November 8

The [ambient implementation checkpoint](../ci/ANDROID_AMBIENT_QUALIFICATION.md#implementation-and-verification-checkpoint) performs source work while current-source builds/native qualification wait for unchanged memory admission. One preliminary APK build and compact passing/interrupted test evidence remain reusable or diagnostic inputs; its source receipt correctly marks that APK stale for later edits. Current SDK/dependency/compiler caches and all private feed fixtures are preserved.

No new AVD or PostgreSQL fixture is created in this pass; no path/image/volume is removed and no service is interrupted. The active guarded-work marker is absent and the recorded completed test roots are gone. A sanitized Java/Dart/emulator inventory establishes no task-owned residual worker to stop; unrelated editor/Gradle processes remain untouched. Private audit `ambient-source-maintenance.json` observes **983.855 GiB** free on C: at `2026-11-09T01:32:43Z`. Shared host changes are not attributed to this task from that free-space observation. Compact receipts/logs, current APK/tool identities and the earlier policy-blocked TEMP fixture remain; no broad cache purge or WSL reclamation is attempted.
