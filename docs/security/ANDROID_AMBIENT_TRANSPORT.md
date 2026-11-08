# Android ambient native transport

## Status and authority

Proposed implementation/security contract, November 8, 2026, linked from [Android ambient migration](../design/VOD_ANDROID_AMBIENT_MIGRATION.md). Confirmed Q8/Q9 select successful public player release plus owned request/stream/storage/worker completion, excluding private Android audio/codec and HTTP/TLS housekeeping. Complete-contract confirmation and runtime qualification remain pending. This document does not claim a production transport fix has shipped.

The qualified [Dart relay](FLUTTER_HLS_RELAY.md) remains interactive. Do not infer its background lifetime or its confirmed raw-TCP closure guarantee for a different Kotlin transport. Native ambient needs no loopback listener or bearer transfer to a separately running player.

## Official evidence

MCP research checks the October 2026-compatible platform APIs and Media3 tag 1.10.1 (`5fb306449733dd71595700c1227ad6087578c559`).

- [DataSource](https://github.com/androidx/media/blob/1.10.1/libraries/datasource/src/main/java/androidx/media3/datasource/DataSource.java) and [HlsDataSourceFactory](https://github.com/androidx/media/blob/1.10.1/libraries/exoplayer_hls/src/main/java/androidx/media3/exoplayer/hls/HlsDataSourceFactory.java) expose native resource admission and streaming.
- [HlsMediaSource](https://github.com/androidx/media/blob/1.10.1/libraries/exoplayer_hls/src/main/java/androidx/media3/exoplayer/hls/HlsMediaSource.java) supports custom parser/loader executors. [ReleasableExecutor](https://github.com/androidx/media/blob/1.10.1/libraries/exoplayer/src/main/java/androidx/media3/exoplayer/util/ReleasableExecutor.java) and [Loader](https://github.com/androidx/media/blob/1.10.1/libraries/exoplayer/src/main/java/androidx/media3/exoplayer/upstream/Loader.java) do not make release notification a task-completion join.
- [DefaultHttpDataSource](https://github.com/androidx/media/blob/1.10.1/libraries/datasource/src/main/java/androidx/media3/datasource/DefaultHttpDataSource.java) is unsuitable for a strict bearer boundary: factory options do not disable all redirects, and error bodies are buffered without this application's limits.
- [HttpURLConnection](https://developer.android.com/reference/java/net/HttpURLConnection) supports per-request redirect refusal and explicit identity coding. Disconnect can return pooled connections; it does not prove TCP destruction. Use ordinary `HttpsURLConnection` certificate/name verification without global trust overrides.
- [ExoPlayer release implementation](https://github.com/androidx/media/blob/1.10.1/libraries/exoplayer/src/main/java/androidx/media3/exoplayer/ExoPlayerImpl.java) and [release timeout API](https://developer.android.com/reference/androidx/media3/exoplayer/ExoPlayer.Builder#setReleaseTimeoutMs(long)) require distinguishing returned release from timeout-free owned completion.
- [AudioTrackAudioOutput](https://github.com/androidx/media/blob/1.10.1/libraries/exoplayer/src/main/java/androidx/media3/exoplayer/audio/AudioTrackAudioOutput.java) schedules private asynchronous audio release; [AsynchronousMediaCodecCallback](https://github.com/androidx/media/blob/1.10.1/libraries/exoplayer/src/main/java/androidx/media3/exoplayer/mediacodec/AsynchronousMediaCodecCallback.java) requests private thread shutdown without a public join. Q9 explicitly confirms this additional physical-completion limit while retaining release-failure quarantine.

## Boundary and trust

Only the captured, authenticated Duskcue server origin is a destination. This is a client connection to its selected server; backend Dispatcharr destination pins remain a separate policy. No provider URL, ambient metadata, arbitrary absolute URL, controller-supplied URI, redirect target or remote artwork can broaden it.

Validate scheme/host/effective port and canonical authority, rejecting user-info, fragments, encoded separators, traversal, malformed/ambiguous paths and unsupported query parameters before dispatch. Permit only explicit API templates for context/selection/preparation/command/status/renewal/checkpoint/withdrawal and exact current viewer/output resources. Construct templates from validated UUIDs rather than trusting response locations. A response Location/ETag must match that expected representation. Validate every HLS reference before publication and again before a later open.

Use a per-request connection with redirects disabled, explicit no-store/identity encoding, finite request bodies and no automatic POST retry. Supply bearer only after target admission. Refuse challenges/redirects rather than invoking a credential discovery flow. Do not install global authenticator, proxy, cookie, hostname-verifier, socket-factory or trust-manager overrides. The isolated qualification CA must never enter production trust or the normal package. Preserve user-selected HTTP/HTTPS deployment policy; HTTPS uses normal platform trust and name checks.

## Proposed finite ambient budgets

These limits apply to the new ambient owner. Existing interactive limits stay unchanged. Simultaneous interactive/ambient playback can therefore own both separately bounded clients; these are public-operation bounds, not a hard process-RSS, native-socket or private-framework-thread ceiling.

| Resource | Bound and ownership |
| --- | --- |
| Ambient lifecycle | One current viewer; one source-changing operation; no pending user-command queue; old output drains before replacement |
| Control HTTP | Three regular operations, with renewal capacity protected from other regular work; two reserved cleanup operations; permits retained through actual completion |
| Logical scheduling | One renewal, status and checkpoint in flight each; checkpoint retains at most one coalesced latest sample; status at most once/second for sixty seconds |
| Control payloads | 32-KiB request/response maximum; checkpoint/withdraw request at most 1 KiB; bounded strict JSON depth/nodes/strings |
| Control/storage observation | Five seconds; timeout cancels observation/public work but retains actual completion and capacity |
| Media requests | Four public requests including opening/cancelling/closing; immediate admission refusal when full; no pending media-request queue |
| HLS loader work | At most four owned loader executors; one active and at most one bounded SDK continuation/release task per executor; executor stays counted through termination |
| Playlist | At most 1 MiB, sixteen renditions, 8,192 references; one qualified selected rendition; no external/key/subtitle fetch expansion |
| Media body | Stream at most 16 MiB per request; at most two 64-KiB application chunks; do not buffer whole segments |
| Media deadlines | Five-second connect/header, ten-second idle, sixty-second body, seventy-five-second whole operation; cancellation retains the owner |
| Player | One current owned playback worker/thread; thirty-second whole attachment observer and five-second ordinary control observer; release timeout remains a failure signal |
| Receipt | One 4-KiB logical non-secret journal; one retained serialized off-main storage operation |

A queued SDK release callback is internal finite cleanup work; it is not permission to queue new playback commands. If the locked SDK requires work beyond these limits, refuse/retain it and report the evidence rather than silently widening admission. Normal automatic SDK network retries are disabled for this lane. Explicit status reads and continued lease renewal are distinct from mutation replay.

## Response and HLS validation

Inspect status, MIME, Content-Encoding, framing evidence exposed by the SDK, Content-Length and range correspondence before returning data. Do not forward raw upstream headers or errors into UI/logs. Bound error bodies too. Read successful unknown-length content to actual EOF within the byte/deadline ceiling; short fixed-length or failed content must remain failed. Reject unsupported multipart/encoding/ranges, excessive lengths, unsafe MIME and bodies attached to refusal responses. SDK-normalized raw header syntax is outside this public API's inspection guarantee; qualification must not claim otherwise.

Read bounded UTF-8 manifests into one immutable candidate, validate the same generated server HLS subset as the qualified lane, then pass validated bytes to Media3's public parser. Reject mixed master/media roles, unsupported tags, malformed/duplicate URI attributes, keys, arbitrary schemes/authorities and unsupported nested resources. Publish an allowlist atomically for the captured viewer/output epoch; a failed parse cannot partially authorize references. Retired manifests/references never reopen after seek or scope change.

First source admission requires actual encoded H.264/AAC stereo SDR HLS within 1080p. No direct/copied-video fallback, provider HLS, DRM, HDR, broad multichannel or decoder-downgrade inference from generic error text. Source/capability checks remain server-owned, with an additional native response/readiness check.

## Cancellation and cleanup

Register ownership before connection/stream/task creation; retain late-created handles after cancellation. Cancel media admission first, detach the visible surface, cancel all active public requests and close their streams/connections. Observe actual open/read/write/close completion, bound loader-executor release/termination and owned playback-thread shutdown. Calls to disconnect, `shutdownNow`, `release`, a listener or a timed-out bridge Future alone are insufficient evidence. Do not abandon a blocking read merely because its observer ended.

Retain separate public-operation and player cleanup failures. A timeout/unconfirmed close quarantines ambient attachment across wrapper recreation. Other app features remain usable within their own limits. Local cleanup cannot confirm server reservation release; the exact Start withdrawal/Stop settlement is independently required before automatic channel advancement. Server settlement likewise cannot erase unknown local work.

The confirmed Q9 guarantee requires successful public ExoPlayer release without timeout/error and excludes private AudioTrack/codec cleanup as well as Android's HTTP connection pool, DNS/TLS machinery and housekeeping that public APIs cannot individually join. Audio/decoder release callbacks provide supporting lifecycle evidence; their absence or presence does not join those private workers. No statement about exact physical socket count, all OS/native worker termination or arbitrary kernel-memory reclamation follows from this owner. Qualification reports owned public completion and observable process/backend evidence with those limits.

## Current outcome

Research establishes feasible public injection points and the precise cleanup boundary accepted in Q8/Q9. Complete-contract confirmation, implementation, adversarial HTTP/TLS/storage/worker tests and actual Android evidence remain pending under [the qualification contract](../ci/ANDROID_AMBIENT_QUALIFICATION.md). No dependency/version/OS or production setting changes are made by this checkpoint.
