# Audio Formats Domain

## Overview

This document is the authoritative design for the audio formats domain -- the comprehensive catalog of audio codecs, channel configurations, spatial audio formats, sample rates, bit depths, and container audio support that the server handles as input sources and transcode targets. It defines what the server can ingest, what it can deliver, and how it transforms between them.

The audio formats domain covers six concerns:

1. **Audio codecs** -- Source and target codecs, lossless vs lossy, channel limits, bitrate ranges
2. **Channel configurations** -- Stereo, surround (5.1, 7.1), and spatial audio layouts
3. **Spatial audio** -- Dolby Atmos and DTS:X object-based audio metadata
4. **Container audio support** -- Which codecs work in MKV, fMP4, MPEG-TS, and WebM
5. **Audio transcode targets** -- What the server transcodes to, when, and why
6. **Downmixing** -- Channel reduction strategies with ATSC and RFC 7845 coefficients

This document works alongside [STREAMING.md](STREAMING.md) (delivery pipeline), [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md) (transcoding decision engine and audio passthrough strategy), and [MEDIA_SCANNING.md](MEDIA_SCANNING.md) (ffprobe-based audio stream probing during library scan).

## Audio Codecs

### Supported Source Codecs (Input)

The server accepts media files containing any of the following audio codecs. Source audio is never re-encoded unless the transcoding decision engine determines it is necessary (see [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md)).

| Codec | Type | Max Channels | Max Bitrate | Max Bit Depth | Max Sample Rate | Typical Source |
|---|---|---|---|---|---|---|
| **TrueHD** | Lossless | 8 (7.1) | ~18 Mbps (variable) | 24-bit | 192 kHz | UHD Blu-ray (lossless), Dolby Atmos carrier |
| **DTS-HD MA** | Lossless | 8 (7.1) | ~24 Mbps (variable) | 24-bit | 192 kHz | UHD Blu-ray (lossless), Blu-ray |
| **FLAC** | Lossless | 8 (7.1) | ~4 Mbps (variable) | 32-bit | 655 kHz | Web downloads, music, archival |
| **ALAC** | Lossless | 8 (7.1) | ~2 Mbps (variable) | 32-bit | 384 kHz | Apple ecosystem, iTunes rips |
| **PCM/LPCM** | Uncompressed | 8 (7.1) | ~27.6 Mbps (7.1 24-bit/96kHz) | 24-bit | 192 kHz | Blu-ray, DVD, WAV files |
| **E-AC-3** | Lossy | 8 (7.1) | 6.144 Mbps | 24-bit | 48 kHz | Streaming services, Dolby Atmos carrier |
| **AC-3** | Lossy | 6 (5.1) | 640 kbps | 24-bit | 48 kHz | DVD, older Blu-ray, broadcast TV |
| **DTS** | Lossy | 6 (5.1) | 1,509 kbps | 24-bit | 48 kHz | DVD, Blu-ray core, broadcast |
| **AAC** | Lossy | 48 (7.1 practical) | ~800 kbps (typical) | 32-bit | 96 kHz | Web downloads, iTunes, universal |
| **Opus** | Lossy | 255 (7.1 practical) | 510 kbps | 24-bit | 48 kHz | WebM files, WebRTC, newer downloads |
| **Vorbis** | Lossy | 255 (7.1 practical) | ~400 kbps (typical) | 32-bit | 192 kHz | OGG files, older WebM, game audio |
| **MP3** | Lossy | 2 (stereo) | 320 kbps | 16-bit | 48 kHz | Legacy files, music libraries |

### Codec Details

#### TrueHD (Dolby TrueHD)

Dolby's lossless audio codec based on Meridian Lossless Packing (MLP). The primary lossless audio format on UHD Blu-ray. Carries Dolby Atmos object metadata as an extension sub-stream.

- **Lossless compression:** ~2:1 ratio vs PCM; bit-perfect reproduction
- **Up to 24-bit / 192 kHz** per channel
- **Up to 8 channels (7.1):** FL, FR, FC, LFE, BL, BR, SL, SR
- **Dolby Atmos carrier:** Atmos metadata (object positions, bed configuration) is embedded as extension data within the TrueHD bitstream. An Atmos-capable decoder reads this metadata to render object-based spatial audio. Without Atmos support, the decoder falls back to the channel-based 7.1 mix.
- **Seamless branching:** Supports angle changes and multi-version audio without gaps (used on Blu-ray discs with multiple cuts)
- **No licensing for playback:** Dolby licenses encoder implementations; decoder support is built into virtually all AV receivers, soundbars, and modern TVs
- **FFmpeg decoder:** `truehd` (native); FFmpeg can decode and demux TrueHD including Atmos metadata
- **Common in our library:** 4K Blu-ray remuxes -- virtually all UHD Blu-ray discs use TrueHD Atmos as the primary audio track

