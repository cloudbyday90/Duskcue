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

---

## Implementation Notes

### Phase 10 Task 1 — Domain Scaffolding (Complete)

Segment retrieval and manual override API surface implemented. The detection pipeline itself (Methods 1–4) lands in Tasks 2 and 5.

**Files built:**

| File | Purpose |
|---|---|
| `server/src/domains/segments/mod.rs` | Module declarations + router assembly with 3 route groups (5 endpoints) |
| `server/src/domains/segments/error.rs` | `SegmentError` enum with 9 variants covering not-found, validation, conflict, and Database catch-all |
| `server/src/domains/segments/types.rs` | Three-type DTOs: `SegmentRow` (internal), `CreateSegmentRequest`/`UpdateSegmentRequest` (Deserialize + Validate), `SegmentResponse`/`SegmentListResponse`/`AnalyzeSegmentsResponse` (Serialize); `SegmentListQuery` for `?type=` filter; `VALID_SEGMENT_TYPES`/`VALID_SEGMENT_SOURCES` statics matching the DB CHECK constraints |
| `server/src/domains/segments/service.rs` | 5 `todo!()` service function stubs — list/create/update/delete + trigger_library_analysis |
| `server/src/domains/segments/handlers.rs` | 5 working handlers wired to Axum extractors; mutation endpoints use `Require<CanManageLibraries>` |
| `server/src/error.rs` | `AppError::Segment(#[from] SegmentError)` variant + `segment_error_to_http()` mapping |
| `server/src/router.rs` | Segments router merged via `.merge(crate::domains::segments::router(state.clone()))` |
| `server/src/domains/mod.rs` | `pub mod segments;` added |

**Decisions reconciled with this design doc:**

- **Error code mapping confirmed** — The "No new error codes" rule is honored. `SegmentError` variants map: `MediaItemNotFound`/`SegmentNotFound` → `MEDIA_001` (404); `LibraryNotFound` → `LIB_001` (404); `InvalidSegmentType`/`InvalidSegmentSource`/`InvalidTimestamps` → `VALID_001` (422); `ManualSegmentExists`/`AnalysisAlreadyInProgress` → `CONFLICT` (409). `PLAY_001` is reserved for playback-path segment lookups (will be used if/when the playback start endpoint embeds segments in its response per the "Streaming" integration section above).
- **`PUT` (not PATCH) for manual override** — Honors the API Endpoints table verbatim. The semantic is full replacement of the segment's timestamp triple (`start_ms`/`end_ms`/`skip_to_ms`) plus optional `confidence`, with all fields using `COALESCE` partial update semantics for fields the client omits (Task 2 implementation detail).
- **`can_edit` field computed in handler** — Per the Response Format spec, `can_edit` reflects whether the requesting user can mutate the segment. Handler computes it once from `AuthenticatedUser` (true if `role == "owner"` or `capabilities` contains `can_manage_libraries`), then passes the boolean to `list_segments`. This avoids per-row DB capability lookups and keeps the service layer user-agnostic.
- **Mutation capability: `CanManageLibraries`** — The spec says "admin or owner". Owner bypass is built into the capability framework (owner passes any `Require<C>`); "admin" maps to `can_manage_libraries` rather than `can_manage_server` because segments are library-scoped resources, consistent with the libraries domain (`scan_library` uses `CanManageLibraries`).
- **`skip_to_ms` optional on create** — Defaults to `end_ms` when not provided, matching the design's "For credits, this is typically `end_ms`" guidance. The analysis pipeline (Task 5) sets `skip_to_ms = end_ms - intro_end_padding_ms` for intros; manual creation lets the user pick (most users will accept the default).
- **`confidence` optional on create, defaults to 1.0** — Matches the design's "Chapter markers are always 1.0" precedent for human-authored segments. Manual segments are authoritative.

**Not yet implemented (deferred to Tasks 2–5):**

- All five service functions are `todo!()` stubs — Task 2 implements list/create/update/delete against the `media_segments` table; Task 5 (`workers/segment_detector.rs`) implements `trigger_library_analysis` enqueuing work on the scheduler.
- `segment_analysis` scheduled task seeding — Task 5 adds the task to `seed_default_tasks()` (mirroring `subtitle_auto_fetch` from Phase 9 Task 7) plus a migration seeding it for existing deployments.
- Detection methods (chapter regex matching, chromaprint fingerprinting, black frame + silence via FFmpeg) — Task 2 implements the `services/segments.rs` module containing all four methods and the confidence scoring/2s padding logic.
- `PlaybackError::MediaNotFound` integration — When `start_playback` is updated to embed segments in its response (per "Streaming" integration above), the playback service will call `segments::service::list_segments` and include the results in `PlaybackStartResponse`. The `PLAY_001` mapping is reserved for this path.

### Phase 10 Task 2 — Detection Pipeline (Complete)

The four detection methods, confidence scoring, and 2-second safety padding land in `server/src/services/segments.rs` as a stateless library of pure/async functions. The CRUD layer for `media_segments` (list/create/update/delete) lands in `domains/segments/service.rs`, replacing the Task 1 `todo!()` stubs. `trigger_library_analysis` stays a stub — it is Task 5 (the worker) territory.

