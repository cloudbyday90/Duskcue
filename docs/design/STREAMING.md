# Streaming & Transcoding Design

## Overview

This document covers the complete streaming and transcoding architecture: how media is delivered from server to client, how playback decisions are made, how transcoding works, and how adaptive bitrate streaming is handled.

The full video format catalog — supported codecs, container formats, HDR standards, bit depths, and the transcode codec matrix — is documented in [VIDEO_FORMATS.md](VIDEO_FORMATS.md). The transcoding decision engine is documented in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md).

---

## Protocol Selection: HLS with fMP4 Segments

### Decision

**HLS (HTTP Live Streaming)** is the sole adaptive streaming protocol, using **fMP4 (fragmented MP4)** segments with a 6-second segment duration.

### Why HLS Over DASH

| Factor | HLS | DASH | Winner |
|---|---|---|---|
| **Safari/iOS native support** | Built-in since Safari 6 / iOS 3.2 | Never (requires MSE since iOS 17, iPhone only) | **HLS** |
| **Chrome/Edge native support** | Chrome 142+, Edge 142+ (May 2026) | Yes (MSE-based) | Tie (both work) |
| **Firefox native support** | No (needs hls.js) | Yes (MSE-based) | DASH |
| **Android native support** | Chrome for Android 147+ | Yes | Tie |
| **Smart TV support** | Samsung, LG both support HLS | Samsung, LG both support DASH | Tie |
| **Simplicity** | Plain text .m3u8 manifest; hand-editable | XML .mpd manifest; strict schema | **HLS** |
| **Self-hosted tooling** | FFmpeg first-class; simpler pipeline | FFmpeg supports; more config | **HLS** |
| **Apple TV** | Native | Not supported | **HLS** |
| **CMAF convergence** | Both can share fMP4 segments | Both can share fMP4 segments | Tie |

**Rationale:**
- In 2026, HLS and DASH have converged at the segment level via CMAF (same fMP4 segments). The manifests differ, but the underlying bytes are often identical.
- For a self-hosted Duskcue targeting families, HLS is the pragmatic default: native on all Apple devices (iPhone, iPad, Apple TV, Safari), native on Chrome 142+ and Edge 142+ (covering most desktop browsers), and works on Firefox/Opera via hls.js.
- DASH adds no meaningful capability for our use case. We are not doing multi-DRM (FairPlay + Widevine + PlayReady), SCTE-35 ad insertion, or YouTube-scale delivery. HLS covers all our target platforms with simpler infrastructure.
- If DASH is needed in the future (e.g., a specific Smart TV requires it), adding DASH manifests on top of existing CMAF segments is straightforward — same segments, additional manifest format.

### Why fMP4 Over MPEG-TS

| Factor | fMP4 | MPEG-TS | Winner |
|---|---|---|---|
| **Segment size overhead** | ~0.5% | ~2-4% (188-byte packets) | **fMP4** |
| **CMAF compatible** | Yes (native) | No | **fMP4** |
| **HLS specification** | Supported since HLS v7 (2016) | Original format | Both |
| **Seek precision** | Sample-accurate | Keyframe-aligned only | **fMP4** |
| **Subtitle muxing** | WebVTT in MP4 (WebVTT-in-ISOBMFF) | Sidecar only | **fMP4** |
| **Future DASH compatibility** | Direct reuse | Requires re-segmenting | **fMP4** |

**Rationale:** fMP4 is the modern standard for both HLS and DASH. It has lower overhead, better seek precision, native CMAF compatibility, and enables future DASH support without re-segmenting. MPEG-TS is legacy; only worth using if a specific client requires it.

### Segment Duration: 6 Seconds

- **6 seconds** is the sweet spot for VOD streaming: good ABR reactivity without excessive HTTP request overhead
- Aligns with keyframe interval (GOP size = frame rate × 6 = 144 frames at 24fps, 180 frames at 30fps)
- Shorter segments (2s) are only needed for low-latency live streaming (not our use case)
- Longer segments (10s) reduce request overhead but slow ABR adaptation

### hls.js for Browser Compatibility

The web client (SvelteKit) uses **hls.js** as a fallback for browsers without native HLS support:

- **Native HLS** (Safari, Chrome 142+, Edge 142+, iOS, Samsung Internet): `<video src="manifest.m3u8">` — no library needed
- **hls.js** (Firefox, older Chrome/Edge, desktop Opera): parses .m3u8, transmuxes fMP4 segments, feeds MSE
- **Neither** (ancient browsers, IE): not supported; the web client shows an "unsupported browser" message

Feature detection pattern:
```javascript
if (video.canPlayType('application/vnd.apple.mpegurl')) {
    // Native HLS — Safari, Chrome 142+, Edge 142+, iOS
    video.src = manifestUrl;
} else if (Hls.isSupported()) {
    // hls.js — Firefox, older Chrome/Edge, Opera
    const hls = new Hls();
    hls.loadSource(manifestUrl);
    hls.attachMedia(video);
} else {
    // Unsupported
}
```

---

## Streaming Decision Flow

When a client requests to play a media item, the server follows a strict decision flow to determine how to deliver the content. This flow is inspired by Plex's proven model but simplified to avoid Plex's common pitfalls (TrueHD deprioritization, DV Profile 7 rejection).

### Decision Categories

| Category | Description | Server Work | Quality |
|---|---|---|---|
| **Direct Play** | Client supports container, video codec, audio codec, and subtitles natively | Read from disk, send bytes | Original quality |
| **Direct Stream** (Remux) | Client supports video and audio codecs but not the container; OR client needs HDR/ DV metadata stripped | Copy streams into new container; possibly strip DV enhancement layer | Original quality (no re-encoding) |
| **Transcode** | Client cannot decode the video and/or audio codec; OR bandwidth requires lower bitrate; OR subtitle burn-in needed | Full re-encode via FFmpeg | Reduced quality (lossy) |

### Decision Algorithm

```
Client requests playback of media_item_id:
    1. Fetch media_files for this item
    2. Client sends Device Profile (supported containers, codecs, max bitrate, subtitle capabilities)
    3. For each available media_file:
        a. CHECK DIRECT PLAY:
           - Container in client's supported list?
           - Video codec in client's supported list?
           - Audio codec in client's supported list?
           - Selected subtitle supported natively (not requiring burn-in)?
           - Total bitrate <= client's max bitrate (or within network limit)?
           → If ALL yes: DIRECT PLAY — serve file as-is via HTTP range requests

        b. CHECK DIRECT STREAM (REMUX):
           - Video codec in client's supported list (stream copy possible)?
           - Audio codec in client's supported list (stream copy possible)?
           - BUT container not in client's supported list?
           - OR Dolby Vision metadata needs stripping (DV Profile 7 → HDR10 fallback)?
           → If YES: DIRECT STREAM — FFmpeg remux with -c:v copy -c:a copy, new container

        c. CHECK PARTIAL TRANSCODE:
           - Video codec supported (copy) BUT audio codec NOT supported?
           → Transcode audio only; copy video stream
           - Audio codec supported (copy) BUT video codec NOT supported?
           → Transcode video only; copy audio stream

        d. FULL TRANSCODE:
           - Neither video nor audio codec supported by client?
           - OR bandwidth requires downscaled/low-bitrate stream?
           - OR subtitle burn-in required (image subtitles on incompatible client)?
           - OR HDR → SDR tone mapping needed?
           → Full transcode via FFmpeg with HLS output

    4. Prefer DIRECT PLAY > DIRECT STREAM > PARTIAL TRANSCODE > FULL TRANSCODE
    5. Among multiple media_files (multi-version), prefer the one closest to client capabilities
```