#### DTS-HD MA (DTS-HD Master Audio)

DTS's lossless audio codec. The other primary lossless audio format on UHD Blu-ray alongside TrueHD. Uses a core + extension architecture for backward compatibility.

- **Lossless compression:** ~2:1 ratio vs PCM; bit-perfect reproduction
- **Up to 24-bit / 192 kHz** per channel
- **Up to 8 channels (7.1)**
- **Core + extension:** A DTS-HD MA stream contains a lossy DTS core (up to 5.1, ~1.5 Mbps) that any DTS decoder can play, plus a lossless extension that adds the remaining data. Devices that only support DTS extract the core automatically -- no transcoding needed for basic playback.
- **DTS:X:** Object-based spatial audio metadata can be embedded within DTS-HD MA, similar to how Atmos rides inside TrueHD. DTS:X adds height channels and object positioning. DTS:X decoders render the spatial mix; non-DTS:X decoders fall back to the channel-based 7.1 core.
- **No licensing for playback:** DTS licenses encoder implementations; decoder support is universal in AV receivers
- **FFmpeg decoder:** `dca` (native); supports DTS, DTS-HD MA, and DTS:X decoding
- **Common in our library:** Blu-ray and UHD Blu-ray remuxes -- approximately half of Blu-ray releases use DTS-HD MA, the other half use TrueHD

#### FLAC (Free Lossless Audio Codec)

Open-source lossless codec. Excellent compression ratio and wide hardware support. Common in music libraries and web downloads.

- **Lossless compression:** ~50-70% of PCM size; bit-perfect reproduction
- **Up to 32-bit / 655 kHz** per channel
- **Up to 8 channels (7.1)**
- **Universal support:** Every modern media player, browser (via WebM/OGG), smartphone, and many AV receivers support FLAC natively
- **ReplayGain:** Supports per-track and per-album loudness normalization metadata
- **FFmpeg decoder:** `flac` (native, well-optimized)
- **Container support:** MKV, MP4 (limited), OGG, native `.flac` files

#### E-AC-3 (Dolby Digital Plus / DDP)

Dolby's successor to AC-3. Higher bitrates, more channels, and improved coding efficiency. The primary audio codec for streaming services (Netflix, Disney+, Apple TV+). Carries Dolby Atmos metadata in streaming contexts.

- **Lossy compression:** Improved efficiency over AC-3 at equivalent quality
- **Up to 7.1 channels** at up to 6.144 Mbps
- **Up to 24-bit / 48 kHz**
- **Dolby Atmos carrier:** For streaming services, Atmos metadata is embedded in E-AC-3 using the JOC (Joint Object Coding) extension. This is how Netflix and Disney+ deliver Atmos to soundbars and AV receivers without the bandwidth of TrueHD.
- **Blu-ray support:** Used on some Blu-ray discs as a secondary audio track
- **Universal support:** Virtually all TVs, AV receivers, soundbars, streaming devices, and game consoles support E-AC-3 decode
- **FFmpeg encoder:** `eac3` (native); decoder: `eac3` (native)
- **Common in our library:** Web/streaming captures, secondary audio tracks, and the target format for surround transcode output

#### AC-3 (Dolby Digital)

The legacy surround sound codec. Present on virtually every DVD and many Blu-ray discs. Still the most widely supported surround audio format.

- **Lossy compression:** Good quality at moderate bitrates
- **Up to 5.1 channels** at up to 640 kbps
- **Up to 24-bit / 48 kHz**
- **Universal support:** Every device with a digital audio output supports AC-3. The lowest common denominator for surround sound.
- **HDMI/SPDIF native:** AC-3 was designed for S/PDIF (optical/coaxial) and is native to HDMI
- **FFmpeg encoder:** `ac3` (native); decoder: `ac3` (native)
- **Common in our library:** DVD rips, secondary tracks on Blu-ray, broadcast TV captures
- **Role in our server:** Fallback transcode target for surround when AAC surround is not supported by the client's AV receiver

#### DTS (DTS Digital Surround)

The standard lossy DTS codec. Core component of DTS-HD MA (backward compatible extraction). Also used standalone on DVDs and some Blu-rays.

