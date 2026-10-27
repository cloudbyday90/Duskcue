# VOD command and delivery protocol

## Status and authority

Research and design review, October 27, 2026, against `84fa21e` on main, published in `b2c32dd`. The user confirmed all seven product decisions and the complete concrete implementation contract. The confirmed contract is implemented and scoped qualification passes on native Windows and managed amd64 Linux. Verified runtime implementation: `e8914d12d880502de67e4ed1a4fe427bea2e5f4e` on main; documentation-only successors preserve its qualified source. Main only; no release, branch or PR merge.

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
| [Tokio TCP socket options](https://docs.rs/tokio/latest/tokio/net/struct.TcpStream.html) | A qualification relay that splits PostgreSQL frames sets TCP_NODELAY on both sockets, avoiding unintended Nagle buffering while preserving explicit acknowledgement faults. Installed source remains authoritative for the locked version. |
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

## Final recommendation stack

| Layer | Selection | Benefit | Cost or limit |
|---|---|---|---|
| Toolchain | Locked Rust/Cargo 1.99.1, existing edition/workspace | Reproducible builds with the current verified SDK. | Dependency upgrades remain separately reviewed. |
| HTTP | Existing Axum/Hyper, cookie/bearer auth, RFC 9110/9457 | Explicit acceptance/reconciliation and fixed private failures. | Clients must explicitly migrate; old URLs cannot prove new protocol support. |
| Durable state | SQLx 0.9/PostgreSQL 18, UUIDv7, short default-discard transactions | Atomic intent/authority/receipt attribution and replay refusal. | More connection churn and coordination than history-only bookkeeping. |
| Resource ownership | Existing Tokio/Tokio-util, scoped groups and retained completion | Accepted work survives observers; cleanup uncertainty cannot free capacity. | Quarantined resources require verified retry or fenced operator reconciliation. |
| Media | Managed FFmpeg 8.1.2 and existing confined file delivery | Reuses executable/readiness/process/cache safeguards. | The first bound lane uses software output and does not qualify new hardware fallback, formats or clients. |
| Rollout | Isolated backend qualification before reconfiguration/client migration | Preserves current clients while testing the new boundary. | Production shared caps and Live TV play actions remain deferred. |

## Confirmed implementation contract

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

## Implemented wire contract

The isolated router uses `/api/v1/playback-commands` and the positive `duskcue-vod-command/1` schema. Start is POST on the collection; a command has GET status, POST `renew`, POST `stop`, GET `retirement`, and authorized direct `file` or HLS manifest/rendition/init/segment paths. All mutations require application/json, bounded bodies and fixed errors. Start/renew also require the authoritative `x-session-context` precondition. Credentials remain cookies or Authorization headers.

| Operation | Definite result | Meaning |
|---|---|---|
| Fresh Start | 202, preparing, Location/status URI, database time and initial deadline | Durable acceptance; no readiness or media URL claim. |
| Exact repeat / status GET | Current bounded command state | No allocation, launch, revival or lease extension. Only positively published local ready output carries a media URL. |
| Renew | 200, renewed, database time and deadline | Lease acknowledgement only, with no post-accept readiness read. |
| Stop | 202, stopping and retirement Location | Retirement accepted; resources remain counted until cleanup and release are observed. |
| Retirement GET | Released/failed/stopping/cleanup_uncertain/foreign uncertain metadata | Captured exact-session cleanup evidence, never media. Available after profile switching/deletion; it does not relax viewing scope. |

`request_id` identifies the stable logical viewer. `ready.session_id` identifies its partitioned history record; its exact partition timestamp remains server-side. Media and controls use the request identity. Physical generation IDs remain internal, and bound history IDs cannot be used on legacy control routes.

Ordinary status and output require the selected viewing profile/context. Stop/retirement authenticate the captured active account/sign-in/credential revision/transport without requiring a selected profile. They accept no new playback progress. Bad or revoked authentication remains independent from retirement evidence. Explicit resource cleanup retries do not override a foreign owner or lost completion.

The first bound output uses the existing software FFmpeg path so hidden hardware fallback cannot change its admitted generation. Physical output failure/retirement also cancels its logical viewer; successful encoding remains playable and counted until retirement. Source inspection and delivery are retained, bounded and checked against the admitted file evidence. Existing legacy hardware/recovery behavior remains unchanged.

## Qualification method and corrections

Qualification uses disposable, ownership-checked PostgreSQL 18 databases, actual loopback HTTP/cookie/bearer authentication, managed FFmpeg and copied bounded media fixtures. Wire proxies interrupt BEGIN and admission/readiness/release COMMIT acknowledgements before/after publication. A controlled private SQL-task panic verifies lost completion without adding a production failure hook. Tests use real sixty-second lease expiry and existing root deadlines; clocks and cleanup proofs are not bypassed.

Adversarial work found and corrected two runtime boundaries: existing authentication decoded a null selected profile as a UUID after profile deletion, and logical viewers did not observe retired physical output. Ordinary authentication now refuses missing viewing context; captured Stop/retirement authentication remains exact-session scoped. The bound viewer observes physical retirement/failure without mistaking successful encoding for viewer completion. Cleanup status and renewal acknowledgements are separate from playable readiness.

Early attempts also corrected isolated fixture/history partition setup, a test assumption that every HLS manifest was a master playlist, and a fresh-acceptance assertion incorrectly applied to reconciled ready/released commands. Managed Linux initially passed twenty command subcontracts before a readiness-fault observer timed out. The fault observer now covers the existing fifteen-second readiness allowance plus bounded SQL/source inspection, and reports its precise checkpoint and bounded state evidence. The shared test-only PostgreSQL relay disables Nagle buffering on both sockets; it previously emitted headers/payloads separately without that option. This is a harness transport correction, not a production deadline change. A subsequent in-progress build was explicitly interrupted through its verified compiler client/plugin, leaving the owning helper to clean up and restore Classifarr. Those failed/interrupted attempts remain excluded from the successful evidence below. The full retention regression also identified the new child foreign key preempting the old bare-TRUNCATE SQLSTATE assertion. Qualification now checks the exact PostgreSQL foreign-key refusal and separately preserves the PBR04 CASCADE-trigger refusal. These attempts are not final qualification evidence. Each driver retained its owned-cleanup result. Final successful receipts and immutable source identities are recorded below.

## Qualification results

Scoped qualification is complete. Native and managed Linux runtime tests execute against the current qualified source. Later harness-only refinements received a complete native rerun, Linux execution and the shared admission/lint recheck. Production mounting and client migration remain deferred.

| Check | Completed evidence |
|---|---|
| Native Windows VOD | 22 tests in eleven groups pass, including 28 command subcontracts, all nine receipt-retention subcontracts, reader/worker/task-loss, legacy HLS HTTP, real root deadline and Stop contracts. Resource `c3af04de-f0db-4992-9484-2283c98ed69c`; owned cleanup passes. Receipt `2026-10-27T15-38-02-564Z-c097e706-0889-4c70-835a-fb2d214f6e07.jsonl`. |
| Managed amd64 Linux VOD | 21 private-library tests plus the actual Stop/recovery integration test pass, including all 28 command and nine retention subcontracts. Resource `54ba5e78-f1c3-4636-881e-cdb17052f397`; owned cleanup passes. Receipt `2026-10-27T16-17-10-431Z-5895dc8d-95fc-42e1-84a7-616a32aae058.jsonl`. |
| Default Windows library | 1,017 passed, eleven fixture-dependent ignores, 59.00 seconds. Receipt `2026-10-27T14-38-00-579Z-8b44c133-8da1-425a-aec0-45df3c0bb8be.jsonl`. |
| Compilation | Normal and playback-qualification all-target checks pass. |
| Admission | Real PostgreSQL/owned-resource integration recheck passes after the relay correction, 145.38 seconds. Receipt `2026-10-27T15-49-34-025Z-19db68ea-77de-458f-b433-9e093b077a03.jsonl`. |
| Root HTTP/TLS/process | All three cases pass, 241.41 seconds, including actual HTTP/pool deadlines, signals and join ordering. Receipt `2026-10-27T14-46-08-728Z-73d81e31-b1b9-4c1c-8231-3794b4990cfd.jsonl`. |
| Workflow | 27 pass, two existing skips. Receipt `2026-10-27T14-50-32-646Z-2e5f592a-141c-48aa-8a0a-53e2ed9e8abe.jsonl`. |
| Lint | Strict Clippy retains its nonzero baseline; diagnostic comparison matches warning/error messages and source files exactly, with no additions or removals. Existing 28 library / 32 library-test warnings and two Tonight-navigation casts remain. |

Native source inventory is `9a8c2197e39c17d743f952313494bd43300a87141a5fec6542230663b3b5ae56`, with driver SHA-256 `e5e813a011af41bff7743fce4f6c0e051c60b01cb83adf8ee0603be302dd657d`. The broader Linux/compiler inventory additionally includes the testing-memory drivers; its current frozen digest is `927525bd2701c4e5e75d5e9c36c2d868631641cb301f1564da23cb636fd2426a`. The final Linux image is `sha256:4623c0a16d91c65ed49a7c6b66d68405b035f4533fbda67ba169adf2dfaa358d`, based on immutable `sha256:f45eda1c8bbe3a9685196aae1200543601ec5efc2fb0f858ca863dd9d42ae8dc`. The existing integration/library/receipt compiler checkpoints all report cached on the final retry. Seventeen actual Cargo/cache durability checks pass, including changed/reverted backdated-source evidence. Executable hashes and source/image labels are verified before every fresh runtime group.

The normal Linux attempt compiled successfully but the next runtime preflight refused insufficient guest headroom. An unchanged-source retry under the approved Classifarr handoff reused all three completed checkpoints and ran every runtime test afresh. Compiler two-job/thirty-minute/32-MiB limits, 5.5-GiB preflight/1.5-GiB active reserve, four-GiB/two-CPU/256-PID runtime bounds and root thirty/sixty-second budgets remain unchanged. Every approved handoff restored the exact original Classifarr container/image healthy through finally cleanup; Harmoniarr was not stopped by this task. No memory guard, clock, cleanup acknowledgement or trust boundary was bypassed.

Qualification covers controlled local H.264/AAC media, cookie/bearer HTTP, actual FFmpeg/file/process behavior and PostgreSQL faults. It does not establish production-wide caps, new browser/native client adoption, new hardware fallback/format compatibility, provider VOD, DVR, or a new UI accessibility-conformance claim. Existing compatibility and client authorities retain those separate boundaries.

Final publication review covers 73 task files and 823 local Markdown references with no missing target or supplied private URL/host/address match. Copyright passes for 935 tracked source files; formatting and staged whitespace pass. Every staged qualified-source blob matches the final Linux inventory, and recomputing that inventory yields the same `927525bd2701c4e5e75d5e9c36c2d868631641cb301f1564da23cb636fd2426a` digest. Documentation-only commit references do not change that source evidence.

## PR applicability and next increment

GitHub MCP open-PR search returned zero candidates. No random patch or stale Dependabot rebase applies; superseded PR 5 is already closed without merge. Research changes no dependency or alert.

The earlier documentation-only checkpoint covered seven Markdown files, 679 local links and 911 source headers; it claimed no runtime result. Final publication checks below apply to the implemented source.

Next: all ten choices for [same-viewer reconfiguration](VOD_RECONFIGURATION.md) are confirmed. Its complete version-two contract preserves one viewer and original Start/history evidence, with conditional operations, output revisions, separate checkpoints, qualified recovery and bounded receipts; final contract confirmation precedes implementation. No new runtime qualification is claimed. Then migrate all online client families and close legacy bypasses before production caps. Live TV media output, provider-VOD connector/switches and recordings remain separate milestones.
