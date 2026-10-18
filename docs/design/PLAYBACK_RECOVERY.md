# Bounded playback recovery

Status: implemented, no-cache production image evaluated and real stalled-worker recovery qualified, 17 October 2026. This document owns recovery policy and qualification; [playback compatibility](PLAYBACK_COMPATIBILITY.md) owns format admission and stream planning.

## Scope and decisions

The confirmed first milestone serves the web client and the existing desktop application through their shared player. Recovery is automatic and bounded, preserves original files and selected streams, and discloses any skipped content or changed presentation. Mobile and TV can adopt the same server contract later. No additional native playback engine or release is introduced.

The server owns failure classification, retry eligibility and attempt counts. The player supplies the last confirmed source position and its active transcode generation. Encoder progress can run ahead of viewing and must never become the resume position. Each replacement has a new UUID, cache directory, initialization segment and playlist. The stable database play session remains unchanged.

## Official-source research and alternatives

Research uses sources discovered through search/GitHub MCP on 17 October 2026.

| Choice | Benefits | Costs and limits |
| --- | --- | --- |
| Fresh HLS generation and explicit reattachment | Isolates initialization/timestamp state; reuses existing seek ownership | Brief interruption; client must reject stale responses and apply a source offset |
| Append a restarted encoder to an existing playlist | Potentially smoother handoff | Requires continuity/discontinuity, codec, timestamp and initialization correctness; unsuitable as an unqualified shortcut |
| Scoped hardware encoder fallback | Retains codec and tracks without changing global hardware detection | Software encoding uses more CPU and may change compression quality |
| Fixed recovery budget and advancing-media watchdog | Bounds resource use and prevents retry loops even when logs continue | Slow storage or very slow encoding can trigger the watchdog; stall alone does not prove corruption |
| Client-only media recovery | Cheap and useful for a transient browser decoder problem | Cannot supervise the server process or own a shared retry budget |

