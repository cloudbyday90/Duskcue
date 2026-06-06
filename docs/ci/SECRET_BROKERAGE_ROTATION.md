# Secret Brokerage & Rotation for Trusted Release and Maintenance Workflows

## Overview

This document defines how trusted release and maintenance workflows obtain credentials, when GitHub-native authentication is sufficient, when an external broker such as Vault is justified, and how short-lived credentials are rotated or revoked. It complements:

- [BUILDER_TRUST_BOUNDARY.md](BUILDER_TRUST_BOUNDARY.md) - runner trust tiers, private dependency ingress, and self-hosted exception rules
- [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md) - governance of protected environments, workflow freeze authority, and runner-group emergency controls
- [TRUSTED_RUNNER_COMPROMISE_RESPONSE.md](TRUSTED_RUNNER_COMPROMISE_RESPONSE.md) - quarantine and emergency revocation flow after suspected privileged-runner exposure
- [DOCKER_BUILD_RELEASE.md](../operations/DOCKER_BUILD_RELEASE.md) - secure injection of build-time secrets and registry publication behavior
- [CI_TESTING.md](CI_TESTING.md) - trusted workflow lanes, release gates, and workflow security posture
- [RELEASE_ENGINEERING.md](RELEASE_ENGINEERING.md) - protected release paths, rollback boundaries, and operator-facing release behavior
- [RELEASE_ARTIFACT_RETENTION.md](RELEASE_ARTIFACT_RETENTION.md) - durable release evidence and rollback-proof retention

The design goal is to keep long-lived secrets out of GitHub workflows wherever possible, while still giving protected release and maintenance jobs the minimum access they need to publish, verify, rotate, restore, or administer production systems.

## Scope Classification

This document is **Advanced** guidance, not a baseline product requirement.

Under [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md), this document is currently retained as deferred guidance and is not part of the active first-release path.

Use it when the project has trusted release or maintenance workflows that need scoped credentials beyond ordinary GitHub-native defaults, especially OIDC-bound cloud access or an external broker such as Vault. For the baseline single-admin path, prefer the smallest secure default that avoids standing secrets and add brokerage only when the extra isolation materially helps.

## Ownership & Review Metadata

- **Primary owner:** Project maintainer
- **Review status:** Dormant
- **Last reviewed:** 2026-06-02
- **Review cadence:** Re-review before activation; no recurring cadence while dormant
- **Review set:** [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md)
- **Review triggers:** Any material change to credential sourcing, OIDC trust scoping, Vault usage, environment protection, rotation and revocation guarantees, or decision to activate this deferred guidance

## Goals

1. Prefer ephemeral credentials over stored static secrets.
2. Keep GitHub-native operations on GitHub-native authentication paths where possible.
3. Use a broker only when it materially improves isolation, auditability, or credential type support.
4. Make secret-bearing workflows centrally reviewable and environment-gated.
5. Ensure every credential path has an explicit revocation and rotation model.

## Official Research Findings (May 2026)

### GitHub stored secrets and default limitations

- GitHub documents that, except for `GITHUB_TOKEN`, secrets are not passed to workflows triggered from forks.
- GitHub documents that secrets are not automatically passed to reusable workflows.
- GitHub documents that secrets are not available to workflows triggered by Dependabot events.
- GitHub documents that workflows accessing cloud providers should prefer OpenID Connect (OIDC) so long-lived cloud secrets do not need to be stored in GitHub.

### `GITHUB_TOKEN` lifecycle and least privilege

- GitHub documents that `GITHUB_TOKEN` is created per job and expires when the job completes, or when the maximum lifetime is reached.
- GitHub documents that GitHub-hosted jobs have an effective `GITHUB_TOKEN` lifetime limit of 6 hours.
- GitHub documents that if any `permissions` block is declared, unspecified scopes become `none`.
- GitHub documents that actions can access the token through the `github.token` context even if the workflow does not explicitly pass `secrets.GITHUB_TOKEN`, so permissions must still be minimized.
- GitHub documents that if a workflow needs permissions unavailable to `GITHUB_TOKEN`, the preferred escalation is a GitHub App installation token before falling back to a personal access token.

### Reusable workflows and secret propagation

- GitHub documents that reusable workflows use `workflow_call` and may declare named secrets explicitly.
- GitHub documents that `secrets: inherit` passes all secrets available to the caller, including organization, repository, and environment secrets.
- GitHub documents that secrets only flow one hop unless explicitly passed again to nested reusable workflows.
- GitHub documents that environment secrets cannot be passed through `on.workflow_call`; if a called job references an environment, that environment's secrets are used instead.
- GitHub documents that nested reusable workflows can only maintain or reduce permissions, not increase them.

### Environments and protected secret release

