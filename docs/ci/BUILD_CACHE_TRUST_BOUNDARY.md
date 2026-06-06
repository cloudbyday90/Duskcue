# Build Cache Trust Boundary

## Overview

This document defines which build caches may be persisted, which workflows may read or write them, how cache backends are separated across trust tiers, and which build inputs must stay transient. It complements:

- [DOCKER_BUILD_RELEASE.md](../operations/DOCKER_BUILD_RELEASE.md) - image build, publication, and baseline cache mechanics
- [REGISTRY_CACHE_RETENTION.md](REGISTRY_CACHE_RETENTION.md) - registry cache naming, retention windows, and cleanup ownership
- [BUILDER_TRUST_BOUNDARY.md](BUILDER_TRUST_BOUNDARY.md) - runner trust tiers, secret-bearing job separation, and private dependency ingress rules
- [PRIVILEGED_ARTIFACT_HANDOFF.md](PRIVILEGED_ARTIFACT_HANDOFF.md) - rules for keeping untrusted workflow outputs out of privileged release paths
- [CI_TESTING.md](CI_TESTING.md) - validation lanes, protected release gates, and workflow security posture

The design goal is to keep builds fast without turning cache state into a cross-boundary trust channel for secrets, private dependencies, or poisoned build outputs.

## Scope Classification

This document is **Advanced** guidance, not a baseline product requirement.

Under [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md), this document is currently retained as deferred guidance and is not part of the active first-release path.

Use it when the project separates untrusted and trusted build lanes or persists cache state across privileged workflows. For a baseline self-hosted release flow, cache tuning should stay simple and PR-safe; the stricter trust-tier cache split in this document is only justified once privileged automation becomes real.

## Ownership & Review Metadata

- **Primary owner:** Project maintainer
- **Review status:** Dormant
- **Last reviewed:** 2026-06-02
- **Review cadence:** Re-review before activation; no recurring cadence while dormant
- **Review set:** [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md)
- **Review triggers:** Any material change to cache backends, trust-tier separation, GitHub Actions cache visibility, protected release-build reuse assumptions, or decision to activate this deferred guidance

## Goals

1. Prevent untrusted workflows from poisoning caches later consumed by trusted integration or release jobs.
2. Prevent credentials, private packages, and other sensitive material from being exposed through cache storage.
3. Preserve fast CI builds without treating cache hits as evidence of trust.
4. Make cache destinations, readers, and writers explicit and reviewable.
5. Keep trusted release builds reproducible and easy to invalidate after incidents.

## Official Research Findings (May 2026)

### GitHub cache visibility and pull request scope

- GitHub documents that workflow runs can restore caches from the current branch and the default branch, and pull request workflows can also restore caches from the base branch, including the base branch of a fork target.
- GitHub documents that caches created by pull request runs are created for the merge ref (`refs/pull/.../merge`) and can only be restored by re-runs of that same pull request.
- GitHub explicitly recommends not storing sensitive information in cached paths because anyone with read access can create a pull request and access cache contents, including from forks targeting the base branch.
- GitHub documents that cache contents are immutable once written; changing cache contents requires a new key.

### GitHub privileged workflow risks

- GitHub warns that running untrusted code on `pull_request_target` or `workflow_run` can lead to cache poisoning and unintended access to secrets or write privileges.
- GitHub documents that a workflow started by `workflow_run` can access secrets and write tokens even when the earlier workflow could not, which makes cache handoff across that boundary security-sensitive.

### Docker cache backend behavior

- Docker documents that external caches must be explicitly exported with `--cache-to` and explicitly imported with `--cache-from`.
- Docker documents `inline`, `registry`, `local`, and `gha` as the current cache storage backends, with `s3` and `azblob` still unreleased.
- Docker warns that each cache location writes to one location and should not be written twice without overwrite, while also supporting imports from multiple cache sources.
- Docker documents that the `registry` backend stores cache separately from the final image and supports `mode=max` for multi-stage build reuse.
- Docker documents that the `gha` backend is the recommended backend inside GitHub Actions, but explicitly says GitHub cache access restrictions still apply.
- Docker documents that the `gha` backend defaults to `scope=buildkit`, so multiple builds overwrite one another unless scope names are separated deliberately.

### Docker cache optimization and non-persistent inputs

