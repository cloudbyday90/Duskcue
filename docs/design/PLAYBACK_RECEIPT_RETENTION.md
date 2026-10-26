# Playback reservation receipt retention

## Status and authority

Research and decision review, October 26, 2026, against `eaaace1` on main. No implementation or retention qualification is claimed. The proposed first increment is bounded reservation replay retention before production VOD traffic; the user has been asked whether to take that prerequisite first or include the VOD backend protocol now. Numerical policies, final scope and the complete contract remain unconfirmed.

[Shared admission](PLAYBACK_ADMISSION.md) owns the already confirmed viewer/lease/cleanup semantics. [VOD output ownership](VOD_OUTPUT_LIFECYCLE.md), [build durability](../ci/LINUX_BUILD_DURABILITY.md), [streaming](STREAMING.md) and [root lifecycle](SSE_LIFECYCLE.md) retain their authority. One viewer per logical session, preparation/pause counting, sixty-second renewal, explicit revocation and confirmed physical cleanup before release are settled. Production admission remains unattached, and Live TV playback stays separate. Main only; no release or PR merge.

## Source findings

Three read-only explorations inspected backend integration, every online client family and durable receipt/history ownership. They changed no files, ran no builds/tests, accessed no private feed and stopped no service.

| Boundary | Current evidence and implication |
|---|---|
| Durable request identity | `services/playback_admission/types.rs::Request::encode` requires UUIDv7 and binds account/authentication session/profile/revisions/transport/subject. It does not impose an issuance-age policy. `ledger.rs::claim` can insert an absent authorized request. A released row currently prevents revival; deleting it without another durable refusal rule would allow the same old request to allocate again. This is a prerequisite for a proposed pruner, not an existing production vulnerability: neither pruning nor production admission is enabled. |
| Receipt schema | `20261026180000_playback_admission.sql` stores held/retiring/released state and a constrained `released_at`; it has no pruning cutoff, terminal-age index or storage quota. Existing count indexes cover nonreleased reservations. |
| Physical cleanup | `owner/lifecycle.rs::resource` releases only after trusted completion and a durable transition. Failed or uncertain cleanup stays counted. Lease expiry and observer loss are not deletion evidence. |
| Stop/history | `domains/playback/stop.rs` commits frozen `duskcue_stop_result_v1` history, then awaits cleanup; repeated Stop replays history and retries cleanup. A stopped history row is not confirmed resource release. Removing that snapshot alone also changes its replayed result. |
| Other retention domains | Play history is monthly partitioned. `workers/partition_management.rs` creates current/future partitions; it does not implement deletion. Reservation replay, play history/frozen Stop results, instance fencing and operator audit retention need explicit separate policies. |
| Owner and scheduler | Admission has synchronized registration/closure, retained tasks, eight database command slots and bounded transactions. Root already joins an attached owner before pool closure. Scheduler executor tasks use separate spawn paths, so registering a periodic job there does not establish observed root completion. |
| Backend VOD migration | Current start launches work before inserting play history; PlaybackActor drops stronger authentication-session/revision context. Direct `/stream/{file}` has no logical viewer identity. HLS checks user/profile/generation but not exact admission/authentication session. Start needs canonical command binding and retained readiness; status must not call a path that can allocate an unknown request. |
| Online clients | Shared web/Tauri reconstructs direct URLs and handles ordinary renewal errors as bookkeeping. Flutter, Android/iOS ambient and Android/Fire TV have independent start/renew/Stop paths; several discard failed cleanup or renewal outcomes. Web/Flutter quality/track changes currently Stop then Start. Whole-family migration is larger than one shared-player edit. Offline downloads remain outside the settled online viewer ledger. |
| Existing server-time context | `domains/auth/service/session_context.rs` returns current account/session/profile revisions, transport and database `observed_at`. Future request-ID generation can use a scoped server-time observation; a UUID timestamp is still not authentication or proof of real creation time. |

## Official research

URLs were discovered through MCP search or its retrieved links and opened on October 26, 2026. Product windows and quotas below are Duskcue decisions to review, not requirements imposed by these sources.

