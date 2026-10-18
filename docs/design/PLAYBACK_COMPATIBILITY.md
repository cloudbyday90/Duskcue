# Playback compatibility and recovery

## Scope and decisions — October 17, 2026

The next playback initiative delivers mainstream movie and episode files through a tested compatible path and recovers playable content when a damaged source still contains readable media. Web and the existing Tauri desktop client are the first qualification targets. The shared server contracts remain usable by mobile and TV clients.

The user authorized automatic bounded recovery, including skipped damaged passages and reduced quality, with disclosure when content or presentation changes. Original files remain immutable. Work proceeds on `main`; this increment includes local testing of open PR #5 and a no-cache local Compose rebuild, with no release or remote PR merge.

The initial input matrix includes MKV, MP4/M4V/MOV, WebM, TS/M2TS, AVI, MPEG-PS and WMV/ASF; H.264, HEVC/Main10, AV1, VP9, MPEG-2, MPEG-4 Part 2 and VC-1; common lossy/lossless audio; and SRT/WebVTT, ASS/SSA and PGS/VobSub subtitles. This is the qualification target, not a claim that every combination already works. HDR10/HLG, Dolby Vision profiles and object-audio passthrough require explicit fidelity evidence. Disc menus, encrypted media and obscure professional formats are outside the first milestone. Background preparation of compatible copies is a later option.

## Official-source research and tradeoffs

Research was checked against official sources on October 17, 2026. The FFmpeg documentation follows current upstream development; the actual packaged executable and its build configuration determine runtime availability.

