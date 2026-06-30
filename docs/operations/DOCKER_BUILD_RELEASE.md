# Docker Build & Release Process

## Overview

This document defines how container images are built, tagged, attested, cached, and published. It complements:

- [DOCKER_DEPLOYMENT.md](DOCKER_DEPLOYMENT.md) - runtime container architecture and operator deployment defaults
- [RELEASE_ENGINEERING.md](../ci/RELEASE_ENGINEERING.md) - versioning, release classes, and rollback boundaries
- [CI_TESTING.md](../ci/CI_TESTING.md) - quality gates and publish preconditions
- [BASE_IMAGE_REFRESH_POLICY.md](../ci/BASE_IMAGE_REFRESH_POLICY.md) - digest pinning, refresh cadence, and base-image CVE response rules
- [BUILDER_TRUST_BOUNDARY.md](../ci/BUILDER_TRUST_BOUNDARY.md) - runner trust tiers, self-hosted runner exceptions, and private dependency ingress rules
- [SECRET_BROKERAGE_ROTATION.md](../ci/SECRET_BROKERAGE_ROTATION.md) - credential-source order, protected secret-bearing workflow rules, and broker rotation policy
- [PRIVILEGED_ARTIFACT_HANDOFF.md](../ci/PRIVILEGED_ARTIFACT_HANDOFF.md) - metadata-only cross-boundary handoff and trusted rebuild or verification rules
- [BUILD_CACHE_TRUST_BOUNDARY.md](../ci/BUILD_CACHE_TRUST_BOUNDARY.md) - PR-visible versus trusted-only cache policy, poisoning prevention, and safe-to-persist rules
- [REGISTRY_CACHE_RETENTION.md](../ci/REGISTRY_CACHE_RETENTION.md) - dedicated cache package design, retention windows, and cleanup ownership
- [RELEASE_ARTIFACT_RETENTION.md](../ci/RELEASE_ARTIFACT_RETENTION.md) - durable release evidence, checksum manifests, and rollback-proof retention rules
- [API_SECURITY.md](../security/API_SECURITY.md) - dependency auditing, SBOM expectations, and supply-chain posture

The design goal is to keep the Docker release path close to Classifarr's operational shape where that helps operators, while grounding the actual build and publication mechanics in current official Docker and GitHub guidance.

## Goals

1. Produce small, predictable, multi-architecture images for `linux/amd64` and `linux/arm64`.
2. Keep release publication inside protected GitHub Actions workflows instead of ad hoc local pushes.
3. Generate verifiable supply-chain metadata for published images.
4. Avoid leaking credentials into image layers, build logs, or image history.
5. Keep the default operator install path simple: one tagged image, one manifest list, one registry source of truth.

## Official Research Findings (May 2026)

### Docker build path and Dockerfile structure

- Docker documents that `docker build` uses Buildx and BuildKit by default, with the legacy builder now only relevant for Windows-container cases or when `DOCKER_BUILDKIT=0` is forced.
- Docker documents multi-stage builds as the preferred way to separate build tooling from the final runtime image.
- Docker documents that BuildKit only evaluates the stages required by the selected target, unlike the legacy builder.
- Docker documents `.dockerignore` as the mechanism for pruning build context, and supports Dockerfile-specific ignore files when multiple Dockerfiles exist.

### Multi-platform publication

- Docker documents multi-platform images as a manifest list that points to per-platform manifests.
- Docker documents `--platform linux/amd64,linux/arm64` as the standard multi-platform publish flow.
- Docker documents three strategies for multi-platform builds: QEMU emulation, multiple native nodes, and cross-compilation.
- Docker documents that QEMU is the easiest path but can be much slower for compilation-heavy workloads.

### Secrets and private inputs

- Docker documents that build arguments and environment variables are inappropriate for secrets because they persist in the final image.
- Docker documents secret mounts, SSH mounts, and Git authentication secrets as the supported secure mechanisms.
- Docker documents `GIT_AUTH_TOKEN` as the standard GitHub-oriented secret for private Git contexts.

### Cache behavior in GitHub Actions

- Docker documents the `gha` cache backend as the recommended cache backend inside GitHub Actions, within GitHub's size and retention limits.
- Docker documents that the `gha` cache backend is not supported with the default `docker` driver; a separate Buildx builder is required.
- Docker documents that cache scope must be separated per image or target to avoid builds overwriting each other's cache.
- Docker documents that `docker/build-push-action` auto-populates the GitHub cache authentication values.

### Attestations and registry persistence