- **Lossy compression:** Higher bitrate than AC-3 at equivalent quality
- **Up to 5.1 channels** at up to 1,509 kbps
- **Up to 24-bit / 48 kHz**
- **Backward compatible:** The DTS core within DTS-HD MA is standard DTS. Any DTS-capable device can extract it.
- **HDMI native:** Supported by all AV receivers and most soundbars
- **FFmpeg encoder:** `dca` (native); limited encoding support -- FFmpeg can encode DTS but not DTS-HD MA
- **Common in our library:** DVD rips, core extraction from DTS-HD MA remuxes

#### AAC (Advanced Audio Coding)

The universal lossy codec. Default audio codec for MP4/M4A containers, iTunes, YouTube, and most web platforms. Best balance of quality, compatibility, and encoding speed.

- **Lossy compression:** ~25% better than MP3 at the same quality
- **Up to 48 channels** (7.1 is practical; higher channel counts rarely used)
- **Up to 32-bit / 96 kHz**
- **HE-AAC (High Efficiency):** AAC with Spectral Band Replication (SBR) for very low bitrates (24-64 kbps). Used in some streaming contexts. FFmpeg supports via `libfdk_aac` with `-profile:a aac_he`.
- **Universal support:** Every smartphone, browser, media player, smart TV, and streaming device supports AAC decode. The safest transcode target.
- **FFmpeg encoders:** `aac` (native, good quality), `libfdk_aac` (external, best quality)
- **Role in our server:** Primary transcode target for stereo output and universal compatibility fallback

#### Opus

The modern open-source lossy codec. Best quality-per-bit of any lossy codec. Built for interactive audio (WebRTC) but excellent for media streaming. Limited hardware decoder support is the main drawback.

- **Lossy compression:** Better quality than AAC at equivalent bitrates, especially below 128 kbps
- **Up to 255 channels** (7.1 is practical)
- **Up to 24-bit / 48 kHz**
- **Ultra-low latency:** Designed for real-time communication; encoding latency as low as 2.5 ms
- **Open and royalty-free:** No licensing fees for encoding or decoding
- **FFmpeg encoder:** `libopus` (excellent quality); decoder: `opus` (native)
- **Hardware support limitations:** No native hardware decode on most AV receivers, TVs, or streaming devices. Software decode only on most platforms. Apple added Opus decode in iOS 11 / macOS High Sierra. Android added Opus decode in Android 5.0. Smart TVs and soundbars generally do NOT support Opus.
- **Role in our server:** Best transcode target when the client supports it (web browsers, mobile apps). Not used for direct play to AV receivers or TVs.

#### PCM/LPCM (Pulse-Code Modulation)

Uncompressed raw audio. The highest quality and largest file size. Found in WAV files, Blu-ray PCM tracks, and DVD LPCM.

- **Uncompressed:** Zero compression; every sample stored verbatim
- **Up to 24-bit / 192 kHz** per channel
- **Up to 8 channels (7.1)**
- **HDMI native:** LPCM over HDMI is universally supported; all AV receivers accept multichannel PCM
- **FFmpeg:** `pcm_s16le`, `pcm_s24le`, `pcm_s32le` (various bit depths and endianness)
- **Common in our library:** WAV music files, Blu-ray PCM tracks (rare in personal libraries)
- **Role in our server:** Passthrough to HDMI-connected receivers (PCM is universally decoded). Transcode to FLAC for lossless size reduction when needed.

### Transcode Target Codecs

| Target Codec | When Used | Channels | Typical Bitrate | FFmpeg Encoder |
|---|---|---|---|---|
| **AAC** (default) | Universal fallback; all clients; stereo output | 2.0 or 5.1 | 128-256 kbps (stereo), 256-384 kbps (5.1) | `aac` (native) or `libfdk_aac` |
| **E-AC-3** | Surround output for devices that support DD+ but not AAC surround | 5.1 | 384-640 kbps | `eac3` (native) |
| **Opus** | Clients that support Opus (web browsers, mobile apps) | 2.0 or 5.1 | 64-128 kbps (stereo), 128-256 kbps (5.1) | `libopus` |

The transcoding decision engine (documented in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md)) selects the audio target based on:

1. Device capability profile (what audio codecs the client supports)
2. Source audio properties (channels, codec, bitrate)
3. Passthrough eligibility (can the audio be passed through unmodified?)

**Audio codec selection priority for transcode output:**
```
Opus    -> if client supports Opus (best quality/efficiency)
AAC     -> universal fallback (all clients support AAC)
E-AC-3  -> surround fallback (when client supports DD+ but not AAC 5.1)
```

## Channel Configurations

### Supported Channel Layouts

