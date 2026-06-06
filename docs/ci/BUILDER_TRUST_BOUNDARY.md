# Builder Trust Boundary & Private Dependency Ingress

## Overview

This document defines which workflows are allowed to access trusted build infrastructure, how private dependencies are made available to builds, and when self-hosted runners are permitted. It complements:

- [DOCKER_BUILD_RELEASE.md](../operations/DOCKER_BUILD_RELEASE.md) - image build and publication workflow
- [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md) - emergency authority for freezing privileged workflows and tightening runner-group access
- [SECRET_BROKERAGE_ROTATION.md](SECRET_BROKERAGE_ROTATION.md) - credential-source order, broker usage, and rotation rules for trusted workflows
- [TRUSTED_RUNNER_STATE_DISPOSAL.md](TRUSTED_RUNNER_STATE_DISPOSAL.md) - how self-hosted exception runners wipe workspaces, credentials, local caches, and builder state after privileged jobs
- [TRUSTED_RUNNER_COMPROMISE_RESPONSE.md](TRUSTED_RUNNER_COMPROMISE_RESPONSE.md) - quarantine, evidence, revocation, and rebuild rules after suspected privileged-runner exposure
- [PRIVILEGED_ARTIFACT_HANDOFF.md](PRIVILEGED_ARTIFACT_HANDOFF.md) - rules for moving evidence across trust boundaries without promoting untrusted payloads
- [BUILD_CACHE_TRUST_BOUNDARY.md](BUILD_CACHE_TRUST_BOUNDARY.md) - cache backend separation, PR-visible cache rules, and poisoning prevention controls
- [CI_TESTING.md](CI_TESTING.md) - validation lanes and workflow security posture
- [API_SECURITY.md](../security/API_SECURITY.md) - supply-chain posture, SBOMs, and dependency auditing
- [RELEASE_ENGINEERING.md](RELEASE_ENGINEERING.md) - release classes and protected publication rules

The design goal is to prevent untrusted code from reaching privileged builders or secrets, while still allowing trusted workflows to consume private packages, private Git sources, and internal registries in a controlled way.

## Scope Classification

This document is **Advanced** guidance, not a baseline product requirement.

Under [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md), this document is currently retained as deferred guidance and is not part of the active first-release path.

Use it when the project needs trusted build separation beyond ordinary GitHub-hosted CI, such as private dependency ingress, privileged release builders, or narrowly scoped self-hosted runner exceptions. For the baseline single-admin path, keep the default secure GitHub-hosted model and defer this extra machinery until those trust-boundary requirements actually exist.

## Ownership & Review Metadata

- **Primary owner:** Project maintainer
- **Review status:** Dormant
- **Last reviewed:** 2026-06-02
- **Review cadence:** Re-review before activation; no recurring cadence while dormant
- **Review set:** [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md)
- **Review triggers:** Any material change to GitHub Actions trust boundaries, private dependency ingress, self-hosted runner usage, protected release-builder assumptions, or decision to activate this deferred guidance

## Goals

1. Keep untrusted pull requests off privileged infrastructure.
2. Prefer short-lived credentials over stored long-lived secrets.
3. Make private dependency access explicit, reviewable, and least-privilege.
4. Avoid leaking credentials into images, logs, caches, or shared runners.
5. Preserve a simple operator model without weakening the build trust boundary.

## Official Research Findings (May 2026)

### GitHub Actions runner trust model

- GitHub's secure-use guidance states that GitHub-hosted runners execute code inside ephemeral, clean isolated virtual machines.
- GitHub's secure-use guidance states that self-hosted runners do not have equivalent guarantees and can be persistently compromised by untrusted workflow code.
- GitHub's secure-use guidance warns that self-hosted runners should almost never be used for public repositories, and that even private or internal repositories are risky because users with read access may be able to open fork-based pull requests that compromise the runner environment.
- GitHub documents that runner groups can restrict which organizations and repositories may use self-hosted runners, reducing blast radius.
- GitHub documents just-in-time self-hosted runners as a one-job registration model, but also warns that reusing underlying hardware can still leak information unless automation restores a clean environment.

