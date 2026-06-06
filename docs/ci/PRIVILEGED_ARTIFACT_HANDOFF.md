# Privileged Artifact Handoff & Revalidation

## Overview

This document defines how data may cross from an untrusted validation workflow into a trusted workflow, and what revalidation is required before a trusted release or publication workflow can consume that data. It complements:

- [BUILDER_TRUST_BOUNDARY.md](BUILDER_TRUST_BOUNDARY.md) - runner trust tiers, secret-bearing job separation, and private dependency ingress rules
- [BUILD_CACHE_TRUST_BOUNDARY.md](BUILD_CACHE_TRUST_BOUNDARY.md) - cache backend separation and rules that keep untrusted cache state out of privileged builds
- [CI_TESTING.md](CI_TESTING.md) - validation lanes, workflow security posture, and release quality gates
- [DOCKER_BUILD_RELEASE.md](../operations/DOCKER_BUILD_RELEASE.md) - image build and publication flow
- [RELEASE_ENGINEERING.md](RELEASE_ENGINEERING.md) - release classes, protected refs, and rollback boundaries

The design goal is to let untrusted workflows produce useful evidence without allowing them to smuggle release payloads, caches, or executable artifacts into privileged contexts.

## Scope Classification

This document is **Advanced** guidance, not a baseline product requirement.

Under [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md), this document is currently retained as deferred guidance and is not part of the active first-release path.

Use it when the project has a real trust boundary between untrusted validation jobs and trusted publication or maintenance jobs. If the release path remains simple and rebuilds only from protected source in one trusted lane, this policy can stay deferred until cross-workflow evidence handoff becomes operationally necessary.

## Ownership & Review Metadata

- **Primary owner:** Project maintainer
- **Review status:** Dormant
- **Last reviewed:** 2026-06-02
- **Review cadence:** Re-review before activation; no recurring cadence while dormant
- **Review set:** [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md)
- **Review triggers:** Any material change to artifact promotion rules, workflow trust boundaries, attestation policy, trusted rebuild assumptions, or decision to activate this deferred guidance

## Goals

1. Prevent untrusted pull request outputs from becoming release artifacts by accident or convenience.
2. Preserve privilege separation between untrusted validation and trusted publication.
3. Allow limited cross-workflow evidence handoff where it is operationally useful.
4. Require trusted workflows to rebuild or cryptographically verify release artifacts before publication.
5. Keep the policy auditable and simple enough that operators can reason about it.

## Official Research Findings (May 2026)

### GitHub's privileged workflow boundary

- GitHub's secure-use reference warns that `pull_request_target` and `workflow_run` become dangerous when they process untrusted code or content.
- GitHub's secure-use reference explicitly says workflows triggered by `workflow_run` should treat artifacts uploaded from other workflows with caution.
- GitHub's events documentation states that a workflow started by `workflow_run` can access secrets and write tokens even if the earlier workflow did not, which is exactly why it is useful for privilege separation and exactly why consuming untrusted outputs there is risky.
- GitHub's events documentation warns that running untrusted code on `workflow_run` can lead to cache poisoning and unintended access to secrets or write privileges.

### Artifact transfer behavior

- GitHub documents workflow artifacts as the standard way to share data between jobs and to store outputs after a workflow completes.
- GitHub documents that artifacts are immutable in the `upload-artifact` v4 model; if a later job needs to upload a changed version, it must use a different artifact name.
- GitHub documents that `upload-artifact` returns a `digest` output containing a SHA-256 digest of the uploaded artifact archive.
- GitHub documents that `download-artifact` automatically validates that digest when it downloads the artifact and emits a warning if the digest does not match.
- GitHub documents that downloading artifacts from a different workflow or workflow run requires an explicit token and run identifier.

### Artifact attestations and verification

- GitHub documents that artifact attestations create cryptographically signed provenance claims linking an artifact to the workflow, repository, organization, environment, commit SHA, triggering event, and other OIDC-derived information.
- GitHub documents that generating attestations alone does not create security value; the attestations must be verified.
- GitHub documents that artifact attestations are not a guarantee an artifact is secure; they provide provenance and must be evaluated against policy.
- GitHub documents that you should sign released software, downloadable packages, binaries, and manifests with hashed contents, but not frequent automated test builds.
- GitHub documents `gh attestation verify` as the standard verification path for binaries and container images.

