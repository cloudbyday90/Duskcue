# Same-viewer VOD reconfiguration

## Status and authority

Research and design review, October 27, 2026, against `7a1aaa17987e0d9fc308d3168131e45b94a1cc60` on main; initial research checkpoint `9adc976`. All ten product decisions below are confirmed. This document now defines the complete proposed implementation contract for the required shared-understanding checkpoint. Complete-contract confirmation is pending. No runtime change, dependency update, production registration, client migration or release is claimed.

This follows the qualified [VOD command protocol](VOD_COMMAND_DELIVERY.md). [Admission](PLAYBACK_ADMISSION.md), [receipt retention](PLAYBACK_RECEIPT_RETENTION.md), [output ownership](VOD_OUTPUT_LIFECYCLE.md), [recovery](PLAYBACK_RECOVERY.md), [source capabilities](PLAYBACK_SOURCE_CAPABILITIES.md), [seek timing](PLAYBACK_SEEK_TIMING.md) and [memory](MEMORY.md) retain their existing authority. One logical online viewer, one exact history identity, sixty-second renewal and confirmed physical cleanup remain required.

## Confirmed decisions

1. Seek, recovery and track/quality changes remain within the originally admitted media item and file. Another title, episode or file requires a new Start. Preserve the viewer, history and interactive/ambient context.
2. Retire the old output, cancel its readers and confirm cleanup before preparing replacement output. Playback pauses during preparation; the old output cannot be restored after cleanup.
3. Accept one reconfiguration per viewer at a time. Refuse competing commands without queuing or supersession. An exact repeated command reconciles its existing operation; Stop and revocation can cancel it.
4. An observed worker failure leaves a counted, nondeliverable viewer eligible for explicit recovery or Stop within its renewed lease. Clean up failed output without automatically launching a replacement. Natural successful encoding remains playable and is not a failure.
5. Accept separate absolute source-position and pause/buffering checkpoints, bound to the exact current output revision and an increasing sequence. Update viewing history without renewing the lease. Stale reports cannot move progress or trigger recovery; encoder progress never becomes the resume position.
6. Seek and worker recovery preserve current negotiated tracks/quality. Explicit track/quality changes obtain a fresh plan under current policy and capability evidence. Every operation still requires current viewing/source/mode permission.
7. Qualify server-observed worker recovery and the existing qualified direct/copied-video decoder downgrade cases in this backend lane. Require fresh output-bound evidence and preserve the shared two-replacement budget. Client adoption and additional hardware/format qualification remain separate.
8. Direct-to-encoded requires an available transcode-viewer slot. Encoded-to-encoded retains its existing slot despite a cap-only reduction. Encoded-to-direct relinquishes its extra slot only after old encoded resources are confirmed gone. Keep one total viewer throughout.
9. A known replacement-preparation failure after old-output cleanup leaves the viewer unavailable but renewable for explicit correction or Stop. Do not restore old output or replay automatically. An exhausted recovery budget stays exhausted.
10. Completed operation receipts remain reconcilable for at least 24 hours after output cleanup, with initial retained-operation limits of 256 per viewer, 8,192 per household and 131,072 total. Refuse new changes when full; do not evict receipts early or block renewal/Stop/cleanup. UUIDv7 time fencing prevents pruned commands becoming new work.

Known refusal, known failed preparation and uncertain acceptance/publication/cleanup are distinct outcomes. Existing fail-closed ownership rules continue to apply to uncertainty; a correction command cannot bypass an unresolved resource obligation.

## Repository evidence

Three read-only fact explorations inspected control behavior, resource ownership and persistence. They changed no files, ran no builds/tests, accessed no private feeds and stopped no service.

