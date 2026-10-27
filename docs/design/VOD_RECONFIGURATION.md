# Same-viewer VOD reconfiguration

## Status and authority

Research and design review, October 27, 2026, against `7a1aaa17987e0d9fc308d3168131e45b94a1cc60` on main. This is a review draft, not an implemented or qualified capability. The first four product decisions below are confirmed; the remaining interview and complete-contract confirmation are pending. No runtime change, dependency update, production registration, client migration or release is claimed.

This follows the qualified [VOD command protocol](VOD_COMMAND_DELIVERY.md). [Admission](PLAYBACK_ADMISSION.md), [receipt retention](PLAYBACK_RECEIPT_RETENTION.md), [output ownership](VOD_OUTPUT_LIFECYCLE.md), [recovery](PLAYBACK_RECOVERY.md), [source capabilities](PLAYBACK_SOURCE_CAPABILITIES.md), [seek timing](PLAYBACK_SEEK_TIMING.md) and [memory](MEMORY.md) retain their existing authority. One logical online viewer, one exact history identity, sixty-second renewal and confirmed physical cleanup remain required.

## Confirmed decisions

1. Seek, recovery and track/quality changes remain within the originally admitted media item and file. Another title, episode or file requires a new Start. Preserve the viewer, history and interactive/ambient context.
2. Retire the old output, cancel its readers and confirm cleanup before preparing replacement output. Playback pauses during preparation; the old output cannot be restored after cleanup.
3. Accept one reconfiguration per viewer at a time. Refuse competing commands without queuing or supersession. An exact repeated command reconciles its existing operation; Stop and revocation can cancel it.
4. An observed worker failure leaves a counted, nondeliverable viewer eligible for explicit recovery or Stop within its renewed lease. Clean up failed output without automatically launching a replacement. Natural successful encoding remains playable and is not a failure.

## Decisions under review

| Choice | Recommendation under review | Consequence |
|---|---|---|
| Progress | Separate absolute source-position/pause/buffering checkpoints, bound to output revision and increasing sequence; no lease extension. | Late reports cannot overwrite the replacement's progress. Encoder progress never establishes viewing position. |
| Configuration | Seek/recovery preserve negotiated tracks/quality; explicit changes obtain a fresh plan under current limits and capability evidence. | Actual viewing/source/mode revocation still retires the viewer. |
| Recovery coverage | Qualify existing worker recovery and direct/copied-video decoder downgrade in this backend lane. | Preserve the shared two-replacement budget; client adoption and additional hardware/format qualification remain separate. |
| Transcode-viewer claim | Direct-to-encoded requires capacity; encoded-to-encoded retains its claim; encoded-to-direct relinquishes it only after old resource cleanup. | One total viewer throughout; no timeout or uncertain cleanup releases a classification. |
| Replacement failure | Keep a known failed replacement unavailable but renewable for explicit correction or Stop. | No automatic replay or restoration; spent recovery attempts stay spent. |
| Operation receipts | At least 24 hours after terminal completion and confirmed output cleanup; 256 per viewer, 8,192 per household, 131,072 total. | Separate storage backpressure and UUIDv7 replay fencing; existing renewal/Stop/cleanup remain available when full. |

The complete contract will distinguish a known preparation refusal from uncertain admission/publication or unconfirmed cleanup. Existing fail-closed ownership rules continue to apply to uncertainty; a correction command cannot bypass an unresolved resource obligation.

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

## Verification plan

No new runtime result is claimed at this research checkpoint. The implementation contract must include:

- Real PostgreSQL migration/backfill, row guards, operation replay/time fencing, quota races and indexed bounded pruning.
- Same viewer/history across seek and track/quality changes; exact selected audio/subtitle mapping and truthful requested-time/output-origin separation.
- Old file/HLS/initialization/segment URLs, reordered checkpoints, concurrent changes, Stop, revocation, source edits and profile/session changes.
- Cap transitions and cap-only reductions, encoded audio in direct-stream output, failed/uncertain cleanup, and completed-but-playable encoding.
- Worker failure versus successful completion; explicit recovery, healthy hints, shared budget across seeks, near/far decoder evidence and applicable decoder downgrade cases.
- Lost request observers and BEGIN/acceptance/publication/cleanup COMMIT acknowledgements; no unowned allocation or hidden replay after uncertainty.
- Real FFmpeg/file/HTTP behavior on Windows and managed Linux, independent decoding/timing evidence for applicable qualified routes, and existing admission/root regression.
- Existing source/resource/deadline guards, source-frozen managed qualification, input-verified compiler checkpoint reuse with fresh runtime tests, and exact baseline lint comparison.

Preserve the existing 128-viewer/generation/response bounds, eight delivery I/O operations, 64-KiB chunks with two queued chunks, 1-MiB playlists, seven regular plus one maintenance SQL operation, thirty-second root drain and sixty-second pool-close budgets. Operation receipt quotas supplement these limits; they do not expand media capacity. Use the approved guarded Classifarr handoff only if Linux headroom requires it, restore it through owned finally cleanup, and leave Harmoniarr running. Supplied private IPTV sources and feed data are unnecessary for this local-file work and remain ignored.

## PR applicability and outcome

The repository-wide GitHub MCP open-PR search returned zero candidates at this checkpoint. No random local PR patch or stale Dependabot rebase applies. [Earlier Dependabot maintenance](../operations/DEPENDABOT_MAINTENANCE.md) already closed superseded PR 5 without merging or downgrading either lockfile. No alert was dismissed.

Research and the first four decisions are complete. The remaining decision round and full concrete contract require confirmation before implementation under [the repository design-review procedure](../../CONTEXT.md#design-review-with-grill-me). After qualification, migrate every online client and close weaker legacy delivery/control paths before enabling production viewer caps. Live TV media output, provider VOD/switches, recordings, additional formats/hardware and unrelated history/audit retention remain separate.