### Key Differences from Plex/Jellyfin

| Issue | Plex | Jellyfin | Our Approach |
|---|---|---|---|
| **DV Profile 7 fallback** | Direct Play works (client-side HDR10 fallback) | Rejects or forces transcode | Allow Direct Play when HDR10 base layer exists; let client handle fallback |
| **TrueHD passthrough** | Direct Play works | Deprioritizes TrueHD for HLS, transcodes to AAC | Allow TrueHD copy in remux when client supports it; only transcode audio when truly needed |
| **Subtitle burn-in** | CPU-only burn-in (slow) | GPU-accelerated burn-in | GPU-accelerated subtitle burn-in via FFmpeg overlay filters when available |
| **Client-side DV fallback** | Automatic (protocol=* wildcard) | Missing — device profile rejects DV | Add `allow_client_side_dv_fallback` flag; send original file if device supports HDR10 |

### Device Profiles

Each client sends a **Device Profile** on connection describing its capabilities. The server uses this to make streaming decisions. Device profiles are persisted in the `device_profiles` table and can be refined via the capability wizard (empirical testing). Full design documented in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md).

```rust
struct DeviceProfile {
    name: String,
    supported_containers: Vec<String>,       // ["mkv", "mp4", "mov"]
    supported_video_codecs: Vec<CodecSupport>,// [{ codec: "hevc", max_profile: "main10", ... }]
    supported_audio_codecs: Vec<CodecSupport>,// [{ codec: "truehd", max_channels: 8, ... }]
    max_streaming_bitrate: u64,               // bits per second
    max_video_resolution: (u32, u32),         // (width, height)
    supported_subtitle_formats: Vec<String>,  // ["srt", "ass", "pgs", "webvtt"]
    supports_hdr: bool,
    supports_dolby_vision: bool,
    allow_client_side_dv_fallback: bool,      // NEW: trust client to handle DV→HDR10
    hardware_decode_capabilities: Vec<String>,// ["h264", "hevc", "av1"]
}
```

Default profiles are built into each client (web, Tauri desktop, Flutter mobile, TV apps). Users can override if needed.

### Dolby Vision Handling

Full design documented in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md) — Dolby Vision Handling section. Key rule: when a DV Profile 7 file has an HDR10 base layer and the device supports HDR10, allow direct play via `allow_client_side_dv_fallback`. Never transcode video just because of DV Profile 7.

### Audio Passthrough

Full design documented in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md) — Audio Passthrough Strategy section. Key rule: never deprioritize TrueHD/DTS for HLS streaming. When the device reports codec support, pass through unmodified. Use fMP4 segments (not MPEG-TS) which support all audio codecs.

### Smart Subtitle Strategy

Full design documented in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md) — Smart Subtitle Strategy section. Key rule: never burn in text-based subtitles. SRT/WebVTT always passthrough. ASS converts to SRT if unsupported. PGS burn-in only as last resort.

---

## Direct Play (HTTP Range Requests)

When the client can handle the original file, the server serves it via standard **HTTP range requests** (RFC 7233). No transcoding, no remuxing — the server is just a file server.

### How It Works

1. Client sends `GET /api/v1/stream/{media_file_id}` with `Range: bytes=0-` header
2. Server reads the file from disk and responds with `206 Partial Content`
3. Client requests subsequent ranges as needed (seeking, buffering)
4. Server updates `user_item_data.resume_position_ms` via periodic heartbeat

### Headers

```
GET /api/v1/stream/{media_file_id} HTTP/1.1
Range: bytes=0-
Authorization: Bearer {session_token}
```

```
HTTP/1.1 206 Partial Content
Content-Type: video/x-matroska
Content-Length: 66554259044
Content-Range: bytes 0-66554259043/66554259044
Accept-Ranges: bytes
```

### Requirements for Direct Play

- Client supports the file's container format (MKV, MP4, MOV, etc.)
- Client supports the file's video codec (H.264, H.265, AV1, VP9, etc.)
- Client supports the file's audio codec (AAC, AC-3, E-AC-3, TrueHD, DTS, FLAC, Opus, etc.)
- If a subtitle is selected, the client supports it natively (text subtitles like SRT/ASS, or image subtitles like PGS/VOBSUB)
- Total bitrate is within the client's max streaming bitrate (local network typically no limit; remote may be constrained by upload speed)

---

## Direct Stream (Remux)

When the client supports the video and audio streams but not the container (or needs metadata changes), the server remuxes — copying streams without re-encoding into a compatible container.

### When Remux Is Needed

| Scenario | Action |
|---|---|
| MKV container, client only supports MP4 | Copy video+audio into MP4 container |
| Dolby Vision Profile 7 on non-DV display | Strip DV enhancement layer via `hevc_metadata=remove_dovi=1` BSF; keep HDR10 base layer |
| Subtitle format incompatibility (e.g., ASS in MKV, client needs SRT) | Extract and convert text subtitles (no re-encode) |

### FFmpeg Remux Command (MKV → MP4)

```bash
ffmpeg -i "input.mkv" \
    -map 0:0 -map 0:1 \
    -c:v copy \
    -c:a copy \
    -movflags +faststart \
    -f mp4 \
    "output.mp4"
```

### FFmpeg Remux with DV Stripping

```bash
ffmpeg -i "input.mkv" \
    -map 0:0 -map 0:1 \
    -c:v copy \
    -bsf:v hevc_mp4toannexb,hevc_metadata=remove_dovi=1 \
    -c:a copy \
    -f hls \
    -hls_time 6 \
    -hls_segment_type fmp4 \
    -hls_list_size 0 \
    -hls_playlist_type vod \
    "output.m3u8"
```

Remux uses very little CPU since no encoding/decoding occurs — just container rewriting.

---

## Transcoding

When the client cannot handle the source codec, container, or bitrate, the server transcodes in real-time using FFmpeg. Transcoding is the most resource-intensive operation the server performs.

### Transcode Triggers

| Trigger | Type | Example |
|---|---|---|
| Video codec incompatibility | Video transcode | HEVC file, H.264-only client |
| Audio codec incompatibility | Audio transcode | TrueHD 7.1 file, stereo-only client |
| Resolution downsizing | Video transcode | 4K file, 1080p remote stream |
| Bitrate reduction | Video transcode | 80 Mbps remux, 10 Mbps upload limit |
| HDR → SDR tone mapping | Video transcode | HDR10 file, SDR display |
| Subtitle burn-in | Video transcode | PGS subtitle, web browser client |
| Container + codec incompatibility | Full transcode | MKV+HEVC+TrueHD, Apple TV needs MP4+AAC |