- Docker documents that bind mounts used during a build are not persisted in the final image or the build cache.
- Docker documents that cache mounts are persistent and cumulative across builds, which means they should only target directories that are safe to reuse.
- Docker documents that secret material should use dedicated secret mechanisms rather than `COPY` or `ARG`, to avoid leaked credentials.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Single shared cache namespace across all workflows | Maximum cache-hit potential; simplest YAML | Weak trust boundary; easy cross-tier contamination; poor secret hygiene; one lane can overwrite another | Reject |
| `gha` only, with scope separation | Simple GitHub-native setup; Docker-recommended backend in GitHub Actions | Base/default-branch cache visibility means anything persisted there must be safe for pull request visibility; weak fit for trusted-only cache state | Accept only for PR-safe, non-sensitive caches |
| Trust-tier split: PR-safe `gha` plus trusted registry cache refs | Strong separation between PR-visible and trusted-only persistence; keeps trusted multi-stage cache state out of release images; good performance | More cache naming and registry management | Preferred |
| No persisted cache anywhere | Simplest trust model; easy incident response | Slow builds; higher CI cost; worse contributor ergonomics | Reject as default, keep as emergency fallback |

## Recommended Policy

### Core rule

Build caches are performance artifacts, not trust artifacts. A trusted workflow may import only cache locations reserved for its trust tier, and release correctness must never depend on a cache hit.

### Cache trust classes

#### Class 1: PR-visible caches

These are cache locations that are visible to pull request workflows under GitHub's cache access rules, including default-branch and base-branch GitHub Actions caches.

Examples:

- public package-manager download caches
- public dependency layer reuse
- compiler or toolchain caches that contain no credentials, no private packages, and no secret-derived state

Policy:

1. Persist only material that is safe for pull request visibility.
2. Never store credentials, auth-bearing config, private package downloads, or trusted release payloads here.
3. Trusted jobs may use these caches only as optional performance hints, never as proof that an input is trusted.

#### Class 2: Trusted-only persistent caches

These are cache locations reserved for protected-branch, release, or approved trusted reusable workflows.

Examples:

- registry-backed multi-stage cache refs for protected branches
- cache mounts holding private package downloads for trusted jobs
- `mode=max` cache exports from trusted integration or release builds

Policy:

1. Store these caches only in locations not read by untrusted workflows.
2. Allow writes only from trusted workflows on protected refs or approved trusted reusable workflows.
3. Never let pull request validation import or export these locations.

#### Class 3: Non-persistent transient inputs

These are build inputs that must remain outside persisted cache entirely.

Examples:

- BuildKit secret mounts
- SSH mounts
- generated short-lived credentials
- large source trees or fixture corpora mounted only to generate artifacts

Policy:

1. Keep these inputs transient by using secret mounts, SSH mounts, or bind mounts.
2. Do not export them through any cache backend.

### Backend policy

#### `gha`

1. Use for pull request validation and other non-sensitive GitHub-hosted acceleration.
2. Set `scope` by trust tier, image target, and lane so unrelated builds do not overwrite one another.
3. Assume anything persisted here may be visible to pull request workflows under GitHub's cache access rules.
4. Do not use `gha` as the canonical trusted release cache.

#### `registry`

1. Use as the preferred persistent backend for trusted integration and release jobs.
2. Keep cache refs separate from final image refs.
3. Separate refs by trust tier and branch family so pull request and release lanes never write the same cache location.
4. Allow `mode=max` only on trusted-only registry cache refs.

#### `local`

1. Accept for local development and for isolated trusted ephemeral builders.
2. Disallow persistent local caches on reused self-hosted runners that span repositories or trust tiers.
3. If a self-hosted runner exception exists, wipe builder state, local caches, and mounted secrets after each job.
4. Follow [TRUSTED_RUNNER_STATE_DISPOSAL.md](TRUSTED_RUNNER_STATE_DISPOSAL.md) for the wider disposal contract around workspaces, processes, auth-bearing config, and Docker residue.

#### `inline`

1. Do not use for the CI release path or for shared trusted cache state.
2. Reserve for local or explicitly one-tier scenarios where embedding cache into the image is intentional.

### Safe-to-persist content rules

