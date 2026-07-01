# Quality Management Domain

## Overview

This document is the authoritative design for the quality management domain — the system that ensures optimal playback quality across diverse devices and network conditions. It covers three concerns:

1. **Device capability detection** — What formats can the device play?
2. **Network quality measurement** — How fast is the connection?
3. **Transcoding decision engine** — Does this file need transcoding for this device on this network?

The goal is to maximize direct play (zero server CPU cost) while ensuring smooth playback for users on slow networks or low-performing devices.

Phase 16c offline downloads reuse these same device capability and transcoding-decision inputs, but download planning also considers package size, selected audio/subtitle tracks, mobile platform constraints, user download quality preference, and server download policy. The offline-download package and policy contract is documented in [OFFLINE_DOWNLOADS.md](OFFLINE_DOWNLOADS.md).

## Architecture

### Three-Layer Quality Decision

```
┌─────────────────────────────────────────────────────┐
│  Layer 1: Device Capability Profile (static)         │
│                                                       │
│  What formats CAN the device decode?                  │
│  Source: client report + capability wizard + database │
├─────────────────────────────────────────────────────┤
│  Layer 2: Network Quality Assessment (dynamic)        │
│                                                       │
│  What bitrate CAN the network sustain?                │
│  Source: segment telemetry + periodic probing          │
├─────────────────────────────────────────────────────┤
│  Layer 3: Transcoding Decision Engine (per request)   │
│                                                       │
│  Layer 1 + Layer 2 + media file → direct play or      │
│  transcode with optimal parameters                    │
└─────────────────────────────────────────────────────┘
```

## Device Capability Profiles

### Overview

Each client device has a capability profile that describes what video/audio/subtitle formats it can play. The profile is built from three sources (in priority order):

1. **Capability wizard results** — empirical test results from actually playing sample clips (highest confidence)
2. **Client self-report** — the client app's assessment of its own capabilities via MediaSource API, MediaCapabilities API, or platform-specific APIs
3. **Known device database** — server-side database of common devices and their known capabilities (lowest confidence, used as fallback)

### Profile Structure

The device profile captures:

| Category | Fields |
|---|---|
| **Video codecs** | H.264, H.265/HEVC, VP9, AV1 — with supported profiles (Baseline/Main/High), levels (3.0–6.1), and bit depths (8/10/12) |
| **Video features** | Max resolution, max framerate, HDR support (HDR10/HDR10+/Dolby Vision/HLG) |
| **Audio codecs** | AAC, AC3, EAC3, DTS, DTS-HD MA, TrueHD, Dolby Atmos, Opus, Vorbis, FLAC, ALAC |
| **Audio channels** | Max channel count (2.0/5.1/7.1), spatial audio support |
| **Subtitle formats** | SRT, ASS/SSA, PGS (bitmap), VobSub, WebVTT, TTML |
| **Containers** | MP4, MKV, WebM, TS |
| **Network** | Max supported bitrate (device-reported) |

### Capability Wizard

Inspired by the Jellyfin community's feature request (February 2026). Device capability self-reporting is unreliable — devices misreport, OS-level APIs differ from actual decoding ability, and TVs often support formats via USB but not via apps.

The capability wizard solves this by **empirically testing playback**:

**Test matrix (sample clips):**

| Test | Format | Purpose |
|---|---|---|
| 1 | H.264 8-bit 1080p MP4 | Baseline — must pass for any playback |
| 2 | H.264 10-bit 1080p MP4 | 10-bit H.264 support |
| 3 | HEVC 8-bit 1080p MP4 | Basic HEVC |
| 4 | HEVC 10-bit 1080p MP4 | HEVC 10-bit (common failure point) |
| 5 | HEVC 10-bit 4K HDR10 MKV | 4K + HDR + MKV container |
| 6 | AV1 8-bit 1080p MP4 | AV1 support |
| 7 | AV1 10-bit 4K MP4 | AV1 10-bit 4K |
| 8 | Dolby Vision Profile 8 MP4 | DV support |
| 9 | AAC 5.1 + AC3 + DTS audio | Audio codec support |
| 10 | PGS subtitle overlay | Subtitle rendering |

Each test plays a 5-second sample clip. The client reports:
- `success` — played without issues
- `failed` — playback error or codec not supported
- `stuttered` — played but with visible issues (possible decode performance issue)

**When the wizard runs:**
- First connection from an unknown device model → wizard offered
- Admin can trigger it for any user's device
- User can trigger it from Settings > Device > Test Capabilities
- Results are cached per device model — identical devices share results

**Sample clip storage:**
- Pre-generated 5-second clips stored in `/data/probe-clips/`
- Total storage: ~50MB for all test clips
- Generated during server setup from a blank test pattern via FFmpeg
- Clips are not user media — they're system assets

### Known Device Database

A server-side database of common device models and their known capabilities. This serves as the initial profile before the capability wizard runs. Built from:

- Public codec support tables (like Jellyfin's codec support documentation)
- Community-contributed profiles
- Manufacturer specifications

Not comprehensive — covers the most popular devices. Unknown devices fall back to a conservative baseline profile (H.264 8-bit, AAC stereo, SRT subtitles, MP4 container, 1080p max).

## Network Quality Measurement

### Two-Mechanism Approach

#### Passive Measurement (Ongoing, Zero Overhead)

Every HLS segment download is already a bandwidth measurement. The client reports:

```
POST /api/v1/playback/telemetry
{
  "session_id": "...",
  "segment_index": 42,
  "rung": "1080p-6mbps",
  "segment_bytes": 4500000,
  "download_start_ms": 1234,
  "download_end_ms": 2834,
  "buffer_seconds": 24,
  "rebuffer_count": 0,
  "rebuffer_total_ms": 0
}
```

From this, the server computes:
- **Segment throughput** = `segment_bytes / (download_end - download_start)` in bits/sec
- **Running estimate** = harmonic mean of last 5 segment throughputs (per ABR best practice)
- **Buffer health** = `buffer_seconds` — how much runway the client has

The harmonic mean is used because it's resistant to outlier segments (e.g., first segment after a seek, or a segment that was cached by a CDN edge). Throughput-based ABR (hls.js default) uses this approach.

#### Active Probing (Periodic, Low Overhead)

Every 5 minutes during active playback, the client downloads a probe file:

```
GET /api/v1/probe/bandwidth?t=1704067200
```

The probe endpoint returns a fixed-size payload (100KB). The client measures download time and reports:

```
POST /api/v1/probe/bandwidth/result
{
  "session_id": "...",
  "probe_bytes": 102400,
  "download_ms": 150,
  "estimated_throughput_bps": 5461333
}
```

Active probing validates the passive measurement and detects network changes faster than waiting for ABR segment timing. Particularly useful when:
- Playback is paused (no segment downloads happening)
- User is browsing the library (no active stream)
- Buffer is full (client stopped downloading)

**Probe cadence:**
- During active playback: every 5 minutes
- During library browsing: every 15 minutes
- While paused: every 10 minutes
- Not during network-metered connections (client detects via Network Information API)

### Network Quality Classification

The server classifies network quality into tiers based on measured throughput:

| Tier | Throughput | Quality Level | Max Sustained Rung |
|---|---|---|---|
| `excellent` | > 25 Mbps | 4K HDR direct play | All rungs |
| `good` | 10–25 Mbps | 1080p high quality | 1080p HQ (10 Mbps) |
| `moderate` | 5–10 Mbps | 1080p standard | 1080p (6 Mbps) |
| `slow` | 2–5 Mbps | 720p | 720p (3 Mbps) |
| `very_slow` | 0.5–2 Mbps | 480p | 480p (1.5 Mbps) |
| `critical` | < 0.5 Mbps | Minimal quality | Lowest rung |

Tiers are stored in the `client_network_reports` table as a snapshot. The server uses the tier to:
- Pre-select the starting ABR rung for new playback sessions
- Warn the user if their network is too slow for the selected quality
- Adjust streaming policy enforcement (e.g., don't allow 4K on slow networks)
- Generate admin analytics about user network conditions

### Network Change Detection

When throughput drops by more than 50% between consecutive measurements:
- Log a `network_degradation` trust event
- If the drop is sustained (3+ consecutive segments): notify the client to switch down
- Admin dashboard shows network degradation events per user/session

When throughput increases by more than 50%:
- Client's ABR algorithm naturally probes upward
- Server pre-caches higher-quality segments if transcode cache allows

## Transcoding Decision Engine

### Decision Flow

When a user presses Play, the server runs this decision flow:

```
┌──────────────────┐
│  Play Request     │
│  (user + device   │
│   + media item)   │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐     ┌──────────────────┐
│  Look up device   │────>│  Device Profile   │
│  profile          │     │  (capabilities)   │
└────────┬─────────┘     └──────────────────┘
         │
         ▼
┌──────────────────┐     ┌──────────────────┐
│  Look up media    │────>│  media_files      │
│  file properties  │     │  (codec, res,     │
└────────┬─────────┘     │   bitrate, HDR)   │
         │               └──────────────────┘
         ▼
┌──────────────────────────────────────────┐
│  FOR EACH stream (video, audio, subtitle):│
│                                            │
│  Can device decode the codec?              │
│    NO → mark for transcode                 │
│    YES ↓                                   │
│  Is the profile/level within device cap?   │
│    NO → mark for transcode                 │
│    YES ↓                                   │
│  Is the bit depth supported?               │
│    NO → mark for transcode                 │
│    YES ↓                                   │
│  Is the resolution within device cap?      │
│    NO → mark for transcode (downscale)     │
│    YES ↓                                   │
│  Is HDR format supported?                  │
│    NO → mark for tone-mapping              │
│    YES ↓                                   │
│  Is the container supported?               │
│    NO → mark for remux                     │
│    YES ↓                                   │
│  Is the bitrate within network capacity?   │
│    NO → mark for transcode (lower bitrate) │
│    YES ↓                                   │
│  ✅ DIRECT PLAY                            │
└──────────────────────────────────────────┘
```

### Decision Outcomes

| Outcome | CPU Cost | Quality | Description |
|---|---|---|---|
| **Direct Play** | Zero | Original | Client plays the file as-is. No server processing. |
| **Direct Stream (Remux)** | Very low | Original | Container changed (MKV→MP4) but streams are not re-encoded. ~0.1% CPU. |
| **Transcode (Video)** | High | Reduced | Video re-encoded to compatible codec/resolution/bitrate. |
| **Transcode (Audio)** | Low | Reduced | Audio re-encoded (e.g., DTS→AAC, 7.1→stereo). |
| **Burn-in Subtitles** | Moderate | Modified | Image subtitles (PGS/VobSub) or complex text (ASS) rendered onto video frames. Requires video transcode. |
| **Tone-map HDR→SDR** | High | Reduced | HDR content converted to SDR for non-HDR displays. |

### Transcode Parameter Selection

When transcoding is needed, the engine selects optimal parameters. Full codec capabilities, container formats, HDR profiles, and bit depth handling are documented in [VIDEO_FORMATS.md](VIDEO_FORMATS.md).

**Video transcode targets (in priority order):**
1. H.264 High Profile, 8-bit (universal compatibility)
2. HEVC Main 10 Profile, 10-bit (HDR-capable devices, better compression)
3. AV1 Main Profile (best compression, newest devices only — future option)

**Resolution selection:** min(device max resolution, network-capable resolution, user preference)

**Bitrate selection:** Based on ABR ladder rung that matches the network tier, with a safety factor of 0.8

**Bit depth selection:**
- HDR content on HDR-capable device → 10-bit HEVC Main 10 (preserves HDR)
- HDR content on SDR-only device → 8-bit H.264 High (after BT.2390 tone mapping)
- SDR content → 8-bit H.264 High

**HDR handling:**
- Dolby Vision Profile 7 → client-side DV fallback (if enabled) or HDR10 base layer extraction (remux)
- Dolby Vision Profile 5 → must transcode + tone-map (no HDR10 fallback)
- Dolby Vision Profile 8.1 → passthrough (HDR10-compatible)
- HDR10 → passthrough (on HDR devices) or tone-map to SDR (on SDR devices)
- HDR10+ → passthrough (on HDR10+ devices), fallback to HDR10 passthrough or tone-map
- HLG → passthrough (on HDR devices) or tone-map to SDR

**Audio transcode targets:**
- Primary: AAC (universal)
- Secondary: AC3 (for surround sound on older devices)
- Channel downmix: 7.1 → 5.1 → 2.0 based on device support

## Dolby Vision Handling

### Problem Statement

Dolby Vision Profile 7 (dual-layer DV+HDR10) is the #1 quality complaint across all Duskcues in 2025-2026. It's common in 4K Blu-ray remuxes, yet most playback devices (Android TV, Fire TV, web browsers) don't support it natively. The result: files that "played beautifully" become stuttering messes or show black screens when servers reject them from direct play.

### DV Profile Architecture

| Profile | Layers | Compatibility | Common Source |
|---|---|---|---|
| Profile 5 | Single-layer DV only | Limited (no HDR fallback) | Streaming services |
| Profile 7 | Dual-layer DV + HDR10 base | Most 4K Blu-ray remuxes | UHD Blu-ray |
| Profile 8 | Single-layer DV with HDR10/HLG fallback | Widely supported | Streaming, conversions |

Profile 7 contains **both** a DV enhancement layer (RPU + optional FEL/MEL) AND an HDR10 base layer. Most devices that can't decode DV can still decode the HDR10 base layer — the video decoder handles this automatically. The problem is that server-side logic rejects these files from direct play instead of trusting the client.

### Client-Side DV Fallback (Plex-Parity Design)

Our server implements a `allow_client_side_dv_fallback` flag in device profiles. This is the approach Plex uses with `protocol=*` — send the raw file and let the client's video decoder handle the fallback.

**Decision flow for DV Profile 7 content:**

```
File contains DV Profile 7 with HDR10 base layer?
  │
  ├─ Device reports HDR10 support AND
  │  device profile has allow_client_side_dv_fallback=true?
  │   → DIRECT PLAY (trust the client — decoder uses HDR10 base layer)
  │
  ├─ Device reports HDR10 support BUT
  │  allow_client_side_dv_fallback=false?
  │   → REMUX: strip DV enhancement layer via FFmpeg hevc_metadata=remove_dovi=1
  │   → Use fMP4 container (not MPEG-TS) to preserve audio passthrough
  │
  ├─ Device does NOT support HDR10?
  │   → TRANSCODE: tone-map HDR→SDR using BT.2390
  │   → This is expensive but necessary for SDR-only devices
  │
  └─ Device reports Dolby Vision support?
      → DIRECT PLAY (device handles DV natively)
```

**Key rules:**
- **Never transcode video just because of DV Profile 7** — remux at most (strip DV layer, keep video stream copy)
- **Never use MPEG-TS for remuxed output** — use fMP4 segments which support TrueHD passthrough in HLS
- **Default `allow_client_side_dv_fallback=true`** for all Android TV, Fire TV, and Apple TV devices — these platforms' ExoPlayer/AVFoundation handle DV→HDR10 fallback correctly
- **Default `false`** only for web browsers — browser MSE pipelines can't handle dual-layer DV

### DV Profile 7 → 8 Conversion (Future)

A future scheduled task could pre-convert DV Profile 7 files to Profile 8 using `dovi_tool`. This creates universally compatible single-layer DV files with HDR10 fallback. Not a day-one feature — the client-side fallback handles this correctly with zero processing.

## HDR→SDR Tone Mapping

### Problem Statement

When HDR content must be played on an SDR display, tone mapping quality varies wildly. Legacy algorithms (Hable, Mobius, Reinhard) produce incorrect colors — "people look orange." The industry standard is ITU-R BT.2390, which correctly preserves highlight intent and midtones.

### Tone Mapping Pipeline

```
HDR content on SDR display:
  │
  ├─ libplacebo available (Vulkan GPU)?
  │   → Use libplacebo FFmpeg filter (best quality, GPU-accelerated)
  │     -vf "hwupload=derive_device=vulkan,libplacebo=format=yuv420p:\
  │          colorspace=bt709:color_primaries=bt709:color_trc=bt709"
  │
  ├─ OpenCL available (NVIDIA/AMD/Intel GPU)?
  │   → Use tonemap_opencl with BT.2390
  │     -vf "tonemap_opencl=format=nv12:p=bt709:t=bt709:m=bt709:tonemap=bt2390:\
  │          peak=100:desat=0"
  │
  └─ Software fallback (CPU)?
      → Use CPU tonemap with BT.2390
        -vf "zscale=t=linear:npl=100,tonemap=bt2390,zscale=t=bt709:m=bt709:r=tv:p=bt709"
```

**Key rules:**
- **BT.2390 is the only acceptable tone mapping algorithm** — never default to Hable, Mobius, or Reinhard
- **libplacebo preferred** when Vulkan is available — produces the best results, GPU-accelerated
- **Peak luminance** defaults to 100 nits (standard SDR display) — configurable via `server_config.quality.tone_mapping_peak_nits`
- **Desaturation** defaults to 0 — BT.2390 handles desaturation correctly without additional parameters

### DV-Specific Tone Mapping

When tone mapping DV Profile 7 content, the DV RPU metadata can cause incorrect colors if not stripped first. The pipeline:
1. Strip DV RPU via `hevc_metadata=remove_dovi=1` (remux step)
2. Then apply BT.2390 tone mapping to the resulting HDR10 base layer

Never apply tone mapping directly to DV metadata — always strip the RPU first.

## Audio Passthrough Strategy

### Problem Statement

Jellyfin explicitly deprioritizes TrueHD and DTS for HLS streaming, forcing lossless audio to be transcoded to lossy AAC even when the client's AV receiver fully supports the original format. Plex handles this correctly by passing audio through untouched.

The full audio format catalog -- codec details, channel configurations, spatial audio (Dolby Atmos, DTS:X), container audio support, transcode targets, and downmix algorithms -- is documented in [AUDIO_FORMATS.md](AUDIO_FORMATS.md). This section covers the decision flow for audio passthrough vs transcode.

### Audio Decision Flow

```
FOR EACH audio track in the media file:
  │
  ├─ Does the device profile report this codec as supported?
  │   │
  │   ├─ YES → Pass through unmodified (zero processing)
  │   │         Even during remux, keep audio as stream copy
  │   │         Use fMP4 container which supports all audio codecs in HLS
  │   │
  │   └─ NO → Can the device handle fewer channels of the same codec?
  │       │
  │       ├─ YES → Downmix only (e.g. TrueHD 7.1 → TrueHD 5.1)
  │       │         Keep same codec, reduce channel count
  │       │
  │       └─ NO → Transcode to best supported format
  │               Priority: Opus > AAC > AC3
  │               Channel downmix: 7.1 → 5.1 → 2.0
```

### Key Rules

1. **Never deprioritize TrueHD or DTS** — Jellyfin's hardcoded deprioritization (`EncodingHelper.cs` lines 7384-7388) is the root cause of audio quality loss. Our server trusts the device profile's reported codec support.
2. **Audio passthrough during remux** — when the server remuxes video (e.g., stripping DV layer), audio is always stream-copied, never re-encoded. Use fMP4 segments which support all audio codecs.
3. **HDMI-connected AV receivers** — when the device reports audio codec support (TrueHD, DTS-HD MA, Dolby Atmos), trust it completely. The device knows what its connected receiver supports.
4. **Channel downmix preference** — prefer downmixing within the same codec (TrueHD 7.1 → TrueHD 5.1) over cross-codec transcoding (TrueHD → AAC). Downmix is fast and lossless; cross-codec is slow and lossy.
5. **Audio codec selection order for transcode** — Opus (best quality/efficiency) > AAC (universal) > AC3 (legacy surround). The server checks which the device supports.

### Container Considerations

| Container | TrueHD | DTS-HD MA | Dolby Atmos | AC3/EAC3 | AAC |
|---|---|---|---|---|---|
| MKV (direct play) | Yes | Yes | Yes | Yes | Yes |
| fMP4 (HLS segments) | Yes | Yes | Yes | Yes | Yes |
| MPEG-TS (legacy HLS) | **Unreliable** | **Unreliable** | **No** | Yes | Yes |

This is why our server uses fMP4 segments exclusively — MPEG-TS breaks TrueHD and DTS-HD MA passthrough.

## Smart Subtitle Strategy

### Problem Statement

Subtitle format incompatibility is the #1 cause of unnecessary transcoding. PGS (image-based, from Blu-rays) and ASS (styled text, from anime) force full video transcode on many devices because the server burns them into the video. But most modern clients can render text subtitles natively.

### Three-Tier Subtitle Handling

| Tier | Subtitle Type | Strategy | Server Cost | When |
|---|---|---|---|---|
| **1. Passthrough** | SRT, WebVTT, ASS (if client supports) | Deliver as-is in the stream or as sidecar | Zero | Client reports support for this format |
| **2. Convert** | ASS → SRT (if client doesn't support ASS) | Strip styling, deliver plain text | Minimal (text processing only) | Client supports text subtitles but not ASS |
| **3. Burn-in** | PGS, VobSub (if client doesn't support AND no OCR/external alternative) | Render onto video frames via FFmpeg | High (requires video transcode) | Last resort only |

### Decision Flow

```
FOR EACH subtitle track:
  │
  ├─ Text-based (SRT/WebVTT)?
  │   → Always passthrough (universally supported)
  │
  ├─ Text-based (ASS/SSA)?
  │   ├─ Client reports ASS support?
  │   │   → Passthrough (deliver as text sidecar)
  │   └─ Client does NOT support ASS?
  │       → Convert to SRT (strip styling, keep text + timing)
  │         This is text processing only — NO video transcode
  │
  └─ Image-based (PGS/VobSub)?
      ├─ OCR'd SRT exists (subtitle_ocr_cache)?
      │   → Deliver OCR'd SRT instead (no burn-in!)
      ├─ External SRT exists for same language?
      │   → Deliver external SRT instead (no burn-in!)
      ├─ Client reports PGS support?
      │   → Passthrough (deliver as embedded stream)
      └─ No alternative → Burn-in as last resort
          → Requires video transcode
            Log a QUALITY_008 warning for admin visibility
```

### Key Rules

1. **Never burn in text-based subtitles** — SRT, WebVTT, and ASS are always delivered as text. If the client can't render ASS, convert to SRT first.
2. **Burn-in is the last resort** — only for image subtitles (PGS/VobSub) on devices that can't overlay them AND when no OCR'd or external SRT alternative exists. The admin should see a warning that burn-in is occurring and consider enabling OCR or providing SRT alternatives.
3. **Subtitle format support in device profiles** — `device_profiles.subtitle_formats` stores an array of supported formats. Example: `["srt", "webvtt", "ass", "pgs"]`. Used by the decision engine.
4. **External subtitles preferred over burn-in** — if an external SRT file exists for the item, deliver that instead of burning in embedded PGS subtitles.
5. **OCR'd subtitles preferred over burn-in** — if PGS/VobSub has been OCR'd to SRT (via PaddleOCR), deliver the text result instead of burning in. See [SUBTITLES.md](SUBTITLES.md) for the full OCR pipeline.

The full subtitle domain — including OCR conversion, synchronization, external provider fetching, and delivery mechanics — is documented in [SUBTITLES.md](SUBTITLES.md).

## Intelligent Version Selection

### Problem Statement

Users with both 4K and 1080p versions of the same movie face confusing behavior: servers default to the highest resolution (4K), which wastes bandwidth and requires transcoding on slow connections. There's no automatic selection based on device or network conditions.

### Design

When multiple `media_files` rows exist for the same `media_item`, the server selects the optimal version based on:

**Selection priority:**

```
1. User preference (manual override from quality picker)
2. Device capability (can the device play 4K HDR?)
3. Network quality (can the connection sustain 4K bitrate?)
4. Efficiency (prefer 1080p source for 1080p transcode over 4K→1080p transcode)
```

**Auto-selection logic:**

| Device + Network | Selected Version | Rationale |
|---|---|---|
| 4K HDR TV on LAN (excellent network) | 4K version, direct play | Best quality, zero processing |
| 1080p TV on LAN | 1080p version, direct play | Device max is 1080p; no upscale needed |
| Phone on cellular (moderate network) | 1080p version, transcode to 720p | 1080p→720p is 4x faster than 4K→720p |
| Tablet on slow WiFi (slow network) | 1080p version, transcode to 480p | Lower-resolution source = faster transcode |
| Any device, user selected "4K" | 4K version | User explicitly chose 4K |

**Efficiency rule:** When transcoding to a lower resolution, prefer the closest source version that is ≥ target resolution. Transcoding 1080p→720p is significantly faster than 4K→720p because the decode workload is 4x smaller.

### Quality Picker UI

Present a simple quality picker like YouTube/Vimeo:

| Mode | Behavior |
|---|---|
| **Auto** (default) | Server selects optimal version based on device + network |
| **Maximum** | Always use highest quality version, direct play preferred |
| **4K** | Use 4K version if available |
| **1080p** | Use 1080p version if available |
| **720p** | Use best available version, transcode to 720p |
| **480p** | Use best available version, transcode to 480p |

The selected mode is persisted per device in `user_item_data.metadata` and used as the starting quality for future playback on that device.

## Automatic Quality Mode

### Problem Statement

Jellyfin's most glaring usability issue (4+ years, unresolved): casual users must manually select bitrates from a confusing list of numbers. Netflix "just plays." YouTube "just plays." Our server should "just play."

### Three Quality Modes

| Mode | Behavior | Target User |
|---|---|---|
| **Auto** (default) | Measures network speed, selects best quality that won't buffer. Adapts during playback via ABR. | Everyone (default) |
| **Maximum** | Always selects highest quality. May buffer on slow connections. User accepts this trade-off. | Power users on fast connections |
| **Manual** | User picks resolution: 4K / 1080p / 720p / 480p. No adaptation. | Users who know exactly what they want |

### Auto Mode Implementation

The "Auto" mode leverages our existing network quality measurement (passive + active probing from earlier in this document):

1. **New session** — use the device's last known network tier (from `client_network_reports`) as starting quality
2. **No history** — start at 1080p (safe middle ground), adapt based on first 2-3 segment downloads
3. **During playback** — the client's ABR algorithm (hls.js or native) handles quality switches based on real-time throughput
4. **After playback** — record the session's average throughput in `client_network_reports` for next session's starting point

This means returning users get the right quality immediately (no buffering during startup), while new users converge within 2-3 segments (~18 seconds).

## Quality of Experience (QoE) Metrics

Five industry-standard metrics tracked per session and aggregated per user/device/library:

| Metric | Target | Measurement |
|---|---|---|
| **Startup time** | < 2 seconds | Time from Play request to first frame rendered |
| **Rebuffer ratio** | < 0.5% | (Buffering seconds) / (Total viewing seconds) |
| **Average bitrate** | As high as network allows | Mean bitrate of played segments |
| **Switches per minute** | < 0.5 | ABR ladder switches / viewing minutes |
| **Quality drops** | < 2 per session | Downward quality switches |

These are reported by the client every 30 seconds during playback via the telemetry endpoint.

## Database Tables

Full DDL is in [DATABASE.md](DATABASE.md) — Quality Management Domain section:

- `device_profiles` — Per-device-model capability profiles
- `device_capability_tests` — Capability wizard test results
- `client_network_reports` — Per-segment and per-probe network measurements
- `qoe_reports` — Quality of experience metrics per playback session

## Configuration

Quality management configuration is stored in `server_config.quality` JSONB column. Example:

```json
{
  "capability_wizard_enabled": true,
  "network_probe_interval_minutes": 5,
  "network_probe_browsing_interval_minutes": 15,
  "network_probe_paused_interval_minutes": 10,
  "network_probe_bytes": 102400,
  "throughput_estimate_window": 5,
  "throughput_safety_factor": 0.8,
  "default_transcode_codec": "h264",
  "fallback_max_resolution": "1080p",
  "fallback_max_bitrate_bps": 6000000,
  "qoe_report_interval_seconds": 30,
  "allow_client_side_dv_fallback": true,
  "tone_mapping_algorithm": "bt2390",
  "tone_mapping_peak_nits": 100,
  "audio_passthrough_enabled": true,
  "subtitle_burn_in_policy": "last_resort",
  "default_quality_mode": "auto"
}
```

See [CONFIGURATION.md](../operations/CONFIGURATION.md) for the `QualityConfig` Rust struct.

## API Endpoints

### Device Capabilities

| Method | Endpoint | Auth Required | Description |
|---|---|---|---|
| `POST` | `/api/v1/device/capabilities` | Yes | Report device capabilities (client self-report) |
| `GET` | `/api/v1/device/capabilities` | Yes | Get current device's capability profile |
| `GET` | `/api/v1/device/capability-tests` | Yes | List capability wizard tests |
| `POST` | `/api/v1/device/capability-tests/start` | Yes | Start the capability wizard |
| `POST` | `/api/v1/device/capability-tests/{test_id}/result` | Yes | Report result of a single wizard test |

### Network Probing

| Method | Endpoint | Auth Required | Description |
|---|---|---|---|
| `GET` | `/api/v1/probe/bandwidth` | Yes | Download probe payload for bandwidth test |
| `POST` | `/api/v1/probe/bandwidth/result` | Yes | Report bandwidth probe result |

### Playback Telemetry

| Method | Endpoint | Auth Required | Description |
|---|---|---|---|
| `POST` | `/api/v1/playback/telemetry` | Yes | Report per-segment download telemetry |
| `POST` | `/api/v1/playback/qoe` | Yes | Report QoE metrics (every 30 seconds) |

### Admin Quality Dashboard

| Method | Endpoint | Auth Required | Description |
|---|---|---|---|
| `GET` | `/api/v1/admin/quality/network` | Yes (`can_manage_server`) | Network quality summary across all users |
| `GET` | `/api/v1/admin/quality/devices` | Yes (`can_manage_server`) | Device capability summary |
| `GET` | `/api/v1/admin/quality/qoe` | Yes (`can_view_analytics`) | QoE metrics across all sessions |
| `GET` | `/api/v1/admin/quality/transcodes` | Yes (`can_manage_server`) | Transcode decision breakdown (direct play % vs transcode %) |

## Error Codes

Quality management error codes are defined in [ERROR_HANDLING.md](ERROR_HANDLING.md):

| Code | HTTP | Description |
|---|---|---|
| `QUALITY_001` | 400 | Capability wizard test not found |
| `QUALITY_002` | 409 | Capability wizard already completed for this device |
| `QUALITY_003` | 400 | Invalid telemetry report |
| `QUALITY_004` | 429 | Too many telemetry reports (rate limited) |
| `QUALITY_005` | 400 | Invalid bandwidth probe result |
| `QUALITY_006` | 400 | Device profile not found |
| `QUALITY_007` | 409 | Transcode decision conflict (concurrent request) |
| `QUALITY_008` | 200 | Subtitle burn-in required (warning, not error — PGS burn-in was necessary for this device) |
| `QUALITY_009` | 400 | Unsupported tone mapping algorithm |
| `QUALITY_010` | 503 | Tone mapping unavailable (no supported algorithm for this hardware) |
| `QUALITY_011` | 400 | Invalid quality mode selection |
| `QUALITY_012` | 404 | Requested media version not found |

## Research Sources

- Fora Soft — Adaptive Bitrate Streaming Explained (May 2026): ABR algorithm families, bitrate ladder design, QoE metrics
- Reddit r/jellyfin — Client Capability Wizard Discussion (February 2026): Device misreporting problem, empirical testing proposal
- Jellyfin Documentation — Hardware Selection and Codec Support: Codec matrices, transcoding targets, device capability gaps
- webrtcHacks — Probing WebRTC Bandwidth Probing (May 2024): GCC algorithm, probe clusters, bandwidth estimation techniques
- IETF RFC 8216 — HTTP Live Streaming: HLS multi-variant playlists, ABR protocol specification
- Fora Soft — Bandwidth Estimation and Congestion Control in WebRTC (May 2026): GCC delay-based and loss-based estimators
- Reddit r/jellyfin — Building a Duskcue Without Transcoding (November 2025): PGS/TrueHD transcoding pain points on LG TVs
- Reddit r/PleX — Subtitles Skyrocketing Quality (March 2026): ASS subtitle burn-in causing quality spikes and stuttering
- Jellyfin Android TV #5073 — Dolby Vision Profile 7 HDR Fallback (October 2025): DV Profile 7 rejection regression, 18-comment thread
- Jellyfin Android TV #5303 — Cannot Match Plex Direct Play for DV Profile 7 (December 2025): Detailed comparison with Plex protocol=* behavior, TrueHD deprioritization root cause
- Jellyfin Android TV #5368 — DV+TrueHD Transcoding with Compatible Hardware (January 2026): Server transcodes TrueHD to AAC despite device support
- Reddit r/PleX — TrueHD No Longer Transcoding (April 2026): Plex EasyAudioEncoder codec corruption, Codecs folder workaround
- Reddit r/PleX — dovi_convert v6.6 (December 2025): Community tool for converting DV Profile 7 to Profile 8; FEL detection; batch processing
- Reddit r/jellyfin — Dolby Vision Transcoding Incorrect Colors (April 2026): Tone mapping produces orange skin tones on DV content
- Emby Community — HDR→SDR Tone Mapping Technically Inferior (January 2026): BT.2390 vs legacy algorithms; VFX artist comparison of Emby vs Jellyfin
- Reddit r/ffmpeg — HDR to SDR Conversion (August 2025): FFmpeg tone mapping algorithms comparison; libplacebo Vulkan approach
- Jellyfin GitHub Discussion #4795 — Quality Switcher (2022–2026): 4-year community discussion on Netflix-like automatic quality
- Firecore Infuse Community — Wrong Version from Merged Movies (February 2026): API returns MediaSources sorted by resolution highest first
- Reddit r/jellyfin — Movie with Two Versions (February 2026): Group Versions feature loses extras content

## Implementation Status

**Domain module** implemented in `server/src/domains/quality/` (Phase 7, Tasks 4–6). Five-file pattern with:

- `types.rs` — 4 Row types (`DeviceProfileRow`, `DeviceCapabilityTestRow`, `ClientNetworkReportRow`, `QoeReportRow`), 7 Request types with `validator` validation, 8 Response types including admin summaries, 3 ack response types (`TelemetryAckResponse`, `ProbeAckResponse`, `QoeAckResponse`)
- `error.rs` — `QualityError` enum with 12 variants matching QUALITY_001–012 error codes
- `service.rs` — Full implementations for capabilities, wizard, telemetry, probing, QoE, and admin summaries; `classify_network_tier()` 6-tier classification, `compute_segment_throughput()` per-segment throughput, `compute_harmonic_mean_throughput()` running estimate with configurable window
- `handlers.rs` — 13 working handlers (5 capability + 2 probe + 2 telemetry/QoE + 4 admin)
- `mod.rs` — Router with 13 routes; `QualityError` integrated into `AppError` with `quality_error_to_http()` mapping

**Decision engine** implemented in `server/src/services/decision_engine.rs` (Phase 7, Task 7). Pure shared service with:

- Input structs: `MediaFileInfo`, `DeviceCapabilities`, `NetworkConditions`, `DecisionEngineConfig` — independent of DB types for full testability
- Output struct: `PlaybackDecision` with `VideoDecision` (DirectPlay/Remux/Transcode/ToneMap/Convert/Error), `AudioDecision` (Passthrough/Transcode), `SubtitleDecision` (Passthrough/BurnIn/Convert)
- 10-factor evaluation order: quality_mode bypass → codec support → bit depth → resolution → HDR/DV → container → bitrate → manual quality cap
- Dolby Vision handling: Profile 7/8 with `allow_client_side_dv_fallback` → DirectPlay; Profile 7/8 without fallback flag → Remux (strip DV); Profile 5 → Transcode (no base layer)
- Codec alias system: `CODEC_ALIASES` static maps common aliases (avc/avc1→h264, h265→hevc, dts-hd ma→dts_hd_ma, etc.)
- Target codec selection: HEVC for 4K/10-bit, falls back to config default; audio prefers Opus→EAC3→AC3→config default
- Resolution normalization: snaps to standard tiers (2160p/1080p/720p/480p)
- Bitrate ladder: delegates to existing `TranscodeRendition::smart_ladder()` from `services/transcoding.rs`
- 21 unit tests covering all decision paths

**Not yet implemented** (Tasks 8–13): Streaming policy system, HLS manifest/segment serving, direct play/remux, HW accel detection, play session tracking, user item data.

**Phase 16a Task 10 client integration notes:**

- `clients/mobile/lib/services/quality_service.dart` is the mobile boundary for capability reporting, per-item quality preference storage, active bandwidth probes, telemetry, and QoE reporting.
- Mobile reports a conservative platform capability profile on authenticated app launch/login and after app/client version changes. The playback route sends that same profile in `POST /api/v1/playback/start`.
- `POST /api/v1/playback/start` accepts optional `quality_mode` (`auto`, `maximum`, `manual`) in addition to `max_streaming_bitrate`. Manual mobile choices map to bitrate presets and server-side resolution caps; Auto/Maximum keep the existing network/device decision behavior.
- Mobile probes `/api/v1/probe/bandwidth` during active playback and submits `/api/v1/probe/bandwidth/result`; probes skip cellular connections by default via `connectivity_plus`.
- Mobile submits heartbeat-cadenced coarse segment telemetry and 30-second QoE reports. The current Flutter `video_player` surface does not expose HLS segment request byte counts or native access logs, so exact segment download timing remains a future native Media3/AVPlayer adapter improvement.
- Desktop/web continues to submit coarse QoE through the shared web player. Deeper hls.js fragment telemetry remains a future web-player enhancement.

**Phase 16a Task 11 settings integration notes:**

- The Flutter settings route persists a device-level default quality mode through `QualityService.saveDefaultSelection`. Playback still checks per-item preferences first, then falls back to the `_default` entry.
- Full server-side quality policy administration remains web-first; mobile exposes the personal default quality setting and a copyable web settings URL for admin workflows.