- GitHub documents that all deployment protection rules must pass before a job referencing an environment is sent to a runner.
- GitHub documents that environment protection can require reviewers, prevent self-review, restrict branches or tags, and apply wait timers or custom rules.
- GitHub documents that environment secrets are made available through the environment boundary, not through generic workflow secret inheritance.
- GitHub documents that referencing a new environment name can create that environment, so environment governance matters and names cannot be treated as self-securing.

### OIDC trust scoping

- GitHub documents that OIDC removes the need to store long-lived cloud secrets in GitHub Actions.
- GitHub documents that trust policies should always include at least one restrictive condition so arbitrary repositories cannot mint useful tokens.
- GitHub documents OIDC claims including `repository`, `repository_id`, `repository_visibility`, `environment`, `job_workflow_ref`, `runner_environment`, and `repo_property_*`.
- GitHub documents that the subject claim can be customized to require reusable workflow identity, repository IDs, environments, or custom repository properties.
- GitHub documents that `id-token: write` only allows a job to request an OIDC token; it does not itself grant broader repository write access.

### GitHub-native registry and package auth

- GitHub documents that workflows should prefer `GITHUB_TOKEN` over a personal access token for GitHub Container Registry and GitHub Packages where package permissions permit it.
- GitHub documents that some cross-repository private package cases still require explicit package permission grants or a PAT classic fallback with the narrowest possible package scope.

### Vault as a broker

- GitHub documents OIDC integration with HashiCorp Vault and shows workflows authenticating to Vault using `id-token: write` plus a Vault role constrained by claim bindings.
- GitHub recommends environment protection rules when environments are used in OIDC trust decisions.
- Vault documents JWT and OIDC auth roles with `bound_subject`, `bound_audiences`, and arbitrary `bound_claims` for claim-level authorization.
- Vault documents that JWT roles require exact audience matching when the incoming JWT contains an `aud` claim.
- Vault documents that claim values can be copied into token metadata using `claim_mappings`, which can improve auditability without widening privileges.

### Vault leases, dynamic credentials, and revocation

- Vault documents that every dynamic secret has a lease with a TTL and renewability state.
- Vault documents that expiring or revoking a lease invalidates the secret and prevents further renewal.
- Vault documents that revoking a token also revokes all leases created from that token.
- Vault documents prefix-based revocation for emergency invalidation of a class of issued secrets.
- Vault's database secrets guidance documents dynamic credentials with a default TTL and max TTL, and explicitly recommends defining creation, revocation, renewal, and rotation statements suitable for the target database.

### Docker secret injection

- Docker documents that secrets must not be passed through Docker build args or normal environment variables because those can persist in image history.
- Docker documents BuildKit secret mounts, SSH mounts, `secret-envs`, and `secret-files` as the supported mechanisms for passing sensitive build-time data.
- Docker documents that secrets passed through `docker/build-push-action` are mounted under `/run/secrets/<id>` by default.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| GitHub repository and environment secrets for everything | Simple mental model; no external broker | Long-lived secret inventory grows; rotation becomes manual; harder revocation blast-radius control; secrets can sprawl across workflows | Reject as baseline |
| GitHub-native ephemeral auth only, no broker | Minimal stored secret surface; simplest for GitHub, GHCR, and direct OIDC-capable providers | Fails where the target system cannot trust GitHub directly or where dynamic per-use credentials are needed | Too limited |
| GitHub-native first, Vault broker second | Uses `GITHUB_TOKEN` and direct OIDC wherever possible, but still supports dynamic or centrally audited credentials for non-native targets | Adds broker operations and policy work for the exceptional paths | Selected |
| Vault mandatory for all trusted workflows | Centralized audit and issuance model | Unnecessary indirection for GitHub-native operations; more moving parts; weaker simplicity for GHCR and repo APIs | Reject |

## Recommended Policy

### Credential source order

Trusted workflows must select credentials in this order:

1. `GITHUB_TOKEN` for repository-native operations such as releases, attestations, GHCR publication, package access, and GitHub API calls that fit inside GitHub's permission model.
2. Direct OIDC to cloud or external providers that can validate GitHub's identity claims without stored long-lived credentials.
3. Vault-issued short-lived credentials for systems that cannot directly trust GitHub OIDC, or where Vault can issue safer dynamic credentials than GitHub can store.
4. GitHub stored static secrets only as a documented exception when none of the first three paths can satisfy the integration safely.

### 1. GitHub-native authentication rules

Use `GITHUB_TOKEN` when the target is GitHub itself or a GitHub-native registry integration.

- Declare explicit job-level `permissions` blocks.
- Grant only the scopes needed for that job, for example `contents: read`, `packages: write`, `attestations: write`, or `id-token: write`.
- Do not widen workflow-wide permissions for convenience.
- Prefer GitHub App installation tokens over PATs when `GITHUB_TOKEN` lacks the required scope.
- Treat PATs as a last-resort exception for GitHub-integrated operations.

