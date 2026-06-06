# Registry Cache Retention & Garbage Collection

## Overview

This document defines how registry-backed build caches are named, retained, pruned, and separated from release artifacts. It complements:

- [DOCKER_BUILD_RELEASE.md](../operations/DOCKER_BUILD_RELEASE.md) - image build, publication, and the baseline cache strategy
- [BUILD_CACHE_TRUST_BOUNDARY.md](BUILD_CACHE_TRUST_BOUNDARY.md) - which cache backends may cross which trust tiers
- [BUILDER_TRUST_BOUNDARY.md](BUILDER_TRUST_BOUNDARY.md) - which workflows are trusted to push or delete cache state
- [RELEASE_ENGINEERING.md](RELEASE_ENGINEERING.md) - release rollback boundaries and protected publication rules
- [RELEASE_ARTIFACT_RETENTION.md](RELEASE_ARTIFACT_RETENTION.md) - durable release evidence that must stay outside cache cleanup scope

The design goal is to keep registry cache storage bounded and recoverable without letting cache cleanup threaten release images, rollback evidence, or trusted supply-chain artifacts.

## Scope Classification

This document is **Advanced** guidance, not a baseline product requirement.

Under [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md), this document is currently retained as deferred guidance and is not part of the active first-release path.

Use it when the project keeps dedicated trusted registry cache lanes for protected branches or privileged builders. If the baseline release flow does not depend on registry-backed cache retention, this policy is optional and can be postponed until cache storage becomes a real operational concern.

## Ownership & Review Metadata

- **Primary owner:** Project maintainer
- **Review status:** Dormant
- **Last reviewed:** 2026-06-02
- **Review cadence:** Re-review before activation; no recurring cadence while dormant
- **Review set:** [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md)
- **Review triggers:** Any material change to registry cache namespaces, cleanup automation, package permissions, protected-branch cache retention assumptions, or decision to activate this deferred guidance

## Goals

1. Keep trusted registry cache storage bounded and predictable.
2. Ensure cache cleanup cannot delete production release images or evidence.
3. Separate remote registry cache retention from local builder garbage collection.
4. Preserve fast builds on active protected branches without treating cache artifacts as rollback assets.
5. Keep the cleanup path auditable, least-privilege, and recoverable after mistakes.

## Official Research Findings (May 2026)

### Docker registry cache behavior

- Docker documents that the `registry` cache backend stores cache separately from the final image and supports multi-stage cache reuse, including `mode=max`.
- Docker documents that a registry cache ref must not be the same as the target image location used for the final pushed image.
- Docker documents that all external caches are explicit: builds export with `--cache-to` and import with `--cache-from`.
- Docker documents that each cache location should be treated as a single writer location and should not be written multiple times without overwrite.
- Docker documents that multiple remote caches can be imported, such as a branch cache plus a main-branch cache.

### Docker builder garbage collection

- Docker documents `docker buildx du` as the inspection surface for builder disk usage, including whether records are reclaimable, mutable, shared, and last used.
- Docker documents that records marked non-reclaimable are still in use and are not removed by pruning.
- Docker documents `docker buildx prune` as the selected-builder cache cleanup mechanism, with filters and space-target controls for reclaiming local builder cache.

### GitHub container package management

- GitHub documents that the container registry supports granular permissions.
- GitHub documents that workflows can authenticate to the container registry with `GITHUB_TOKEN`, and package access can be linked to repositories.
- GitHub documents that registries with granular permissions can grant a repository workflow administrative access to a package.
- GitHub documents that packages and package versions can be deleted through GitHub management surfaces, and deleted packages or versions are restorable for 30 days as long as the namespace remains available.
- GitHub documents that workflows can delete or restore granular-permission packages through the REST API when the workflow repository has package admin permission.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Keep registry cache in the same package namespace as release images | Fewer names to manage; simple initial setup | Cleanup risk overlaps with production image storage; harder to scope permissions; easy operator error | Reject |
| Create per-commit or per-run registry cache refs and retain them indefinitely | Maximum forensic detail; avoids overwrite races | Unbounded package-version growth; high cleanup burden; no real rollback value | Reject |
| Use one mutable cache ref per active protected branch and target in a dedicated private cache package | Bounded storage; simple import rules; cleanup is easy to reason about; separates cache from release images | Slightly lower cache diversity; requires a clear naming contract | Preferred |
| Disable registry cache retention and rely only on local builder cache | Lowest remote storage burden | Weak for ephemeral builders; poor performance on clean runners; no shared trusted reuse | Reject as default |

## Recommended Policy

### Core rule

Registry-backed caches are disposable performance artifacts. They are retained only long enough to accelerate supported trusted branches and are never required for rollback, audit, or release recovery.

### Package separation

1. Store registry cache in a dedicated private GHCR package namespace that is separate from the production image package.
2. Do not push registry cache refs to the same package name used for release images.
3. Grant package-admin capability only to the trusted workflow repository or trusted reusable workflow path that builds and prunes cache artifacts.
4. Keep cache cleanup credentials scoped to the cache package, not to production image packages.

### Ref model

