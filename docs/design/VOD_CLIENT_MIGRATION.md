# Shared VOD client migration

## Status and authority

Research and design review, October 28, 2026, against `903e670` on main. The user confirmed Q1: migrate and qualify the complete shared web/Tauri player first, including Start/status, seek, track/quality changes, recovery, checkpoints, renewal and Stop. Normal production attachment and shared-cap enforcement remain off. Flutter interactive playback, native Android/iOS ambient playback and Android/Fire TV follow before final rollout.

Here a client is an existing platform app connecting to the same Duskcue server. Browser and Tauri desktop share one player implementation; mobile/background and TV players have their own existing platform code. The migration creates no additional app or household model. All platforms retain the same backend permissions and source-priority rules.

This is the complete proposed client contract, not an implementation or a new runtime qualification. Q1–Q9 below are confirmed; final shared-understanding confirmation of the concrete contract is still required before implementation under [the repository procedure](../../CONTEXT.md#design-review-with-grill-me). No release, branch, dependency update or CHANGELOG change is proposed.

The qualified [protocol-two backend](VOD_RECONFIGURATION.md), [command/delivery](VOD_COMMAND_DELIVERY.md), [admission](PLAYBACK_ADMISSION.md), [receipt retention](PLAYBACK_RECEIPT_RETENTION.md), [output lifecycle](VOD_OUTPUT_LIFECYCLE.md), [recovery](PLAYBACK_RECOVERY.md), [source capabilities](PLAYBACK_SOURCE_CAPABILITIES.md) and [memory](MEMORY.md) retain their authority. Earlier server qualification does not qualify this new client adoption.

## Verified repository facts

Three read-only fact explorations inspected client families, server rollout and shared player/qualification code. They changed no files, ran no builds/tests, accessed no private feeds and stopped no services.

| Boundary | Current evidence | Migration consequence |
|---|---|---|
| Shared web/Tauri | [Playback API](../../clients/web/src/lib/api/playback.js) uses legacy Start/heartbeat/seek/Stop/info; [player store](../../clients/web/src/lib/stores/player.js) reconstructs generic file/transcode URLs. Desktop builds the same web application. | Adopt returned immutable output URLs and the complete new control protocol together. |
| Track/quality changes | [Runtime](../../clients/web/src/lib/playback/runtime.js) calls another `player.play`, then a separate seek. | Use same-viewer Reconfigure instead of allocating another Start/history identity. |
| Current heartbeat | [Heartbeat service](../../clients/web/src/lib/playback/heartbeat.js) combines position, pause/buffering, renewal and recovery; sending is suppressed during loading/seeking and ordinary failures are absorbed. | Separate lease renewal, sequenced checkpoints, command outcomes and failure observations. |
| Scope | [Release service](../../clients/web/src/lib/playback/release.js) binds account/server only. [Auth](../../clients/web/src/lib/stores/auth.js) and [session schema](../../clients/web/src/lib/household/schema.js) already expose stronger session/profile/context/credential/transport evidence and database observation time. | Bind the controller and command identities to the exact authoritative context; do not use cached account identity alone. |
| Start/reload | There is no playback UUIDv7 generator or retained lost-Start reconciliation. The [play route](../../clients/web/src/routes/play/[id]/+page.svelte) starts again on reload; current player state is memory-only. | Define stable command identity and reload/uncertain-outcome behavior before wiring Start. |
| Background | Current visibility/native-background logic gates autoplay countdowns only; active playback and paused/hidden heartbeats continue. No playback bfcache journal/restoration handler exists. | Preserve that policy while separating renewal; verify actual session/deadline after suspension and tear down stale source explicitly. |
| Close | The existing release coordinator retains failed Stop and supports explicit retry. SPA navigation waits, but hard unload/component disposal cannot await confirmed cleanup. | Preserve truthful pending cleanup and never report a fire-and-forget Stop as successful release. |
| Native media | [Source adapter](../../clients/web/src/lib/playback/source.js) supplies bearer headers through hls.js and refuses bare bearer direct/native-HLS attachment. Browser cookies use a different media path. | Qualify real cookie and bearer requests through every output resource, not just JSON control calls. |
| Transport primitives | [API core](../../clients/web/src/lib/api/core.js) already supports bounded JSON, deadlines, scoped cancellation, ETag/Location/Retry-After metadata and selected-server credentials. | Reuse these primitives in focused protocol modules instead of creating another fetch stack. |
| Server attachment | [Normal router](../../server/src/router.rs) omits [command routes](../../server/src/domains/playback/commands/mod.rs); normal startup does not attach the admission owner. [Controlled fixtures](../../server/src/domains/playback/commands/tests/support.rs) already attach it and merge both routers. | Client adoption can receive controlled real-backend qualification while normal startup stays unchanged. Mounting a router alone is insufficient. |
| Accounting | The new lane always checks inherited total/transcode policy and calls the real owner. Generic legacy file/control routes still exist. | A migrated subset cannot establish accurate household-wide cap enforcement across all legacy clients. No accounting-bypass mode is proposed. |
| Identity | New Start/operation IDs are UUIDv7. Viewer/start ID, history ID and output revision are distinct. Session context supplies database time; status/renew provide observations/deadlines. | Keep all identities distinct, generate fresh IDs against scoped server-time evidence, and reconcile the same intended command rather than creating a replacement ID. |

## Remaining online families

| Family | Actual source boundary | Known migration work |
|---|---|---|
| Flutter interactive | [Playback service](../../clients/mobile/lib/services/playback_service.dart), [entry screen](../../clients/mobile/lib/screens/playback_entry_screen.dart) | Heartbeat response is discarded; Stop clears state and swallows failures; track/quality changes start another legacy session. |
| Native Android ambient | [Ambient service](../../clients/mobile/android/app/src/main/kotlin/com/duskcue/mobile/AmbientPlaybackService.kt) | Independent legacy Start/heartbeat/Stop; clears runtime before Stop and advances despite failed release. |
| Native iOS ambient | [Ambient bridge](../../clients/mobile/ios/Runner/AmbientPlaybackBridge.swift) | Independent legacy control; heartbeat/Stop completion is ignored. |
| Android/Fire TV | [Shared API](../../clients/tv/android/app/src/main/java/com/duskcue/tv/api/DuskcueApiClient.kt), [shared player](../../clients/tv/android/app/src/main/java/com/duskcue/tv/playback/TvPlaybackService.kt), [Fire source sets](../../clients/tv/fire/app/build.gradle.kts) | Unit-valued control results cannot adopt revisions; playback discards outcomes; local seek precedes server reporting. Fire pre-play authorization also consumes history/session identity. |

Downloaded/offline playback remains separate from the online viewer ledger. Migrating the visible web player alone does not migrate native ambient or TV playback.

## Official research

Sources were discovered through MCP search and retrieved links on October 28, 2026. WHATWG media/timer pages identify an October 26 update; Fetch identifies October 19. Installed code and the repository's qualified contracts govern version-specific behavior.

| Primary source | Application |
|---|---|
| [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html) | Accepted processing is distinct from completion. Preserve exact conditional representation tags, bounded status observation and Retry-After. Do not automatically replay a resource-changing POST after an uncertain response. |
| [RFC 9562](https://www.rfc-editor.org/rfc/rfc9562.html) | UUIDv7 includes a millisecond timestamp and random bits. Reuse one identity for the same command intent; it grants no authority. Server-time freshness and permanent replay refusal remain Duskcue policy. |
| [WHATWG media](https://html.spec.whatwg.org/multipage/media.html) | Media position, seeking and readiness differ. Checkpoints must reflect actual attached output and its measured origin; setting a requested position does not confirm decoded playback. |
| [WHATWG timers](https://html.spec.whatwg.org/multipage/timers-and-user-prompts.html) | Timer execution can be delayed. A scheduled callback cannot establish that a sixty-second lease is still valid after suspension or backgrounding. |
| [WHATWG Fetch](https://fetch.spec.whatwg.org/) | Scope cancellation and response-body ownership explicitly. Aborting local observation is not evidence that an accepted server command rolled back or released resources. |
| [W3C status messages](https://www.w3.org/WAI/WCAG22/Understanding/status-messages), [focus order](https://www.w3.org/WAI/WCAG22/Understanding/focus-order), [error identification](https://www.w3.org/WAI/WCAG22/Understanding/error-identification) | Localize accepted/preparing/uncertain/expired/cleanup outcomes; expose meaningful status without moving focus; keep recovery errors and corrective actions identifiable and keyboard reachable. These informative guides support WCAG evaluation, not a conformance claim. |
| [Tauri security/configuration](https://v2.tauri.app/reference/config/) | Preserve explicit native trust/permission boundaries. The installed desktop uses its existing Rust/keyring/session path and shared media transport; the HTTP plugin guide does not justify adding another transport or widening IPC permissions. |

The controller, journal and qualification proposals below are engineering conclusions from these sources and repository evidence. The standards do not prescribe Duskcue's storage policy, polling intervals or rollout sequence.

## Alternatives, pros and cons

| Approach | Pros | Cons | Recommendation |
|---|---|---|---|
| Migrate only Start | Small visible patch. | Legacy heartbeat/seek/Stop/output reject protocol-tagged sessions; uncertain starts can duplicate viewers. | Reject. |
| Extend the legacy store with every new state | Reuses familiar entry points. | Mixes two protocols, authority lifetimes and output identities in an already large orchestrator. | Select a focused new controller with a thin existing-store adapter. |
| Migrate all client families at once | One final rollout boundary. | Couples Flutter, Kotlin, Swift, shared web and both native/media transports before independent qualification. | Q1 selects shared web/Tauri first. |
| Qualify shared web/Tauri against the retained backend first | One shared implementation, real cookie/bearer evidence, bounded review surface. | Other online clients and generic legacy paths remain; production-wide caps stay deferred. | Confirmed Q1. |
| Fall back to legacy after a new-lane failure | Can appear to recover quickly. | Can allocate another viewer or bypass the admitted output/policy boundary. | Reject mid-session downgrade; Q2 covers explicit initial negotiation. |
| Memory-only command identity | Least persisted metadata. | Reload loses reconciliation identity while server work may remain counted. | Q3 selects a minimal tab-local journal instead. |
| Minimal tab-local reconciliation journal | Can reconcile a lost response/reload without inventing another Start. | Requires strict bounds, exact-session validation, lifecycle clearing and honest crash limitations. | Confirmed Q3; exact lifecycle remains to review. |

## Recommended stack and modular seams

Retain the locked Svelte 5/TypeScript/ES-module, hls.js and Tauri 2 client stack; reuse API core, auth/session context, source/timing/capability adapters, localized Player controls and isolated native qualification. Retain the qualified Rust/Axum/Hyper, Tokio, SQLx/PostgreSQL, UUIDv7 and managed FFmpeg server stack. No dependency, framework, engine or general-purpose retry library is proposed.

Separate bounded wire/schema/URL validation, server-time UUIDv7 generation, exact scope capture, command/status reconciliation, renewal, checkpoint sequencing, output adoption, recovery eligibility and optional journal ownership. Compose these behind an injected controller and a thin player/runtime adapter. Preserve existing legacy behavior before explicit new-lane selection; never intermingle viewer, history and output identities or treat a returned URL as authorization.

## Confirmed product decisions

1. **Q1 confirmed:** complete shared web/Tauri player qualification first; no normal production attachment or shared-cap enforcement in this increment.
2. **Q2 confirmed:** positive server acknowledgement selects protocol two, absence retains initial legacy selection, and new-lane admission/uncertainty forbids downgrade. No production advertisement is enabled here.
3. **Q3 confirmed:** one bounded tab-local reconciliation journal with IDs/scope identifiers only, no credentials/media/request bodies; verify actual session before recovery and never automatically Start/resume after reload.
4. **Q4 confirmed:** retain automatic recovery only for a positively eligible worker/decoder failure through a fresh output-bound command; uncertain commands require status reconciliation and are not replayed. Server eligibility/budgets remain authoritative.
5. **Q5 confirmed:** make a bounded final checkpoint attempt on Close, then proceed with Stop even if progress cannot be confirmed. Progress uncertainty is separate from actual resource release; preserve last confirmed history.
6. **Q6 confirmed:** renew independently during preparation, pause and background playback while the client can run. After suspension leaves the confirmed lease expired/uncertain, detach stale output and reconcile before continuing; no automatic new Start or grace period.

7. **Q7 confirmed:** include [conclusive Start withdrawal/settlement](VOD_START_SETTLEMENT.md) as a backend prerequisite. Explicit Close/Stop fences the exact UUID against later admission and observes already-owned preparation completion. Unknown/timeout is unresolved until then.
8. **Q8 confirmed:** a profile-only change detaches media and retains one cleanup-only journal record. Account/server/session/credential changes clear that local record and rely on server retirement; clearing it is never proof that a slot was released.
9. **Q9 confirmed:** known accepted Start/change/Stop/withdrawal may be observed at most once per second for at most sixty seconds, then through explicit Check status. Timeout is not server failure. No mutation replay; valid renewal remains independent.

The following concrete contract, acceptance criteria and deferred items were confirmed by the user after `67c6efc`. Hard unload/OS termination cannot establish successful Stop; Tauri tray Quit currently exits directly. Server-owned expiry and confirmed cleanup remain authoritative when a client can no longer observe them.

## Concrete rollout and protocol selection

Use the existing authorized playback-info/capability path to return an optional explicit command capability only when the controlled fixture has mounted the complete command/settlement router, attached a compatible owner and satisfied its instance/migration boundary. Require the complete feature acknowledgement, including conclusive Start settlement, conditional controls, checkpoints and lease observations. Source-codec capability negotiation remains separate. An absent acknowledgement retains the initial legacy branch; malformed/partial acknowledgement is a known refusal. Normal startup advertises no new capability and attaches no new owner in this increment.

Freeze the chosen protocol for one intended playback. Once protocol two is selected, never send its history/output IDs to legacy heartbeat/seek/Stop/info or reconstruct generic file/transcode URLs. A failed/uncertain command cannot fall back to legacy. Preserve existing legacy behavior and tests before initial selection. Other client families remain unchanged.

Reuse authenticated no-store session context before mutation. Capture exact server origin, account, session, credential version, selected profile/context revision and transport, not just origin/account or profile ID. Gate every response/adoption on that current capture. Credential-only rotation must invalidate protocol-two playback even when the profile ID is unchanged. An old 401 cannot sign out a newer session.

Generate UUIDv7 locally with the platform CSPRNG and a fresh database-time observation advanced by a monotonic client clock. Refuse ID generation when cryptography or trustworthy time is unavailable; never substitute UUIDv4 or Math.random. Refresh observations after suspension and before a new mutation if older than thirty seconds. Use one immutable UUID/body/condition per intended Start or change. No automatic Start/change replay; status reconciliation or explicit cleanup precedes another intended Start.

## Command, renewal and output ownership

Separate the Start/viewer request ID, retained history ID, current output revision and operation ID in typed state. Start adopts only its server-provided accepted/status resources. A 202 is not ready playback. Before publishing output, validate protocol/IDs, exact captured scope, current held state/deadlines, source timing and URL structure. Require returned media/control/status paths to remain on the selected server and exact viewer/output resource; reject credentials, fragments, unexpected queries or unrelated paths.

Configuration writes use the exact strong configuration ETag, UUIDv7 operation ID, expected output revision and strict action. A checkpoint-only sequence change must not be confused with a configuration revision. One resource-changing operation at a time, no queue/supersession. Stop/withdrawal can preempt observation and request retained cleanup. Definitive validation/policy refusal before acceptance preserves eligible current playback; stale conditions or uncertain outcomes require reconciliation instead of silently rebuilding a request or merging state.

Seek preserves admitted item/file, negotiated tracks/quality and recovery budget. Explicit track/quality changes obtain a fresh server plan within the same admitted item/file. Detach retired/uncertain output, discard late source events and preserve pause intent when adopting the new revision. A failed replacement has no old-output restoration; the counted viewer can remain renewable for explicit correction or Stop. Picking another item/file is another Start after confirmed release, not Reconfigure.

Renewal is independent of media progress and accepted changes, including preparation/pause/background when a held viewer and valid deadline are known. Keep at most one renewal in flight and ordinarily renew every fifteen seconds. A request still awaiting a receipt/lease cannot invent one. A lost renewal response may hide a successful extension; stop further automatic renewal dispatch until explicit status reconciliation confirms current authority/deadline. Detach stale local output when the confirmed viewer or authentication deadline expires or becomes uncertain, and revalidate after suspension before adoption. Neither browser timer scheduling nor buffered media can prove server authorization; the server remains authoritative for further delivery.

Checkpoints report actual attached media time plus the measured source origin, exact output revision, a positive increasing safe-integer sequence and paused/buffering state. Keep one in flight and at most one coalesced latest observation; no unbounded telemetry queue or per-report persisted receipt. Send ordinary progress at most every fifteen seconds and on meaningful state transitions. Do not report requested seek targets, encoder progress, detached output or an unconfirmed attachment as viewed position. Reconcile a lost sequence acknowledgement through the checkpoint resource without inventing progress or renewing the lease.

Eligible automatic recovery is a fresh output-bound command for a newly observed qualified worker/decoder failure, after a current checkpoint and positive existing recovery offer. Preserve the server's shared two-replacement budget, exact descriptor/position checks and skipped-time/quality disclosure. Do not recover from HTTP/metadata/auth errors, spend another command on the same unresolved failure, reset budget on seek, or introduce new hardware/format fallbacks. Unknown acceptance remains an explicit status/cleanup state.

## Journal, reload, scope and Close

One versioned sessionStorage record per tab, at most four KiB, holds only bounded Start/operation/output UUIDs, exact scope identifiers/fingerprint and lifecycle markers. No token, passkey/proof, source URL, title/guide metadata, selection body or private capability observation. Validate it as untrusted input with strict keys/types/lengths before use. Persist and round-trip the required record before a resource-changing POST; storage refusal prevents that mutation rather than silently losing identity or downgrading protocol.

Reload/bfcache restoration detaches sources and verifies fresh authoritative session context. Journal recovery is status/cleanup only; it does not automatically attach media, resume or Start. Keep a new intended Start blocked until the recorded intent is conclusively withdrawn/settled or its existing parent is released. Unknown and a bare age-based 410 are not absence fences. SessionStorage is not a guarantee across OS termination, window closure, copied tabs or data loss; server-owned retention/expiry/fencing still determines resource cleanup.

Profile-only change invalidates active control/media scope but retains minimal parent cleanup identity for the same exact account/session/credential/transport. Use captured-cleanup endpoints, not normal status/configuration reads, after that boundary. Account/server/session/credential change detaches media, cancels local observation and clears the journal without sending current credentials to an old server or claiming old resources are gone. An active owner grant merely expiring is not credential rotation.

Close captures actual current source position, detaches media and ends ordinary progress/renewal dispatch. Drain or attempt a final checkpoint for at most five seconds, then proceed with Stop/withdrawal even if progress is unconfirmed. Expose that uncertainty separately. Keep exact intent and cleanup ownership until terminal parent release or conclusive Start settlement; child-operation cleanup alone does not free a parent viewer. Repeated explicit cleanup actions reconcile the same identity and never allocate another viewer.

Preserve focused explicit recovery on SPA navigation failure. Natural end/next-episode advancement waits for parent release before another title Start. Hard unload, app/process termination and network loss cannot prove Stop success; do not clear a counted server obligation by reporting a local teardown. Existing autoplay background gating remains; this increment adds no automatic episode advancement while native/background state is unknown.

## Fixed budgets and accessible presentation

| Resource | Initial bound |
|---|---|
| Metadata and commands | Existing 32-KiB bounded JSON/envelopes; checkpoint request at most 1 KiB; five-second request/body observation deadline. No arbitrary dynamic URL or recursive unbounded response processing. |
| Client ownership | One active Start/viewer and one change/withdrawal intent; one current output; one renewal, one checkpoint and one status observation in flight. Coalesce one latest checkpoint only. |
| Journal | One strict record, four KiB maximum, before mutation; explicit cleanup/reconciliation, no raw request persistence. |
| Monitoring | At most one status GET per second for sixty seconds after known acceptance or explicit Check status. Respect Retry-After; stop on stale scope/auth/deadline and do not interpret monitoring timeout as server failure. |
| Server | Existing 128 command/viewer/generation/response, eight I/O, 64-KiB/two-chunk output, 1-MiB playlist, seven regular/one reserved SQL and root/pool deadlines remain. Settlement must share these limits, not add an unbounded observer/task queue. |

Use separate localized accepted/preparing/changing/recovering/uncertain/expired/withdrawing/cleanup states with honest actions. Announce meaningful state transitions through existing polite status regions; identify errors and explicit correction without repeated timer announcements. Preserve keyboard operation, menu/Close focus and visible focus through source replacement. A media play promise/autoplay refusal becomes an actionable user-gesture state without another Start. Test browser and actual native focus/status behavior separately; no blanket WCAG or device-compatibility claim follows from metadata tests.

The [local/recording/provider lifecycle](IPTV_SOURCE_LIFECYCLE.md) records the newly confirmed source policy. It does not add cross-file replacement to this client increment. New local availability never silently retargets an active source; provider/recording/Live TV playback implementation stays deferred.

## Required qualification and current outcome

Meaningful qualification must cover actual cookie and bearer backend requests, lost Start/change/renew/Stop observations, exact request replay/mismatched reuse, stale session/profile/output responses, direct/HLS adoption, seek and selected tracks/quality without another viewer/history, actual source-position checkpoints, lease expiry/suspension, eligible worker/decoder recovery, Stop/navigation/reload and owned cleanup. Keep browser UI/keyboard/focus/status tests distinct from isolated actual Tauri execution and real media/database/process evidence. Mocked desktop fixtures alone cannot qualify native transport or playback.

Preserve existing managed testing, memory/headroom, source-freeze, process/SQL/drain deadlines and cleanup rules. Supplied private IPTV feeds are unnecessary for this local-media work and remain ignored. New source files may use the previously approved legal headers; no explanatory code comments or dependency changes are proposed.

GitHub MCP's repository-wide open-PR search returns zero candidates. No random local patch or stale Dependabot rebase applies; [superseded PR 5 cleanup](../operations/DEPENDABOT_MAINTENANCE.md) remains complete. No PR was merged, dependency changed or alert dismissed.

All nine client decisions and the concrete implementation contract were confirmed after `67c6efc`. Research/fact inspection only at this checkpoint. No new application implementation, build/runtime qualification or production enablement is claimed. Existing backend results remain historical evidence for the unchanged source. Commit/push of documentation on main does not complete the pending client implementation.
