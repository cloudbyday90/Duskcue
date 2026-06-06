# Trusted Runner State Disposal

## Overview

This document defines how trusted self-hosted runner exception paths dispose of state after privileged jobs. It covers workspace cleanup, credential residue, package-manager auth files, Docker builder state, local caches, and the difference between true ephemeral runners and weaker reused-host cleanup patterns. It complements:

- [BUILDER_TRUST_BOUNDARY.md](BUILDER_TRUST_BOUNDARY.md) - when self-hosted runners are allowed at all
- [SECRET_BROKERAGE_ROTATION.md](SECRET_BROKERAGE_ROTATION.md) - how privileged jobs obtain short-lived credentials and how those credentials are revoked
- [TRUSTED_RUNNER_COMPROMISE_RESPONSE.md](TRUSTED_RUNNER_COMPROMISE_RESPONSE.md) - what happens when cleanup guarantees fail or privileged runner exposure is suspected
- [BUILD_CACHE_TRUST_BOUNDARY.md](BUILD_CACHE_TRUST_BOUNDARY.md) - what cache content may persist and what must remain transient
- [REGISTRY_CACHE_RETENTION.md](REGISTRY_CACHE_RETENTION.md) - remote registry cache lifecycle versus local builder hygiene
- [CI_TESTING.md](CI_TESTING.md) - which workflows may use trusted runners and how they are gated

The design goal is to ensure that privileged self-hosted jobs do not leave behind credentials, build artifacts, network state, or package-manager residue that could be observed or reused by later jobs.

## Scope Classification

This document is **Advanced** guidance, not a baseline product requirement.

Under [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md), this document is currently retained as deferred guidance and is not part of the active first-release path.

Use it only if the project introduces trusted self-hosted runner exceptions. If the project stays on GitHub-hosted runners for normal CI and release automation, the disposal model here remains optional because the baseline path already relies on GitHub's ephemeral hosted-runner isolation instead of custom runner hygiene.

## Ownership & Review Metadata

- **Primary owner:** Project maintainer
- **Review status:** Dormant
- **Last reviewed:** 2026-06-02
- **Review cadence:** Re-review before activation; no recurring cadence while dormant
- **Review set:** [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md)
- **Review triggers:** Any material change to self-hosted runner lifecycle, cleanup hooks, ephemeral-runner design, Docker local-state handling, log-forwarding guarantees, or decision to activate this deferred guidance

## Goals

1. Prefer runner designs that guarantee one job per runner.
2. Prevent later jobs from observing secrets, tokens, workspaces, or process state from earlier jobs.
3. Keep local Docker and BuildKit state bounded and disposable.
4. Make cleanup fail closed when residue is detected.
5. Preserve enough external evidence for troubleshooting without relying on state left on the runner.

## Official Research Findings (May 2026)

### GitHub's security model for self-hosted runners

- GitHub documents that GitHub-hosted runners execute in ephemeral, clean, isolated virtual machines.
- GitHub documents that self-hosted runners do not have equivalent guarantees and can be persistently compromised by untrusted workflow code.
- GitHub warns that simply destroying a self-hosted runner after each job is not always sufficient if GitHub can still assign more than one job to that runner, because another job may observe secrets exposed through process arguments or other residual state.

### JIT and ephemeral runners

- GitHub documents just-in-time (JIT) runners as ephemeral runners created through the REST API that perform at most one job before being automatically removed from the repository, organization, or enterprise.
- GitHub documents `--ephemeral` registration for autoscaling, with automatic de-registration after one job.
- GitHub recommends autoscaling with ephemeral self-hosted runners and does not recommend autoscaling with persistent self-hosted runners.
- GitHub explicitly warns that reusing hardware for JIT or ephemeral runners can still expose information from the environment unless automation ensures the runner uses a clean environment.

### Logs and diagnostics for ephemeral runners

- GitHub documents that application logs for ephemeral runners must be forwarded to an external log storage solution for troubleshooting and diagnostics.
- GitHub recommends preserving runner logs externally before deploying ephemeral runner autoscaling in production.

### ARC lifecycle behavior

- GitHub documents ARC as the recommended Kubernetes autoscaling solution for self-hosted runners in Kubernetes environments.
- GitHub documents that ARC creates ephemeral runner pods with JIT configuration tokens.
- GitHub documents that after a successful job, the EphemeralRunner controller checks whether the runner can be deleted and then deletes it.
- GitHub documents that ARC handles the runner lifecycle from provisioning through cleanup.

### Pre- and post-job scripts

- GitHub documents `ACTIONS_RUNNER_HOOK_JOB_STARTED` and `ACTIONS_RUNNER_HOOK_JOB_COMPLETED` for self-hosted runner scripts that execute before and after jobs.
- GitHub documents that these scripts run synchronously and block job execution while running.
- GitHub documents that the completed hook runs before the job completes, which makes it unsuitable for actions that interrupt the runner, such as deleting the runner machine as part of autoscaling.
- GitHub documents that there is currently no timeout setting for these hooks, so timeout handling must be implemented in the script itself if needed.
- GitHub documents that hook scripts should not be stored inside the `actions-runner` application directory and should avoid printing sensitive information to logs.

