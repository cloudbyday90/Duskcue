# Trusted Runner Compromise Response

## Overview

This document defines the containment, evidence-capture, credential-revocation, and rebuild workflow for suspected compromise of a trusted self-hosted runner used by privileged release or maintenance jobs. It complements:

- [BUILDER_TRUST_BOUNDARY.md](BUILDER_TRUST_BOUNDARY.md) - when trusted self-hosted runners are allowed at all and how runner groups constrain blast radius
- [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md) - who may freeze privileged workflows, tighten runner-group access, and bypass normal change windows during incidents
- [TRUSTED_RUNNER_STATE_DISPOSAL.md](TRUSTED_RUNNER_STATE_DISPOSAL.md) - normal post-job cleanup and one-job disposal rules for trusted runners
- [SECRET_BROKERAGE_ROTATION.md](SECRET_BROKERAGE_ROTATION.md) - credential-source order and normal rotation or revocation rules for trusted workflows
- [CI_TESTING.md](CI_TESTING.md) - workflow security posture and trusted-lane separation
- [LOGGING_OBSERVABILITY.md](../operations/LOGGING_OBSERVABILITY.md) - off-box log retention and operator observability expectations

The design goal is to treat suspected privileged-runner exposure as a containment and credential incident, not merely a cleanup failure. A runner that may have been compromised must be quarantined, investigated from off-box evidence, and rebuilt from a known-clean baseline before it is allowed back into a trusted runner group.

## Scope Classification

This document is **Advanced** guidance, not a baseline product requirement.

Under [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md), this document is currently retained as deferred guidance and is not part of the active first-release path.

Use it only if the project operates trusted self-hosted runners for privileged jobs. If release and maintenance automation stay on GitHub-hosted runners, the baseline incident path is much simpler and focuses on disabling workflows, revoking credentials, and rebuilding trusted infrastructure without a runner-forensics track.

## Ownership & Review Metadata

- **Primary owner:** Project maintainer
- **Review status:** Dormant
- **Last reviewed:** 2026-06-02
- **Review cadence:** Re-review before activation; no recurring cadence while dormant
- **Review set:** [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md)
- **Review triggers:** Any material change to incident containment, runner trust assumptions, evidence sources, credential-revocation flow, privileged automation exposure, or decision to activate this deferred guidance

## Goals

1. Stop additional privileged jobs from landing on a suspect runner.
2. Preserve enough off-box evidence to understand what happened without relying on the compromised host as a trustworthy source.
3. Revoke or rotate credentials according to actual blast radius, including the period since the last known-clean baseline on reused hosts.
4. Prefer rebuild and re-registration over in-place cleanup.
5. Keep the response auditable, automatable, and fail-closed.

## Official Research Findings (May 2026)

### GitHub's documented impact model for compromised runners

- GitHub's compromised-runner guidance states that a determined attacker who can run malicious commands on a runner can harvest referenced secrets and the job's `GITHUB_TOKEN`.
- GitHub documents that secrets used as environment variables can be read directly from the environment, and secrets used in expressions can be exposed through generated scripts stored on disk.
- GitHub documents that attackers can exfiltrate secrets or repository data through logs, HTTP requests, or other arbitrary commands.
- GitHub documents that a stolen `GITHUB_TOKEN` can be used quickly while the job is running, and that repository modification risk depends on the permissions granted to that token.
- GitHub documents that extra GitHub authentication tokens, deploy keys, or personal-account SSH keys in workflows can widen compromise beyond the single repository that invoked the workflow.

### GitHub's self-hosted runner trust guidance

- GitHub's secure-use guidance states that GitHub-hosted runners execute in ephemeral, clean isolated virtual machines, while self-hosted runners do not have equivalent guarantees and can be persistently compromised by untrusted workflow code.
- GitHub documents that runner groups can restrict which repositories may access self-hosted runners, which is a blast-radius reduction control rather than a recovery control.
- GitHub documents that just-in-time and `--ephemeral` runners provide one-job assignment guarantees, but also warns that reused underlying hardware can still expose information unless automation restores a clean environment.

### GitHub runner containment and removal surfaces

