# Segment Detection Design

## Overview

Native intro and credit detection built directly into the server — no plugins required. Detects skippable segments (intros, credits, recaps, previews) in TV episodes and movies using a multi-method pipeline that prioritizes accuracy and safety. Users see "Skip Intro" / "Skip Credits" buttons during playback, matching the experience of commercial streaming platforms.

## Design Principles

**1. Safety first — never cut into content**

Every detection method includes conservative boundaries, duration caps, and search window limits. The skip button is opt-in per press (not auto-skip by default). Multi-method validation is required for credits detection.

**2. Multi-method pipeline — best result wins**

Four detection methods with different strengths are applied in priority order. Chapter markers (instant) are checked first. Chromaprint audio fingerprinting (background task) handles the majority of cases. Black frame + silence detection supplements credits. Manual override always takes precedence.

**3. Background analysis — no impact on scanning or playback**

Segment analysis is a scheduled task that runs during off-peak hours. It does not block media scanning or playback. Fingerprints are cached in the database — re-analysis only runs when files change.

**4. Incremental — only new/changed files**

After the initial full analysis, only newly added or modified files are analyzed. Cached fingerprints survive server restarts.

---

## Detection Methods

### Method 1: Chapter Markers

| Aspect | Detail |
|---|---|
| **When** | During media scanning (Phase 3 — probing), zero additional cost |
| **How** | `ffprobe -show_chapters` already extracts chapter data; match chapter titles against regex patterns |
| **Cost** | Zero — data already available from the probe step |
| **Accuracy** | Highest when chapters are present and properly named; covers ~20-30% of files |
| **Safety** | Highest — chapters are authoritatively placed by the encoder |

Chapter titles are matched against these regex patterns (proven defaults from Jellyfin Intro Skipper):

| Segment Type | Pattern |
|---|---|
| Intro | `(^|\s)(Intro\|Introduction\|OP\|Opening)(\s\|:$\|$\|(?!\\sEnd))` |
| Credits | `(^|\s)(Credits?\|ED\|Ending\|Outro)(\s\|:$\|$\|)` |
| Recap | `(^|\s)(Re?cap\|Sum{1,2}ary\|Prev(ious(ly)?)?\|(Last\|Earlier))(\s\|$)` |
| Preview | `(^|\s)(Preview\|PV\|Sneak\s?Peek\|Coming\s?(Up\|Soon)\|Next\s+(time\|on\|episode))(\s\|$)` |

When a chapter matches, the chapter's start and end timestamps are used directly — no further analysis needed for that segment type.

### Method 2: Chromaprint Audio Fingerprinting

| Aspect | Detail |
|---|---|
| **When** | Background scheduled task (`segment_analysis`) |
| **How** | Extract PCM audio from file via FFmpeg; fingerprint with `chromaprint-next`; compare fingerprints across episodes in the same season |
| **Cost** | High initially (CPU-bound); cached; incremental after first scan |
| **Accuracy** | Very high for intros (recurring theme music is a strong signal) |
| **Safety** | High — recurring audio across 3+ episodes is very unlikely to be content |

**How chromaprint works:**

1. FFmpeg extracts raw PCM audio from the file (downmixed to mono, resampled to 11025 Hz)
2. `chromaprint-next` generates a fingerprint — a sequence of 32-bit sub-fingerprint hashes, one per ~11.6ms of audio
3. Fingerprints for all episodes in a season are compared to find recurring audio segments
4. A recurring segment that appears in 3+ episodes within the search window is marked as a candidate
5. Candidates that pass duration thresholds (15s–120s for intros) are confirmed as segments

**Search windows:**

| Segment | Search Range |
|---|---|
| Intro | First 25% of episode or first 10 minutes, whichever is smaller |
| Credits | Last 30% of episode |
| Recap | First 15% of episode |

**Minimum episode count:** At least 3 episodes in a season must be analyzed before chromaprint comparison produces results. Seasons with fewer episodes fall through to other methods.

### Method 3: Black Frame Detection

| Aspect | Detail |
|---|---|
| **When** | Same background task as chromaprint |
| **How** | FFmpeg `blackframe` filter via `tokio::process::Command` |
| **Cost** | Medium (always CPU; no GPU acceleration for this filter) |
| **Accuracy** | Medium — detects credit sequences (scrolling text on black background) but can false-positive on dark movie scenes |
| **Safety** | Medium — requires combination with silence detection for high confidence |

FFmpeg command:

```
ffmpeg -i <file> -vf "blackframe=amount=75:threshold=2" -f null -
```