### Transcode Pipeline Architecture

```
Client Request
    │
    ├─ Playback Decision (Direct Play / Remux / Transcode)
    │
    ├─ If Transcode:
    │   ├─ Select source media_file
    │   ├─ Determine target parameters (codec, resolution, bitrate, audio)
    │   ├─ Check hardware acceleration availability
    │   ├─ Build FFmpeg command
    │   ├─ Spawn FFmpeg process (tokio-process-tools)
    │   ├─ FFmpeg outputs HLS fMP4 segments to transcode directory
    │   ├─ FFmpeg sends structured progress via -progress pipe:1 (stdout)
    │   ├─ Server serves segments via HTTP (standard file serving)
    │   └─ Client plays via HLS manifest
    │
    └─ Session lifecycle:
        ├─ On start: create transcode session, spawn FFmpeg with sandboxing
        ├─ During playback: FFmpeg writes segments ahead of client position; progress parsed from stdout
        ├─ On seek: gracefully terminate FFmpeg, restart from new position (with segment cleanup)
        ├─ On pause: FFmpeg continues to buffer ahead (or pause after buffer)
        ├─ On stop: gracefully terminate FFmpeg, schedule segment cleanup
        └─ On session end: delete all transcode artifacts
```

### Transcode Session State

Each transcode session is tracked in memory (not persisted to DB — transient state):

```rust
struct TranscodeSession {
    id: Uuid,
    media_file_id: Uuid,
    user_id: Uuid,
    started_at: DateTime<Utc>,

    source_video_codec: String,
    source_video_resolution: (u32, u32),
    source_audio_codec: String,

    target_video_codec: String,
    target_video_resolution: (u32, u32),
    target_audio_codec: String,
    target_bitrate: u32,

    hw_accel: HwAccelMethod,
    ffmpeg_pid: u32,

    segment_dir: PathBuf,
    manifest_path: PathBuf,
    segments_written: u32,
    client_position_segment: u32,

    is_complete: bool, // VOD transcode finished all segments
}
```

### FFmpeg Transcode Command (Example: 4K HEVC → 1080p H.264 HLS)

```bash
ffmpeg \
    -analyzeduration 200M -probesize 1G \
    -fflags +genpts \
    -i "input.mkv" \
    \
    -map 0:0 -map 0:1 \
    \
    -c:v:0 h264 \
    -preset veryfast \
    -crf 23 \
    -maxrate 8000000 \
    -bufsize 16000000 \
    -vf "scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2" \
    -r 24000/1001 \
    -g 144 -keyint_min 144 \
    -sc_threshold 0 \
    -profile:v high -level 4.1 \
    -pix_fmt yuv420p \
    \
    -c:a:0 aac \
    -ac 2 \
    -b:a 192k \
    \
    -progress pipe:1 \
    \
    -f hls \
    -hls_time 6 \
    -hls_segment_type fmp4 \
    -hls_list_size 0 \
    -hls_playlist_type vod \
    -hls_segment_filename "/cache/transcodes/{session_id}/seg_%04d.m4s" \
    -y \
    "/cache/transcodes/{session_id}/manifest.m3u8"
```

Key flags explained:
- `-g 144 -keyint_min 144 -sc_threshold 0` — fixed GOP of 144 frames (6 seconds at 24fps); disables scene-change keyframes for segment-aligned boundaries
- `-preset veryfast` — balances speed vs quality for real-time transcoding; `medium` for offline transcodes
- `-crf 23 -maxrate 8M -bufsize 16M` — constrained quality (CRF) with bitrate cap for network streaming
- `-pix_fmt yuv420p` — maximum compatibility; 10-bit → 8-bit downsampling when tone mapping
- `-progress pipe:1` — structured machine-readable `key=value` progress output to stdout (separate from stderr logs); parsed by the server for real-time progress tracking and stall detection. See [MEMORY.md](MEMORY.md) for progress parsing details

---

## Hardware Acceleration

### Supported Methods

| Method | Platform | Video Encode | Video Decode | Notes |
|---|---|---|---|---|
| **NVENC/NVDEC** | x86_64 (NVIDIA GPU) | H.264, H.265, AV1 | H.264, H.265, AV1, VP9 | Best quality; consumer GPUs limited to 3 concurrent encode sessions (patchable) |
| **Intel QSV** (Quick Sync) | x86_64 (Intel iGPU/dGPU) | H.264, H.265, AV1 | H.264, H.265, AV1, VP9 | Excellent for NAS/home server; built into most Intel CPUs; 7th gen+ recommended |
| **VAAPI** | x86_64 + ARM64 (Linux) | H.264, H.265, VP9, AV1 | H.264, H.265, VP9, AV1 | Standard Linux HW accel interface; works with Intel, AMD, and some ARM GPUs |
| **VideoToolbox** | ARM64 (macOS) | H.264, H.265, ProRes | H.264, H.265, ProRes | Apple Silicon only; excellent quality per watt |
| **AMF** (Advanced Media Framework) | x86_64 (AMD GPU) | H.264, H.265, AV1 | H.264, H.265, AV1 | AMD GPU support; behind NVIDIA and Intel in quality |
| **Software fallback** | All platforms | All codecs | All codecs | CPU-only; highest quality but slowest; used when no HW accel available |

### Detection & Fallback Strategy

```rust
enum HwAccelMethod {
    Nvenc,
    Qsv,
    Vaapi,
    VideoToolbox,
    Amf,
    Software,
}

fn detect_hw_accel() -> HwAccelMethod {
    // 1. Check runtime platform (OS + architecture)
    // 2. Probe for available acceleration via FFmpeg -hwaccels
    // 3. Respect server_config.transcoding.hardware_accel setting:
    //    - "auto": use best available (NVENC > QSV > VAAPI > VideoToolbox > AMF > Software)
    //    - "nvenc"/"qsv"/"vaapi"/"videotoolbox"/"amf": force specific method
    //    - "software": force CPU-only
    // 4. Verify codec support for chosen method (not all methods support all codecs)
    // 5. Fall back to software if HW accel fails at runtime
}
```

### Hardware Acceleration in Docker

Docker containers need device passthrough for hardware acceleration:

| Method | Docker Flag | Example |
|---|---|---|
| NVIDIA | `--gpus all` or `deploy.resources.reservations.devices` | `docker run --gpus all ...` |
| Intel QSV | `--device /dev/dri` | `devices: ["/dev/dri"]` |
| VAAPI | `--device /dev/dri` | `devices: ["/dev/dri"]` |
| VideoToolbox | N/A (macOS native) | Docker Desktop on macOS passes through automatically |

### HDR → SDR Tone Mapping

When HDR content must be played on an SDR display, tone mapping converts the HDR color/brightness range to SDR. This is a video filter applied during transcoding:

```bash
# Software tone mapping (works everywhere)
-vf "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv,format=yuv420p"

# Intel QSV tone mapping (7th gen+, Linux only)
-vf "tonemap_vaapi=format=nv12:p=bt709:t=bt709:m=bt709"
```

Hardware-accelerated tone mapping is significantly faster. The server checks for HW tone mapping support during startup and uses it when available.

---