| Primary source | Application |
|---|---|
| [RFC 9562](https://www.rfc-editor.org/rfc/rfc9562.html), [PostgreSQL 18 UUID functions](https://www.postgresql.org/docs/18/functions-uuid.html) | UUIDv7 contains time ordering, not an application replay policy or permission. PostgreSQL can extract its timestamp and version; generation details can differ. Explicit eligibility and clock handling are required before deleting the row that prevents reuse. |
| [PostgreSQL constraints](https://www.postgresql.org/docs/18/ddl-constraints.html) | A time-dependent CHECK does not continuously enforce expiration. Use transactionally serialized insertion/pruning policy; static CHECKs can still protect shape and state relationships. |
| [PostgreSQL explicit locking](https://www.postgresql.org/docs/18/explicit-locking.html?trk=article-ssr-frontend-pulse_little-text-block), [bounded DELETE](https://www.postgresql.org/docs/18/sql-delete.html) | Use consistent lock ordering and indexed finite batches. Bound lock/statement/transaction duration, and distinguish a definite commit from uncertain acknowledgment. A batch limit is different from a total storage ceiling. |
| [Tokio graceful shutdown](https://tokio.rs/tokio/topics/shutdown) | Close admission, signal cancellation and observe retained completion. Reuse the existing owner rather than assuming a periodic scheduler task is already joined. The repository's installed versions remain authoritative. |
| [RFC 9110](https://httpwg.org/specs/rfc9110) | A lost POST response does not prove rollback or permit a fresh start identity. Reconciliation reads must not allocate or renew; later client/API work needs explicit expired/unknown/uncertain outcomes. |
| [WCAG status messages](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html?trk=article-ssr-frontend-pulse_little-text-block) | Later replay-expiry, renewal and capacity UI should provide meaningful status without unnecessary focus changes. This proposed backend slice would not establish client accessibility conformance. |

## Alternatives and recommendation

| Option | Benefits | Costs and limits |
|---|---|---|
| Retain every released receipt | Preserves current replay refusal. | Unbounded cumulative storage under real traffic. |
| Delete solely after an age/TTL | Small implementation. | Reopens absent old IDs unless a separate refusal rule is durable; expiry does not prove cleanup. |
| Permanent tombstone for every deleted receipt | Can preserve exact old-ID refusal. | Moves unbounded cardinality to another table. |
| Monotonic durable fresh-request cutoff plus finite terminal retention and backlog bounds | Can discard old terminal rows without reviving their IDs, retain a defined reconciliation window and refuse new growth when maintenance falls behind. | Requires atomic claim/prune coordination, explicit clock-error behavior, quota policy and real race/fault evidence. |
| Full VOD/backend/all-client migration in this increment | Delivers an end-to-end feature once every path is qualified. | Couples retention with command readiness, direct/HLS authority, source replacement, transport adoption and several native client runtimes. |

Recommendation for review: complete the reservation receipt boundary first, then migrate the VOD protocol/delivery and clients against it. Prefer a database-persisted cutoff that never silently retreats, checked only for genuinely absent fresh claims; keep existing active/replayed requests distinct. Advance the necessary refusal boundary transactionally before deleting eligible released rows. A clock rollback must not turn a pruned identity into a new start. This is a proposed architecture, not implemented behavior.

Time retention alone is not a hard row bound. The reviewed contract must distinguish the reconciliation window, finite prune work and admission/backlog limits. New-start backpressure must preserve renewal, retirement and cleanup of existing reservations. Prune eligibility must never include held, retiring, foreign-uncertain or cleanup-unconfirmed rows merely because a timer expired.

Recommended stack: locked Rust/Cargo 1.99.1, SQLx 0.9/PostgreSQL 18, UUIDv7, Tokio/Tokio-util and the existing owner/transaction services. Use focused policy/cutoff, quota, bounded database pruning and retained maintenance modules. Add no cache service, new dependency, generic settings escape hatch or broad singleton. Keep current three-second database, eight-command and root 30/60-second bounds unless separately reviewed.

## Open decisions and acceptance work

1. Confirm retention-first scope or include the VOD backend contract now.
2. Choose how long a definitively released receipt remains exactly reconcilable, and the expired outcome afterward. Long-lived active viewers must not expire because their original start is old.
3. Choose fresh-request age/future-skew behavior or an alternative server-issued identity scheme; define durable clock rollback handling and later client server-time use.
4. Choose household/global retained-row ceilings and maintenance/backpressure behavior without falsely reporting a viewer-cap refusal.
5. Bound maintenance batches/cadence/ownership, and distinguish reservation pruning from instance/operator/history retention. Do not add incidental deletion of those other domains.

Qualification should use disposable PostgreSQL 18 and genuine admission services: old-ID replay after deletion, fresh and existing requests at boundaries, clock rollback/advance, multi-instance claims/pruning, quota races and recovery, commit-acknowledgment loss, caller cancellation, root cutoff/drain and restart. Prove that held/retiring/uncertain ownership is retained and failed pruning cannot revive an identity or free an unconfirmed resource. Inspect a realistically sized indexed query plan. Run relevant existing admission/root/playback regressions under unchanged guards; private feeds are unnecessary for this retention work.

No new implementation test or production enablement is claimed at this research checkpoint. After the open decisions are settled, record a complete concrete contract and obtain the shared-understanding confirmation required by the [repository review procedure](../../.agents/skills/grilling/SKILL.md), then implement and qualify it. Standard legal-header authorization remains in effect; explanatory source comments remain excluded.

The documentation checkpoint passes staged whitespace, 611 local link targets and supplied-private-value exclusion checks across five task documents. The previously qualified runtime source inventory is unchanged. No runtime build/test was repeated for these research-only edits.

## PR applicability

GitHub MCP search at this checkpoint returned zero open repository PRs. There is no eligible random selection, applicable patch or stale Dependabot rebase. [Earlier maintenance](../operations/DEPENDABOT_MAINTENANCE.md) already closed superseded PR 5 without merge. No PR, alert, dependency version or branch was changed by this research.
