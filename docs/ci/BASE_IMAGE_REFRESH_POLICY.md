# Base Image Refresh Policy

## Overview

This document defines how release Dockerfiles choose, pin, refresh, and emergency-update their base images. It complements:

- [DOCKER_BUILD_RELEASE.md](../operations/DOCKER_BUILD_RELEASE.md) - build and publication workflow, attestations, and registry strategy
- [OS_HARDENING.md](../operations/OS_HARDENING.md) - host minimums, container hardening, and platform compatibility
- [API_SECURITY.md](../security/API_SECURITY.md) - supply-chain posture, SBOM expectations, and dependency auditing
- [RELEASE_ENGINEERING.md](RELEASE_ENGINEERING.md) - versioning rules and release-class boundaries

The design goal is to keep the runtime image small and predictable without relying on mutable tags or ad hoc operator rebuilds. The underlying system should prefer supported upstream branches, cryptographically pinned inputs, and auditable update workflows.

## Goals

1. Make release builds reproducible and auditable.
2. Consume upstream security fixes quickly without silently changing published images.
3. Stay on supported distro branches and avoid unsupported package repositories.
4. Keep the default runtime image minimal and easy for operators to understand.
5. Separate scheduled freshness updates from emergency CVE response.

## Official Research Findings (May 2026)

### Docker base-image selection, pinning, and rebuild behavior

- Docker's build best-practices guidance says to choose trusted and minimal base images, highlighting Docker Official Images and Verified Publisher images as preferred sources.
- Docker documents that images are immutable snapshots and should be rebuilt regularly to stay current on base images, libraries, and other build inputs.
- Docker documents `--pull` as the mechanism to fetch a newer base image even when one is cached locally.
- Docker documents that `--no-cache` forces a clean rebuild of all layers, and that `--pull --no-cache` is the combination for a completely fresh build.
- Docker documents that tags are mutable, while fully securing supply-chain integrity requires pinning base images to a specific digest.
- Docker documents tag-plus-digest references such as `alpine:3.21@sha256:...` as the pattern that keeps the human-readable version intent while making the actual content immutable.
- Docker Scout's Up-to-Date Base Images policy checks whether the pinned base-image version is still current for its tag.
- Docker Scout remediation can raise GitHub pull requests that update Dockerfiles to the latest base-image digest, which preserves review and audit trail instead of relying on silently moving tags.
- Docker build policies can require canonical digest references, validate exact checksums, restrict allowed registries, and require provenance on trusted images.

### GitHub automation for Docker dependencies

- GitHub Dependabot officially supports the `docker` package ecosystem.
- GitHub documents scheduled version checks for Docker dependencies with intervals such as `daily`, `weekly`, `monthly`, and `cron`.
- GitHub documents `cooldown` support for Docker updates, which can reduce PR churn for normal version updates without suppressing security updates.

### Alpine lifecycle and support posture

- Alpine Linux documents that stable release branches are created every May and November.
- Alpine Linux documents that the `main` repository is typically supported for two years and the `community` repository is supported until the next stable release.
- Alpine Linux documents that `edge` is a development branch and explicitly warns against using it for production or deterministic containerized environments.
- Alpine Linux's releases page shows that the current stable branch as of May 2026 is `v3.23`.
- Alpine Linux documents that support status is branch-specific, which means staying on an older branch is an explicit support decision rather than a neutral default.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Minor tag only, such as `alpine:3.23` | Simple, automatically picks up upstream patch refreshes on rebuild | Mutable input, weak audit trail, no deterministic reproduction, tag can move unexpectedly | Reject for release builds |
| Tag plus digest with manual updates | Deterministic, auditable, compatible with Docker guidance | Manual lookup and rotation work, easy to fall behind on fixes | Acceptable fallback, not preferred |
| Tag plus digest with Docker Scout remediation and CI enforcement | Deterministic, reviewable, auditable, aligns with Docker Scout policy model, keeps human approval in the loop | Requires Scout integration and workflow ownership | Preferred |
| Distroless or Docker Hardened Images track | Smaller attack surface, stronger upstream hardening story | Harder debugging, operational change for self-hosters, not all workflows need it | Optional future hardening track |

## Recommended Policy

### Base image allowlist

1. Release Dockerfiles may only use runtime and builder base images from trusted sources: Docker Official Images, Verified Publisher images, Docker Hardened Images, or an internal mirror of those images.
2. The default runtime base remains Alpine Linux because it is small, well-understood by self-hosters, and compatible with the existing musl-oriented build target.
3. The runtime base must use the current Alpine stable branch after the defined soak window. As of May 2026, that baseline branch is `3.23`.
4. `edge` is prohibited for release images.
5. Remaining on the previous Alpine stable branch is only allowed by documented exception, and only while compatibility blockers are being removed.

### Pinning contract

1. Every external base image in a release Dockerfile must use the `name:tag@sha256:digest` form.
2. The tag communicates the intended upstream release line; the digest makes the actual content immutable.
3. Tag-only references are not allowed in release Dockerfiles, even when the tag is a stable minor line such as `alpine:3.23`.
4. CI should reject non-allowlisted registries and tag-only external base images.
5. Where Docker build policies are enabled, the policy should require canonical image references or checksum matches for all external images.