- Docker documents that minimal provenance attestations are added by default by BuildKit.
- Docker documents explicit `--sbom` and `--provenance=mode=max` controls for richer metadata.
- Docker documents that attestations persist for the default Docker image store only when the image is pushed to a registry, unless a containerd-backed store is used.
- GitHub documents the `actions/attest` flow for container images, with `id-token: write`, `contents: read`, `attestations: write`, and `packages: write` as the required permissions.

### GitHub workflow security and package publication

- GitHub documents least-privilege `GITHUB_TOKEN` permissions and notes that unspecified permissions become `none` once any permission block is declared.
- GitHub documents pinning third-party actions to a full commit SHA as the strongest integrity option.
- GitHub documents `GITHUB_TOKEN` as the recommended authentication mechanism for GHCR instead of long-lived personal access tokens.
- GitHub documents optional `artifact-metadata: write` when attested artifacts should also appear in linked artifacts metadata.

### Platform note on Docker Hub automation

- Docker Hub release notes state that Docker Hub Automated Builds were deprecated in May 2026, with migration guidance toward GitHub Actions or Bitbucket Pipelines.

## Design Options

### 1. Build Orchestration Surface

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Docker Hub Automated Builds | Low conceptual overhead; historically familiar | Deprecated as of May 2026; weak fit for protected release gates; poorer integration with existing GitHub-centric CI evidence | Rejected |
| Repo-owned GitHub Actions using official Docker actions | Full control over gates, tags, permissions, and release conditions; best fit with existing CI and release docs; easiest GHCR integration | More workflow YAML to maintain in-repo | Selected |
| Docker GitHub Builder reusable workflow | Docker-maintained build implementation; centralizes BuildKit, caching, provenance, and manifest assembly | Extra abstraction boundary; less direct repo-local control; new external coupling to Docker's reusable workflow contract | Defer as future simplification option |

### 2. Multi-platform Build Strategy

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| QEMU-only | Easiest to start; no Rust-specific cross-build design needed | Slow for compile-heavy Rust workloads; longer release times; more CI minutes | Rejected as default |
| Managed native builders / Build Cloud | Best performance; native per-arch builds; shared cache | Extra platform dependency and cost; not required for initial scale | Deferred |
| Buildx manifest publication with language-aware cross-compilation | Fastest path without extra platform service; maps well to Rust; keeps one protected release workflow | Needs deliberate Dockerfile staging and target wiring | Selected |

### 3. Registry Strategy

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| GHCR only | Native GitHub Actions auth; best alignment with attestations and package permissions; single source of truth | Some users still prefer Docker Hub discovery | Selected initial source of truth |
| GHCR primary plus Docker Hub mirror | Broader discoverability; keeps GitHub-native build flow while supporting Docker Hub users | More publish complexity; extra tag drift risk; another registry to validate | Optional follow-up |
| Docker Hub only | Familiar user experience | Worse fit for GitHub-native attestation flow and token model; no reason to make GitHub secondary | Rejected |

## Recommended Workflow Shape

### A. Pull request lane

Purpose: validate Dockerfile and packaging behavior without publishing.

- Trigger on Dockerfile, container-script, workflow, or relevant source changes.
- Use Buildx with a dedicated builder created by `docker/setup-buildx-action`.
- Run a packaging build with `push: false`.
- Prefer single-platform `linux/amd64` packaging validation in PRs unless the change is explicitly multi-arch-sensitive.
- Import and export cache with a per-image scope, for example `scope=server-runtime`.
- Run build configuration validation before expensive publication paths when useful, using BuildKit checks.
- Do not rely on publish secrets for forked pull requests.

### B. Mainline packaging lane

Purpose: keep container packaging healthy between releases.

- Build on protected branches after the fast PR lane passes.
- Use `pull: true` so release candidates refresh base-image references instead of silently reusing stale local parents.
- Rebuild the runtime image through the same target graph used by release publication.
- Produce machine-readable build metadata and keep it as workflow evidence.
- Keep publication optional here; if used, publish only non-canonical canary tags such as branch or SHA tags.

### C. Release publication lane

Purpose: produce the operator-facing image.

- Trigger only from protected SemVer tags and optionally manual dispatch for controlled rebuilds.
- Require the release gates already defined in [CI_TESTING.md](../ci/CI_TESTING.md) to pass before publication.
- Use Buildx to publish a single manifest list for `linux/amd64` and `linux/arm64`.
- Push to GHCR first and treat the resulting digest as the source-of-truth release artifact.
- Generate explicit SBOM attestations and max-detail provenance.
- Generate GitHub artifact attestations with `actions/attest` using the pushed digest.
- Publish durable release evidence manifests and checksums according to [RELEASE_ARTIFACT_RETENTION.md](../ci/RELEASE_ARTIFACT_RETENTION.md), not only as workflow artifacts.
- Optionally mirror to Docker Hub only after the GHCR publish and attestation steps succeed.