### Untrusted pull requests and privileged workflows

- GitHub's secure-use guidance warns against using `pull_request_target` or `workflow_run` with untrusted checked-out code or artifacts.
- GitHub documents manual approval of workflow runs from forks and explicitly tells maintainers to inspect workflow changes before approval.
- GitHub's secure-use guidance recommends least-privilege `GITHUB_TOKEN` permissions and CODEOWNERS protection for workflow files.

### OIDC and trust scoping

- GitHub's OIDC reference states that cloud trust should always include at least one condition so untrusted repositories cannot request access tokens.
- GitHub documents OIDC claims such as `repository`, `ref`, `environment`, `job_workflow_ref`, `repository_visibility`, and `runner_environment` that can be used to narrow trust.
- GitHub documents customizing the OIDC subject claim to require specific reusable workflows, repository owners, repository IDs, or repository custom properties.
- GitHub documents `id-token: write` as the required permission for requesting an OIDC token, and notes that it only enables token minting, not broader write access.

### Private network access from GitHub-hosted runners

- GitHub documents an API-gateway pattern in which GitHub-hosted runners authenticate to an edge service using OIDC and the gateway makes requests into the private network on behalf of the workflow.
- GitHub's private-network OIDC guidance says the gateway must validate not only that the token came from GitHub Actions, but that it came from the expected workflows using OIDC claims.

### Private registry and package access

- GitHub documents organization-level private registry definitions for code scanning and Dependabot.
- GitHub documents that Dependabot supports OIDC authentication for organization-level private registries, eliminating the need to store long-lived secrets for those update jobs.
- GitHub documents that code scanning advanced setup does not automatically inherit organization-level private registries; any private registries needed by the observed build must be accessible to the workflow running `codeql-action`.
- GitHub's container registry documentation recommends using `GITHUB_TOKEN` in GitHub Actions workflows when possible instead of a personal access token.
- GitHub documents that cross-repository private package access may still require explicit package permission grants or, in some cases, a PAT classic with the narrowest possible package scopes.

### Docker private dependency ingress

- Docker documents that build arguments and environment variables are inappropriate for secrets because they persist in the final image.
- Docker documents secret mounts, SSH mounts, and pre-flight Git authentication secrets as the supported ways to provide sensitive build inputs.
- Docker documents `GIT_AUTH_TOKEN` and `GIT_AUTH_HEADER` as the BuildKit mechanism for authenticating remote private Git contexts and `ADD` operations against private repositories.
- Docker documents `--mount=type=ssh` as the supported path for SSH-based access to private Git repositories during builds.
- Docker's GitHub Actions build-secret guidance documents `secrets`, `secret-envs`, and `secret-files` inputs for `docker/build-push-action`, including examples for private package manager configuration files such as `.npmrc`.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| GitHub-hosted runners only, with no access to private-only dependencies | Strong isolation, minimal ops burden | Cannot reach internal-only registries or private network services | Too limited |
| GitHub-hosted runners for all untrusted work, plus OIDC-backed gateway or registry access for trusted jobs | Keeps the strongest default isolation, uses short-lived credentials, avoids broad self-hosted exposure | Requires private-network or registry integration work | Preferred default |
| Shared self-hosted runners for all CI and release jobs | Flexible network access, custom hardware possible | Large blast radius, persistent compromise risk, hard secret hygiene | Reject |
| Ephemeral or JIT self-hosted runners only for narrowly scoped trusted jobs | Can satisfy hardware or network exceptions while narrowing exposure | Still more operationally risky than GitHub-hosted, requires clean-environment automation | Accept only by exception |

## Recommended Policy

### Builder trust tiers

