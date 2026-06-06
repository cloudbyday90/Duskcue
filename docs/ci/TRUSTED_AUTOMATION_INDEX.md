# Trusted Automation Index

## Overview

This document defines the maintainer-facing index for the repository's advanced trusted-automation guidance so related documents can be reviewed as a set instead of drifting independently. It complements:

- [PROJECT.md](../../PROJECT.md) - top-level scope and documentation architecture rules
- [CI_TESTING.md](CI_TESTING.md) - baseline CI posture and release quality gates
- [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md) - current audit for whether this advanced cluster is active or deferred
- [ADVANCED_DOC_ACTIVATION_CHECKLIST.md](../governance/ADVANCED_DOC_ACTIVATION_CHECKLIST.md) - lightweight checklist for reactivating this deferred cluster when the operating model changes
- [DOCUMENTATION_SCOPE_LABELING.md](../governance/DOCUMENTATION_SCOPE_LABELING.md) - baseline versus advanced classification rules
- [DOCUMENTATION_REVIEW_OWNERSHIP.md](../governance/DOCUMENTATION_REVIEW_OWNERSHIP.md) - per-document ownership metadata and review cadence
- [TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md](TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md) - release-blocking trusted-automation changes that must not outrun documentation re-review
- [TRUSTED_AUTOMATION_MANUAL_VALIDATION.md](TRUSTED_AUTOMATION_MANUAL_VALIDATION.md) - dedicated human validation rule for blocker classes that need explicit release-time confirmation
- [BUILDER_TRUST_BOUNDARY.md](BUILDER_TRUST_BOUNDARY.md) - trusted build separation and private dependency ingress
- [SECRET_BROKERAGE_ROTATION.md](SECRET_BROKERAGE_ROTATION.md) - credential sourcing and revocation policy for trusted workflows
- [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md) - emergency governance for privileged automation

The design goal is to create one authoritative entry point for the current trusted-automation design set so maintainers can review privilege boundaries, secret paths, cache trust, runner controls, and incident procedures together instead of relying on scattered cross-links.

## Scope Classification

This document is **Advanced** guidance, not a baseline product requirement.

Under [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md), this document is currently retained as deferred guidance and is not part of the active first-release path.

Use it when the project maintains any privileged automation guidance beyond the baseline GitHub-hosted CI path. If the project stays on a simple baseline release flow with no privileged self-hosted runner exceptions or complex trust boundaries, this index can remain dormant along with the documents it groups.

## Ownership & Review Metadata

- **Primary owner:** Project maintainer
- **Review status:** Dormant
- **Last reviewed:** 2026-06-02
- **Review cadence:** Re-review before activation; no recurring cadence while dormant
- **Review set:** Trusted automation
- **Review triggers:** Any material change to release privilege boundaries, self-hosted runner usage, secret brokerage, cache trust, artifact handoff, environment approvals, incident-response assumptions, or decision to activate this deferred guidance

## Goals

1. Provide one authoritative landing page for the trusted-automation document set.
2. Make set-level review obligations explicit so related advanced docs are re-evaluated together.
3. Reduce the chance that one privileged-automation document is updated while a sibling doc silently drifts.
4. Preserve secure defaults by keeping advanced trusted-automation guidance clearly separate from baseline product guidance.
5. Keep the system secure by making trust-boundary assumptions, related owner docs, and review triggers easy to inspect in one place.

## Official Research Findings (May 2026)

### Microsoft guidance for information architecture and authoritative content

- Microsoft Learn states that simple information architecture, clear document structure, detailed metadata, and well-governed access improve the quality and trustworthiness of authoritative knowledge.
- Microsoft Learn recommends modeling top-level, hub, and local navigation on user mental models and aligning information architecture to common tasks and roles.
- Microsoft Learn recommends clear, action-oriented labels in navigation, page titles, and metadata to improve findability.
- Microsoft Learn recommends reusable metadata fields such as owner and review date so current authoritative content can be filtered and reviewed consistently.

### Microsoft guidance for lifecycle and freshness

