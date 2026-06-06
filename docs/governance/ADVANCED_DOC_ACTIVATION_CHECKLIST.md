# Advanced Document Activation Checklist

## Overview

This document defines a lightweight checklist for moving deferred advanced guidance back to active use when the project's real operating model starts depending on it. It complements:

- [PROJECT.md](../../PROJECT.md) - top-level project scope and documentation architecture
- [ADVANCED_DOC_DEFER_POLICY.md](ADVANCED_DOC_DEFER_POLICY.md) - which advanced docs are currently deferred and why
- [DOCUMENTATION_SCOPE_LABELING.md](DOCUMENTATION_SCOPE_LABELING.md) - baseline versus advanced classification rules
- [DOCUMENTATION_REVIEW_OWNERSHIP.md](DOCUMENTATION_REVIEW_OWNERSHIP.md) - review-status values, metadata rules, and dormancy handling
- [TRUSTED_AUTOMATION_INDEX.md](../ci/TRUSTED_AUTOMATION_INDEX.md) - grouped surface for the deferred trusted-automation cluster
- [CI_TESTING.md](../ci/CI_TESTING.md) - release gates that may need to become active once a deferred path is reactivated
- [RELEASE_ENGINEERING.md](../ci/RELEASE_ENGINEERING.md) - release-path assumptions that must stay aligned with active guidance

The design goal is to let the sole maintainer reactivate dormant advanced docs in a controlled, secure, low-friction way without creating enterprise-style approval overhead.

## Goals

1. Define the smallest defensible checklist for moving a deferred advanced doc back to active use.
2. Ensure reactivation happens only when the governed behavior is real, not speculative.
3. Keep security controls, release gates, and doc metadata aligned in the same patch series.
4. Require one focused validation step before dormant guidance is treated as current again.
5. Keep the underlying system secure by activating higher-risk guidance only alongside the controls it assumes.

## Official Research Findings (May 2026)

### GitHub guidance for higher-risk workflow activation

- GitHub Docs states that self-hosted runners are systems you deploy and manage yourself, and that runner scope can exist at the repository, organization, or enterprise level.
- GitHub Docs states that self-hosted runners can be persistently compromised by untrusted workflow code and that wider runner scope can increase blast radius.
- GitHub Docs states that runner groups can restrict which workflows, repositories, and organizations can access self-hosted runners.
- GitHub guidance therefore supports activating deferred automation docs only when the real control surface exists and its scope boundaries are explicitly known.

### Microsoft guidance for authoritative content lifecycle

- Microsoft Learn recommends clear information architecture, action-oriented labels, and reusable metadata so current authoritative content is easy to find and trust.
- Microsoft Learn states that owner and review-date metadata improve retrieval quality and help readers identify the most relevant current guidance.
- Microsoft Learn recommends periodic audits and lifecycle handling so outdated or inactive content does not crowd out current guidance.
- Microsoft guidance therefore supports changing the doc's visible status, metadata, and navigation context at the same time reactivation occurs.

### NIST guidance for significant-change review

- NIST SP 800-18r2 states that system plan reviews provide accountability for periodic reviews and significant changes over the life cycle.
- NIST SP 800-18r2 states that organization-designated reviewers verify the accuracy and completeness of plan information and its alignment after changes.
- NIST guidance therefore supports a change-triggered checklist that updates the authoritative plan set before newly risky behavior is treated as current.

### CISA guidance for secure-by-default activation and validation

- CISA states that secure-by-design and secure-by-default products should be secure out of the box without requiring extra compensating controls to reach a safe starting point.
- CISA recommends validating security controls rather than assuming they work because a policy or design exists.
- CISA guidance therefore supports a checklist that activates advanced docs only after the relevant technical controls and one focused validation step are in place.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Reactivate deferred docs ad hoc whenever the maintainer thinks they might be needed | Lowest ceremony | Easy to drift, easy to miss linked docs or controls, weak auditability | Reject |
| Reactivate the full deferred cluster whenever any one advanced feature becomes relevant | Simple mental model | Too much overhead for a sole maintainer; turns one real change into a large review event | Reject |
| Use a lightweight activation checklist that updates only the affected doc set and validates the relevant controls | Smallest reliable process, good fit for this repo, keeps security explicit | Requires disciplined same-patch updates | Preferred |
| Require external change-management workflow before reactivation | Stronger formal traceability | Unnecessary complexity for a docs-first sole-maintainer project | Defer |

## Recommended Policy

### Core rule