- GitHub documents that if a self-hosted runner only needs to be temporarily prevented from receiving jobs, operators can stop the runner application or shut down the machine, which leaves the runner registered but offline.
- GitHub documents that the normal runner removal command removes the runner from GitHub, removes runner application configuration files on the machine, and removes configured services when applicable.
- GitHub documents that if operators cannot access the machine, GitHub supports force-removing the runner from the repository or organization.

### GitHub evidence sources

- GitHub documents that workflow run logs can be viewed, searched, downloaded, and deleted.
- GitHub documents that each job log includes GitHub-added "Set up job" and "Complete job" steps in addition to workflow-defined steps.
- GitHub documents that self-hosted runner application logs are stored in the runner `_diag` directory with `Runner_` filenames, and per-job execution logs are stored there with `Worker_` filenames.
- GitHub explicitly warns that ephemeral runner application logs must be forwarded and preserved externally for troubleshooting and diagnostics.

### GitHub log and artifact cleanup

- GitHub's secure-use guidance states that if an unredacted secret is sent to a workflow run log, operators should delete the log and rotate the secret.
- GitHub documents that workflow logs can be deleted through the web UI or programmatically.
- GitHub documents that workflow logs and artifacts can be downloaded before cleanup.
- GitHub documents that deleting workflow artifacts is irreversible, and that deleting a workflow run also deletes its associated artifacts.

### GitHub audit-log evidence relevant to incident response

- GitHub documents audit-log events for workflow and evidence handling, including `workflows.cancel_workflow_run`, `workflows.delete_workflow_run`, `checks.delete_logs`, and `artifact.destroy`.
- GitHub documents audit-log events for self-hosted runner lifecycle changes, including registration, removal, offline, online, and runner-group membership updates.
- GitHub documents `workflows.prepared_workflow_job`, which records the workflow job start and includes metadata such as runner labels, environment name, and the list of secrets passed to the job.
- GitHub documents audit-log events for credential and access changes, including token revocations, secret removals, and secret updates across organization and repository scopes.

### Vault revocation and incident-response surfaces

- Vault documents that revoking a lease invalidates the associated dynamic secret immediately and prevents further renewal.
- Vault documents that revoking a token revokes all leases created using that token.
- Vault documents prefix-based revocation as a fast way to invalidate a whole tree of issued secrets when exact lease enumeration is uncertain.
- Vault documents token accessors as references that can be used to look up and revoke tokens without needing the original token value.
- Vault documents that audit devices record request and response activity in detail, recommends enabling at least two audit devices, and notes that accessor hashing can be disabled, which speeds emergency revocation workflows but increases denial-of-service risk if the logs are abused.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Leave the runner registered, rotate obvious secrets, and return it after manual cleanup | Lowest immediate disruption | No trustworthy recovery boundary; easy to miss hidden persistence or prior-job residue | Reject |
| Stop the runner, export evidence, remove it from GitHub, then preserve the host image for deeper forensics before rebuild | Strongest host-level evidence preservation; useful for high-severity or regulated incidents | Slower to recover capacity; requires snapshot or forensic tooling; more operator overhead | Accept for high-severity incidents |
| Stop the runner, export off-box evidence, revoke credentials, remove it from GitHub, and immediately destroy or reimage it | Fastest safe containment; simplest to automate; best fit for ephemeral or JIT designs | Loses host-local evidence that was not already exported | Selected default |
| Automatically destroy the runner without first exporting logs or audit evidence | Very fast teardown | Can erase the only practical troubleshooting trail for ephemeral runners | Reject |

## Recommended Policy

### 1. Incident triggers and scope assumptions

Treat the runner as potentially compromised when any of the following occurs:

1. Untrusted or unexpectedly modified code executes on a trusted secret-bearing runner.
2. A workflow run or artifact contains an exposed secret, token, key, or credential-bearing config.
3. A pre-job hygiene check detects residue on a runner that was expected to be clean.
4. The runner is observed contacting unexpected destinations or running unknown processes.
5. The runner is assigned to the wrong repository, workflow class, or trust tier.
6. Workflow or environment governance changes suggest a privileged job may have been launched through an unauthorized path.

For reused trusted hosts, the credential-review window begins at the last known-clean baseline, not merely the current workflow run.

### 2. Immediate containment sequence