- Microsoft Learn recommends periodic audits, versioning, retention and expiry policies, and owner notifications so only current authoritative content remains published.
- Microsoft Learn recommends archiving or deleting duplicated or outdated content so one authoritative copy remains.
- Microsoft Learn recommends metadata that signals freshness, such as last updated or review information, because context and staleness matter to retrieval and trust.

### GitHub guidance for grouped governance and protected review

- GitHub Docs states that rulesets are a named list of rules that can apply to one repository or multiple repositories, which makes grouped governance a first-class concept rather than an ad hoc per-file pattern.
- GitHub Docs states that CODEOWNERS automatically requests review from the responsible owners when matching files change.
- GitHub Docs states that rulesets and branch protections can require review from code owners before protected content is merged.
- GitHub Docs states that stale approvals can be dismissed when new changes land, which reinforces the principle that related protected guidance should be re-reviewed after material changes.
- GitHub Docs recommends more specific rulesets closer to the protected content because local maintainers have more awareness of requirements for their own code.

### NIST guidance for consolidated plans and significant-change review

- NIST SP 800-18r2 states that consolidated plans can encompass multiple related requirement domains when they share one governed system context.
- NIST SP 800-18r2 states that plans require periodic review to maintain continued accuracy and relevancy after authorization.
- NIST SP 800-18r2 states that reviews must also occur after significant changes and that review logs should record who reviewed what and when.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Leave related trusted-automation docs as independent files with only cross-links | Minimal edit work; preserves one-owner-doc model | Easy to review incompletely; no set-level status; hard to see the full trust boundary at once | Reject |
| Merge all trusted-automation guidance into one large document | One page to review | Loses topic ownership boundaries; harder to maintain; weakens scannability | Reject |
| Keep owner docs separate, but add one maintainer-facing index that defines the set, review cadence, and grouped triggers | Preserves owner-doc boundaries; gives one authoritative entry point; supports secure set-level review | Adds one more governance page to maintain | Preferred |
| Build a workflow or ticketing system before defining a simple index | Strong future automation potential | Too much overhead for the current repo and sole-maintainer model | Defer |

## Recommended Policy

### Core rule

The trusted-automation docs remain separate owner documents, but they must be reviewed through a single index whenever the trust boundary or operating model changes materially.

### What belongs in the trusted-automation set

Include a document in this set when its main purpose is to define or protect privileged automation, trusted build separation, self-hosted runner exceptions, artifact trust boundaries, secret brokerage, emergency workflow governance, or incident response for trusted automation.

### Current set members

The current repository keeps this set deferred under [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md) until privileged automation becomes part of the real operating model.

If that changes, start the reactivation work with [ADVANCED_DOC_ACTIVATION_CHECKLIST.md](../governance/ADVANCED_DOC_ACTIVATION_CHECKLIST.md) before marking this set current again.

#### Build and credential trust

- [BUILDER_TRUST_BOUNDARY.md](BUILDER_TRUST_BOUNDARY.md)
- [BUILD_CACHE_TRUST_BOUNDARY.md](BUILD_CACHE_TRUST_BOUNDARY.md)
- [PRIVILEGED_ARTIFACT_HANDOFF.md](PRIVILEGED_ARTIFACT_HANDOFF.md)
- [REGISTRY_CACHE_RETENTION.md](REGISTRY_CACHE_RETENTION.md)
- [SECRET_BROKERAGE_ROTATION.md](SECRET_BROKERAGE_ROTATION.md)

#### Runner lifecycle and incident handling

- [TRUSTED_RUNNER_STATE_DISPOSAL.md](TRUSTED_RUNNER_STATE_DISPOSAL.md)
- [TRUSTED_RUNNER_COMPROMISE_RESPONSE.md](TRUSTED_RUNNER_COMPROMISE_RESPONSE.md)

#### Governance and approval boundaries

- [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md)

### Review workflow