| Recommendation | Benefit | Cost or limit | Source |
|---|---|---|---|
| Probe explicit stream metadata and retain global indices | Correct profile, bit depth, color and track decisions; attached pictures cannot replace the movie | Existing library rows need rescanning to acquire metadata | [FFprobe](https://ffmpeg.org/ffprobe.html) |
| Use short normal probing followed by one bounded larger attempt for incomplete streams | Handles dispersed metadata without making every file use a huge probe | More complex supervision; byte/media-analysis limits still need wall-clock deadlines | [Format options](https://ffmpeg.org/ffmpeg-formats.html#Format-Options), [Tokio process lifecycle](https://docs.rs/tokio/latest/tokio/process/) |
| Copy compatible video and encode incompatible audio independently | Preserves video quality and avoids unnecessary GPU/CPU work | Destination-container and client compatibility must both hold | [FFmpeg streamcopy](https://ffmpeg.org/ffmpeg.html#Streamcopy), [Emby playback methods](https://emby.media/support/articles/DirectPlay-Stream-Transcoding.html) |
| Treat client limits as hard constraints in every quality mode | Maximum quality cannot approve an unsupported stream | May still require conversion on a Maximum setting | [Media Capabilities working draft](https://www.w3.org/TR/media-capabilities/) |
| Select recovery by failure class; retain source timeline | Avoids retry loops and unnecessary degradation | Requires process, HTTP and client state to share attempt ownership | [Decoder options](https://ffmpeg.org/ffmpeg-codecs.html#Codec-Options), [HLS.js recovery](https://hlsjs.video-dev.org/api-docs/hls.js.hls.recovermediaerror) |
| Qualify the exact runtime with generated clean and damaged samples | Reproducible evidence of output, seeking and failure bounds | A small corpus does not establish universal format support | [FFmpeg FATE](https://ffmpeg.org/fate.html) |

VLC's [AVI demuxer](https://github.com/videolan/vlc-3.0/blob/master/modules/demux/avi/avi.c) illustrates container-specific index recovery. Missing essential MP4 metadata is a different failure: upstream documents why ordinary unfinalized MP4 may be undecodable while interrupted fragmented MP4 can remain readable. See [fragmentation](https://ffmpeg.org/ffmpeg-formats.html#Fragmentation). Missing content cannot be reconstructed by tolerant decoding.

## Recommended implementation stack

1. Normalize and bound probing. Retain actual video identity, codec/profile/level, bit depth, frame rate, interlace/aspect/color and Dolby Vision metadata. Preserve unknown values instead of inventing known-compatible codecs. Probe success establishes usable metadata, not verified decode health.
2. Produce an executable per-stream plan. Independently choose video and audio copy/encode, preserve selected global stream identity through subtitle filters and seeking, and keep Maximum quality within device capabilities.
3. Qualify clean controls and deterministic damaged derivatives against production arguments. Verify decoded media, selected tracks, content beyond damage, bounded failure and cleanup. Record the runtime identity.
4. Add recovery orchestration after these foundations: classify decoder/encoder/container/network failures, enforce attempt and no-progress budgets, resume at the last confirmed source position, and disclose degraded playback. Hardware decoder and encoder fallback are separate stages.

The final recommendation is this ordered stack around the existing managed FFmpeg service. A second desktop playback engine can be evaluated later using measured corpus failures and resource costs.

## Foundation implementation contract

Scanner probing uses local-file protocols only, bounded stdout/stderr, process deadlines, cancellation cleanup and at most two attempts. A nonzero probe exit is not admitted as a healthy library file; useful partial metadata admission needs an explicit health model in a later increment. Existing authorization and media-access checks remain at playback entry.

Normalized metadata lives in the existing `additional_streams` JSONB contract. `video.index` is the global input index of the first moving video stream; attached pictures are excluded. Old rows without that index retain type-based first-moving-video selection until rescanned. Output mapping and burn-in filters use the same selection. Source files are never rewritten.

Executable plans copy video for audio-only adaptation and re-encode video when incompatible, forced, or required for subtitle burn-in. Seek retains the same stream plan. Probe retries do not constitute mid-playback corruption recovery. HDR transformations, transparent damaged-passage skipping, stall-driven recovery and browser-to-live-server corpus qualification remain separately tracked work until demonstrated.

`StartPlaybackRequest.force_hls` requests HLS delivery without forcing video re-encoding. `force_transcode` retains its explicit encoding meaning. Authentication and preferred/overridden audio selection request HLS transport; subtitle rendering can separately require video encoding. Finalized audio channels and bitrate are retained independently of copied-video bitrate across seeking. Output codec selection is finalized against both client support and implemented encoders before launch; unimplemented HDR video transformations fail explicitly. An explicit file selection must belong to the already-authorized media item.

## Verification and outcome

The foundation is implemented. After the Rust dependency follow-up, the final Windows library suite passed **870 tests**, with six environment-dependent tests intentionally ignored by the default suite. The generated corpus is qualified separately. Fresh installations with the final npm security lockfiles passed, followed by **385 Vitest cases plus 37 Node cases** and zero Svelte errors/warnings. Formatting, all 119 client route contracts, resource-preset checks and copyright checks pass.

The desktop static production build and the dedicated Windows native Cargo check passed. Both explicit FFmpeg tests passed against final local image `sha256:d66de8693593bbe4a4ec6b9fb6f4e2e4fc7ed53bec0b20f480bcab9f2308e20b`, which packages **FFmpeg 8.1.2**. The compatibility corpus preserved all **192 decoded frames** on copy routes, selected the expected audio, and decoded advancing video and audio after deliberate TS damage at 2.25 seconds. The truncated source reached **6.458333 seconds** of output before its 6.5-second truncation. An MP4 stripped of essential `moov` metadata failed without publishing a media playlist. Input BLAKE3 hashes remained unchanged, and the UUID-owned corpus directory was cleaned up. The separate audio-first/default-description/selected-SRT test also passed through production arguments. Neither test establishes native desktop hardware qualification or a live browser-to-server playback result.

Strict all-target Clippy exits 101 with the same **13 baseline diagnostics** recorded in the [Tonight completion audit](../ci/TONIGHT_COMPLETION_AUDIT.md). The final lint run has no added diagnostic. This is a known lint limitation, not a clean strict-lint result. Logs are under ignored `.cache/local-evaluation/playback-*`, with guarded host-memory evidence in `.cache/testing-memory/`.

All three no-cache source builds passed. Compose was recreated successfully using the final qualified image and its existing data/cache volumes. The container is healthy; `/health/live`, `/health/ready` and the homepage return **200**, and an unauthenticated playback-start request returns **401**. Runtime inspection confirms a read-only root filesystem, `no-new-privileges` and the `ALL` capability drop list with CHOWN/SETUID/SETGID bootstrap allowances. These checks verify the deployed local instance and its existing hardening settings, not a full application security audit. Sanitized results are retained in `.cache/local-evaluation/playback-runtime-evaluation.json`.

The running instance is in initial setup mode, so live authenticated playback is not qualified by this smoke test. Startup also reports the absent optional GeoIP database and disables enrichment. The sampled request-error entry corresponds to the deliberate unauthenticated 401 check. Completing account setup and exercising the corpus through an authenticated live client remain separate qualification gates.

The generated corpus manifest is [compatibility/v1](../api/fixtures/playback/compatibility/v1/manifest.json). Full native hardware, HDR fidelity, comparative VLC/Kodi measurements, live-browser coupling, A/V synchronization qualification and the broad format matrix remain work beyond the initial small corpus. Target bitrate remains a video estimate, not a guaranteed aggregate ceiling including audio and container overhead.

The production Compose file intentionally consumes a published image. For a real local source rebuild, [docker/compose.local.yml](../../docker/compose.local.yml) supplies the build definition and `duskcue:local` tag. Run `node scripts/testing-memory/run.mjs compose-build`, then `docker compose -f docker-compose.yml -f docker/compose.local.yml up -d --no-build --wait --wait-timeout 180`. This does not publish an image or create a release.

All verification builds use Compose's [`--no-cache`](https://docs.docker.com/reference/cli/docker/compose/build/). BuildKit can invalidate matching cache mounts as well as rerun image steps; repeated dependency downloads and compilation are consistent with this mode. See [BuildKit #1092](https://github.com/moby/buildkit/pull/1092) and the maintainer clarification in [#5305](https://github.com/moby/buildkit/issues/5305). These runs qualify clean builds, not cache reuse. Ordinary [cache mounts](https://docs.docker.com/build/cache/optimize/#use-cache-mounts) can accelerate builds when retained by the builder, but correctness cannot depend on their contents.

## Locally applied open PR

The random selection, exact PR provenance, compatible security follow-ups and dependency validation are recorded separately in [Playback dependency review](../ci/PLAYBACK_DEPENDENCY_REVIEW.md). PR #5 remains open and unmerged.

The post-push GitHub/RustSec review, patched Rust dependency graph, bounded subtitle ZIP migration and remaining audit exceptions are recorded in [Playback dependency security follow-up](../security/PLAYBACK_DEPENDENCY_FOLLOWUP.md).

## Recovery implementation and next qualification gate

The next controller increment is implemented and its qualification is recorded in [Bounded playback recovery](PLAYBACK_RECOVERY.md). Focused modules own failure hints, complete progress records, a no-progress watchdog, retry policy and client status. Replacements retire the old process/cache, preserve selected streams and use the last confirmed client position. The logical play session and its bounded budget survive recovery and seeking.

The 18 October [direct-play fallback increment](PLAYBACK_DIRECT_FALLBACK.md) implements native-decode handoff with source position, selected tracks, profile authorization and the shared committed attempt budget preserved. Cookie-authenticated native web delivery and bearer-authenticated desktop HLS remain distinct paths; consult its execution record before making device claims. Next extend that harness with HEVC/AV1, damaged-source recovery and HDR fidelity evidence. Include real unavailable-hardware initialization and decoder failure near the playhead, A/V synchronization, cancellation during replacement and exhaustion. Unsupported HDR transformations remain explicit failures until qualified. Existing lifecycle/controller tests qualify only their recorded boundary; broad format support and VLC/Kodi parity remain unproven.
