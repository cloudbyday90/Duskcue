# Subtitle Domain

## Overview

This document is the authoritative design for the subtitle domain — the system that discovers, converts, synchronizes, fetches, and delivers subtitles for all media items. Subtitle handling is one of the most complex areas of media streaming because subtitle support varies wildly between clients, and subtitle format incompatibility is the #1 cause of unnecessary transcoding.

The subtitle domain covers five concerns:

1. **Subtitle discovery** — Finding embedded and external subtitle files during library scan
2. **Subtitle conversion** — OCR (image→text) and text format conversion (ASS→SRT) to eliminate burn-in
3. **Subtitle synchronization** — Server-side offset correction, FPS rate adjustment, and voice activity alignment
4. **Subtitle fetching** — Auto-download from external providers (OpenSubtitles) based on server settings
5. **Subtitle delivery** — Serving the right subtitle in the right format to each client with zero client-side processing

The guiding principle: **the server does all the work, the client just displays subtitles.** All sync, conversion, and format decisions happen server-side. Clients — especially low-power devices — never perform subtitle processing.

## Subtitle Format Support Matrix

| Format | Type | Source | Storage | Delivery | OCR Target |
|---|---|---|---|---|---|
| **SRT** | Text | External, embedded, fetched, OCR output | Original file | WebVTT sidecar (HLS) or text sidecar (direct play) | N/A |
| **WebVTT** | Text | Generated from SRT/ASS for HLS delivery | Generated on-the-fly | HLS sidecar (`#EXT-X-TEXT-STREAM`) | N/A |
| **ASS/SSA** | Text (styled) | External, embedded | Original file | Text sidecar (if client supports) or convert→SRT | N/A |
| **PGS (.sup)** | Image (bitmap) | Embedded (Blu-ray) | Original file | Burn-in (last resort) or OCR→SRT | **Yes → SRT via PaddleOCR** |
| **VobSub (.sub+.idx)** | Image (bitmap) | Embedded (DVD) | Original file | Burn-in (last resort) or OCR→SRT | **Yes → SRT via PaddleOCR** |
| **TTML** | Text (XML) | Rare, streaming services | Convert→SRT on ingestion | WebVTT sidecar | N/A |

## Subtitle Discovery

Subtitle discovery occurs during Phase 1 of the media scanning pipeline (see [MEDIA_SCANNING.md](MEDIA_SCANNING.md)). External subtitle files are matched to their parent media item by:

1. **Same directory, same base name:** `Movie.2024.mkv` → `Movie.2024.en.srt`
2. **Subtitle directory convention:** `Movie.2024/Subs/` or `subtitles/`
3. **Language suffix parsing:** `.en.srt`, `.eng.srt`, `.en-US.srt`, `.en.ssa`, `.en.ass`

Embedded subtitles (inside the container) are extracted from the ffprobe output during Phase 3.

### External Subtitle File Extensions

| Extension | Format | Type |
|---|---|---|
| `.srt` | SubRip | Text |
| `.ass` / `.ssa` | Advanced SubStation Alpha | Text (styled) |
| `.vtt` | WebVTT | Text |
| `.sub` + `.idx` | VobSub (requires both files) | Image |
| `.sup` | PGS (Presentation Graphic Stream) | Image |

### External Subtitle Naming Convention

```
Movie Name (Year).{language}.{flags}.srt
```

- **language:** ISO 639-1 (e.g. `en`, `es`, `fr`) or ISO 639-2/T (e.g. `eng`, `spa`, `fre`)
- **flags (optional):** `forced`, `hi` (hearing impaired), `sdh`, `cc`
- Examples: `The.Matrix.1999.en.srt`, `The.Matrix.1999.es.forced.srt`, `The.Matrix.1999.en.hi.srt`

### Subtitle Files Table

```sql
CREATE TABLE subtitle_files (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    file_path TEXT NOT NULL,
    language TEXT NOT NULL,
    subtitle_type TEXT NOT NULL CHECK (subtitle_type IN ('embedded', 'external', 'fetched')),
    is_forced BOOLEAN NOT NULL DEFAULT false,
    is_hearing_impaired BOOLEAN NOT NULL DEFAULT false,
    source_provider TEXT,

    UNIQUE(media_item_id, file_path)
);

CREATE INDEX idx_subtitle_files_media_item_id ON subtitle_files (media_item_id);
```

