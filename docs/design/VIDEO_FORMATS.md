# Video Formats Domain

## Overview

This document is the authoritative design for the video formats domain — the comprehensive catalog of video codecs, container formats, HDR standards, bit depths, and chroma subsampling schemes that the server supports as input sources and transcode targets. It defines what the server can ingest, what it can deliver, and how it transforms between them.

The video formats domain covers five concerns:

1. **Video codecs** — Source and target codecs, profiles, levels, and hardware support
2. **Container formats** — MKV, MP4, MPEG-TS, WebM capabilities and roles
3. **HDR standards** — HDR10, HDR10+, Dolby Vision profiles, HLG, and SDR handling
4. **Bit depth and color** — 8-bit, 10-bit, 12-bit; chroma subsampling; color spaces
5. **Transcode codec matrix** — What the server transcodes to, when, and why

This document works alongside [STREAMING.md](STREAMING.md) (delivery protocol and pipeline), [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md) (transcoding decision engine and device capability detection), and [MEDIA_SCANNING.md](MEDIA_SCANNING.md) (ffprobe-based file probing during library scan).

## Video Codecs

### Supported Source Codecs (Input)

The server accepts media files encoded with any of the following codecs. Source files are never re-encoded unless the transcoding decision engine determines it is necessary (see [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md)).

| Codec | Standard | Common Profiles | Bit Depth | Typical Source | Hardware Decode |
|---|---|---|---|---|---|
| **H.264/AVC** | ITU-T H.264 (2003) | Baseline, Main, High, High 10 | 8-bit (High), 10-bit (High 10) | Web downloads, older rips, cameras | Universal (all devices since ~2005) |
| **H.265/HEVC** | ITU-T H.265 (2013) | Main, Main 10, Main 12, Rext | 8-bit, 10-bit, 12-bit | UHD Blu-ray, 4K cameras, iPhone | Broad (all devices since ~2016) |
| **AV1** | AOMedia AV1 (2018) | Main, High | 8-bit, 10-bit | Web downloads, newer rips | Mid-tier+ (2022+); Apple M3+ |
| **VP9** | Google VP9 (2013) | Profile 0 (8-bit), Profile 2 (10-bit) | 8-bit, 10-bit | YouTube downloads, WebM files | Broad (all devices since ~2017) |
| **MPEG-2** | ITU-T H.262 (1995) | Main, High | 8-bit | DVD rips, old recordings | Universal |
| **VC-1** | SMPTE 421M (2006) | Main, Advanced | 8-bit | Some Blu-ray, WMV files | Broad |
| **VP8** | Google VP8 (2008) | — | 8-bit | Older WebM files | Broad |

### Codec Details

#### H.264/AVC

The universal compatibility codec. Every client device supports H.264 decode. It is the **default transcode target** because it guarantees playback on any device.

- **Profiles we support:** Baseline (legacy), Main, High (most common), High 10 (10-bit)
- **Levels we encounter:** 3.0 (720p), 3.1 (720p high), 4.0 (1080p), 4.1 (1080p high), 4.2 (1080p 60fps), 5.0 (4K), 5.1 (4K 60fps), 5.2 (4K high bitrate)
- **Maximum resolution:** Up to 4096×2304 (Level 5.2)
- **Hardware encode:** NVENC, Intel QSV, AMD AMF, Apple VideoToolbox — all mature, fast, universally available
- **Transcode preset:** `veryfast` for live streaming (good balance of speed/quality); `medium` for offline transcoding
- **Licensing:** Single patent pool (MPEG LA / Via LA); free for internet streaming

#### H.265/HEVC

The modern quality codec. ~50% better compression than H.264 at the same visual quality. Required for 4K, HDR, and 10-bit content. It is the **preferred transcode target for modern devices** that support it.