### Branch selection contract

1. The default policy is to move to the newest Alpine stable branch after a 14-day soak window and compatibility sweep.
2. If the runtime image depends on Alpine `community` packages, the project should not intentionally remain on a branch after the next stable release because community support ends at that point.
3. If the runtime image only depends on `main` packages, a previous stable branch can be tolerated temporarily, but only with a tracked exception and an explicit migration owner.
4. Branch hops must be tested through the existing release-quality gates before publication.

### Refresh cadence

1. Every CI build that publishes or rehearses a publishable image must use `--pull` so cached base images do not mask upstream refreshes.
2. A scheduled weekly job should evaluate base-image freshness and open a pull request when Docker Scout or the fallback updater detects a newer approved digest.
3. At least once per month, the release workflow should perform a clean image rebuild with `--pull --no-cache` so the full image is re-evaluated against current upstream content and package indexes.
4. A new Alpine stable branch must be reviewed within 14 days of release and either adopted or explicitly deferred with a recorded blocker.
5. If Docker Scout GitHub remediation is unavailable, GitHub Dependabot should run at least weekly for the `docker` ecosystem as the fallback signal for Dockerfile updates.

### CVE response rules

Use fix availability, not raw CVE count alone, as the operational trigger. Docker Scout and upstream distro support status determine whether an actionable fix exists.

| Condition | Response target | Required action |
|---|---|---|
| Critical or actively exploited base-image vulnerability with an upstream fix available on a supported branch | Start immediately, merge and rebuild within 24 hours | Refresh the pinned digest or move to a newer supported branch, rerun release gates, publish a new release artifact |
| High-severity base-image vulnerability with an upstream fix available on a supported branch | Merge and rebuild within 72 hours | Refresh the pinned digest, rerun release gates, publish a new release artifact |
| Medium-severity fixable base-image vulnerability | Next weekly refresh, no later than 14 days | Include in the next scheduled base-image refresh PR and rebuild cycle |
| Low-severity fixable base-image vulnerability | Next monthly refresh, no later than 30 days | Roll into the monthly clean rebuild unless exposure analysis justifies faster action |
| No upstream fix available yet | Track until fixed; do not churn tags without a real fix | Record the exception, verify exploitability and exposure, apply compensating controls where available, and re-check on each scheduled scan |

If the current branch no longer receives the necessary fix but the newer supported branch does, the response is to move branches rather than waiting on the older branch.

### Versioning and publication rules

1. Published stable application versions must not be silently repointed to a new image digest after base-image changes.
2. A base-image-only refresh for a stable release requires a new application release artifact and updated supply-chain evidence.
3. SHA tags and CI snapshot tags remain immutable by digest; channel tags may move, but they must always point at already-published immutable digests.
4. Base-image refresh pull requests must record the old and new base-image digests in the PR body or workflow summary.

### Enforcement and evidence

1. Docker Scout Up-to-Date Base Images policy should run on release-candidate and stable-release images.
2. The base-image policy should be evaluated alongside provenance and SBOM checks so freshness does not weaken supply-chain evidence.
3. GitHub-hosted automation should create the digest-refresh PR rather than allowing local ad hoc updates.
4. Release evidence should include the effective base-image reference, the resulting image digest, and the policy status for freshness and vulnerabilities.

## Final Recommendation Stack

1. Use Alpine's current stable branch for the runtime image, not `edge`, and treat `3.23` as the baseline branch at the time of this May 2026 research.
2. Pin every release base image as `tag@sha256:digest`, never tag-only.
3. Enforce trusted registries and digest usage in CI, with Docker build policies where practical.
4. Use Docker Scout Up-to-Date Base Images plus GitHub remediation pull requests as the primary refresh mechanism.
5. Run weekly freshness review, monthly clean rebuilds, and per-build `--pull` for publishable images.
6. Treat fixable critical base-image vulnerabilities as 24-hour work and high-severity fixable vulnerabilities as 72-hour work.
7. Never republish a stable application version to a new digest after a base-image change; publish a new release artifact instead.

## Three High-Value Next Design Areas

1. Builder trust boundary and private dependency ingress: define how builders authenticate to private package sources, which runners may access those credentials, and how untrusted pull requests are isolated.
2. Runtime package exception policy: define when adding shell tools, package managers, or debugging utilities to the runtime image is allowed, and what approval path is required.
3. Deployment-side admission verification: define how operators or future orchestrated deployments verify image attestations, signatures, and approved registries before runtime.

## Official Sources

- Docker Docs - Building best practices: https://docs.docker.com/build/building/best-practices/
- Docker Docs - Validating image inputs: https://docs.docker.com/build/policies/validate-images/
- Docker Docs - Remediation with Docker Scout: https://docs.docker.com/scout/policy/remediation/
- GitHub Docs - Dependabot options reference: https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference
- Alpine Linux - Release branches: https://alpinelinux.org/releases/
- Alpine Linux Wiki - Repositories: https://wiki.alpinelinux.org/wiki/Repositories