`source_provider` is `NULL` for embedded/external subtitles. For fetched subtitles, it stores the provider name (e.g. `'opensubtitles'`, `'subdl'`).

## Subtitle Conversion

### PGS/VobSub → Text (OCR)

Image subtitles (PGS from Blu-rays, VobSub from DVDs) force full video transcode on many clients because the server must burn them into the video. OCR eliminates this by converting image subtitles to SRT text — a one-time cost per subtitle track, cached forever.

#### OCR Pipeline

```
PGS/VobSub detected during scan
    │
    ├─ Check if OCR result already exists in subtitle_ocr_cache
    │   ├─ Yes → skip OCR, use cached SRT
    │   └─ No → queue OCR task
    │
    ├─ OCR Task:
    │   1. Extract subtitle stream from container (FFmpeg)
    │   2. Render subtitle bitmaps to individual PNG frames (FFmpeg)
    │   3. OCR each frame via PaddleOCR (sub-convert approach)
    │   4. Assemble SRT with timestamps from subtitle metadata
    │   5. Store result in subtitle_ocr_cache
    │   6. Create subtitle_files row (subtitle_type='fetched', source_provider='ocr')
    │
    └─ Fallback: if PaddleOCR unavailable, try Tesseract
        └─ Last resort: no OCR, PGS burn-in at playback time
```

#### OCR Tool Selection

| Tool | Approach | Accuracy | Speed | Non-Latin Support |
|---|---|---|---|---|
| **PaddleOCR** (primary) | Deep learning OCR, sub-convert pipeline | High (SOTA for text recognition) | Fast (GPU-optional) | Excellent (80+ languages) |
| **Tesseract** (fallback) | Traditional OCR engine | Moderate (struggles with low-res PGS) | Moderate | Good (100+ languages, but less accurate) |

**PaddleOCR via sub-convert** is the primary choice for best accuracy, especially on low-resolution PGS bitmaps common in Blu-ray subtitles. It adds a Python runtime dependency (PaddleOCR requires Python) but the OCR is a one-time background task — it never runs during playback and never affects streaming performance.

**Tesseract** is the fallback when PaddleOCR is unavailable. It's available as a system library (no Python dependency) but produces less accurate results, especially on low-resolution bitmaps and non-Latin scripts.

#### OCR Cache Table

```sql
CREATE TABLE subtitle_ocr_cache (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    subtitle_stream_index INT NOT NULL,
    source_hash TEXT NOT NULL,

    ocr_engine TEXT NOT NULL CHECK (ocr_engine IN ('paddleocr', 'tesseract')),
    confidence_score NUMERIC(3,2),

    srt_content TEXT NOT NULL,

    UNIQUE(media_item_id, subtitle_stream_index)
);

CREATE INDEX idx_subtitle_ocr_cache_media_item_id ON subtitle_ocr_cache (media_item_id);
```

`source_hash` — Blake3 hash of the subtitle stream data. If the source file changes (re-rip, different version), the hash changes and OCR is re-run.

`confidence_score` — average OCR confidence across all frames. Below 0.80, the admin is warned that the OCR result may contain errors and should be reviewed.

### ASS → SRT Conversion

ASS (Advanced SubStation Alpha) subtitles contain rich styling (fonts, colors, positioning, karaoke effects). When a client doesn't support ASS, the server converts to plain SRT by stripping all styling.

**Implementation: Rust-native text parsing.** No FFmpeg subprocess needed — ASS→SRT is trivial text processing:

1. Parse `[Events]` section
2. Extract `Dialogue:` lines
3. Strip override tags (`{\.*?}`)
4. Reformat timestamps from `H:MM:SS.CC` to `HH:MM:SS,mmm`
5. Output SRT

This is ~50 lines of Rust. No external dependency. No subprocess. Runs synchronously during subtitle delivery if the client doesn't support ASS.

The conversion is lossy (styling, positioning, karaoke effects are discarded) but the text content and timing are preserved.

### TTML → SRT Conversion