- **Profiles we support:** Main (8-bit), Main 10 (10-bit, required for HDR), Main 12 (12-bit, source only)
- **Levels we encounter:** 3.1 (720p), 4.0 (1080p), 4.1 (1080p high), 5.0 (4K), 5.1 (4K 60fps), 5.2 (4K high), 6.0 (8K), 6.1 (8K high)
- **Tiers:** Main tier (most content), High tier (high bitrate 4K/8K)
- **Maximum resolution:** Up to 8192×4320 (8K)
- **CTU sizes:** 8×8 to 64×64 (quad-tree partitioning, vs H.264's fixed 16×16 macroblocks)
- **Hardware encode:** NVENC, Intel QSV, AMD AMF, Apple VideoToolbox — all available, 5-10% larger files than software encode
- **Key feature for HDR:** Main 10 profile is required for all HDR formats (HDR10, HDR10+, Dolby Vision, HLG)
- **Licensing:** Three patent pools (MPEG LA, HEVC Advance/Access Advance, Velos Media) — complex but irrelevant for self-hosted use; the FFmpeg encoder (libx265) and HW encoders handle licensing internally

#### AV1

The next-generation royalty-free codec. 30-50% better compression than HEVC. Adoption is accelerating but still limited in personal media libraries (most common in web downloads and streaming captures).

- **Profiles we support:** Main (8/10-bit), High (12-bit, source only)
- **Levels we encounter:** 2.0–5.1 (up to 4K 60fps), 6.0–7.3 (8K)
- **Hardware decode:** NVIDIA RTX 3000+ (NVDEC), Intel Arc/Xe, AMD RDNA3+, Apple M3+, Snapdragon 8 Gen 1+, Samsung Exynos 2200+
- **Hardware encode:** NVIDIA RTX 4000+ (NVENC AV1), Intel Arc (QSV AV1), AMD RX 7000+ (VCN AV1). Apple Silicon has AV1 decode but **no AV1 encode** as of M4.
- **Software encode:** libaom-av1 (slow, highest quality), SVT-AV1 (faster, good quality)
- **Browser support:** Chrome 70+, Firefox 67+, Edge 121+, Safari 17+ (hardware decode only on Apple)
- **Role in our server:** Source format (read), future transcode target (when HW encode is ubiquitous, ~2028+)

#### VP9

Google's royalty-free codec, predecessor to AV1. Common in YouTube downloads and WebM files. Never used as a transcode target.

- **Profiles we support:** Profile 0 (8-bit 4:2:0), Profile 2 (10-bit 4:2:0)
- **Hardware decode:** Broad (same as HEVC era hardware)
- **Hardware encode:** Limited (some Intel/AMD, no NVIDIA hardware VP9 encode)
- **Role in our server:** Source format only (read-only)

### Transcode Target Codecs

| Target Codec | When Used | Profile | Bit Depth | Hardware Encode |
|---|---|---|---|---|
| **H.264** (default) | Universal fallback; all clients; Chrome/Firefox web | High | 8-bit | NVENC, QSV, AMF, VideoToolbox |
| **H.264 HDR→SDR** | Tone-mapped output for SDR-only clients | High | 8-bit | NVENC, QSV, AMF, VideoToolbox |
| **HEVC** | Modern clients with HEVC support; HDR passthrough; 4K | Main 10 | 10-bit | NVENC, QSV, AMF, VideoToolbox |
| **AV1** (future) | Optional; royalty-free pipeline; 2028+ | Main | 8/10-bit | NVENC (RTX 4000+), QSV (Arc), VCN (RX 7000+) |

The transcoding decision engine (documented in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md)) selects the target codec based on:
1. Device capability profile (what the client supports)
2. Source content properties (HDR, resolution, bitrate)
3. Network conditions (available bandwidth)
4. Server hardware acceleration availability

**Codec selection priority for transcode output:**
```
HEVC Main 10 → if client supports HEVC AND (source is HDR OR 10-bit OR resolution > 1080p)
H.264 High   → universal fallback (always works)
AV1          → future option, not yet enabled by default
```

## Container Formats

### Supported Source Containers (Input)