### 2. Direct OIDC rules

Use direct OIDC when the external target can validate GitHub tokens itself.

- Require at least one restrictive trust condition, and normally several.
- Prefer binding on `repository_id` or another rename-stable identifier rather than repository name alone.
- For production release or maintenance jobs, also bind on `environment` and `job_workflow_ref` where supported.
- Require `runner_environment=github-hosted` for roles intended only for GitHub-hosted runners.
- Grant `id-token: write` only to the jobs that actually mint OIDC tokens.

### 3. Vault broker rules

Use Vault when direct OIDC is unavailable or when Vault can issue a safer credential form.

- Authenticate the workflow to Vault with GitHub OIDC or JWT, not with a stored long-lived Vault token.
- Constrain Vault roles with `bound_audiences`, `bound_subject`, and `bound_claims` to the expected repository, environment, and trusted reusable workflow identity.
- Prefer Vault dynamic secret engines for databases, cloud credentials, or other lease-aware targets.
- If a target only supports static credentials, broker access through Vault with the shortest practical TTL and the narrowest policy path rather than storing broad credentials directly in many workflows.
- Use `claim_mappings` only for audit-relevant metadata; do not use metadata copying as a substitute for authorization constraints.

### 4. Static GitHub secret exception rules

Stored GitHub secrets are allowed only when:

1. The target cannot trust GitHub OIDC directly.
2. Vault cannot broker that target safely or would add no real security benefit.
3. The secret can be attached to a protected environment rather than broadly available at repository scope.
4. The workflow uses explicit secret names, not blanket `secrets: inherit`.

Static secrets are not the baseline design for release publication, restore drills, or routine maintenance automation.

## Trusted Workflow Topology

### Central trusted workflow path

Secret-bearing release and maintenance jobs should run through centrally reviewed reusable workflows.

- Put publication, restore, key-rotation, and privileged maintenance logic in trusted reusable workflows.
- Pin cross-repository reusable workflows to full commit SHAs.
- Use `job_workflow_ref` in OIDC or Vault trust policies when central reusable workflows are part of the trust boundary.
- Avoid duplicating secret-bearing logic across many repository-local workflow files.

### Secret passing rules

- Prefer no caller-passed secrets for reusable workflows when the called workflow can obtain credentials with `GITHUB_TOKEN`, environment secrets, or OIDC on its own.
- If secrets must be passed, declare them by name and pass only the minimum required set.
- Do not use `secrets: inherit` for release or maintenance workflows unless the caller and callee are under the same tightly governed trust domain and the broader exposure is explicitly accepted.
- Do not rely on environment secrets traversing `workflow_call`; attach the environment at the job that actually needs the protected secret or approval boundary.

### Environment requirements

High-privilege jobs must use protected environments.

- Production publication, signing, restore, backup-admin, and secret-rotation jobs require environment protection before the job reaches a runner.
- Require reviewers for production-facing environments.
- Prevent self-review where supported.
- Restrict the environment to protected branches or release tags.
- Treat environment naming as governed configuration, because ad hoc environment creation is not a security control.

## Rotation and Revocation Model

### `GITHUB_TOKEN`

- Rotation is automatic because the token is job-scoped and expires automatically.
- No secret inventory should exist for this path.
- Review permissions, not rotation calendars.

### Direct OIDC

- Rotation is provider-side issuance of short-lived tokens; there should be no stored secret in GitHub to rotate.
- Keep provider-issued tokens bounded to a single job or job phase.
- Revoke by removing or tightening the provider trust policy, environment approval path, or reusable workflow reference.

### Vault authentication tokens

- For CI release and maintenance workflows, issue short-lived Vault auth tokens from GitHub OIDC or JWT.
- Default Vault auth token TTL should be around 10 minutes for high-privilege jobs unless the workflow legitimately needs a longer window.
- Cap Vault auth token max TTL to the smallest practical job window, normally no more than 1 hour for privileged release or maintenance tasks.
- Revoke the Vault token explicitly at job completion when the client or action supports it, and rely on automatic expiration as the backstop.

### Vault-issued dynamic credentials

- Prefer default TTLs in the 15 to 60 minute range for privileged workflow-issued credentials.
- Keep max TTL aligned to the job's operational window rather than a generic platform default.
- Only allow longer max TTLs, such as restore-drill or backup-verification windows, when the workflow cannot reasonably complete inside the shorter interval and the credential is read-only or tightly scoped.
- Define explicit creation, revocation, renewal, and rotation statements for database-style dynamic credentials rather than relying on generic defaults.

### Static secret exceptions

