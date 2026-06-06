# Trusted Automation Manual Validation Gate

## Overview

This document defines whether release-blocking trusted-automation changes should also require a dedicated manual validation step in the release workflow, and if so, when and how that step should be applied. It complements:

- [TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md](TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md) - blocker classes that stop release work until the doc set is re-reviewed
- [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md) - grouped review surface for the trusted-automation design set
- [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md) - current audit for whether this manual gate is active or deferred
- [CI_TESTING.md](CI_TESTING.md) - release quality gates and trusted workflow posture
- [RELEASE_ENGINEERING.md](RELEASE_ENGINEERING.md) - release classes, preflight gates, and rollback boundaries
- [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md) - required-reviewer posture, self-review limits, and fail-closed environment governance
- [SECRET_BROKERAGE_ROTATION.md](SECRET_BROKERAGE_ROTATION.md) - secret-release boundaries for protected jobs

The design goal is to use a supported, auditable human checkpoint for high-risk trusted-automation changes without turning every release into a manual ceremony.

## Scope Classification

This document is **Advanced** guidance, not a baseline product requirement.

Under [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md), this document is currently retained as deferred guidance and is not part of the active first-release path.

Use it when the project operates privileged release automation whose safety can depend on a human confirming that blocker-triggering changes were reviewed correctly. If releases remain on the baseline GitHub-hosted path with no privileged automation exception, this gate can remain dormant.

## Ownership & Review Metadata

- **Primary owner:** Project maintainer
- **Review status:** Dormant
- **Last reviewed:** 2026-06-02
- **Review cadence:** Re-review before activation; no recurring cadence while dormant
- **Review set:** [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md)
- **Review triggers:** Any material change to environment approval design, blocker classes, protected publication flow, secret-release posture for trusted automation, or decision to activate this deferred guidance

## Goals

1. Decide when blocker-triggering changes require a dedicated human validation step in the release workflow.
2. Prefer supported GitHub protection mechanisms over ad hoc manual workflow patterns.
3. Keep the manual gate narrow so routine low-risk releases stay efficient.
4. Make human approval auditable, fail-closed, and resistant to self-approval abuse where supported.
5. Preserve secure defaults by ensuring manual validation complements rather than replaces technical gates.

## Official Research Findings (May 2026)

### GitHub guidance for supported manual approval in release workflows

- GitHub Docs states that environments can require approval for a job to proceed, restrict branches, gate deployments with custom deployment protection rules, and limit access to secrets.
- GitHub Docs states that any protection rules configured for an environment must pass before a job referencing that environment is sent to a runner.
- GitHub Docs states that required reviewers can approve a job referencing an environment and that only one required reviewer is needed for the job to proceed.
- GitHub Docs states that self-review prevention can be enabled for protected environments so the person initiating the deployment cannot approve that deployment.
- GitHub Docs states that environments can be used to manually trigger specific jobs in a workflow by referencing an environment with required reviewers.
- GitHub Docs states that custom deployment protection rules can integrate third-party systems or other manual configurations, but these rules are GitHub Apps-based and, in the cited docs, still marked public preview.

### Microsoft guidance for manual gates and safe deployment

- Microsoft Learn states that safe deployment practices should use quality-gated release methods and account for routine and emergency deployments.
- Microsoft Learn states that approvals, gates, and manual validations are appropriate when teams need to verify external conditions before promotion.
- Microsoft Learn states that manual approvals should be applied when necessary to balance speed with control, not as a blanket substitute for automation.
- Microsoft Learn states that release practices should keep rollback and deployment procedures current and document roles and responsibilities when manual steps are required.

### NIST guidance for approval after significant change

- NIST SP 800-18r2 states that significant changes that alter system risk posture justify reassessment and renewed approval attention.
- NIST SP 800-18r2 states that plan review and approval are accountability mechanisms rather than optional documentation hygiene.
- NIST-style governance therefore supports a release-time human validation step only when the change class is significant enough that human approval should explicitly confirm continued authorization of the protected path.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| No dedicated manual validation step for blocker classes | Fastest workflow | Relies entirely on earlier documentation review; weak final confirmation for high-risk protected changes | Reject |
| Dedicated manual validation on every release | Strong human oversight | Too much friction; encourages approval fatigue; poor fit for sole-maintainer OSS releases | Reject |
| Dedicated manual validation only for selected blocker classes, implemented through protected environments with required reviewers or equivalent supported gates | Good balance of security and usability; auditable; uses supported platform controls | Requires classification discipline and repo/plan support for the best GitHub features | Preferred |
| Ad hoc workflow_dispatch inputs or informal comment-based approvals | Easy to improvise | Weak auditability; easy to bypass; not a strong protected control surface | Reject |

## Recommended Policy

### Core rule

Yes: release-blocking trusted-automation changes should map to a dedicated manual validation step in the release workflow, but only for blocker classes that can materially change protected publication safety even after the documentation review is updated.