| Container | Extension | Video Codecs | HDR | Dolby Vision | Multi-Audio | Multi-Subtitle | Primary Role |
|---|---|---|---|---|---|---|---|
| **Matroska** | `.mkv` | All | Yes | Yes (all profiles including 7) | Unlimited | Unlimited (SRT/ASS/PGS/VobSub) | **Primary source format** |
| **ISOBMFF/MP4** | `.mp4` `.m4v` `.mov` | All | Yes | Yes (Profiles 5, 8) | Yes | Yes (limited) | Common source; delivery container |
| **MPEG-TS** | `.ts` `.m2ts` | All | Yes | Yes (from Blu-ray) | Yes | Yes (PGS) | Blu-ray rips |
| **WebM** | `.webm` | VP8, VP9, AV1 | Limited (VP9/AV1 only) | No | Limited | Limited (WebVTT) | Web downloads |
| **AVI** | `.avi` | MPEG-4, H.264 (rare) | No | No | Limited | Limited | Legacy files |
| **FLV** | `.flv` | H.264, VP6 | No | No | Limited | No | Legacy Flash video |
| **WMV** | `.wmv` | VC-1, WMV | No | No | Limited | No | Legacy Windows Media |
| **MPG/MPEG** | `.mpg` `.mpeg` | MPEG-2, MPEG-1 | No | No | Yes | Limited | DVD rips, old recordings |
| **3GP** | `.3gp` | H.263, H.264 | No | No | Limited | No | Old mobile recordings |
| **OGV** | `.ogv` | Theora | No | No | Limited | No | Legacy open-source video |

### Container Details

#### Matroska (MKV)

The de facto standard container for personal media libraries. Virtually all media management tools, downloaders, and ripping software produce MKV files.

- **Supports all video codecs** — H.264, HEVC, AV1, VP9, MPEG-2, VC-1, and more
- **Supports all HDR formats** — HDR10, HDR10+, Dolby Vision (all profiles including Profile 7 with FEL/MEL), HLG
- **Unlimited audio tracks** — TrueHD, DTS-HD MA, FLAC, Opus, AAC, AC-3, and more
- **Unlimited subtitle tracks** — SRT, ASS/SSA, PGS, VobSub
- **Chapter support** — XML-style chapters with names and timestamps
- **Attachment support** — Fonts (for ASS subtitles), cover art
- **Flexible metadata** — Tags for title, season/episode, etc.
- **Not streamable** — MKV is not designed for HTTP streaming; must be remuxed for HLS delivery

**Our approach:** MKV is the primary input format. During direct play, MKV files are served as-is to clients that support them (VLC, mpv-based players). For HLS streaming, the server remuxes MKV to fMP4 (no re-encode, container rewrite only) or transcodes if needed.

#### ISOBMFF/MP4

The standard container for streaming and Apple ecosystem. MP4 is the delivery format for HLS.

- **HLS delivery container** — fMP4 (fragmented MP4) is used for all HLS segments (see [STREAMING.md](STREAMING.md))
- **Dolby Vision Profiles 5 and 8** — MP4 is the native container for streaming DV content
- **Dolby Vision Profile 7** — **not supported in MP4**; Profile 7 requires dual-layer (base + enhancement), which only MKV and MPEG-TS support
- **Apple compatibility** — `hvc1` codec tag required (not `hev1`); the server adds this during remux

#### MPEG-TS

The broadcast and Blu-ray container.

- **Blu-ray structure** — `.m2ts` files from BDMV directories; contains H.264 or HEVC video with PGS subtitles
- **Dolby Vision Profile 7** — dual-layer DV is native in MPEG-TS (the original UHD Blu-ray format)
- **Not used for delivery** — MPEG-TS has 2-4% overhead from 188-byte packet framing; fMP4 is more efficient for HLS

#### WebM

Google's open container for VP8/VP9/AV1 video. Rarely seen in personal media libraries but supported for completeness.

- **No HDR metadata** — VP9 Profile 2 can carry 10-bit BT.2020 content but without proper HDR10 static metadata
- **No Dolby Vision** — proprietary DV metadata is not supported in WebM
- **Limited subtitle support** — WebVTT only

### Delivery Container: fMP4

All HLS output uses **fragmented MP4 (fMP4)**. This is the only delivery container. Source containers (MKV, TS, etc.) are never sent to clients over HLS — they are always remuxed or transcoded to fMP4.

Key codec tags for MP4 compatibility:
- H.264: `avc1` (all devices)
- HEVC 8-bit: `hvc1` (Apple-compatible tag; NOT `hev1`)
- HEVC 10-bit: `hvc1` with Main 10 profile
- AV1: `av01`
- VP9: Not muxed into MP4 (transcode to H.264/HEVC instead)

## HDR Standards

### Supported HDR Formats