- Rotation must be deliberate and documented because GitHub will not rotate these automatically.
- High-privilege static secrets should be rotated immediately after personnel changes, trust-boundary changes, or exposure events.
- Static secrets used for trusted release or maintenance workflows should live behind environment approval and should not be duplicated across multiple scopes unless redundancy is necessary.
- If a static secret can be replaced by direct OIDC or Vault-issued credentials, the static secret path should be retired rather than perpetuated.

## Workflow-Specific Recommendations

### Release publication workflows

- Use `GITHUB_TOKEN` for GitHub release creation, GHCR publication, and artifact attestations when package permissions allow it.
- Use direct OIDC for cloud-side publication or deployment targets that support workload identity.
- Use Vault only for release-time credentials that GitHub or the provider cannot handle directly, such as short-lived signing access, dynamic database admin credentials for final migration windows, or centrally brokered credentials to internal systems.
- Keep publication credentials separated from build-validation jobs that process untrusted code.

### Trusted maintenance workflows

Examples: restore drills, backup verification, registry cleanup, database maintenance, and secret rotation jobs.

- Default to read-only or maintenance-scoped credentials.
- Use direct OIDC or Vault-issued read-only credentials where the operation is external.
- Give destructive or admin-level maintenance workflows their own stricter environment boundary and approval path.
- Use workflow concurrency controls for jobs that should never overlap, such as production restore drills or root credential rotation.

### Docker build secret usage

- Secret source selection is decided by this document.
- Secret injection into Docker builds must follow [DOCKER_BUILD_RELEASE.md](../operations/DOCKER_BUILD_RELEASE.md): BuildKit secret mounts, SSH mounts, `secret-envs`, or `secret-files` only.
- Credentials brokered or minted for a build must never be promoted into Docker build args, plain environment variables, image layers, or shared caches.

## Final Recommendation Stack

1. Use `GITHUB_TOKEN` as the default for GitHub-native release and package operations.
2. Use direct OIDC as the default for external providers that can validate GitHub identity claims.
3. Introduce Vault only for systems that cannot trust GitHub directly or where Vault can issue materially safer short-lived credentials.
4. Put secret-bearing release and maintenance logic behind trusted reusable workflows and protected environments.
5. Avoid `secrets: inherit` for privileged workflows; pass named secrets only when unavoidable.
6. Keep privileged broker or provider-issued credentials short-lived, revocable, and aligned to single-job execution windows.
7. Reserve static GitHub secrets for narrowly documented exception paths, and retire them when an ephemeral alternative becomes available.
8. Keep Docker secret handling ephemeral end-to-end: brokered or minted in the job, mounted into BuildKit, never stored in image history or shared caches.

## Three High-Value Next Design Areas

1. Deployment-time provenance enforcement: define whether attestations are verified only in CI or also at deployment, promotion, and admission-control boundaries.
2. Workflow-to-secret blast-radius inventory: define naming, ownership, and a maintained map of which trusted reusable workflows and environments can reach each sensitive credential source.
3. Broker incident response: define how emergency revocation, lease prefix revocation, and provider-side trust shutdown are coordinated when privileged automation is suspected of compromise.

## Official Sources

- GitHub Docs - Using secrets in GitHub Actions: https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-secrets
- GitHub Docs - Use `GITHUB_TOKEN` for authentication in workflows: https://docs.github.com/en/actions/how-tos/security-for-github-actions/security-guides/use-github_token-in-workflows
- GitHub Docs - Workflow syntax for GitHub Actions: https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax
- GitHub Docs - OpenID Connect: https://docs.github.com/en/actions/concepts/security/openid-connect
- GitHub Docs - OpenID Connect reference: https://docs.github.com/en/actions/reference/openid-connect-reference
- GitHub Docs - Reuse workflows: https://docs.github.com/en/actions/how-tos/reuse-automations/reuse-workflows
- GitHub Docs - Manage environments: https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments
- GitHub Docs - OIDC with reusable workflows: https://docs.github.com/en/actions/how-tos/secure-your-work/security-harden-deployments/oidc-with-reusable-workflows
- GitHub Docs - OIDC in HashiCorp Vault: https://docs.github.com/en/actions/how-tos/secure-your-work/security-harden-deployments/oidc-in-hashicorp-vault
- GitHub Docs - Working with the Container registry: https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry
- Docker Docs - Build secrets: https://docs.docker.com/build/building/secrets/
- Docker Docs - Using secrets with GitHub Actions: https://docs.docker.com/build/ci/github-actions/secrets/
- HashiCorp Vault Docs - Use JWT/OIDC authentication: https://developer.hashicorp.com/vault/docs/auth/jwt
- HashiCorp Vault Docs - Lease, renew, and revoke: https://developer.hashicorp.com/vault/docs/concepts/lease
- HashiCorp Vault Tutorial - Dynamic secrets for database credential management: https://developer.hashicorp.com/vault/tutorials/db-credentials/database-secrets