Output parsing — each line contains a frame number and percentage of black pixels:

```
[Parsed_blackframe_0 @ ...] frame:12345 pblack:99 pts:1456789 t:60.123 type:I last_keyframe:0
```

Frames with ≥75% black pixels that persist for ≥15 consecutive seconds within the credits search window are marked as credit candidates.

### Method 4: Silence Detection

| Aspect | Detail |
|---|---|
| **When** | Same background task; used primarily for boundary refinement |
| **How** | FFmpeg `silencedetect` filter |
| **Cost** | Low — fast audio-only analysis |
| **Accuracy** | Medium — silence alone is ambiguous; combined with black frame for credits |
| **Safety** | Medium — used as boundary refinement, not standalone detection |

FFmpeg command:

```
ffmpeg -i <file> -af silencedetect=noise=-55dB:d=2 -f null -
```

Output:

```
[silencedetect @ ...] silence_start: 3420.12
[silencedetect @ ...] silence_end: 3425.67 | silence_duration: 5.550000
```

Silence periods are used to:
- Refine the end boundary of intros (silence between intro and episode start)
- Refine the start boundary of credits (silence between content end and credit roll)
- Detect post-credits silence (extra scenes follow silence gaps)

---

## Crate Selection

| Crate | Version | License | Role |
|---|---|---|---|
| `chromaprint-next` | 0.1 | MIT + LGPL-2.1 | Pure Rust audio fingerprinting — bit-identical to C Chromaprint, SIMD-optimized, ~4% faster than C reference |
| FFmpeg (existing) | — | LGPL-2.1 | Audio extraction, `blackframe` filter, `silencedetect` filter — already in our stack |

### Why `chromaprint-next`

| Criterion | `chromaprint-next` | `rusty-chromaprint` | C Chromaprint (FFI) |
|---|---|---|---|
| Pure Rust | Yes | Yes | No (C FFI) |
| Bit-identical to reference | Yes | No (different resampler) | N/A (is the reference) |
| All 5 algorithm variants | Yes | 2 of 5 | Yes |
| SIMD optimization | Yes (NEON) | No | Platform-dependent |
| Maintained (2026) | Yes (Feb 2026) | No (unmaintained) | Yes |
| Performance | ~4% faster than C | Slower than C | Baseline |

`chromaprint-next` is the clear choice — pure Rust (no C build dependency), bit-identical output (compatible with any future AcoustID integration), actively maintained, and faster than the C reference.

### Audio Extraction for Chromaprint

FFmpeg extracts PCM audio, piped directly to `chromaprint-next`:

```
ffmpeg -i <file> -vn -ac 1 -ar 11025 -f s16le -acodec pcm_s16le pipe:1
```

The raw PCM stream is fed into `chromaprint-next`'s streaming API:

```rust
use chromaprint::{Fingerprinter, Algorithm};

let mut fp = Fingerprinter::new(Algorithm::default());
fp.start(11025, 1)?;

// Read PCM chunks from FFmpeg stdout
while let Some(chunk) = ffmpeg_stdout.read_chunk().await? {
    let samples: Vec<i16> = chunk.cast();
    fp.feed(&samples);
}

fp.finish()?;
let raw: &[u32] = fp.fingerprint();
let encoded: String = fp.encode();
```

---

## Safety Design

### Conservative Boundaries

All automatically detected segments are padded with configurable safety buffers:

| Parameter | Default | Purpose |
|---|---|---|
| `intro_start_padding_ms` | 0 | How far into the detected intro before showing the skip button |
| `intro_end_padding_ms` | 2000 | How far before the detected intro end to skip to (prevents cutting into content) |
| `credits_start_padding_ms` | 0 | How far into detected credits before showing the skip button |
| `credits_end_padding_ms` | 0 | How far past detected credits end to skip to |

The 2-second `intro_end_padding_ms` default means: if the intro is detected as ending at 01:30.000, the skip button jumps the user to 01:28.000 — they see the last 2 seconds of the intro rather than the first 2 seconds of the episode content. This matches user expectations and avoids the "cut into content" problem reported by XDA for Jellyfin's Intro Skipper.

### Duration Thresholds

| Segment | Minimum | Maximum (TV) | Maximum (Movie) |
|---|---|---|---|
| Intro | 15s | 120s | N/A |
| Credits | 15s | 300s (5 min) | 900s (15 min) |
| Recap | 15s | 120s | N/A |
| Preview | 15s | 120s | N/A |
| Outro | 15s | 120s | 300s |

