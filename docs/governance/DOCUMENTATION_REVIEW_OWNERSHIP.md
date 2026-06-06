# Documentation Review Cadence & Ownership Metadata

## Overview

This document defines how the repository records document ownership, review cadence, and review status so advanced guidance does not quietly become stale while still keeping maintenance overhead reasonable for a sole-maintainer, self-hosted open-source project. It complements:

- [PROJECT.md](../../PROJECT.md) - project scale, documentation scope, and top-level repository rules
- [DOCUMENTATION_SCOPE_LABELING.md](DOCUMENTATION_SCOPE_LABELING.md) - baseline versus advanced classification rules
- [ADVANCED_DOC_ACTIVATION_CHECKLIST.md](ADVANCED_DOC_ACTIVATION_CHECKLIST.md) - lightweight checklist for moving deferred advanced guidance back to active use
- [TRUSTED_AUTOMATION_INDEX.md](../ci/TRUSTED_AUTOMATION_INDEX.md) - maintainer-facing grouped review surface for the current trusted-automation doc set
- [TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md](../ci/TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md) - release-blocking change classes that require trusted-automation doc-set re-review
- [SECURITY.md](../security/SECURITY.md) - baseline security posture and secure-by-default expectations
- [CI_TESTING.md](../ci/CI_TESTING.md) - workflow governance and validation posture that may trigger advanced-doc reviews
- [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](../ci/ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md) - an example advanced doc that requires recurring review

The design goal is to make ownership and staleness visible inside the documents that matter, not only in maintainer memory, so privileged-CI and runner-governance guidance can be trusted when referenced.

## Goals

1. Ensure every active advanced doc has a clearly accountable owner.
2. Make stale guidance visible before it is relied on for security-sensitive decisions.
3. Use a review cadence that is strong enough for privileged automation guidance without creating enterprise-style process overhead.
4. Tie review obligations to both elapsed time and material architectural change.
5. Keep the system secure by making authoritative guidance easy to verify and outdated guidance easy to quarantine.

## Official Research Findings (May 2026)

### Microsoft guidance for metadata and lifecycle governance

- Microsoft Learn states that rich, consistent metadata such as document type, owner, and review date improves accuracy and helps readers and retrieval systems find the most relevant, up-to-date content.
- Microsoft Learn recommends using reusable content types to standardize fields like owner and effective date.
- Microsoft Learn recommends versioning, retention and expiry policies, review notifications, and periodic audits so only current authoritative content stays published.
- Microsoft Learn recommends archiving or deleting duplicated or outdated content so one authoritative copy remains.

### GitHub guidance for ownership and review enforcement

- GitHub Docs states that CODEOWNERS defines who is responsible for paths in a repository and automatically requests those owners for review when matching files change in a pull request.
- GitHub Docs states that repository rules or branch protection can require review from code owners before merges.
- GitHub Docs states that required reviews can dismiss stale approvals when new commits are pushed, which helps prevent old approvals from validating newly changed content.
- GitHub Docs recommends rulesets as the governance surface for requiring pull requests, approvals, and code-owner review on protected content.

### NIST guidance for periodic and change-triggered review

- NIST SP 800-18r2 states that system plans require methodical reviews and periodic updates to maintain accurate information about the system, technologies, components, personnel, and controls.
- NIST SP 800-18r2 states that system plan reviews and change records provide accountability for periodic reviews and significant-change reviews across the system life cycle.
- NIST guidance therefore supports a policy that uses both scheduled review and out-of-cycle review after material change, rather than calendar review alone.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Keep advanced docs without explicit owner or review metadata | Lowest immediate effort | No accountability, hidden staleness, weak auditability, easy to trust outdated guidance by accident | Reject |
| Track document review only in maintainer memory or ad hoc notes | Flexible, no visible markup in docs | Not durable, not reviewable in pull requests, easy to drift | Reject |
| Add per-document ownership metadata plus a risk-based cadence and change-triggered reviews | Clear accountability, visible freshness, modest maintenance cost, strong fit for a docs-first repo | Requires periodic upkeep and disciplined status changes | Preferred |
| Build a separate external ticketing or CMDB workflow before adding any doc metadata | Strong enterprise governance potential | Excessive overhead for the current repo, poor fit for a sole maintainer | Defer |

## Recommended Policy

### Core rule

Every active advanced document must declare who owns it, when it was last reviewed, how often it must be reviewed, which changes trigger an immediate review, and whether the document is still current.

### Required metadata block

Advanced documents should include a `## Ownership & Review Metadata` section near the top of the document, immediately after `## Scope Classification`.

That section must contain at least these fields:

- **Primary owner**
- **Review status**
- **Last reviewed**
- **Review cadence**
- **Review triggers**

Optional fields such as backup owner, implementation status, or linked issue are allowed if the repo later needs them.

When multiple advanced docs describe one operational trust boundary, they may also declare an optional `Review set` field that points to a shared maintainer-facing index.

### Review status values

Use only these values:

- **Current** - reviewed and trusted for present design use
- **Needs review** - no longer trustworthy as authoritative guidance until re-reviewed
- **Superseded** - replaced by a newer owner document
- **Dormant** - intentionally inactive guidance for a path the project is not currently using

### Cadence tiers

For the current project shape:

1. Active advanced docs for privileged CI, secrets, runner governance, cache trust, or artifact trust boundaries should be reviewed every 90 days.
2. The same docs must be reviewed immediately when a material change affects their assumptions.
3. Dormant or superseded docs are excluded from the 90-day cycle, but they must be clearly marked and must not present themselves as current guidance.

