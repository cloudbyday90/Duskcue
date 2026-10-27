# Playback reservation receipt retention

## Status and authority

Research began October 26, 2026 against `eaaace1`; research checkpoint `df4e475` is pushed on main. The user confirmed retention-first scope and the four product choices below. The complete concrete contract is ready for shared-understanding confirmation; no implementation or retention qualification is claimed yet.

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

The confirmed direction is the reservation receipt boundary first, followed by VOD protocol/delivery and client migration. Use a database-persisted cutoff that never silently retreats, checked only for genuinely absent fresh claims; keep existing active/replayed requests distinct. Advance the necessary refusal boundary transactionally before deleting eligible released rows. A clock rollback must not turn a pruned identity into a new start. The concrete architecture below remains unimplemented pending complete-contract confirmation.

Time retention alone is not a hard row bound. The reviewed contract must distinguish the reconciliation window, finite prune work and admission/backlog limits. New-start backpressure must preserve renewal, retirement and cleanup of existing reservations. Prune eligibility must never include held, retiring, foreign-uncertain or cleanup-unconfirmed rows merely because a timer expired.

Recommended stack: locked Rust/Cargo 1.99.1, SQLx 0.9/PostgreSQL 18, UUIDv7, Tokio/Tokio-util and the existing owner/transaction services. Use focused policy/cutoff, quota, bounded database pruning and retained maintenance modules. Add no cache service, new dependency, generic settings escape hatch or broad singleton. Keep current three-second database, eight-command and root 30/60-second bounds unless separately reviewed.

## Confirmed product decisions

1. Complete bounded playback-reservation receipt retention first. VOD delivery/protocol, client migration, production cap enablement and Live TV playback remain separate.
2. Preserve a definitively released receipt for at least 24 hours after its first confirmed release/cleanup evidence. Then it may be deleted and yield an explicit expired result. Never prune held, retiring or cleanup-uncertain ownership by age. Frozen Stop/history records remain separate.
3. A genuinely absent new UUIDv7 request must be within five minutes behind or two minutes ahead of database time. Check existing receipts first. Persist a rejection cutoff that never moves backward; clock errors refuse new starts rather than reopen pruned IDs. Future clients can anchor generation to scoped server-time context. UUID time is eligibility data, not authentication.
4. Count all retained reservation states toward initial limits of 4,096 rows per household and 65,536 total. Refuse additional growth when full while preserving existing renewal, retirement and cleanup. Never evict a receipt before its retention window to make space.

## Complete contract for shared-understanding confirmation

### Scope and modular ownership

Extend the normal-build internal admission service with focused retention policy/types, bounded database pruning/accounting and owned maintenance modules. Keep handlers, public viewing APIs and clients unchanged. Do not attach admission during normal production bootstrap, add a management/prune override, enable playback caps or fetch private feeds. Reuse locked dependencies and approved standard legal headers only. Add no explanatory code comments or unrelated scheduler repair.

Keep reservation replay retention distinct from play-session/frozen Stop history, instance identities, operator events and unrelated analytics retention. Do not delete those records incidentally. Retain existing instance fencing, current authority, resource completion and replay semantics; a receipt remains neither a credential nor delivery permission.

### Atomic database invariants and migration

Install schema/backfill/guards atomically in a forward migration. Preserve all existing identities, fingerprints, states, deadlines, first release timestamps and uncertain ownership. Seed exact counters from existing rows. If legacy data is already above a ceiling, retain it and refuse further growth until eligible pruning lowers the count; do not delete protected data or clamp accounting to manufacture compliance.

Use a durable database retention record for the monotonic cutoff and global count, plus bounded household accounting. Enforce insertion eligibility, quotas, immutable identity/accounting keys and first-release evidence in database guards as well as typed service flow. Counter changes and reservation insertion/deletion commit together. A refusal, rollback, duplicate request or uncertain acknowledgment must not create a second reservation or grant speculative capacity. Protect against direct identity mutation, unsafe deletion and TRUNCATE bypass; no automatic floor reset or permissive counter reinitialization is allowed. Missing or invalid retention metadata refuses new mutation rather than becoming unlimited policy.

An existing matching row is reconciled before the first-use age rule; mismatched reuse remains refused. Old active reservations retain their settled renewal/authority rules regardless of original UUID age. For absent identities, use millisecond-consistent database time, the five-minute/two-minute bounds and the persisted floor. Once an identity is behind the committed rejection floor, it can never allocate again after pruning, including after restart or clock rollback. Retention reads/replays do not extend the lease or reset the original release time. Distinguish expired request, invalid clock/first-use time and retained-row backpressure from viewer-cap refusal, generic storage failure and indeterminate commit. Use narrow fixed database error mapping without exposing raw SQL or fingerprints.

