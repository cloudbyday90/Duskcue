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

## Implementation Notes

### Phase 9 Task 1 — Domain Scaffolding (Complete)

The subtitle domain five-file pattern was created with the following API surface:

| Method | Route | Purpose |
|---|---|---|
| `GET` | `/api/v1/items/{item_id}/subtitles` | List all subtitles for a media item |
| `POST` | `/api/v1/items/{item_id}/subtitles/fetch` | Manually trigger subtitle fetch from external providers |
| `GET` | `/api/v1/items/{item_id}/subtitles/{subtitle_id}` | Get specific subtitle metadata |
| `DELETE` | `/api/v1/items/{item_id}/subtitles/{subtitle_id}` | Delete a fetched/OCR-generated subtitle |
| `GET` | `/api/v1/items/{item_id}/subtitles/{subtitle_id}/content` | Serve subtitle text content (with optional format conversion) |
| `PUT` | `/api/v1/items/{item_id}/subtitles/{subtitle_id}/offset` | Set per-user per-item subtitle offset |
| `POST` | `/api/v1/items/{item_id}/subtitles/{subtitle_id}/ocr` | Trigger OCR for image subtitle (PGS/VobSub) |
| `GET` | `/api/v1/items/{item_id}/subtitles/{subtitle_id}/sync` | Get subtitle synchronization data |

**Error codes** — SUB_001 (FileNotFound, 404), SUB_002 (OcrUnavailable, 503), SUB_003 (OcrLowConfidence, 422), SUB_004 (ProviderUnavailable, 503), SUB_005 (ProviderRateLimited, 429), SUB_006 (VoiceAnalysisFailed, 422) are mapped in `subtitle_error_to_http()` in `server/src/error.rs`. Additional domain-specific variants (MediaItemNotFound, InvalidSubtitleFormat, InvalidLanguageCode, FetchFailed, ConversionFailed, SyncDataNotFound) map to existing SUB codes or INTERNAL.

**Row types** — `SubtitleFileRow` (7 columns matching `subtitle_files` table), `SubtitleOcrCacheRow` (7 columns matching `subtitle_ocr_cache` table), `SubtitleSyncDataRow` (9 columns matching `subtitle_sync_data` table). All match the DDL in DATABASE.md exactly.

**Validation statics** — `VALID_SUBTITLE_TYPES` (`embedded`, `external`, `fetched`), `VALID_SUBTITLE_FORMATS` (`srt`, `ass`, `ssa`, `vtt`, `sup`, `sub`, `idx`, `ttml`), `VALID_OCR_ENGINES` (`paddleocr`, `tesseract`), `VALID_SYNC_METHODS` (`voice_activity`, `fps_adjust`, `manual`), `VALID_DELIVERY_FORMATS` (`srt`, `vtt`), `VALID_SUBTITLE_PROVIDERS` (`subdl`, `opensubtitles`).

**Offset range validation** — `SetSubtitleOffsetRequest.offset_ms` validated to ±300000ms (±5 minutes) via `#[validate(range(min = -300000, max = 300000))]`. SUBTITLES.md specifies 30-second max offset for voice activity alignment; the wider manual range allows users to correct larger known offsets for alternate editions.

### Phase 9 Tasks 2–3 — Subtitle Discovery (Complete)

Subtitle discovery is implemented in `server/src/services/subtitle_discovery.rs` and integrated into the library scan pipeline. It populates the `subtitle_files` table with both external sidecar and embedded container subtitles.

**External subtitle discovery** — During the scan, all files with subtitle extensions are discovered in Phase 1 (Discover). The `discover_external_subtitles()` function iterates discovered files and matches each to its parent video file by:

1. Building a `HashMap<PathBuf, Vec<usize>>` directory map from all video files in the library for O(1) lookup
2. For each subtitle file, determining the search directory (parent directory, or grandparent if the parent is named `Subs` or `subtitles`)
3. Finding candidate video files in the same directory whose file stem is a prefix of the subtitle's file stem
4. Parsing the remaining filename segments after the video base name for language codes (2–5 char ASCII alpha, optionally with region suffix like `en-US`) and flags (`forced`, `hi`, `sdh`, `cc`, `hearing_impaired`, `hearing_impaired`, `default`)
5. Selecting the longest-matching video base name (most specific match)