#### Tier 1: Untrusted validation

Applies to pull requests from forks, external contributors, and any workflow processing untrusted source changes before maintainer approval.

- Run only on GitHub-hosted runners.
- Use read-only or minimal `GITHUB_TOKEN` permissions.
- Do not expose environment secrets, cloud credentials, private registry credentials, or self-hosted runners.
- Do not publish artifacts to release registries.
- Do not use `pull_request_target` or `workflow_run` to check out untrusted code.

#### Tier 2: Trusted integration

Applies to pushes on protected branches, scheduled jobs, and maintainer-approved internal workflows.

- Prefer GitHub-hosted runners.
- Permit OIDC for narrowly scoped cloud or registry access.
- Permit access to approved private registries and package feeds.
- Keep write permissions and secret access job-specific rather than workflow-wide.

#### Tier 3: Trusted release and exceptional builder access

Applies to release publication, signing, or jobs that must reach internal-only systems or specialized hardware.

- Prefer GitHub-hosted runners plus OIDC-authenticated ingress into the private network.
- Allow self-hosted runners only when GitHub-hosted runners cannot meet a hard requirement such as internal network reachability that cannot be solved through a gateway, or required hardware not available on GitHub-hosted runners.
- Require protected refs, environments, and explicit approvals before secret-bearing jobs start.

### Self-hosted runner allowance rules

1. Self-hosted runners are not the default builder path.
2. Self-hosted runners must never process untrusted pull request code from forks.
3. If self-hosted runners are approved, they must be isolated into dedicated runner groups aligned to a single trust domain.
4. Use JIT or ephemeral runners where possible, and automate post-job cleanup of workspace, credentials, caches, and any mounted secrets.
5. Do not store long-lived cloud credentials, registry passwords, or reusable SSH keys persistently on the host when OIDC or dynamic secret retrieval is available.
6. Shared organization-wide runners are only acceptable for repositories with equivalent trust and governance.

### Private dependency ingress rules

#### Docker build-time secrets

1. Use BuildKit secret mounts for private package manager credentials, API tokens, and config files.
2. Use `secret-envs` or `secret-files` in `docker/build-push-action` when workflow-generated credentials or files such as `.npmrc` are needed.
3. Never pass secrets through Docker build args or plain environment variables that become part of the image history.

#### Private Git during build

1. Use `GIT_AUTH_TOKEN` or `GIT_AUTH_HEADER` for HTTPS-based remote private Git contexts and `ADD` operations.
2. Use `--mount=type=ssh` for SSH-based private Git access.
3. Scope credentials per host when multiple Git hosts are involved.
4. Prefer host-specific, read-only tokens over broad personal credentials.

#### Package and registry access in workflows

1. Prefer `GITHUB_TOKEN` for GHCR and GitHub Packages access when the package is linked to the workflow repository or explicit package access has been granted.
2. If GitHub Packages access cannot be solved with repository linkage and package permissions, use the narrowest possible PAT classic scope as an exception, avoiding broader `repo` scope when package-only scope is sufficient.
3. For third-party or cloud-hosted private registries, prefer OIDC-backed short-lived credentials over stored static secrets.
4. Register organization-level private registries for security tooling so Dependabot and code scanning are not blind to private dependencies.

#### Private network access

1. Prefer GitHub-hosted runners plus an OIDC-authenticated API gateway for workflows that need to reach services inside a private network.
2. The gateway must validate workflow identity claims such as repository, ref, environment, reusable workflow, and runner type before forwarding requests.
3. Direct network reach from self-hosted runners is only acceptable when the gateway model or other GitHub-hosted connectivity model cannot satisfy the requirement.

### Workflow protections