## Adaptive Bitrate (ABR) Ladder

### Default ABR Ladder (H.264)

For transcoded streams, the server generates an HLS manifest with multiple quality renditions. The client's HLS player (native or hls.js) automatically switches between renditions based on network conditions.

| Rung | Resolution | Video Bitrate | Audio | Target |
|---|---|---|---|---|
| 1 | 480p (854×480) | 1.5 Mbps | AAC stereo 128k | Mobile/cellular, slow connections |
| 2 | 720p (1280×720) | 3 Mbps | AAC stereo 160k | Tablet, moderate connections |
| 3 | 1080p (1920×1080) | 6 Mbps | AAC 5.1 256k | Desktop/laptop, good connections |
| 4 | 1080p (1920×1080) | 10 Mbps | AAC 5.1 320k | Fast local/wired connections |
| Original | Source resolution | Source bitrate | Source audio | Direct Play (no transcode) |

### Multi-Codec Ladder (Future Enhancement)

When hardware AV1 encoding is widely available (NVENC 8th gen+, Intel Arc, etc.), the server can offer AV1 renditions for significantly better compression:

| Rung | Codec | Resolution | Bitrate | Savings vs H.264 |
|---|---|---|---|---|
| 1 | AV1 | 480p | 0.8 Mbps | ~47% |
| 2 | AV1 | 720p | 1.8 Mbps | ~40% |
| 3 | AV1 | 1080p | 3.5 Mbps | ~42% |

AV1 encoding is currently too slow for real-time transcoding without hardware support. The server falls back to H.264 for transcoded streams. AV1 is only used when the source file is already AV1 and the client supports it (Direct Play).

### When ABR Is Used vs Single Rendition

- **Remote streaming with bandwidth limits**: Full ABR ladder with multiple renditions
- **Local streaming, transcoded for codec compatibility**: Single rendition at source resolution (no bandwidth constraint)
- **Direct Play**: No ABR — original file served as-is

### Smart Ladder Selection

Rather than always generating all rungs, the server is intelligent about which rungs to transcode:

- If source is 720p: skip 1080p rungs (no upscale)
- If source is 480p: only generate the 480p rung
- If client reports max resolution 720p: skip 1080p rungs
- If user's bandwidth limit is 5 Mbps: skip rungs above 5 Mbps
- If source bitrate is below a rung's bitrate: skip that rung (no upscale in transcoding)

This saves CPU time by not generating renditions that won't be used.

---

## Subtitle Handling

Subtitle handling is one of the most complex areas of media streaming because subtitle support varies wildly between clients.

The full subtitle domain design — including discovery, OCR conversion, synchronization, external provider fetching, and delivery mechanics — is documented in [SUBTITLES.md](SUBTITLES.md). The three-tier subtitle strategy (passthrough → convert → burn-in) is documented in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md).

### Subtitle Types

| Type | Format | Delivery | Direct Play | Transcode Required? |
|---|---|---|---|---|
| **Text — embedded** | SRT, ASS/SSA | Inside container | Yes (if client supports) | No |
| **Text — external** | SRT, ASS/SSA | Sidecar file | Yes (if client supports) | No |
| **Image — embedded** | PGS (Blu-ray), VobSub (DVD) | Inside container | Yes (if client supports) | Burn-in if unsupported |
| **Text — WebVTT** | WebVTT | HLS WebVTT track | Yes | No |
| **Fetched** | SRT, ASS | Downloaded from provider | Yes (converted to sidecar) | No |
| **OCR'd** | SRT | Converted from PGS/VobSub | Yes | No |

### Subtitle Delivery Strategy

```
Client requests media with subtitle selected:
    │
    ├─ TEXT SUBTITLE (SRT, ASS):
    │   ├─ DIRECT PLAY: Send subtitle file alongside media (external) or
    │   │              client reads from container (embedded)
    │   └─ HLS TRANSCODE: WebVTT sidecar track in HLS manifest
    │
    ├─ IMAGE SUBTITLE (PGS, VobSub):
    │   ├─ OCR'd SRT exists? → Deliver OCR'd SRT (no burn-in!)
    │   ├─ External SRT exists? → Deliver external SRT (no burn-in!)
    │   ├─ Client supports natively → Direct Play (embedded in container)
    │   └─ Client does NOT support → TRANSCODE: Burn into video via FFmpeg overlay filter
    │
    └─ NO SUBTITLE: No subtitle processing
```

### Subtitle Burn-In

When image subtitles (PGS, VobSub) must be burned into the video and no text alternative (OCR result or external SRT) exists:

```bash
ffmpeg -i "input.mkv" \
    -filter_complex "[0:v][0:s:0]overlay" \
    -c:v h264 -preset veryfast -crf 23 \
    -c:a copy \
    -f hls -hls_time 6 -hls_segment_type fmp4 \
    "output.m3u8"
```

Burn-in is CPU/GPU intensive because it requires full video re-encoding. The server only burns in subtitles when there is no alternative. Admin sees a QUALITY_008 warning when burn-in occurs.

### Subtitle Preference

Each user has subtitle preferences stored in `users.metadata` JSONB:
- `subtitle_mode`: "default" | "always" | "none" | "forced_only"
- `subtitle_language_preference`: preferred language code(s)
- `subtitle_prefer_external`: boolean — prefer external files over embedded

Per-item overrides in `user_item_data.metadata` JSONB:
- `subtitle_offset_ms`: per-user per-item offset in milliseconds (server applies at delivery time)
- `subtitle_language_override`: override language for this specific item
- `subtitle_track_index`: specific subtitle track to use

---

## Audio Handling

### Audio Codec Strategy

The full audio format catalog -- supported codecs, channel configurations, spatial audio (Dolby Atmos, DTS:X), container audio support, transcode targets, and downmix algorithms -- is documented in [AUDIO_FORMATS.md](AUDIO_FORMATS.md). The audio passthrough strategy is documented in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md).

| Source Audio | Client Supports | Action |
|---|---|---|
| AAC | AAC (universal) | Direct Play / copy |
| AC-3 / E-AC-3 (Dolby Digital) | AC-3 | Direct Play / copy |
| TrueHD / TrueHD Atmos | TrueHD | Direct Play (passthrough to AVR) |
| TrueHD / TrueHD Atmos | No TrueHD | Transcode to E-AC-3 or AAC (lossy) |
| DTS / DTS-HD MA | DTS | Direct Play (passthrough to AVR) |
| DTS / DTS-HD MA | No DTS | Transcode to E-AC-3 or AAC (lossy) |
| FLAC | FLAC | Direct Play / copy |
| FLAC | No FLAC | Transcode to AAC |
| Opus | Opus | Direct Play / copy |
| Opus | No Opus | Transcode to AAC |
| PCM | PCM | Direct Play / copy |
| PCM | No PCM | Transcode to FLAC or AAC |

### Audio Passthrough Principle

The server **never transcodes audio unless the client cannot decode it**. Unlike Jellyfin (which deprioritizes TrueHD for HLS), we trust the client's device profile. If the client reports TrueHD support, we pass it through — even during remux. Audio quality loss from unnecessary transcoding is unacceptable for home theater users.