Processed extensions: `.srt`, `.ass`, `.ssa`, `.vtt`, `.sub`, `.sup`. The `.idx` companion file for VobSub is excluded — only `.sub` creates a row.

**Embedded subtitle discovery** — During Phase 3 (Probe), `probe_file` now captures subtitle streams from ffprobe output. The `FfprobeStream` struct includes `index` and `disposition` fields (via new `FfprobeDisposition` struct with `forced`, `hearing_impaired`, `default` fields). Subtitle streams are collected into `additional_streams.subtitles` JSONB array with: `index`, `codec_name`, `language` (from `tags.language`, defaults to `"und"`), `title` (from `tags.title`), `is_forced` (from disposition or title containing "forced"), `is_hearing_impaired` (from disposition or title containing "hearing impaired"/"sdh"/"cc"). The `discover_embedded_subtitles()` function reads this JSONB and inserts rows with synthetic path `{media_file_path}::embedded::{stream_index}`.

**Idempotent inserts** — All subtitle inserts use `INSERT ... ON CONFLICT (media_item_id, file_path) DO NOTHING`, leveraging the `UNIQUE(media_item_id, file_path)` constraint. Re-scans add new subtitles without duplicating existing ones. Stale subtitle rows (deleted sidecar files) are not removed during scan — cleanup is deferred to a future task.

**Scan integration** — `discover_subtitles()` is called after Phase 4 (Identify) in `scan_path_pipeline`, when `media_items` and `media_files` rows already exist in the database. The count of newly inserted rows is returned as `subtitles_discovered` in `ScanResult` and aggregated across scan paths in `scan_library`.

**13 unit tests** — Cover language code detection (2-char, 3-char, region suffix, non-language), flag parsing (forced, hi, sdh, cc, hearing_impaired, multiple flags, no language with flag), subtitle file detection (all extensions, non-subtitle exclusion), and simple filename parsing.

### Phase 9 Task 4 — Subtitle Delivery (Complete)

Subtitle delivery is implemented in `server/src/domains/subtitles/service.rs` with inline format conversion and offset application. The delivery endpoint (`GET /api/v1/items/{item_id}/subtitles/{subtitle_id}/content`) serves text-based subtitles in the client's requested format with per-user offset correction applied transparently.

**Delivery pipeline:**

1. Query `subtitle_files` row by id + media_item_id
2. Reject embedded subtitles (`::embedded::` path marker) — extraction requires FFmpeg (Task 5)
3. Reject image subtitles (`.sup`, `.sub`, `.idx`) — require OCR (Task 5)
4. Read subtitle file content from disk (`tokio::fs::read_to_string`)
5. Convert to intermediate SRT format (source format detected from file extension)
6. Convert from SRT to requested delivery format (`vtt` or native `srt`)
7. Apply per-user offset (timestamp arithmetic, clamped to ≥0)
8. Return `(content, content_type)` tuple

**Format conversions implemented:**

| Source | Target | Method |
|---|---|---|
| SRT | WebVTT | Replace `,` with `.` in timestamps, add `WEBVTT` header, sequential cue numbers |
| WebVTT | SRT | Replace `.` with `,` in timestamps, add sequential cue numbers, strip `WEBVTT`/`NOTE` blocks |
| ASS/SSA | SRT | Parse `[Events]` section, strip `{\.*?}` override tags via state machine, reformat `H:MM:SS.CC` → `HH:MM:SS,mmm`, replace `\N`/`\n` with newlines |
| ASS/SSA | WebVTT | ASS → SRT → WebVTT (two-step) |

