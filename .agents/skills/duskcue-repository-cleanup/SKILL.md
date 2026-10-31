---
name: duskcue-repository-cleanup
description: Maintain disk usage and cleanup of the Duskcue repository, generated build/test files, development images and WSL build storage after substantial work or reported growth. This is repository housekeeping; application storage and runtime retention are outside scope.
---

# Duskcue repository cleanup

Apply this skill to the development checkout and its build/test infrastructure. Its output is a maintained repository cleanup process and verified artifact removal. Application media, databases, recordings and their runtime retention remain outside scope; do not implement a cleanup service in Duskcue.

Keep development storage bounded without destroying application data, expensive current compiler checkpoints or evidence needed to understand a failed run. Use the active Duskcue workspace as the root; resolve a linked skill directory to its canonical repository path before following relative references. Read [development storage](../../../docs/operations/DEVELOPMENT_STORAGE.md) for the current policy, official sources and maintenance record. Follow CONTEXT.md and the existing memory workflow for any actual build or runtime qualification.

## Routine use

At the beginning and end of a substantial Rust/Docker/Android qualification series, inspect growth and retire that series' disposable resources once physical process, reader and socket completion is confirmed. Do not wait for disk exhaustion. For ordinary small edits, avoid repeatedly scanning the whole workspace. A skill is not a scheduled job; do not install timers or change Docker/WSL settings without a request.

Honor existing cleanup authorization. Routine maintenance covers identified, regenerable Duskcue artifacts. Confirm a concrete plan only when it introduces application-data deletion, other-project scope, loss of a needed rollback/artifact, service interruption or a new persistent storage policy. Current-session approval for a Docker restart does not grant future outage permission.

## Establish ownership before deletion

1. Check working-tree state, actual free disk space, the guarded-work marker and active compiler/emulator/build processes. Inventory project paths first and drill into the largest areas; do not recursively scan the whole host or media/network shares. Use read-only inventories of `.cache`, the effective Cargo target/build directories, client outputs, Docker containers/images/cache/volumes and WSL distributions. Keep logical object sizes separate from actual host free-space changes; shared layers and sparse files defeat simple addition.
2. Separate source, media, recordings, application databases/volumes, keys and private feed fixtures from generated artifacts. Ignored/untracked, old, anonymous or unattached does not establish disposability. Preserve other projects unless the user includes them. Never unregister WSL, delete Docker's VHDX, factory-reset Docker, or use global `system prune -a --volumes` as routine cleanup.
3. Prepare exact candidate paths or immutable object IDs, ownership evidence, protected resources and expected rebuild costs. Revalidate immediately before mutation. On Windows, resolve absolute targets inside the exact intended root, refuse reparse points/junction traversal, and keep file operations in one shell. Do not pass enumerated filesystem paths to another shell's deletion builtins.

## Apply the smallest useful cleanup

- **Rust:** prefer Cargo's package/profile-scoped cleanup with a dry run. Resolve configured target/build locations first; a bare `cargo clean` removes dependencies too. Quiesce compilers and let Cargo own its lock. Package cleanup can remove server test executables as well as incremental data: retain receipts, but require fresh executable/input validation or rebuild before any cached runner is reused. Do not compile merely to repopulate what was just cleaned.
- **Android:** preserve the installed SDK/image, Gradle dependency cache, current verified APKs and source/tool identities. Retire per-run AVD disks after observing exact emulator/ADB exit and listener closure. Bind the installed manager to that run's AVD home and verify the registered name and resolved disk path. A manager that cannot list an externally created AVD is not evidence that it is absent. Do not delete global/personal AVDs. Preserve run logs, results and diagnostic screenshots; keep failed state only when investigation needs it, with an explicit reason to retain it.
- **Docker images:** protect images referenced by every container, including stopped ones; the two newest distinct images per Duskcue repository; and any separately pinned required compiler/runtime image. Trace Duskcue ownership from repository/recipe/labels and actual IDs. Remove reviewed exact references without forced image deletion, rechecking identity and container references. A tag count is not a reclaimed-byte estimate.
- **Build cache:** preserve current dependency/compiler cache mounts and reusable checkpoints. Select only proven Duskcue records; match immutable IDs, recipe evidence, type, last use, active/shared state and protected outputs. Use the installed builder's supported filters. Do not prune a shared builder globally to satisfy a Duskcue-only request, or silently change its garbage-collection configuration.
- **Volumes/files:** preserve application volumes. Remove a volume only when it is positively identified as a disposable Duskcue development/qualification fixture and cleanup is authorized; an anonymous name is insufficient evidence. Handle private fixtures with the same ownership checks. Preserve compact success/failure/cleanup receipts and manifests. Remove duplicate downloads or old client outputs only after checking they are no longer referenced by the current tool/artifact receipt.

When automatic approval rejects an operation, report the rejected action and reason; stop that operation. Do not disguise or split the command, retry through another language, or change permissions to bypass review. A genuinely narrower native cleanup operation may be considered on its own merits; explain its scope and validate it normally.

## WSL reclamation and completion

Delete eligible Docker objects through Docker first. A WSL VHD's apparent maximum, logical file length and physical host allocation are different measurements. Prefer supported Docker Desktop reclamation; manual VHD compaction requires the exact supported procedure, a stopped/unmounted disk and explicit outage approval. Do not zero-fill a live filesystem or launch a privileged reclaim container as a default.

Before an approved restart, record exact running container IDs, image IDs, mounts, restart policies and health. Restore only that prior running set, verify the same images/mounts and health, and report any uncertainty promptly. Leave containers that were stopped before maintenance stopped.

Record a private before/after manifest, methods, confirmed removals, retained/blocked candidates, rebuild implications and measured host free-space change. Update the relevant maintenance record with sanitized totals; do not publish credentials, private URLs or raw configuration. Cleanup is not a new application-test pass. Report the previously pending development next step without resuming it unless requested.