| Format | Type | Metadata | Bit Depth | Color Space | Backward Compatible | Source |
|---|---|---|---|---|---|---|
| **SDR** | Standard | N/A | 8-bit | BT.709 | N/A | All non-HDR content |
| **HDR10** | Open standard | Static (MaxCLL, MaxFALL, MaxMDL) | 10-bit | BT.2020 / PQ (ST.2084) | No (SDR fallback only) | UHD Blu-ray, streaming, cameras |
| **HDR10+** | Open standard (Samsung) | Dynamic (per-scene, JSON SEI) | 10-bit | BT.2020 / PQ | No | Some streaming, Samsung devices |
| **Dolby Vision Profile 5** | Proprietary (Dolby) | Dynamic RPU (per-frame) | 10/12-bit | IPTPQc2 (proprietary) | **No** (purple/green without DV decoder) | Netflix, Apple TV+, Disney+ |
| **Dolby Vision Profile 7** (MEL/FEL) | Proprietary (Dolby) | Dynamic RPU + Enhancement Layer | 10+12-bit | BT.2020 / PQ + EL | Yes (HDR10 base layer) | UHD Blu-ray only |
| **Dolby Vision Profile 8.1** | Proprietary (Dolby) | Dynamic RPU (per-frame) | 10-bit | BT.2020 / PQ | Yes (HDR10 compatible) | Streaming, most compatible DV |
| **Dolby Vision Profile 8.4** | Proprietary (Dolby) | Dynamic RPU | 10-bit | BT.2020 / HLG | Yes (HLG compatible) | Broadcast content |
| **HLG** | Open standard (BBC/NHK) | None (gamma-based) | 10-bit | BT.2020 / HLG | **Yes** (SDR compatible) | Broadcast TV |

### HDR Handling Strategy

The server's HDR handling follows the design in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md). This section provides the format-specific technical details.

#### HDR10 (Universal HDR Baseline)

HDR10 is the mandatory HDR format — every HDR file has HDR10 metadata, and every HDR-capable device supports it. It serves as the baseline for all other HDR formats.

- **Static metadata:** MaxCLL (Maximum Content Light Level), MaxFALL (Maximum Frame Average Light Level), MaxMDL (Maximum Mastering Display Luminance)
- **All HDR content includes HDR10** — even DV and HDR10+ files carry an HDR10 base layer
- **Passthrough:** Always preserve HDR10 metadata during remux (`-c:v copy`)
- **Tone mapping:** When converting HDR→SDR for non-HDR clients, use BT.2390 tone mapping via libplacebo (documented in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md))

#### Dolby Vision

Dolby Vision uses dynamic metadata (Reference Picture Unit / RPU) that adjusts tone mapping on a per-scene or per-frame basis. The server handles DV differently depending on the profile:

**Profile 5 (Streaming DV, IPTPQc2 color space):**
- Uses Dolby's proprietary IPTPQc2 color space — **no HDR10 base layer**
- **Cannot fall back to HDR10** — if the client doesn't support DV Profile 5, the server must transcode (tone map to SDR)
- No purple/green artifacts because there's no base layer to display incorrectly — it either works (DV decoder) or requires transcoding
- Common in streaming captures (Netflix, Apple TV+)

**Profile 7 (UHD Blu-ray, Dual Layer):**
- Contains an HDR10 base layer (10-bit) + an enhancement layer (MEL or FEL)
- **MEL (Minimum Enhancement Layer):** Enhancement layer contains only dynamic metadata (RPU). Functionally similar to streaming DV but with HDR10 fallback.
- **FEL (Full Enhancement Layer):** Enhancement layer contains both dynamic metadata AND additional video data, producing 12-bit output when combined with the 10-bit base layer. The extra data provides slightly smoother gradients and higher fidelity.
- **HDR10 fallback:** The base layer is standard HDR10 — any HDR-capable device can play it
- **Client-side DV fallback (already designed):** When `allow_client_side_dv_fallback` is true in the device profile, the server allows direct play of DV Profile 7 content to devices that support HDR10 (even without DV support). The client's video decoder handles the DV→HDR10 fallback by reading the base layer. See [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md).
- **Remux option:** For clients that can't handle the dual-layer stream, the server can strip the DV enhancement layer via `hevc_metadata=remove_dovi=1` and deliver HDR10-only. This is a remux (no re-encode) — very fast.