**Per-user offset storage** — Offset stored in `user_item_data.metadata` JSONB as `{"subtitle_offset_ms": -2500}`. Requires `metadata` column added via migration `20260617_080000`. The delivery handler transparently queries this before serving content — clients never need to pass offset explicitly. Uses `INSERT ... ON CONFLICT DO UPDATE SET metadata = COALESCE(metadata, '{}') || $3::jsonb` for atomic upsert.

**Content types** — WebVTT: `text/vtt; charset=utf-8`; SRT: `application/x-subrip; charset=utf-8`; ASS/SSA: `text/plain; charset=utf-8`. All include charset.

**Subtitle ordering** — `list_subtitles` returns subtitles ordered by type priority (external → fetched → embedded, matching the subtitle selection algorithm's preference for external subtitles), then forced subtitles first, then alphabetical language. This helps clients auto-select the best subtitle.

**Delete protection** — `delete_subtitle` rejects deletion of `embedded` or `external` subtitle rows. Only `fetched` subtitles (provider-downloaded, OCR-generated) are user-deletable via API.

**13 unit tests** covering SRT→WebVTT, WebVTT→SRT, ASS→SRT (with override tag stripping and multi-format timestamp parsing), ASS timestamp conversion, offset application (positive, negative-clamped, VTT separator), timecode parsing/formatting, format detection, content type mapping, language code validation.

### Phase 9 Task 5 — Subtitle Processing Service (Complete)

The shared subtitle processing service is implemented in `server/src/services/subtitles.rs` as a cross-cutting service module (not a domain module), following the same pattern as `subtitle_discovery.rs`, `media_matching.rs`, and `nfo_parser.rs`. It centralizes all subtitle text manipulation so that the domain service layer (`domains/subtitles/service.rs`) and future workers (`workers/subtitle_processor.rs`) share a single source of truth.

**Module responsibilities:**

1. **Format conversion** — `srt_to_webvtt`, `vtt_to_srt`, `ass_to_srt`, `srt_to_ass` (bidirectional). The text-conversion functions were extracted from `domains/subtitles/service.rs` (where Task 4 placed them inline) and made `pub`. The domain service now delegates to this module via `use crate::services::subtitles as sub_svc;`. Deduplication eliminates ~250 lines of duplicated parsing logic.

2. **Timestamp primitives** — `parse_timecode_to_ms`, `ms_to_timecode`, `apply_offset`, `adjust_fps`. These are the building blocks used by both conversion and synchronization. All are `pub` for reuse by future workers and the domain layer.

3. **FPS rate adjustment** — `adjust_fps(content, source_fps, target_fps) -> String`. Rescales every timecode in the subtitle by `scale = source_fps / target_fps`. Handles both SRT (`,` separator) and WebVTT (`.` separator) formats by detecting the separator from the first `-->` line. NTSC↔PAL conversions (23.976 ↔ 25) are the most common; the math is exact (`new_ms = original_ms × scale`). The function is format-agnostic — it scans for `-->` lines and rescales both endpoints. Source FPS comes from ASS `Timer:` field or admin configuration; target FPS comes from `media_files.metadata->>'frame_rate'`.

4. **Offset correction** — `apply_offset(content, format, offset_ms) -> String`. Extracted from the domain service. Applies a constant millisecond shift to every timecode, clamped to ≥0 (negative offsets cannot produce negative timestamps). Per-user per-item offset stored in `user_item_data.metadata->>'subtitle_offset_ms'`.

5. **OCR engine detection** — `detect_ocr_engine() -> Option<OcrEngine>`. Probes for `paddleocr` CLI (or `python3 -m paddleocr`) and `tesseract` CLI via `std::process::Command::new(...).arg("--version")`. Returns the first available engine in priority order (PaddleOCR primary, Tesseract fallback). Returns `None` when neither is installed → callers surface `SUB_002 OcrUnavailable`.

6. **OCR pipeline stub** — `run_ocr(source_path, stream_index, engine, media_item_id) -> Result<OcrResult, SubtitleError>`. Implements the OCR pipeline scaffold:
   - Validates that an OCR engine is available (returns `OcrUnavailable` if not)
   - Extracts the subtitle stream to a raw `.sup`/`.sub` file via FFmpeg (`ffmpeg -i input -map 0:s:N -c copy output.sup`)
   - Returns `OcrUnavailable` after extraction because the full PaddleOCR/Tesseract image-rendering and frame-by-frame OCR subprocess pipeline requires a Python runtime and complex PNG-frame orchestration that is deferred to a dedicated background worker (future enhancement, post-Phase-9)
   - The function signature, FFmpeg extraction, and engine detection are production-ready; only the actual OCR image processing is stubbed

   **Rationale for stub:** PaddleOCR (v3.6, PP-OCRv6 as of June 2026) requires a Python runtime and ~34.5M model parameters. The full pipeline (FFmpeg overlays bitmap subtitles onto blank video → extract PNG frames → paddleocr CLI per frame → assemble SRT with timestamps) is a one-time background task that belongs in `workers/subtitle_processor.rs` (Task 7). Task 5 delivers the engine detection + FFmpeg extraction + result types that the future worker will call.

7. **Voice activity alignment** — `analyze_voice_activity(media_path, subtitle_content) -> Result<VoiceAlignmentResult, SubtitleError>`. Full implementation:
   - Runs FFmpeg `silencedetect=noise=-30dB:d=0.5` against the media file's first audio track
   - Parses silence intervals from FFmpeg stderr log (`silence_start:` / `silence_end:` lines)
   - Computes speech segments (the gaps between silence intervals)
   - Parses subtitle cue start times from SRT content
   - Cross-correlates speech-segment starts against subtitle cue starts across offset range `[-30000ms, +30000ms]` in 250ms steps (241 offset candidates)
   - Returns the offset with the highest correlation count, plus a confidence score (correlation peak sharpness vs. mean)
   - Confidence below 0.60 → result still returned but caller should not auto-apply (per SUBTITLES.md)

**Integration with domain service (`domains/subtitles/service.rs`):**

- The inline conversion functions (`srt_to_webvtt`, `vtt_to_srt`, `ass_to_srt`, `ass_timestamp_to_srt`, `strip_ass_override_tags`, `apply_offset`, `parse_timecode_to_ms`, `ms_to_timecode`) were removed from the domain service and replaced with `use crate::services::subtitles as sub_svc;` imports
- `get_subtitle_content` now calls `sub_svc::to_srt`, `sub_svc::srt_to_webvtt`, `sub_svc::apply_offset`
- `trigger_ocr` now calls `sub_svc::run_ocr` instead of returning `OcrUnavailable` unconditionally — when an engine is available, the extraction + scaffold runs; when no engine is available, the error surfaces immediately with a clear message

**New types:**

- `OcrEngine` enum — `PaddleOcr`, `Tesseract` (priority order matches SUBTITLES.md OCR Tool Selection table)
- `OcrResult` struct — `engine: OcrEngine`, `confidence_score: Option<f64>`, `srt_content: String`, `source_hash: String` (Blake3 of extracted `.sup` bytes for cache invalidation)
- `VoiceAlignmentResult` struct — `offset_ms: i32`, `confidence: f64`, `speech_segments: usize`, `subtitle_cues: usize`

**Key decisions:**

- **Service module, not domain module** — Subtitle text processing is cross-cutting: used by the domain handlers (delivery), the scanner (FPS adjustment at scan time), and future workers (OCR, voice analysis, auto-fetch). Placing it in `services/` follows the established pattern (`media_matching.rs`, `subtitle_discovery.rs`, `enrichment_persistence.rs`).
- **No new workspace dependencies** — FFmpeg invocation uses `tokio::process::Command` (already in workspace via `transcoding.rs`); OCR engine detection uses `std::process::Command` (already used by `hw_accel.rs`); Blake3 hashing uses existing `blake3` workspace dep; all text parsing is standard library string manipulation.
- **OCR engine detection cached at startup** — `detect_ocr_engine()` is cheap (two subprocess spawns) but called rarely. Result not cached in `AppState` because OCR is a background task, not request-path. The future `subtitle_processor` worker will call `detect_ocr_engine()` once at startup and pass the engine to subsequent `run_ocr` calls.
- **Voice activity uses cross-correlation, not dynamic time warping** — Per SUBTITLES.md "Plex-Style" alignment: simple offset cross-correlation is the documented algorithm. DTW is overkill for the consistent-offset assumption (different editions can't be auto-corrected anyway).
- **250ms cross-correlation step** — Balances precision (sub-second offset accuracy) against computation cost (241 offset candidates × N subtitle cues). For a typical 100-cue subtitle file, this is 24,100 comparisons — sub-millisecond on modern hardware.
- **`-30dB:d=0.5` silencedetect thresholds** — Per FFmpeg community consensus for speech boundary detection: -30dB noise floor separates speech from background; 0.5s minimum duration filters out brief pauses within sentences.
- **ASS→SRT bidirectional added** — `srt_to_ass` produces a minimal valid ASS with default `[V4+ Styles]` and `[Events]` sections. Used when a client requests ASS delivery from an SRT source (rare, but completes the conversion matrix).

### Phase 9 Task 6 — Subtitle Provider Fetching (Complete)

Subtitle fetching from external providers is implemented with SubDL (primary) and OpenSubtitles (fallback). Both provider clients follow the established per-client module pattern (`tmdb_client.rs`, `tvdb_client.rs`, `fanart_client.rs`, `omdb_client.rs`).

**Provider clients:**

- **`SubdlClient`** (`services/subdl_client.rs`) — SubDL API at `api.subdl.com/api/v1`. Search by TMDB ID (`/subtitles?tmdb_id=...`), IMDb ID, or film name. Returns `SubtitleSearchResult` list with normalized language codes (uppercase in API, normalized to ISO 639-1). Download via `dl.subdl.com` URL prefix + API key. Responses are ZIP archives — `extract_subtitle_from_zip()` scans for `.srt`/`.ass`/`.ssa`/`.vtt`/`.ttml` entries. `test_connection()` searches TMDB ID 27205 (Inception) as health check.

- **`OpensubtitlesClient`** (`services/opensubtitles_client.rs`) — OpenSubtitles API at `api.opensubtitles.com/api/v1`. Search by OSHash + file size (`/subtitles?moviehash=...&moviebytesize=...`), TMDB ID, IMDb ID, or query string. Two-step download: `POST /download {file_id}` → response contains `link` → GET link returns subtitle bytes. Responses may be plain text or ZIP (checked via PK magic bytes). `test_connection()` searches TMDB ID 27205 as health check. Requires `Api-Key` and `User-Agent` headers.

**OSHash implementation:**

`compute_oshash()` in `services/subtitles.rs` computes the OpenSubtitles hash: `hash = file_size + sum_uint64_le(first_64KB) + sum_uint64_le(last_64KB)`, wrapping at 64 bits, output as 16-char hex. Minimum file size 128KB. Uses `tokio::io` for async file reads. This hash enables exact-match subtitle search, which is the most accurate method on OpenSubtitles.

**Fetch flow (`domains/subtitles/service.rs::fetch_subtitles()`):**

1. Load media item (`title`, `tmdb_id`, `imdb_id`, `media_type`) and primary media file path from DB
2. Determine provider order: if `req.provider` specified, use that only; otherwise try SubDL then OpenSubtitles
3. For each enabled provider with non-empty API key:
   - Search using best available identifier (TMDB ID → IMDb ID → title for SubDL; hash+size → TMDB ID → IMDb ID → query for OpenSubtitles)
   - `pick_best_result()` filters by language match, then forced/HI preference, then ranks by vote count + format
   - Download subtitle (ZIP for SubDL, two-step for OpenSubtitles)
   - Extract subtitle bytes from ZIP if needed
   - Save to `{media_stem}.{language}.{ext}` next to media file
   - Insert `subtitle_files` row (`subtitle_type = 'fetched'`, `source_provider = 'subdl'` or `'opensubtitles'`)
   - Return `FetchSubtitlesResponse` with fetched subtitle and provider used
4. If all providers return no results or are unavailable, return `{ fetched: [], no_results: true }`

**Error handling:**

- `ProviderUnavailable` (SUB_004, 503) — provider returned 401/403 (invalid credentials). Causes fallthrough to next provider.
- `ProviderRateLimited` (SUB_005, 429) — provider returned 429. Causes fallthrough to next provider.
- `FetchFailed` (SUB_006, 502) — network error, JSON parse error, or ZIP extraction failure. Propagates immediately.
- All other errors (DB, IO) propagate immediately.

**Config changes:**

`IntegrationsConfig` expanded from empty `{}` to:

```rust
pub struct IntegrationsConfig {
    pub subtitle_providers: SubtitleProviderConfig,
}

pub struct SubtitleProviderConfig {
    pub subdl: SubdlProviderConfig,
    pub opensubtitles: OpensubtitlesProviderConfig,
}
```

Each provider config: `enabled: bool`, `api_key: Option<String>`, `auto_fetch_enabled: bool`, `auto_fetch_languages: Vec<String>`, `prefer_hearing_impaired: bool`. OpenSubtitles additionally has `api_token: Option<String>`. All default to `enabled: false` (opt-in).

**Key decisions:**

- **SubDL as primary** — Larger free tier (2,000 req/day, 300 downloads/day), single-step download (ZIP), no user account needed
- **OSHash first for OpenSubtitles** — Hash-based matching gives exact-file results, more accurate than TMDB/IMDb title matching
- **Normalized `SubtitleSearchResult`** — Both clients return the same struct so the domain service can rank/filter uniformly without provider-specific logic
- **`zip` crate v2** added to workspace — SubDL always returns ZIP archives; needed for extraction
- **Subtitle files saved next to media** — Follows discovery convention so `subtitle_discovery.rs` finds them on re-scan

**Deferred to later tasks:**

- Subtitle settings UI — Task 8
- Full PaddleOCR/Tesseract image OCR pipeline (PNG frame rendering, per-frame OCR, SRT assembly from bitmap subtitles) — future enhancement, requires Python runtime orchestration in a background worker
- Scan-time FPS mismatch detection and automatic `subtitle_sync_data` row creation — future enhancement (the `adjust_fps` function is ready; scanner integration is deferred)
- Automatic voice-alignment scheduled task (`subtitle_voice_analysis`) — future enhancement (the `analyze_voice_activity` function is ready; scheduler registration is deferred)

### Phase 9 Task 7 — Auto-Fetch Worker (Complete)

Auto-fetch is implemented in `server/src/workers/subtitle_processor.rs` as a scheduled-task worker that follows the same pattern as `workers/metadata_refresh.rs`. Rather than running inline during the scan pipeline (which would block scan completion on slow provider responses and risk rate-limit exhaustion during bulk imports), auto-fetch runs as a periodic scheduled task that picks up newly-scanned items shortly after the scan completes.

**Worker entry point:** `run_subtitle_auto_fetch(state: &AppState, task_id: Uuid, config: serde_json::Value)`

**Pipeline:**

1. **Gate on global config** — Load `SubtitleConfig` from `RuntimeConfig`. If `auto_fetch_enabled = false` OR `auto_fetch_languages` is empty, log and return immediately (no-op run).
2. **Gate on providers** — Inspect `IntegrationsConfig.subtitle_providers`. If no enabled provider (SubDL/OpenSubtitles) has both `enabled = true` AND a non-empty API key, log and return. The per-provider `auto_fetch_enabled` flag is an additional gate; if neither provider opts into auto-fetch, the run is a no-op.
3. **Resolve target languages** — The effective language set is the union of:
   - `SubtitleConfig.auto_fetch_languages` (global preference)
   - `SubdlProviderConfig.auto_fetch_languages` (when SubDL enabled)
   - `OpensubtitlesProviderConfig.auto_fetch_languages` (when OpenSubtitles enabled)
   - Task config `languages` override (if present in `scheduled_tasks.config`, takes precedence and replaces the runtime-derived set)
4. **Find items missing subtitles** — Per target language, query `media_items` for movie/episode types (the playable leaves that own `media_files`) that:
   - Are not soft-deleted (`deleted_at IS NULL`)
   - Have at least one healthy `media_files` row (so the file is on disk for OSHash + sidecar placement)
   - Do NOT have any `subtitle_files` row whose `language` matches the target (prefix match: `"en"` matches `"en"`, `"en-US"`, `"eng"`)
   - Are capped at `max_items_per_language` per run (default 50, overridable via task config `max_items_per_language`) to respect provider rate limits and bound runtime
5. **Fetch** — For each (item, language) pair, construct a `FetchSubtitlesRequest { language, provider: None, is_forced: None, is_hearing_impaired: None }` and call the existing `domains::subtitles::service::fetch_subtitles()`. The service handles provider priority (SubDL → OpenSubtitles), `pick_best_result` ranking, ZIP extraction, sidecar save, and `subtitle_files` insert.
6. **Track results** — Counters for `items_processed`, `subtitles_fetched`, `no_results`, and `failures` are accumulated and logged at the end of the run. Individual item failures are logged at WARN but do not abort the run.

**Scheduled task wiring:**

- New migration `20260619_080000_seed_subtitle_auto_fetch_task.sql` inserts the `subtitle_auto_fetch` task into `scheduled_tasks` with `interval_seconds = 1800` (30 min), `is_enabled = false` (opt-in per SUBTITLES.md), `timeout_seconds = 1800`. The 30-minute interval approximates the "event-triggered after scan" semantics: the Library Scan runs daily at 03:00, and the auto-fetch task picks up new items within ~30 minutes of any scan (scheduled or handler-triggered).
- Executor registered in `main.rs` via `.register_executor("subtitle_auto_fetch", ...)` capturing `AppState` (for runtime config + `fetch_subtitles` access).
- Added to runtime `seed_default_tasks()` so fresh installs that skip the migration seed (e.g., test databases) still register the task.

**Key decisions:**

- **Scheduled task over inline scan integration** — Inline auto-fetch during `scan_library` would block scan completion on provider HTTP calls (SubDL/OpenSubtitles rate limits, network latency). For bulk imports (1000+ items), this could take hours and exceed HTTP request timeouts. A periodic background task decouples scan completion from subtitle availability and naturally batches work across runs. Newly-scanned items get subtitles within ~30 minutes — acceptable latency for a non-blocking background process.
- **`max_items_per_language` cap (50)** — SubDL free tier is 300 downloads/day; OpenSubtitles free tier is 5 downloads/IP/24h. Capping at 50 items per language per run prevents exhausting the daily quota in a single run and leaves budget for manual fetches via the API. The cap is configurable via task config for deployments with VIP provider subscriptions.
- **Movie/episode only** — Series and seasons are container types without direct `media_files`; `fetch_subtitles` would fail with `MediaItemNotFound` from `resolve_media_file_path`. Filtering at the query level avoids wasted API calls.
- **Language prefix match** — `"en"` matches `"en"`, `"en-US"`, `"eng"` (ISO 639-1, IETF tag, ISO 639-2/T). This prevents re-fetching when the existing subtitle has a region/code variant of the same base language. The `LIKE 'en%'` pattern is deliberately broad — it's a "good enough" deduplication; the cost of a redundant fetch is low (provider returns no results or the existing file), but the cost of missing a needed fetch is high (user has no subtitle).
- **Task config overrides runtime config** — Admins can override the auto-fetch behavior per-task via `scheduled_tasks.config`: `{ "languages": ["en", "es"], "max_items_per_language": 100, "providers": ["subdl"] }`. This enables scenarios like a one-off Spanish subtitle backfill run without changing global config.
- **No new workspace dependencies** — All functionality uses existing `sqlx`, `serde_json`, `uuid`, and the already-built `fetch_subtitles` service.