| Layout | Channels | Description | Speaker Positions |
|---|---|---|---|
| **1.0** (Mono) | 1 | Single channel | Center |
| **2.0** (Stereo) | 2 | Standard stereo | FL, FR |
| **2.1** | 3 | Stereo + subwoofer | FL, FR, LFE |
| **5.1** | 6 | Standard surround | FL, FR, FC, LFE, BL, BR (or SL, SR) |
| **7.1** | 8 | Extended surround | FL, FR, FC, LFE, BL, BR, SL, SR |
| **5.1.2** (Atmos) | 8 | 5.1 + 2 height | 5.1 + TFL, TFR (top front left/right) |
| **7.1.4** (Atmos) | 12 | 7.1 + 4 height | 7.1 + TFL, TFR, TBL, TBR |

### Channel Layout Naming

FFmpeg uses two 5.1 layouts that differ in surround channel naming:

| Layout | Surround Channels | Common Source |
|---|---|---|
| `5.1` | BL, BR (Back Left/Right) | Blu-ray, DVD |
| `5.1(side)` | SL, SR (Side Left/Right) | Some recordings, AAC/Opus default |

The server handles both transparently -- FFmpeg's downmix algorithms work with either layout.

### Bit Depth and Sample Rate

| Bit Depth | Dynamic Range | Use Case |
|---|---|---|
| **16-bit** | ~96 dB | CD quality, legacy content |
| **24-bit** | ~144 dB | Blu-ray, studio recordings, standard for lossless |
| **32-bit** (int/float) | ~192 dB (int) or unlimited (float) | Professional, FLAC, internal processing |

| Sample Rate | Use Case |
|---|---|
| **44.1 kHz** | CD quality, music |
| **48 kHz** | Video standard (Blu-ray, DVD, streaming) -- most common |
| **96 kHz** | High-resolution audio, some Blu-ray |
| **192 kHz** | Ultra high-resolution, some UHD Blu-ray |

All audio transcode output uses **48 kHz** sample rate (video standard) regardless of source sample rate. This avoids sample rate conversion artifacts during video playback and matches the expectation of every consumer playback device.

## Spatial Audio

### Dolby Atmos

Dolby Atmos is **not a codec** -- it is a spatial audio technology that adds object-based metadata to an existing audio codec. The metadata describes 3D positions (x, y, z coordinates) for individual sound objects, plus "bed" channels that map to traditional speaker layouts. An Atmos-capable renderer uses this metadata to place sounds precisely in 3D space, adapting to the listener's actual speaker configuration.

**Atmos carriers (how Atmos is delivered):**

| Carrier | Codec | Max Quality | Common Source |
|---|---|---|---|
| **TrueHD Atmos** | TrueHD + Atmos extension | Lossless 24-bit/48kHz + 128 objects | UHD Blu-ray (primary source for personal libraries) |
| **E-AC-3 Atmos** (DD+ Atmos) | E-AC-3 + JOC extension | Lossy up to 768 kbps + objects | Streaming services (Netflix, Disney+, Apple TV+) |
| **MAT (Metadata-enhanced Audio Transport)** | PCM + Atmos metadata | Lossless | Xbox, Windows (Dolby Access app) |

**How Atmos works:**

1. **Bed channels:** Traditional channel-based audio (5.1, 7.1) forms the "bed" -- ambient sounds, music, and effects mixed to specific speaker positions
2. **Audio objects:** Up to 128 individual sound elements, each with dynamic x/y/z position metadata updated per frame (typically 24-60 times per second)
3. **Renderer:** The playback device's Atmos renderer combines bed channels + objects, mapping them to the actual speaker configuration (3.1.2 soundbar, 5.1.4 home theater, 7.1.4 home theater, or binaural headphones)
4. **Fallback:** Non-Atmos devices decode the underlying TrueHD or E-AC-3 codec normally, producing the standard 7.1 or 5.1 channel mix. No data is lost -- the Atmos metadata is simply ignored.

**Our server's Atmos handling:**

- **TrueHD Atmos passthrough:** When the device reports TrueHD support (via device profile), the server passes the TrueHD Atmos stream through unmodified -- including Atmos metadata. The device's Atmos renderer handles the rest.
- **E-AC-3 Atmos passthrough:** Same approach -- if the device supports E-AC-3 (which most do), the Atmos metadata within E-AC-3 passes through.
- **Atmos metadata is never stripped or modified** -- it is opaque binary data that our server does not parse or transform. The only exception is when transcoding is necessary (client cannot decode TrueHD or E-AC-3), in which case the Atmos metadata is lost in the transcode output.
- **Atmos loss during transcode is unavoidable** -- there is no way to preserve Atmos object metadata when converting TrueHD to AAC or Opus. The transcoded output is a standard channel-based mix. This is acceptable because transcoding only happens when the client cannot decode the original codec, which means it also cannot render Atmos anyway.

