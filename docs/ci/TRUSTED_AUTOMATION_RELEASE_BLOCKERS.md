# Trusted Automation Release-Blocking Review Gates

## Overview

This document defines which trusted-automation changes are serious enough to stop release-candidate promotion or stable release publication until the trusted-automation document set is re-reviewed. It complements:

- [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md) - grouped review surface for the trusted-automation design set
- [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md) - current audit for whether this blocker policy is active or deferred
- [DOCUMENTATION_REVIEW_OWNERSHIP.md](../governance/DOCUMENTATION_REVIEW_OWNERSHIP.md) - ownership metadata and recurring review cadence
- [CI_TESTING.md](CI_TESTING.md) - release quality gates and trusted workflow posture
- [RELEASE_ENGINEERING.md](RELEASE_ENGINEERING.md) - release classes, preflight gates, and rollback expectations
- [TRUSTED_AUTOMATION_MANUAL_VALIDATION.md](TRUSTED_AUTOMATION_MANUAL_VALIDATION.md) - when blocker classes also require a dedicated manual validation step in the release workflow
- [BUILDER_TRUST_BOUNDARY.md](BUILDER_TRUST_BOUNDARY.md) - trusted build separation and private dependency ingress
- [SECRET_BROKERAGE_ROTATION.md](SECRET_BROKERAGE_ROTATION.md) - trusted workflow credential sourcing and revocation
- [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md) - protected environment and emergency governance rules

The design goal is to keep release automation from outrunning its governing documentation when a change materially affects privilege, credentials, protected approvals, artifact trust, or trusted runner assumptions.

## Scope Classification

This document is **Advanced** guidance, not a baseline product requirement.

Under [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md), this document is currently retained as deferred guidance and is not part of the active first-release path.

Use it when the project has privileged or protected automation whose safety depends on the trusted-automation doc set being current. If releases remain on a simple baseline GitHub-hosted path with no privileged automation exception, these gates can remain dormant with the rest of the trusted-automation set.

## Ownership & Review Metadata

- **Primary owner:** Project maintainer
- **Review status:** Dormant
- **Last reviewed:** 2026-06-02
- **Review cadence:** Re-review before activation; no recurring cadence while dormant
- **Review set:** [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md)
- **Review triggers:** Any material change to protected release workflows, environment approvals, secret release paths, artifact trust boundaries, self-hosted runner usage, incident-driven emergency controls, or decision to activate this deferred guidance

## Goals

1. Define the exact trusted-automation changes that must halt release work until documentation is re-reviewed.
2. Keep the release gate narrow enough to be usable, but strong enough to stop unsafe privileged-automation drift.
3. Tie release blocking to risk-bearing changes, not to every editorial doc update.
4. Keep the release-blocking rule auditable and easy to explain in pull requests and release preparation.
5. Support a secure system by ensuring documentation remains aligned with protected release behavior.

## Official Research Findings (May 2026)

### GitHub guidance for protected merges and deployment approval gates

- GitHub Docs states that protected branches can require pull request reviews, code-owner reviews, status checks, conversation resolution, and approval from someone other than the last pusher before merge.
- GitHub Docs states that required status checks can block merges and can be bound to a specific expected app source.
- GitHub Docs states that environments can require reviewer approval before a deployment job starts, and protected deployments can wait in a pending state until approval is granted.
- GitHub Docs states that rulesets are the grouped governance surface for protected content and can enforce reviews and checks before changes reach protected branches.

### Microsoft guidance for safe deployment and gated promotion

- Microsoft Learn states that safe deployment practices should use quality-gated release methods and explicit predeployment checks such as code review, security scans, and compliance checks.
- Microsoft Learn states that approvals, gates, and manual validations are appropriate when teams need to ensure external conditions are satisfied before a release proceeds.
- Microsoft Learn states that teams should document dependencies, define clear practices for releases, and keep rollback and deployment procedures updated as changes occur.

### NIST guidance for significant change and reauthorization-style review

- NIST SP 800-18r2 states that plans require periodic review and also ad hoc review when significant changes alter security, privacy, or supply-chain risk.
- NIST SP 800-18r2 states that weaknesses, incidents, changes to key personnel, changes to system categorization, or changes that significantly alter risk posture justify reassessment and renewed approval attention.
- NIST guidance therefore supports a policy that blocks protected release progression when trust-boundary changes have occurred but the governing plan set has not yet been re-reviewed.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Never block release for documentation re-review | Fastest release path | Allows privileged automation to drift beyond documented controls; weak auditability | Reject |
| Block every release whenever any trusted-automation doc changes | Very conservative | Too noisy; punishes editorial changes; encourages process avoidance | Reject |
| Block release only for material trusted-automation changes that affect privilege, approvals, secrets, artifact trust, or runner risk | Strong risk alignment; workable for a sole maintainer; clear rationale | Requires judgment to classify changes correctly | Preferred |
| Require external change-management tooling before release gating docs | Strong enterprise traceability | Excessive overhead for the current repo and operating model | Defer |

## Recommended Policy

### Core rule

Release work must pause when a change materially alters the trusted-automation risk boundary and the affected trusted-automation doc set has not yet been re-reviewed to match that change.

### Changes that block release work

The following classes are release-blocking until the trusted-automation set is re-reviewed and updated where needed:

1. **Protected release workflow privilege changes**
Changes that widen, remove, or materially reorder permissions, secrets access, publish authority, attestation authority, or trusted reusable workflow behavior in release or maintenance workflows.

2. **Protected environment or approval-boundary changes**
Changes to required reviewers, self-review rules, wait timers, deployment protection rules, branch deployment restrictions, or emergency bypass assumptions for protected release environments.