**Files built:**

| File | Purpose |
|---|---|
| `server/src/services/segments.rs` | Stateless detection library — chapter regex matching, chromaprint fingerprinting + cross-episode comparison, FFmpeg `blackframe` parser, FFmpeg `silencedetect` parser, search-window helpers, confidence scoring table, 2s padding applier, combined blackframe+silence credits detector |
| `server/src/services/mod.rs` | Added `pub mod segments;` |
| `server/src/domains/segments/service.rs` | `list_segments` (SELECT with optional type filter, verify media item), `create_segment` (validate type + timestamps, default confidence=1.0 + skip_to_ms=end_ms, INSERT with is_manual=true source='manual', unique-violation → `ManualSegmentExists`), `update_segment` (SELECT-then-COALESCE, revalidate), `delete_segment` (DELETE, rows-affected → `SegmentNotFound`); `trigger_library_analysis` stays as `todo!()` (Task 5) |
| `Cargo.toml` | `chromaprint-next = "0.1"` added to workspace deps |
| `server/Cargo.toml` | `chromaprint-next.workspace = true`, `regex.workspace = true` (already present) |

**Key decisions reconciled with this design doc:**

- **`chromaprint-next` 0.1 confirmed via crates.io research** — Released Feb 20 2026 by Attila Györffy. Pure-Rust, bit-identical to C reference across all 5 algorithm variants, MIT AND LGPL-2.1-or-later (LGPL on the resampler port of FFmpeg's `av_resample` only). API matches the design's pseudocode exactly: `Fingerprinter::new(Algorithm::default())` → `start(sample_rate, channels)` → `feed(&[i16])` → `finish()` → `fingerprint() -> &[u32]`. Default algorithm is `test2` (matches `media_fingerprints.fingerprint_algorithm` default).
- **`blackframe` parameter defaults** — FFmpeg's filter-level defaults are `amount=98, threshold=32`. SEGMENT_DETECTION.md specifies `amount=75, threshold=2`. The implementation makes both configurable via `BlackframeParams` and uses the design's stricter values as defaults — credit sequences are not pure black (text against a dim background), so 75% black-pixel threshold is the proven Jellyfin Intro Skipper value; threshold=2 means a pixel is "black" only if all YUV components are ≤2 (out of 255), again stricter than FFmpeg's default of 32.
- **`silencedetect` parameter defaults** — FFmpeg's filter-level defaults are `noise=-60dB, duration=2s`. SEGMENT_DETECTION.md specifies `noise=-55dB, duration=2s`. Configurable via `SilenceParams`; the design's slightly higher threshold (-55dB) is the default because end-credits music has low but non-zero volume and -60dB misses many credit boundaries.
- **FFmpeg `blackframe` stderr line format** — Per the official ffmpeg-filters documentation (section 11.13), each detection emits a line containing frame number, percentage of blackness, position-or-`-1`, and timestamp in seconds. FFmpeg also sets the metadata key `lavfi.blackframe.pblack`. The implementation parses the line format `frame:(\d+)\s+pblack:(\d+)\s+.*t:([\d.]+)` and ignores the `pts`/`type`/`last_keyframe` fields, which are unreliable across container formats.
- **FFmpeg `silencedetect` stderr line format** — Per ffmpeg-filters section 8.107, the filter emits `silence_start: <ts>` and `silence_end: <ts> | silence_duration: <dur>` (prefixed with the `[silencedetect @ 0x...]` log header), all timestamps in seconds. Output goes to **stderr** at INFO loglevel (FFmpeg default), so the implementation captures `output.stderr` and parses with simple `find("silence_start:")`/`find("silence_end:")` scans rather than relying on JSON output (FFmpeg metadata export via `-f json` does not include filter log lines).
- **PCM extraction for chromaprint via FFmpeg pipe** — `ffmpeg -i <file> -vn -ac 1 -ar 11025 -f s16le -acodec pcm_s16le pipe:1` writes raw signed-16-bit little-endian PCM to stdout in real time. The implementation reads stdout in 8 KiB chunks via `tokio::process::Command::stdout(Mutex<ChildStdout>)` and feeds each chunk to `Fingerprinter::feed()` after casting `&[u8]` → `&[i16]` (LE native-endian on x86/ARM, both of which are LE — little-endian conversion is unconditional via `bytemuck`-free manual cast through `i16::from_le_bytes` pairs to be portable). The fingerprinter's internal resampler is bypassed because FFmpeg already downmixed and resampled.
- **Fingerprint storage format** — Raw `&[u32]` reinterpreted as `&[u8]` (4 bytes per sub-fingerprint, native LE) and stored in `media_fingerprints.fingerprint` BYTEA. Loaded back with `bytemuck::cast_slice`-free manual `chunks_exact(4) → u32::from_le_bytes`. The encoded base64 form (`Fingerprinter::encode()`) is NOT persisted — it is only used for debug logging — because we compare raw u32 arrays in-process, not via AcoustID lookups.
- **Cross-episode comparison algorithm** — Pure function `find_recurring_segments(&[FingerprintWithContext]) -> Vec<RecurringMatch>`. For each ordered pair `(ep_a, ep_b)` in a season: slide `ep_b`'s fingerprint across `ep_a`'s; at each offset compute the fraction of sub-fingerprint pairs with Hamming similarity ≥ 30/32 bits (the standard Chromaprint "exact-ish match" threshold). Track the longest contiguous run of high-similarity offsets above 30/32. If that run's duration (count × 11.6 ms) is within the intro duration window (15–120 s) and lies within the intro search window (first 25% of episode or first 10 min, whichever is smaller), it is a candidate. A candidate confirmed across 3+ episodes scores 0.9 base; 2-episode confirmation scores 0.7. The "first 10 minutes" cap is enforced at this layer, not later.
- **Search-window helpers** — Pure functions over `runtime_ms` returning `(start_ms, end_ms)`:
  - Intro TV: `(0, min(runtime_ms / 4, 10 * 60_000))`
  - Intro movie: `(0, 10 * 60_000)`
  - Credits TV: `(runtime_ms - runtime_ms * 3 / 10, runtime_ms)` (last 30%)
  - Credits movie: `(runtime_ms - runtime_ms / 5, runtime_ms)` (last 20%)
  - Recap TV: `(0, runtime_ms * 15 / 100)` (first 15%)
- **Confidence scoring table implemented as `match` arms** — The design's scoring table maps directly to a function `score_segment(source, matching_episodes, agreeing_methods) -> f32`. Modifiers (`+0.05` for black frame agreeing, `+0.1` for silence agreeing, `-0.2` for dark scene counter-indicators) are applied additively then clamped to `[0.0, 1.0]`. Final scores below `min_confidence` (default 0.7) are still written to DB but flagged in metadata as `"surfaced": false` — the client filters them; the admin can lower the threshold.
- **2-second `intro_end_padding_ms` applied as `skip_to_ms` shortening** — `apply_safety_padding(&mut DetectedSegment, safety)` sets `skip_to_ms = end_ms - safety.intro_end_padding_ms` for intros (clamped to `start_ms` floor), `skip_to_ms = end_ms + safety.credits_end_padding_ms` for credits (clamped to runtime ceiling). Manual segments skip the padding applier — admin-supplied timestamps are authoritative.
- **`source='combined'` for credits blackframe+silence** — When credits detection requires both methods per the "Multi-Method Validation for Credits" rule, the resulting `media_segments.source` is `'combined'` (already in the DB CHECK constraint list) with `metadata.methods = ["blackframe", "silence"]` for traceability. Lone blackframe-only credits are written with `source='blackframe'` and `confidence` capped at 0.5 (not surfaced by default).
- **`SegmentPipelineError` separate from `SegmentError`** — The pipeline (services layer) has its own error type because it surfaces *operational* failures (FFmpeg spawn failures, IO errors, chromaprint calculation failures) that the worker (Task 5) logs and skips, while `SegmentError` (domain layer) surfaces *API* failures (not-found, validation, conflict) that bubble up through `AppError` to the HTTP client. The two are kept deliberately separate — `SegmentPipelineError` does NOT implement `Into<SegmentError>` or `Into<AppError>`. The worker is the explicit translation point: it logs the pipeline error, records the failure on the file's `media_fingerprints.metadata` JSONB, and continues to the next file.
- **Validation statics reused, not duplicated** — `VALID_SEGMENT_TYPES` and `VALID_SEGMENT_SOURCES` already exist in `domains/segments/types.rs` and are re-exported via `pub use` from `services/segments.rs` so the pipeline library and CRUD layer share a single source of truth.
- **Chapter timecode parsing** — `ffprobe` writes chapter `start_time`/`end_time` as strings in either `SS.mmmmmm` format (most containers) or `H:MM:SS.mmmmmm` (rare; legacy MKV). `parse_chapter_time_ms(&str) -> Option<i32>` handles both via a `:` count check. Returns `None` on malformed values, which the chapter extractor treats as "skip this chapter" rather than failing the whole file.
- **`trigger_library_analysis` deliberately left as `todo!()`** — The `todo!()` panics if called before Task 5 lands; the API endpoint will return 500 if hit. This is intentional — Task 5 will replace it with a scheduler enqueue (mirroring `subtitle_auto_fetch` from Phase 9 Task 7). The handler is wired through `handlers::analyze_library_segments` and the route is registered in `mod.rs`, so no API surface change is needed when Task 5 lands — only the service function body changes.

**Not yet implemented (deferred to Task 5 / worker):**

- `trigger_library_analysis` — still `todo!()`; will enqueue the `segment_analysis` scheduled task.
- The `segment_analysis` task seeding migration — will be added with Task 5.
- Per-file orchestration (loop fingerprinting + comparison + blackframe/silence, write results) — the worker ties together the functions in `services/segments.rs` with the CRUD in `domains/segments/service.rs`.
- The `outro` segment type via silence-gap detection — implemented in the worker because it requires reading existing `credits` segments (chicken-and-egg within a single library scan).
