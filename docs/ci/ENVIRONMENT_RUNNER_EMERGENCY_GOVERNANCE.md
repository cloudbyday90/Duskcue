# Environment & Runner-Group Emergency Governance

## Overview

This document defines who may freeze privileged GitHub Actions workflows, who may change self-hosted runner-group access during an incident, who may bypass normal change windows, and which emergency actions are never allowed to bypass the normal approval model. It complements:

- [TRUSTED_RUNNER_COMPROMISE_RESPONSE.md](TRUSTED_RUNNER_COMPROMISE_RESPONSE.md) - quarantine, evidence capture, credential revocation, and rebuild requirements after suspected privileged-runner exposure
- [BUILDER_TRUST_BOUNDARY.md](BUILDER_TRUST_BOUNDARY.md) - runner trust tiers, self-hosted runner exception rules, and trusted ingress boundaries
- [SECRET_BROKERAGE_ROTATION.md](SECRET_BROKERAGE_ROTATION.md) - protected secret-bearing workflows, environment-gated credentials, and revocation expectations
- [CI_TESTING.md](CI_TESTING.md) - workflow security posture, trusted release lanes, and least-privilege CI design
- [RELEASE_ENGINEERING.md](RELEASE_ENGINEERING.md) - protected release paths, rollback boundaries, and operator-facing release controls

The design goal is to give responders enough authority to stop privileged automation quickly without turning emergency response into a broad standing admin path.

## Scope Classification

This document is **Advanced** guidance, not a baseline product requirement. For the current project shape, the default assumption is:

- a sole maintainer
- mostly GitHub-hosted automation
- minimal or no privileged self-hosted runner usage
- no need for multi-admin emergency delegation in the first release

Under [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md), this document is currently retained as deferred guidance and is not part of the active first-release path.

If the project stays on that path, the practical baseline is much simpler: the maintainer can disable the affected workflow, reject pending protected jobs, revoke credentials, and avoid broad standing governance machinery. The fuller model in this document matters only if privileged automation or multi-actor operations become real.

## Ownership & Review Metadata

- **Primary owner:** Project maintainer
- **Review status:** Dormant
- **Last reviewed:** 2026-06-02
- **Review cadence:** Re-review before activation; no recurring cadence while dormant
- **Review set:** [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md)
- **Review triggers:** Any material change to environment protections, runner-group policy, branch or ruleset governance, emergency authority assumptions, incident-response operating model, or decision to activate this deferred guidance

## Goals

1. Freeze privileged workflows quickly when abuse, compromise, or governance drift is suspected.
2. Separate repository-side environment control from organization-side runner and Actions policy control.
3. Keep production release and maintenance environments fail-closed even during incidents.
4. Make emergency actions auditable with durable GitHub evidence.
5. Delegate narrowly where GitHub supports it, and keep break-glass ownership minimal where GitHub does not.

## Official Research Findings (May 2026)

### Environment controls and approval boundaries

- GitHub documents that configuring environments in organization repositories requires repository `admin` access.
- GitHub documents that environment protection can require reviewers, prevent self-review, apply wait timers, restrict branches or tags, and disable administrator bypass.
- GitHub documents that environment secrets are released only after protection rules pass.
- GitHub documents that jobs referencing an environment can be approved or rejected while pending, and rejection fails the workflow.
- GitHub documents that administrator bypass of environment protection rules is only available while jobs are pending and only when the environment allows administrator bypass.
- GitHub documents that referencing a non-existent environment name in a workflow can create that environment, which makes environment naming and workflow-edit governance security-relevant.
- GitHub explicitly warns that environments do not make self-hosted runners safe; environment approval controls secret release, not runner isolation.

### Organization Actions policy and self-hosted runner policy

- GitHub documents that organization owners can enable, disable, and limit GitHub Actions for an organization.
- GitHub documents that organization Actions policy can restrict which actions and reusable workflows may run, require full-length SHA pinning for actions, and set default `GITHUB_TOKEN` permissions.
- GitHub documents that organization-level self-hosted runner policy can allow all repositories, selected repositories, or disable repository-level self-hosted runners.
- GitHub documents that disabling repository-level self-hosted runners does not block workflows from using organization-level or enterprise-level self-hosted runners.
- GitHub documents that workflows from public forks and private forks have separate approval policies, but also warns those approvals do not make self-hosted runner execution safe.

### Runner-group governance