## Dockerfile and Build Rules

### Multi-stage contract

1. Build dependencies live only in builder stages.
2. Final runtime stage contains only the application, adapter-node web runtime, embedded PostgreSQL runtime components, and the minimum runtime utilities required by the entrypoint.
3. Every important stage is named explicitly.
4. Release workflows build the explicit runtime stage, not the first unnamed stage.
5. Debug or smoke-test stages may exist, but they are not publish targets.

### Current Dockerfile implementation

The Phase 15 Task 1 Dockerfile uses these release-stage boundaries:

| Stage | Purpose |
|---|---|
| `web-deps` | Installs SvelteKit dependencies from `clients/web/package-lock.json` with an npm cache mount. |
| `web-builder` | Produces the adapter-node web artifact under `clients/web/build`. |
| `rust-builder` | Builds the `duskcue` server binary on Alpine for `linux/amd64` and `linux/arm64` Buildx targets. |
| `runtime` | Publishes the Alpine runtime image with Duskcue, web output, PostgreSQL 18 packages, FFmpeg, Node.js, `tini`, `su-exec`, and documented volume paths. |

All external base images in the Dockerfile use Docker Official Images with `tag@sha256:digest` references. The current baseline is Alpine `3.24` per [BASE_IMAGE_REFRESH_POLICY.md](../ci/BASE_IMAGE_REFRESH_POLICY.md).

### Context hygiene

1. A root `.dockerignore` is required.
2. If additional Dockerfiles are introduced, Dockerfile-specific `.dockerignore` files should be used where context needs differ.
3. Large test fixtures, restore snapshots, and local development state must stay out of the default build context unless intentionally mounted as named contexts.
4. Named contexts are allowed for special inputs, but the default build should stay understandable from the main Dockerfile alone.

### Secret handling

1. Never pass credentials with `ARG` or `ENV` when they are secrets.
2. Use `--secret` plus `RUN --mount=type=secret` for token or file-based build inputs.
3. Use `--ssh` plus `RUN --mount=type=ssh` for SSH-based Git fetches.
4. Use `GIT_AUTH_TOKEN` for private GitHub HTTP contexts when a remote Git context is required.
5. Follow [SECRET_BROKERAGE_ROTATION.md](../ci/SECRET_BROKERAGE_ROTATION.md) for how those credentials are sourced, scoped, and rotated before they reach BuildKit.

## Multi-architecture Strategy

### Decision

Publish `linux/amd64` and `linux/arm64` from one Buildx manifest list, using Rust cross-compilation-friendly stages where possible and avoiding QEMU as the default compilation path.

### Why this fits the project

- The project already targets x86_64 and ARM64 in [PROJECT.md](../../PROJECT.md) and uses Rust as the primary backend.
- Rust cross-compilation is more predictable than emulated full-system compilation for recurring release builds.
- The runtime image stays single-name for operators, which matches the simplicity goal already established in [DOCKER_DEPLOYMENT.md](DOCKER_DEPLOYMENT.md).

### Guardrails

1. QEMU remains acceptable for fallback validation or rare dependency edge cases.
2. If release duration or cache churn becomes material, native multi-node builders or Docker Build Cloud can be revisited.
3. The release workflow should verify the produced manifest list, not just one platform image.

## Tagging and Metadata Contract

### Image tags

- Exact version tag for every published release, for example `1.2.3` or `1.2.3-beta.1`
- Immutable digest as the real deployment anchor
- `1.2` and `1` moving tags only for stable releases
- `latest` only for the current stable release, never for alpha, beta, or rc channels
- Optional SHA tags for diagnostics and rollback evidence

### OCI metadata

Published images should carry OCI metadata for at least:

- source repository
- description
- licenses
- revision
- version
- created timestamp

For multi-architecture images, manifest-level description annotations should also be set so the GHCR package page reflects the intended description.

## Cache Strategy

### Decision

Use a trust-tiered cache policy: `gha` for PR-safe non-sensitive acceleration and dedicated trusted registry-backed cache refs for trusted integration and release lanes.

### Why

- Docker recommends `gha` inside GitHub Actions, but GitHub cache visibility rules mean anything stored there must be safe for pull request visibility.
- Registry cache keeps trusted multi-stage cache state separate from the published image and out of PR-visible cache storage.
- Explicit separation by scope or ref prevents one image target or trust tier from overwriting another.

### Constraints