Segments outside these ranges are ignored — a 10-minute "intro" detection is almost certainly a false positive.

### Search Window Limits

| Segment | Search Range | Rationale |
|---|---|---|
| Intro (TV) | First 25% of episode or first 10 min, whichever is smaller | TV intros never appear in the second half |
| Credits (TV) | Last 30% of episode | Credits always near the end |
| Intro (Movie) | First 10 minutes | Movie opening credits typically in first 10 min |
| Credits (Movie) | Last 20% of runtime | Movie credits always near the end |
| Recap (TV) | First 15% of episode | "Previously on..." always at the start |
| Preview (TV) | After credits | "Next time on..." always after credits |

### Skip Button Behavior

- **Default: show button, user presses to skip** — no auto-skip
- **Auto-skip: opt-in per-user preference** — users who want it can enable it per segment type
- **Button visibility window:** The "Skip Intro" button appears when playback enters the intro segment and disappears after the intro segment ends (or after a configurable timeout, default 10 seconds into the intro)
- **Client responsibility:** The server provides segment timestamps; the client renders the skip button and handles the seek

### Confidence Scoring

Each detected segment has a confidence score (0.0–1.0):

| Score Range | Behavior |
|---|---|
| 0.8–1.0 | High confidence — show skip button |
| 0.5–0.79 | Medium confidence — show skip button with reduced prominence (smaller, shorter timeout) |
| 0.0–0.49 | Low confidence — do not show skip button; log for admin review |

**Confidence calculation:**

| Method | Base Score | Modifiers |
|---|---|---|
| Chapter markers | 1.0 | — (authoritative) |
| Chromaprint (3+ episodes match) | 0.9 | +0.05 if black frame agrees on boundaries |
| Chromaprint (2 episodes match) | 0.7 | +0.1 if silence detection agrees |
| Black frame + silence | 0.8 | -0.2 if dark scene counter-indicators detected |
| Black frame alone | 0.5 | Not shown by default |

### Multi-Method Validation for Credits

Credits detection requires agreement from at least 2 independent methods before surfacing with high confidence:

1. Black frame detects credit-like frames in the credits window
2. Silence detection confirms audio quieting near the same timestamp
3. Chapter markers name "Credits" or "Outro"

If only one method detects credits, confidence is capped at 0.5 (not shown by default). The admin can lower the confidence threshold to see these detections.

---

## Segment Types

| Type | Description | TV | Movie | Detection Methods |
|---|---|---|---|---|
| `intro` | Opening credits / theme music | Yes | Yes (opening credits) | Chapter, Chromaprint, Black frame |
| `credits` | End credits / scrolling text | Yes | Yes | Chapter, Chromaprint, Black frame + Silence |
| `recap` | "Previously on..." segment | Yes | No | Chapter, Chromaprint |
| `preview` | "Next time on..." segment | Yes | No | Chapter, Chromaprint |
| `outro` | Post-credits scene / stinger | Yes | Yes | Silence gap detection |

---

## Analysis Pipeline

### Scheduling

Segment analysis runs as a scheduled task (`segment_analysis`) via the existing `scheduled_tasks` system:

| Parameter | Value |
|---|---|
| Task type | `segment_analysis` |
| Default schedule | `0 3 * * *` (daily 03:00, same as library scan) |
| Timeout | 4 hours |
| Max concurrent | 1 (CPU-intensive) |

The task runs after the daily library scan completes, ensuring all new files are already cataloged.

### Pipeline Steps

```
For each library with segment detection enabled:

  1. Resolve episodes/seasons for analysis:
     - New files (not yet fingerprinted) → full analysis
     - Changed files (file_hash differs) → re-analysis
     - Existing fingerprints → skip (cached)

  2. Extract chapters (Phase 3 of scanning already did this):
     - Read chapter data from media_files.additional_streams JSONB
     - Match chapter titles against regex patterns
     - Create media_segments entries for matched chapters
     - Mark these items as "chapter-analyzed" — skip further analysis

  3. Extract and fingerprint audio (for items not resolved by chapters):
     - FFmpeg extracts PCM audio → pipe to chromaprint-next
     - Store fingerprint in media_fingerprints table
     - Group fingerprints by season

  4. Compare fingerprints across episodes in each season:
     - For each season with ≥3 fingerprinted episodes:
       - Compare all episode pairs
       - Find recurring audio segments within search windows
       - Score candidates by number of matching episodes
     - Create media_segments for high-confidence matches

  5. Run black frame + silence detection:
     - For credits detection on items not yet resolved
     - FFmpeg blackframe filter on credits search window
     - FFmpeg silencedetect for boundary refinement
     - Require agreement from both methods for high confidence
     - Create or update media_segments

  6. Report results:
     - Segments created, updated, unchanged per library
     - Low-confidence detections logged for admin review
     - Errors logged per file
```