### Eligibility and bounded pruning

Prune only released rows whose first release is at least 24 elapsed hours old and whose exact UUID is permanently excluded by the durable floor. Use hour/minute durations and explicit millisecond boundaries. Legacy UUIDs can contain far-future times; those receipts must remain until safe eligibility, even when release age alone would permit deletion.

Maintain a trigger-derived indexed eligibility field that accounts for both release age and UUID exclusion, rather than scanning past a large future-ID backlog with a residual time filter. Protect its derivation and release evidence from arbitrary mutation; recheck deletion safety against committed database state. Advance the floor under compatible locks before deletion. Use an ordered candidate window of at most 128 rows and bounded row-lock acquisition/skip behavior so a locked prefix does not cause an unbounded search for another 128 rows. One attempt performs one finite batch, not a loop until the table is empty. Row ceilings are not a promise about total PostgreSQL disk bytes or vacuum behavior.

### Maintenance, failures and root

Register maintenance through the admission owner's synchronized gate and tracker. Permit one bounded startup attempt and one automatic attempt every sixty seconds, with no overlapping or accumulated catch-up batches. Reserve one operation for maintenance and seven for ordinary admission/renew/use/retirement work, preserving the existing maximum of eight. Share the regular-operation constant with monitor rotation to preserve fairness across the 128 retained resource entries. Keep current pool/container and three-second database, 500-ms lock, 1,500-ms statement and three-second transaction bounds.

Classify ordinary maintenance contention/storage/uncertain-commit outcomes and retry later using current authoritative state. They must not set the resource-failure flag that rejects existing renewals or cancel healthy viewers. Quotas continue to stop unsafe growth while cleanup is delayed. Unexpected lost maintenance completion is retained separately: refuse further new starts and fail root completion without falsely treating existing physical resources as cleaned. Accepted operations stay owned through observer cancellation; no detached replacement or concurrent retry may escape the operation/pool bound.

Root closes maintenance admission, cancels future ticks and joins accepted SQL/connection work before pool closure under unchanged thirty-second drain and sixty-second pool-close budgets. Preserve existing nonzero deadline, cleanup and second-signal behavior. No scheduled executor is assumed to be root-owned merely because it was registered.

### Qualification and delivery

Use genuine PostgreSQL 18 and admission services for deterministic policy boundaries, actual old-ID replay after deletion, first-use time errors, long-lived existing receipts, floor rollback refusal, clock rollback/advance, restart and multi-instance races. Exercise quota equality/exhaustion/recovery, legacy over-limit backfill, future-ID/young/held/retiring/uncertain rows, immutable-field and direct SQL/bulk/TRUNCATE refusals, and unchanged first-release evidence. Inspect actual plans/row/buffer work on a realistically sized mixed backlog and locked candidate prefix. Backfill fixtures start at the real preceding schema and apply the real forward migration; add no production clock or trigger bypass.

Use the existing wire-fault fixtures for BEGIN and both COMMIT acknowledgment outcomes, counter/floor/deletion atomicity, observer loss and root drain with real database locks. Verify reserved maintenance capacity, monitor fairness, ordinary-failure isolation and unexpected completion loss. Test actual native Windows and managed Linux execution with disposable PostgreSQL; extend existing qualification-only executable checkpoints and narrowly allowlisted owned runtime commands as needed. Preserve source/image/hash/mode checks, two Cargo jobs, existing compiler/memory/log/container guards and fresh runtime execution. Retain the existing identity-checked temporary Classifarr handoff only if required by Linux admission, with healthy restoration in finally; Harmoniarr remains untouched.

Run appropriate existing admission/root/VOD regressions, normal/qualification compilation, library tests, strict-lint baseline comparison, formatting, copyright, staged whitespace, local links and private-value exclusion. Record exact qualified source/images/receipts, corrected attempts and limitations; commit and push all task changes on main without a release or PR merge. The next increment is the VOD backend command/delivery contract and client migration against this bounded ledger.

The [repository review procedure](../../.agents/skills/grilling/SKILL.md) requires confirmation of this complete shared understanding before implementation. All four product choices above are already confirmed and do not need to be asked again.

The documentation checkpoint passes staged whitespace, 611 local link targets and supplied-private-value exclusion checks across five task documents. The previously qualified runtime source inventory is unchanged. No runtime build/test was repeated for these research-only edits.

## PR applicability

GitHub MCP search at this checkpoint returned zero open repository PRs. There is no eligible random selection, applicable patch or stale Dependabot rebase. [Earlier maintenance](../operations/DEPENDABOT_MAINTENANCE.md) already closed superseded PR 5 without merge. No PR, alert, dependency version or branch was changed by this research.