- GitHub documents that runner groups are an organization or enterprise control surface used to limit which repositories may use self-hosted runners.
- GitHub documents that each organization has a default runner group, and runners register there unless assigned elsewhere.
- GitHub documents that runner groups can be limited to selected repositories rather than all repositories.
- GitHub warns that self-hosted runners should only be used with private repositories because forked public repository workflows can execute dangerous code on the runner machine.
- GitHub audit events for runner groups include creation, deletion, renaming, configuration changes, visibility changes, repository-access policy changes, and runner membership changes.

### Delegated organization roles

- GitHub Enterprise Cloud documents a predefined `CI/CD admin` role that can manage organization Actions policies, runners, runner groups, hosted compute network configuration, Actions secrets, Actions variables, and usage metrics.
- GitHub Enterprise Cloud documents predefined `Owner` as the full administrative role and the default role with audit-log access.
- GitHub Enterprise Cloud custom organization roles can split permissions such as `Manage organization Actions policies`, `Manage organization runners and runner groups`, and `View organization audit log`.
- GitHub Enterprise Cloud custom organization roles can also add repository-wide permissions such as `Manage GitHub Actions general settings`, `Manage runners`, and `Manage environments`.
- GitHub documents that custom organization and repository permissions are additive, which means mixed grants can silently widen access if role design is sloppy.

### Workflow freeze and audit evidence

- GitHub documents that workflows can be disabled and re-enabled through the web UI, REST API, or GitHub CLI.
- GitHub documents that disabling a workflow is an intended temporary control for abusive, broken, or operationally dangerous automation.
- GitHub documents that an organization's audit log retains 180 days of events.
- GitHub documents that only owners can access the organization audit log by default, while Enterprise Cloud custom organization roles can delegate audit-log viewing.
- GitHub documents searchable audit categories and exports for environments, organizations, repositories, teams, workflows, and runner-group changes.
- GitHub audit events directly relevant to emergency governance include `environment.update_protection_rule`, `environment.add_protection_rule`, `environment.remove_protection_rule`, `org.update_actions_settings`, `org.update_repo_self_hosted_runners_policy`, `org.runner_group_created`, `org.runner_group_updated`, `org.runner_group_removed`, `workflows.disable_workflow`, `workflows.enable_workflow`, `workflows.cancel_workflow_run`, `workflows.approve_workflow_job`, and `workflows.reject_workflow_job`.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Org owners hold all emergency powers and no one else can freeze or isolate privileged automation | Simple authority model; smallest number of actors | Slow response; owners become an operational bottleneck; no practical least-privilege delegation | Reject as default |
| Give broad repository admins and `CI/CD admin` standing authority to freeze, bypass, and modify production controls everywhere | Fast response; easy to explain | Too much standing privilege; weak separation between deployment approval and incident containment; high insider-risk blast radius | Reject |
| Split emergency control across repo-side environment custodians, org-side runner governors, and a very small owner-held break-glass path | Strongest least-privilege model; keeps production fail-closed; incident freezes remain fast | Requires explicit runbook and role design; some plans need owner participation for audit access | Selected |
| Use environment bypass as the main emergency tool for production jobs | Quick for getting a job through | Wrong objective; progresses privileged automation instead of freezing it; easy to misuse during incidents | Reject |

## Recommended Policy

### Governance principle

Emergency powers are for containment and isolation first. They are not a shortcut for shipping, publishing, or deploying around normal review.

### 1. Separate the emergency control surfaces

Emergency governance is split across four distinct control surfaces that should not be owned by the same standing role unless GitHub plan limits make that unavoidable.

#### Repository workflow freeze

Used to stop a specific privileged workflow from being triggered again.

Allowed actions:

1. Disable the affected workflow.
2. Cancel active or queued workflow runs.
3. Reject pending environment-gated jobs.

Intended owner:

- Repository-level privileged workflow maintainers for the affected trusted repository, or an Enterprise Cloud delegated role that includes repository Actions settings authority on privileged repositories.

#### Environment governance

Used to control which human approvals are required before a secret-bearing job can start.

Allowed actions:

1. Maintain required reviewers.
2. Keep `prevent self-review` enabled.
3. Disable administrator bypass on production-facing environments.
4. Restrict protected environments to protected branches or release tags.

Intended owner:

- Environment custodians for privileged repositories. In organization repositories, this is effectively a repository-admin function unless Enterprise Cloud delegation is used.

#### Runner-group isolation

Used to prevent repositories or runners from reaching privileged self-hosted capacity.

Allowed actions:

1. Change runner-group repository access from `All repositories` to `Selected repositories`.
2. Remove affected repositories from privileged runner-group access.
3. Move or remove suspect runners.
4. Disable repository-level self-hosted runner creation where that helps containment.

Intended owner:

- Organization-level runner governors using `CI/CD admin` or a custom organization role with `Manage organization runners and runner groups`.

#### Break-glass organizational controls

Used only when repository and runner-group controls are insufficient.

Allowed actions:

1. Tighten or disable organization-wide Actions policy.
2. Reassign or revoke emergency roles.
3. Export or supervise owner-only audit evidence where plan limits require it.

Intended owner:

- A minimum-sized organization owner group, with at least two human owners available for resiliency.

### 2. Named emergency roles

#### Role A: Environment Custodian

Scope:

- Repository-side privileged environments only.

Required capability:

- Repository `admin` on privileged repositories, or Enterprise Cloud delegated repository-wide `Manage environments` plus related Actions settings authority.

May do:

1. Approve or reject pending environment-gated jobs under normal policy.
2. Freeze a privileged repo by rejecting pending jobs or coordinating workflow disablement.
3. Maintain environment reviewers, branch restrictions, wait timers, and self-review prevention.

May not do:

1. Grant a repository new access to a privileged runner group.
2. Use production environment bypass as routine incident handling.
3. Change organization-wide Actions policy.

#### Role B: Runner Governor

Scope:

- Organization runner groups, runner membership, and repository self-hosted runner policy.

Required capability:

- `CI/CD admin`, or Enterprise Cloud custom role with `Manage organization runners and runner groups`.

May do:

1. Remove repositories from privileged runner-group access.
2. Move or remove runners from privileged groups.
3. Tighten organization self-hosted runner policy during incidents.

May not do:

1. Approve a protected production deployment.
2. Broaden production environment reviewer or bypass policy.
3. Hold standing owner-level governance outside runner and Actions operations.

#### Role C: Workflow Freeze Operator

Scope:

- Specific privileged workflow files and workflow runs.

Required capability:

- Repository-side workflow administration authority for the trusted repository.

May do:

1. Disable an individual workflow.
2. Re-enable it only after incident closure criteria are met.
3. Cancel runs that were queued or started on the affected path.

May not do:

1. Convert an emergency freeze into a deployment approval.
2. Modify runner-group repository access unless also holding Role B.

#### Role D: Audit Steward

Scope:

- Evidence export and verification of governance changes.

Required capability:

- Organization owner by default, or Enterprise Cloud custom role with `View organization audit log`.

May do:

1. Export the audit-log slice for the incident window.
2. Validate that freeze, rejection, runner-group, and policy actions were recorded.
3. Provide evidence for post-incident review.

May not do:

1. Unilaterally change deployment or runner policy unless holding another authorized role.

### 3. Production bypass policy

#### Production environments

1. Production publication, signing, restore-admin, and other secret-bearing maintenance environments must disable administrator bypass.
2. Production environments must enable `prevent self-review`.
3. Production incident response uses freeze actions, not deployment bypass.
4. If a production job is already pending during an incident, the default action is reject, not bypass.

#### Non-production recovery environments

Administrator bypass may exist only for explicitly designated recovery or break-glass environments where progress of a recovery action is safer than delay.

Required controls:

1. The environment must not share its credentials with ordinary release paths.
2. Bypass may be used only while the job is pending, with a mandatory incident comment recorded in GitHub.
3. A second human approver must confirm the incident record outside the workflow itself, because GitHub's bypass control alone is not a full two-person control.

### 4. Emergency change-window policy

Normal change windows may be bypassed only for containment actions.

Allowed emergency actions outside normal windows:

1. Disable a privileged workflow.
2. Cancel active or queued privileged runs.
3. Reject pending environment-gated jobs.
4. Remove repository access to a privileged runner group.
5. Disable repository-level self-hosted runner usage where needed.

Not allowed outside normal windows without the usual release governance:

1. Publishing a release.
2. Approving a production deployment.
3. Broadening environment bypass settings.
4. Adding new repositories to privileged runner groups except as part of a separately approved recovery plan.

### 5. Environment governance hardening rules

1. Pre-create and name privileged environments deliberately; do not treat workflow-authored environment creation as acceptable governance.
2. Protect `.github/workflows` with CODEOWNERS so workflow authors cannot silently create or retarget privileged environments.
3. Route secret-bearing workflows through centrally reviewed reusable workflows where possible.
4. Treat environment reviewer teams as stable named governance groups, not ad hoc individual approvers.
5. Keep production and recovery environments distinct so emergency recovery powers do not automatically imply production release powers.

### 6. Runner-group hardening rules

