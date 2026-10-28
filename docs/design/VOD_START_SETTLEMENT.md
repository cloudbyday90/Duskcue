# VOD Start withdrawal and settlement

## Status and authority

Implementation contract confirmed after `67c6efc`, October 28, 2026, against `903e670`. The user confirmed Q7 of [shared client migration](VOD_CLIENT_MIGRATION.md): include bounded server-owned withdrawal for an uncertain Start; permanently fence the exact UUID against later admission, retain already-owned preparation through actual completion, and report conclusive settlement. The user confirmed the complete concrete implementation contract. Scoped implementation and qualification are complete at `577b95f6bb473a64a79b506b2020f56d653264ba` on main; exact evidence and limits follow below.

Preserve [admission](PLAYBACK_ADMISSION.md), [retention](PLAYBACK_RECEIPT_RETENTION.md), [command/delivery](VOD_COMMAND_DELIVERY.md), [reconfiguration](VOD_RECONFIGURATION.md), [output ownership](VOD_OUTPUT_LIFECYCLE.md) and [memory](MEMORY.md). This prerequisite supports controlled client qualification; it does not mount normal production routes, enable shared caps or add provider/recording playback.

## Gap found during design

[Start](../../server/src/domains/playback/commands/service.rs) performs retained preparation before receipt admission and before the [local viewer registry](../../server/src/domains/playback/commands/ownership.rs) can register its resolved plan. A concurrent status read can return `unknown`, then the original Start can later acquire the household lock and commit. Current Stop requires an existing receipt. Neither observation is an absent-request cancellation fence.

[Missing-receipt lookup](../../server/src/services/playback_admission/retention/database.rs) does not advance a durable floor or write a tombstone. Current 410 conflates an ID behind the persisted cutoff with an ID merely older than the current database clock window. The latter is not a permanent abandonment certificate under clock rollback. Existing pruned, known-released receipts are a different case.

Missing change receipts have the analogous pre-acceptance race. Existing parent Stop already fences further changes once it marks a known viewer retiring. There is no equivalent transition for an unresolved pre-admission Start.

## Preparation completion must be observable

The root [VOD command tracker](../../server/src/services/vod_lifecycle/owner.rs) retains outer execution after observer loss. It has 128 command permits, separate from 128 physical-generation permits. Cleanup commands may pass lifecycle closure but still need an available command permit. A new withdrawal signal must therefore act on an already owned preparation/short database owner rather than require another free Start slot or create an unbounded cleanup queue.

The [generation tracker](../../server/src/services/vod_lifecycle/generation.rs) provides retained inspect/drain primitives. However, current source planning includes two-second asynchronous file-open/metadata timeouts and a nested root inspection not bound to the Start UUID. Locked Tokio 1.52.3 filesystem helpers use blocking tasks; dropping their awaiter does not establish those tasks have finished. Wrapping an internally timing-out helper in another tracker is insufficient.

The new path must own every actual synchronous filesystem inspection through an observable retained completion. Deadlines bound observation/admission, not proof that an underlying operation stopped. Bind that ownership to the exact Start before any preparation I/O; never mark settlement from outer-future return, timeout, missing local state or a database flag alone.

