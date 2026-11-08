# Android ambient playback command migration

## Status and scope

Research and decision checkpoint, November 8, 2026. Q1–Q8 below are confirmed; Q9's additional private audio/codec cleanup boundary is awaiting an answer. The complete implementation contract still requires shared-understanding confirmation under [CONTEXT.md](../../CONTEXT.md#design-review-with-grill-me). No application implementation or new runtime qualification is claimed by this document.

This follows the [qualified Android relay increment](../ci/ANDROID_RELAY_QUALIFICATION.md). Ambient means the existing curated local-media channels, including foreground video and background audio. It does not mean IPTV channels, provider VOD, recordings or audio-only media. The [existing ambient authority](PROFILES_AND_AMBIENT_CHANNELS.md) continues to govern eligibility and exclusion from personal history.

Related documents own [native transport security](../security/ANDROID_AMBIENT_TRANSPORT.md) and [qualification](../ci/ANDROID_AMBIENT_QUALIFICATION.md). The existing [command](VOD_COMMAND_DELIVERY.md), [reconfiguration](VOD_RECONFIGURATION.md), [Start settlement](VOD_START_SETTLEMENT.md) and [receipt retention](PLAYBACK_RECEIPT_RETENTION.md) authorities remain unchanged.

## Evidence from the current implementation

| Boundary | Finding | Required change |
| --- | --- | --- |
| Native service | `AmbientPlaybackService.kt` owns one ExoPlayer/MediaSession but uses legacy Start, heartbeat and Stop. | A composable native protocol-two owner, independent of the Flutter screen/engine. |
| Acceptance | The bridge returns success immediately after service dispatch. | Separate local command acceptance, server acceptance, output readiness and cleanup completion. |
| Stop and advancement | Runtime/session identity is cleared before best-effort Stop; failed Stop is ignored before the next item starts. | Retain exact ownership and wait for native output and server settlement before advancing. |
| Renewal | A fifteen-second heartbeat shares an unbounded single-thread executor with thirty-second network requests. | Independent single-flight renewal and bounded admission, including while paused/preparing. |
| Transport | Bearer defaults are supplied to the generic Media3 source; absolute returned URLs are accepted. JSON/error bodies are unbounded. | Exact-server native transport, bounded responses and strict generated-HLS validation. |
| Scope | Runtime lacks exact session/profile/credential revisions; late failure callbacks can act on newer runtime. | Authoritative session context and epoch-bound callbacks, commands and output. |
| UI | Status reads can overlap; failures are ignored; Stop navigates away in `finally`. | Truthful preparing/paused/uncertain/stopping states and explicit reconciliation. |
| Backend | Protocol two already accepts ambient mode/channel revision and rechecks current authority; ambient checkpoints omit personal history. | Reuse these contracts rather than creating a new server queue. |
| Coverage | Ambient tests currently mock three bridge behaviors. | Real Android/background/HTTP/PostgreSQL/FFmpeg and ownership-fault qualification. |

## Official research and October 2026 baseline

Sources were discovered and read through MCP search/GitHub on November 8. The design uses platform contracts available by October 31, 2026 and verifies version-sensitive Media3 APIs against tagged 1.10.1, commit `5fb306449733dd71595700c1227ad6087578c559`. Living documentation is not represented as an archived October snapshot. Later platform/API additions are not prerequisites.

