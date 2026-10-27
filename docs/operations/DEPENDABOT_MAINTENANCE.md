# Dependabot maintenance

## October 26, 2026 outcome

The user explicitly requested stale Dependabot PR cleanup/rebase as part of the next platform increment. GitHub MCP search of all open repository PRs returned only [PR 5](https://github.com/cloudbyday90/Duskcue/pull/5), so uniform random selection from that singleton selected it for local applicability review.

The PR was authored by `dependabot[bot]`, targeted main and remained open/unmerged at immutable head `c018d77b7f09543e5960a571dc43e2f061cedf0a`. MCP filename discovery returned exactly `clients/desktop/package-lock.json` and `clients/web/package-lock.json`. Both exact file patches and both local lockfiles on `3127055` were inspected.

| Dependency | PR target | Both current lockfiles | Result |
|---|---|---|---|
| PostCSS | 8.5.23 | 8.5.30 | Already superseded. |
| Nano ID | 3.3.16 | 3.3.20 | Already superseded. |

No applicable local patch remained. Applying the PR would downgrade current dependency versions; rebasing it would not add the requested update to main. The current head/author/state were rechecked immediately before the authorized cleanup. The PR was closed through GitHub MCP, then fetched again to verify `state=closed`, `merged=false` and the same immutable head. No merge, new branch, branch rewrite, package change, comment command, dependency-ignore rule or release was created. Future Dependabot updates remain enabled. A fresh repository-wide open-PR search after closure returned zero open PRs.

## Official guidance and recommendation

[GitHub's Dependabot management guidance](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/manage-your-dependency-security/manage-dependabot-prs) explains automatic rebasing and its age/extra-commit limits. [GitHub's troubleshooting guidance](https://docs.github.com/en/code-security/reference/supply-chain-security/troubleshoot-dependabot/dependabot-errors) describes updates that remain open after the default branch has already been updated, and checking current dependency evidence before closing an unnecessary PR. Both pages were discovered through MCP search and opened on October 26.

Prefer rebasing a still-applicable update when its dependency target remains necessary and can be locally qualified. Close a verified superseded update instead of downgrading main or merging obsolete lockfiles. Preserve future updates and review evidence. Closure is separate from advisory dismissal; no security alert was dismissed by this cleanup.

The [VOD lifecycle authority](../design/VOD_OUTPUT_LIFECYCLE.md) owns the subsequent verified worker/output increment. The cleanup above remains independent evidence and does not qualify later runtime changes.

The follow-on [receipt-retention research](../design/PLAYBACK_RECEIPT_RETENTION.md) against `eaaace1` repeated the GitHub MCP open-PR search on October 26 and found zero candidates. No random PR patch, stale Dependabot rebase, merge or dependency change applied at that checkpoint.

The October 27 retention implementation repeated the repository-wide GitHub MCP search and again found zero open PRs. No candidate can be randomly selected or rebased; the superseded PR 5 closure above remains complete. Dependency locks and future Dependabot updates remain unchanged.

The October 27 [VOD command/delivery review](../design/VOD_COMMAND_DELIVERY.md), against `84fa21e`, repeated that GitHub MCP search with the same zero-open-PR result. No random implementation or stale Dependabot rebase applies; this research checkpoint changes no dependency and creates no PR mutation.

The completed October 27 VOD command implementation repeated GitHub MCP open-PR discovery and still found zero candidates. No random local patch, stale rebase, merge, dependency change or alert dismissal applied.
