# Flutter protocol-two HLS relay

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
| Accepted sockets | Eight total, including incomplete headers; immediate refusal/closure above the cap, held until observed completion. OS backlog is not the accounting mechanism. |
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