TTML (Timed Text Markup Language) subtitles are rare in personal media libraries but may appear in streaming service rips. Converted to SRT on ingestion during scan — XML parsing, extract text and timing, output SRT. One-time conversion, cached.

## Subtitle Synchronization

Subtitle sync issues are a top user complaint across all Duskcues (Plex, Jellyfin, Emby). The server handles all sync correction — clients do nothing except display the subtitles they receive.

### Three Types of Sync Issues

| Type | Cause | Symptom | Solution |
|---|---|---|---|
| **Constant offset** | Added/removed intro, different edit | All subtitles shifted by fixed amount | Server applies offset when serving |
| **FPS rate mismatch** | 24fps subtitle on 23.976fps source (or vice versa) | Subtitles drift progressively | Server rescales timestamps |
| **Different edition** | Extended vs theatrical cut | Subtitles match at start, drift in middle | Voice activity alignment or manual offset |

### Server-Side Offset Correction

The server rewrites subtitle timestamps before delivery. For text subtitles (SRT, ASS→SRT), this is pure arithmetic — no transcoding, no FFmpeg, no I/O beyond reading the subtitle file. Zero runtime cost.

```
User sets offset (ms) for an item
    │
    ├─ Offset stored in user_item_data.metadata.subtitle_offset_ms
    │
    └─ At delivery time:
        FOR EACH subtitle cue:
            new_start = original_start + offset_ms
            new_end = original_end + offset_ms
        Serve modified subtitle
```

The offset is **per-user per-item** — different users can have different offsets for the same item (e.g. different subtitle files in different languages). Stored in `user_item_data.metadata` JSONB:

```json
{
    "subtitle_offset_ms": -2500,
    "subtitle_language_preference": "en",
    "subtitle_mode": "default"
}
```

### FPS Rate Adjustment

When the subtitle file's framerate doesn't match the media file's framerate, timestamps drift progressively. The server detects this during scan:

1. Extract media file framerate from ffprobe data (`media_files.metadata`)
2. Extract subtitle framerate from subtitle file header (ASS `Timer:` field) or infer from timestamp intervals
3. If mismatch detected, compute scale factor: `scale = target_fps / source_fps`
4. Rescale all timestamps: `new_time = original_time * scale`
5. Store corrected SRT with `subtitle_type = 'fetched'`, `source_provider = 'fps_adjust'`

This is a one-time correction during scan — cached forever, zero playback cost.

### Voice Activity Alignment (Plex-Style)

The gold standard for subtitle sync, inspired by Plex's Auto-Sync Subtitles feature. The server analyzes the audio track to detect speech patterns, then aligns subtitle timestamps to match.

**Pipeline:**

```
1. Extract audio from media file (FFmpeg, downmix to mono)
2. Run voice activity detection (VAD) using FFmpeg silencedetect or WebRTC VAD
3. Generate speech timeline: list of (start_ms, end_ms) speech segments
4. Compare speech timeline with subtitle timestamps
5. Compute optimal offset (cross-correlation or sliding window)
6. Store offset in subtitle_sync_data table
7. At delivery time: apply offset to all cues
```

This is a **scheduled background task** (`subtitle_voice_analysis`) — it runs after library scan, processing new/changed items. It does not run during playback. Results are cached in `subtitle_sync_data`.