**Profile 8.1 (HDR10-compatible DV, Streaming):**
- Single-layer 10-bit HEVC with HDR10-compatible base + DV RPU metadata
- **Best DV profile for self-hosted streaming** — any HDR10 device can play the base layer; DV devices get the dynamic metadata
- The server's preferred DV format for any remuxed or transcoded output
- Conversion from Profile 7 to Profile 8.1 is possible using `dovi_tool convert` (strips FEL, keeps RPU)

**Profile 8.4 (HLG-compatible DV):**
- Single-layer with HLG base + DV RPU
- Rare in personal libraries; mostly broadcast content
- Handled the same as 8.1 but with HLG transfer characteristics

#### HDR10+ (Dynamic Metadata, Open Standard)

- Per-scene dynamic metadata carried in JSON SEI messages
- Functionally similar to Dolby Vision's dynamic metadata but open and royalty-free
- **Passthrough:** Preserve HDR10+ metadata during remux. Never strip it.
- **No tone mapping adjustment needed** — the server does not modify HDR10+ metadata during transcode; it falls back to HDR10 static metadata for the transcode output
- **Device support:** Samsung TVs, Amazon Prime Video, some Panasonic TVs. Less common than Dolby Vision but growing.

#### HLG (Hybrid Log Gamma)

- Gamma-based HDR — no metadata needed. The gamma curve itself encodes HDR information.
- **Backward compatible with SDR** — an SDR TV displays HLG content as normal SDR. An HDR TV displays the full HDR range.
- Primarily used in broadcast TV (BBC, NHK, EBU)
- Rare in personal media libraries but supported for completeness
- **Passthrough:** Always preserve HLG characteristics during remux

### HDR Detection During Scan

The ffprobe output during Phase 3 (see [MEDIA_SCANNING.md](MEDIA_SCANNING.md)) is used to detect HDR properties:

| ffprobe Field | HDR Property | Example Values |
|---|---|---|
| `color_transfer` | Transfer characteristics | `smpte2084` (PQ/HDR10), `arib-std-b67` (HLG) |
| `color_primaries` | Color gamut | `bt2020` (HDR), `bt709` (SDR) |
| `color_space` | Matrix coefficients | `bt2020nc`, `bt709` |
| `bits_per_raw_sample` | Bit depth | `8`, `10`, `12` |
| `profile` | HEVC profile | `Main 10` (indicates 10-bit) |
| Side data: `Dolby Vision` | DV profile detection | `Profile 5`, `Profile 7`, `Profile 8.1` |
| Side data: `HDR Dynamic Metadata` | HDR10+ detection | `HDR10+ Profile A` |
| Side data: `Mastering Display Metadata` | HDR10 static metadata | `G(13250,34500)B(7500...` |
| Side data: `Content Light Level Metadata` | MaxCLL/MaxFALL | `MaxCLL=1000, MaxFALL=400` |

DV profile detection requires ffprobe to read the DV RPU NAL units. The server parses these during the probe phase and stores them in `media_files.additional_streams` JSONB:

```json
{
    "dolby_vision": {
        "profile": 7,
        "level": 6,
        "compatibility_mode": "hdr10",
        "enhancement_layer": "fel"
    },
    "hdr10_plus": false
}
```

## Bit Depth and Color

### Bit Depth Support

| Bit Depth | Colors | HDR | Profile Required | Server Handling |
|---|---|---|---|---|
| **8-bit** | 16.7 million | No (SDR only) | H.264 High, HEVC Main | Passthrough or transcode to 8-bit |
| **10-bit** | 1.07 billion | Yes (HDR10, DV, HLG) | HEVC Main 10 | Passthrough or transcode to 10-bit (HDR) or 8-bit (SDR) |
| **12-bit** | 68.7 billion | Yes (DV FEL only) | HEVC Main 12 | Source only; 12-bit data is merged during transcode to 10-bit |

### Chroma Subsampling

| Format | Description | Typical Source | Server Handling |
|---|---|---|---|
| **4:2:0** | Half horizontal + half vertical chroma | Virtually all consumer video | Passthrough; transcode output always 4:2:0 |
| **4:2:2** | Half horizontal chroma | Some professional/broadcast content | Passthrough; transcode to 4:2:0 |
| **4:4:4** | Full resolution chroma | Professional, rare | Passthrough; transcode to 4:2:0 |