1. Start any trusted-automation design review from this index, not from an individual doc in isolation.
2. If one set member changes substantively, check whether the change invalidates assumptions in sibling documents before closing the review.
3. During the 90-day review cycle, review the full active set together and update each member's `Last reviewed` field in the same maintenance pass.
4. If one set member becomes dormant or superseded, update this index in the same patch so the set definition stays authoritative.
5. If the project no longer operates privileged automation, mark this index and its member docs dormant instead of leaving them apparently current.

### Set-level triggers

Re-review the set whenever any of the following occurs:

1. The release workflow gains or loses privileged build or publication behavior.
2. Self-hosted runner usage, runner groups, or environment protection changes materially.
3. Secret sourcing, OIDC trust, package access, or registry auth changes.
4. Artifact promotion, cache reuse, or attestation policy changes.
5. An incident, postmortem, or near miss suggests that current trusted-automation assumptions are incomplete.
6. The project moves beyond the current sole-maintainer operating model.

### Release-blocking subset

Some trusted-automation changes are strong enough that release work must pause until the set is re-reviewed.

That subset is defined in [TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md](TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md).

Some blocker classes also require a dedicated release-time human validation step rather than documentation re-review alone. That decision rule is defined in [TRUSTED_AUTOMATION_MANUAL_VALIDATION.md](TRUSTED_AUTOMATION_MANUAL_VALIDATION.md).

### Security posture

This index does not create new authority or bypass any document owner. Its security value comes from making the full privileged-automation boundary visible, reducing partial updates, and giving maintainers one authoritative review entry point before making sensitive workflow or credential decisions.

### Future enforcement path

If the project grows beyond one active maintainer:

1. Add `CODEOWNERS` coverage for the trusted-automation docs or their directory.
2. Require pull requests and code-owner review for the set.
3. Use repository rules or rulesets so stale approvals are dismissed when material changes land.
4. Keep this index as the human-readable grouping authority even if automated enforcement is added.

## Pros vs Cons

### Pros

- Creates one authoritative, scannable entry point for the full trusted-automation boundary.
- Preserves separate owner docs while still enabling grouped review.
- Reduces the chance of half-updated security guidance after workflow or credential changes.
- Fits the repo's current scale without adding enterprise-style tooling overhead.

### Cons

- Adds another governance document that can itself become stale if ignored.
- Still relies on maintainer discipline until CODEOWNERS or ruleset enforcement is adopted.
- Requires judgment about which docs belong in the set as the architecture evolves.

## Final Recommendation Stack

1. Keep trusted-automation topics as separate owner docs.
2. Add one authoritative trusted-automation index for grouped review and navigation.
3. Mark each set member with a shared `Review set` reference back to this index.
4. Review the full set every 90 days and after material trust-boundary changes.
5. Add CODEOWNERS or ruleset enforcement only if the project grows past the sole-maintainer model.

## Three More High-Value Design Areas

1. Decide whether trusted-automation docs should expose an explicit implementation-status field alongside review status.
2. Define when trusted-automation governance should move from a single review set to multiple review bundles, such as build trust, incident response, and release governance.
3. Define how temporary emergency controls are reconciled back into the trusted-automation set after an incident.

## Official Sources

- Microsoft Learn: Optimizing SharePoint content for Employee Self-Service agents - https://learn.microsoft.com/en-us/microsoft-365/copilot/employee-self-service/optimization-sharepoint
- Microsoft Learn: Recommendations for following design standards - https://learn.microsoft.com/en-us/power-platform/well-architected/experience-optimization/design-standards
- Microsoft Learn: Create architecture design diagrams - https://learn.microsoft.com/en-us/azure/well-architected/architect-role/design-diagrams
- GitHub Docs: About rulesets - https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/about-rulesets
- GitHub Docs: About code owners - https://docs.github.com/en/enterprise-server@3.21/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners
- GitHub Docs: Available rules for rulesets - https://docs.github.com/enterprise-server@3.21/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets
- GitHub Docs: Maintaining codebase standards in a GitHub Copilot rollout - https://docs.github.com/en/copilot/tutorials/roll-out-at-scale/govern-at-scale/maintain-codebase-standards
- NIST SP 800-18r2 Initial Public Draft - https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-18r2.ipd.pdf