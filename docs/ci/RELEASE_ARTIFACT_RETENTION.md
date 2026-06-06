# Release Artifact Retention & Rollback Evidence

## Overview

This document defines which release artifacts and evidence are durable, where they live, how long they are retained, and which records are required to support rollback, audit, and operator recovery. It complements:

- [RELEASE_ENGINEERING.md](RELEASE_ENGINEERING.md) - release classes, compatibility boundaries, and when rollback becomes restore or PITR instead of binary-only rollback
- [DOCKER_BUILD_RELEASE.md](../operations/DOCKER_BUILD_RELEASE.md) - image build, tag, publish, SBOM, and provenance generation rules
- [CI_TESTING.md](CI_TESTING.md) - restore drills, migration rehearsal evidence, and release quality gates
- [REGISTRY_CACHE_RETENTION.md](REGISTRY_CACHE_RETENTION.md) - disposable registry cache lifecycle that must stay separate from release retention
- [PRIVILEGED_ARTIFACT_HANDOFF.md](PRIVILEGED_ARTIFACT_HANDOFF.md) - trusted rebuild defaults and rules for which workflow outputs may become release evidence

The design goal is to keep rollback evidence durable and reviewable without confusing it with short-lived workflow artifacts, build cache state, or disposable CI diagnostics.

## Goals

1. Define the minimum durable evidence set required to support a documented rollback posture.
2. Separate durable release evidence from disposable workflow artifacts and disposable cache storage.
3. Keep release recovery anchored to immutable digests and verifiable metadata rather than mutable tags or workflow UI state.
4. Ensure deletion and cleanup paths are least-privilege, auditable, and unlikely to remove supported release evidence by mistake.
5. Preserve enough evidence to re-verify what was shipped even after routine CI retention windows expire.

## Official Research Findings (May 2026)

### GitHub workflow artifacts are temporary workflow storage

- GitHub documents workflow artifacts as files produced during a workflow run that persist after a job completes, but explicitly distinguishes them from dependency caches and says the two are not interchangeable.
- GitHub documents that when a workflow run is deleted, all artifacts associated with that run are also deleted from storage.
- GitHub documents artifact and log retention as configurable, with a default of 90 days and a configurable range of 1 to 400 days, and notes that updated settings apply only to new artifacts and logs rather than retroactively changing existing objects.

### GitHub artifact attestations provide provenance, not durable release storage by themselves

- GitHub documents artifact attestations as cryptographically signed provenance claims that can describe where and how software was built.
- GitHub documents that attestations can include an associated SBOM.
- GitHub documents offline verification and attestation management surfaces, which means attestations should be treated as evidence that must remain verifiable even if a particular GitHub UI surface changes.
- GitHub recommends uploading attested assets to linked artifact views for discovery, but those views are not the project's sole durability contract.

### Docker and OCI metadata make registry-pushed images the durable container evidence anchor

- Docker documents build attestations as OCI-compliant metadata attached to images.
- Docker documents SBOM and provenance generation as first-class build outputs.
- Docker documents that attestations persist reliably for the default Docker image store when images are pushed to a registry, unless a containerd-backed local store is used.

### GitHub container packages support granular permissions and operator recovery after deletion mistakes

- GitHub documents that the container registry supports granular permissions.
- GitHub documents that workflows can authenticate to the container registry with `GITHUB_TOKEN`.
- GitHub documents that repositories can be granted administrative access to package management surfaces.
- GitHub documents that deleted packages or package versions are restorable for 30 days as long as the namespace remains available.

### GitHub Releases can carry independently addressable release assets

- GitHub's GraphQL release schema exposes release assets as first-class objects with names, download URLs, and SHA-256 digests.
- GitHub's GraphQL release schema also exposes whether a release is immutable, which makes the release record an appropriate durable operator-facing index for human-consumable assets.

## Evidence Classes

### Class 1: Durable release anchors

These are the records without which a supported rollback or re-verification story is incomplete.

Examples:

- GHCR image package version identified by immutable digest
- exact SemVer image tag that resolves to that digest
- protected Git tag for the release commit
- GitHub Release record for the shipped version

Policy:

1. These records are part of the supported release lifecycle.
2. They are never cleaned up by cache-pruning or routine workflow-artifact retention jobs.
3. Rollback instructions and support statements must reference these anchors, not mutable moving tags such as `latest`.

### Class 2: Durable verification materials

These are the materials required to verify what the durable release anchors contain and how they were produced.

Examples:

- OCI SBOM attached to the pushed image
- OCI provenance attached to the pushed image
- GitHub artifact attestation for the published image digest
- checksum manifest for downloadable assets
- release asset digest manifest

Policy:

1. These materials must remain available for as long as the corresponding supported release remains available.
2. They may live in more than one surface, but the project must define one canonical durable home per evidence type.
3. If a convenience surface disappears, operators must still be able to verify the shipped release from the canonical durable stores.

### Class 3: Durable release evidence manifest

This is the compact operator-facing record that ties the release together.

Minimum contents:

- release version and protected source tag
- source commit SHA
- GHCR digest and exact package name
- published platform list
- checksum manifest reference
- SBOM and provenance reference
- GitHub attestation reference and expected verification command
- workflow run identifiers for the trusted publish run
- restore-drill and migration-rehearsal evidence identifiers used to clear the release gate
- rollback classification such as binary-compatible rollback allowed or restore/PITR required after boundary crossing

Policy:

1. This manifest is durable release evidence, not a temporary CI artifact.
2. Store it as a GitHub Release asset or another durable release-controlled asset location, not only as a workflow artifact.
3. Keep it small, structured, and stable enough for operators to inspect during incidents.

### Class 4: Short-lived workflow evidence

These are useful during investigation but are not the canonical rollback contract.

Examples:

- raw test reports
- browser traces and screenshots
- full restore logs
- raw migration job logs
- intermediate build outputs kept only to pass data between jobs

Policy:

1. Retain these on workflow-appropriate timelines only.
2. Do not require them to remain present for the release to stay supported.
3. Extract the durable facts they prove into the release evidence manifest before the workflow retention window expires.

### Class 5: Explicitly excluded artifacts

These objects are not rollback evidence.

Examples:

- `gha` caches
- registry-backed build cache refs
- local builder cache
- PR-built binaries or images
- mutable channel tags such as `latest`, `1`, or `1.2`

Policy:

1. These may improve performance or convenience.
2. They are never required for supported rollback or release verification.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Treat workflow artifacts as the primary release archive | Simple initial implementation; no new release manifest concept | Run deletion removes them; retention windows are short and configurable; weak rollback durability | Reject |
| Keep only the registry image digest and rely on live GitHub UI state for everything else | Lowest duplication; strong image anchor | Weak human-readable release package; release evidence becomes fragmented; harder operator audit story | Reject |
| Use dual durable stores: registry-backed release digest plus release-controlled evidence assets | Clear split between machine-verifiable and operator-facing evidence; durable enough for support windows; easy to reason about | Requires a compact manifest and release discipline | Preferred |
| Archive every workflow log, report, and intermediate artifact indefinitely | Maximum forensic detail | Expensive, noisy, and unnecessary for supported rollback; creates cleanup burden | Reject as default |

## Recommended Policy

### Core rule

Supported rollback evidence must survive routine workflow-artifact expiry, workflow-run deletion, and cache cleanup. If deleting a workflow run or cache ref would erase the only copy of evidence needed to understand or verify a release, that evidence is stored in the wrong place.

### Canonical durable stores

#### Container releases

1. GHCR package versions identified by immutable digests are the canonical durable release anchor for container payloads.
2. Exact SemVer image tags must continue to resolve to the retained supported digest.
3. Moving tags such as `latest`, major-only, or minor-only tags are convenience aliases and are not part of the rollback evidence contract.

#### Human-facing release record

1. Every stable release must have a GitHub Release record.
2. That release record must carry or link the compact release evidence manifest.
3. Downloadable release assets, if any, must publish checksums and retain their digests as part of the durable evidence set.

#### Verification metadata

1. SBOM and provenance for published images must be attached to the pushed image digest through OCI-capable metadata where possible.
2. GitHub artifact attestations may be used as an additional provenance surface and discovery mechanism, but they are not the only durability anchor.
3. The release evidence manifest must include verification commands or references that still make sense after the routine CI artifact window expires.

### Retention windows

#### Stable releases

1. Retain all stable release digests, exact version tags, Git tags, checksum manifests, SBOMs, provenance, and release evidence manifests for the full supported life of the release.
2. Because the project's expected stable-release volume is low and rollback clarity is more important than aggressive pruning, the default policy is to keep stable release evidence indefinitely until a later archival policy explicitly supersedes this rule.
3. If a future archival policy is introduced, it must still retain at minimum:
   - the current stable release in each supported line
   - the immediately previous stable release in each supported line
   - the last release before any incompatible migration boundary or PostgreSQL major-upgrade boundary that changes rollback rules

#### Pre-release channels

1. Retain `rc` releases and their durable evidence for at least 180 days after supersession or until the corresponding stable release has been operating successfully beyond the release freshness window, whichever is longer.
2. Retain `beta` and `alpha` releases for at least 90 days after supersession unless they are the only evidence of a materially different upgrade path still under investigation.
3. If a pre-release is used for an operator-facing migration or rollback rehearsal, retain its compact evidence manifest at least until the next stable release closes that change window.

#### Short-lived workflow evidence