### Runner removal behavior

- GitHub documents that removing a runner from GitHub also removes runner application configuration files and configured services when the normal removal command is used on the machine.
- GitHub documents that ordinary self-hosted runners are automatically removed from GitHub after 14 days offline, while ephemeral self-hosted runners are automatically removed after 1 day offline.
- GitHub documents that JIT runners run only a single job and are automatically removed if they never run a job.
- GitHub documents that deleting the `.runner` file on a machine allows the runner to be re-registered without re-downloading the runner application, which means runner software removal is not the same thing as host sanitization.

### Docker local-state cleanup

- Docker documents `docker buildx du` as the inspection surface for builder disk usage.
- Docker documents `docker buildx prune` as the build-cache cleanup surface, including filters such as `until`, `type`, and space-pressure controls like `--max-used-space`, `--min-free-space`, and `--reserved-space`.
- Docker documents `docker buildx rm` as the builder removal surface and states that local build cache for that builder is also removed.
- Docker documents `docker system prune` as a shortcut for pruning stopped containers, unused networks, dangling images, and unused build cache, and `--volumes` expands that to unused volumes.
- Docker documents that volumes are never removed automatically because removal may destroy data.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Shared persistent self-hosted runners with best-effort cleanup scripts | Lowest platform change; simple to start | No one-job guarantee; weak against residual processes, tokens, sockets, or reused hardware state | Reject |
| Ephemeral VM or container per job using `--ephemeral` or JIT registration | One-job assignment guarantee; easiest clean-environment reasoning outside Kubernetes | Requires orchestration for provisioning, log forwarding, and teardown | Selected default exception path |
| ARC runner scale sets with ephemeral runner pods | Strong lifecycle control in Kubernetes; GitHub-recommended Kubernetes solution; clean pod deletion model | Requires Kubernetes and cluster operations maturity | Selected for Kubernetes environments |
| JIT registration on reused hardware without full reprovisioning | Better registration security than a long-lived runner | Still exposes residue if the underlying host is not reset to a clean state | Accept only as degraded transition state |

## Recommended Policy

### 1. Baseline posture

1. Do not use self-hosted runners unless a hard requirement cannot be met with GitHub-hosted runners.
2. If self-hosted runners are required for privileged jobs, prefer one-job ephemeral runners registered with `--ephemeral` or JIT configuration.
3. In Kubernetes environments, prefer ARC runner scale sets over custom persistent runner pools.
4. Treat reused persistent runners as a degraded exception path requiring additional controls and explicit approval.

### 2. Clean-environment contract

The trusted runner disposal contract is satisfied only if all of the following are true:

1. The runner accepts at most one privileged job before de-registration.
2. The underlying compute instance, pod, VM, or container is destroyed or returned to a known-clean baseline before another job can run.
3. Any externally persisted logs or evidence are shipped off-box before destruction.
4. Credentials issued to the job are short-lived and revoked or allowed to expire immediately after use.

Deleting the workspace alone is not sufficient.

### 3. Disposal hierarchy

#### Preferred: destroy the compute boundary

1. For VM-based autoscaling, destroy the VM after the runner de-registers.
2. For container-based autoscaling, destroy the entire runner container or pod after the job.
3. For ARC, rely on the ephemeral runner deletion model and keep the runner pod disposable rather than stateful.
4. Reused hardware must be treated as infrastructure underneath a disposable execution boundary, not as the execution boundary itself.

#### Allowed only by exception: reused host with cleanup enforcement

If the host itself must be reused:

1. Restrict the runner to a single trust domain, ideally a single repository family or one tightly governed workflow class.
2. Run pre-job hygiene checks that fail the job if residue from a previous run is detected.
3. Run post-job cleanup hooks for non-destructive scrubbing.
4. Reimage or rebuild the host on a short operational cadence and immediately after suspected compromise.
5. Do not call this equivalent to an ephemeral runner.

### 4. What must be removed after each privileged job

#### Workspace and temporary state

1. Remove the checked-out workspace and any job-specific scratch directories.
2. Remove temporary files created under the runner work directory, OS temp directories, and any explicitly configured tool temp paths.
3. Remove downloaded release artifacts, restore bundles, fixture copies, and generated reports unless they were explicitly exported to durable storage.

#### Credentials and auth-bearing files

1. Remove any job-issued token files, key files, kubeconfigs, cloud credential files, and SSH material.
2. Remove auth-bearing package-manager files such as `.npmrc`, `pip.conf`, `.cargo/credentials`, `.netrc`, and equivalent job-generated credential helpers.
3. Remove per-job Git credential helpers and extra HTTP auth configuration.
4. Ensure credential brokers or providers revoke or expire issued credentials promptly according to [SECRET_BROKERAGE_ROTATION.md](SECRET_BROKERAGE_ROTATION.md).

#### Process and service residue

1. Stop any background services started for the job.
2. Ensure no job-owned processes remain running after cleanup.
3. Ensure agent sockets, forwarded SSH agents, and similar IPC artifacts are removed.
4. Fail the cleanup if privileged processes remain attached to the runner after the job.