All transcode output uses **4:2:0** chroma subsampling. 4:2:2 and 4:4:4 are preserved during direct play/remux but downsampled during transcode.

### Color Spaces

| Color Space | Gamut Coverage | Use Case |
|---|---|---|
| **BT.709** | 35.9% of CIE 1931 | SDR content |
| **BT.2020** | 75.8% of CIE 1931 | HDR content (HDR10, DV, HLG) |
| **IPTPQc2** | Proprietary (Dolby) | DV Profile 5 only |

### Transcode Bit Depth Rules

| Source | Target (HDR client) | Target (SDR client) |
|---|---|---|
| 8-bit SDR | Passthrough (8-bit) | Passthrough (8-bit) |
| 10-bit HDR | Passthrough (10-bit) or transcode HEVC Main 10 | Tone-map + transcode H.264 High (8-bit) |
| 10-bit SDR | Passthrough (10-bit) or transcode H.264 High 10 (rare) | Transcode H.264 High (8-bit) |
| 12-bit DV FEL | Passthrough or remux to 10-bit HDR10 | Tone-map + transcode H.264 High (8-bit) |

## High Bitrate Video

### Bitrate Ranges by Content Type

| Content Type | Typical Bitrate | Maximum Encountered |
|---|---|---|
| 720p H.264 | 2-5 Mbps | 10 Mbps |
| 1080p H.264 | 5-15 Mbps | 40 Mbps |
| 1080p HEVC | 2-8 Mbps | 20 Mbps |
| 4K HEVC (streaming) | 10-20 Mbps | 40 Mbps |
| 4K HEVC (UHD Blu-ray) | 50-80 Mbps | 128 Mbps |
| 4K HEVC (remux, no compression loss) | 40-100 Mbps | 150 Mbps |
| 8K HEVC | 40-80 Mbps | 200 Mbps |

The server handles high bitrate content through:

1. **Direct play** — Serve the file as-is via HTTP range requests. No server processing. Works for all bitrates.
2. **Direct stream (remux)** — Container rewrite only. No re-encode. Preserves original bitrate. CPU cost: negligible.
3. **Transcode** — Re-encode to lower bitrate based on device capability and network conditions. The ABR ladder (documented in [STREAMING.md](STREAMING.md)) defines the output bitrate targets.

### ABR Ladder Bitrate Targets

| Rung | Resolution | Bitrate | Codec | Use Case |
|---|---|---|---|---|
| 1 | 480p | 1.5 Mbps | H.264 High | Slow networks, mobile |
| 2 | 720p | 3 Mbps | H.264 High | Moderate networks |
| 3 | 1080p | 6 Mbps | H.264 High | Good networks |
| 4 | 1080p HQ | 10 Mbps | H.264 High / HEVC Main 10 | Excellent networks, HDR |

## FFmpeg Codec Support

### Encoder Selection

| Encoder | Type | Quality | Speed | When Used |
|---|---|---|---|---|
| `libx264` | Software (H.264) | Excellent | Fast (120-180 fps 1080p) | Default software transcode |
| `libx265` | Software (HEVC) | Excellent | Slow (15-30 fps 1080p) | Software HEVC transcode |
| `h264_nvenc` | Hardware (NVIDIA) | Good (5-10% larger than SW) | Very fast (240+ fps 1080p) | NVIDIA GPU available |
| `hevc_nvenc` | Hardware (NVIDIA) | Good | Very fast | NVIDIA GPU HEVC |
| `h264_qsv` | Hardware (Intel QSV) | Good | Fast | Intel GPU available |
| `hevc_qsv` | Hardware (Intel QSV) | Good | Fast | Intel GPU HEVC |
| `h264_amf` | Hardware (AMD) | Good | Fast | AMD GPU available |
| `hevc_amf` | Hardware (AMD) | Good | Fast | AMD GPU HEVC |
| `h264_videotoolbox` | Hardware (Apple) | Good | Fast | macOS VideoToolbox |
| `hevc_videotoolbox` | Hardware (Apple) | Good | Fast | macOS VideoToolbox HEVC |
| `libsvtav1` | Software (AV1) | Good | Moderate | Future AV1 transcode |
| `av1_nvenc` | Hardware (NVIDIA AV1) | Good | Fast | Future, RTX 4000+ only |

