# Documentation Scope Labeling

## Overview

This document defines how the repository should label baseline versus advanced design guidance so the documentation set stays secure by default without implying that every hardening measure is required for a first release. It complements:

- [PROJECT.md](../../PROJECT.md) - overall project framing, scale assumptions, and top-level documentation scope
- [DOCUMENTATION_REVIEW_OWNERSHIP.md](DOCUMENTATION_REVIEW_OWNERSHIP.md) - ownership metadata, review cadence, and stale-guidance controls for active documents
- [ADVANCED_DOC_DEFER_POLICY.md](ADVANCED_DOC_DEFER_POLICY.md) - activation rule and current audit for which advanced docs stay active versus deferred
- [TRUSTED_AUTOMATION_INDEX.md](../ci/TRUSTED_AUTOMATION_INDEX.md) - maintainer-facing index for the current advanced trusted-automation document set
- [SECURITY.md](../security/SECURITY.md) - baseline security posture for a self-hosted open-source deployment
- [CI_TESTING.md](../ci/CI_TESTING.md) - baseline versus trusted-lane CI posture
- [BUILDER_TRUST_BOUNDARY.md](../ci/BUILDER_TRUST_BOUNDARY.md) - trusted builder separation and private dependency ingress
- [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](../ci/ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md) - emergency controls for privileged automation

The design goal is to keep the repository easy to navigate for a sole-maintainer, self-hosted open-source project while still preserving stronger guidance for privileged automation, supply-chain isolation, and higher-operational-complexity deployments.

## Goals

1. Keep the default path secure by default for normal self-hosted deployments.
2. Mark advanced operational hardening clearly so optional complexity is not mistaken for a baseline requirement.
3. Use a consistent document pattern that is easy to scan and maintain.
4. Keep advanced guidance discoverable instead of hiding or deleting it.
5. Preserve a single authoritative owner document per design topic.

## Official Research Findings (May 2026)

### OWASP secure-by-default guidance

- OWASP defines secure by default as software starting in a secure state without requiring extensive user configuration.
- OWASP recommends least privilege for applications, processes, and service accounts.
- OWASP recommends removing unnecessary functionality rather than shipping optional complexity as part of the default path.
- OWASP recommends keeping configuration human-readable to support auditing and controlled change management.

### Microsoft guidance for scannable, focused content

- Microsoft recommends making content scannable, concise, and focused on the user's immediate task.
- Microsoft recommends providing only the necessary information at the right time rather than overwhelming readers with extra detail.
- Microsoft recommends plain language, consistent terminology, and consistent phrasing so readers can quickly understand what is required.
- Microsoft recommends using clear headings and parallel structure so readers can find instructions quickly.

### Microsoft guidance for authoritative, well-labeled knowledge

- Microsoft recommends making it obvious where official content is located and how to navigate it.
- Microsoft recommends centralizing official content and labeling it consistently so readers can find one authoritative source of truth.
- Microsoft recommends starting each article with a summary so readers and retrieval systems understand the page's purpose and audience quickly.
- Microsoft recommends topic-focused pages with clear headings, versioning, review ownership, and lifecycle management so outdated or duplicate guidance does not crowd out current guidance.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Leave advanced topics unlabeled and rely on reader judgment | No edit churn; no extra structure | Makes privileged CI and runner-hardening pages look mandatory; increases cognitive load; weakens the baseline story | Reject |
| Move advanced topics out of the repo entirely until needed | Very simple baseline set | Loses important design work; harder to rediscover; encourages repeated re-analysis later | Reject |
| Keep advanced topics, but mark them explicitly with a consistent scope section and central policy doc | Preserves guidance, lowers ambiguity, keeps secure defaults clear, easy to extend | Requires small maintenance effort across affected docs | Preferred |
| Split the repo into separate baseline and advanced trees immediately | Strong separation; clean navigation at large scale | Too much information-architecture overhead for the current repository size and single-maintainer model | Defer |

## Recommended Policy

### Core rule

Baseline documentation must describe the secure default path for a normal self-hosted deployment. Documents whose main purpose exists because of privileged CI, self-hosted runners, trusted artifact separation, or multi-actor incident handling must be labeled **Advanced** explicitly near the top of the document.

### Required labeling pattern

Advanced documents should include a `## Scope Classification` section immediately after the overview and design-goal material.

Advanced documents should also include ownership and review metadata as defined in [DOCUMENTATION_REVIEW_OWNERSHIP.md](DOCUMENTATION_REVIEW_OWNERSHIP.md).

That section should do three things:

1. State plainly that the document is **Advanced** guidance and not a baseline product requirement.
2. Say when the document becomes relevant.
3. Say what the simpler baseline path is when the advanced machinery is not in use.

### Classification rule

Mark a document as **Advanced** when one or more of these are true:

1. The topic exists mainly because of trusted or privileged GitHub Actions workflows.
2. The topic depends on self-hosted runner lifecycle controls, runner groups, or incident response for privileged automation.
3. The topic adds supply-chain isolation, cache separation, secret brokerage, or provenance controls beyond the default single-admin release path.
4. The topic assumes multi-step operator governance that a baseline self-hosted deployment can safely defer.

Keep a document as baseline when it is needed for one of these reasons:

1. Normal installation, backup, restore, upgrade, media handling, auth, or security posture depends on it.
2. The first working release would be unsafe or unusable without it.
3. The guidance applies whether or not privileged CI or self-hosted runners exist.

### Current advanced document set

The current repository should explicitly treat the following as advanced guidance:

- [BUILDER_TRUST_BOUNDARY.md](../ci/BUILDER_TRUST_BOUNDARY.md)
- [BUILD_CACHE_TRUST_BOUNDARY.md](../ci/BUILD_CACHE_TRUST_BOUNDARY.md)
- [PRIVILEGED_ARTIFACT_HANDOFF.md](../ci/PRIVILEGED_ARTIFACT_HANDOFF.md)
- [REGISTRY_CACHE_RETENTION.md](../ci/REGISTRY_CACHE_RETENTION.md)
- [SECRET_BROKERAGE_ROTATION.md](../ci/SECRET_BROKERAGE_ROTATION.md)
- [TRUSTED_RUNNER_STATE_DISPOSAL.md](../ci/TRUSTED_RUNNER_STATE_DISPOSAL.md)
- [TRUSTED_RUNNER_COMPROMISE_RESPONSE.md](../ci/TRUSTED_RUNNER_COMPROMISE_RESPONSE.md)
- [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](../ci/ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md)

### Authoring rules

1. Keep the first paragraph and headings scannable.
2. Prefer one owner doc per topic, with cross-links instead of repeated policy text.
3. Advanced docs should carry explicit ownership and review metadata so their lifecycle is visible in the document itself.
4. Do not dilute baseline docs by importing advanced caveats unless the baseline behavior truly depends on them.
5. When a topic later becomes baseline because the product architecture changes, remove the Advanced label and update [PROJECT.md](../../PROJECT.md) in the same change.
6. Keep `CHANGELOG.md` empty until the first pushed release.

### Related governance

Scope labeling controls whether a document is baseline or advanced.

Ownership and review metadata controls whether that document is still current enough to trust.

Trusted-automation set indexing controls whether related advanced docs are reviewed together instead of drifting independently.

Use [DOCUMENTATION_SCOPE_LABELING.md](DOCUMENTATION_SCOPE_LABELING.md), [DOCUMENTATION_REVIEW_OWNERSHIP.md](DOCUMENTATION_REVIEW_OWNERSHIP.md), and [TRUSTED_AUTOMATION_INDEX.md](../ci/TRUSTED_AUTOMATION_INDEX.md) together for advanced trusted-automation guidance.

Use [ADVANCED_DOC_DEFER_POLICY.md](ADVANCED_DOC_DEFER_POLICY.md) when the repo needs to decide which advanced docs are active now and which are retained but deferred until the operating model actually grows into them.

## Current activation note

Advanced guidance can be present in the repo without being part of the active operating baseline.

If a document describes privileged automation, self-hosted runner governance, or protected publish controls that the project is not currently using, keep the document but mark it deferred through the dormancy rules in [ADVANCED_DOC_DEFER_POLICY.md](ADVANCED_DOC_DEFER_POLICY.md) and [DOCUMENTATION_REVIEW_OWNERSHIP.md](DOCUMENTATION_REVIEW_OWNERSHIP.md).

## Pros vs Cons

### Pros

- Keeps the default project story aligned with a secure-by-default self-hosted deployment.
- Preserves higher-assurance guidance without forcing every reader through privileged-CI detail.
- Makes future doc growth easier because the repo now has a concrete classification rule instead of ad hoc judgments.
- Improves scanability and retrieval by putting audience and scope near the top of each advanced page.

### Cons

- Introduces one more repository convention that has to be maintained.
- Some docs will still require judgment calls if the project architecture changes over time.
- The Advanced label can be misread as unimportant unless cross-links continue to explain when the topic becomes necessary.

## Final Recommendation Stack

1. Keep the repository baseline secure by default and sole-maintainer-first.
2. Preserve advanced CI and runner-hardening guidance in the repo rather than deleting it.
3. Mark advanced docs explicitly with a shared `Scope Classification` section near the top.
4. Use [PROJECT.md](../../PROJECT.md) plus this document as the only repo-wide classification authorities.
5. Reclassify docs only when the actual product architecture changes, not preemptively.

## Three More High-Value Design Areas

1. Define which design or workflow changes should block release work until the trusted-automation doc set is re-reviewed.
2. Decide whether advanced docs should expose an explicit implementation-status field in addition to review status.
3. Define when an advanced topic must escalate into a baseline requirement for internet-exposed or multi-admin deployments.

## Official Sources

- OWASP Developer Guide: Secure by Default - https://devguide.owasp.org/en/04-design/02-web-app-checklist/01-secure-by-default
- Microsoft Learn: Recommendations for writing user interface content - https://learn.microsoft.com/en-us/power-platform/well-architected/experience-optimization/user-interface-content
- Microsoft Learn: Optimizing SharePoint content for Employee Self-Service agents - https://learn.microsoft.com/en-us/microsoft-365/copilot/employee-self-service/optimization-sharepoint
- Microsoft Learn: Writing step-by-step instructions - https://learn.microsoft.com/en-us/style-guide/procedures-instructions/writing-step-by-step-instructions