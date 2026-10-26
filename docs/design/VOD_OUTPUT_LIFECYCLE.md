# VOD worker and output lifecycle

## Status and authority

Research and design review, October 26, 2026, against `3127055` on main. The user confirmed the six product choices below. The complete implementation contract awaits final shared-understanding confirmation. No new VOD implementation or lifecycle qualification is claimed yet.

[Build order](../../BUILD_ORDER.md), [shared admission](PLAYBACK_ADMISSION.md), [streaming](STREAMING.md), [recovery](PLAYBACK_RECOVERY.md), [direct fallback](PLAYBACK_DIRECT_FALLBACK.md), [memory](MEMORY.md) and [root lifecycle](SSE_LIFECYCLE.md) retain their existing authority. This increment establishes the physical ownership needed before production viewer admission. It does not attach the viewer ledger, prune its receipts, change client playback protocols or enable Live TV playback. The supplied private feed fixtures remain ignored; this VOD increment does not need to fetch them.

## Confirmed choices

1. Complete normal-build root ownership for existing VOD FFmpeg workers, output readers and cache cleanup first. Shared-viewer enforcement, client protocol migration, bounded admission-receipt retention and Live TV play actions follow separately.
2. Root shutdown immediately cancels VOD workers/readers and discards remaining application-queued output, then observes owned cleanup. Already-transmitted bytes cannot be recalled. A cancelled partial HTTP response remains incomplete.
3. Ordinary Stop, seek and recovery retire the old HLS generation: refuse new reads, cancel accepted readers, finish their owned I/O, then delete cache. Successful encoding still releases encoder capacity while retaining playable output until explicit retirement. A replacement generation does not create a new logical play session.
4. Unconfirmed process/read completion or failed deletion retains and quarantines the affected generation. Explicit Stop cleanup retry remains available. Other healthy outputs may continue within finite budgets. Root cannot report clean shutdown until required cleanup is verified; caller loss and timeouts never erase ownership.
5. Initial machine budgets are 128 retained/pending output generations, 128 active delivery responses and eight concurrent delivery I/O operations. Use 64-KiB chunks with at most two application-queued chunks per response. Bound HLS playlist input/output to 1 MiB; stream media segments instead of whole-file materialization. Refuse new admission promptly at capacity. These limits are independent of household viewer and encoder ceilings.
6. Authorized late start/seek/recovery/output admission during closure or temporary output-capacity exhaustion returns no-store 503 with Retry-After. Preserve current authorization and explicit Stop cleanup retry. Add no automatic client replay, shutdown event or new UI flow.

## Repository findings

Three read-only fact explorations inspected worker, delivery and client/receipt lifetimes. They edited no files, ran no heavy tests and accessed no private feeds.