| Boundary | Verified finding | Design consequence |
|---|---|---|
| Viewer | `commands/ownership.rs` has one immutable prepared plan and optional physical generation; physical retirement currently ends its viewer. | Introduce an explicit operation/output-revision owner under the retained viewer. Do not call a second Start or mutate a generation pointer in place. |
| Start receipt | `20261027130000_vod_commands.sql` freezes intent/source/policy/mode attribution and published history/generation/result. | Preserve original Start evidence. Add separate immutable operation and output attribution. |
| Ledger | `playback_admission/vod.rs` matches the original binding on each active operation; `ledger.rs` counts one immutable `encodes` flag per viewer. | Separate immutable Start attribution, current permission checks and retained transcode-resource claims. |
| Replacement | `transcoding/commands.rs` already retires and confirms old cleanup before starting replacement. | Reuse physical ordering without copying legacy transactions held across process preparation. |
| Readers | Logical viewer groups and physical generation groups are distinct. Retiring readers discard queues and observe owned I/O before cache deletion. | Give direct and HLS output revisions distinct admission/cancellation groups. |
| URLs | Current command delivery URLs contain the viewer ID but no output revision. | Revision-address every file, playlist, initialization and segment request; never reinterpret an old URL as new output. |
| Readiness | Managed startup verifies usable assets within fifteen seconds. Publication rechecks authority/source; pending worker ownership is acknowledged after known commit. | Preserve retained ownership and positive readiness for each replacement. |
| Failure | Successful encoding retains playable output; uncertain termination/cache cleanup retains its owner and capacity. | Distinguish successful completion, recoverable failure and uncertain cleanup. |
| History | One partitioned history tuple is created at initial readiness and frozen after cleanup. New renewal/Stop bodies carry no position. | Keep one history row. Add explicit client progress rather than inventing a playhead from process advancement or seek intent. |
| Existing client | Legacy track/quality changes call a whole new `player.play`; recovery uses revision-bound client position and a shared budget. | Reuse qualified planning/recovery rules, not the legacy replacement-Start behavior. |
| Retention | Current quotas/pruning bound parent reservations only. | Bound operation/output rows independently before permitting repeated changes. |

## Official research

Primary sources were discovered through MCP search and retrieved links on October 27, 2026. The installed dependency source and existing qualified format contracts govern implementation where public documentation describes another version. Durations and quotas are Duskcue policy choices, not standards requirements.