### Reusable workflows and stronger provenance

- GitHub documents that artifact attestations alone provide SLSA v1.0 Build Level 2.
- GitHub documents that reusable workflows plus artifact attestations can provide SLSA v1.0 Build Level 3 by requiring builds to run through known, vetted build instructions.
- GitHub documents that `gh attestation verify` can constrain verification to a specific reusable workflow with `--signer-workflow` or to a specific signer repository with `--signer-repo`.

### Platform availability caveat

- GitHub documents that artifact attestations are available on all current plans for public repositories, but for private or internal repositories they require GitHub Enterprise Cloud.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Direct promotion of untrusted workflow artifacts into trusted release jobs | Fastest path; no duplicate build work | Violates GitHub's caution on `workflow_run` artifacts; turns untrusted build outputs into privileged inputs; digest checks only prove transfer integrity | Reject |
| Metadata-only handoff plus trusted rebuild from protected source | Strongest trust boundary; simple reasoning model; release payload always comes from trusted source | Duplicates some build work; slower than direct promotion | Preferred default |
| Trusted-to-trusted artifact promotion with attestation verification | Avoids unnecessary rebuilds between trusted workflows; strong provenance when verifier constrains signer workflow | Only appropriate after the artifact was built in a trusted workflow; not valid for fork PR outputs; depends on attestation availability | Accept for trusted-only lanes |
| Manual maintainer approval of untrusted build outputs for release | Low automation effort | Weak auditability; error-prone; still crosses trust boundary with untrusted payloads | Reject |

## Recommended Policy

### Trust classes for workflow outputs

#### Class 1: Untrusted evidence

Produced by pull request workflows, especially from forks or any workflow that processes untrusted source changes.

Examples:

- test reports
- coverage reports
- lint results
- dependency review output
- pull request metadata
- proposed artifact names or digests

Policy:

1. Treat all such outputs as claims, not as release-ready artifacts.
2. They may inform a trusted workflow, but they must not be published, deployed, or repackaged as the production artifact.

#### Class 2: Trusted release payloads

Produced only by trusted workflows running on protected refs or by approved reusable workflows in the trusted build lane.

Examples:

- release binaries
- container images
- SBOMs that describe the release artifact
- release manifests or checksum files

Policy:

1. These artifacts may be published only if they are built in the trusted lane itself or handed off from another trusted workflow with successful provenance verification.
2. Untrusted workflows never create Class 2 artifacts for publication.

### Trigger and workflow model

1. Untrusted validation runs on `pull_request` and keeps secrets and write permissions out of scope.
2. `workflow_run` may be used for privilege separation, but the follow-up workflow must treat prior artifacts as untrusted input.
3. Release publication must trigger from protected branch pushes, protected tags, controlled `workflow_dispatch`, or trusted reusable workflow calls, not from direct promotion of a fork PR artifact.
4. A `workflow_run`-triggered trusted workflow may use untrusted outputs for commentary, triage, or policy decisions, but not as a release payload.

### Allowed cross-boundary handoff

Allowed from untrusted to trusted workflows:

1. Small metadata artifacts whose contents are independently revalidated, such as a pull request number, head SHA, changed-files summary, or machine-readable test verdict manifest.
2. Human-readable diagnostics such as logs or reports used for investigation.
3. Artifact digests or names only as advisory metadata that a trusted workflow rechecks.

Not allowed from untrusted to trusted workflows:

1. Compiled binaries, packages, or container images intended for publication.
2. Deployment manifests or scripts that a privileged workflow would execute.
3. Build caches intended for reuse in privileged jobs.
4. Release SBOMs, release checksums, or provenance documents that claim to describe production artifacts.

### Required revalidation before trusted consumption

If a trusted workflow reads data from an untrusted workflow, it must perform the following revalidation steps before acting on that data:

1. Revalidate identity: confirm the triggering workflow name, run conclusion, repository, and branch context from `github.event.workflow_run` or GitHub's API instead of trusting artifact contents alone.
2. Revalidate metadata: treat values inside downloaded artifacts as untrusted and cross-check them against the event payload, repository state, or GitHub API.
3. Revalidate transport integrity: if `download-artifact` is used, require a clean digest validation result; if artifacts are fetched through other mechanisms, apply an equivalent hash check before parsing.
4. Revalidate the release payload by rebuilding it from the trusted ref or, if promotion occurs between trusted workflows, by verifying its attestation against the expected signer policy.