### Incremental Analysis

```sql
SELECT mf.media_item_id, mf.file_path, mf.file_hash
FROM media_files mf
JOIN media_items mi ON mf.media_item_id = mi.id
WHERE mi.library_id = $1
AND mi.type IN ('episode', 'movie')
AND NOT EXISTS (
    SELECT 1 FROM media_fingerprints fp
    WHERE fp.media_file_id = mf.id
    AND fp.file_hash = mf.file_hash
);
```

Only files not yet fingerprinted (or whose hash has changed) are analyzed. This makes subsequent runs fast — only new additions are processed.

---

## API Endpoints

### Segment Retrieval

| Method | Endpoint | Description |
|---|---|---|
| `GET` | `/api/v1/items/{id}/segments` | Get all detected segments for a media item |
| `GET` | `/api/v1/items/{id}/segments?type=intro` | Get segments of a specific type |
| `PUT` | `/api/v1/items/{id}/segments/{segment_id}` | Manually override segment timestamps (admin or owner) |
| `DELETE` | `/api/v1/items/{id}/segments/{segment_id}` | Remove a segment (admin or owner) |
| `POST` | `/api/v1/items/{id}/segments` | Create a manual segment |
| `POST` | `/api/v1/libraries/{id}/analyze-segments` | Trigger segment analysis for a library |

### Segment Response Format

```json
{
    "segments": [
        {
            "id": "uuid",
            "type": "intro",
            "start_ms": 15000,
            "end_ms": 90000,
            "confidence": 0.95,
            "source": "chromaprint",
            "skip_to_ms": 88000,
            "can_edit": true
        }
    ]
}
```

`skip_to_ms` — the actual timestamp the client should seek to when the user presses skip (includes safety padding). The client does not need to calculate this.

`source` — how this segment was detected: `chapter`, `chromaprint`, `blackframe`, `silence`, `manual`.

`can_edit` — whether the current user can edit or delete this segment (admin/owner only).

---

## Configuration

### Server-Wide Segment Config

Stored in `server_config.transcoding` JSONB (existing column):

```json
{
    "segment_detection_enabled": true,
    "segment_types": {
        "intro": { "enabled": true, "min_duration_s": 15, "max_duration_s": 120 },
        "credits": { "enabled": true, "min_duration_s": 15, "max_duration_s": 300 },
        "recap": { "enabled": true, "min_duration_s": 15, "max_duration_s": 120 },
        "preview": { "enabled": true, "min_duration_s": 15, "max_duration_s": 120 },
        "outro": { "enabled": false, "min_duration_s": 15, "max_duration_s": 120 }
    },
    "segment_safety": {
        "intro_end_padding_ms": 2000,
        "credits_end_padding_ms": 0,
        "min_confidence": 0.7
    },
    "segment_analysis": {
        "max_concurrent_analyses": 1,
        "chromaprint_sample_rate": 11025,
        "blackframe_threshold": 2,
        "blackframe_amount": 75,
        "silence_noise_db": -55,
        "silence_min_duration_s": 2
    }
}
```

### Per-Library Segment Config

Stored in `libraries.metadata` JSONB:

```json
{
    "segment_detection_enabled": true,
    "segment_types_disabled": ["preview"],
    "chapter_regex_overrides": {}
}
```

### Per-Item Override

Admin can disable detection for specific movies/shows (e.g., anthology series with no recurring intro):

```json
{
    "segment_detection_enabled": false
}
```

Stored in `media_items.metadata` JSONB.

---

## Playback Integration

### How Skip Buttons Work

1. Client requests `GET /api/v1/items/{id}/segments` before or during playback
2. Client receives segment list with `start_ms`, `end_ms`, `skip_to_ms`, `confidence`
3. Client filters segments by confidence threshold (configurable, default 0.7)
4. During playback, client monitors current position
5. When position enters a segment's `start_ms`..`end_ms` window:
   - Client displays "Skip Intro" / "Skip Credits" button
   - Button auto-hides after the segment ends or after a timeout (default: 10 seconds)
6. User presses button → client seeks to `skip_to_ms`
7. Client sends `POST /api/v1/playback/heartbeat` as usual (position updates)