| Area | Evidence and implication |
|---|---|
| Root | `services/runtime/mod.rs:18` closes/drains admission, refresh, catalog, SSE, HTTP and the root tracker, but not TranscodeManager. Preserve the shared thirty-second drain and sixty-second pool-close budgets. |
| Worker registration | `services/transcoding/lifecycle.rs:131` independently spawns actors without retained root completion. Its Drop at line 298 signals only. |
| Whole start | `transcoding/start.rs:97` takes the physical permit, creates a directory at line 243, launches at line 247 and registers at line 294. Registration must cover preparation before those awaits, not only an already-running child. |
| Completed output | `lifecycle.rs:178` releases encoder capacity after physical exit, then waits for cancellation before removing cache. Natural process completion is not output retirement. Preserve completed VOD serving. |
| Cleanup retry | `lifecycle.rs:212` increments a request sequence and awaits that attempt or a successful completion. Delete failure retains the actor and permits fresh retry. Extend this ownership instead of forgetting failures. |
| Fallback deletion | `transcoding.rs:324` removes a session before awaiting deletion and reinserts only on observed error. Dropping that observer can lose metadata without observed cleanup. Make this retained work too. |
| Direct output | `playback/file_delivery.rs:17` puts a file inside ReaderStream with 64-KiB chunks. The handler ends before the response reader. There is no root cancellation or retained file/body completion. |
| HLS output | `playback/hls.rs:12` reads playlists into String and segments into Vec; handlers at lines 675, 699 and 732 construct independent Body::from responses. Cache deletion cannot recall copied application bytes or close an independent reader. |
| Path checks | `hls.rs:94` restricts rendition/segment names and lines 142–170 bind rendition paths to the actual manifest. Startup `readiness_cache.rs` already has owned-directory, regular-file and symlink checks. Preserve confinement while extending ownership. |
| Seek/recovery | TranscodeManager and recovery stop an old generation before replacement. Playback stores the replacement UUID while retaining the same logical play UUID. Preserve existing source/revision checks and pending-publication cancellation. |
| Stop history | `playback/stop.rs:75` returns a frozen replayable history result and retries physical cleanup. First Stop commits history before awaiting cleanup at line 165. Do not rewrite history on cleanup retry or make selected-profile loss prevent captured cleanup. |
| Existing limits | Physical starts use nonqueued semaphore acquisition. Startup readiness is fifteen seconds, startup playlist evidence is limited to 1 MiB and advancing-media-time watchdog is thirty seconds. Do not replace these with older aspirational timeout descriptions. |
| Delivery identity | Direct GET has file identity but no play-session/admission identity. PlaybackActor also discards the real authentication-session and credential revision. Root ownership cannot claim viewer enforcement or Stop correlation for direct readers. |
| Clients | Web reconstructs direct URLs; multiple mobile/TV clients discard heartbeat or Stop outcomes. Starts have no pre-dispatch stable request identity. The coordinated client/delivery migration remains necessary. |
| Receipt retention | Absent reservation IDs can be inserted as fresh starts. Deleting terminal receipts alone would revive old identities. No retention/pruning is added here. |
| Other jobs | Download packages, storyboards, ffprobe and subtitle jobs use separate process paths; scheduler tracking does not join every spawned job. This increment does not claim to supervise all background FFmpeg work or offline playback. |

## Official research, October 2026

URLs below were discovered through MCP search or retrieved primary-page links and opened on October 26. Product durations and resource budgets are Duskcue choices, not mandates from these sources. Online Tokio/TaskTracker pages can describe newer releases than installed code: the lock retains Tokio 1.52.3, Tokio-util 0.7.18 and tokio-process-tools 0.11.2. Installed Tokio file and TaskTracker source independently confirms the required completion/closure semantics. No dependency update is proposed.

