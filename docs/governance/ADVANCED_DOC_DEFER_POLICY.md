# Advanced Document Deferral Policy

## Overview

This document defines how the repository should audit advanced guidance and mark anything beyond the current baseline as deferred rather than leaving it mixed into the active operating model. It complements:

- [PROJECT.md](../../PROJECT.md) - top-level product scope and documentation architecture
- [DOCUMENTATION_SCOPE_LABELING.md](DOCUMENTATION_SCOPE_LABELING.md) - baseline versus advanced classification rules
- [DOCUMENTATION_REVIEW_OWNERSHIP.md](DOCUMENTATION_REVIEW_OWNERSHIP.md) - metadata, review status values, and dormancy handling
- [ADVANCED_DOC_ACTIVATION_CHECKLIST.md](ADVANCED_DOC_ACTIVATION_CHECKLIST.md) - lightweight checklist for moving deferred advanced guidance back to active use
- [TRUSTED_AUTOMATION_INDEX.md](../ci/TRUSTED_AUTOMATION_INDEX.md) - the current advanced trusted-automation cluster
- [CI_TESTING.md](../ci/CI_TESTING.md) - baseline CI posture and release quality gates
- [RELEASE_ENGINEERING.md](../ci/RELEASE_ENGINEERING.md) - release path assumptions and protected publication guidance
- [SECURITY.md](../security/SECURITY.md) - baseline secure-by-default posture for the product

The design goal is to keep the repository secure by default and easy to navigate without deleting valuable advanced design work that may become relevant later.

## Goals

1. Define when advanced docs are active versus deferred.
2. Keep the default project story aligned with the first-release baseline rather than optional privileged automation.
3. Preserve stronger guidance in the repo without pretending it is part of today's operating model.
4. Make dormancy explicit so inactive advanced docs are not mistaken for current release requirements.
5. Keep the system secure by minimizing standing complexity in the default path.

## Official Research Findings (May 2026)

### OWASP secure-by-default guidance

- OWASP defines secure by default as software starting in a secure state without requiring extensive user configuration.
- OWASP recommends least privilege for applications, processes, and service accounts.
- OWASP recommends removing unnecessary functionality rather than shipping optional complexity as part of the default path.
- OWASP recommends keeping security configuration human-readable to support auditing and controlled change management.

### GitHub Actions guidance for self-hosted runner risk

- GitHub Docs states that self-hosted runners do not have guarantees around running in ephemeral clean virtual machines and can be persistently compromised by untrusted workflow code.
- GitHub Docs states that compromise of organization-level or enterprise-level self-hosted runner environments can have wide impact across repositories.
- GitHub Docs states that runner groups and protected access boundaries are compensating controls, not reasons to normalize self-hosted runners as the default path.
- GitHub Docs therefore supports treating runner-governance design as conditional guidance that should only become active when the project truly adopts that operating model.

### Microsoft guidance for authoritative content lifecycle

- Microsoft Learn states that clear information architecture, owner metadata, and review date metadata help readers and retrieval systems find the most relevant current guidance.
- Microsoft Learn recommends retention, expiry, and periodic audits so only current authoritative content stays published.
- Microsoft Learn recommends archiving or deleting duplicate or outdated content so one authoritative copy remains.
- Microsoft Learn also gives concrete examples of marking content status with values such as current, archived, or superseded.

### CISA secure-by-design guidance

- CISA states that secure-by-design and secure-by-default products should be secure out of the box without requiring customers to add extra compensating controls just to reach a safe starting point.
- CISA states that manufacturers should change default settings to prevent exposing administrative interfaces to the internet.
- CISA states that unused or unnecessary features should not remain part of the exposed operating surface when they increase risk.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Keep all advanced docs active and current even when the project is not using those features | Maximum retained detail | Makes optional privileged automation look like part of the baseline; increases maintenance and cognitive load | Reject |
| Delete advanced docs until the project implements them | Simplest active surface | Throws away useful design work and forces re-analysis later | Reject |
| Keep advanced docs in the repo, but explicitly mark non-active ones as deferred through `Dormant` status and a central audit policy | Preserves design work, keeps baseline clean, supports secure defaults, matches existing metadata model | Requires a clear activation rule and occasional audit | Preferred |
| Split the repository into separate active and deferred trees now | Strong separation | Too much information-architecture overhead for the current repo size and stage | Defer |

## Recommended Policy

### Core rule

An advanced document stays active only when the current project operating model actually uses the privileged, protected, or higher-complexity behavior that the document governs.

If the project is not currently using that behavior, keep the document in the repo but mark it deferred by setting its review status to **Dormant**.

### Activation rule

Move an advanced document from deferred to active only when at least one of these becomes true in the real operating model:

1. Release or maintenance jobs run on trusted self-hosted runners or runner groups.
2. Protected publication depends on special environment approvals, secret-release choreography, or privileged reusable workflows beyond the simple baseline path.
3. The release process depends on artifact trust separation, privileged rebuild-versus-promote rules, or specialized cache trust boundaries.
4. Incident handling for release automation requires explicit emergency governance beyond a sole-maintainer procedural pause.
5. The project grows beyond the current simple single-maintainer baseline in a way that makes the advanced control surface operationally real.

### Deferred-state rule

When a doc is deferred:

1. Keep it in the repo.
2. Mark its `Review status` as **Dormant**.
3. State near the top that the document is not part of the active baseline or first-release path.
4. Exclude it from the active 90-day review cycle until an activation trigger is met.
5. Re-review it before any implementation or release starts depending on it.

### Audit outcome for the current repository

For the current project shape, the baseline remains a secure-by-default, sole-maintainer, GitHub-hosted path with minimal privileged automation complexity.

That means the following trusted-automation cluster should be retained but marked deferred now:

- [TRUSTED_AUTOMATION_INDEX.md](../ci/TRUSTED_AUTOMATION_INDEX.md)
- [TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md](../ci/TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md)
- [TRUSTED_AUTOMATION_MANUAL_VALIDATION.md](../ci/TRUSTED_AUTOMATION_MANUAL_VALIDATION.md)
- [BUILDER_TRUST_BOUNDARY.md](../ci/BUILDER_TRUST_BOUNDARY.md)
- [BUILD_CACHE_TRUST_BOUNDARY.md](../ci/BUILD_CACHE_TRUST_BOUNDARY.md)
- [PRIVILEGED_ARTIFACT_HANDOFF.md](../ci/PRIVILEGED_ARTIFACT_HANDOFF.md)
- [REGISTRY_CACHE_RETENTION.md](../ci/REGISTRY_CACHE_RETENTION.md)
- [SECRET_BROKERAGE_ROTATION.md](../ci/SECRET_BROKERAGE_ROTATION.md)
- [TRUSTED_RUNNER_STATE_DISPOSAL.md](../ci/TRUSTED_RUNNER_STATE_DISPOSAL.md)
- [TRUSTED_RUNNER_COMPROMISE_RESPONSE.md](../ci/TRUSTED_RUNNER_COMPROMISE_RESPONSE.md)
- [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](../ci/ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md)

The governing classification docs remain active because they describe how the repo handles scope and dormancy, not because the trusted-automation path is active.

### Security posture

Deferring inactive advanced docs is not a weakening step. It is a secure-by-default measure that reduces the chance that optional privileged automation becomes an accidental requirement or that readers assume the project already operates a higher-risk workflow model than it really does.

## Pros vs Cons

### Pros

- Keeps the active baseline aligned with the current first-release path.
- Preserves advanced design work without promoting it into an unnecessary default control surface.
- Uses the existing `Dormant` review status instead of inventing a second governance system.
- Reduces the chance that inactive runner and release-governance rules are mistaken for current operational requirements.

### Cons

- Requires occasional judgment about when a feature has become real enough to activate its docs.
- Leaves more documents in the repo, even though some are not currently active.
- A dormant doc can still confuse readers if its scope note is weak or inconsistent.

## Final Recommendation Stack

1. Keep the advanced trusted-automation cluster in the repo.
2. Mark the cluster deferred now by using `Dormant` review status rather than leaving it active.
3. Keep baseline scope authority in [PROJECT.md](../../PROJECT.md), [DOCUMENTATION_SCOPE_LABELING.md](DOCUMENTATION_SCOPE_LABELING.md), and [DOCUMENTATION_REVIEW_OWNERSHIP.md](DOCUMENTATION_REVIEW_OWNERSHIP.md).
4. Reactivate deferred advanced docs only when the operating model actually adopts the privileged behavior they govern, using [ADVANCED_DOC_ACTIVATION_CHECKLIST.md](ADVANCED_DOC_ACTIVATION_CHECKLIST.md).
5. Re-review any deferred doc before the first implementation or release that depends on it.

## Three More High-Value Design Areas

1. Decide whether dormant advanced docs need a shared visual marker in their titles or overview sections for faster scanning.
2. Define which baseline docs should carry short links to deferred advanced guidance without importing its complexity.
3. Define whether reactivating a deferred doc should require a short implementation-status note in addition to the review-status change.

## Official Sources

- OWASP Developer Guide: Secure by Default - https://devguide.owasp.org/en/04-design/02-web-app-checklist/01-secure-by-default
- GitHub Docs: Secure use reference - https://docs.github.com/en/actions/reference/security/secure-use
- Microsoft Learn: Optimizing SharePoint content for Employee Self-Service agents - https://learn.microsoft.com/en-us/microsoft-365/copilot/employee-self-service/optimization-sharepoint
- CISA Cybersecurity Advisory AA26-097A - https://www.cisa.gov/news-events/cybersecurity-advisories/aa26-097a