1. Stop the runner application or power off the machine so GitHub marks the runner offline and no new jobs can be accepted.
2. Cancel any active privileged workflow runs that might still be using live credentials.
3. Remove the runner from GitHub using the normal removal path if the machine is accessible; otherwise force-remove it from the repository or organization.
4. If the runner belonged to a dedicated trusted group, temporarily freeze that workflow class by tightening runner-group access, disabling the affected workflow, or requiring manual environment approval until credential review is complete.
5. Record the incident envelope immediately: runner name and ID, repository, workflow run ID, job ID if available, actor, ref, head SHA, environment name, first-detection time, and the last attested clean baseline for that trust domain.

Containment does not wait for perfect forensics. Quarantine first, investigate second.

### 3. Evidence capture sequence

Evidence collection must be external-first because the host is no longer trustworthy.

1. Download the affected workflow run logs and any prior run attempts needed to reconstruct the full execution trail.
2. Export the organization or repository audit-log slice for the incident window, including workflow-run, runner-lifecycle, runner-group, log-deletion, artifact-deletion, token-revocation, and secret-update events.
3. Preserve off-box copies of self-hosted runner `Runner_` and `Worker_` logs from `_diag`. For ephemeral runners, this must come from the external log sink because the runner may already be gone.
4. Preserve infrastructure-control-plane evidence such as VM, container, pod, autoscaler, gateway, and identity-provider logs associated with the workflow window.
5. Preserve workflow artifacts only if they are genuinely useful as evidence and can be stored in a trusted incident repository. If they contain live secrets, copy them to trusted incident storage first, then remove the GitHub-hosted copies.
6. If deeper host forensics are required, capture a disk or instance snapshot before rebuild, but do not delay credential revocation or runner removal to finish long-running forensic work.

Do not treat the compromised workspace, shell history, or local temp files as authoritative evidence unless they were copied into a trusted store during the response window.

### 4. Log and artifact hygiene after evidence capture

1. If workflow logs contain exposed secrets, delete the GitHub-hosted logs after trusted evidence capture and rotate the affected credentials.
2. If artifacts contain sensitive dumps, credentials, or internal-only payloads, delete the exposed artifacts after evidence capture, accepting that deletion is irreversible.
3. Delete the entire workflow run only when necessary and only after the required logs and metadata have already been exported, because run deletion also removes associated artifacts.
4. Retain audit-log evidence showing the cleanup actions, including log deletion and artifact destruction events.

### 5. Credential revocation and rotation model

#### GitHub-native job credentials

1. Assume the current job's `GITHUB_TOKEN` was exposed if attacker execution on the runner is plausible.
2. Because `GITHUB_TOKEN` is job-scoped and expires automatically, focus on canceling the run, reviewing what the token could have changed, and verifying any repository, release, or package actions performed during its lifetime.
3. Revoke or rotate any GitHub App tokens, OAuth app tokens, deploy keys, fine-grained PATs, classic PATs, or SSH keys that were available to the job or persisted on the host.
4. Rotate repository, environment, and organization secrets referenced by the affected workflow.
5. If the runner was reused and the last known-clean baseline is uncertain, rotate every credential that may have been exposed within that trust domain since the last trusted rebuild, not just the credentials referenced by the final observed run.

#### Vault-brokered credentials

1. Revoke the exact Vault auth token or accessor associated with the compromised job as soon as the affected issuance path is identified.
2. Prefer revoking the parent CI token for that workflow run when its scope is bounded to one run, because token revocation also revokes all leases created from that token.
3. If the exact lease set is unclear, use prefix-based revocation on the relevant secrets-engine path as the break-glass response.
4. Reissue fresh brokered credentials only after the workflow path has been restored to a known-clean runner and the trust policy remains valid.
5. Do not depend on unhashed token accessors in shared audit logs by default. If fast accessor-based revocation is required, document the operational tradeoff explicitly and prefer capturing the accessor in trusted broker-side incident records.

#### Static external credentials

1. Replace credentials that cannot be directly revoked, rather than merely marking the incident closed after deleting GitHub copies.
2. Remove superseded GitHub secret objects after the replacement path is live so old material is not accidentally reused.
3. Revalidate downstream allowlists, app registrations, and trust policies after replacement to ensure the old credential really lost effect.