[FFmpeg's progress protocol](https://www.ffmpeg.org/ffmpeg.html) publishes records ending in `progress=continue` or `progress=end`. Parse complete records and normalize microseconds before comparing media advancement. Its seeking guidance also distinguishes accurate transcoding seeks from stream-copy preroll: automatic content skipping must not claim frame-accurate repair of copied video.

[Hls.recoverMediaError](https://hlsjs.video-dev.org/api-docs/hls.js.hls.recovermediaerror) reattaches the browser media element; [Hls.startLoad](https://hlsjs.video-dev.org/api-docs/hls.js.hls.startload) restarts loading. These are client mechanisms, not evidence that a source file needs another server encode. Existing bounded local recovery remains before a server request.

[SQLx transactions](https://docs.rs/sqlx/latest/sqlx/struct.Transaction.html) roll back if dropped before commit. A replacement therefore needs an armed cancellation guard through database promotion and commit. A disconnected request must not leave an unpublished encode running.

[RFC 8216](https://www.rfc-editor.org/rfc/rfc8216.html) and [Apple's HLS authoring specification](https://developer.apple.com/documentation/http-live-streaming/hls-authoring-specification-for-apple-devices/) define continuity and initialization requirements. A fresh generation makes those boundaries explicit; appending a restart is deferred until its timestamp and decoder behavior is qualified.

[Tokio child-process guidance](https://docs.rs/tokio/latest/tokio/process/struct.Child.html) distinguishes dropping a handle from terminating and waiting for the child. The existing process-tools owner remains responsible for physical exit, permit release and cache retirement. [FFmpeg's implementation](https://github.com/FFmpeg/FFmpeg/blob/master/fftools/ffmpeg.c) also confirms that the legacy `out_time_ms` label contains microseconds; this implementation consumes `out_time_us` explicitly.

## Recommended stack and ownership

1. Existing sandboxed FFmpeg backend and normalized per-stream plan.
2. Complete-record progress parser, safe diagnostic categories and a 30-second advancing-media watchdog in focused transcoding modules.
3. Attempt-local hardware encoder selection, preserving stream actions, selected indices, audio settings and source start position through recovery and later seeking.
4. Playback-domain orchestration under the existing play-session row lock, shared with heartbeat, seek and stop. At most two automatic replacement attempts per play session, including any startup hardware fallback. Cancellation, failed cleanup, unknown failures, unsupported transformations and exhaustion terminate recovery.
5. Generation-aware heartbeat delivery and shared ES-module client adoption. Display client-owned localized recovery/degradation text; never return paths or raw FFmpeg diagnostics.

Only an observed failed hardware encode can trigger the same-codec software fallback. A stalled process permits one restart at the confirmed client position. A classified decoder failure can request one four-second skip only while video is already encoded and the last encoder checkpoint lies within the following four seconds of the client's playhead. A failure far ahead of viewing must not skip healthy content near the viewer. The skip is a bounded seek request, not a guarantee that the damaged passage ends there. Remux/container failure does not silently promote HDR or copied video to an unqualified encode. Unknown/container failures stop with an explicit result. Retry limits persist across seeks; a new user-initiated play is a new session.

Recovery is opt-in through the generation-aware heartbeat request. Direct-file legacy bookkeeping remains unchanged. The 18 October [direct-play fallback increment](PLAYBACK_DIRECT_FALLBACK.md) adds a separately admitted, revision-bound native-decode handoff for new direct sessions; its exact output, capability and runtime boundaries are owned by that document. An HLS heartbeat with a missing or stale generation receives canonical replacement state but cannot overwrite the current position or spend another attempt; HLS status requires the original active profile. The client also guards responses against context changes, seeks, stop and disposal. Normal successful FFmpeg completion never triggers recovery; a browser-only fatal error cannot force a new process when the HLS worker is healthy. The transcoder continues producing output while the viewer is paused, so a failed worker can be replaced while retaining the viewer's pause intent.

Explicit seeks and recovery replacements share the [mixed-stream seek timing rule](PLAYBACK_SEEK_TIMING.md). Copied-audio preroll must not shift accurately sought video away from the requested source offset; trimming is scoped to the qualified video-encode/audio-copy boundary and does not promote copied video or HDR processing.

## Additive heartbeat contract

`POST /api/v1/playback/heartbeat` accepts optional `transcode_session_id` and `recovery_requested`. The server derives eligibility from its worker observation; the client flag cannot manufacture a failure. A healthy explicit request returns `exhausted / not_eligible` without charging or exhausting the server budget. A later observed stall can still recover through periodic heartbeats.

Heartbeat and playback-info responses expose canonical `transcode_session_id`, `stream_url`, `stream_offset_ms` and optional `recovery`. Recovery reports `status`, `attempts`, a fixed public `reason`, `skipped_ms` and `quality_reduced`. A hardware encoder substitution retains codec, resolution and bitrate, so it does not assert measured quality reduction. Startup responses also disclose a successful hardware fallback. Client notices describe a prepared recovery stream without claiming verified uninterrupted playback.

The session row stores `recovery_budget`, the current generation, source offset and the last recovery result. Replacement failure commits a terminal result and the spent attempt; it cannot repeat on every heartbeat. Authorization executes on the already owned transaction connection, avoiding an additional pool acquisition while holding the session lock.

## Security and race invariants

- Authenticate the caller, bind the active profile and revalidate media access before allocating a replacement.
- Classify diagnostics conservatively, with bounded retained state. Treat diagnostic text as hints only after observed process failure.
- Await physical termination and owned-cache cleanup before allocating the next worker. Preserve existing sandbox, protocol whitelist, concurrency permit and shutdown behavior.
- Hold pending replacement ownership through transaction commit. Stop/seek/recovery serialize through the same play-session lock; cancellation wins over retry.
- Preserve selected streams and the qualified plan. Do not add arbitrary input URLs, shell arguments, wider file access or an HDR conversion bypass.
- Count attempts even when replacement startup fails and return terminal status, preventing repeated heartbeat retries.

The dependency graph is unchanged. Existing audit limits and the RSA advisory exception remain documented in the [dependency security follow-up](../security/PLAYBACK_DEPENDENCY_FOLLOWUP.md); recovery qualification does not supersede that assessment.

## Acceptance and outcome

Qualification must cover complete progress records and timestamp units, nonadvancing log spam, stdout EOF with a live child, successful completion, classified hardware/decoder failures, cancellation during replacement, seek after recovery, stale reports, generation replay after a lost response, exhaustion and owned process/cache cleanup. The generated media corpus remains the source-preservation and decode baseline.

The implementation is split into progress, classification, watchdog, startup, encoding and recovery services. JavaScript additions use ES modules. No dependency or database migration is required.

Executed locally on 17 October 2026 through the serialized resource runner:

| Check | Outcome |
| --- | --- |
| Rust library suite | 894 passed, zero failed, seven opt-in cases ignored; corpus, selected-track/subtitle and mixed-stream seek cases also executed separately |
| Shared client suite | 410 Vitest and 37 Node cases passed |
| Shared source/type check | Zero errors or warnings |
| Desktop static build and Windows desktop Cargo check | Passed; existing bundle-size advisory remains |
| Rust Cargo check | Passed |
| Strict Clippy | Blocked by the same 13 recorded baseline diagnostics; no additional diagnostics introduced |
| HTTP route contracts | 119 checks passed |
| Generated compatibility corpus against rebuilt image | Passed: 192 copied-frame hashes, surviving damaged-TS media, bounded missing-MOOV failure and unchanged source BLAKE3 hashes |
| Real selected-track and subtitle fixture | Passed: audio-first input retains the selected audio stream and selected SRT rendering |
| Mixed video-encode/audio-copy seek regression | Passed after fixing retained audio preroll: first video PTS 0.083008 seconds, leading A/V delta about 15 ms, correct selected audio and preserved first-frame pixels |
| Browser recovery cases | Four passed, including decoded-but-paused video after a delayed replacement manifest |
| Ordinary playback and authorization browser cases | 14 passed, including real MP4/HLS decoding, keyboard seeking, heartbeat checkpoints, session expiry and superseded starts |
| Controlled Linux recovery contract | Passed: real process stall, generation replay, profile authorization, serialized seek/stop and physical cleanup |

The final production Compose build used `--no-cache` after both the client pause and mixed-stream seek corrections; both Compose files then recreated the service with `--no-build --wait`. The resulting immutable local image is `sha256:f45eda1c8bbe3a9685196aae1200543601ec5efc2fb0f858ca863dd9d42ae8dc`, with FFmpeg 8.1.2. Liveness, readiness and the home page returned HTTP 200; unauthenticated playback start returned 401. The container is healthy with a read-only root, no-new-privileges, dropped capabilities and the existing three bootstrap capabilities. Existing data/cache volumes and read-only media binds were preserved. Log evaluation found no unexpected errors; the deliberate unauthorized request produced one expected error entry. Initial setup mode and an absent optional GeoIP database remain visible.

The first Linux qualification attempt stopped safely during compilation after a read-only WSL metadata timeout, with owned cleanup confirmed. The shared telemetry policy now retries only typed metadata transport failures within the original sample's unchanged ten-second freshness window. Invalid identity, parsing, stale evidence and insufficient reserve still fail closed. Metadata errors retain only bounded code/signal/termination/timing context. Independent WSL inventory and Docker identity reads overlap, while invalid evidence dominates a concurrent transport failure. All 13 focused metadata, recovery, telemetry and preset checks pass. Later reserve holds, registry DNS failure and persistent telemetry loss all retained failed/held outcomes with cleanup; none counts as runtime qualification. Memory limits, five-second metadata deadlines and ten-second freshness remain unchanged.

The first admitted HTTP test exposed the [mixed-stream seek timing defect](PLAYBACK_SEEK_TIMING.md) before reaching the recovery case. A separate real-media regression reproduced it and verified the scoped output trim, retaining the original 30-second target and 250 ms bound. The corrected current-source HTTP contract then passed in 176.44 seconds, including the one-connection authorization fixture, original progressive playback and real stalled-worker recovery. All four progressive generations and three recovery/seek generations decoded their first video frame at 0.083008 seconds; the completed progressive output contained 285 segments. Healthy client hints did not allocate another worker, stale/missing generations preserved the checkpoint and budget, wrong owners/profiles were refused, and owned children and caches were confirmed retired.

That current-source proof is `.cache/testing-memory/playback-recovery-1f3886d2-cd73-4b22-8e42-53aa91057998/result.json`, source SHA-256 `a2b0ea66fc57fb496a21c650b87f4c9b20b2816681a6129290f8981c9d23ed63`. After the final no-cache rebuild, the same contract passed again in 178.27 seconds against production image `sha256:f45eda1c8bbe3a9685196aae1200543601ec5efc2fb0f858ca863dd9d42ae8dc`. The final result is `.cache/testing-memory/playback-recovery-5e203f37-72b2-402c-b493-03a2e5070159/result.json`, qualification image `sha256:2b5e3e409db16d18cb4b1490da871928358404768f18272cfb37e22ee5a5092a`, with the same source hash, exit zero, no OOM and cleanup passed. Its FFmpeg and FFprobe executable hashes match the earlier corpus-qualified image. The task-owned local Duskcue service was temporarily stopped for fresh Linux headroom during each isolated run, then automatically restored healthy; its volumes and the other running applications were preserved. No global memory/network settings or admission limits changed.

Final client review exposed a manifest-loading pause race. Attachment now reads current play intent, and explicit controls update that intent before touching the media element. Stale unload events cannot reverse it. Three source regressions and a real-HLS browser case cover the interval between replacement attachment and manifest readiness. The direct-source failure case now asserts the failed request and visible recovery flow because owned source cleanup intentionally clears `video.error`.

Command logs and sanitized runtime evidence are retained under ignored `.cache/local-evaluation/recovery-*`; source manifests, memory samples and owned cleanup results are under `.cache/testing-memory/playback-recovery-*`. Live deployment data remains untouched; initial setup mode is not evidence of authenticated playback qualification. The subsequent [live-client gate](PLAYBACK_LIVE_CLIENT_QUALIFICATION.md) adds actual browser and desktop presentation rather than treating compilation or backend requests as playback proof. Its separate [native worker-failure follow-up](PLAYBACK_NATIVE_FAILURE_QUALIFICATION.md) has passed initial and final-image paced desktop recovery gates, the no-cache production build and restored Compose evaluation.

The controlled recovery fixture injects a real process stall. Hardware-failure substitution and four-second decoder-skip eligibility have policy and lifecycle coverage, but actual GPU failure and a heartbeat replacement across a real decoder-corrupted passage still require distinct runtime evidence. Packaged desktop presentation and its final-image real stalled-worker handoff are recorded in the later live-client and native worker-failure authorities above. The generated damaged-TS result establishes surviving-media decoding with preserved sources; it does not establish recovery of arbitrary corruption or universal format support.

## Direct-play follow-up and format qualification

The authenticated direct-file decoder handoff is implemented and tracked separately in [Direct-play fallback](PLAYBACK_DIRECT_FALLBACK.md), including its current execution results and remaining gates. It retains the original requirements: bind the current user, profile, file and direct-source revision; preserve selected tracks, source position and pause intent; share the existing committed recovery budget; and revalidate access on the locked connection before allocation. Decoder hints cannot bypass authorization or provide FFmpeg arguments or paths. Unsupported HDR conversion remains refused.

The direct-fallback increment establishes a real native decoder failure followed by authenticated HLS frames and selected audio in installed Chrome using shared source/store modules and isolated Axum/database/FFmpeg services. The subsequent [live-client increment](PLAYBACK_LIVE_CLIENT_QUALIFICATION.md) joins the complete product Player, production API router and actual Tauri bearer/keyring transport to those real services, including seek/reprobe and visible Close. [Native worker-failure qualification](PLAYBACK_NATIVE_FAILURE_QUALIFICATION.md), implemented at `ec7de656a968b9e5986081af7289a520de69c6a7`, adds the deterministic owned paced fixture. Its initial and final-image real paused desktop stall/recovery/further-seek gates, eight Linux feature cases, no-cache production build/evaluation and physical cleanup passed. The unchanged-source final-image repeat took 131.854 seconds at `.cache/testing-memory/playback-recovery-d88f3229-4f75-4df1-b58d-3ec6949d7873/result.json`. Preserve desktop bearer/HLS delivery; include denied, stale, cancelled and exhausted attempts. Synthetic media-error events establish client behavior only. Next expand the real-client harness with the following format and fidelity corpus.

Build six generated masters beside the H.264/AAC control: HEVC Main 8-bit SDR, HEVC Main10 10-bit SDR, AV1 Main 8-bit SDR, AV1 Main 10-bit SDR, HEVC Main10 BT.2020/PQ HDR10 and HEVC Main10 BT.2020/HLG. Ten-bit SDR must remain distinct from HDR. Keep clips small, with motion, frame identifiers, reference ramps and distinguishable audio. Remux selected masters into MP4/MKV and AV1 into WebM without another encode. [FATE guidance](https://www.ffmpeg.org/fate.html) supports small correctness fixtures; these cannot qualify 4K performance.

The [June 2026 Media Capabilities draft](https://www.w3.org/TR/2026/WD-media-capabilities-20260609/) supports testing the actual delivery configuration, including media-source decoding, dimensions, rate and HDR properties. Record supported, smooth and power-efficient outcomes separately and verify actual playback. [AV1's container binding](https://aomediacodec.github.io/av1-isobmff/v1.3.0.html) requires codec strings consistent with bitstream and color information. hls.js support alone does not prove the browser decoder supports the selected profile.

For copy/remux, compare decoded frame hashes at an explicit precision-preserving pixel format, frame count/timing, selected audio and color/range/HDR metadata. Use actual PQ/HLG numeric reference patches from current [BT.2100-3](https://www.itu.int/rec/R-REC-BT.2100); attaching HDR tags to an SDR pattern establishes metadata handling only. HDR-to-SDR conversion remains rejected until a reference-checked transform handles transfer function, gamut, range and output tags. The [FFmpeg tone mapper](https://ffmpeg.org/ffmpeg-filters.html#tonemap) requires linear-light floating-point input for its software path.

Qualify one installed branded Windows browser and the actual Tauri/WebView2 application against an isolated real server with a disposable database. [Playwright's media-codec guidance](https://playwright.dev/docs/browsers#media-codecs) and [Tauri's webview documentation](https://v2.tauri.app/reference/webview-versions/) explain why a generic Chromium pass does not establish packaged desktop support. Require authenticated manifest/init/segment delivery, presented-frame advancement, audible selected audio, source-position-correct seek/resume, one real watchdog recovery, its visible notice, profile/authorization refusal and confirmed cleanup. Native SDR decoding that fails qualifies only if the real fallback plays; unsupported HDR processing qualifies as an explicit refusal. Physical HDR display fidelity, AV1 HDR, Dolby Vision/HDR10+, hardware performance and other desktop platforms need separate evidence.