This keeps the maintenance burden small while still treating security-sensitive operational guidance as something that can expire.

### Mandatory review triggers

An out-of-cycle review is required when any of the following happens:

1. The release or maintenance workflow privilege boundary changes.
2. The project introduces, removes, or materially changes self-hosted runner usage.
3. Credential sourcing changes, including OIDC, Vault, package permissions, or registry authentication.
4. GitHub Actions rules, branch protections, or approval flows materially change.
5. An incident, postmortem, or near miss reveals that the document's assumptions are incomplete or wrong.
6. The product changes from sole-maintainer assumptions toward multi-admin or internet-exposed operations.

### Repository enforcement model

For the current sole-maintainer project, the baseline enforcement model is lightweight:

1. Put ownership and review metadata in the document itself.
2. Keep advanced docs in the normal pull-request workflow.
3. Treat stale metadata as a required documentation fix before relying on the document for new design decisions.

If the project grows beyond one active maintainer, add GitHub governance on top:

1. Add `CODEOWNERS` entries for advanced docs or their directories.
2. Require pull requests for protected branches.
3. Enable review from code owners for advanced-governance docs.
4. Dismiss stale approvals when new commits materially change those docs.

### Review sets for related advanced docs

When multiple advanced docs collectively define one trusted operational surface, the repo should maintain a single maintainer-facing index for that set.

The index should:

1. List the current member docs.
2. Explain the grouping logic.
3. Define set-level review cadence and triggers.
4. Make it obvious when one document change should trigger sibling-document review.
5. Link any change classes that block release work until the set is re-reviewed.

For the current repository, that grouped surface is defined in [TRUSTED_AUTOMATION_INDEX.md](../ci/TRUSTED_AUTOMATION_INDEX.md).

The release-blocking subset of those changes is defined in [TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md](../ci/TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md).

### Current repository application

The trusted-automation cluster should still carry ownership and review metadata now, but for the current repository it is presently deferred rather than active under [ADVANCED_DOC_DEFER_POLICY.md](ADVANCED_DOC_DEFER_POLICY.md).

That deferred cluster currently includes:

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

Those docs should use `Dormant` review status until the operating model actually activates the privileged path they govern.

### Review workflow for this repo

1. When an advanced doc is touched for a substantive design change, update its `Last reviewed` date in the same patch.
2. If an advanced doc is older than its cadence and has not been re-reviewed, change `Review status` to **Needs review** until the review is completed.
3. If a document is no longer the authoritative source, mark it **Superseded** and link the replacement near the top.
4. If a document describes an inactive path that the project is deliberately not using, mark it **Dormant** instead of pretending it is current.
5. During quarterly maintenance, audit the active advanced-doc set for overdue reviews and duplicated guidance.
6. When an audit moves an advanced doc from active to deferred, update its scope note and metadata in the same patch so it no longer reads like a baseline release requirement.
7. When a document moves from **Dormant** back to **Current**, use [ADVANCED_DOC_ACTIVATION_CHECKLIST.md](ADVANCED_DOC_ACTIVATION_CHECKLIST.md) and complete the metadata, linked-doc, and validation updates in the same patch series.

## Pros vs Cons

### Pros

- Makes staleness visible inside the documents instead of depending on maintainer memory.
- Keeps the governance lightweight enough for a sole maintainer while still protecting security-sensitive guidance.
- Aligns scheduled review with change-triggered review, which is closer to how real operational risk evolves.
- Creates a clean future path to CODEOWNERS or ruleset enforcement if the repo gains more maintainers.

### Cons

- Adds maintenance overhead to advanced docs even when the underlying feature set changes slowly.
- Review dates can themselves drift if the maintainer stops updating them consistently.
- A metadata block does not enforce correctness by itself; it still depends on disciplined review.

## Final Recommendation Stack

1. Put ownership and review metadata directly in every active advanced doc.
2. Use a 90-day cadence for active advanced guidance in this repo.
3. Require out-of-cycle review on material changes to runners, secrets, trust boundaries, approvals, or incidents.
4. Use `Current`, `Needs review`, `Superseded`, and `Dormant` as the only review-status values.
5. Add CODEOWNERS and ruleset enforcement only if the project grows beyond a sole-maintainer model.

## Three More High-Value Design Areas

1. Decide whether docs should expose an explicit "implementation status" field so readers can separate current design intent from already-shipped behavior.
2. Define when advanced-doc governance should escalate from maintainer discipline to enforced CODEOWNERS or ruleset controls.
3. Define how incident-driven temporary exceptions are tracked and formally retired after trusted-automation re-review.

## Official Sources

- Microsoft Learn: Optimizing SharePoint content for Employee Self-Service agents - https://learn.microsoft.com/en-us/microsoft-365/copilot/employee-self-service/optimization-sharepoint
- GitHub Docs: About code owners - https://docs.github.com/en/enterprise-server@3.18/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners
- GitHub Docs: Maintaining codebase standards in a GitHub Copilot rollout - https://docs.github.com/en/enterprise-cloud@latest/copilot/tutorials/roll-out-at-scale/govern-at-scale/maintain-codebase-standards
- GitHub Docs: Available rules for rulesets - https://docs.github.com/en/enterprise-server@3.19/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets
- NIST SP 800-18r2 Initial Public Draft - https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-18r2.ipd.pdf