### DTS:X

DTS:X is DTS's object-based spatial audio format, analogous to Dolby Atmos. It uses object metadata within DTS-HD MA streams.

- **Object-based:** Sounds are positioned in 3D space using object metadata, similar to Atmos
- **Carried within DTS-HD MA:** DTS:X metadata is embedded in DTS-HD MA streams; non-DTS:X decoders fall back to the standard 7.1 channel mix
- **No fixed speaker layout required:** DTS:X renders to any speaker configuration, adapting the spatial image to the available speakers
- **DTS:X decoders:** Found in mid-to-high-end AV receivers (Denon, Marantz, Onkyo, Yamaha)
- **Less common than Atmos:** Fewer titles use DTS:X, but it is present in some UHD Blu-ray releases
- **Our handling:** Identical to Atmos -- passthrough unmodified when the device supports DTS-HD MA; DTS:X metadata is lost during transcode (acceptable for the same reason as Atmos)

### Spatial Audio Summary

| Format | Carrier Codec | Metadata | Passthrough | Transcode Result |
|---|---|---|---|---|
| **Dolby Atmos** (TrueHD) | TrueHD | 128 objects + beds | Yes (stream copy) | Standard 7.1 channel mix (Atmos metadata lost) |
| **Dolby Atmos** (DD+) | E-AC-3 | JOC objects | Yes (stream copy) | Standard 5.1/7.1 channel mix (Atmos metadata lost) |
| **DTS:X** | DTS-HD MA | Object positions | Yes (stream copy) | Standard 7.1 channel mix (DTS:X metadata lost) |

**Key rule:** Spatial audio metadata is **always preserved during passthrough and remux**. It is **only lost when transcoding** the underlying codec (TrueHD -> AAC, DTS-HD MA -> Opus, etc.), which only happens when the client cannot decode the original codec and therefore could not render spatial audio anyway.

## Container Audio Support

### Source Container Audio Capabilities

| Audio Codec | MKV | MP4/fMP4 | MPEG-TS | WebM |
|---|---|---|---|---|
| **TrueHD** | Yes | Limited* | Unreliable | No |
| **DTS-HD MA** | Yes | No** | Yes | No |
| **DTS** | Yes | No** | Yes | No |
| **FLAC** | Yes | Yes (limited) | No | No |
| **AAC** | Yes | Yes | Yes | No |
| **AC-3** | Yes | Yes | Yes | No |
| **E-AC-3** | Yes | Yes | Yes | No |
| **Opus** | Yes | Yes | No | Yes |
| **Vorbis** | Yes | No | No | Yes |
| **PCM/LPCM** | Yes | Yes | Yes | No |
| **ALAC** | Yes | Yes | No | No |
| **MP3** | Yes | Yes | Yes | No |

\* TrueHD in MP4/fMP4 is technically possible but poorly supported by most players. Our server does not rely on MP4 TrueHD for delivery.

\*\* DTS and DTS-HD MA are not supported in the MP4 container specification. While FFmpeg can mux DTS into MP4 using private codec IDs, most players will not decode it.

### Delivery Container: fMP4

All HLS output uses fMP4 segments (see [STREAMING.md](STREAMING.md)). The server must ensure audio compatibility with the fMP4 delivery container:

**Audio codecs that can be directly muxed into fMP4 for HLS delivery:**
- AAC (universal -- works in every HLS client)
- AC-3 (widely supported in HLS clients with AV receivers)
- E-AC-3 (widely supported; carrier for Atmos in streaming contexts)
- FLAC (supported in HLS v7+ with fMP4; limited client support)
- Opus (supported in MP4 container; limited HLS client support)
- PCM/LPCM (supported in MP4 container)

**Audio codecs that CANNOT be muxed into fMP4:**
- TrueHD (not in MP4 specification)
- DTS / DTS-HD MA (not in MP4 specification)
- Vorbis (WebM only)

This means TrueHD and DTS/DTS-HD MA must be transcoded when delivered via HLS. However, if the client supports direct play (not HLS), the original container (MKV) is served as-is with the original audio codec intact.

### Container Decision Flow for Audio

```
Source audio is:
  TrueHD or DTS-HD MA?
    Client supports direct play (MKV)?
      -> Direct Play: serve MKV as-is, audio stream copy
    Client needs HLS (fMP4)?
      -> Must transcode audio: TrueHD/DTS-HD MA -> AAC/E-AC-3/Opus

  AAC, AC-3, E-AC-3, FLAC, Opus, PCM?
    Client supports this codec?
      -> Passthrough: stream copy into fMP4 or serve MKV as-is
    Client does NOT support this codec?
      -> Transcode to supported format

  Vorbis, ALAC, MP3?
    Client supports this codec?
      -> Passthrough (direct play only; Vorbis/ALAC/MP3 may not work in HLS)
    Client does NOT support?
      -> Transcode to AAC
```