| Source | Finding and application |
| --- | --- |
| [Android background playback](https://developer.android.com/media/media3/session/background-playback) | Native media service ownership survives activity navigation; task dismissal, paused-service lifetime and resumption require deliberate handling. Keep resumption disabled after service/process death. |
| [Android audio focus](https://developer.android.com/media/optimize/audio-focus?authuser=131) | A media foreground service is relevant to background focus eligibility on modern Android. Audio focus is distinct from a counted server viewer or confirmed cleanup. |
| [Flutter lifecycle](https://api.flutter.dev/flutter/dart-ui/AppLifecycleState.html) | Lifecycle callbacks are not guaranteed before termination. Do not depend on a final Dart callback for Stop or receipt persistence. |
| [Media3 1.10.1 HLS factory](https://github.com/androidx/media/blob/1.10.1/libraries/exoplayer_hls/src/main/java/androidx/media3/exoplayer/hls/HlsMediaSource.java) | Public data-source/parser/executor injection supports a bounded native owner without a Dart background relay. |
| [Media3 release timeout](https://developer.android.com/reference/androidx/media3/exoplayer/ExoPlayer.Builder#setReleaseTimeoutMs(long)) | Release can report timeout. A returned release call cannot erase retained work or establish healthy cleanup by itself. |
| [AtomicFile](https://developer.android.com/reference/android/util/AtomicFile.html?authuser=6), [no-backup app directory](https://developer.android.com/reference/android/content/Context) | The platform supplies atomic file replacement; the caller must serialize access. Keep the non-secret receipt in private, backup-excluded storage. |
| [W3C status messages](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html?trk=article-ssr-frontend-pulse_little-text-block), [focus order](https://www.w3.org/WAI/WCAG22/Understanding/focus-order.html) | Expose status and control state through accessibility semantics; preserve meaningful focus when controls change. Native Flutter semantics implement these outcomes without adding HTML/ARIA to Android. |
| [Flutter accessibility testing](https://docs.flutter.dev/ui/accessibility/accessibility-testing) | Test control labels, target sizes, contrast and semantics, while distinguishing automated checks from spoken screen-reader evidence. |
| [Media3 release history](https://github.com/androidx/media/blob/release/RELEASENOTES.md) | Newer releases exist. Retaining 1.10.1 here isolates lifecycle work; a coordinated dependency upgrade needs its own compatibility evidence. |

## Confirmed decisions

1. **Q1:** Native Android Media3 owns protocol-two commands, renewal and cleanup while Flutter is closed. Keep foreground video/background audio, independent interactive/ambient lifetimes, no personal history and production attachment/caps off. iOS/offline remain separate.
2. **Q2:** Natural EOF advances automatically only after both native output and server viewer are definitively released. Unknown/failed cleanup pauses the channel for explicit reconciliation. Channel replacement uses the same boundary.
3. **Q3:** One independent ambient journal of at most 4 KiB stores command IDs and exact scope identifiers only. Restart offers authenticated reconciliation/Stop, never automatic playback resumption. No credentials, stream URLs or media metadata are persisted.
4. **Q4:** Active playback continues through Home, screen lock and Recent-app dismissal under the media foreground service. Paused/stopped/failed states obey Android service rules. Death, reboot or lost lease never triggers automatic restart.
5. **Q5:** First controls are Play/Pause/Stop and same-item seek through one owner, including notification/headset control. Track/quality pickers and automatic worker/decoder recovery remain separate. Failure detaches output and requires explicit action. Unknown controllers have no playback access; supplied media URLs are never accepted.
6. **Q6:** Unknown local cleanup retains an ambient-only barrier across service/Flutter recreation and account/server changes. Login/navigation and the independently owned interactive player remain usable. Recreation is not release.
7. **Q7:** First delivery is fully encoded HLS, H.264/AAC, SDR, stereo, up to 1080p, requiring transcode permission, viewer capacity and encoder capacity. Direct/copied video and broader formats are separate.
8. **Q8:** Use owned native HTTP and a small Media3 data source with exact-server/output checks, no redirects, ordinary TLS and finite budgets. Join owned public requests/streams/loaders/playback work; do not claim physical socket destruction or termination of private Android HTTP/TLS pooling.

**Q9 pending:** tagged Media3 also schedules private asynchronous AudioTrack/codec cleanup without a public join. The proposed guarantee requires successful public ExoPlayer release without timeout/error and joined application-owned work, excluding private audio/codec as well as HTTP/TLS housekeeping. Release callbacks are supporting observations, not proof that all private workers have exited. This refinement must be explicitly confirmed before implementation.

## Alternatives and recommendation stack

| Option | Pros | Cons | Recommendation |
| --- | --- | --- | --- |
| Flutter-owned commands/relay during ambient playback | Reuses qualified Dart code | Requires a new qualified background-engine lifetime; activity callbacks cannot establish it | Do not select for this native owner |
| Generic background-media package or new HTTP dependency | Potentially less custom platform code | New dependency/lifecycle boundary and migration work; does not establish exact-source authority | Keep outside this increment |
| Modular native Media3 owner with injected HTTP/HLS work | Matches current background architecture; public APIs expose required ownership hooks | Native protocol implementation and public-operation cleanup need separate adversarial evidence | Selected |
| Default Media3 HTTP source with bearer defaults | Small integration | No all-redirects-off factory control; error buffering and URI acceptance do not meet this lane | Replace for the new lane |
| New raw-socket HTTP parser | Could expose owned TCP handles | Adds HTTP parsing and TLS/cancellation complexity to obtain a stronger guarantee | Defer; retain the precise Q8 guarantee |
| Native app-private atomic non-secret journal | No plugin-native coupling or new encryption code; explicit serialized I/O | Scope identifiers remain plaintext within the application sandbox; not a credential vault or arbitrary-power-loss guarantee | Proposed storage mechanism for Q3 |

Final recommended stack: existing Rust protocol two/PostgreSQL owner; locked Kotlin/JDK 17 and Media3 1.10.1; native bounded HTTP plus public HLS/parser/executor injection; one app-lifetime ambient coordinator and private atomic receipt; thin Flutter bridge and accessible existing screens; real isolated Android/backend qualification. Explicitly declare the already-resolved Media3 HLS module at 1.10.1 for direct imports, without changing the resolved graph. Keep Flutter/Dart, secure-storage, player-plugin and SDK versions unchanged.

## Proposed implementation contract

### Identity, negotiation and queue ownership

Fetch the actual authenticated `/api/v1/auth/session-context` before initial admission and after reconciliation boundaries. Capture exact origin/account/session/credential revision/profile/profile-context revision/transport and a local epoch. Use the existing `X-Session-Context`, exact ETags and current authority. Scope identifiers and profile lists alone never authorize a request.

Use existing explicit protocol-two acknowledgement and its four required features unchanged; do not append an ambient feature to the interactive parser's exact feature list. Normal production acknowledgement remains off. Before any new-lane Start, a server without acknowledgement may retain the legacy branch only when no ambient journal/local barrier exists. After dispatch or uncertainty, there is no legacy downgrade or automatic command replay. Retained cleanup always takes precedence over capability fallback.

The native channel owns one current viewer and at most one accepted source-changing command, with no user-command queue. Obtain a fresh deterministic `next` selection and exact channel revision; the backend rechecks ambient authority at Start and renewal. Each item is a new complete UUIDv7 Start, using recently observed server time and monotonic elapsed time. Exact identities reconcile existing work without another viewer or lease extension. Do not persist or replay the Start body.

At normal EOF, retire local output, observe server settlement and clear/read-back the journal before selecting/starting the next item. Any failure/uncertainty stops automatic advancement. A definite stale-selection refusal may obtain one fresh selection under the existing bounded policy; do not interpret arbitrary 409/network failure as permission to retry a mutation. Starting another channel requires explicit intent and confirmed cleanup of the previous channel.

Pause preserves the counted viewer and independently renews it. Same-item seek uses a fresh UUIDv7 operation, exact configuration ETag/output revision and confirmed old-output retirement. It preserves admitted item/file, negotiated quality/tracks and viewer identity. No direct ExoPlayer seek may bypass the server operation. Automatic next/previous media-item mutation through controllers is unavailable; queue advancement belongs to this owner.

### Journal and scope

Use one fixed ambient-only file beneath `noBackupFilesDir`, accessed by a single off-main I/O owner. Serialize a strict versioned record containing UUIDs, bounded phase and exact scope/fingerprint only, at most 4,096 UTF-8 bytes. Enforce the bound before allocation and during reads. Platform replacement/backup files are a bounded implementation overhead, not additional logical receipts. Use explicit synchronization/error handling plus bounded read-back; retain actual I/O completion beyond its observer. No `readFully`/unbounded JSON read or generic reset-on-error.

The interactive secure-storage journal remains independent and unchanged. The ambient file needs no secret encryption scheme: it contains no credential, URL, request body, title, position or queue catalog. Its IDs do not grant server authority. Corruption, unknown version, failed write/clear or unresolved I/O blocks new ambient playback; it is not silently erased to make room. Atomic replacement/read-back does not promise survival of arbitrary hardware failure or power loss. Retain only an opaque origin fingerprint, using a freshly authenticated origin for reconciliation; no server URL is persisted.

Persist the exact request identity before Start/seek can dispatch. Mark possible dispatch conservatively; only a proven-unsent intent may clear after pending I/O settles. A profile-only change immediately detaches output and retains cleanup-only identity when the same account/session/credential can still Stop. Stronger account/server/session/credential changes clear local recovery state after owned storage settles and rely on server revocation/lease retirement, while retaining unfinished local work. No further old-credential request is dispatched after the stronger boundary; already transmitted bytes cannot be recalled. A cold app must authenticate and verify the actual current context before offering status/cleanup.

### Lifecycle and controls

Keep the owner at application lifetime, independent of service/view wrappers. The bridge returns structured acceptance/status and never equates local dispatch with server acceptance or release. Fence native work synchronously when Flutter's authoritative scope changes, before exposing new scoped content; do not require login/navigation to await a network Stop. Late success/error/disposal callbacks remain bound to the captured epoch/output.

Native renewal runs independently during preparation, pause, seek and background playback. Preserve the fifteen-second single-flight cadence and sixty-second server deadline. Failed/uncertain/expired renewal detaches output, stops advancement and requires explicit status reconciliation; status/checkpoint do not extend the lease. Observe monotonic time across device suspension conservatively.

Use diagnostic revision-bound increasing-sequence checkpoints. Preserve zero personal-history/resume/play-count/Trakt/recommendation mutation. On Stop, a bounded final checkpoint attempt must not prevent cleanup. For a possibly dispatched Start, conclusive withdrawal fences that same UUID and waits for server settlement. A missing receipt, timeout, disconnected observer or returned platform call is not settled ownership.

Route MediaSession, PlayerView and Flutter controls through the same owner. Restrict controllers to the app and explicitly trusted system/media-control identities with an allowlisted command set. Do not expose credentials, local configuration URIs or arbitrary media-item injection. Preserve Android audio focus without treating focus loss as viewer release or an implicit handoff to/from interactive playback.

Playing stays background-capable when the screen detaches; service/task callbacks never create a new Start. Paused/service timeout/death cannot claim confirmed server Stop. Stop admission, cancel public work and retain cleanup on destruction; do not enqueue Stop and immediately discard its executor. App-process death ends local ownership, but the durable receipt and backend lease/settlement remain authoritative after restart.

### Modules and UI

Split Kotlin protocol schema/context/clock, retained operations, command/status, lease/checkpoint, journal, queue, output/worker ownership, transport/HLS validation and MediaSession controller adapter into focused service files. Keep `AmbientPlaybackService` a lifecycle adapter. Compose through an app-lifetime owner rather than another large service singleton. Keep Dart scope binding/bridge/status and channel/player widgets small and independently testable.

The visible screen distinguishes preparing, playing, paused, changing, checking, stopping, settled, unavailable and cleanup-uncertain. Show exact permitted actions: Play/Pause/seek for confirmed current output; Check status/Stop for uncertainty; explicit channel Start after settled failure. Never navigate as though Stop succeeded merely because a `finally` block ran. Screen dismissal may leave active background playback; Stop explicitly requests settlement. Single-flight screen reads, bounded strings and epoch fencing prevent stale UI adoption. Announce meaningful status through semantics without stealing focus or continuously reading position updates.

### Budgets and acceptance

The linked transport and qualification documents define finite new ambient-client budgets explicitly; they do not enlarge existing interactive/backend/root/host limits. One ambient and one interactive viewer remain independent. No new queue, retry framework, production capability advertisement or server queue worker is introduced.

Implementation is accepted only after meaningful native/storage/transport/control tests, actual ambient background/seek/advancement/uncertainty/scope/concurrency journeys, preserved original Android regressions, exact zero ambient personal-history mutation and confirmed owned cleanup. Document every failed attempt separately and retire newly owned disposable artifacts. Physical-device, private SDK housekeeping and broader format claims stay limited to the evidence actually obtained.

## Current outcome and next step

Eight product/ownership decisions are confirmed; the discovered Q9 cleanup refinement, complete-contract confirmation and implementation are pending. Source exploration and official research are complete for this proposal. The open Dependabot patch is tracked separately in [its review](../ci/SVELTEKIT_3_UPGRADE_REVIEW.md). This checkpoint does not change application behavior, dependencies, SDKs, OS configuration or test limits and creates no release.

Next: confirm this complete contract and its linked security/qualification boundaries, then implement and qualify the native Android owner. After that scoped qualification, review remaining platform-player migrations before any all-client production attachment/shared-cap rollout. Track/quality selection, automatic ambient recovery, iOS/offline, broader devices/formats and provider/DVR work stay separate.