### 6. Rebuild and re-admission workflow

1. Never return a suspect runner to trusted service through workspace deletion or manual host cleanup alone.
2. Rebuild from a known-clean runner image or provision a fresh VM, pod, or container boundary.
3. Update runner software and baseline packages as part of rebuild, especially if the prior baseline was already near or past update thresholds.
4. Re-establish external log forwarding before the runner is allowed to execute a privileged job.
5. Re-verify runner-group placement, allowed repositories, environment approval rules, minimal `GITHUB_TOKEN` permissions, and OIDC or Vault claim bindings.
6. Register the rebuilt runner as JIT or ephemeral wherever possible. Do not shortcut trusted re-admission by reusing stale `.runner` registration residue from the previous host state.
7. Run a non-privileged smoke workflow first, then explicitly re-enable secret-bearing workflows for that runner group.

### 7. Explicitly forbidden shortcuts

1. Do not clear only the workspace and declare the runner clean.
2. Do not leave the runner registered and merely hope that offline state is enough without formal removal or rebuild.
3. Do not keep exposed logs or artifacts available on GitHub after deciding they contain live secrets.
4. Do not narrow secret rotation to the current run when a reused runner may have retained material from earlier jobs and the last known-clean time is uncertain.
5. Do not depend on host-local logs as the only incident record for ephemeral runner environments.

## Final Recommendation Stack

1. Quarantine first: stop the runner application or host, cancel active privileged jobs, and remove the runner from GitHub before deeper investigation.
2. Treat evidence as off-box data: export workflow logs, audit logs, external runner logs, and control-plane logs before deleting exposed GitHub-hosted copies.
3. Treat exposed workflow logs as a secret incident: delete logs after evidence capture and rotate affected credentials.
4. Review blast radius from the last known-clean baseline on any reused trusted host, not just from the final observed workflow run.
5. Revoke Vault auth tokens and leases directly; use prefix revocation when exact lease scope is uncertain.
6. Revoke or rotate every non-ephemeral GitHub-side credential accessible to the job, including app tokens, deploy keys, PATs, SSH keys, and GitHub secrets.
7. Prefer immediate destroy-and-rebuild for routine incidents, and preserve host snapshots only for the higher-severity cases that genuinely need deeper forensics.
8. Never re-admit the same compromised runner registration to a trusted runner group; rebuild from a known-clean baseline and register fresh.
9. Require external log forwarding, restricted runner groups, and one-job execution boundaries before trusted runners are considered production-ready.

## Three High-Value Next Design Areas

1. Required-workflow and action-allowlist governance: define the central reusable workflows, allowed external actions, and SHA-pinned allowlist that keep privileged automation narrow and reviewable.
2. Workflow-to-secret blast-radius inventory: define a maintained map of which repository, environment, organization, OIDC, and Vault credentials each trusted workflow can access so incident rotations are faster and more complete.
3. Runner image attestation and drift detection: define how custom trusted runner images are signed, attested, and checked for unauthorized package or configuration drift before privileged jobs start.

## Official Sources

- GitHub Docs - Compromised runners: https://docs.github.com/en/actions/concepts/security/compromised-runners
- GitHub Docs - Secure use reference: https://docs.github.com/en/actions/reference/security/secure-use
- GitHub Docs - Self-hosted runners reference: https://docs.github.com/en/actions/reference/runners/self-hosted-runners
- GitHub Docs - Monitoring and troubleshooting self-hosted runners: https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/monitor-and-troubleshoot
- GitHub Docs - Using workflow run logs: https://docs.github.com/en/actions/how-tos/monitor-workflows/use-workflow-run-logs
- GitHub Docs - Removing self-hosted runners: https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/remove-runners
- GitHub Docs - Audit log events for your organization: https://docs.github.com/en/organizations/keeping-your-organization-secure/managing-security-settings-for-your-organization/audit-log-events-for-your-organization
- Vault Docs - Lease, renew, and revoke: https://developer.hashicorp.com/vault/docs/concepts/lease
- Vault Docs - Tokens: https://developer.hashicorp.com/vault/docs/concepts/tokens
- Vault Docs - Audit logging: https://developer.hashicorp.com/vault/docs/audit