Digest validation is necessary but not sufficient. GitHub's artifact digest only proves that the downloaded archive matches the uploaded archive. It does not prove that the uploaded archive was safe, correct, or produced by a trusted workflow.

### Release artifact rule

1. The default release model is rebuild-from-trusted-source.
2. A pull request workflow may prove that a candidate build is likely to succeed, but it does not manufacture the release artifact.
3. The trusted release workflow rebuilds from the merged protected commit or protected release tag.
4. The trusted release workflow then generates the artifact attestation and any SBOM attestation for the artifact it actually publishes.

### Trusted-to-trusted promotion rule

Promotion without rebuild is allowed only when all of the following are true:

1. The artifact was built by a trusted workflow, not by an untrusted PR workflow.
2. The artifact has an attestation generated in that trusted workflow.
3. The consuming workflow verifies the attestation with `gh attestation verify`.
4. Verification constrains the expected owner or repository and, where practical, the expected reusable workflow via `--signer-workflow` or `--signer-repo`.
5. The artifact still matches the intended protected ref or release identity.

This promotion path is for trusted-to-trusted handoff only. It does not relax the prohibition on promoting untrusted PR outputs.

### Reusable workflow recommendation

1. Centralize trusted build and release creation inside a reusable workflow owned by the platform or release boundary.
2. Require attestation generation inside that reusable workflow.
3. When a downstream workflow consumes the artifact, verify that the signer was the expected reusable workflow.
4. This is the preferred path when the project later wants faster trusted promotions without allowing PR-built payloads to cross the boundary.

### Permissions and review controls

1. Keep untrusted workflows on read-only or minimal `GITHUB_TOKEN` permissions.
2. Grant expanded permissions such as `attestations: write`, `packages: write`, or environment secret access only in trusted jobs.
3. Protect release or deployment environments with required reviewers.
4. Protect `.github/workflows` with CODEOWNERS and pin third-party actions and reusable workflows to full commit SHAs.

## Final Recommendation Stack

1. Do not directly publish or deploy artifacts produced by untrusted pull request workflows.
2. Use untrusted workflows for evidence only, not for release payload creation.
3. If data crosses from untrusted to trusted workflows, limit it to small metadata artifacts and revalidate that metadata against GitHub event data or repository state.
4. Treat artifact digest validation as a transport-integrity check, not as proof that an artifact is trusted.
5. Rebuild release artifacts from protected refs by default, then attest the artifact that is actually published.
6. Use `workflow_run` only when the privileged workflow avoids checking out or executing untrusted code and treats prior artifacts with caution.
7. Allow artifact promotion without rebuild only for trusted-to-trusted handoff, with attestation verification and expected signer-workflow constraints.
8. Use reusable trusted build workflows plus artifact attestations when the project later wants stronger provenance and faster trusted promotion paths.

## Three High-Value Next Design Areas

1. Trusted workflow artifact retention: define how long trusted pre-release artifacts, attestations, and verifier inputs must remain available for audit and rollback evidence.
2. Secret brokerage and rotation: define whether trusted workflows pull credentials from GitHub secrets, a broker such as Vault, or only OIDC-minted short-lived access.
3. Deployment-time provenance enforcement: define whether attestation verification is enforced only in CI or also at deployment and cluster admission boundaries.

## Official Sources

- GitHub Docs - Secure use reference: https://docs.github.com/en/actions/reference/security/secure-use
- GitHub Docs - Events that trigger workflows: https://docs.github.com/actions/using-workflows/events-that-trigger-workflows
- GitHub Docs - Store and share data with workflow artifacts: https://docs.github.com/en/actions/tutorials/store-and-share-data
- GitHub Docs - Artifact attestations: https://docs.github.com/en/actions/concepts/security/artifact-attestations
- GitHub Docs - Using artifact attestations to establish provenance for builds: https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations
- GitHub Docs - Using artifact attestations and reusable workflows to achieve SLSA v1 Build Level 3: https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/increase-security-rating