1. Privileged runner groups must use `Selected repositories`, not `All repositories`.
2. The default runner group must not be used for secret-bearing privileged workflows.
3. Privileged runner groups must serve private repositories only.
4. Runner-group membership and repository access changes are emergency-sensitive events and must be captured in audit evidence.
5. If the platform supports workflow-restricted runner groups, privileged groups should allow only the expected trusted workflow references.

### 7. Plan-aware delegation model

#### Preferred model: GitHub Enterprise Cloud

Use custom organization roles so authority can be separated cleanly:

1. `Manage organization runners and runner groups` for Role B.
2. `Manage organization Actions policies` only for the small group allowed to change organization-wide Actions settings.
3. `View organization audit log` for Role D.
4. Repository-wide `Manage environments` and `Manage GitHub Actions general settings` only for the narrow set of privileged repositories that need them.

#### Fallback model: no custom organization roles

1. Use the smallest possible owner set.
2. Delegate routine runner governance to `CI/CD admin` where available.
3. Keep audit export owner-run if the plan cannot delegate it.
4. Use repository-admin teams only on the privileged repositories that own protected environments.

### 8. Minimum audit evidence for every emergency action

Capture and retain the audit slice that proves:

1. Which workflow was disabled, enabled, canceled, approved, or rejected.
2. Which environment protection rule changed, if any.
3. Which runner group changed, including repository-access and runner-membership changes.
4. Which organization Actions or self-hosted runner policy changed.
5. Which human or automation actor performed the action.

Preferred query dimensions:

1. `created` for the incident window.
2. `repo` for the affected trusted repository.
3. `actor` for the responder or automation account.
4. `action` for specific workflow, environment, and runner-group events.

## Final Recommendation Stack

1. Separate emergency authority into repo-side environment custody, org-side runner governance, workflow freeze operation, and audit stewardship.
2. Keep production environments fail-closed: required reviewers on, self-review prevention on, and administrator bypass off.
3. Treat emergency governance as a containment system, not a fast lane for production deployment.
4. Use workflow disablement, run cancellation, job rejection, and runner-group isolation as the primary emergency actions.
5. Require privileged runner groups to use selected private repositories only, never the default group and never public repositories.
6. Prefer Enterprise Cloud custom organization roles to split `Manage organization Actions policies`, `Manage organization runners and runner groups`, `View organization audit log`, and repository-side environment authority.
7. If custom delegation is unavailable, keep the owner set very small and use `CI/CD admin` only for runner and Actions operations, not as a blanket production-approval role.
8. Allow change-window bypass only for freeze and isolation actions; production publish or deployment still follows normal reviewed release governance.
9. Require audit-log export and review for every emergency governance action so containment decisions remain provable after the fact.

## Three High-Value Next Design Areas

1. Required-workflow and action-allowlist governance: define the central reusable workflows, allowed external actions, SHA-pinning policy, and org-level allowlist needed so privileged automation changes remain narrow and reviewable.
2. Workflow-to-secret blast-radius inventory: define a maintained map of which repository, environment, organization, OIDC, and brokered credentials each trusted workflow can reach so emergency rotation is faster and more complete.
3. Break-glass identity and credential governance: define how owner accounts, emergency tokens, hardware keys, session controls, and out-of-band approval records are protected so the incident path itself does not become the weakest link.

## Official Sources

- GitHub Docs - Managing environments for deployment: https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments
- GitHub Docs - Reviewing deployments: https://docs.github.com/en/actions/managing-workflow-runs/reviewing-deployments
- GitHub Docs - Disabling or limiting GitHub Actions for your organization: https://docs.github.com/en/organizations/managing-organization-settings/disabling-or-limiting-github-actions-for-your-organization
- GitHub Docs - Managing access to self-hosted runners using groups: https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/manage-access
- GitHub Docs - Disabling and enabling a workflow: https://docs.github.com/en/actions/how-tos/manage-workflow-runs/disable-and-enable-workflows
- GitHub Docs - Permissions of predefined organization roles: https://docs.github.com/en/organizations/managing-peoples-access-to-your-organization-with-roles/permissions-of-predefined-organization-roles
- GitHub Docs - Permissions of custom organization roles: https://docs.github.com/en/enterprise-cloud@latest/organizations/managing-peoples-access-to-your-organization-with-roles/permissions-of-custom-organization-roles
- GitHub Docs - Reviewing the audit log for your organization: https://docs.github.com/en/organizations/keeping-your-organization-secure/managing-security-settings-for-your-organization/reviewing-the-audit-log-for-your-organization
- GitHub Docs - Audit log events for your organization: https://docs.github.com/en/organizations/keeping-your-organization-secure/managing-security-settings-for-your-organization/audit-log-events-for-your-organization