| Primary source | Application |
|---|---|
| [Tokio graceful shutdown](https://tokio.rs/tokio/topics/shutdown), [TaskTracker](https://docs.rs/tokio-util/latest/tokio_util/task/struct.TaskTracker.html) | Separate admission closure, cancellation and observed task completion. TaskTracker close does not prevent later spawning; use a synchronized gate. Retain accepted tasks and their cleanup through caller cancellation. |
| [Tokio Child](https://docs.rs/tokio/latest/tokio/process/struct.Child.html), [tokio-process-tools](https://docs.rs/tokio-process-tools/latest/tokio_process_tools/) | A signal or dropped observer does not prove process exit. Reuse the installed process-group/job and explicit termination/wait machinery, bounded diagnostics and existing sandbox. Do not create a competing subprocess framework. |
| [Tokio File](https://docs.rs/tokio/latest/tokio/fs/struct.File.html) | File operations can remain in flight. Retain their ownership through completion; async conversion to the standard file waits for outstanding operations. Cancelling a request must not be reported as confirmed file cleanup merely because its future was dropped. |
| [RFC 9110](https://httpwg.org/specs/rfc9110), [RFC 9112](https://httpwg.org/specs/rfc9112.html) | Preserve current range, length, MIME and method semantics. Partial cancelled media must remain distinguishable from a completed response. Transport/application bytes already sent are outside queue-discard guarantees. Temporary unavailability does not authorize mutation replay. |
| [FFmpeg protocols](https://ffmpeg.org/ffmpeg-protocols.html) | Preserve the existing protocol allowlist, managed input and platform sandbox; ownership changes grant no additional network/source permission. Bounded I/O is complementary to process containment. |
| [OWASP authorization](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html) | Keep current request authorization and object/profile checks before delivery; lifecycle capacity is not viewing permission. Do not bypass existing access, generation or source-context validation. |
| [WCAG 2.2](https://www.w3.org/TR/wcag/), [status messages](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html?trk=article-ssr-frontend-pulse_little-text-block), [error identification](https://www.w3.org/WAI/WCAG22/Understanding/error-identification) | Preserve client behavior in this server increment. Later admission/expiry UI must expose meaningful status and errors without unnecessary focus theft or repetitive alerts. Backend lifecycle tests do not establish UI or screen-reader conformance. |

## Alternatives and recommendation

| Approach | Pros | Cons | Decision |
|---|---|---|---|
| Focused VOD lifecycle service with generation/reader ownership and retained completion | Covers preparation, child exit, outstanding I/O, queued bytes and cache retirement while retaining existing client contracts. | Requires coordinated manager/delivery refactoring and real race/physical-cleanup evidence. | Selected bounded increment. |
| Track only existing worker actors at root | Smaller patch; improves child termination. | Misses preparing starts, direct readers, copied HLS data and completed cache ownership. | Insufficient for the confirmed output contract. |
| Drop handles or sweep old directories | Simple apparent cleanup. | Cannot prove process/I/O completion, loses retry evidence and can remove resources still in use. | Not a release or shutdown authority. |
| Materialize all HLS segments and retain Body::from | Minimal internal API changes. | Whole-segment memory and copied-body cancellation remain outside the owner. | Stream segments with bounded queues instead. |
| Enable viewer admission and migrate every client now | Could deliver end-to-end caps once fully qualified. | Combines worker ownership, identity/replay, source transitions, direct/HLS authorization, every client and retention in one increment. | Deferred by confirmed choice. |

Recommended stack: retain Rust/Cargo 1.99.1, Axum/Hyper, existing HTTP body types, Tokio 1.52.3, Tokio-util 0.7.18 and tokio-process-tools 0.11.2. Use focused internal modules for admission, generations, retained commands, file I/O, bounded queues/body polling, cleanup and root completion. Extend the existing process supervisor and platform sandbox. Add no external coordinator, new dependency or broad singleton.

## Complete contract for shared-understanding confirmation

### Ownership and scope

Create a per-manager/AppState internal VOD lifecycle owner. This is not a new HTTP domain: avoid empty handlers or a single oversized service file. Wire normal startup and root shutdown directly; no generic JSON switch or qualification-only production bypass. Keep existing playback request/response payloads, source/recovery contracts and logical play IDs. New internal identities, where needed, use UUIDv7.

Register a start/replacement command before its first owned preparation await. Synchronize close with command, generation and response registration. A closed TaskTracker is not the admission gate. Retain accepted work until it settles, even when a caller disconnects. A cancelled or unpublishable pending start must observe process/cache cleanup; it must not detach cleanup or launch an unowned fallback after root closure. Existing hardware/software retry classification, readiness, watchdog and physical encoder rules remain authoritative.

Use a separate generation lifetime for preparing/running/completed/quarantined output. Natural encoding completion releases its encoder permit while retaining completed output and its generation slot. Pending, completed and cleanup-uncertain generations all consume the 128-generation budget until their owned work and output are actually gone. Do not manufacture analytics/history rows or viewer reservations.

### Readers and response bodies

Cover direct-file and all three HLS response families, including initial file open, metadata, seek, playlist parsing/normalization, file reads, body queues and actual completion/destructors. Root cancellation must close a reader even if its response body is unpolled or its client stops reading. A body Drop cancels its own reader without cancelling another independent output.

Use retained file-owning actors or an equivalent explicitly joined mechanism with 128 nonqueued active-response slots, eight concurrent delivery I/O operations, 64-KiB chunks and a two-chunk application queue per response. Waiting for bounded I/O capacity remains within already accepted response ownership; there is no unbounded admission queue. Backpressure must not hold an I/O permit merely while waiting for a consumer to poll. Bound both source and normalized HLS playlist evidence to 1 MiB; stream media segments rather than imposing an arbitrary whole-segment memory allocation or new segment-size ceiling.

Cancellation discards remaining application-owned queued bytes and wakes blocked/unpolled body state. If response content was not completely emitted, report an interrupted body, not a forged successful EOF. Preserve existing successful status, range/416 behavior, Content-Length, MIME and no-store posture. Existing auth/object/profile checks happen before output admission; a lifecycle token is never viewing permission. Preserve path/rendition restrictions and safe regular-file/owned-cache confinement. Do not introduce arbitrary paths, symlink following in owned cache, source networking or broader FFmpeg protocols.

Do not equate dropping an async file operation with physical cleanup. Await outstanding work, close the actual owned handle and retain failure evidence. Permanently stalled filesystem/transport work may exhaust the unchanged root deadline; report failure instead of claiming it was interrupted or recalled.

The current direct route has no play-session identity. Its readers become root-owned, but ordinary Stop does not acquire a new ability to correlate that route to a viewer. Session-bound direct delivery, ongoing viewing revocation and shared caps remain part of the later protocol migration.

### Retirement, retries and root

Retirement closes a generation's read admission, cancels its accepted readers and waits for owned I/O/application-queue completion before cache deletion. Seek/recovery preserve one logical play identity and cannot attach an unowned replacement. Preserve frozen Stop history: committed history replay and physical-cleanup retry remain separate, including cleanup after profile change/deletion. Concurrent cleanup observers share owned completion; retry after a reported deletion failure must attempt fresh cleanup rather than replaying a stale error forever.

Unconfirmed process/I/O completion, panic/lost completion or failed deletion never frees ownership based on a timeout, map removal, orphan sweep or dropped observer. Quarantine the affected generation and keep explicit cleanup retry possible; healthy independent work can continue within finite budgets. Verified retry may resolve a recoverable cleanup failure. Lost ownership/completion cannot be treated as proof, and root cannot succeed with any unresolved generation or required task/handle cleanup.

Return authenticated/authorized no-store 503 with Retry-After for lifecycle closure and temporary generation/response capacity exhaustion. Preserve the existing encoder-capacity semantics and all ordinary access refusals. Explicit Stop cleanup remains usable after close; add no automatic client mutation replay, shutdown event or UI behavior. New lifecycle diagnostics expose bounded categories/IDs, not credentials, media paths or raw subprocess output.

Root first closes admission and signals workers/readers, then joins accepted commands, process supervisors, I/O/body ownership and cache retirement alongside HTTP and existing owners. Use the existing shared thirty-second drain and sixty-second pool-close budgets. Do not close database dependencies before required joins succeed or convert cleanup/deadline/second-signal failures into successful exit. Unrelated download/storyboard/probe/subtitle/scheduler jobs retain their separate ownership and are not claimed as fixed.

### Qualification and completion

Use real controlled subprocesses and FFmpeg, genuine regular files/read handles, unpolled/slow/dropped direct and HLS bodies, range and invalid-path cases, preparation/registration/retirement races, actual I/O/cleanup failure and retry, naturally completed output, seek/recovery generations and repeated Stop history. Prove no late admission after close, no premature resource-slot release and no cache deletion while its accepted reader remains owned.

Exercise actual production HTTP routes with disposable PostgreSQL and owned local media, plus root ordering, real deadline/failure and executable signal cases. Preserve the Linux managed launch/sandbox and native Windows process ownership; report precise platform evidence rather than inferring runtime behavior from compilation. Reuse existing source/recovery, Stop/profile, admission and root regressions, unchanged memory/container/pool/worker guards and locked dependencies. Run heavy checks serially. Add no production test route, network escape, private source fixture content or release.

Record corrected attempts, exact qualified source, receipts and limits. Run formatting, copyright/legal headers, whitespace, link and private-value exclusion checks including newly staged files. Commit and push all task changes on main without a release. The next item remains coordinated production VOD admission/delivery/client migration and bounded replay-receipt retention before Live TV media output/play actions.

## Baseline and review gate

Before implementation, guarded `cargo-test --lib services::transcoding::lifecycle` passes 17 cases with one existing real-FFmpeg fixture ignore, 992 filtered cases and no failures in 6.77 seconds. Receipt: `.cache/testing-memory/2026-10-26T19-33-10-077Z-ea3946da-d55f-4d84-94bf-2541b8d41b73.jsonl`. This is existing worker regression, not new VOD root/output qualification.

All six choices are confirmed. The complete contract above still requires the [review skill](../../.agents/skills/grilling/SKILL.md)'s final shared-understanding confirmation: “Do not act on it until the user confirms you have reached a shared understanding.” Implementation follows that confirmation. Approved standard legal headers remain permitted; explanatory code comments remain excluded.

## Random PR and Dependabot cleanup

The [maintenance outcome](../operations/DEPENDABOT_MAINTENANCE.md) records singleton random selection of open PR 5, inspection of both exact patches/current lockfiles and verified closure without merge. No applicable code patch or rebase was needed; no downgrade or future-update suppression was performed.