### Audio Downmixing

When a multi-channel audio track must be transcoded for a stereo-only client:
- 5.1 / 7.1 → stereo: FFmpeg downmix with `-ac 2`
- Dolby Atmos metadata: lost in transcode (no Atmos passthrough in HLS without Dolby's encoder)
- Preferred transcode target: **E-AC-3** (better than AAC for surround → stereo downmix) or **AAC** (universal compatibility)

---

## Transcode Session Lifecycle

### Session Start

1. Client sends `POST /api/v1/playback/start` with `media_item_id`, `media_file_id`, device profile
2. Server builds `MediaFileInfo` from `media_files` row, `DeviceCapabilities` from client profile or conservative defaults, `NetworkConditions` from telemetry or `max_streaming_bitrate`, `DecisionEngineConfig` from `RuntimeConfig`
3. Server runs playback decision algorithm (`decision_engine::decide()`)
4. Dispatch based on `StreamDecision`:
   - **DirectPlay** — Return direct stream URL (`GET /api/v1/stream/{file_id}`); no FFmpeg needed
   - **DirectStream (remux)** — Create remux session via `start_remux_session()` with `-c:v copy -c:a copy` (stream copy, no re-encoding); spawn FFmpeg with HLS output; return HLS manifest URL
   - **Transcode** — Create transcode session via `start_session()` with full encoding pipeline; spawn FFmpeg with Landlock + seccomp sandboxing; return HLS manifest URL
5. Create `play_sessions` row (Activity domain) with session ID, stream decision, user, media item

> **Implemented** in Phase 7 Task 10 — `start_playback()` in `playback/service.rs`, `start_remux_session()` in `transcoding.rs`. See [BUILD_ORDER.md](../../BUILD_ORDER.md) Task 10 for details.

### During Playback

- Client requests HLS segments via standard HTTP GET
- Server serves segments from transcode directory
- Client sends heartbeat every 10-30 seconds (`POST /api/v1/playback/heartbeat`) with:
  - Current position in milliseconds
  - Current playback state (playing, paused, buffering)
- Server updates `user_item_data.resume_position_ms`
- Server emits `play_events` (play, pause, seek, buffer)
- FFmpeg writes segments ahead of client position

> **Implemented** in Phase 7 Task 12 — `heartbeat()` in `playback/service.rs` updates `play_sessions` metadata via JSONB `||` merge, detects state transitions (playing↔paused↔buffering) and emits corresponding `play_events`, upserts `user_item_data.resume_position_ms` with HOT-update friendly pattern. See [BUILD_ORDER.md](../../BUILD_ORDER.md) Task 12 for details.

### Seeking

1. Client sends seek request (or requests a segment far from current position)
2. Server kills current FFmpeg process
3. Server starts new FFmpeg process from seek target position (`-ss` before `-i` for fast seek)
4. Old segments before the seek point are deleted
5. Client resumes from new manifest

> **Implemented** in Phase 7 Task 12 — `seek()` in `playback/service.rs` delegates to `TranscodeManager::seek_session()` for transcoded sessions (returns new transcode session ID written to metadata and returned in `SeekResponse`); for Direct Play, updates metadata position only (client-side seek passthrough). See [BUILD_ORDER.md](../../BUILD_ORDER.md) Task 12 for details.

### Session End

1. Client sends `POST /api/v1/playback/stop` or session times out (no heartbeat for 60 seconds)
2. Server kills FFmpeg process if still running
3. Server deletes transcode directory and all segments
4. Server updates `play_sessions` with stop time, duration, percent complete
5. Server updates `user_item_data` (play_count, is_watched, clear resume if >90% complete)
6. Server removes in-memory transcode session

> **Implemented** in Phase 7 Task 12 — `stop_playback()` in `playback/service.rs` kills transcode session (if active), emits stop `play_event`, marks `play_sessions.ended_at`/`percent_complete`/`playback_duration_seconds`, upserts `user_item_data` (increments `play_count`, sets `is_watched` at 90% threshold, clears `resume_position_ms` when watched). Session heartbeat timeout (60s auto-stop background task) and paused session auto-termination deferred. See [BUILD_ORDER.md](../../BUILD_ORDER.md) Task 12 for details.

### Transcode Cleanup

- **Active sessions**: Segments deleted when session ends (stop, timeout, error)
- **Orphaned sessions**: Startup task scans `/cache/transcodes/` and deletes any directories not matching active sessions (crash recovery)
- **Disk space monitoring**: If transcode directory exceeds configured limit, oldest sessions are terminated first
- **Transcode directory**: Configured via `server_config.transcoding.transcode_path`, defaults to `/cache/transcodes/` (tmpfs in Docker for RAM-backed transcoding)

---

## Transcode Configuration

Stored in `server_config.transcoding` JSONB column. Maps to a typed Rust struct.

```rust
struct TranscodingConfig {
    hardware_accel: HwAccelMethod,
    transcode_path: String,
    max_concurrent_transcodes: u32,
    segment_duration_seconds: u32,
    allow_hw_tone_mapping: bool,
    allow_hw_subtitle_burn_in: bool,
    default_video_codec: VideoCodec,
    default_audio_codec: AudioCodec,
    max_downscale_resolution: (u32, u32),
    enable_thumb_extraction: bool,
    thread_count: u32,
    thread_type: String,
    prefer_hw_decode: bool,
}

    thread_type: `frame`,
    prefer_hw_decode: bool,
}
```

| Field | Type | Default | Description |
|---|---|---|---|
| `hardware_accel` | enum | `"auto"` | Hardware acceleration method (auto, nvenc, qsv, vaapi, videotoolbox, amf, software) |
| `transcode_path` | string | `"/cache/transcodes"` | Directory for transcode segments |
| `max_concurrent_transcodes` | u32 | `2` | Maximum simultaneous transcode sessions |
| `segment_duration_seconds` | u32 | `6` | HLS segment duration in seconds |
| `allow_hw_tone_mapping` | bool | `true` | Use GPU for HDR → SDR tone mapping when available |
| `allow_hw_subtitle_burn_in` | bool | `true` | Use GPU for subtitle burn-in overlay when available |
| `default_video_codec` | enum | `"h264"` | Default transcode video codec (h264, hevc) |
| `default_audio_codec` | enum | `"aac"` | Default transcode audio codec (aac, eac3) |
| `max_downscale_resolution` | (u32,u32) | `(3840, 2160)` | Maximum output resolution for transcoded streams |
| `enable_thumb_extraction` | bool | `true` | Generate thumbnail sprites during transcode |
| `thread_count` | u32 | `0` | FFmpeg thread count (0 = auto, based on CPU). See [CPU.md](CPU.md) for per-architecture defaults |
| `thread_type` | String | `"frame"` | FFmpeg thread_type: `"frame"`, `"slice"`, or `"slice+frame"`. Frame-only recommended for streaming. See [CPU.md](CPU.md) |
| `prefer_hw_decode` | bool | `true` | Use hardware decode even when software encode is used |

---

## API Endpoints

### Playback Control

| Method | Endpoint | Description |
|---|---|---|
| `POST` | `/api/v1/playback/start` | Start playback session; returns stream URL or HLS manifest |
| `POST` | `/api/v1/playback/heartbeat` | Update playback position and state |
| `POST` | `/api/v1/playback/stop` | End playback session |
| `POST` | `/api/v1/playback/seek` | Seek to new position (restarts transcode if needed) |
| `GET` | `/api/v1/playback/info/{session_id}` | Get current transcode/playback info |

### Segment Skip

Segment detection is documented in [SEGMENT_DETECTION.md](SEGMENT_DETECTION.md). Endpoints for retrieving and managing skippable segments:

| Method | Endpoint | Description |
|---|---|---|
| `GET` | `/api/v1/items/{id}/segments` | Get all detected segments for a media item |
| `GET` | `/api/v1/items/{id}/segments?type=intro` | Get segments of a specific type |
| `POST` | `/api/v1/items/{id}/segments` | Create a manual segment (admin/owner) |
| `PUT` | `/api/v1/items/{id}/segments/{segment_id}` | Override segment timestamps (admin/owner) |
| `DELETE` | `/api/v1/items/{id}/segments/{segment_id}` | Remove a segment (admin/owner) |
| `POST` | `/api/v1/libraries/{id}/analyze-segments` | Trigger segment analysis for a library |

### Media Streaming

| Method | Endpoint | Description |
|---|---|---|
| `GET` | `/api/v1/stream/{media_file_id}` | Direct Play — serve file with HTTP range support |
| `GET` | `/api/v1/transcode/{session_id}/manifest.m3u8` | HLS master manifest |
| `GET` | `/api/v1/transcode/{session_id}/{rendition}/index.m3u8` | HLS rendition playlist |
| `GET` | `/api/v1/transcode/{session_id}/{rendition}/seg_{num}.m4s` | HLS fMP4 segment |

### Storyboards (Seek Preview Thumbnails)

Storyboard design is documented in [STORYBOARDS.md](STORYBOARDS.md). Endpoints for retrieving and managing seek preview thumbnails:

| Method | Endpoint | Description |
|---|---|---|
| `GET` | `/api/v1/items/{id}/storyboard` | Get storyboard metadata (sprite URLs, dimensions, interval) |
| `GET` | `/api/v1/items/{id}/storyboard/index.vtt` | Serve the WebVTT index file |
| `GET` | `/api/v1/items/{id}/storyboard/{sprite}` | Serve a sprite sheet image |
| `POST` | `/api/v1/libraries/{id}/generate-storyboards` | Trigger storyboard generation for a library |
| `POST` | `/api/v1/items/{id}/generate-storyboards` | Trigger storyboard generation for a specific item |
| `DELETE` | `/api/v1/items/{id}/storyboard` | Delete cached storyboard data for an item |

### Stream URL Security

- All stream endpoints require authentication (session token or API key)
- Stream URLs contain the session ID, not the media file path — no path leakage
- Transcode segment URLs are unguessable (UUID session IDs)
- Direct play URLs validate that the authenticated user has library access
- **When `network_mode = "exposed"`:** HMAC-SHA256 signed URLs with short TTL (60s manifests, 300s segments), session-bound, 24h key rotation. Full design in [SECURITY.md](../security/SECURITY.md)

---

## Streaming Policy System

### Overview

A reusable, named policy system for controlling how users can stream media. Policies are first-class database objects (`streaming_policies` table) that define limits on concurrent streams, transcode sessions, bandwidth, resolution restrictions, and IP-based access. Users are assigned policies via `users.streaming_policy_id`; per-user override columns on the `users` table take precedence over policy values.

This is unique among self-hosted Duskcues — no other platform (Plex, Jellyfin, Emby) has reusable policy objects, separate transcode vs direct play limits, or resolution-aware transcode restrictions.

### Policy Evaluation Flow

When a user starts a new stream, the server evaluates limits in this order:

```
1. Resolve the effective policy for this user:
   ┌─────────────────────────────────────────────────────────────┐
   │  users.streaming_policy_id SET?                             │
   │    YES → Use that streaming_policies row                    │
   │    NO  → Use the policy with streaming_policies.is_default  │
   │           If no default → use server_config.transcoding     │
   └─────────────────────────────────────────────────────────────┘

2. Resolve effective limits (higher precedence wins):
   ┌─────────────────────────────────────────────────────────────┐
   │  User-level overrides (users table):                        │
   │    max_streams, max_transcode_streams, bandwidth_limit_bps  │
   │         ↓ (null values fall through)                        │
   │  Policy values (streaming_policies row):                    │
   │    max_streams, max_transcode_streams, bandwidth_limit_bps  │
   │         ↓ (null values fall through)                        │
   │  Server-level limits (server_config.transcoding):           │
   │    global_max_concurrent_streams,                           │
   │    global_max_concurrent_transcodes,                        │
   │    global_internet_upload_speed_mbps                        │
   └─────────────────────────────────────────────────────────────┘

3. Check IP restrictions from the effective policy:
   ┌─────────────────────────────────────────────────────────────┐
   │  blocked_ip_ranges contains client IP? → PLAY_011          │
   │  allowed_ip_ranges is non-empty AND                         │
   │    client IP NOT in any range?         → PLAY_011          │
   └─────────────────────────────────────────────────────────────┘

4. Check stream method restrictions from the effective policy:
   ┌─────────────────────────────────────────────────────────────┐
   │  allow_direct_play  = false AND method = direct_play?       │
   │    → reject (upgrade to direct_stream or transcode)         │
   │  allow_direct_stream = false AND method = direct_stream?    │
   │    → reject                                                 │
   │  allow_transcode = false AND method = transcode?            │
   │    → reject (must direct play)                              │
   └─────────────────────────────────────────────────────────────┘

5. Check resolution restrictions from the effective policy:
   ┌─────────────────────────────────────────────────────────────┐
   │  Source is 4K AND require_direct_play_4k = true             │
   │    AND client requests transcode? → PLAY_013               │
   │  allow_transcode_4k = false AND source is 4K               │
   │    AND method = transcode? → PLAY_013                      │
   │  max_transcode_resolution is set                             │
   │    AND target exceeds it? → clamp to max_transcode_res      │
   └─────────────────────────────────────────────────────────────┘

6. Count active sessions and enforce limits:
   ┌─────────────────────────────────────────────────────────────┐
   │  Active sessions >= effective max_streams? → PLAY_012      │
   │  Active transcodes >= effective max_transcode_streams?      │
   │    → PLAY_012                                              │
   │  Global transcodes >= global_max_concurrent_transcodes?     │
   │    → PLAY_003                                              │
   └─────────────────────────────────────────────────────────────┘

7. Apply bandwidth cap:
   ┌─────────────────────────────────────────────────────────────┐
   │  effective bandwidth_limit_bps → cap ABR rendition bitrate  │
   │  global_internet_upload_speed_mbps → cap total server WAN   │
   └─────────────────────────────────────────────────────────────┘
```

### Policy Evaluation in Code

```rust
struct ResolvedStreamingLimits {
    max_streams: Option<u32>,
    max_transcode_streams: Option<u32>,
    bandwidth_limit_bps: Option<u64>,
    allowed_methods: EnumSet<StreamMethod>,
    max_transcode_resolution: Option<Resolution>,
    allow_transcode_4k: bool,
    require_direct_play_4k: bool,
    allowed_ip_ranges: Vec<Cidr>,
    blocked_ip_ranges: Vec<Cidr>,
    auto_terminate_paused_minutes: Option<u32>,
}

async fn resolve_streaming_limits(
    user: &User,
    policy: Option<&StreamingPolicy>,
    server_config: &TranscodingConfig,
) -> ResolvedStreamingLimits {
    ResolvedStreamingLimits {
        max_streams: user.max_streams
            .or(policy.and_then(|p| p.max_streams))
            .or(server_config.global_max_concurrent_streams),
        max_transcode_streams: user.max_transcode_streams
            .or(policy.and_then(|p| p.max_transcode_streams))
            .or(server_config.global_max_concurrent_transcodes),
        bandwidth_limit_bps: user.bandwidth_limit_bps
            .or(policy.and_then(|p| p.bandwidth_limit_bps)),
        allowed_methods: policy
            .map(|p| p.allowed_methods())
            .unwrap_or(EnumSet::all()),
        max_transcode_resolution: policy
            .and_then(|p| p.max_transcode_resolution),
        allow_transcode_4k: policy
            .map(|p| p.allow_transcode_4k)
            .unwrap_or(true),
        require_direct_play_4k: policy
            .map(|p| p.require_direct_play_4k)
            .unwrap_or(false),
        allowed_ip_ranges: policy
            .map(|p| p.allowed_ip_ranges.clone())
            .unwrap_or_default(),
        blocked_ip_ranges: policy
            .map(|p| p.blocked_ip_ranges.clone())
            .unwrap_or_default(),
        auto_terminate_paused_minutes: policy
            .and_then(|p| p.auto_terminate_paused_minutes),
    }
}
```

### Paused Session Auto-Termination

When a policy sets `auto_terminate_paused_minutes`, the server monitors active sessions and terminates any that have been continuously paused for longer than the configured duration. This frees transcode resources (FFmpeg processes, segment storage) without requiring the user to manually stop playback.

The check runs as part of the session heartbeat cycle:
1. Client sends heartbeat every 60 seconds
2. Server checks if the session is paused and `paused_since` is set
3. If `now - paused_since >= auto_terminate_paused_minutes`: stop the session, emit a `play` event with `stop` reason `auto_terminated_paused`
4. The session is written to `play_sessions` with `stream_decision` metadata indicating auto-termination

### Default Seeded Policies

Five policies are seeded on first-run database initialization. They are marked `is_system = true` (cannot be deleted, but can be modified). One is marked `is_default = true` (the "Family" policy).

| Name | max_streams | max_transcode | Key Restrictions |
|---|---|---|---|
| Admin | null (unlimited) | null (unlimited) | None |
| Family | 3 | 2 | None |
| Guest | 1 | 0 | `allow_transcode: false`, `auto_terminate_paused_minutes: 30` |
| Remote Only | null | null | `blocked_ip_ranges: [RFC 1918 ranges]` |
| LAN Only | null | null | `allowed_ip_ranges: [RFC 1918 ranges]` |

The server admin can create additional custom policies and assign them to users via the admin UI. The "Family" policy is the default for new users unless the invitation specifies a different policy.

---

## Concurrency & Resource Management

### Transcode Queue

When `max_concurrent_transcodes` is reached, new transcode requests are queued rather than rejected:

```rust
struct TranscodeQueue {
    max_concurrent: u32,
    active_sessions: HashMap<Uuid, TranscodeSession>,
    waiting_queue: VecDeque<QueuedTranscode>,
}

struct QueuedTranscode {
    session_id: Uuid,
    requested_at: DateTime<Utc>,
    user_id: Uuid,
    priority: TranscodePriority,
}

enum TranscodePriority {
    High,    // Currently playing (user is waiting)
    Medium,  // Pre-buffering (upcoming in playlist)
    Low,     // Offline conversion / background task
}
```

- Active transcodes exceeding the limit are rejected with `PLAY_003` (Transcode capacity reached) and a `Retry-After` header
- Priority: currently-playing sessions > pre-buffering > background
- Admin users can override the limit (`max_streams` per user in `users` table)

### Resource Limits

| Limit | Source | Default |
|---|---|---|
| Max concurrent transcodes | `server_config.transcoding.max_concurrent_transcodes` | 2 |
| Max streams per user | `users.max_streams` | None (unlimited) |
| Bandwidth limit per user | `users.bandwidth_limit_bps` | None (unlimited) |
| Max transcode resolution | `server_config.transcoding.max_downscale_resolution` | 4K (3840×2160) |
| Transcode disk space | `server_config.transcoding.max_disk_space_mb` | 4096 MB |
| Session heartbeat timeout | Hardcoded | 60 seconds |

### FFmpeg Process Management

FFmpeg processes are managed via `tokio-process-tools` v0.11.2 — a correctness-focused subprocess library that handles process groups, graceful shutdown, bounded output, and zombie prevention:

- **Progress tracking**: FFmpeg `-progress pipe:1` writes structured `key=value` lines to stdout (`out_time_ms`, `speed`, `fps`, `bitrate`, `progress`). Dedicated consumer parses and updates session state. See [MEMORY.md](MEMORY.md) for full parsing design
- **Log collection**: stderr collected in bounded buffer for crash diagnostics; lossy delivery prevents backpressure from stalling FFmpeg
- **Graceful termination**: `GracefulShutdown` builder — Unix: SIGTERM → 10s grace → SIGKILL; Windows: CTRL_BREAK → 10s grace → TerminateProcess. Signals sent to process group (not just PID)
- **Per-process sandboxing**: Landlock (filesystem) + seccomp (syscalls) applied in child via `pre_exec`. See [SECURITY.md](../security/SECURITY.md) for full sandboxing design
- **Thread config**: `-threads` and `thread_type` configured from `CpuConfig` before spawn. Frame threading recommended for streaming. See [CPU.md](CPU.md) for per-architecture settings
- **Process priority**: `nice -n 10` (Unix) and `ionice -c 2 -n 7` (Linux) applied when enabled in `CpuConfig`. Lowers CPU and I/O scheduling priority so server API + DB always take precedence
- **CPU affinity**: On ARM64 big.LITTLE (RK3588), FFmpeg can be pinned to big cores via `cpu_affinity` config. See [CPU.md](CPU.md)
- **Zombie prevention**: tokio-process-tools panics on drop of armed handle (loud failure in dev); `terminate_on_drop` for production graceful cleanup
- **Crash recovery**: If FFmpeg exits unexpectedly, the server detects it via process monitoring and emits a `transcode_change` play event; attempts restart once, then falls back to a lower quality or rejects playback

### Architecture-Specific Encoding Notes

| Architecture | Software Codec | Recommended Preset | Notes |
|---|---|---|---|
| **x86_64** (AVX2/SSE) | x264 or x265 | `veryfast` | Both codecs well-optimized; x265 quality-per-bit is better |
| **ARM64** (NEON) | **x264 preferred** | `veryfast` (SBC) / `fast` (Apple Silicon) | x264 has extensive NEON assembly; x265 is significantly slower on ARM |
| **ARM64 NAS** (2-4 cores) | x264 | `ultrafast` | Only option for real-time 1080p on slow CPUs |
| **Apple Silicon** | x264 or VideoToolbox HW | `fast` / `medium` | VideoToolbox preferred; software fallback uses x264 |
| **RK3588** (big cores) | x264 or RKMPP HW | `veryfast` | RKMPP preferred (4x 4K transcodes at <10W); software fallback uses x264 |

---

## Streaming in Docker

### Transcode Directory

In Docker, the transcode directory is a tmpfs mount (RAM-backed) for zero-disk-wear transcoding:

```yaml
tmpfs:
  - /cache/transcodes:size=4G,mode=1777
```

This means transcode segments never touch persistent storage — they exist in RAM and vanish when the container stops. For servers with limited RAM, the transcode directory can be changed to a disk-backed volume.

### Hardware Acceleration Passthrough

Docker Compose example with Intel QSV:

```yaml
services:
  media-server:
    image: ghcr.io/org/media-server:latest
    devices:
      - /dev/dri:/dev/dri
    environment:
      DUSKCUE_TRANSCODING__HARDWARE_ACCEL: qsv
```

NVIDIA GPU:

```yaml
services:
  media-server:
    image: ghcr.io/org/media-server:latest
    deploy:
      resources:
        reservations:
          devices:
            - driver: nvidia
              count: 1
              capabilities: [gpu]
    environment:
      DUSKCUE_TRANSCODING__HARDWARE_ACCEL: nvenc
```

---

## Bandwidth & Remote Streaming

### Bandwidth Estimation

For remote streaming (WAN), the server must be aware of bandwidth constraints:

- **Server upload speed**: The server's internet upload bandwidth (configured in `server_config.network`)
- **Per-user bandwidth limit**: `users.bandwidth_limit_bps` — caps stream bitrate for specific users
- **Client-reported bandwidth**: hls.js reports measured bandwidth; the server can use this to adjust ABR rendition selection
- **Quality profile**: User-configurable maximum remote streaming quality (720p, 1080p, 4K, original)

### Remote Streaming Quality Profiles

| Profile | Max Resolution | Max Video Bitrate | Audio |
|---|---|---|---|
| Original | Source | Source bitrate | Source audio |
| 4K (60 Mbps) | 3840×2160 | 60 Mbps | E-AC-3 5.1 |
| 1080p (20 Mbps) | 1920×1080 | 20 Mbps | AAC 5.1 |
| 1080p (10 Mbps) | 1920×1080 | 10 Mbps | AAC stereo |
| 720p (4 Mbps) | 1280×720 | 4 Mbps | AAC stereo |
| 480p (2 Mbps) | 854×480 | 2 Mbps | AAC stereo |
| 480p (720 Kbps) | 854×480 | 720 Kbps | AAC stereo |

Users set their remote quality profile in their user preferences. The server respects this limit when making transcode decisions.

---

## Metrics & Observability

Transcode metrics are exposed via the existing Prometheus `/metrics` endpoint (see LOGGING_OBSERVABILITY.md):

| Metric | Type | Labels | Description |
|---|---|---|---|
| `transcode_sessions_active` | gauge | hw_accel, video_codec | Currently active transcode sessions |
| `transcode_sessions_total` | counter | hw_accel, decision (direct_play/remux/transcode) | Total playback sessions by type |
| `transcode_duration_seconds` | histogram | hw_accel, video_codec | Time from session start to first segment ready |
| `transcode_segment_write_seconds` | histogram | hw_accel | Time to write a single segment |
| `transcode_queue_depth` | gauge | — | Number of sessions waiting in queue |
| `transcode_ffmpeg_errors_total` | counter | error_type | FFmpeg process crashes by error type |
| `transcode_disk_usage_bytes` | gauge | — | Current disk usage of transcode directory |

---

## Error Codes

Streaming and transcoding errors use the existing `PLAY` domain error codes from ERROR_HANDLING.md:

| Code | HTTP | Description |
|---|---|---|
| `PLAY_001` | 404 | Media item not found |
| `PLAY_002` | 403 | User lacks library access or `play_media` capability |
| `PLAY_003` | 503 | Transcode capacity reached (max concurrent transcodes) |
| `PLAY_004` | 500 | FFmpeg process failed |
| `PLAY_005` | 409 | Session already active for this item |
| `PLAY_006` | 400 | Invalid seek position |
| `PLAY_007` | 416 | Invalid byte range for direct stream |

Additional transcode-specific error codes:

| Code | HTTP | Description |
|---|---|---|
| `PLAY_008` | 500 | Hardware acceleration initialization failed; fell back to software |
| `PLAY_009` | 500 | FFmpeg process crashed during transcode; session terminated |
| `PLAY_010` | 507 | Transcode disk space exhausted |

Streaming policy error codes:

| Code | HTTP | Description |
|---|---|---|
| `PLAY_011` | 403 | Client IP address blocked by streaming policy |
| `PLAY_012` | 429 | Per-user stream limit exceeded (max_streams or max_transcode_streams) |
| `PLAY_013` | 403 | Resolution requires direct play — transcode restricted by policy (e.g. 4K) |

---

## Research Sources

### Streaming Protocols
- CutFast — HLS vs DASH in 2026: A Plain-English Comparison (May 2026)
- Mux — The Developer's Guide to Video Encoding for Streaming (2026)
- Fora Soft — Video Encoding 101: A Beginner's Guide for 2026 (March 2025)
- TestMu AI — HLS: Browser Support, Codecs, Known Issues (May 2026)
- Apple — HTTP Live Streaming (HLS) Authoring Specification for Apple Devices

### Duskcue Architecture
- Plex Support — Direct Play, Direct Stream, Transcoding Overview (July 2021)
- Jellyfin GitHub — Dolby Vision Profile 7 Direct Play Issue #5303 (December 2025)
- Reddit r/PleX — Plex Hardware Transcoding, Explained (March 2023)
- Reddit r/PleX — Direct Play vs Transcoding differences (March 2022)
- ZimaSpace — Best Hardware for Plex Server: Transcoding, 4K, and Storage (January 2026)

### FFmpeg & Transcoding
- Mux — Adaptive Bitrate Streaming: How It Works and How to Get It Right (2026)
- FFmpeg Documentation — HLS muxer, hardware acceleration (NVENC, QSV, VAAPI, VideoToolbox)
- NVIDIA — Video Encode and Decode GPU Support Matrix

### Rust & FFmpeg Integration
- OxiMedia — Pure Rust Multimedia Framework v0.1.7 (May 2026)
- tokio-process-tools v0.11.2 — correctness-focused async subprocess library for Tokio (May 2026)
- FFmpeg `-progress` flag — structured key=value progress output; `pipe:1` for stdout separation
- landlock crate — unprivileged filesystem sandboxing via Linux LSM (Linux 5.13+)
- seccompiler crate (rust-vmm) — seccomp-BPF syscall filtering for Linux
