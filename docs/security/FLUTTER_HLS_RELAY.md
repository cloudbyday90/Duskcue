# Flutter protocol-two HLS relay

## Current qualification review

The [bootstrap checkpoint](../ci/ANDROID_DRIVER_BOOTSTRAP.md) qualifies both scope journeys and all eight original journeys, including decoded playback and worker recovery. The complete relay adversarial/backpressure/format matrix remains open. [Current research and implementation contract](../ci/ANDROID_RELAY_QUALIFICATION.md) records concrete ownership/parser/MIME and format-evidence gaps; the existing seven relay tests pass but do not establish that missing coverage. All three new design choices are confirmed; complete-contract confirmation and implementation remain pending.

The selected admission policy pauses native acceptance at eight accepted/closing sockets until observed release, replacing the earlier always-immediate excess-socket refusal. Outbound ownership will retain the SDK HTTP parser through a bounded public raw-socket adapter. HTTPS completion means all owned public operations settled plus awaited destruction of returned raw TCP handles; no public API proves a join of the SDK's private TLS filter, and no such claim is made. Normal trust/name verification and all guards remain. This review authorizes no plaintext downgrade or certificate bypass. Until implemented and qualified, source behavior retains the gaps explicitly listed in the successor authority.

## Status and authority

Implementation checkpoint, October 28, 2026. Q7 and the complete contract at `21cd8b1` are confirmed. The existing player now uses modular owned loopback relay services; current unit/socket and APK preparation results are recorded below. Actual Android transport/decoder/backpressure qualification is still pending. Production enablement is not claimed.

The [command/delivery](../design/VOD_COMMAND_DELIVERY.md), [reconfiguration](../design/VOD_RECONFIGURATION.md), [Start settlement](../design/VOD_START_SETTLEMENT.md) and [memory](../design/MEMORY.md) authorities retain their server-side scope. This is a client media-transport boundary, not a provider connector, Dispatcharr replacement, download cache or server authorization override.

## Evidence and recommendation

Locked video_player_android owns a redirect-capable Media3 data source without an application request-policy hook. Cached native code shows default headers can accompany redirected HTTP/HTTPS requests. Current Duskcue output redirects are not alleged. JSON transport configuration cannot control that independent native stack.

The selected relay keeps the server bearer out of the native player. It can enforce captured server/output authority without a player fork or new engine. Its costs are an owned local listener, a deliberately narrow HTTP/HLS subset, streaming/backpressure complexity and actual-device-process qualification. Typed decoder proof remains deferred; a relay does not make generic player errors into decoder evidence.

