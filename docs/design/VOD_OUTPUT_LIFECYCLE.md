# VOD worker and output lifecycle

## Start preparation and shared-client successor — October 28, 2026

The qualified [Start-settlement successor](VOD_START_SETTLEMENT.md) binds preparation before inspection and observes actual blocking filesystem completion under this existing output/I/O/root owner. Withdrawal, observer loss and timeouts do not prove physical cleanup. The [shared browser/Tauri player](VOD_CLIENT_MIGRATION.md) now adopts immutable session-bound output, preserves explicit user intent and waits for conclusive settlement. Limits and normal production registration remain unchanged; other online clients and legacy delivery closure still precede production caps.

## Same-viewer output revisions — October 27, 2026

The confirmed [reconfiguration implementation](VOD_RECONFIGURATION.md) adds one retained viewer owner with immutable direct/HLS output revisions, cleanup before replacement, revision-specific URLs/read groups, and explicit recovery/correction without another viewer/history row. Stop and root closure cancel every current/pending revision. Process cancellation and reader draining begin together; physical-generation ownership still prevents cache deletion before its I/O completes. Failed cleanup retains uncertainty, ownership and retry rather than freeing capacity.

Successful encoding remains playable. A version-two failed worker leaves a counted, unavailable viewer after old-output cleanup, eligible for explicit recovery within its lease. This supersedes the first command lane's failed-worker viewer retirement for version two. All output/read/cache/deadline limits remain unchanged; the linked authority owns current qualification and rollout limits.

## Session-bound command integration — October 27, 2026

The following [VOD command/delivery increment](VOD_COMMAND_DELIVERY.md) adds logical viewer groups without consuming additional physical-generation permits. Direct readers register with their viewer; HLS readers register with both the viewer and physical generation. Scoped retirement discards queued output and joins actual I/O before process/cache cleanup and counted ledger release. Successful encoding retains playable output; physical failure/retirement cancels its bound viewer. Existing legacy clients and root/resource budgets retain this document's boundary. The command authority owns the new exact-session protocol and its final qualification; production mounting and client/cap migration remain separate.

## Status and authority

Scoped implementation and qualification complete, October 26, 2026, on main. Research began against `3127055`; all six choices and the complete implementation contract were confirmed before coding. The verification record below binds the qualified working tree and runtime image. Main only; no release.

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

## Implementation notes

Normal builds now share one internal owner per TranscodeManager. Focused owner/generation/command/read/I/O/queue/body modules retain accepted work; root joins it alongside HTTP under the existing budgets. Starts reserve generation and command capacity atomically with retained task registration. Replacement commands keep old retirement and new preparation owned through observer loss. The existing supervisor retains retry sequencing, physical encoder permits and completed output; reader retirement precedes cache deletion. The unused map-based orphan sweep is removed rather than treating missing in-memory metadata as cleanup authority for an old instance.

The worker control record retains encoder capacity through unexpected supervisor exit; the actor releases it only after confirmed physical completion. A destructor guard irreversibly retires the generation on lost task completion. Test-only abort handles exercise actual task loss with a real child, independently verify child exit and confirm that the uncertain slot and cache remain quarantined. There is no production abort hook or cleanup-proof override. Retained direct preparation captures only Range fields; HLS playlists retain explicit byte lengths before streaming.

Startup filesystem inspection is retained separately from its caller's timeout. Linux bootstrap observation uses a nonblocking AsyncFd; closing that observer does not leave a blocking filesystem operation. Source-origin inspection also remains owned through timeout. Delivery actors finish outstanding operations and convert Tokio files to standard handles asynchronously before dropping them. Unpolled bodies cannot prevent root queue discard, while unexpected completion loss quarantines capacity and prevents successful root closure.

An additional finite command budget of 128 prevents repeated cleanup observers from accumulating unbounded retained tasks. Stop remains admitted after closure when a command slot is available; internal start cleanup stays within its already-owned command. These are service bounds rather than household viewer policy.