| Source | Application |
|---|---|
| [RFC 9110](https://httpwg.org/specs/rfc9110) | Acceptance does not establish completion. Retain an accepted operation with a status resource. Use explicit version preconditions and reconcile lost responses without allocating again. An entity tag must describe its selected representation; do not borrow a changing status tag for unrelated state. |
| [RFC 8216](https://www.rfc-editor.org/rfc/rfc8216.html) | Playlist segment identity and discontinuity/timestamp rules matter across replacement. Use fresh, revision-bound resources rather than splicing unrelated encodes into old playlist/segment addresses. |
| [WHATWG media elements](https://html.spec.whatwg.org/multipage/media.html) | Media position, seeking and readiness are distinct. Report requested absolute source position separately from measured output origin. A position assignment does not establish decoded playback or confirmed viewing history. |
| [FFmpeg CLI](https://www.ffmpeg.org/ffmpeg.html) | Input seeking may retain earlier copied packets, while accurate transcoding discards preroll. Timestamp preservation and muxer rewriting require actual output evidence, not an assumed requested-time origin. Preserve the qualified copy/mixed-stream timing rules. |
| [PostgreSQL 18 concurrency](https://www.postgresql.org/docs/18/mvcc.html), [explicit locking](https://www.postgresql.org/docs/18/explicit-locking.html) | Keep transactions short and acquire shared policy, household, reservation and revision locks in a consistent order. Database atomicity does not prove process/read/cache completion. |
| [Tokio shutdown](https://tokio.rs/tokio/topics/shutdown) | Cancellation requests and observed task completion are separate. Keep accepted work and cleanup joined under existing root owners even if an HTTP observer disconnects. |
| [OWASP object authorization](https://api-security.owasp.org/editions/2023/en/0xa1-broken-object-level-authorization/) | Check exact actor/object authority on control, status and every output request. UUIDs and revisions identify resources; they do not authenticate or authorize them. |
| [WCAG 2.2 status messages](https://www.w3.org/WAI/WCAG22/Understanding/status-messages), [error identification](https://www.w3.org/WAI/WCAG22/Understanding/error-identification), [focus order](https://www.w3.org/WAI/WCAG22/Understanding/focus-order.html) | Expose stable operation, recovery and error distinctions for later localized status/error presentation. Client migration must preserve focus and announce meaningful changes without noisy progress announcements. Backend qualification cannot establish UI accessibility conformance. |

Fresh output revision addresses, retained command ownership and the proposed module arrangement are engineering conclusions from these sources and repository evidence; no cited standard mandates this schema.

## Alternatives and recommendation

| Approach | Pros | Cons | Recommendation |
|---|---|---|---|
| Second Start for each change | Reuses an existing endpoint. | Changes viewer/history identity, can double count and resets attribution. | Reject. |
| Mutable current generation behind old URLs | Small apparent implementation. | Delayed requests can reach different media; old reports may affect new output. | Reject. |
| Immutable operation/output revisions under one viewer | Exact reconciliation, cancellation and resource attribution. | Requires bounded receipts, current/pending policy checks and careful publication. | Recommended. |
| Keep old output while preparing new | Can reduce interruption and preserve rollback. | Overlapping encoders/readers/cache ownership and capacity behavior. | Not selected; drain first is confirmed. |
| Hold SQL locks while running FFmpeg | Makes orchestration appear sequential. | Long locks, cancellation hazards and false process atomicity. | Reject; separate short acceptance/publication transactions from retained physical work. |
| Extend the isolated backend before client rollout | Qualifies one-viewer behavior without changing existing online clients. | Production caps remain unavailable until client and legacy-path migration. | Recommended. |

## Recommended stack and modular implementation

Keep the existing Rust/Cargo 1.99.1 qualification environment, edition-2024 workspace, Axum/Hyper, SQLx 0.9/PostgreSQL 18, UUIDv7, existing hashing, Tokio/Tokio-util and managed FFmpeg 8.1.2. Add no dependency, external coordinator or competing process framework. Existing online clients and normal production registration remain unchanged during isolated backend qualification.

Separate the following responsibilities rather than expanding `commands/ownership.rs` into a singleton:

1. Thin request/status/checkpoint handlers and bounded request/response types.
2. Operation intent normalization, exact actor/resource/output-revision preconditions and replay reconciliation.
3. SQL acceptance, immutable revision attribution, current/pending authority and transcode-viewer claims.
4. Retained per-viewer operation execution, output retirement, replacement readiness and publication.
5. Recovery eligibility, durable shared budget and bounded public reason projection.
6. Revision-bound output authorization and direct/HLS URL generation.
7. Checkpoint/history updates and independently bounded receipt maintenance under the existing operation owner.

The original Start receipt remains immutable. A current control state refers to immutable output revisions and at most one accepted operation. Accepted operations survive observer loss but never implicitly renew a viewer. Every replacement remains bound to the exact account, sign-in session, credential revision, selected profile/context, transport, source and interactive/ambient context. Stop, revocation, expiry, instance fencing and shutdown cover all retained revisions and unpublished work.

## Concrete HTTP contract

Advance the positive acknowledgement for this isolated lane to `duskcue-vod-command/2`. New Start/control/checkpoint bodies must explicitly select that protocol before admission. Preserve original version-one receipts and their attribution; do not reinterpret an old request as a new version-two start. Old owned-instance uncertainty still requires the existing fenced reconciliation. Production legacy clients/routes keep their current contract; this lane remains unmounted in normal production startup.

Version-two planning may admit the existing qualified direct-recovery/capability selection; accepting a protocol number alone cannot enable fallback. Expose a bounded output-bound recovery offer only where the existing route/descriptor checks qualify it. Revalidate fresh acknowledgement against that exact offer during recovery; observations from another output cannot be transferred. Extend existing legacy rejection to every new bound history/output generation and both protocol versions. Old unrevisioned command output paths cannot alias version-two replacement output. The generic legacy file route remains a later production-migration boundary, not evidence of enforced caps here.

Use the existing `/api/v1/playback-commands` base. Every response, including outer authorization/validation refusals, remains private/no-store. Continue existing cookie/bearer, explicit-origin credentialed CORS, session binding and fixed problem responses. Revisions and URLs contain no credential, file path, private capability observations or diagnostic text.

| Method and relative path | Contract |
|---|---|
| Existing Start/status/renew/Stop/retirement | Preserve retained acceptance, explicit lease renewal and captured exact-session cleanup. Status distinguishes preparation, ready output, unavailable output, uncertainty, retirement and release. Status/replay never renews. Initial Start preparation failure retains its existing retirement/cleanup behavior; the new correction rule concerns replacement of an established viewer. |
| `GET /{viewer}/configuration` | Bounded control representation with a resource-specific strong ETag. Include control revision, current/last-published output revision, pending operation, negotiated metadata, playback intent and bounded recovery eligibility. Exclude observation time, lease deadline and checkpoint sequence/position from this representation so unrelated renewal/progress cannot invalidate its precondition. |
| `POST /{viewer}/configuration` | Require protocol, UUIDv7 operation ID, current strong If-Match, expected output revision, action and complete action intent. Require current X-Session-Context and transaction-checked exact actor/resource authority. Commit acceptance and retained ownership before 202/Location; do not hold SQL locks during output retirement or FFmpeg preparation. |
| `GET /{viewer}/operations/{operation}` | Reconcile that exact operation with fixed metadata and its immutable result. Never allocate, renew or restore a historical output. Publish a media URL only through current-viewer status after current output/authority/readiness checks. No operation-list endpoint or historical media URL replay is added. |
| `GET /{viewer}/operations/{operation}/retirement` | Metadata-only outcome/cleanup observation through the same captured exact account/sign-in/credential/transport boundary as viewer retirement. Supports cleanup observation after profile switch/deletion; grants no media or management access. |
| `POST /{viewer}/checkpoint` | Separate protocol/output-revision/sequence/absolute-position/playback-state body. No replacement, lease extension or implicit recovery. Return the accepted sequence/outcome; identical repeats are harmless, conflicting reuse and older/stale reports cannot update history. |
| `GET /{viewer}/checkpoint` | Read the latest accepted checkpoint through current viewer authority after an uncertain report response. No mutation, renewal, ETag reuse or process action. |
| `GET /{viewer}/outputs/{output}/file` and corresponding HLS paths | Authorize that immutable output revision and physical generation. Rewrite all playlist, rendition, initialization and segment references within its revision scope. Retired/stale/unpublished output never resolves to replacement bytes. |

The configuration ETag describes the exact bounded selected configuration representation. New conditional writes evaluate it under the serialized control transaction; missing conditions use 428, stale conditions 412. Evaluate authorization first. An exact previously accepted operation is reconciled before attempting new work, even if its original precondition is now old; prove the same actor, original condition and normalized intent. A changed payload/condition under the same ID is a conflict. This is reconciliation of an already applied request, not permission to execute a failed conditional request.

Conflicting operations receive fixed 409 busy/conflict without a queue, supersession or resource allocation. Invalid protocol/action/evidence/position is refused before retiring healthy output. Malformed input uses fixed 400, oversized bodies 413 and unsupported content type 415. Transient closed/capacity conditions retain authorized no-store 503/Retry-After 5; errors never encourage automatic mutation replay. An old absent operation uses 410 after replay-window checks. A lost or uncertain acknowledgement directs explicit status/cleanup observation, not a second operation with a new ID. Public failures use existing problem conventions with fixed reasons; do not return raw SQL/process errors or submitted sensitive data.

Action intent is a strict tagged shape:

- **Seek:** requested absolute source `position_ms`; preserve the current finalized route, selected tracks, codecs, bitrate/resolution and spent recovery budget. Rotate the output revision even for direct-file playback, retire its old readers and return a new scoped file resource. No new history or viewer is created.
- **Reconfigure:** requested absolute source position and complete replacement track/quality/device/capability selection, restricted to the admitted item/file and playback/ambient context. Normalize and hash all options, resolve actual track indices, obtain a fresh plan and apply current policy/capability limits. Reject unsupported HDR/transformation or stale evidence rather than widening an existing qualified route.
- **Recover:** expected output revision and exact accepted checkpoint sequence, plus the fixed worker or qualified decoder-fallback evidence. Resolve position from that client checkpoint, never encoder advancement. Server-observed failure or an admitted decoder-fallback route establishes eligibility; a client reason alone cannot manufacture it. No extra source selection or arbitrary FFmpeg options are accepted.

Hash the protocol, viewer, action, expected output revision, original configuration condition and complete normalized action intent with the existing hash implementation. Bind the exact actor/profile-context fingerprint separately and immutably. Use fresh UUIDv7 operation/output identities; capability source-context hashes remain hashes, not UUID replacements. Stable request IDs reconcile known outcomes but never extend a deadline, reset a recovery budget, duplicate a viewer or revive a retired operation.

## Durable records and resource claims

Retain `playback_vod_commands` as immutable original Start attribution. Introduce focused control, operation and output-revision records rather than weakening its source/mode/result guards:

| Record | Required properties |
|---|---|
| Control, one per viewer | Exact reservation/source ownership, current/last-published output, at most one pending operation, monotonic control revision, encoded-viewer claim and durable shared recovery/checkpoint state. Compare immutable Start binding separately from current permission/classification. |
| Operation | UUIDv7, immutable parent/actor/intent/precondition/action attribution; one candidate output; monotonic accepted/retiring/preparing/completed/failed/cancelled/uncertain lifecycle. Terminal outcomes cannot become pending work. |
| Output revision | Server UUIDv7, immutable viewer/source/configuration/mode/encoding attribution and physical generation; publish metadata once after readiness; retain monotonic retirement and observed cleanup evidence. An output ID cannot be reassigned or republished after retirement. |
| Original history | Preserve the exact partitioned `(history_id, history_started_at)` tuple. Record current playback metadata and confirmed checkpoints without another successful-start history row. Freeze final Stop once after complete resource cleanup. |

Add database constraints/triggers for ownership, pending-operation uniqueness, state transitions, source/context immutability, monotonic cleanup and no resurrection/deletion shortcuts. Atomically backfill existing nonempty command data and counters from its actual completion/state evidence; absence of a generation is not proof of cleanup. Migration must preserve original receipts/history and refuse inconsistent attribution rather than inventing completion. Backfilled owned-instance work is not automatically resumed.

Retain current policy-before-household-before-reservation ordering, then lock control and operation/output records consistently. Counters/replay fences have one documented lock order across admission, maintenance and reconciliation. Use the existing three-second/default-discard SQLx owner, 500-ms lock, 1,500-ms statement and three-second transaction limits. Do no physical preparation/retirement while holding those locks.

Count total viewers from the original nonreleased reservation exactly once. Count encoded viewers from one durable claim per viewer, including audio-only encoding in direct-stream output and subtitle burn-in. Direct-to-encoded reserves a claim atomically against current inherited transcode capacity before disturbing old output. Encoded-to-encoded keeps its claim through cleanup, replacement preparation, pause and known failure, even after a cap-only reduction. An encoded-to-direct claim is cleared only after known direct publication and confirmed cleanup of every old encoded obligation. A failed new encoded candidate from an otherwise direct viewer can relinquish its provisional claim only after its cleanup is confirmed; uncertainty retains the claim. Physical FFmpeg encoder capacity stays separate.

Current and pending deliverable configurations, not the immutable original Start mode, govern current viewing/mode/4K/ambient permissions. Retained obsolete output contributes cleanup/capacity obligations without granting delivery. Update renewal, per-request authority and deferred revocation triggers together. Numeric ceiling reductions alone do not evict existing authorized viewers; actual permission/source/account/session/profile/credential revocation cancels the viewer and all pending output. An unavailable encoded viewer retains its negotiated classification until a qualified change or final cleanup; physical failure alone does not turn its next recovery into an additional viewer.

## Retained execution, progress and recovery

Under one retained viewer owner:

1. Bound and validate input, current context, target eligibility and available operation/resource capacity. Do not retire old playback before known acceptance. If preacceptance work is abandoned, retain already launched source I/O until observed completion.
2. Serialize acceptance, replay checks, current control/output condition, target policy and provisional claim in one short transaction. Register retained execution before acknowledging acceptance. Only a known acceptance commit permits replacement execution; no optimistic launch after an indeterminate commit.
3. Close old-output and checkpoint admission, cancel/discard its queued application output, join readers/I/O and confirm process/cache cleanup. Preserve failure observations needed by an eligible recovery before physical retirement discards worker state. Already transmitted bytes cannot be recalled.
4. Prepare only the accepted target under existing managed process/readiness limits. No second active generation, widened sandbox, dynamic input URL, new hardware route or playback-time probe is introduced. Direct output gets its own revision group even without a process.
5. Recheck exact source identity, current revocable authority, control/operation ownership and target permission in a short publication transaction. Publish the immutable output result and current pointer together; acknowledge pending worker ownership only after known commit and local current ownership. Return requested source position separately from the measured actual output origin.
6. A known replacement failure completes its cleanup and leaves no deliverable output; retain the last successful configuration and confirmed checkpoint for explicit correction/recovery. No old URL is revived. Reopen only eligible control/checkpoint admission after the known outcome; a competing command cannot enter during retained pending work or unresolved cleanup.
7. Stop, revocation, lease expiry, fencing or root closure cancels all current/pending work and prevents publication. Join every process/read/body/cache obligation before completing resources and releasing the parent. Retry cleanup explicitly; uncertainty stays counted/quarantined. Captured cleanup remains possible after profile deletion.

An uncertain acceptance/publication never proves rollback, readiness or cleanup. Status reports uncertainty; no new operation bypasses it and no automatic replay/promotion occurs. Retain actual resources, reconcile exact durable evidence through owned cleanup, and require the existing two-step operator fence/resources-gone procedure after loss of the owning instance. A timeout, observer loss or database row alone cannot authorize freeing physical resources. Root completion remains nonzero if owned completion is still unconfirmed.

Checkpoint sequences are positive safe JSON integers, increasing within the exact last-published output revision. Keep one latest checkpoint, not a new receipt row per report. Matching sequence/payload reconciles; changed reuse, older sequence or a replaced output cannot mutate history, renew a lease or spend recovery. Checkpoints for an unpublished candidate are refused. A still-current failed output can accept the client's last confirmed position while no replacement is pending; accepting replacement freezes its checkpoint reference before retiring it. A later publication changes revision and rejects late reports from the old output.

Validate finite nonnegative source milliseconds against the admitted precise duration and existing history integer bounds; missing trustworthy timing refuses position-dependent operations without destroying healthy output. Do not store a requested seek as confirmed viewing progress. Preserve interactive versus ambient history/resume rules, pause/buffering state and exact history tuple. Reuse/extract history primitives on the owned connection with that exact tuple; do not call the legacy heartbeat/recovery orchestration or its history-by-ID-only mutation. Configuration ETags do not include checkpoint positions/sequences; playback-state changes exposed by configuration do change that representation's ETag. An advancing sequence does not require an advancing position, since explicit user seeking can move backward. A recovery operation names the latest accepted checkpoint sequence; a mismatched sequence refuses before spending an attempt or allocating.

Keep at most two recovery replacements per logical viewer, including any applicable already-spent attempt; seeks/reconfiguration never reset that budget. Reuse the existing one stall restart and narrowly eligible single four-second encoded-video decode skip, with near-playhead evidence and explicit skipped-time/quality disclosure. Unknown/container/unsupported or exhausted recovery cannot silently promote copied video/HDR or launch another process. A healthy worker hint does not charge an attempt or mark the worker failed. A qualified direct/copied-video decoder downgrade is a separate admitted case: require its exact current output/descriptor and positive compatible-output evidence, preserve tracks/confirmed position, use the existing qualified software route and share the same budget. Direct fallback retains its admitted output-support requirement; copied-video fallback requires both support and smoothness for the exact fallback descriptor. Client decode-error reports are hints, not server verification of a decoder failure. No additional native decoder, format, hardware startup fallback or automatic client retry is qualified here.

Persist attempt reservation and fixed failure/action evidence with operation acceptance. Once an eligible recovery attempt is accepted, failed preparation does not refund or repeat it. Refusals before acceptance do not spend it. Pause intent survives replacement; delivery readiness does not claim the player resumed or decoded successfully. Independently qualify the existing precise copy-origin and video-encode/audio-copy timing rules before returning a ready replacement.

## Operation retention and fixed budgets

The confirmed extra limits count retained reconfiguration operations: 256/viewer, 8,192/household and 131,072/global. Initial output attribution is bounded by the existing parent-reservation quotas; each operation has at most one candidate output. Persist only bounded attribution, hashes and allowlisted result/evidence, not raw source paths, client observations, complete private plans or process diagnostics. Enforce admission/counter/deletion rules in PostgreSQL as well as service code.

An operation becomes eligible for pruning no earlier than 24 hours after both terminal outcome and confirmed cleanup of its output. A current/pending output, an unresolved operation or uncertain cleanup is never pruned by age. A still-playable completed operation therefore remains retained until actual retirement. Storage fullness refuses additional changes; it never evicts a young receipt or blocks existing renewal, Stop, status or cleanup.

Check existing operation receipts first. Genuinely new UUIDv7 operation IDs must fall within the established five-minute-past/two-minute-future database-time window and a durable monotonic rejection cutoff that independently covers operation admission. Pruning cannot reopen a command ID, even after backward clock movement or replay through another viewer. Changed parent/actor/intent/precondition reuse conflicts. An old absent operation receives explicit expired semantics without allocation. Keep counter/cutoff update and deletion atomic.

Extend retained sixty-second maintenance within its existing single reserved operation. Prune at most 128 logical receipts total per pass, fairly sharing work between operation and parent receipts. Index eligibility and scan a bounded candidate set with owned locks; do not scan a large guide/history table or mass-delete children through a parent cascade. Prune eligible operation/output pairs before deleting their parent, and permit parent pruning only when no retained operation remains. Final released current pointers can be cleared only under exact completion/retention guards. Cascade/FK/DELETE/TRUNCATE tests must prove there is no age/cleanup bypass. Maintenance failure must not disable valid renewal/cleanup or silently lose its root-owned completion.

| Resource | Contract |
|---|---|
| Request/response | Existing 32-KiB command/status ceiling and five-second body deadline; checkpoint body at most 1 KiB, stored operation result at most 8 KiB; strict fields, bounded nesting/arrays and fixed public enums. |
| Execution | At most one accepted operation per each of 128 retained local viewers; no queue or replacement overlap. No new unbounded task/channel/reader collection. |
| Output | Existing 128 retained/pending generations, 128 responses, eight concurrent I/O operations, 64-KiB chunks with at most two queued chunks, 1-MiB playlists. |
| SQL | Existing seven regular plus one reserved maintenance operation and transaction/deadline limits. Prompt refusal when full; retain accepted completion ownership. |
| Preparation | Existing fifteen-second managed readiness allowance and existing bounded source inspection/progress/watchdog rules. |
| Lifecycle | Existing sixty-second renewal, thirty-second shared root drain and sixty-second pool-close budgets. No timeout-only resource/claim release. |

The 128-row maintenance limit bounds logical operation/output-pair and parent receipt work, not every physical index entry touched by PostgreSQL. Each operation has one candidate output; parent pruning must not perform an unbounded child cascade. Verify representative full-limit query plans and lock/time bounds rather than claiming a quota guarantees a storage-byte or latency ceiling.

## Verification plan

No new runtime result is claimed at this documentation checkpoint. Qualification must include:

- Real PostgreSQL migration/backfill, row guards, operation replay/time fencing, quota races and indexed bounded pruning.
- Same viewer/history across seek and track/quality changes; exact selected audio/subtitle mapping and truthful requested-time/output-origin separation.
- Old file/HLS/initialization/segment URLs, reordered checkpoints, concurrent changes, Stop, revocation, source edits and profile/session changes.
- Cap transitions and cap-only reductions, encoded audio in direct-stream output, failed/uncertain cleanup, and completed-but-playable encoding.
- Worker failure versus successful completion; explicit recovery, healthy hints, shared budget across seeks, near/far decoder evidence and applicable decoder downgrade cases.
- Lost request observers and BEGIN/acceptance/publication/cleanup COMMIT acknowledgements; no unowned allocation or hidden replay after uncertainty.
- Real FFmpeg/file/HTTP behavior on Windows and managed Linux, independent decoding/timing evidence for applicable qualified routes, and existing admission/root regression.
- Existing source/resource/deadline guards, source-frozen managed qualification, input-verified compiler checkpoint reuse with fresh runtime tests, and exact baseline lint comparison.

Exercise both cookie and bearer paths with actual HTTP authentication/context and cross-account/session/profile/transport refusal. Test configuration ETags against progress-only updates, pause-state changes, accepted replay, stale revision and concurrent operation admission. Verify that lost checkpoint acknowledgements reconcile their sequence without history duplication or renewal. Test full operation quotas, changed-parent replay, old absent UUIDv7s, active/current/uncertain non-pruning, counter consistency, parent deletion/cascade refusal and representative large indexed query plans.

Decode actual replacement output independently at requested positions, including earlier copy origins and mixed video-encode/audio-copy timing. Verify selected tracks, pause intent, source immutability, unchanged budget across seeks and recovery refusal near/far from the playhead as appropriate. Every applicable existing direct/copied-video fallback case must pass; do not infer full format compatibility from only an H.264 transcode fixture. Test direct-to-encoded cap refusal before old retirement, encoded-to-encoded recovery after a lower cap, direct publication/claim release and retained claims after failed/uncertain cleanup.

Fault qualification covers lost completion/panic and cancelled BEGIN plus before/after acceptance, readiness and cleanup COMMIT acknowledgements. Observe actual worker/read/body/cache completion through Stop, profile deletion, lease expiry, source/permission revocation, root closure/deadline and second-signal termination. Preserve existing real timed boundaries. A test relay may inject acknowledgement faults but cannot relax production clocks, resource limits, source authority or cleanup proofs.

Preserve the existing 128-viewer/generation/response bounds, eight delivery I/O operations, 64-KiB chunks with two queued chunks, 1-MiB playlists, seven regular plus one maintenance SQL operation, thirty-second root drain and sixty-second pool-close budgets. Operation receipt quotas supplement these limits; they do not expand media capacity. Use the approved guarded Classifarr handoff only if Linux headroom requires it, restore it through owned finally cleanup, and leave Harmoniarr running. Supplied private IPTV sources and feed data are unnecessary for this local-file work and remain ignored.

## Complete implementation checkpoint

The proposed implementation is one isolated normal-build backend increment: explicit version-two Start/control/checkpoint acknowledgement; conditional, retained seek/reconfiguration/recovery operations; immutable source-scoped outputs; preserved viewer/history and negotiated seek/recovery settings; current authority and encoded-viewer claims; the confirmed bounded receipt policy; and actual PostgreSQL/HTTP/FFmpeg Windows/Linux qualification. Reuse locked dependencies, approved standard legal headers and existing managed test ownership. Keep code modular and qualified-source inventories complete; do not add explanatory source comments, a branch, a release or a CHANGELOG update.

Acceptance requires all operations to preserve one viewer/history, reject stale/replayed/mismatched work, honor revocation, retain uncertain cleanup/claims and join owned completion under unchanged root limits. Confirmed outcomes and corrected attempts must be recorded after testing; prior qualification is not evidence for new code. Documentation/link/privacy checks and exact warning-baseline comparison precede the authorized commit/push of all task changes on main.

Existing online clients, normal production attachment/shared-cap enforcement, legacy bypass closure, Live TV media output, provider VOD/switches, recordings, alternate item/files, new format/hardware qualification, native passkeys and unrelated history/instance/audit retention remain deferred. Once this backend increment passes, all online client families must adopt the retained command/checkpoint/renewal/Stop protocol and weaker online control/delivery bypasses must close before production enforcement can be claimed.

## PR applicability and outcome

The repository-wide GitHub MCP open-PR search returned zero candidates at this checkpoint. No random local PR patch or stale Dependabot rebase applies. [Earlier Dependabot maintenance](../operations/DEPENDABOT_MAINTENANCE.md) already closed superseded PR 5 without merging or downgrading either lockfile. No alert was dismissed.

The complete-contract checkpoint changes eleven Markdown documents. Staged whitespace, 739 local Markdown targets and exclusions for the supplied private source values pass. The current source inventory remains `927525bd2701c4e5e75d5e9c36c2d868631641cb301f1564da23cb636fd2426a`, identical to the qualified command/delivery source; no runtime test was rerun for these documentation-only changes. These checks qualify documentation consistency, not the proposed implementation.

Research and all ten product decisions are complete. The concrete contract above awaits only the final shared-understanding confirmation required by [the repository design-review procedure](../../CONTEXT.md#design-review-with-grill-me). No new runtime result is claimed. The next action after that confirmation is modular implementation and scoped qualification, then commit/push on main without a release.