#### Docker and BuildKit residue

1. Remove job-created Buildx builders when the build created a dedicated builder instance; `docker buildx rm` is preferred because it also removes local build cache for that builder.
2. Use `docker buildx du` to observe local builder growth and `docker buildx prune` to remove residual cache state when builders are reused.
3. Remove stopped containers and unused networks after privileged jobs if the runner hosts Docker workloads.
4. Remove unused volumes only when those volumes are explicitly part of the runner's disposable job state; do not use blanket volume deletion on shared hosts without labels or allowlists.
5. Keep remote registry cache lifecycle separate; this document covers local runner disposal, not durable registry cache retention.

### 5. Pre-job and post-job hook rules

#### Pre-job hook use

Use `ACTIONS_RUNNER_HOOK_JOB_STARTED` for fail-closed hygiene verification.

Recommended checks:

1. Assert the workspace is empty or absent.
2. Assert no prior job processes, forwarded agents, or known credential files remain.
3. Assert no unexpected Docker containers, builder instances, or temp directories remain from prior jobs.
4. Abort the job if residue is found.

#### Post-job hook use

Use `ACTIONS_RUNNER_HOOK_JOB_COMPLETED` for cleanup that does not interrupt the runner.

Recommended actions:

1. Delete workspaces, temp files, and auth-bearing config.
2. Stop job-created local services.
3. Prune or remove job-scoped Docker builder state.
4. Emit sanitized telemetry about cleanup success or failure.

Do not use the completed hook to destroy the runner machine or otherwise interrupt the runner lifecycle, because GitHub documents that this hook executes before the job completes.

### 6. Logging and forensic evidence

1. Forward ephemeral runner application logs to an external log store before production rollout.
2. Keep cleanup telemetry off the disposable runner so troubleshooting survives runner destruction.
3. Do not preserve secrets in those logs; sanitize cleanup reports and avoid printing sensitive file contents or token material.
4. Keep the minimal forensic evidence needed to understand cleanup failures, not a full copy of privileged workspace contents.

### 7. Runner software and image freshness

1. If ephemeral runners are built from images, bake the runner version into the image and update it promptly.
2. If `--disableupdate` is used, maintain an explicit update pipeline and stay within GitHub's 30-day update requirement.
3. Prefer minimal runner images so the disposable environment contains only what privileged jobs actually need.

### 8. Secure package-manager and tool-cache handling

1. Keep package-manager download caches on disposable storage when credentials or private packages may be involved.
2. Do not persist auth-bearing tool configuration across privileged jobs.
3. Separate public, non-sensitive caches from privileged runner state.
4. If a persistent host is reused, maintain a strict allowlist of what may survive between jobs; everything else is deleted.

## Final Recommendation Stack

1. Avoid self-hosted runners by default; when exceptions are required, use one-job ephemeral runners rather than persistent pools.
2. Prefer `--ephemeral` or JIT registration for VM or container runners, and prefer ARC runner scale sets in Kubernetes.
3. Treat reused hardware as safe only when a disposable execution boundary is created on top of it and reset to a known-clean baseline between jobs.
4. Use pre-job hooks for fail-closed residue detection and post-job hooks for non-destructive scrubbing, but do not use post-job hooks to terminate the runner itself.
5. Remove workspaces, temp files, background services, auth-bearing package-manager files, and job-issued credentials after every privileged job.
6. Remove or prune job-scoped Docker builders and local BuildKit cache; do not confuse local runner cleanup with remote registry cache retention.
7. Forward runner logs and cleanup telemetry to external storage before destroying ephemeral runners.
8. Keep runner images minimal and updated, especially when automatic updates are disabled.
9. Follow [TRUSTED_RUNNER_COMPROMISE_RESPONSE.md](TRUSTED_RUNNER_COMPROMISE_RESPONSE.md) for quarantine, off-box evidence capture, credential revocation, and rebuild rules after suspected runner exposure.

## Three High-Value Next Design Areas

1. Runner image governance: define the approved software baseline, update cadence, SBOM expectations, and signing or attestation rules for custom runner images.
2. Trust-domain segmentation for maintenance runners: define how restore, signing, registry-admin, and networked maintenance jobs are separated so one privileged workflow class cannot inherit another's local state.
3. Runner drift detection and admission control: define what host, image, or package drift blocks a trusted runner from accepting privileged work.

## Official Sources

- GitHub Docs - Secure use reference: https://docs.github.com/en/actions/reference/security/secure-use
- GitHub Docs - Self-hosted runners reference: https://docs.github.com/en/actions/reference/runners/self-hosted-runners
- GitHub Docs - Running scripts before or after a job: https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/run-scripts
- GitHub Docs - Removing self-hosted runners: https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/remove-runners
- GitHub Docs - Actions Runner Controller: https://docs.github.com/en/actions/concepts/runners/actions-runner-controller
- Docker Docs - Manage builders: https://docs.docker.com/build/builders/manage/
- Docker Docs - docker buildx prune: https://docs.docker.com/reference/cli/docker/buildx/prune/
- Docker Docs - Prune unused Docker objects: https://docs.docker.com/engine/manage-resources/pruning/