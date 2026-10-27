# VOD command and delivery protocol

## Status and authority

Research and design review, October 27, 2026, against `84fa21e` on main. The seven product decisions below are confirmed. This document makes the implementation contract concrete for shared-understanding confirmation; no new runtime implementation or qualification is claimed. Main only; no release, branch or PR merge.

[Receipt retention](PLAYBACK_RECEIPT_RETENTION.md), [admission](PLAYBACK_ADMISSION.md), [VOD ownership](VOD_OUTPUT_LIFECYCLE.md), [streaming](STREAMING.md), [source qualification](PLAYBACK_SOURCE_CAPABILITIES.md), [memory](MEMORY.md) and [root lifecycle](SSE_LIFECYCLE.md) retain their existing authority. Preserve one online viewer through preparation/pause, sixty-second renewal, current viewing authority and confirmed physical cleanup before release. Downloads/offline viewing, history retention and instance/audit retention remain separate.

This increment qualifies a separate backend protocol. Normal-build services/handlers receive controlled HTTP qualification through the existing qualification lane, without production registration/admission attachment, client migration, production-wide cap enforcement or Live TV playback. Provider VOD and recordings are separate milestones. Fixture trust stays qualification-only.

## Confirmed product decisions

1. Provider Movies and Series controls apply to IPTV-provider VOD only. Local libraries and recordings remain independent.
2. Future provider VOD has independent administrator-controlled Movies and Series switches, both default off. Household/profile permissions may narrow the server allowance. This is a [future connector requirement](IPTV_XTREAM.md), not implemented switches or DVR.
3. A definitely accepted VOD start survives browser disconnection through retained preparation. Return 202 Accepted with explicit status/reconciliation. Reads do not renew the sixty-second lease; observer loss proves neither rollback nor permission for another viewer.
4. Qualify Start, status, renewal, Stop and session-bound direct/HLS delivery first. Seek, recovery and track/quality reconfiguration follow. Existing clients keep their contract; production enforcement remains off. Bound session/generation identities refuse weaker legacy control/HLS paths.
5. A start UUIDv7 binds complete normalized playback intent: item/file, tracks, quality/bitrate, device/source capabilities, force flags and interactive/ambient context. Matching repeats reconcile without another viewer or lease extension; changed intent or actor/context reuse refuses.
6. Existing viewers retain negotiated quality through bitrate/resolution/quality-only edits until a later start/reconfiguration. Viewing or current delivery-mode permission loss retires affected new-protocol viewers. Stream-count-only reductions preserve the agreed no-eviction rule.
7. The inherited `max_transcode_streams` additionally limits household transcoded VOD viewers through preparation, pause and completed-but-playable output until confirmed release. Physical encoder capacity remains separate; cap-only reductions refuse additional starts without evicting authorized viewers.

## Repository evidence

Three read-only explorations inspected commands, every online client family and physical ownership. They edited no files, ran no builds/tests, accessed no private feeds and stopped no service.

| Boundary | Finding and implication |
|---|---|
| Authentication | `extractors.rs` retains authentication-session identity, credential revision and transport; `PlaybackActor` drops them. The existing session-context service supplies authoritative profile-context revision and database time. Its context identifier is a concurrency precondition, not a credential. |
| Start | `StartPlaybackRequest` has no stable ID. `start_playback_with_transport` launches work before history identity/publication. The reservation fingerprint omits the full playback options. |
| Readiness | Ledger acknowledgement precedes the resource factory. Managed HLS readiness already has a fifteen-second deadline and usable-output evidence. Admission is not playable readiness. |
| Observer loss | The VOD command observer cancels unfinished work when dropped. A higher retained coordinator must own that observer after accepted HTTP work. |
| Direct/HLS | Direct `response(None,...)` lacks logical viewer grouping. HLS has generation/path ownership but legacy authorization checks account/profile/history, not exact sign-in context or lease. |
| Cleanup | `stop_session` observes process/read/cache cleanup. Natural encoder completion releases only physical capacity; playable output remains owned. The resource factory must stay alive. |
| History | Partitioned `play_sessions` has no global unique ID and has deletion cascades. It cannot own canonical commands or uncertain viewers. Existing frozen Stop results and captured cleanup after profile change/deletion must remain valid. |
| Policies | Saved mode/quality flags are not enforced by the online planner. The ledger validates numeric ceilings but does not count classified transcode viewers; policy writes lack mode-retirement hooks. Reuse strict inheritance, not the permissive limits-response fallback. |
| Locking | Policy reads already take a shared streaming-policy table lock. Fence policy before household/reservation locks consistently, and make related policy/default/assignment writes transactional when adding revocation. |
| Clients | Web/Tauri reconstructs legacy URLs; several native paths discard lease/Stop outcomes. Source-recovery negotiation is not admission negotiation. Every online family needs later explicit migration. |
| Provider/recordings | The current feed connector is Live TV/EPG only; provider VOD and DVR are absent. Local movie/episode rows have no trusted recording provenance. Do not implement provider switches by disabling local libraries. |