### FFmpeg HDR Passthrough

```bash
ffmpeg -i input.mkv -c:v copy -c:a copy -tag:v hvc1 output.mp4
```

- `-c:v copy` preserves all video data including HDR metadata, DV RPU, HDR10+ SEI
- `-tag:v hvc1` sets the Apple-compatible codec tag (required for Apple devices)
- No quality loss, no re-encode — container rewrite only

### FFmpeg DV Profile 7 HDR10 Stripping (Remux)

```bash
ffmpeg -i input.mkv -c:v copy -bsf:v hevc_metadata=remove_dovi=1 -tag:v hvc1 output.mp4
```

- Removes the DV enhancement layer, delivering HDR10-only base layer
- No re-encode — still a remux
- Used when client doesn't support DV but supports HDR10

### FFmpeg HDR→SDR Tone Mapping

```bash
ffmpeg -i input.mkv \
    -vf "hwupload,tonemap_vaapi=format=nv12:p=bt709:t=bt709:m=bt709:tonemap=hable:npl=100" \
    -c:v h264_nvenc -preset p4 -tune ll \
    -c:a aac -b:a 192k \
    output.mp4
```

The actual tone mapping pipeline uses libplacebo/BT.2390 as documented in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md). The command above shows the VAAPI hardware tone mapping path as one example.

### FFmpeg 10-bit Transcode Output

```bash
ffmpeg -i input.mkv \
    -c:v hevc_nvenc -profile:v main10 -pix_fmt yuv420p10le \
    -preset p4 -b:v 6M \
    -c:a copy \
    -tag:v hvc1 \
    output.mp4
```

- `-profile:v main10` — 10-bit HEVC output (required for HDR preservation)
- `-pix_fmt yuv420p10le` — 10-bit 4:2:0 pixel format
- Preserves HDR metadata automatically when outputting HEVC Main 10

## Video Format Storage in Database

Video format properties are stored in the `media_files` table (see [DATABASE.md](DATABASE.md)):

| Column | Type | Source | Example Values |
|---|---|---|---|
| `container_format` | TEXT | ffprobe `format.format_name` | `matroska`, `mov/mp4/m4a/3gp/3g2`, `mpegts` |
| `video_codec` | TEXT | ffprobe `streams[video].codec_name` | `h264`, `hevc`, `av1`, `vp9`, `mpeg2video` |
| `video_resolution` | TEXT | ffprobe width×height | `3840x2160`, `1920x1080`, `1280x720` |
| `video_bitrate` | INT | ffprobe `streams[video].bit_rate` | `15000000`, `8000000` |
| `video_dynamic_range` | TEXT | Derived from color_transfer + DV detection | `sdr`, `hdr10`, `dolby_vision_p7`, `dolby_vision_p8.1`, `hdr10_plus`, `hlg` |
| `video_frame_rate` | NUMERIC(6,3) | ffprobe `r_frame_rate` | `23.976`, `24.000`, `29.970`, `60.000` |
| `additional_streams` | JSONB | Full ffprobe output | Contains HDR metadata, DV profile, all streams, side data |

The `video_dynamic_range` column is derived by the server during the probe phase, combining ffprobe's `color_transfer`, `color_primaries`, and DV side data into a normalized string. This allows the transcoding decision engine to query format capabilities efficiently without parsing JSONB.

## Key Decisions