1. Use a non-default Buildx builder driver.
2. Separate cache namespaces by trust tier and image target so untrusted and trusted lanes never write the same location.
3. Keep secrets, private dependency material, and secret-mounted steps out of persisted caches.
4. Follow [BUILD_CACHE_TRUST_BOUNDARY.md](../ci/BUILD_CACHE_TRUST_BOUNDARY.md) for backend-specific and safe-to-persist rules.
5. Follow [REGISTRY_CACHE_RETENTION.md](../ci/REGISTRY_CACHE_RETENTION.md) for dedicated cache package naming, retention windows, and cleanup ownership.

## Publication Permissions Contract

The publish job should own the expanded token permissions, not the whole workflow.

Baseline publish-job permissions:

```yaml
contents: read
packages: write
attestations: write
id-token: write
artifact-metadata: write
```

`artifact-metadata: write` is optional unless linked-artifact metadata is desired.

All non-publish jobs should declare narrower permissions.

## Final Recommendation Stack

1. Use a repo-owned GitHub Actions release workflow with official Docker actions pinned to full commit SHAs.
2. Use Buildx and BuildKit for all Linux image builds; do not design around the legacy builder.
3. Keep a multi-stage Dockerfile with explicit named stages and a minimal runtime publish target.
4. Publish one GHCR multi-architecture manifest list for `linux/amd64` and `linux/arm64`.
5. Use Rust-friendly cross-compilation as the default multi-arch strategy, with QEMU only as fallback.
6. Use `.dockerignore` and per-Dockerfile ignore files to keep build context tight.
7. Use a trust-tiered cache model: `gha` only for PR-safe non-sensitive caches, and separate trusted registry cache refs for protected-branch and release reuse.
8. Never pass secrets through build args or plain environment variables; use BuildKit secrets and SSH mounts.
9. Generate explicit SBOM plus max-detail provenance for published images, then create GitHub artifact attestations from the pushed digest.
10. Treat GHCR as the canonical release registry; add Docker Hub mirroring only after the GHCR-first flow is stable.
11. Govern runtime base-image freshness separately through [BASE_IMAGE_REFRESH_POLICY.md](../ci/BASE_IMAGE_REFRESH_POLICY.md) rather than leaving it implicit in Dockerfile tags.
12. Govern trusted builders and private dependency access separately through [BUILDER_TRUST_BOUNDARY.md](../ci/BUILDER_TRUST_BOUNDARY.md) rather than allowing secrets and self-hosted access to sprawl across workflows.
13. Govern any cross-boundary artifact handoff separately through [PRIVILEGED_ARTIFACT_HANDOFF.md](../ci/PRIVILEGED_ARTIFACT_HANDOFF.md) so untrusted workflow outputs never become release payloads without trusted rebuild or verified trusted provenance.

## Three High-Value Next Design Areas

1. Artifact promotion model: define whether beta images can be promoted by digest into stable channels or whether every stable image must always be rebuilt from source.
2. Secret brokerage and rotation: define how trusted build and release workflows obtain short-lived registry or cloud credentials without expanding GitHub-stored secret scope.
3. Secondary registry and archive strategy: define whether older release evidence or published images should be mirrored outside GHCR for continuity and end-of-life archival.

## Official Sources

- Docker CLI reference - `docker buildx build`: https://docs.docker.com/reference/cli/docker/buildx/build/
- Docker CLI reference - legacy `docker image build`: https://docs.docker.com/reference/cli/docker/image/build/
- Docker Docs - Multi-stage builds: https://docs.docker.com/build/building/multi-stage/
- Docker Docs - Multi-platform builds: https://docs.docker.com/build/building/multi-platform/
- Docker Docs - Build secrets: https://docs.docker.com/build/building/secrets/
- Docker Docs - Build context and `.dockerignore`: https://docs.docker.com/build/concepts/context/
- Docker Docs - GitHub Actions cache backend: https://docs.docker.com/build/cache/backends/gha/
- Docker Docs - Docker Build GitHub Actions: https://docs.docker.com/build/ci/github-actions/
- Docker Docs - Build attestations: https://docs.docker.com/build/metadata/attestations/
- Docker Hub release notes - Automated Builds deprecation: https://docs.docker.com/docker-hub/release-notes/
- GitHub Docs - Workflow syntax and permissions: https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax
- GitHub Docs - Secure use reference: https://docs.github.com/en/actions/reference/security/secure-use
- GitHub Docs - Using artifact attestations: https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations
- GitHub Docs - Publishing packages with GitHub Actions: https://docs.github.com/en/packages/managing-github-packages-using-github-actions-workflows/publishing-and-installing-a-package-with-github-actions