1. Protect `.github/workflows` with CODEOWNERS.
2. Pin third-party actions and reusable workflows to full commit SHAs.
3. Default `GITHUB_TOKEN` to minimal permissions and widen only on the jobs that need extra scopes.
4. Require maintainer approval for fork-triggered runs where the repository policy allows approval-based execution.
5. Treat artifacts produced by untrusted workflows as untrusted input unless they are revalidated in a trusted workflow.
6. Separate validation from publication so the job that sees untrusted code is never the same job that has release credentials.

### OIDC trust-policy requirements

1. Every cloud or registry trust policy must include at least one restrictive condition.
2. Prefer combining repository identity with branch or tag, environment, and reusable workflow identity.
3. Where practical, require `runner_environment=github-hosted` for roles intended only for GitHub-hosted runners.
4. For organization-wide trust models, consider repository custom properties or repository IDs to reduce drift and rename risk.
5. Grant `id-token: write` only to jobs that need OIDC.

## Final Recommendation Stack

1. Use GitHub-hosted runners as the default for all pull requests and most trusted builds.
2. Keep untrusted PR validation entirely separate from secret-bearing and publish-capable jobs.
3. Prefer GitHub-hosted runners plus OIDC-backed private registry or gateway access instead of broad self-hosted runners.
4. Allow self-hosted runners only by exception, only for trusted refs, and only with runner-group isolation plus JIT or equivalent clean-environment automation.
5. Use BuildKit secret mounts, SSH mounts, and `GIT_AUTH_TOKEN` or `GIT_AUTH_HEADER` for private build inputs; never use ARG or persistent ENV for secrets.
6. Prefer `GITHUB_TOKEN` for GHCR and repository-linked package access; use PATs only as narrow, documented exceptions.
7. Configure organization-level private registries so Dependabot and code scanning can reason about private dependencies instead of silently missing them.
8. Scope every OIDC trust relationship to the expected repository, ref or environment, and, where practical, the expected reusable workflow and runner environment.
9. Follow [SECRET_BROKERAGE_ROTATION.md](SECRET_BROKERAGE_ROTATION.md) for credential-source precedence, Vault-broker exception rules, and rotation or revocation expectations.
10. Follow [TRUSTED_RUNNER_STATE_DISPOSAL.md](TRUSTED_RUNNER_STATE_DISPOSAL.md) for the disposal contract required on any self-hosted exception runner.
11. Follow [TRUSTED_RUNNER_COMPROMISE_RESPONSE.md](TRUSTED_RUNNER_COMPROMISE_RESPONSE.md) for quarantine, emergency credential revocation, and rebuild requirements if a trusted runner may have been exposed.

## Three High-Value Next Design Areas

1. Required-workflow and action-allowlist governance: define the central reusable workflows, allowed third-party actions, and SHA-pinned allowlist that keep privileged automation narrow and reviewable.
2. Deployment-time provenance enforcement: define whether attestation verification is enforced only in CI or also at deployment and cluster admission boundaries.
3. Runner image attestation and drift control: define how trusted builder images are approved, verified, and blocked on unauthorized drift.

## Official Sources

- GitHub Docs - Secure use reference: https://docs.github.com/en/actions/reference/security/secure-use
- GitHub Docs - OpenID Connect reference: https://docs.github.com/en/actions/reference/security/oidc
- GitHub Docs - Approving workflow runs from forks: https://docs.github.com/en/actions/how-tos/manage-workflow-runs/approve-runs-from-forks
- GitHub Docs - Using an API gateway with OIDC: https://docs.github.com/en/actions/how-tos/manage-runners/github-hosted-runners/connect-to-a-private-network/connect-with-oidc
- GitHub Docs - Giving security features access to private registries: https://docs.github.com/en/code-security/securing-your-organization/enabling-security-features-in-your-organization/giving-org-access-private-registries
- GitHub Docs - Working with the Container registry: https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry
- Docker Docs - Build secrets: https://docs.docker.com/build/building/secrets/
- Docker Docs - Using secrets with GitHub Actions: https://docs.docker.com/build/ci/github-actions/secrets/