| Content | PR-visible `gha` | Trusted registry or isolated local cache | Transient only | Rationale |
|---|---|---|---|---|
| Public package-manager download cache with no credentials | Yes | Yes | No | Safe if it contains only public artifacts and no auth-bearing files |
| Private package downloads or vendor blobs | No | Yes | No | May expose private source or trusted-only inputs |
| Auth-bearing config such as `.npmrc`, token files, `pip.conf`, `.cargo/credentials`, `.netrc`, or SSH keys | No | No | Yes | Secret material must never be persisted in caches |
| Layers produced by steps that used secret mounts, SSH mounts, or private Git auth | No | No | Yes | Sensitive inputs must stay out of persisted cache state |
| Intermediate trusted multi-stage build layers | No | Yes | No | Good candidate for trusted-only `registry` cache reuse |
| Release artifacts, SBOMs, attestations, checksums, or deployment scripts | No | No | Yes | These belong in artifact or release storage, not cache storage |
| Large source or fixture inputs used only to generate an artifact | No | No | Yes | Bind mounts avoid polluting the cache with transient inputs |

### Cache poisoning prevention rules

1. No trusted workflow may import an untrusted pull request cache namespace, pull request merge-ref cache, or pull-request-owned registry cache ref.
2. Workflow files that define cache destinations must stay under CODEOWNERS protection and use SHA-pinned third-party actions.
3. Cache destinations must be named by trust tier and image target so a change in one lane cannot silently overwrite another lane.
4. Keep sensitive or bulky transient inputs out of cache by using BuildKit secrets, SSH mounts, and bind mounts.
5. For package managers that require exclusive access to cache directories, use the package-manager-specific locking mode rather than sharing a mutable directory unsafely.
6. Release workflows must remain correct from a cold cache; a cache miss is a performance event, not a correctness failure.

### Cache invalidation and incident response

1. Prefix cache scopes or refs with an operator-controlled cache epoch so maintainers can cut over to a fresh namespace after poisoning, leakage, or a major toolchain change.
2. Because GitHub cache entries are immutable, invalidation requires a new key or scope rather than rewriting an old entry.
3. For registry-backed caches, invalidation means writing to a new cache ref and stopping imports from the old one.
4. If a cache leak or poisoning event is suspected, stop importing the affected namespace first and investigate second.

## Final Recommendation Stack

1. Treat build caches as untrusted performance hints, not as provenance or release evidence.
2. Separate cache destinations by trust tier, image target, and branch family; never let pull request and trusted lanes write the same location.
3. Use `gha` only for pull-request-safe, non-sensitive caches and assume those caches are visible under GitHub's pull request access rules.
4. Use private registry-backed cache refs as the canonical persistent cache for trusted integration and release jobs.
5. Keep secrets, auth-bearing config, private Git credentials, and secret-mounted steps out of all persisted caches.
6. Disallow `inline` cache in the CI release path and disallow persistent `local` caches on reused runners.
7. Keep release workflows correct from a cold cache and maintain a simple cache-epoch rotation mechanism for incident response.
8. Let trusted workflows import only documented trusted cache refs or explicitly PR-safe cache scopes, never pull-request-owned cache namespaces.

## Three High-Value Next Design Areas

1. Deployment-time provenance enforcement: define whether attestation verification stops at CI or is enforced again by deployment automation or cluster admission policy.
2. Builder incident response: define how trusted builders are quarantined, reimaged, and cache-rotated after suspected cache poisoning or secret exposure.
3. Public-cache minimization: define how far pull-request-safe cache sharing should be reduced to limit metadata and dependency exposure without losing too much performance.

## Official Sources

- GitHub Docs - Dependency caching reference: https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching
- GitHub Docs - Secure use reference: https://docs.github.com/en/actions/reference/security/secure-use
- GitHub Docs - Events that trigger workflows: https://docs.github.com/actions/using-workflows/events-that-trigger-workflows
- Docker Docs - Cache storage backends: https://docs.docker.com/build/cache/backends/
- Docker Docs - GitHub Actions cache backend: https://docs.docker.com/build/cache/backends/gha/
- Docker Docs - Registry cache backend: https://docs.docker.com/build/cache/backends/registry/
- Docker Docs - Optimize cache usage in builds: https://docs.docker.com/build/cache/optimize/