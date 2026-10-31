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

Resume the [positive worker-evidence observation review](../ci/ANDROID_WORKER_RECOVERY.md#current-native-result-and-next-design-decision) after maintenance. The proposal observes successful configuration reads at most once per second for up to sixty seconds, issues one Recover only after positive current server evidence plus a confirmed checkpoint, and cancels on scope/output/background/Close/lease/competing-command changes or failed reads. It must preserve independent renewal, exact viewer identity and no mutation replay. That behavior choice remains pending; cleanup does not authorize it or complete the Android qualification gate.