1. **All four major codecs supported as source** — H.264, HEVC, AV1, VP9 plus legacy formats (MPEG-2, VC-1, VP8). The server never rejects a file based on codec.
2. **H.264 is the universal transcode target** — guaranteed playback on every device. HEVC Main 10 is the modern target for HDR-capable devices. AV1 is reserved for future use.
3. **MKV is the primary source container** — the server handles MKV's full feature set (all DV profiles, all subtitle formats, unlimited audio tracks). MP4/fMP4 is the delivery container for HLS.
4. **All HDR formats supported as source** — HDR10, HDR10+, DV Profiles 5/7/8.1/8.4, HLG. The server preserves HDR metadata during passthrough/remux and tone-maps when the client requires SDR.
5. **Dolby Vision handling is profile-aware** — Profile 7 (UHD Blu-ray) gets client-side fallback or HDR10 base layer extraction. Profile 5 (streaming) requires transcoding for non-DV clients. Profile 8.1 is the ideal streaming format (HDR10-compatible).
6. **10-bit is the HDR standard** — all HDR output uses 10-bit HEVC Main 10. 8-bit H.264 is the SDR standard. 12-bit is source-only (DV FEL), never produced by the server.
7. **All transcode output is 4:2:0** — 4:2:2 and 4:4:4 sources are downsampled during transcode. Preserved during direct play.
8. **High bitrate direct play is always supported** — the server imposes no bitrate cap on direct play. Bitrate limits only apply to transcoded output via the ABR ladder.
9. **`hvc1` codec tag for Apple compatibility** — all HEVC output uses the `hvc1` sample entry (not `hev1`) to ensure Apple device compatibility.
10. **HDR detection during scan** — ffprobe output is parsed for color_transfer, DV side data, and HDR10+ metadata during Phase 3 of the scanning pipeline. Results stored in `media_files.video_dynamic_range` and `media_files.additional_streams`.

## Relationship to Other Domains

| Domain | Relationship |
|---|---|
| **Streaming** ([STREAMING.md](STREAMING.md)) | HLS/fMP4 delivery pipeline, ABR ladder, transcode session lifecycle, FFmpeg command construction. Uses codec/container choices from this document. |
| **Quality Management** ([QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md)) | Transcoding decision engine evaluates codec, profile, bit depth, resolution, HDR, container per stream. Device capability profiles list supported video codecs. Tone mapping (HDR→SDR) pipeline. DV fallback handling. |
| **Audio Formats** ([AUDIO_FORMATS.md](AUDIO_FORMATS.md)) | Companion domain document -- audio codecs, channel configurations, spatial audio, container audio support, downmix algorithms. Container audio capabilities cross-referenced from this document. |
| **Media Scanning** ([MEDIA_SCANNING.md](MEDIA_SCANNING.md)) | ffprobe Phase 3 extracts all video format properties (codec, resolution, HDR, frame rate, bit depth). Maps to `media_files` columns. |
| **Database** ([DATABASE.md](DATABASE.md)) | `media_files` table stores video format properties. `play_session_streams` records the actual codec/HDR delivered during playback. |
| **Configuration** ([CONFIGURATION.md](../operations/CONFIGURATION.md)) | `TranscodingConfig` controls default transcode codec, hardware acceleration, downscale limits. |
| **CPU** ([CPU.md](CPU.md)) | Hardware acceleration detection (NVENC, QSV, AMF, VideoToolbox) determines which encoders are available. |

## Research Sources

- TestMu AI — AV1 Browser Support, Codecs, Hardware (May 2026): browser compatibility matrix, hardware decoder coverage, compression benchmarks
- Bitmovin — The State of AV1 Playback Support (May 2024 update): device-by-device AV1 playback testing, Apple M3/M4 hardware decoder status, dav1d software decoder on Android
- RTINGS — HDR10 vs HDR10+ vs Dolby Vision: What's The Difference? (August 2025): HDR format comparison, static vs dynamic metadata, DV Profile 5 vs Profile 7 MEL/FEL, tone mapping behavior
- CNET — HDR10 vs. Dolby Vision vs. HLG (February 2026): HDR format overview, HLG backward compatibility, content availability per format
- Medium/@mohammad.owais — H.265 Codec: Complete Guide to HEVC (March 2026): codec architecture, compression benchmarks, hardware encode/decode matrix, licensing landscape
- Compresto — HEVC Codec Explained (May 2026): encoding settings, hardware acceleration per platform, FFmpeg commands, `hvc1` vs `hev1` tag significance
- Reddit r/hometheater — Dolby Vision vs HDR10+ (February 2026): practical DV vs HDR10+ comparison, Profile 7 FEL/MEL discussion, Samsung HDR10+ vs LG Dolby Vision
- Reddit r/Dolby — Dolby Vision Profiles Explained (May 2025): Profile 0-8.4 breakdown, Profile 20 (MV-HEVC 3D), MEL vs FEL distinction
- Dolby Vision Profiles and Levels Specification v1.3.6 (April 2022): official Dolby specification for bitstream profiles
- Emby Community — Dolby Vision on Apple TV (December 2025): DV Profile 7 fallback to HDR10, MKV green tint issues, client-side DV handling