### Auto-Skip (Opt-In)

Per-user preference stored in `users.metadata` JSONB:

```json
{
    "auto_skip": {
        "intro": false,
        "credits": false,
        "recap": false
    }
}
```

When auto-skip is enabled for a segment type, the client automatically seeks to `skip_to_ms` when the segment starts — no button press needed. Off by default for all types.

### Mark as Played Integration

Segment detection integrates with the existing "mark as played" logic. The `user_item_data.is_watched` threshold can use credits markers:

| Option | Behavior |
|---|---|
| At final credits marker | Marks as played when reaching the last credits segment (after mid/post-credits scenes) |
| At first credits marker | Marks as played when reaching the first credits segment |
| At threshold percentage | Classic behavior (90% default) |
| Earliest of threshold and first credits | Default — uses whichever comes first |

This is the same approach Plex uses, proven to handle post-credits scenes correctly.

---

## Metrics

Segment detection metrics are exposed via the existing Prometheus `/metrics` endpoint:

| Metric | Type | Labels | Description |
|---|---|---|---|
| `segment_analysis_files_total` | counter | library, method (chapter/chromaprint/blackframe/silence) | Files analyzed by method |
| `segment_analysis_duration_seconds` | histogram | library | Time to analyze one library |
| `segment_segments_created_total` | counter | library, type (intro/credits/etc), source | Segments created by type and source |
| `segment_segments_active` | gauge | library, type | Currently stored segments by type |
| `segment_skip_total` | counter | type, auto (true/false) | Skip button presses |
| `segment_low_confidence_total` | counter | library | Low-confidence detections (not shown to users) |
| `segment_analysis_errors_total` | counter | library, method | Analysis failures by method |

---

## Integration with Existing Systems

### Media Scanning (MEDIA_SCANNING.md)

Phase 3 (probe) already extracts chapter data via `ffprobe -show_chapters`. Chapter titles are stored in `media_files.additional_streams` JSONB. The segment analysis task reads this data — no additional probe step needed.

### Streaming (STREAMING.md)

Segment timestamps are returned alongside playback info. The playback start endpoint (`POST /api/v1/playback/start`) includes segments in the response so the client can prepare skip buttons before playback begins.

### Scheduled Tasks (DATABASE.md — System Domain)

New task type `segment_analysis` added to the `scheduled_tasks.task_type` CHECK constraint. Runs as a standard scheduled task with full run history, error tracking, and auto-disable on consecutive failures.

### Error Handling (ERROR_HANDLING.md)

No new error codes — segment retrieval uses existing codes:
- `MEDIA_001` (404) — media item not found
- `PLAY_001` (404) — media item not found (during playback)
- `VALID_001` (422) — invalid segment timestamps in manual override

Analysis failures are logged and tracked in `scheduled_task_runs` — they don't produce API errors since analysis is a background task.

---

## Research Sources

### Detection Methods & Libraries
- Jellyfin Intro Skipper (intro-skipper/intro-skipper) — multi-method detection: chapters, chromaprint, blackframe, silence (actively maintained, 2026)
- Plex Credits Detection — audio fingerprinting for intros; visual analysis + cloud markers for credits (support.plex.tv)
- plex-credits-detect (cjmanca/plex-credits-detect) — audio spectrographic fingerprinting + black frame detection for credits
- Skiptro (Kodi) — audio fingerprinting + ML boundary detection + silence/energy analysis (February 2026)

### Chromaprint & Audio Fingerprinting
- chromaprint-next (attilagyorffy/chromaprint-next) — pure Rust port of Chromaprint; bit-identical to C reference; SIMD-optimized; MIT + LGPL-2.1 (February 2026)
- Chromaprint reference — oxygene.sk/2011/01/how-does-chromaprint-work/
- AcoustID — acoustic fingerprinting database using Chromaprint

### FFmpeg Filters
- FFmpeg Documentation — `blackframe` filter: frame-level black pixel detection
- FFmpeg Documentation — `silencedetect` filter: audio silence period detection
- Jellyfin Intro Skipper Wiki — analysis pipeline, settings, segment parameters

### User Experience
- XDA Developers — Jellyfin Intro Skipper review: "sometimes skips content" (false positive concern)
- JellyWatch — Intro Skipper configuration guide, CPU impact benchmarks, troubleshooting (March 2026)
- Reddit r/PleX — intro and credit detection discussion: Plex uses audio for intros, visual analysis for credits; ~95% accuracy; cloud marker sharing