## Audio Transcode Targets

### Encoder Selection Matrix

| Encoder | Type | Quality | Speed | When Used | Channels |
|---|---|---|---|---|---|
| `aac` (FFmpeg native) | Software | Good | Fast | Default stereo/surround transcode | 2.0, 5.1 |
| `libfdk_aac` | Software (external) | Excellent | Fast | Highest-quality AAC output (if available) | 2.0, 5.1 |
| `eac3` (FFmpeg native) | Software | Good | Fast | Surround output for DD+-capable devices | 5.1 |
| `libopus` | Software | Excellent | Fast | Best quality/efficiency transcode | 2.0, 5.1 |

### Audio Bitrate Targets

| Output | Codec | Bitrate | Use Case |
|---|---|---|---|
| Stereo | AAC | 128-192 kbps | Default stereo transcode |
| Stereo | Opus | 64-128 kbps | High-efficiency stereo |
| 5.1 Surround | AAC | 256-384 kbps | Universal surround |
| 5.1 Surround | E-AC-3 | 384-640 kbps | DD+ surround (AV receiver) |
| 5.1 Surround | Opus | 128-256 kbps | High-efficiency surround |

### FFmpeg Audio Transcode Commands

**TrueHD 7.1 -> AAC stereo (universal fallback):**
```bash
-c:a:0 aac -ac 2 -b:a 192k
```

**TrueHD 7.1 -> AAC 5.1 (surround preservation):**
```bash
-c:a:0 aac -ac 6 -b:a 384k
```

**DTS-HD MA 7.1 -> E-AC-3 5.1 (DD+ surround):**
```bash
-c:a:0 eac3 -ac 6 -b:a 640k
```

**TrueHD 7.1 -> Opus stereo (high-efficiency):**
```bash
-c:a:0 libopus -ac 2 -b:a 128k -vbr on
```

**TrueHD 7.1 -> Opus 5.1 (high-efficiency surround):**
```bash
-c:a:0 libopus -ac 6 -b:a 256k -vbr on
```

## Downmixing

### When Downmixing Occurs

The server downmixes audio channels when:
1. The client's device profile reports fewer channels than the source (e.g., 7.1 source, stereo-only client)
2. The transcode target codec is limited in channel count (e.g., MP3 is stereo-only)
3. The user's streaming policy limits output channels

### Downmix Priority

```
Source 7.1/5.1 -> Client supports same channel count?
  YES -> Passthrough (no downmix)
  NO  -> Client supports fewer channels of same codec?
    YES -> Same-codec downmix (e.g., TrueHD 7.1 -> TrueHD 5.1)
    NO  -> Transcode to best supported format with channel downmix
           Priority: Opus > AAC > E-AC-3
           Channels: 7.1 -> 5.1 -> 2.0
```

**Same-codec downmix is preferred** over cross-codec transcoding. Downmixing (combining channels mathematically) is fast and preserves codec quality. Cross-codec transcoding is slower and introduces generation loss.

### Downmix Algorithms

#### Standard ATSC Downmix (FFmpeg `-ac 2`)

FFmpeg's built-in `-ac 2` implements the ATSC A/52 standard downmix:

```
Lo = 1.0 * FL + 0.707 * FC + 0.707 * BL (+ 0.707 * SL for 7.1)
Ro = 1.0 * FR + 0.707 * FC + 0.707 * BR (+ 0.707 * SR for 7.1)
```

- The LFE channel (`.1`) is **discarded** by default
- Center channel mixed at -3 dB (0.707) into both stereo channels
- Surround channels mixed at -3 dB into corresponding front channels
- This is the default for all 5.1/7.1 -> stereo downmixes in our server
- Can be supplemented with `-lfe_mix_level 1.0` to include LFE content

#### LFE-Inclusive Downmix (Optional)

For users who want bass content preserved in stereo downmixes:

```bash
-ac 2 -lfe_mix_level 1.0
```

This adds the LFE channel to the stereo mix. Controlled by the device profile's `lfe_mix_enabled` flag (default: false, because most stereo speakers cannot reproduce sub-bass frequencies).

#### Surround Preservation (5.1/7.1 -> 5.1)

When downmixing 7.1 to 5.1, the side surround channels (SL, SR) are folded into the back surround channels (BL, BR):