## Official research

Primary URLs were discovered through MCP search/retrieved links and opened October 27. Product durations/budgets are Duskcue choices. Installed library source remains authoritative where online documentation describes newer releases.

| Source | Application |
|---|---|
| [RFC 9110](https://httpwg.org/specs/rfc9110) | Distinguish acceptance from completion, preserve range/conditional semantics and never infer rollback from a lost response. Reconciliation reads do not allocate, renew or restart. |
| [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457.html) | Fixed machine-readable failures for changed context/intent, expiry, capacity, unavailability and uncertainty; no raw SQL, paths or submitted command data. |
| [RFC 8216](https://www.rfc-editor.org/rfc/rfc8216.html) | Relative references resolve against their containing playlist. Generate server-owned viewer/generation-bound references and authorize each playlist, segment and initialization request. |
| [RFC 9700](https://www.rfc-editor.org/rfc/rfc9700.html), [RFC 6750](https://www.rfc-editor.org/rfc/rfc6750.html) | Existing cookies/Authorization headers authenticate. IDs are not credentials; add no bearer token to delivery query strings. |
| [OWASP authorization](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html) | Validate current actor/object authority per request and prevent weaker legacy routes accepting newly bound session/generation identities. |
| [PostgreSQL SELECT/locking](https://www.postgresql.org/docs/18/sql-select.html) | Consistent locks and bounded transactions serialize admission/revocation. Persist command attribution with its reservation; SQL commit cannot prove external process completion. |
| [Tokio shutdown](https://tokio.rs/tokio/topics/shutdown), [TaskTracker](https://docs.rs/tokio-util/latest/tokio_util/task/struct.TaskTracker.html) | Synchronize admission/cancellation and retain completion. Tracker closure alone does not prevent registration. Reuse installed Tokio 1.52.3/Tokio-util 0.7.18 and the managed launcher. |
| [WCAG status messages](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html?trk=article-ssr-frontend-pulse_little-text-block), [error identification](https://www.w3.org/WAI/WCAG22/Understanding/error-identification) | Explicit lifecycle/error states support later accessible client presentation. Backend tests do not establish UI or assistive-technology conformance. |

## Alternatives and recommendation

| Approach | Pros | Cons | Recommendation |
|---|---|---|---|
| Separate bounded backend protocol | Explicit identity, retained readiness and exact delivery scope without breaking clients. | Durable attribution, viewer-scoped readers and coordinated policy/cleanup work; production waits for migration. | Selected slice. |
| Add fields to legacy start only | Smaller apparent change. | Clients ignore fields, rebuild old URLs or misread pending as success. | Insufficient. |
| Store commands in history | Reuses a table. | Partition uniqueness/cascades and analytics semantics lose uncertain ownership/replay evidence. | Reject. |
| Launch inside a SQL transaction | Looks atomic. | Processes are not transactional; long locks and observer loss remain. | Commit accepted identity, retain work, then publish readiness. |
| DB check per media chunk | Frequent validation. | Excessive load against seven regular operations; emitted bytes still cannot be recalled. | Per-request checks plus owned cancellation. |
| Enable caps/migrate all clients now | Could deliver the final feature when everything passes. | Couples backend safety, native runtimes, legacy closure and UI recovery. | Deferred. |

Recommended stack: locked Rust/Cargo 1.99.1, Axum/Hyper, SQLx 0.9/PostgreSQL 18, UUIDv7, existing hashing/encryption, Tokio/Tokio-util and managed FFmpeg 8.1.2. Reuse source evidence, readiness, generation cleanup, session context, admission transactions and no-store errors through focused modules. Add no dependency upgrade, external coordinator, broad singleton or competing process framework.

## Complete implementation contract for confirmation

### Protocol and authority

Use an explicit new namespace and positively identified response schema for Start/status/renew/Stop and direct/HLS output. Keep legacy client payloads. Qualify normal-build services/handlers without production registration/admission attachment or a generic settings bypass. Do not infer support from ignored fields or the old recovery flag.

Require exact account, sign-in session, credential revision, transport and selected profile/context revision. Start/renew use authoritative context preconditions checked in the mutation transaction. Status/readiness cannot allocate, renew or launch. Publish media URLs only after durable ready publication and locally retained output ownership are both known. Control/status/errors are private/no-store, with fixed safe failures and redacted concrete command paths/body/headers.

Fresh accepted starts return 202/status URI; repeats reconcile current state without reallocation. Distinguish pending/unknown, preparing, ready, stopping, released, failed, expired and uncertain outcomes. Absence behind the durable UUIDv7 cutoff is expired. Instance loss leaves unowned work uncertain/counted with no automatic replay/adoption; retain explicit fenced-instance reconciliation.

New bound history/session/generation IDs refuse legacy info/heartbeat/seek/recovery/Stop and HLS paths. Generic legacy file access remains for legacy clients during isolated qualification, so this is not a production-wide cap boundary. Client migration and legacy closure remain mandatory.

### Durable command and history

Add a globally unique UUIDv7 command record one-to-one with its reservation. Store immutable actor/resource attribution, complete canonical-intent fingerprint, finite technical/result metadata and lifecycle state. Keep raw request bodies, credentials and local paths out of storage/diagnostics. Normalize object keys/defaults deterministically while preserving meaningful array order.

Fresh reservation and command attribution share one bounded/default-discard transaction. Factor a narrow connection-level admission seam; do not nest pool transactions or duplicate authority/policy. Launch nothing after uncertain admission commit. Preserve current time windows, floor, quotas and no revival. Command metadata prunes through its safely released parent after the existing retention window, with indexed foreign-key cleanup and no independent TTL/tombstone archive.

Preparation counts as a viewer; publish history only after real readiness, preserving successful-start analytics semantics. Ready/history publication and current actor/source/lease checks share one transaction. Unknown commit leaves output unpublished and cleanup owned. Bind exact history ID/start attribution rather than assuming partitioned ID uniqueness. Preserve frozen existing Stop results and do not invent history for starts stopped before readiness.

### Policy and source plan

Resolve one valid assigned/default policy with existing user override precedence. Missing, ambiguous, negative or unsupported data refuses. Zero denies additional matching viewers; resolved null remains unlimited. Bind the selected source/decision and whether it encodes media. Actual audio/video/subtitle encoding consumes the transcode-viewer ceiling, including audio encoding inside a direct-stream result; pure copy/remux does not. Encoder permits remain separate.

Honor supported mode/4K permissions and start-time bandwidth/resolution/quality settings without a permissive fallback or silent delivery-mode widening after admission. Quality-only edits preserve negotiated output. Viewing/mode revocation irreversibly retires affected reservations even across revoke/restore between monitor ticks. Keep policy/default/assignment writes coherent and lock policy before account/reservation. Cap-only edits do not evict; Kids local VOD retains current library/rating checks.

IP allow/block and automatic paused-session settings need their own peer/pause qualification. Non-default policy outside this slice refuses the new lane instead of being ignored. Leave legacy/download behavior separate. Reuse qualified source/device negotiation and refuse changed source evidence before publication. Seek/recovery/reconfiguration is explicitly unavailable for new sessions in this slice.

### Ownership, delivery and Stop

Retain accepted preparation independently of HTTP observers. Reuse fifteen-second managed readiness, source checks and launcher/sandbox. The viewer owns direct readers and any HLS worker/generation/files/cache; natural encoder success does not finish its resource factory. Add viewer-scoped cancellation/join without expanding physical generation budgets or draining unrelated viewers.

Each media request rechecks exact current scope, source, ready generation, lease and viewing/mode authority. Validate opened file evidence; preserve ranges, lengths, MIME, bounded HLS parsing and confinement. Generate only server-owned relative output references. Established responses stop through owned cancellation and the existing monitor/local timer, without DB queries per chunk or recall guarantees for sent bytes. Retirement/root/Stop rejects new reads and discards queued output while observing actual I/O/handle/process/cache completion.

Stop is retained/reconcilable and can report stopping/cleanup uncertainty. Released requires trusted resource completion plus durable ledger release. Preserve explicit cleanup/release retry, including acknowledgment loss, without a viewing-device completion override. Keep known completed resources distinct from lost/panicked completion. Captured owning account/session can clean up after profile switching/deletion without a viewing grant or new unauthorized progress; lost session/authority still triggers server retirement. Foreign uncertain owners require operator recovery.

### Budgets and qualification

Preserve seven regular admission DB operations plus one maintenance operation, 128 local viewers and 128 VOD commands/generations/responses, eight delivery I/O operations, 64-KiB/two-chunk queues and 1-MiB playlists. New control/status DB work shares those bounds. Hold no SQL permit/transaction through physical preparation or cleanup. Bound start JSON/normalization and control/status representations to 32 KiB; body acquisition gets five seconds. Keep SQL three-second work, 500-ms locks, 1,500-ms statements and three-second transactions.

Synchronize registration/root closure and retain accepted SQL, preparation, readers and cleanup. Keep thirty-second shared drain, sixty-second pool close, nonzero failure and second-signal behavior. Quarantine/count unconfirmed completion. Reuse locked dependencies and approved legal headers, without explanatory comments, release or changelog changes.

Qualify real PostgreSQL/HTTP cookie/bearer scope, changed-intent/duplicate/expiry races, atomic admission/readiness, BEGIN/COMMIT faults, observer loss, restart/foreign uncertainty, policy inheritance/classification/capacity and transactional mode revocation. Exercise actual direct ranges and HLS manifest/rendition/init/segment requests, wrong session/profile/revision/transport, legacy downgrade refusal, source change, slow/unpolled bodies, lease/Stop cancellation and cleanup/retry. Use real FFmpeg/files/root/process qualification on Windows and managed Linux; add no production clock, abort, trust or cleanup bypass.

Keep all existing compiler/container/telemetry/pool/root limits and verified executable checkpoints. Use the approved exact-identity Classifarr handoff/restoration only if Linux admission requires it; Harmoniarr is untouched. Supplied private IPTV data remains ignored and is unnecessary here. Run meaningful compilation, library/admission/VOD/root/client-contract regressions, strict-lint baseline comparison, format/copyright/link/privacy/source checks. After shared-understanding confirmation, implement modularly, verify, commit and push all task changes on main without a release.

## PR applicability and next increment

GitHub MCP open-PR search returned zero candidates. No random patch or stale Dependabot rebase applies; superseded PR 5 is already closed without merge. Research changes no dependency or alert.

Documentation checkpoint checks pass: seven Markdown files, 679 existing/new local link references with no missing target, no supplied private-feed values, unchanged previously qualified runtime source, clean whitespace and copyright compliance across 911 tracked source files. No new runtime test or implementation result is claimed by these documentation checks.

Next: add seek/source recovery/track-quality replacement to this protocol while preserving one viewer, then migrate all online client families and close legacy bypasses before production caps. Live TV media output, provider-VOD connector/switches and recordings remain separate milestones.