Native qualification exposed an existing Windows cache-path defect: FFmpeg wrote the initialization file in the process directory for a backslash-form playlist path. The [FFmpeg 8.1 HLS source](https://www.ffmpeg.org/doxygen/8.1/hlsenc_8c_source.html), discovered and opened through MCP, confirms that fMP4 setup derives that directory using the last forward slash. Windows HLS output arguments now use forward slashes and normal DOS/UNC spelling; Linux arguments retain their original spelling. The existing owned-cache checks remain authoritative. The failed fixture artifact was retained only under ignored diagnostic storage.

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

All six choices and the complete contract above were confirmed before implementation under the [review skill](../../.agents/skills/grilling/SKILL.md). Approved standard legal headers remain permitted; explanatory code comments remain excluded.

## Random PR and Dependabot cleanup

The [maintenance outcome](../operations/DEPENDABOT_MAINTENANCE.md) records singleton random selection of open PR 5, inspection of both exact patches/current lockfiles and verified closure without merge. No applicable code patch or rebase was needed; no downgrade or future-update suppression was performed.

## Verification outcome

Verified implementation commit: `28f3f9c` on main. Documentation-only successors preserve the qualified source inventory below.

Final managed Linux source inventory: `62a96b2f740e40fbf627e5ef982cf3d6cd464f02a5d674243bb825879b944d20`. Immutable amd64 runtime: `sha256:7caeabf5f2a81517bb81b4b13a27d507ada3c0d0ae36d37cbbc331133c9e1909`, based on `sha256:f45eda1c8bbe3a9685196aae1200543601ec5efc2fb0f858ca863dd9d42ae8dc`. Ignored local evidence contains no publishable private feed values.

| Qualification | Result and local evidence |
|---|---|
| Default Rust library | 1,014 passed, nine existing fixture ignores, 70.81 seconds. `library-final.txt`; memory receipt `2026-10-26T21-56-49-355Z-56f2a321-0cad-4ebf-b3e0-2e9de8ec9c8e.jsonl`. |
| Native Windows VOD | Eight groups / eighteen tests passed, including real FFmpeg completion/cancellation, actual worker-task loss, manager seek/recovery, authenticated HLS/PostgreSQL, held-file/root deadline and Stop history/profile serialization. Resource `b1885d20-1f87-4e83-8fe4-1d11d10f6cbc`; receipt `2026-10-26T21-36-17-757Z-475a9e73-8d8b-4c8e-86a4-d68e2e0aa32a.jsonl`. |
| Managed Linux VOD | Six fresh restricted library groups / seventeen tests plus the actual Stop/recovery integration test passed. Root observed its real thirty-second refusal before successful retry; integration passed in 42.76 seconds. Resource `898a0b6e-1c0b-41cd-b39e-7c635a5e2705`; receipt `2026-10-26T22-30-21-827Z-f3ca0cae-ffea-47b4-add6-edc184af1ae3.jsonl`. Owned cleanup passed. |
| Root HTTP/TLS/process regression | Three passed in 247.96 seconds, including actual HTTP/pool deadlines, connected clients, second signals and pool ordering. Receipt `2026-10-26T20-51-46-043Z-b0b6b089-86f1-48b6-9e84-18efc8aad1b9.jsonl`. Later worker-guard/header refinements did not alter this runtime module. |
| Final all-target feature lint | Diagnostic completed with existing 28 library / 32 library-test warnings and two existing Tonight navigation casts; no introduced lifecycle warning. Strict lint remains blocked by that baseline, without new suppressions. Diagnostic receipt `2026-10-26T22-28-43-864Z-ba90fcda-8c17-4df0-8378-cb67a5ea5e09.jsonl`; strict receipt `2026-10-26T22-33-01-614Z-b1c3755d-e198-492a-b6f2-2537075a2ad1.jsonl`. |

Linux compilation retains two Cargo jobs, a thirty-minute deadline and 32-MiB logs. Runtime retains four GiB, two CPUs, 256 PIDs, read-only root, unprivileged identity, dropped capabilities, no-new-privileges and bounded tmpfs. Guest preflight requires 5.5 GiB available; active reserve remains 1.5 GiB. The VOD lane reuses the existing owned Docker memory probe, avoiding per-sample WSL control-process startup while preserving engine/boot identity, five-second metadata timeout and ten-second freshness. No guard or guest allocation was changed.

Corrected attempts remain failed evidence: an initial reserve refusal; a cold compiler deadline after integration compilation; a WSL metadata timeout; another reserve refusal; and successful compilation followed by runtime isolation refusal. The last refusal exposed inherited data/cache volumes without bounded overlays in the new library runner. Explicit tmpfs overlays now retain the strict isolation check. Native attempts exposed the Windows HLS separator defect and premature destructor-guard capture; both were corrected and exercised by final normal/aborted-worker tests. Intentional panic fixtures verify retained failure ownership.

Classifarr was temporarily stopped only under explicit user authorization, with exact container/image checks and restoration in a finally block. It was restored running and healthy after each handoff. Harmoniarr and unrelated services were not stopped. The final retry reused completed compilation, while every runtime group executed afresh. Media, database credentials and supplied private feed values remain ignored.

Final workflow regression passes twenty-six cases with two existing skips; receipt `2026-10-26T22-34-21-318Z-20b61449-51ac-46e8-ab18-97badbfe1297.jsonl`. Formatting and staged whitespace pass. Copyright checks include all newly staged sources: 901 files pass. The staged publication check covers 51 task files and 685 local Markdown links with zero missing targets or supplied-private-value matches. Recomputed compiler/qualification source inventory still matches the final Linux receipt; documentation-only changes do not replace that evidence.

## Next item

The user's clarification requests Linux build/qualification durability across retries. [Linux build durability](../ci/LINUX_BUILD_DURABILITY.md) records the confirmed checkpoint/cache implementation and its separate successful interruption/retry and runtime qualification. After that focused tooling work, coordinate VOD admission, direct/HLS delivery and every online client's explicit start/renewal/recovery/Stop outcomes, with bounded terminal-receipt retention, before enabling shared viewer caps or Live TV playback. Browser presentation, ARM64 execution, physical GPU failure, unrelated background jobs and full filesystem fault portability are not established by this increment.