Official source links were discovered/retrieved with MCP search on October 28: [Android media troubleshooting](https://developer.android.com/media/media3/exoplayer/troubleshooting?hl=en), [Dart HttpServer](https://api.dart.dev/dart-io/HttpServer-class.html), [RawServerSocket](https://api.dart.dev/dart-io/RawServerSocket-class.html), [bounded RawSocket.read](https://api.dart.dev/dart-io/RawSocket/read.html), [HttpClient](https://api.dart.dev/dart-io/HttpClient-class.html) and [automatic decompression](https://api.dart.dev/dart-io/HttpClient/autoUncompress.html). Starting inspection used Dart 3.12.2 and the locked native sources. The operator-updated Dart 3.13.7 passes the current control/socket suite; actual native qualification remains required. The resource numbers below are Duskcue engineering limits, not limits prescribed by those APIs.

## Authority and routing

Bind one non-shared literal `127.0.0.1` listener on an OS-assigned port for the current output. Do not bind wildcard/IPv6/LAN addresses or enable port reuse. Give the player only a loopback URI containing a fresh 256-bit secure-random read capability; give it no server Authorization/Cookie headers. The capability is ephemeral local read authority: never log, persist, restore, embed in a journal or send it upstream. Replacing/retiring output revokes it and closes its listener. It does not authorize any backend operation.

Capture original server origin, exact session/profile/context/credential identity, bearer and output revision. Construct upstream paths from that fixed current viewer/output and validated resource suffixes, never from an arbitrary target query/Host header. Requests use captured credentials and current context, ordinary TLS certificate/name verification, redirects disabled and identity content coding. Reject unsupported response coding before decompression. No certificate bypass, dynamic proxy target, credential adoption from a newer session or redirect fallback is introduced.

Require current owner/scope and held lease before local admission and before forwarding/adopting bytes. Scope/output invalidation, uncertain/expired lease, Close and local owner closure cancel listener admission, upstream requests and queued output. Already transmitted bytes cannot be recalled. Native media GET retries may access only this still-current immutable output; they do not create/replay Start/change commands or renew a lease. Normal server-side delivery checks remain authoritative on every request.

## Chosen HTTP and HLS subset

Use socket-level acceptance and bounded reads, retaining owned raw sockets and completion. Plain HttpServer.bind callback counters are insufficient: incomplete headers are parsed before callbacks. Implement small separate admission/parser/writer services for one HTTP/1.1 GET per connection, with Connection: close. Use RawServerSocket/RawSocket.read(n) for chosen inbound allocation limits; this is not a general-purpose HTTP server.

Require the exact loopback Host/port, capability and canonical current resource path. Reject absolute-form targets, user information, fragments/queries, percent-encoded aliases, traversal, duplicate security/framing fields, request bodies, Transfer-Encoding, pipelining, unsupported versions/methods and inbound Authorization/Cookie. Permit at most one syntactically valid bounded byte Range for initialization/media; validate successful partial response attribution before forwarding. Do not forward arbitrary player headers upstream. Reject invalid requests without contacting the selected server.

Accept only UTF-8 generated master/media playlists and the existing server subset: bounded rendition names; initialization `init.mp4`; numeric `seg_*.m4s`; exact current viewer/output playlist/segment paths. Rewrite permitted references to the capability-protected local namespace. URI-bearing media tags are limited to the server's supported EXT-X-MAP form. Reject external/different-generation references, keys/encrypted-HLS, unsupported URI tags and malformed playlists. Validate the complete bounded playlist before exposing it. Keep only bounded current reference metadata, with no playlist history or media-body cache.

Only allowlisted successful media headers/status are forwarded; do not expose upstream cookies, redirects, authentication headers or arbitrary diagnostics. Error responses contain fixed safe status text, no upstream paths/capabilities/tokens. Temporary admission exhaustion uses no-store 503/Retry-After. Upstream/control failure is not a new-Start instruction or evidence of release.

## Initial resource and deadline contract

| Resource | Bound |
|---|---|
| Output ownership | One current relay/native owner and at most one retiring owner retained for completion; no overlapping replacement attachment or queue of generations. |
| Accepted sockets | Eight accepted/closing sockets total, including incomplete headers; the confirmed successor pauses native acceptance while full and resumes after observed release. Excess peers can wait in the finite OS backlog, which is not counted as accepted application ownership. Implementation/qualification is pending in the successor authority. |
| Local request framing | Eight KiB total header block, 2,048-byte request target, one request/connection, no body; absolute five-second header deadline. |
| Upstream media requests | Four active/connecting/closing requests; explicit admission before HttpClient invocation, no pending-request queue. Caller permits remain held through completion. |
| Playlist/reference state | One MiB per validated playlist, at most sixteen rendition entries and 8,192 resource references for the current output. No unbounded refresh/history accumulation. |
| Initialization/media body | At most sixteen MiB per response, streamed and counted; no whole-segment materialization. |
| Application write queues | At most two 64-KiB chunks per response; finite raw-socket reads and writes with observed backpressure. |
| Media observation | Five-second upstream connection/header observation; ten-second body/write idle deadline, sixty-second body deadline and seventy-five-second whole-request deadline. |
| Control/metadata | Existing thirty-two-KiB JSON, one-KiB checkpoint/withdrawal and five-second total observation bounds; separate from media transfer. |

Application queue limits do not claim an exact process-RSS ceiling. Upstream Dart HttpClient has SDK/TLS/socket buffers and a fixed one-MiB header parser; retain at most one yielded SDK body chunk per admitted request, pause consumption while writing, and measure native/Dart peaks. It must not create an unbounded internal request backlog: maxConnectionsPerHost alone queues excess requests. The process guard and measured native fixture complement, rather than replace, explicit caller limits.

A deadline ends observation and requests cancellation. It never releases a permit or forgets a pending native/socket/HTTP operation whose actual completion is unknown. Close/cancel remains idempotent and retained. Unknown disposal/completion quarantines the current process owner and blocks new attachment while server Stop/status still proceeds. Do not erase a failure because an observer disconnected or a timer fired.

## Qualification and outcome

Unit/socket tests must cover incomplete/oversized headers, concurrent acceptance/full admission, exact Host/capability, duplicate/framing/Range errors, pipelining/traversal/encoded aliases, no-body/unsupported methods, absent bearer/Cookie, malicious/mixed-generation manifests, external redirects and redirect canaries, unsupported coding, oversized bodies, slow readers, partial writes, cancellation, EOF and failures before/after accepted upstream work. Prove no request reaches an unapproved origin and no permit is released merely from timeout.

Actual [Android qualification](../ci/ANDROID_PLAYBACK_QUALIFICATION.md) must show native Media3 consumes only bearer-free loopback resources while relay requests authenticate to the real controlled backend. Bind output revisions, source position and scope changes to native playback; confirm listener/socket/controller cleanup and zero owned emulator/ADB/test-server processes. Fixture trust and fault injection stay qualification-only.

The concrete contract is confirmed. Modular request/parser, playlist, raw-socket connection, upstream and output-owner services are implemented. The current Flutter suite passes 52 tests, including real socket/HTTP authorization, wrong-output references, redirect canaries, coding/range refusal and incomplete-header closure. This is an initial implementation checkpoint; the complete adversarial matrix, measured native backpressure and actual emulator qualification remain required. Precise source/tool/app identities, corrected failures and limitations will be added here after execution. No claim covers direct/copied-video, encrypted HLS, arbitrary providers, other mobile platforms or production-cap rollout.

## Implementation checkpoint

The current [Flutter/Dart toolchain](../ci/FLUTTER_TOOLCHAIN.md) is operator-updated to 3.47.8/3.13.7. Both APK flavors compile. The native player receives only a loopback capability URI; upstream requests attach captured bearer/context and refuse redirects. Native initialization/disposal and relay completion remain retained; timeout does not authorize another attachment. Completed native and network cleanup are joined before releasing the generation. Reserved cleanup context reads use the cleanup budget; screen disposal does not replay an already-attempted withdrawal.

No native transport, memory-peak, codec, physical-device or complete penetration-test claim follows from these source/unit/build checks. The remaining acceptance cases above and [Android qualification](../ci/ANDROID_PLAYBACK_QUALIFICATION.md) still govern completion.

## Actual root-media playlist qualification

October 29 native execution exposed a valid backend response that is a media playlist at `manifest.m3u8`. The parser now identifies master structure from `EXT-X-STREAM-INF`, permits it only at the root, binds root-media binary references to one rendition and still rejects nested masters, mixed renditions and foreign output paths. Reference publication remains atomic and bounded. Unit coverage rejects mixed/nested evidence while preserving the last validated reference set.

After that correction the actual Android Media3 player displays the authored frame with advancing position through the bearer-free local relay, completes a paused seek and confirms owned cleanup against real PostgreSQL/HTTP/FFmpeg. The current source/APK identities and failed/passing receipts are in [Android qualification](../ci/ANDROID_PLAYBACK_QUALIFICATION.md#approved-storage-patch-and-decoded-playback--october-29). This establishes the initial native transport/presentation lane; complete adversarial backpressure/fault, worker-recovery and OS-lifecycle qualification remains required before phase completion.


The final native repeat passes paused seek/tracks/quality, actual OS background/explicit Play, lease-expiry refusal and journal recovery against the real backend. Only the containing native-attachment observer changed to an explicitly approved thirty seconds; relay connection/header, idle/body/whole-request and ownership limits remain unchanged. Captured direct debug forwarding is qualification-only and keeps VM authentication enabled. [Final source/app/tool evidence](../ci/ANDROID_PLAYBACK_QUALIFICATION.md#final-approved-checkpoint) retains failed attempts separately from the passing six-stage run. Complete relay adversarial/backpressure/output-format qualification remains required.

## October 30 native fault ownership

Current Play intent and exact output ownership now fence late initialization, stale failure and scope invalidation. Delayed disposal and the real thirty-second attachment timeout retain capacity until native/socket completion; timeout cannot free an owner. The [seven-stage native checkpoint](../ci/ANDROID_PLAYBACK_QUALIFICATION.md#october-30-attachment-and-accepted-response-loss-checkpoint) passes accepted Start/change/renew/withdraw loss with one real viewer/history and paused replacement/reconciliation. A fixed-origin qualification-only witness forwards actual authority, follows no redirect/retry and retains completion; no production relay trust or endpoint changes. Seven real HTTP host cases cover loss, route/credential refusal, redirect canaries, malformed/oversized responses, full admission/close, deferred renewal and exact range framing.

The locked stack and existing relay deadlines/resource limits remain unchanged. This evidence complements native presentation and OS lifecycle checks; it does not complete worker recovery, pending scope/I/O or the entire adversarial/backpressure/output-format matrix. Those remain required before the overall Android phase and production rollout.

## November 1 transport ownership

The [confirmed relay qualification increment](../ci/ANDROID_RELAY_QUALIFICATION.md) replaces discarded transport completion with per-generation raw-I/O, writer, public socket-adapter and dial/TLS owners, while retaining the SDK HTTP parser. Eight accepted/closing sockets pause native acceptance; four upstream slots retain creation, cancellation and closing work. Scope retirement aborts pending HTTP work, cancels output and joins actual returned TCP destruction. Close completion is retained before cancellation callbacks; a failure keeps ownership and the native attachment barrier.

Final write readiness is observed even after a complete positive Windows write. Partial writes and waits recheck current scope. Binary responses receive canonical validated MIME/status/range/framing, and failed unknown-length output omits the terminal chunk. HLS publication accepts the generated subset only, rejects malformed/duplicate attributes and uses root revisions to refuse late variants. Unknown-length overflow preserves incomplete media or the last valid reference set, rather than publishing excess bytes or partial metadata.

HTTPS completes ordinary certificate/name verification before exposing the adapter socket. Cancellation joins owned public handshake/HTTP operations and confirmed TCP closure. Private SDK TLS housekeeping remains outside that public guarantee. Pure stream-controller notification completion does not substitute for transport destruction. Test-only roots remain isolated; no global I/O override, verification bypass or deployment CA is installed.

Host analysis, the 141-test Flutter suite plus two additional overflow cases, and 46 host-tool checks pass. The final actual Android run passes all twelve journeys plus the separate Windows worker guard, including socket/backpressure and generated 1080p/720p/480p H.264/AAC stereo BT.709 output with authored-frame checks and exact viewer/history attribution. The authority retains the interrupted full run and its memory evidence separately. These results do not enable production attachment/caps, qualify arbitrary provider HLS or broaden physical-device, speaker, codec or platform support.


The [Windows worker fixture checkpoint](../ci/ANDROID_WORKER_RECOVERY.md) qualifies backend physical stall/cleanup independently of native recovery. Its new source stays within the existing input bound, and source/executable/owned-thread validation is test-only. No relay authority, credential forwarding, budget or production enablement changes. Actual native recovery and the remaining scope/I/O/backpressure/output-format gates remain required; the authority preserves failed VM attempts, the initial host hold and the latest genuine client-observation failure. The controlled repeat passes seven established stages and confirms app/VM/backend cleanup, but not automatic worker recovery.