```bash
-ac 6 -request_channel_layout 5.1
```

FFmpeg handles this automatically with `-ac 6`.

### Downmix Audio Quality

Downmix quality depends on the source codec:

| Source | Downmix Quality | Notes |
|---|---|---|
| TrueHD / FLAC / PCM | Perfect | Lossless source; downmix is mathematically precise |
| DTS-HD MA | Perfect | Lossless source; downmix is mathematically precise |
| AAC / E-AC-3 / AC-3 | Good | Lossy source; downmix is clean but generation loss accumulates |
| Opus | Good | Lossy source; Opus handles downmix well internally |

## Audio Detection During Scan

The ffprobe output during Phase 3 (see [MEDIA_SCANNING.md](MEDIA_SCANNING.md)) is used to detect audio properties:

| ffprobe Field | Audio Property | Example Values |
|---|---|---|
| `streams[audio].codec_name` | Audio codec | `truehd`, `dts`, `flac`, `aac`, `ac3`, `eac3`, `opus` |
| `streams[audio].channels` | Channel count | `2`, `6` (5.1), `8` (7.1) |
| `streams[audio].channel_layout` | Channel layout | `stereo`, `5.1`, `5.1(side)`, `7.1` |
| `streams[audio].bit_rate` | Audio bitrate | `3513000`, `640000`, `192000` |
| `streams[audio].sample_rate` | Sample rate | `48000`, `44100`, `96000` |
| `streams[audio].bits_per_raw_sample` | Bit depth | `16`, `24`, `32` |
| `streams[audio].profile` | Codec profile | `DTS-HD MA`, `DTS`, `HE-AAC` |
| Side data: `Dolby Atmos` | Atmos detection | Present / absent |
| `streams[audio].language` | Language tag | `eng`, `fre`, `jpn` |

Audio properties are stored in `media_files` columns and `additional_streams` JSONB:

| Column | Type | Source | Example Values |
|---|---|---|---|
| `audio_codec` | TEXT | ffprobe `codec_name` | `truehd`, `dts`, `aac`, `eac3` |
| `audio_channels` | INT | ffprobe `channels` | `2`, `6`, `8` |
| `audio_language` | TEXT | ffprobe `language` | `eng`, `fre`, `und` |
| `audio_bitrate` | INT | ffprobe `bit_rate` | `3513000`, `640000` |
| `additional_streams` | JSONB | Full ffprobe output | Contains channel layout, sample rate, bit depth, Atmos detection, all audio tracks |

Dolby Atmos detection is derived from ffprobe side data. The server parses this during the probe phase and stores it in `media_files.additional_streams` JSONB:

```json
{
    "audio": [
        {
            "index": 1,
            "codec": "truehd",
            "channels": 8,
            "channel_layout": "7.1",
            "sample_rate": 48000,
            "bit_depth": 24,
            "bit_rate": 3513000,
            "language": "eng",
            "dolby_atmos": true,
            "is_default": true
        },
        {
            "index": 2,
            "codec": "ac3",
            "channels": 6,
            "channel_layout": "5.1(side)",
            "sample_rate": 48000,
            "bit_rate": 640000,
            "language": "eng",
            "is_default": false
        }
    ]
}
```

## Audio Format Storage in Database

Audio format properties are stored in the `media_files` table (see [DATABASE.md](DATABASE.md)):

| Column | Type | Source | Example Values |
|---|---|---|---|
| `audio_codec` | TEXT | ffprobe `codec_name` | `truehd`, `dts`, `aac`, `eac3`, `flac`, `opus` |
| `audio_channels` | INT | ffprobe `channels` | `2`, `6`, `8` |
| `audio_language` | TEXT | ffprobe `language` | `eng`, `fre` |
| `audio_bitrate` | INT | ffprobe `bit_rate` | `3513000`, `640000` |
| `additional_streams` | JSONB | Full ffprobe output | Contains all audio tracks, Atmos detection, channel layouts, sample rates, bit depths |

The `play_session_streams` table (see [DATABASE.md](DATABASE.md)) records the actual audio codec, channels, bitrate, and language delivered during playback for analytics.

## Key Decisions