### Control surface

Use a dedicated protected environment as the preferred manual-validation mechanism for this workflow stage.

Recommended properties:

1. The validation job references a dedicated environment such as `trusted-automation-review` or an equivalent protected release-control environment.
2. The environment uses required reviewers when the repository plan and visibility support that feature.
3. Self-review prevention should be enabled where supported.
4. The validation job should occur after technical release checks pass but before protected publish steps begin.
5. The manual step must never replace status checks, provenance generation, environment-protected publish approvals, or other technical gates.

### When the manual validation step is required

Require the dedicated manual validation step when the release includes any of these blocker classes:

1. **Protected environment or approval-boundary changes**
Because the change affects the control plane that decides who can approve or bypass protected publication.

2. **Protected release workflow privilege changes**
Because the change can alter what the release workflow is authorized to do or access.

3. **Credential-source and secret-release changes**
Because the change can alter how sensitive credentials are released to protected jobs.

4. **Trusted runner and runner-group changes used by release jobs**
Because the change can alter where privileged release steps execute and how exposure is constrained.

5. **Incident-driven emergency changes that remain in effect for the release**
Because emergency exceptions are high-risk and need an explicit pre-release human confirmation before being normalized.

### When the manual validation step is optional

Documentation re-review alone is sufficient, without a dedicated manual validation step, when the blocker class is limited to:

1. Artifact-trust or cache-boundary clarifications that do not change the active protected publish path for the release.
2. Risk-model updates that affect future governance but do not alter the current release's protected workflow behavior.
3. Editorial or metadata changes that were incorrectly flagged at first but do not actually change the protected release path.

### Validation content

The manual validation step should confirm, at minimum:

1. The blocker-triggering change is reflected in the affected trusted-automation docs.
2. The protected release workflow, environment approvals, and secret-release behavior match the documented model.
3. Any temporary incident controls that remain active are intentional, documented, and acceptable for this release.
4. Publication is still constrained to the intended protected workflow and environment.

### Fallback if required reviewers are unavailable

If the repository plan or visibility does not support the preferred environment-reviewer feature, do not replace it with an insecure pseudo-approval mechanism.

Use this fallback instead:

1. Keep the release blocked procedurally under [TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md](TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md).
2. Require the maintainer to complete the documented trusted-automation re-review before proceeding.
3. Keep protected publication steps environment-gated and least-privilege where available.
4. Upgrade to supported required-reviewer enforcement when the hosting model permits it.

### Security posture

The manual validation step is a narrow, final human confirmation for high-risk protected changes. It is not a general-purpose pause button, not a substitute for automated validation, and not a justification for broader standing admin bypass.

## Pros vs Cons

### Pros

- Gives high-risk blocker classes an explicit, auditable human checkpoint immediately before protected publication.
- Uses supported GitHub environment controls instead of ad hoc approval patterns.
- Reduces the chance that a sensitive release proceeds after docs were updated but the real protected path was not actually checked.
- Keeps manual friction limited to the blocker classes that justify it.

### Cons

- Adds another approval point for some releases.
- Best enforcement quality depends on repository plan and visibility support for required reviewers.
- Poor classification of blocker severity could still cause either excess friction or insufficient review.

## Final Recommendation Stack

1. Use a dedicated manual validation step for selected trusted-automation blocker classes, not for every release.
2. Implement that step through a protected environment with required reviewers and self-review prevention where supported.
3. Run the step after technical checks pass but before protected publication begins.
4. Reserve the step for changes to approvals, privileges, secret release, trusted runners, and active incident exceptions.
5. If supported reviewer enforcement is unavailable, keep the release blocked procedurally rather than inventing a weaker pseudo-approval control.

## Three More High-Value Design Areas

1. Define whether the manual validation step should emit a durable signed release note or artifact proving who approved what.
2. Define expiry and retirement rules for emergency controls that survive long enough to reach a release.
3. Define whether protected publish jobs need a distinct review environment from normal production deployment approval.

## Official Sources

- GitHub Docs: Deployment environments - https://docs.github.com/en/actions/concepts/workflows-and-actions/deployment-environments
- GitHub Docs: Deployments and environments - https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments
- GitHub Docs: Triggering a workflow - https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow
- GitHub Docs: Configuring custom deployment protection rules - https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/configure-custom-protection-rules
- Microsoft Learn: Architecture strategies for safe deployment practices - https://learn.microsoft.com/en-us/azure/well-architected/operational-excellence/safe-deployments
- Microsoft Learn: Understand release gates, checks, and approvals - https://learn.microsoft.com/en-us/azure/devops/pipelines/release/approvals?view=azure-devops
- Microsoft Learn: Architecture strategies for disaster recovery - https://learn.microsoft.com/en-us/azure/well-architected/reliability/disaster-recovery
- NIST SP 800-18r2 Initial Public Draft - https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-18r2.ipd.pdf