Move a deferred advanced doc from **Dormant** to **Current** only in the same patch series that makes the governed behavior real, imminent, and technically supported.

Do not reactivate docs preemptively just because the project might use the feature later.

### Lightweight activation checklist

Complete these checks before a deferred advanced doc becomes active:

1. **Confirm the activation trigger is real**
The implementation, release path, or operating model must now actually depend on the advanced behavior. A hypothetical future idea is not enough.

2. **Identify the affected doc set**
Update the owner doc plus any linked index, blocker, release-gate, or sibling docs whose assumptions change when the advanced path becomes active.

3. **Confirm the control surface exists and is least-privilege**
If activation depends on self-hosted runners, protected environments, credential brokerage, or privileged publication flows, verify that the relevant boundary is actually configured or implementation-ready rather than merely planned.

4. **Flip the metadata in the same patch**
Change `Review status` from **Dormant** to **Current**, update `Last reviewed`, restore the active review cadence, and revise the scope note so the doc no longer reads as deferred guidance.

5. **Update linked governance docs only where the active path now depends on them**
If reactivation changes release blockers, manual validation, the trusted-automation index, or release-quality gates, update those docs in the same patch series.

6. **Run one focused validation step**
Use the cheapest validation that can prove the activated assumption is real. Examples: a protected workflow dry run, a scoped configuration review, a runner-scope check, or a narrow release-path test.

7. **Fail closed if the controls are not ready**
If the implementation or validation shows the required control surface is incomplete, keep the doc **Dormant** and do not present it as active guidance yet.

### Minimum activation evidence by advanced-doc type

Use these lightweight evidence expectations:

1. **Self-hosted runner docs**
Confirm where the runner lives, which repos can reach it, and what boundary limits access.

2. **Protected publication or approval docs**
Confirm the protected environment, approval rule, or release boundary is actually configured for the intended path.

3. **Credential brokerage docs**
Confirm the credential source, scope, and revocation path are defined and usable.

4. **Artifact trust or cache-boundary docs**
Confirm the build or publish path now really depends on that trust split rather than the simpler baseline path.

### Repository application

For the current repository, this checklist should stay lightweight:

1. No separate ticketing workflow is required.
2. No multi-person approval ceremony is required by default.
3. The maintainer should make the doc, metadata, and linked-gate updates in one patch series.
4. The first focused validation can be procedural or technical, as long as it genuinely tests the newly active assumption.

### Security posture

This checklist is intentionally small, but it is not optional ceremony. Its security value comes from refusing to mark higher-risk guidance current until the project has actually crossed the trust boundary that guidance assumes.

## Pros vs Cons

### Pros

- Keeps activation tied to real implementation instead of speculative planning.
- Fits a sole-maintainer project without creating enterprise-style approvals.
- Makes the doc state, linked governance, and control validation move together.
- Helps prevent dormant high-risk guidance from being reactivated without the security boundaries it assumes.

### Cons

- Still depends on maintainer discipline.
- Some activation cases will require judgment about which sibling docs are affected.
- A lightweight checklist is weaker than full automated enforcement if the project later grows in complexity.

## Final Recommendation Stack

1. Reactivate deferred advanced docs only when the governed behavior becomes real in the operating model.
2. Use one lightweight checklist rather than ad hoc judgment or a heavyweight approval system.
3. Change the doc status, metadata, and linked governance docs in the same patch series.
4. Require one focused validation step before the doc becomes current again.
5. If the required controls are not actually ready, keep the doc dormant and fail closed.

## Three More High-Value Design Areas

1. Define a matching deactivation checklist for moving active advanced docs back to dormant cleanly.
2. Decide whether activated advanced docs need a short implementation-status field alongside `Review status`.
3. Define when trusted-automation activation becomes large enough to require a set-level review rather than a single-doc activation.

## Official Sources

- GitHub Docs: Self-hosted runners - https://docs.github.com/en/actions/concepts/runners/self-hosted-runners
- GitHub Docs: Secure use reference - https://docs.github.com/en/actions/reference/security/secure-use
- Microsoft Learn: Optimizing SharePoint content for Employee Self-Service agents - https://learn.microsoft.com/en-us/microsoft-365/copilot/employee-self-service/optimization-sharepoint
- NIST SP 800-18r2 Initial Public Draft - https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-18r2.ipd.pdf
- CISA Cybersecurity Advisory AA26-097A - https://www.cisa.gov/news-events/cybersecurity-advisories/aa26-097a