1. **All major audio codecs supported as source** -- TrueHD, DTS-HD MA, FLAC, ALAC, PCM, E-AC-3, AC-3, DTS, AAC, Opus, Vorbis, MP3. The server never rejects a file based on audio codec.
2. **Passthrough-first for all lossless audio** -- TrueHD, DTS-HD MA, FLAC are passed through unmodified whenever the device reports support. Never deprioritize lossless audio for HLS (unlike Jellyfin). See [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md).
3. **AAC is the universal transcode target** -- guaranteed playback on every device. E-AC-3 is the surround transcode target for AV receivers. Opus is the efficiency target for web/mobile.
4. **Same-codec downmix preferred over cross-codec transcode** -- TrueHD 7.1 -> TrueHD 5.1 is preferred over TrueHD 7.1 -> AAC 5.1. Downmix is fast and lossless; cross-codec transcode is slow and lossy.
5. **Spatial audio metadata is always preserved during passthrough** -- Dolby Atmos (TrueHD/E-AC-3) and DTS:X metadata pass through untouched. Metadata is only lost during transcode, which only happens when the client cannot decode the original codec (and thus could not render spatial audio anyway).
6. **fMP4 requires audio transcoding for TrueHD/DTS** -- TrueHD and DTS/DTS-HD MA cannot be muxed into fMP4 for HLS delivery. When HLS is required and the client does not support direct play, these codecs must be transcoded to AAC, E-AC-3, or Opus.
7. **MKV supports all audio codecs** -- the server handles MKV's full audio feature set (TrueHD Atmos, DTS-HD MA, DTS:X, FLAC, unlimited tracks). For direct play, MKV is always served as-is.
8. **48 kHz is the standard output sample rate** -- all transcoded audio uses 48 kHz regardless of source sample rate. Avoids sample rate conversion artifacts during video playback.
9. **ATSC standard downmix** -- 5.1/7.1 -> stereo downmix uses the ATSC A/52 standard coefficients via FFmpeg `-ac 2`. LFE is discarded by default (most stereo speakers cannot reproduce sub-bass).
10. **Multi-track audio support** -- media files with multiple audio tracks (different languages, different codecs) are fully supported. The transcoding decision engine evaluates each track independently. The user's language preference selects the default track.

## Relationship to Other Domains

| Domain | Relationship |
|---|---|
| **Streaming** ([STREAMING.md](STREAMING.md)) | HLS/fMP4 delivery pipeline, transcode session lifecycle, FFmpeg audio command construction. Uses audio codec choices from this document. Audio handling section defines codec strategy and downmix behavior. |
| **Quality Management** ([QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md)) | Transcoding decision engine evaluates audio codec, channels, and bitrate per stream. Device capability profiles list supported audio codecs. Audio passthrough strategy. |
| **Video Formats** ([VIDEO_FORMATS.md](VIDEO_FORMATS.md)) | Companion domain document -- video codecs, containers, HDR. Container audio support cross-referenced from this document. |
| **Media Scanning** ([MEDIA_SCANNING.md](MEDIA_SCANNING.md)) | ffprobe Phase 3 extracts all audio format properties (codec, channels, bitrate, sample rate, bit depth, language, Atmos detection). Maps to `media_files` columns. |
| **Database** ([DATABASE.md](DATABASE.md)) | `media_files` table stores audio format properties. `play_session_streams` records the actual audio codec/channels/bitrate delivered during playback. |
| **Configuration** ([CONFIGURATION.md](../operations/CONFIGURATION.md)) | `TranscodingConfig` controls default audio transcode codec (`aac` or `eac3`). `QualityConfig.audio_passthrough_enabled` controls whether passthrough is allowed. |

## Research Sources

- Dolby Professional -- Dolby Atmos Documentation: object-based audio, bed channels, Atmos objects, rendering pipeline, fold-down behavior, TrueHD and E-AC-3 carrier formats
- HandBrake Documentation -- Container Formats: MP4 and MKV audio codec support matrix (AAC, AC3, E-AC-3, TrueHD, FLAC, DTS, Opus, Vorbis, ALAC)
- Wikipedia -- Comparison of Video Container Formats: comprehensive container audio codec support table, overhead comparison, VBR/VFR support
- Super User -- Properly Downmix 5.1 to Stereo Using FFmpeg: ATSC A/52 standard downmix coefficients, FFmpeg `-ac 2` implementation analysis, LFE channel handling, RFC 7845 Opus downmix coefficients for 5.1, 6.1, and 7.1
- Jellyfin AndroidTV Issue #5168 -- DTS-HD MA Passthrough Regression on NVIDIA Shield (November 2025): DTS-HD MA being transcoded to AAC despite device support, TrueHD passthrough regression, root cause in ExoPlayer and HLS container limitations, server-side `EncodingHelper.cs` audio codec allowlist
- Reddit r/PleX -- Convert MKV to MP4 With Dolby Atmos and Vision (July 2024): MP4 container limitations for TrueHD Atmos, DD+ vs TrueHD Atmos carriers, container swap quality impact, FFmpeg container conversion commands