**Limitations (aligned with Plex's implementation):**
- Only works for external SRT subtitles (not embedded, not ASS, not PGS)
- Offset must be <30 seconds (larger offsets indicate wrong subtitle file, not sync issue)
- Requires consistent offset across the duration (different editions can't be auto-corrected)

### Subtitle Sync Data Table

```sql
CREATE TABLE subtitle_sync_data (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    subtitle_file_id UUID NOT NULL REFERENCES subtitle_files(id) ON DELETE CASCADE,

    sync_method TEXT NOT NULL CHECK (sync_method IN ('voice_activity', 'fps_adjust', 'manual')),
    offset_ms INT NOT NULL DEFAULT 0,
    confidence NUMERIC(3,2),

    fps_source NUMERIC(8,4),
    fps_target NUMERIC(8,4),

    UNIQUE(media_item_id, subtitle_file_id, sync_method)
);

CREATE INDEX idx_subtitle_sync_data_media_item_id ON subtitle_sync_data (media_item_id);
```

`confidence` — for voice activity alignment, how well the speech timeline correlates with subtitle timestamps. Below 0.60, the offset is not applied automatically.

## Subtitle Fetching (External Providers)

### Overview

The server can automatically download subtitles from external providers during library scan. This is controlled by admin settings — the admin enables providers, configures API keys, and selects which languages to auto-fetch.

### Provider Support

| Provider | API | Coverage | Authentication | Rate Limit |
|---|---|---|---|---|
| **SubDL** (primary) | REST API | ~50 languages, TMDB ID search, generous free tier | API key | 2,000 req/day, 300 downloads/day free |
| **OpenSubtitles** (secondary) | REST API v2 | 75 languages, largest subtitle database, hash matching | API key + optional user token | 5/day free, unlimited with subscription ($1/month) |

### Auto-Download Flow

```
Library scan discovers new media item
    │
    ├─ Check subtitle_files for item
    │   ├─ Subtitle exists for user's preferred language?
    │   │   → Skip auto-fetch (subtitle already available)
    │   │
    │   └─ No subtitle for preferred language?
    │       → Queue subtitle fetch task
    │
    ├─ Subtitle Fetch Task (providers queried in priority order):
    │   1. Search SubDL by TMDB ID (or IMDb ID, or film name)
    │      - Filter results: language, hearing-impaired, format
    │      - Download best match (highest relevance)
    │      - Save to same directory as media file
    │      - Create subtitle_files row (subtitle_type='fetched', source_provider='subdl')
    │   2. If SubDL returns no results and OpenSubtitles is enabled:
    │      - Compute OSHash from media file (first+last 64KB)
    │      - Search OpenSubtitles by hash + filename
    │      - Filter results: language, hearing-impaired, fps match
    │      - Download best match (highest download count + best rating)
    │      - Create subtitle_files row (subtitle_type='fetched', source_provider='opensubtitles')
    │
    └─ Rate limiting:
        - Cache results locally (don't re-query for same file)
        - Respect provider rate limits (see METADATA_PROVIDERS.md)
        - Batch fetches during scan (don't hammer API)
```

### OSHash Computation

OpenSubtitles uses a specific hash algorithm for file identification:

```
OSHash = sum of first 64KB bytes + sum of last 64KB bytes + file size
```

This is computed during Phase 3 (probe) of the media scanning pipeline and stored in `media_files.metadata` JSONB. The hash is fast (only reads 128KB of the file regardless of file size) and is used for exact file matching on OpenSubtitles.

### Subtitle Provider Config

Subtitle provider settings are stored in `server_config.integrations` JSONB:

```json
{
    "classifarr_enabled": false,
    "subtitle_providers": {
        "subdl": {
            "enabled": true,
            "api_key": "",
            "auto_fetch_enabled": true,
            "auto_fetch_languages": ["en"],
            "prefer_hearing_impaired": false
        },
        "opensubtitles": {
            "enabled": false,
            "api_key": "",
            "api_token": "",
            "auto_fetch_enabled": false,
            "auto_fetch_languages": [],
            "prefer_hearing_impaired": false
        }
    }
}
```

**SubDL** is the primary subtitle source (enabled by default when an API key is provided). It supports direct TMDB ID and IMDb ID search, making it a natural fit with the identification pipeline. Free tier: 2,000 requests/day, 300 downloads/day. See [METADATA_PROVIDERS.md](METADATA_PROVIDERS.md) for full provider profiles.

**OpenSubtitles** is a secondary source. It offers the largest subtitle library and hash-based matching, but requires a VIP subscription for meaningful downloads. Disabled by default.

When `auto_fetch_enabled` is `true` and `auto_fetch_languages` is non-empty, the server automatically fetches subtitles during scan for any media item that lacks subtitles in those languages. Providers are queried in priority order (SubDL first, then OpenSubtitles if enabled); the first successful result is used.

## Subtitle Delivery

### Delivery Strategy

The subtitle delivery strategy follows the three-tier approach documented in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md) — Smart Subtitle Strategy. This section covers the delivery mechanics.

```
Client requests media with subtitle selected:
    │
    ├─ NO SUBTITLE
    │   → No subtitle processing
    │
    ├─ TEXT SUBTITLE (SRT, ASS, converted OCR output):
    │   ├─ DIRECT PLAY:
    │   │   → Serve subtitle file as text sidecar
    │   │   → Client loads subtitle natively
    │   │
    │   └─ HLS TRANSCODE:
    │       → Convert to WebVTT (trivial text transformation)
    │       → Add as SUBTITLE group in HLS manifest
    │       → hls.js renders WebVTT-in-ISOBMFF or sidecar
    │
    ├─ TEXT SUBTITLE (ASS, client doesn't support ASS):
    │   → Convert ASS→SRT (strip styling, ~50 lines Rust)
    │   → Serve as SRT (direct play) or WebVTT (HLS)
    │   → No video transcode needed
    │
    └─ IMAGE SUBTITLE (PGS, VobSub):
        ├─ OCR result exists (SRT from PaddleOCR)?
        │   → Serve OCR'd SRT instead (no burn-in!)
        │
        ├─ External SRT exists for same language?
        │   → Serve external SRT instead (no burn-in!)
        │
        ├─ Client supports PGS overlay natively?
        │   → Direct play with embedded PGS
        │
        └─ No alternative → BURN-IN (last resort)
            → Requires video transcode
            → Log QUALITY_008 warning for admin visibility
```

### HLS Subtitle Delivery

For HLS streams, text subtitles are delivered as WebVTT sidecar tracks:

```m3u8
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",LANGUAGE="en",NAME="English",DEFAULT=YES,AUTOSELECT=YES
#EXT-X-STREAM-INF:BANDWIDTH=6000000,CODECS="avc1.640028,mp4a.40.2",SUBTITLES="subs"
stream/1080p.m3u8
```

All text subtitles are converted to WebVTT before HLS delivery. SRT→WebVTT is trivial text transformation (timestamp format change only). ASS→WebVTT goes through ASS→SRT→WebVTT.

**No WebVTT-in-ISOBMFF** — plain WebVTT sidecar tracks are simpler, universally supported by hls.js and native Safari, and avoid the complexity of embedding WebVTT in fMP4 segments.

### User Subtitle Preferences

Each user has subtitle preferences stored in `users.metadata` JSONB:

| Field | Type | Values | Default |
|---|---|---|---|
| `subtitle_mode` | string | `default`, `always`, `none`, `forced_only` | `default` |
| `subtitle_language_preference` | string[] | ISO 639-1 codes | `["en"]` |
| `subtitle_prefer_external` | bool | — | `true` |

And per-item overrides in `user_item_data.metadata` JSONB:

| Field | Type | Values | Default |
|---|---|---|---|
| `subtitle_offset_ms` | int | milliseconds | `0` |
| `subtitle_language_override` | string | ISO 639-1 code | `null` |
| `subtitle_track_index` | int | stream index | `null` |

### Subtitle Selection Algorithm

When a user starts playback:

```
1. Check user preferences:
   - subtitle_mode = "none" → no subtitle
   - subtitle_mode = "forced_only" → look for forced subtitle in user's language
   - subtitle_mode = "always" → auto-select best subtitle
   - subtitle_mode = "default" → auto-select if audio language ≠ user language

2. Select subtitle track (priority order):
   a. Per-item override (user_item_data.metadata.subtitle_track_index)
   b. External subtitle in preferred language (if subtitle_prefer_external=true)
   c. Embedded subtitle in preferred language
   d. Fetched subtitle in preferred language (from provider)
   e. OCR'd subtitle in preferred language (from PGS/VobSub)
   f. Forced subtitle in preferred language
   g. Any subtitle in preferred language

3. Apply sync corrections:
   a. Per-user per-item offset (user_item_data.metadata.subtitle_offset_ms)
   b. Voice activity alignment (subtitle_sync_data, if available)
   c. FPS adjustment (subtitle_sync_data, if applied at scan time)

4. Deliver:
   - Text subtitle → sidecar or WebVTT in HLS
   - Image subtitle with OCR alternative → deliver OCR'd SRT
   - Image subtitle without alternative → burn-in (QUALITY_008 warning)
```

## Subtitle Domain in Configuration

### SubtitleConfig Rust Struct

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubtitleConfig {
    pub ocr_enabled: bool,
    pub ocr_engine: String,
    pub ocr_confidence_threshold: f64,
    pub voice_activity_analysis: bool,
    pub voice_activity_schedule: String,
    pub default_subtitle_mode: String,
    pub default_subtitle_language: String,
    pub auto_fetch_enabled: bool,
    pub auto_fetch_languages: Vec<String>,
}
```

### SubtitleConfig Fields

| Field | Type | Default | Description |
|---|---|---|---|
| `ocr_enabled` | bool | `true` | Enable PGS/VobSub OCR during library scan. Requires PaddleOCR (Python) or Tesseract. |
| `ocr_engine` | string | `"paddleocr"` | Primary OCR engine. Options: `paddleocr`, `tesseract`. Falls back to the other if primary unavailable. |
| `ocr_confidence_threshold` | f64 | `0.80` | Below this confidence score, admin is warned to review the OCR result. |
| `voice_activity_analysis` | bool | `false` | Enable voice activity detection for subtitle sync. CPU-intensive background task. |
| `voice_activity_schedule` | string | `"0 5 * * *"` | Cron schedule for voice activity analysis (default: daily at 05:00, after library scan). |
| `default_subtitle_mode` | string | `"default"` | Default subtitle mode for new users. Options: `default`, `always`, `none`, `forced_only`. |
| `default_subtitle_language` | string | `"en"` | Default subtitle language for new users. |
| `auto_fetch_enabled` | bool | `false` | Enable auto-download of subtitles from external providers during scan. |
| `auto_fetch_languages` | Vec<String> | `[]` | Languages to auto-fetch. Empty = disabled regardless of `auto_fetch_enabled`. |

### Storage in server_config

`SubtitleConfig` is stored as `server_config.subtitles` JSONB column. Maps to the `SubtitleConfig` Rust struct in `RuntimeConfig`.

Example:
```json
{
    "ocr_enabled": true,
    "ocr_engine": "paddleocr",
    "ocr_confidence_threshold": 0.80,
    "voice_activity_analysis": false,
    "voice_activity_schedule": "0 5 * * *",
    "default_subtitle_mode": "default",
    "default_subtitle_language": "en",
    "auto_fetch_enabled": true,
    "auto_fetch_languages": ["en"]
}
```

### Integration with server_config.integrations

Provider-specific settings (API keys, per-provider toggles) are in `server_config.integrations` JSONB under the `subtitle_providers` key. Generic subtitle behavior settings are in `server_config.subtitles` JSONB. This separation keeps provider credentials in the integrations group (which may be restricted in the admin UI) while subtitle behavior is configurable separately.

## Scheduled Tasks

The subtitle domain adds one scheduled task:

### subtitle_ocr

Runs OCR on newly discovered PGS/VobSub subtitle tracks. Queued during library scan when image subtitles are detected without a cached OCR result.

- **Schedule:** After library scan completes (event-triggered)
- **Timeout:** 30 minutes per item
- **Config:** `{ "ocr_engine": "paddleocr", "min_confidence": 0.80 }`

### subtitle_voice_analysis

Analyzes audio tracks for voice activity and aligns external SRT subtitles. CPU-intensive — disabled by default.

- **Schedule:** `0 5 * * *` (daily at 05:00, configurable)
- **Timeout:** 2 hours
- **Config:** `{ "max_offset_seconds": 30, "min_confidence": 0.60 }`

### subtitle_auto_fetch

Downloads missing subtitles from external providers for newly scanned items.

- **Schedule:** After library scan completes (event-triggered)
- **Timeout:** 30 minutes
- **Config:** `{ "providers": ["subdl", "opensubtitles"], "languages": ["en"] }`

## Key Decisions

1. **Server does all the work** — clients never perform subtitle sync, conversion, or offset calculation. All processing happens at scan time or when the user adjusts settings. Playback-time processing is limited to timestamp arithmetic (applying a pre-computed offset), which is negligible even on the weakest server hardware.
2. **PaddleOCR for best accuracy** — accepts Python dependency for one-time background OCR tasks. PaddleOCR's accuracy on low-resolution PGS bitmaps is significantly better than Tesseract. OCR never runs during playback.
3. **OCR results cached forever** — one-time cost per subtitle track. Stored in `subtitle_ocr_cache` table. Only re-run if source file changes (detected via `source_hash`).
4. **Auto-fetch enabled by admin** — subtitles are auto-downloaded during library scan based on server settings (`auto_fetch_enabled` + `auto_fetch_languages`). Admin controls which providers and languages are used.
5. **Voice activity alignment is opt-in** — CPU-intensive audio analysis is disabled by default. Admins on powerful hardware can enable it for Plex-style auto-sync.
6. **External SRT preferred over burn-in** — if an external SRT file exists in the same language as a PGS track, the SRT is always delivered instead of burning in PGS. OCR results also count as SRT alternatives.
7. **Never burn in text subtitles** — SRT, WebVTT, and ASS are always delivered as text. ASS converts to SRT if the client doesn't support it. Text burn-in is never an option.
8. **WebVTT sidecar for HLS** — simple, universally supported. No WebVTT-in-ISOBMFF complexity.
9. **Rust-native ASS→SRT** — ~50 lines of regex-based text processing. No FFmpeg subprocess for text conversion.
10. **Per-user per-item offset** — different users can have different subtitle offsets for the same item. Offset is applied at delivery time with zero cost (timestamp arithmetic only).
11. **FPS rate correction at scan time** — detected and fixed during library scan. Cached forever. No runtime cost.
12. **OSHash computed during scan** — stored in `media_files.metadata` for OpenSubtitles exact file matching. Only reads 128KB per file.

## Relationship to Other Domains

| Domain | Relationship |
|---|---|
| **Quality Management** ([QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md)) | Three-tier subtitle strategy (passthrough → convert → burn-in) is defined here; delivery mechanics are in SUBTITLES.md. `subtitle_burn_in_policy` in `QualityConfig` controls burn-in behavior. `device_profiles.subtitle_formats` drives the decision engine. |
| **Streaming** ([STREAMING.md](STREAMING.md)) | HLS WebVTT sidecar delivery, subtitle burn-in during transcode, FFmpeg subtitle overlay filter. |
| **Media Scanning** ([MEDIA_SCANNING.md](MEDIA_SCANNING.md)) | Subtitle discovery during Phase 1 (external files) and Phase 3 (embedded streams). OSHash computation. |
| **Library Organization** ([LIBRARY_ORGANIZATION.md](LIBRARY_ORGANIZATION.md)) | External subtitle naming conventions, sidecar file locations. |
| **Configuration** ([CONFIGURATION.md](../operations/CONFIGURATION.md)) | `SubtitleConfig` in `RuntimeConfig`. Provider settings in `server_config.integrations`. |
| **Error Handling** ([ERROR_HANDLING.md](ERROR_HANDLING.md)) | `SUB_001`–`SUB_006` error codes for subtitle-specific failures. |
| **Database** ([DATABASE.md](DATABASE.md)) | `subtitle_files`, `subtitle_ocr_cache`, `subtitle_sync_data` tables. `server_config.subtitles` JSONB column. |

## Research Sources

- Reddit r/PleX — Subtitle Offset (October 2024): subtitle sync is a top pain point; offset adjustment is confusing and limited to 50ms increments
- Reddit r/JellyfinCommunity — Top quirks (April 2026): ASS subtitle burn-in causing quality spikes; no subtitle sync adjustment on Android TV
- Plex Auto-Sync Subtitles (September 2024): voice activity detection for automatic subtitle alignment; requires external SRT only; 30-second max offset; Plex Pass required
- OpenSubtitles API v2 Documentation: hash-based search, 75 languages, rate limiting
- sub-convert (GitHub): PaddleOCR-based subtitle OCR tool
- FFmpeg Subtitle Filters: overlay filter for burn-in, subtitle extraction
- hls.js WebVTT support: WebVTT sidecar tracks, WebVTT-in-ISOBMFF rendering
- WebRTC VAD: lightweight voice activity detection for audio analysis