1. Maintain one mutable registry cache ref per active protected branch family, trust tier, image target, and cache epoch.
2. Use stable moving refs such as `main`, `release-x.y`, or `lts-x.y` style cache lanes rather than per-commit cache refs in steady state.
3. Permit importing from more than one trusted cache source when useful, such as the current release branch plus `main`.
4. Do not create PR-owned registry cache refs; pull requests stay on PR-safe cache backends only.

### Retention policy

#### Active refs

Retain only the current mutable cache ref for:

- the default protected branch
- each currently supported release branch
- any explicitly approved long-lived maintenance branch

#### Epoch overlap

1. When cache invalidation requires an epoch rotation, keep the previous epoch for a short overlap window only.
2. The default overlap window is 7 days, which gives trusted workflows time to converge without turning old cache generations into long-term storage.

#### End-of-life branch cleanup

1. When a release branch leaves support, stop writing its cache ref immediately.
2. Delete its stale cache versions after a short grace window, default 30 days.
3. That 30-day grace period is for operational safety and aligns with GitHub's documented package restore window; it is not a rollback contract.

### Garbage-collection ownership

1. A scheduled trusted workflow on the default branch owns registry cache cleanup.
2. That workflow may list cache package versions, delete stale cache versions, and optionally restore versions during operator error recovery.
3. Cleanup must never run from pull request workflows or from workflows that check out untrusted code.
4. Prefer `GITHUB_TOKEN` with package-admin access to the dedicated cache package when GitHub's current package-management behavior allows it; use a narrowly scoped automation credential only as an exception.

### Local builder garbage collection

1. Treat local builder cleanup as separate from registry cache cleanup.
2. Long-lived trusted builders should use `docker buildx du` to inspect reclaimable cache state.
3. Long-lived trusted builders should use `docker buildx prune` on a schedule or threshold basis so local disk pressure does not accumulate indefinitely.
4. Ephemeral GitHub-hosted runners do not need persistent local-cache retention policy because the builder disappears after the job.
5. Trusted self-hosted exception runners should also follow [TRUSTED_RUNNER_STATE_DISPOSAL.md](TRUSTED_RUNNER_STATE_DISPOSAL.md) for builder removal, workspace cleanup, and auth-bearing residue disposal.

### Deletion safeguards

1. Keep cache and release artifacts in separate package namespaces so automated deletion cannot hit release images by path confusion.
2. Match deletion targets by an allowlisted cache package name or prefix, never by broad registry-wide patterns.
3. Delete stale package versions, not the entire cache package, during normal maintenance.
4. Treat release images, SHA tags, SBOMs, attestations, and checksums as separate retention subjects governed outside the cache cleanup workflow.

### Rollback boundary

1. Release rollback must depend on retained release images and recovery evidence, not on retained build cache.
2. Cache refs may be deleted without invalidating the project's supported rollback posture.
3. If a cache version is accidentally deleted, the impact is slower trusted rebuilds, not loss of a deployable release artifact.

## Final Recommendation Stack

1. Use a dedicated private GHCR cache package that is separate from the production image package.
2. Keep only one mutable cache ref per active protected branch family, target, and cache epoch.
3. Do not create per-commit or PR-owned registry cache refs in normal operation.
4. Run registry cache cleanup only from a trusted scheduled workflow on the default branch.
5. Give that workflow package-admin rights only to the dedicated cache package, not to production image packages.
6. Use a short epoch-overlap window, default 7 days, when rotating cache namespaces after poisoning or major toolchain changes.
7. Delete end-of-life branch cache versions after a short grace window, default 30 days, and rely on GitHub's restore window only as emergency recovery from deletion mistakes.
8. Treat local builder `buildx du` and `buildx prune` hygiene as a separate maintenance concern from remote registry cache cleanup.
9. Keep rollback evidence retention separate: release images, SHA tags, SBOMs, attestations, and checksums are not part of cache garbage collection.

## Three High-Value Next Design Areas

1. Secret brokerage and rotation: define whether trusted build and release workflows obtain credentials from GitHub secrets, a broker such as Vault, or only OIDC-minted short-lived tokens.
2. Deployment-time provenance enforcement: define whether attestation verification stops in CI or is enforced again by deployment automation and admission policy.
3. Cache incident forensics: define what minimal cache-invalidation and investigation evidence should be preserved after suspected poisoning or credential leakage.

## Official Sources

- Docker Docs - Cache storage backends: https://docs.docker.com/build/cache/backends/
- Docker Docs - Registry cache backend: https://docs.docker.com/build/cache/backends/registry/
- Docker Docs - docker buildx du: https://docs.docker.com/reference/cli/docker/buildx/du
- Docker Docs - docker buildx prune: https://docs.docker.com/reference/cli/docker/buildx/prune
- GitHub Docs - Working with the container registry: https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry
- GitHub Docs - About permissions for GitHub Packages: https://docs.github.com/en/packages/learn-github-packages/about-permissions-for-github-packages
- GitHub Docs - REST API for packages: https://docs.github.com/en/rest/packages
- GitHub Docs - Deleting and restoring a package: https://docs.github.com/en/enterprise-server@3.20/packages/learn-github-packages/deleting-and-restoring-a-package