1. PR logs, reports, and transient artifacts may follow the shorter CI retention windows already defined in [CI_TESTING.md](CI_TESTING.md).
2. Release-workflow raw logs and raw evidence bundles may expire on that same short timeline once the durable release evidence manifest has been published.
3. Workflow artifacts are staging or investigative storage, not the canonical long-term release archive.

### Rollback evidence requirements

For every stable release, the durable evidence set must be sufficient to answer the following questions without relying on expired workflow artifacts:

1. What exact source tag and commit produced this release?
2. What exact container digest or downloadable asset digest was published?
3. Which SBOM and provenance records describe that payload?
4. Which trusted workflow run published it?
5. Which restore drill and previous-stable migration rehearsal cleared the release gate?
6. Is binary rollback to or from this release still allowed, or does recovery require PITR or restore?

### Deletion and cleanup ownership

1. Only trusted maintainers or trusted release-maintenance workflows may delete release package versions, release assets, or durable evidence manifests.
2. Normal cleanup automation must target cache packages and short-lived workflow artifacts only, not durable release packages.
3. If package-version deletion automation exists at all, it must match only explicitly allowlisted pre-release namespaces or versions and must exclude supported stable versions.
4. Keep package-admin scope for durable release packages narrower than cache-cleanup scope whenever practical.

### Recovery after mistaken deletion

1. Treat GitHub's documented 30-day package restore window as an emergency recovery aid, not as the normal retention policy.
2. If a supported release package version or durable evidence asset is accidentally deleted, restoration is a release-maintenance incident and must be handled before any further cleanup continues.
3. The existence of a restore window does not justify aggressive deletion of supported release evidence.

### Rollback boundary alignment

1. This document does not widen the rollback boundary defined in [RELEASE_ENGINEERING.md](RELEASE_ENGINEERING.md).
2. If a release crossed an incompatible schema or PostgreSQL boundary, binary rollback may still be unsupported even when the old image and evidence remain retained.
3. Retaining the old image and evidence in that case still matters for forensic comparison, controlled rehearse-and-restore procedures, and operator documentation.

## Final Recommendation Stack

1. Treat GHCR digest-addressed image versions as the canonical durable container release anchor.
2. Publish a compact durable release evidence manifest for every stable release and store it with the GitHub Release record or equivalent durable release-controlled assets.
3. Keep SBOMs, provenance, checksums, and attestation references aligned to the same exact published digest.
4. Never rely on workflow artifacts or workflow-run existence as the only durable home for rollback evidence.
5. Keep stable release evidence indefinitely by default; revisit only through an explicit archival policy.
6. Retain `rc` evidence longer than ordinary pre-release artifacts because it may be the last verified state before a stable publish.
7. Restrict deletion automation to caches and explicitly allowlisted pre-release artifacts; exclude supported stable releases from routine cleanup.
8. Treat GitHub's package restore window as a safety net for operator mistakes, not as the system's intended retention strategy.
9. Keep rollback evidence policy separate from cache retention, workflow-artifact retention, and release-boundary rules so each lifecycle stays understandable.

## Three High-Value Next Design Areas

1. Secret brokerage and rotation: define how trusted release and maintenance workflows obtain the credentials needed to publish, verify, restore, or prune release evidence.
2. Deployment-time provenance enforcement: define whether release attestation and digest verification stop at CI or are enforced again by deployment automation and admission policy.
3. Release archival and mirror strategy: define whether older supported or end-of-life release evidence should be mirrored to secondary storage, another registry, or signed offline archives.

## Official Sources

- GitHub Docs - Workflow artifacts: https://docs.github.com/en/actions/concepts/workflows-and-actions/workflow-artifacts
- GitHub Docs - Artifact attestations concept: https://docs.github.com/en/actions/concepts/security/artifact-attestations
- GitHub Docs - Use artifact attestations: https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations
- GitHub Docs - Verify attestations offline: https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/verify-attestations-offline
- GitHub Docs - Manage attestations: https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/manage-attestations
- GitHub Docs - Managing GitHub Actions settings for a repository: https://docs.github.com/en/enterprise-server@3.20/repositories/managing-your-repositorys-settings-and-features/enabling-features-for-your-repository/managing-github-actions-settings-for-a-repository
- GitHub Docs - Working with the container registry: https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry
- GitHub Docs - About permissions for GitHub Packages: https://docs.github.com/en/packages/learn-github-packages/about-permissions-for-github-packages
- GitHub Docs - Deleting and restoring a package: https://docs.github.com/en/enterprise-server@3.20/packages/learn-github-packages/deleting-and-restoring-a-package
- GitHub Docs - GraphQL releases reference: https://docs.github.com/en/graphql/reference/releases
- Docker Docs - Build attestations: https://docs.docker.com/build/metadata/attestations/