3. **Credential-source and secret-release changes**
Changes to OIDC trust, Vault brokerage, package or registry credentials, environment-secret usage, or any other release-time credential path that changes what can be issued, by whom, or under what claims.

4. **Trusted runner and runner-group changes**
Introduction, removal, or material reconfiguration of self-hosted runners, runner groups, disposal assumptions, compromise-response assumptions, or trusted hardware paths used by release or maintenance jobs.

5. **Artifact trust-boundary changes**
Changes to artifact promotion, rebuild-versus-promote policy, attestation requirements, provenance verification, cache-to-release boundaries, or any rule that affects whether a publishable artifact can be trusted.

6. **Incident-driven emergency changes**
Temporary emergency controls, freeze logic, bypass restrictions, or incident-response exceptions introduced during or after an incident that affect how privileged automation is allowed to operate.

7. **Risk-model changes that invalidate current assumptions**
Changes in operating model, repository governance, maintainer count, internet-exposure posture, or external policy requirements that make the current trusted-automation guidance incomplete or misleading.

### Changes that do not block release by themselves

These do not block release unless they also change one of the classes above:

1. Pure wording, structure, or typo fixes in trusted-automation docs.
2. Cross-link improvements that do not alter requirements.
3. Metadata-only updates such as `Last reviewed` after a completed review.
4. Non-privileged CI improvements that do not affect release, maintenance, or trusted automation behavior.

### Release-blocking decision rule

Treat a change as release-blocking when all three are true:

1. The change affects a protected or privileged automation path.
2. The change can alter trust, authorization, or release safety assumptions.
3. A reviewer cannot honestly say the current trusted-automation doc set still describes reality without updates.

If one of those conditions is false, the change is not automatically release-blocking.

### Required response when a blocker is triggered

1. Pause release-candidate promotion or stable release publication.
2. Re-review the trusted-automation set starting from [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md).
3. Update every affected owner doc and `Last reviewed` metadata in the same patch series.
4. Record any doc whose guidance is no longer current as `Needs review`, `Dormant`, or `Superseded` until resolved.
5. If the blocker class requires explicit human release-time confirmation, run the manual validation step defined in [TRUSTED_AUTOMATION_MANUAL_VALIDATION.md](TRUSTED_AUTOMATION_MANUAL_VALIDATION.md).
6. Resume release work only after the blocking change and the doc-set review are aligned.

### Repository application

For this repository, release-blocking review mainly applies to changes touching:

- [BUILDER_TRUST_BOUNDARY.md](BUILDER_TRUST_BOUNDARY.md)
- [BUILD_CACHE_TRUST_BOUNDARY.md](BUILD_CACHE_TRUST_BOUNDARY.md)
- [PRIVILEGED_ARTIFACT_HANDOFF.md](PRIVILEGED_ARTIFACT_HANDOFF.md)
- [REGISTRY_CACHE_RETENTION.md](REGISTRY_CACHE_RETENTION.md)
- [SECRET_BROKERAGE_ROTATION.md](SECRET_BROKERAGE_ROTATION.md)
- [TRUSTED_RUNNER_STATE_DISPOSAL.md](TRUSTED_RUNNER_STATE_DISPOSAL.md)
- [TRUSTED_RUNNER_COMPROMISE_RESPONSE.md](TRUSTED_RUNNER_COMPROMISE_RESPONSE.md)
- [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md)

### Security posture

This policy is intentionally narrower than a generic documentation gate. It exists because privileged release automation can become unsafe when documentation no longer reflects who can publish, which credentials are released, what trust boundary artifacts cross, or which controls limit runner or environment exposure.

## Pros vs Cons

### Pros

- Ties release blocking to high-risk change classes instead of broad documentation churn.
- Keeps privileged automation and its governing docs synchronized before release.
- Gives maintainers a defensible stop-ship rule for security-sensitive workflow changes.
- Fits a sole-maintainer repo better than heavyweight external change-management tooling.

### Cons

- Requires judgment to classify borderline changes correctly.
- Can slow release work when privileged automation is being actively redesigned.
- Still depends on disciplined human review unless future workflow enforcement is added.

## Final Recommendation Stack

1. Block release only for material trusted-automation changes, not for general doc edits.
2. Treat privilege, approval, credential, runner, artifact-trust, incident, and risk-model changes as the release-blocking classes.
3. Re-review the full trusted-automation set from [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md) before candidate promotion or stable release continues.
4. Wire the blocker into release quality gates in [CI_TESTING.md](CI_TESTING.md) and release guidance in [RELEASE_ENGINEERING.md](RELEASE_ENGINEERING.md).
5. Add stronger automated enforcement only if the maintainer model or release complexity grows.

## Three More High-Value Design Areas

1. Decide whether incident-created temporary controls need their own expiry and reconciliation metadata.
2. Define which trusted-automation changes require a fresh rollback-proof evidence capture before release resumes.
3. Define whether trusted-automation manual validation should emit a durable release artifact or signed review record.

## Official Sources

- GitHub Docs: About protected branches - https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches
- GitHub Docs: Deployments and environments - https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments
- GitHub Docs: Deploying with GitHub Actions - https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/control-deployments
- GitHub Docs: About rulesets - https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/about-rulesets
- Microsoft Learn: Architecture strategies for safe deployment practices - https://learn.microsoft.com/en-us/azure/well-architected/operational-excellence/safe-deployments
- Microsoft Learn: Architecture strategies for formalizing development practices - https://learn.microsoft.com/en-us/azure/well-architected/operational-excellence/formalize-development-practices
- Microsoft Learn: Understand release gates, checks, and approvals - https://learn.microsoft.com/en-us/azure/devops/pipelines/release/approvals?view=azure-devops
- NIST SP 800-18r2 Initial Public Draft - https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-18r2.ipd.pdf