Primary sources were discovered through MCP search and retrieved on October 28: [Tokio blocking work](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html) confirms that a started blocking task can outlive abort/shutdown timeout; [Tokio shutdown](https://tokio.rs/tokio/topics/shutdown) separates cancellation notification from waiting for completion; [PostgreSQL 18 isolation](https://www.postgresql.org/docs/18/transaction-iso.html) explains visibility/concurrent-insert behavior. Public latest documentation is supporting guidance; inspected locked Tokio 1.52.3 and current repository code govern implementation. No Tokio upgrade is proposed.

## Proposed durable intent boundary

Introduce a focused pre-admission intent/withdrawal record instead of fake source/policy/file placeholders in existing immutable command rows. Capture the already normalized complete intent hash and original actor/context before resolution. Retain raw selection only in bounded owned memory; do not store credentials, source paths or private client observations in the receipt.

| Record/state | Required invariant |
|---|---|
| Registered intent | UUIDv7, exact original account/session/profile/context/credential/transport and intent hash; owning instance/revision; one physical preparer. Registration is known committed before preparation begins. It grants no viewing, resolved-source or encoder permission. |
| Preparing | Retain each inspection task/permit and cancellation/completion observation. Duplicate exact requests reconcile the same owner; changed intent or actor conflicts. No second preparation after observer loss or on another instance. |
| Withdrawn intent | A durable monotonic no-later-admission fence for that UUID. An explicit Close/Stop requests withdrawal; a network error alone does not silently abandon the user's intent. |
| Promoted intent | Link to the existing reservation/command only after current source/policy/context/instance checks and known admission commit. Preserve immutable resolved attribution and one viewer. Never count the intent and reservation as two logical receipts. |
| Settled withdrawal | No promotion can occur; every already-owned preparation has positively completed. An absent intent can settle only after a known committed fence serializes against all registration/admission paths. |
| Uncertain ownership/cleanup | Remains retained and unavailable. A timeout, local registry absence, restart or elapsed lease cannot establish physical completion. |

Admission and withdrawal must share durable per-ID serialization and consistent existing instance/policy/household/receipt lock ordering. Registration must reject a prior withdrawal; promotion must recheck it transactionally. If admission wins, withdrawal invokes the existing parent retirement/cleanup path. If withdrawal wins, later admission is refused. A read remains observation, not an implicit destructive command.

An unresolved pre-admission intent has a nonextendible sixty-second admission deadline, observed by its current owner's monotonic clock and rechecked transactionally before promotion. It is not an admitted viewer lease and grants no media. Logical preparation failure/timeout or that deadline requests durable withdrawal; later physical inspection completion cannot restart admission. Underlying work remains owned until observed completion. After admission, the existing sixty-second viewer renewal contract applies. A new instance cannot reconstruct a lost monotonic clock and resume old preparation.

## Accounting, replay and compatibility

Use the existing 4,096-household/65,536-global parent-receipt ceiling for logical Start intent/reservation identities, including settled withdrawals. Transfer accounting atomically at promotion instead of double counting, and preserve current non-VOD receipts. There is no reserved storage pool hidden behind the reserved SQL permit: a previously unseen withdrawal can refuse capacity if it needs a new durable fence. Never acknowledge settlement after that refusal. Existing registered work must remain withdrawable/cleanable without another storage allocation.

Preserve first-use UUIDv7/server-time rules and a monotonic durable rejection floor. Existing intents reconcile before fresh-use checks. Pruning requires terminal outcome, observed completion, at least 24 hours and a permanent replay fence. An intent/reservation pair is one logical receipt; prune it as a guarded pair without unbounded cascade. Extend bounded indexed maintenance within the existing 128 logical receipt-work allowance, sixty-second cadence and one reserved operation. Keep seven regular plus one reserved SQL operation and existing timeouts.

Atomically backfill consistent existing admitted VOD attribution without inventing a pre-admission source identity or cleanup proof. Extend instance fencing and exact resources-gone reconciliation to unresolved preparation records. Old instances that could have unindexed preparation cannot be treated as settled by a new process; compatible ownership or explicit fenced reconciliation is required before advertising conclusive settlement. Legacy reservation insertion must not bypass a committed withdrawal fence.

## API and ownership

Keep explicit protocol two and advertise the complete required capability only in controlled qualification. The acknowledgement must cover conclusive Start settlement, conditional controls, checkpoints and lease observation; the old protocol number alone does not establish the new feature.

Add `POST /api/v1/playback-commands/{request}/withdraw` with an explicit protocol-two body and `GET /api/v1/playback-commands/{request}/settlement`. These bounded authenticated resources use the original Start UUID; accepted withdrawal returns 202 and its same-server settlement URL. Known admitted viewers delegate to the existing Stop/retirement owner; no additional viewer is created. Reuse exact captured-cleanup authority: same account, original active session, credential version and transport, without requiring the old profile/context to remain selected. Before a row exists, derive authority from authenticated server state and bind it durably; an identifier alone authorizes nothing. Known instance fencing and actual account/session revocation still take precedence.

Distinguish `admission_fenced` from `preparation_complete` and from an admitted viewer's released resources. Report fixed typed pending/uncertain/settled outcomes with explicit admission-fenced, preparation-complete and parent-release evidence where applicable, and same-server status paths, not source URLs, arbitrary diagnostics or another actor's existence. A lost acknowledgement never licenses a fresh UUID. Use a one-KiB withdrawal body, 32-KiB response ceiling, existing five-second body limit and existing short SQL owner; no automatic mutation replay.

Retain the durable actor and local task owner before acknowledging work. A failed/unknown registration commit launches no preparation. A failed/unknown withdrawal commit does not prove fencing. A failed/unknown promotion or completion commit keeps its retained owner and obligations; no timeout-only slot reuse. Root closure joins preparation, promotion, withdrawal and inspection completion before pool closure under the unchanged shared thirty-second drain and sixty-second pool-close budgets.

## Modular implementation and qualification

Separate intent schema/normalization, durable registration/fencing, quota/retention integration, pre-admission preparation ownership, promotion, withdrawal/settlement handlers and public projection. Reuse existing SQL leases, process/inspection owners and instance controls; do not add a second global scheduler, new dependency or singleton orchestrator.

Required tests include nonempty atomic migration/backfill; same-ID duplicate/mismatch and cross-instance races; withdrawal before registration, during real retained inspection, at admission and after admission; GET-unknown followed by delayed Start; clock rollback and pruning; full quota and no spare cleanup allocation; before/after-COMMIT acknowledgement faults; scope/credential/profile changes; old-instance fencing; root closure/deadline and actual retained filesystem completion. Verify both known absence fencing and positive physical completion independently. Do not use a private feed or production bypass route.

The shared client cannot clear its journal or permit replacement playback merely because a status read is unknown or age-expired. It uses this conclusive settlement path, or known existing-parent release, under the confirmed client rules.

## Qualified implementation — October 28, 2026

Implemented on main at `577b95f6bb473a64a79b506b2020f56d653264ba` after the confirmed contract at `67c6efc`. Dependencies and Rust/Cargo 1.99.1 remain unchanged. Normal production registration and shared-cap rollout remain off; no branch, tag or release is created. The source-policy authority remains [Local/Recording/provider lifecycle](IPTV_SOURCE_LIFECYCLE.md); provider/DVR implementation is separate.

The migration `20261028120000_vod_start_settlement.sql` introduces durable exact-actor Start registration before preparation, monotonic withdrawal fences, promotion attribution and independent physical completion. Focused settlement database/service/handler/type modules share existing admission ownership, quotas and SQL limits. Retained synchronous inspection owns the actual blocking task/permit; cancellation never substitutes for completion. Unknown commits remain sticky/uncertain. Operator fencing/reconciliation covers unresolved preparations and incompatible older instances.

Intent/reservation pairs count once under existing 4,096-household/65,536-global ceilings. Terminal settlements retain at least 24 hours; indexed bounded pruning first advances a permanent replay floor. Existing work can withdraw/clean up at full storage/command capacity. Normal startup mounts no command/settlement router and attaches no admission owner; capability advertisement remains controlled-qualification only.

| Verification | Outcome and receipt |
|---|---|
| Updated Windows Start/settlement | 42 internal cases pass in `.cache/vod-lifecycle/8c28e464-7c76-4050-9a20-410647e6b597/result.json`, including atomic migration/backfill and rollback, absence/admission races, exact scope, real blocked inspection/root drain, full quotas/128 command slots, indexed pruning/replay floor, before/after-COMMIT faults and exact operator recovery. Cleanup passes. |
| Windows full regression | Earlier broader pack: 12 groups/23 Rust test functions, including 27 reconfiguration cases and actual FFmpeg/HTTP/history/root cleanup, passes in `.cache/vod-lifecycle/cb284bdd-6500-4e5d-af76-933bb0c50edf/result.json` at inventory `eac68b40b2175937de269b6c361812f9684bb16e429a408fcc535ee8ec244f65`. Subsequent settlement changes have the updated 42-case qualification; later Rust edits concern client fixtures. |
| Current managed Linux | All 10 library groups/22 Rust functions pass, plus the actual HTTP/Stop subprocess contract and verified cleanup. Exact report `.cache/testing-memory/vod-linux-783371dd-0050-4f5b-84f6-d4afc4708812/result.json`; source inventory `41f60a1bd19ebc5b5a040c2091c21f1f83db1b2730bdd0ee9d11adacbae702d5`; runtime image `sha256:e5d55e24f3ac236810a2681d9ce33ac834de6566579b2c958a5bfa80ab4c0838`; base `sha256:f45eda1c8bbe3a9685196aae1200543601ec5efc2fb0f858ca863dd9d42ae8dc`. |
| Client adoption | Six real browser journeys plus isolated actual Tauri pass against the latest backend, including genuine decoder recovery and conclusive settlement; [client authority](VOD_CLIENT_MIGRATION.md) owns exact executable and artifact identities. |
| Regression and hygiene | Default Rust library: 1,017 pass, 13 fixture ignores. Fresh real PostgreSQL/resource admission regression passes in 143.12 seconds (`2026-10-28T14-33-05-266Z-8b374a4d-94db-456a-bc7e-7832422a7e14.jsonl`). Workflow: 30 pass, two existing skips. Formatting, copyright (990 files), whitespace and privacy checks pass. Strict Clippy remains nonzero with the same 31 diagnostics: normalized message/path multiplicities are unchanged; no warning suppression or introduced diagnostic. |

Linux used the existing guarded compiler/cache/checkpoint and live telemetry rules; no resource/deadline reserve was increased. Fresh headroom admitted this run without stopping Classifarr; Harmoniarr and Classifarr remained running. All 42 Start and 28 reconfiguration cases execute on Linux. Seventeen additional actual-Cargo durability checks pass, including changed/reverted backdated source; integration and library checkpoints complete within the existing compiler deadline. Every runtime attempt is fresh even when compiler checkpoints are reusable. Linux memory receipt: `2026-10-28T14-38-56-304Z-3872156c-2877-4ca4-9ec9-6a481e795ab3.jsonl`. An interrupted Windows admission compilation produced no completion record and was replaced by the complete fresh result above.

These are scoped server and shared-client qualifications, not production cap enforcement or provider/DVR/Live TV playback qualification. Remaining online client migration, legacy-path closure, instance/audit/history retention and actual provider transport remain separate. Source URLs, credentials and supplied private feed data are absent from published changes.
