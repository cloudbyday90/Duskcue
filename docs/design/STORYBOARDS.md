# Storyboards Design

## Overview

**Storyboards** are seek-preview thumbnail grids displayed on the video timeline/seek bar during playback. When a user hovers or scrubs the seek bar, storyboard thumbnails appear at the corresponding timestamp — matching the experience of YouTube, Netflix, and other commercial streaming platforms. Each storyboard consists of a WebVTT index file mapping timestamp ranges to regions of a WebP sprite sheet image.

The name "Storyboard" is chosen over platform-specific terms ("trickplay" = Jellyfin, "BIF" = Roku/Plex, "video preview thumbnails" = verbose). "Storyboard" is a film industry term for showing key frames of a video — concise, memorable, and unique to our platform.

## Design Principles

**1. Cache data — regenerable, not permanent**

Storyboards are derived data that can be regenerated from the source media at any time. They are stored in the `/cache` directory (cache layer, not `/data`), and loss is acceptable — the server regenerates them on demand or via scheduled task.

**2. Background generation — no impact on playback**

Storyboard generation is CPU-intensive (FFmpeg thumbnail extraction). It runs as a background scheduled task, the same as segment analysis. Generation never blocks playback or scanning.

**3. Adaptive defaults — smart interval selection**

Rather than a fixed interval, the default interval adapts to content duration. Short content (episodes) gets more granular thumbnails; long content (movies) uses a wider interval. This optimizes both storage and visual usefulness.

**4. Standard formats — no custom parsers**

WebVTT + WebP sprite sheets are web standards. hls.js has native thumbnail support. Flutter and Tauri can parse WebVTT trivially. No custom binary format (BIF) or platform-specific parser needed.

---

## Format: WebVTT + WebP Sprite Sheets

### Why This Format

| Format | Single Request | Web Native | Custom Parser | Size (2hr, 10s) | Multi-Client |
|---|---|---|---|---|---|
| **BIF (Plex)** | Yes | No | Yes (binary) | ~15-20 MB | Roku native; others need parser |
| **WebVTT + sprite** | Yes (per sprite) | Yes (hls.js) | No | ~3-5 MB (WebP) | All platforms |
| **Individual JPEGs** | No (hundreds) | Yes | No | ~3-5 MB | All platforms |
| **Jellyfin grid** | Yes | Partial | Yes | ~4-5 MB | Jellyfin clients only |

**Decision:** WebVTT index file + WebP sprite sheet images. Standard, efficient, no custom parsers.

### Why WebP Over JPEG

- 25-35% smaller at equivalent quality
- Supported in all major browsers since 2020 (Chrome 32+, Firefox 65+, Safari 16+, Edge 18+)
- Alpha channel support if needed (transparent padding for aspect ratio mismatch)
- Hardware decoding support on most platforms

> The WebP choice aligns with the unified image format policy — see [IMAGE_FORMATS.md](IMAGE_FORMATS.md) for the project-wide format decision covering artwork, storyboards, overlays, and thumbnails. The decision research (AVIF rejected for encode cost on NAS hardware, JPEG XL rejected for browser support) applies here too.

### Sprite Sheet Layout

Each sprite sheet is a grid of thumbnail images. One sprite sheet holds N×M thumbnails.

| Parameter | Default | Description |
|---|---|---|
| Thumbnail width | 320px | Configurable: 160, 320, 640 |
| Thumbnail height | Auto (maintain aspect) | Calculated from source aspect ratio |
| Columns | 10 | Thumbnails per row |
| Rows | 20 | Thumbnails per column |
| Thumbnails per sheet | 200 | columns × rows |
| Image format | WebP (lossy, 75% quality) | Configurable quality: 50-100 |

A 2-hour movie at 10-second intervals produces ~720 thumbnails = ~4 sprite sheets. Each sprite sheet covers ~33 minutes of content.

### WebVTT Index File

The WebVTT file maps timestamp ranges to sprite sheet regions:

```webvtt
WEBVTT

00:00:00.000 --> 00:00:10.000
sprite_001.webp#xywh=0,0,320,180

00:00:10.000 --> 00:00:20.000
sprite_001.webp#xywh=320,0,320,180

00:00:20.000 --> 00:00:30.000
sprite_001.webp#xywh=640,0,320,180
```

The `#xywh=` fragment specifies the x-offset, y-offset, width, and height of the thumbnail region within the sprite sheet. Clients use this to extract and display the correct thumbnail for a given timestamp.

### Storage Path

```
/cache/storyboards/{media_file_id}/
    index.vtt              — WebVTT index for the primary sprite set
    sprite_001.webp        — First sprite sheet (thumbnails 1-200)
    sprite_002.webp        — Second sprite sheet (thumbnails 201-400)
    sprite_003.webp        — etc.
```

`media_file_id` is used (not `media_item_id`) because multi-version items (e.g. 4K + 1080p) may have different aspect ratios, requiring separate storyboards per file.

---

## Generation Pipeline

### FFmpeg Two-Phase Pipeline

**Phase 1: Extract individual frames** via FFmpeg `tokio::process::Command`:

```bash
ffmpeg -i "input.mkv" \
    -vf "fps=1/10,scale=320:trunc(ow/a/2)*2" \
    -c:v mjpeg -q:v 4 \
    -f image2 \
    "/tmp/storyboards/{session_id}/frame_%08d.jpg"
```

- `fps=1/10` — one frame every 10 seconds (configurable interval)
- `scale=320:trunc(ow/a/2)*2` — scale to 320px width, maintain aspect, ensure even dimensions
- Individual frames written to a temporary directory

**Phase 2: Assemble sprite sheets**:

```bash
ffmpeg -i "/tmp/storyboards/{session_id}/frame_%08d.jpg" \
    -vf "tile=10x20:padding=0:margin=0" \
    -c:v webp -lossless 0 -q:v 75 \
    "/cache/storyboards/{media_file_id}/sprite_001.webp"
```

Or batch per 200 frames:

```bash
ffmpeg -start_number 1 -i "frame_%08d.jpg" \
    -vf "tile=10x20" \
    -frames:v 1 \
    -c:v webp -lossless 0 -q:v 75 \
    "sprite_001.webp"

ffmpeg -start_number 201 -i "frame_%08d.jpg" \
    -vf "tile=10x20" \
    -frames:v 1 \
    -c:v webp -lossless 0 -q:v 75 \
    "sprite_002.webp"
```

The WebVTT index file is generated by the Rust application based on the number of frames extracted and the interval used — no FFmpeg step needed.

### Keyframe-Only Mode (Fast Generation)

When `keyframe_only = true` (the default), FFmpeg only extracts frames at keyframe positions:

```bash
ffmpeg -skip_frame nokey \
    -i "input.mkv" \
    -vf "fps=1/10,scale=320:trunc(ow/a/2)*2" \
    -c:v mjpeg -q:v 4 \
    -f image2 \
    "frame_%08d.jpg"
```

This is ~100x faster than full frame extraction because FFmpeg skips inter-frame decoding. The trade-off is that thumbnail timestamps may not align exactly to the configured interval — they snap to the nearest keyframe instead. For seek bar previews, this is imperceptible to users.

### Adaptive Interval

The interval between thumbnails adapts to content duration:

| Content Duration | Interval | Thumbnails (approx.) | Sprite Sheets |
|---|---|---|---|
| < 30 minutes (short episodes) | 5 seconds | ~360 | 2 |
| 30-120 minutes (movies) | 10 seconds | ~180-720 | 1-4 |
| > 120 minutes (long movies) | 15 seconds | ~480+ | 2-3 |

The admin can override with a fixed interval (range: 2-120 seconds). The adaptive formula is:

```rust
fn adaptive_interval(duration_seconds: u32) -> u32 {
    match duration_seconds {
        0..=1800 => 5,     // ≤30 min → 5s
        1801..=7200 => 10, // 30-120 min → 10s
        _ => 15,           // >120 min → 15s
    }
}
```

---

## Storage Estimation

| Library Size | Interval | Resolution | Storage |
|---|---|---|---|
| 100 movies (avg 2hr) | 10s | 320px | ~400-500 MB |
| 1,000 movies | 10s | 320px | ~4-5 GB |
| 5,000 movies | 10s | 320px | ~20-25 GB |
| 1,000 movies | 5s | 320px | ~8-10 GB |
| 1,000 movies | 10s | 640px | ~15-20 GB |

At the default 10-second interval and 320px resolution, storyboards consume ~4-5 MB per movie — far more efficient than Plex's BIF format (10-50 MB per movie at 2-second intervals).

---

## Scheduled Task

Storyboard generation runs as a scheduled task (`storyboard_generation`) via the existing `scheduled_tasks` system:

| Parameter | Value |
|---|---|
| Task type | `storyboard_generation` |
| Default schedule | `0 4 * * *` (daily 04:00, after segment analysis) |
| Timeout | 4 hours |
| Max concurrent | 1 (CPU-intensive) |
| Config | `{ "max_concurrent_analyses": 1, "interval_mode": "adaptive" }` |

### Pipeline Steps

```
For each library with storyboards enabled:

  1. Resolve files needing storyboards:
     - New files (no storyboard entry) → generate
     - Changed files (file_hash differs) → regenerate
     - Existing storyboards → skip (cached)

  2. For each file to process:
     a. Determine interval (adaptive or fixed from config)
     b. Determine resolution (from config, default 320px)
     c. Check if keyframe-only mode is enabled
     d. Phase 1: Extract frames via FFmpeg
     e. Phase 2: Assemble sprite sheets via FFmpeg
     f. Generate WebVTT index file
     g. Write storyboard metadata to database
     h. Clean up temporary frame files

  3. Report results:
     - Storyboards created, updated, skipped per library
     - Errors logged per file
     - Storage consumed
```

### Incremental Generation

```sql
SELECT mf.id, mf.media_item_id, mf.file_path, mf.file_hash, mf.runtime_seconds
FROM media_files mf
JOIN media_items mi ON mf.media_item_id = mi.id
WHERE mi.library_id = $1
AND NOT EXISTS (
    SELECT 1 FROM storyboards sb
    WHERE sb.media_file_id = mf.id
    AND sb.file_hash = mf.file_hash
);
```

Only files without storyboards (or with changed hashes) are processed. This makes subsequent runs fast.

### Manual Trigger

Admins can manually trigger storyboard generation per library or per item:

- `POST /api/v1/libraries/{id}/generate-storyboards` — generate for all missing items in library
- `POST /api/v1/items/{id}/generate-storyboards` — generate for a specific item (force regen)

---

## API Endpoints

### Storyboard Retrieval

| Method | Endpoint | Description |
|---|---|---|
| `GET` | `/api/v1/items/{id}/storyboard` | Get storyboard metadata for a media item (sprite URLs, dimensions, interval) |
| `GET` | `/api/v1/items/{id}/storyboard/index.vtt` | Serve the WebVTT index file |
| `GET` | `/api/v1/items/{id}/storyboard/{sprite}` | Serve a sprite sheet image |
| `POST` | `/api/v1/libraries/{id}/generate-storyboards` | Trigger storyboard generation for a library |
| `POST` | `/api/v1/items/{id}/generate-storyboards` | Trigger storyboard generation for a specific item |
| `DELETE` | `/api/v1/items/{id}/storyboard` | Delete cached storyboard data for an item |

### Storyboard Response Format

```json
{
    "media_file_id": "uuid",
    "interval_seconds": 10,
    "width": 320,
    "height": 180,
    "sprite_count": 4,
    "total_thumbnails": 720,
    "index_url": "/api/v1/items/{id}/storyboard/index.vtt",
    "sprites": [
        {
            "url": "/api/v1/items/{id}/storyboard/sprite_001.webp",
            "thumbnails": 200,
            "columns": 10,
            "rows": 20
        }
    ],
    "generated_at": "2026-05-31T04:15:00Z"
}
```

### Integration with Playback

When a client starts playback, the server includes storyboard metadata in the playback start response:

```json
{
    "session_id": "uuid",
    "stream_url": "/api/v1/stream/...",
    "storyboard": {
        "index_url": "/api/v1/items/{id}/storyboard/index.vtt",
        "interval_seconds": 10,
        "width": 320,
        "height": 180
    },
    "segments": [...]
}
```

The client loads the WebVTT index once and caches it for the session. Thumbnails are fetched on demand as the user scrubs the seek bar.

### hls.js Integration

hls.js supports thumbnail tracks natively. When using HLS transcoding, the storyboard can be loaded as a supplementary track:

```javascript
if (Hls.isSupported()) {
    const hls = new Hls();
    hls.loadSource(manifestUrl);
    hls.attachMedia(video);

    hls.on(Hls.Events.MANIFEST_PARSED, () => {
        // Load storyboard as thumbnail track
        hls.addTrack({
            kind: 'thumbnails',
            url: storyboardIndexUrl
        });
    });
}
```

For native HLS (Safari, Chrome 142+), the client uses a custom seek bar component that fetches thumbnails from the WebVTT index.

---

## Configuration

### Server-Wide Storyboard Config

Stored in `server_config.transcoding` JSONB (existing column):

```json
{
    "storyboards_enabled": true,
    "storyboard_interval_mode": "adaptive",
    "storyboard_fixed_interval_seconds": 10,
    "storyboard_width": 320,
    "storyboard_quality": 75,
    "storyboard_keyframe_only": true,
    "storyboard_sprite_columns": 10,
    "storyboard_sprite_rows": 20
}
```

| Field | Type | Default | Description |
|---|---|---|---|
| `storyboards_enabled` | bool | `true` | Enable/disable storyboard generation |
| `storyboard_interval_mode` | string | `"adaptive"` | `"adaptive"` or `"fixed"` |
| `storyboard_fixed_interval_seconds` | u32 | `10` | Fixed interval when mode is `"fixed"`; range: 2-120 |
| `storyboard_width` | u32 | `320` | Thumbnail width in pixels; valid: 160, 320, 640 |
| `storyboard_quality` | u32 | `75` | WebP quality (lossy); range: 50-100 |
| `storyboard_keyframe_only` | bool | `true` | Use keyframe-only extraction (100x faster, less frame-accurate) |
| `storyboard_sprite_columns` | u32 | `10` | Thumbnails per row in sprite sheet |
| `storyboard_sprite_rows` | u32 | `20` | Thumbnails per column in sprite sheet |

### Per-Library Storyboard Config

Stored in `libraries.metadata` JSONB:

```json
{
    "storyboards_enabled": true,
    "storyboard_width": 320,
    "storyboard_fixed_interval_seconds": 10
}
```

Per-library config overrides server-wide config for that library. This allows different resolutions or intervals for different library types (e.g., 640px for a 4K movie library, 160px for a TV show library).

---

## Metrics

Storyboard metrics are exposed via the existing Prometheus `/metrics` endpoint:

| Metric | Type | Labels | Description |
|---|---|---|---|
| `storyboard_files_processed_total` | counter | library | Files processed for storyboard generation |
| `storyboard_generation_duration_seconds` | histogram | library | Time to generate storyboards for one file |
| `storyboard_sprites_created_total` | counter | library | Sprite sheet images created |
| `storyboard_storage_bytes` | gauge | library | Current disk usage of storyboard cache |
| `storyboard_served_total` | counter | — | Storyboard index/sprite HTTP requests served |
| `storyboard_generation_errors_total` | counter | library, error_type | Generation failures by type |

---

## Integration with Existing Systems

### Media Scanning (MEDIA_SCANNING.md)

Storyboard generation runs after media scanning and segment analysis complete. No interaction with the scanning pipeline — it only reads `media_files` to find new/changed files.

### Streaming (STREAMING.md)

The playback start endpoint (`POST /api/v1/playback/start`) includes storyboard metadata in the response alongside stream URLs and segment data. The client uses this to load thumbnail previews on the seek bar.

### Scheduled Tasks (DATABASE.md — System Domain)

New task type `storyboard_generation` added to the `scheduled_tasks.task_type` CHECK constraint. Runs as a standard scheduled task with full run history, error tracking, and auto-disable on consecutive failures.

### Docker Deployment (DOCKER_DEPLOYMENT.md)

Storyboards are stored in `/cache/storyboards/`, which is part of the cache directory. In Docker, this is a persistent volume (not tmpfs) since storyboards should survive container restarts. Storyboards are cache data (regenerable) but regenerating a full library is expensive.

---

## Error Handling

Storyboard API errors use existing error codes:

- `MEDIA_001` (404) — media item not found
- `MEDIA_002` (404) — media file not found
- `SYS_002` (409) — storyboard generation already running for this library

New error code:

| Code | HTTP | Description |
|---|---|---|
| `MEDIA_007` | 404 | Storyboard not found (not yet generated for this item) |

Generation failures are logged and tracked in `scheduled_task_runs` — they don't produce API errors since generation is a background task.

---

## Implementation Notes

### Phase 10 Task 3 — Domain Scaffolding (Complete)

Storyboard retrieval, serving, generation-trigger, and deletion API surface implemented. The generation pipeline itself (FFmpeg frame extraction, WebP sprite assembly, WebVTT index authoring) lands in Task 4 (`services/storyboards.rs`) and Task 6 (`workers/storyboard_generator.rs`).

**Files built:**

| File | Purpose |
|---|---|
| `server/src/domains/storyboards/mod.rs` | Module declarations + router assembly with 5 route groups (6 endpoints) |
| `server/src/domains/storyboards/error.rs` | `StoryboardError` enum with 7 variants covering not-found, conflict, validation, and Database catch-all |
| `server/src/domains/storyboards/types.rs` | Three-type DTOs: `StoryboardRow` (internal, 14 fields matching `storyboards` table schema), `StoryboardResponse`/`SpriteResponse`/`GenerateStoryboardsResponse`/`DeleteStoryboardResponse` (Serialize); `VALID_STORYBOARD_WIDTHS`/`VALID_INTERVAL_MODES` statics |
| `server/src/domains/storyboards/service.rs` | 6 `todo!()` service function stubs — get_storyboard, get_storyboard_index, get_storyboard_sprite, trigger_library_generation, trigger_item_generation, delete_storyboard |
| `server/src/domains/storyboards/handlers.rs` | 6 working handlers wired to Axum extractors; retrieval endpoints use `AuthenticatedUser`, mutation/generation endpoints use `Require<CanManageLibraries>` |
| `server/src/error.rs` | `AppError::Storyboard(#[from] StoryboardError)` variant + `storyboard_error_to_http()` mapping |
| `server/src/router.rs` | Storyboards router merged via `.merge(crate::domains::storyboards::router(state.clone()))` |
| `server/src/domains/mod.rs` | `pub mod storyboards;` added |

**Decisions reconciled with this design doc:**

- **Error code mapping confirmed** — The "Storyboard API errors use existing error codes" rule is honored. `StoryboardError` variants map: `MediaItemNotFound` → `MEDIA_001` (404); `MediaFileNotFound` → `MEDIA_002` (404); `StoryboardNotFound` → `MEDIA_007` (404, already registered in the error code table); `LibraryNotFound` → `LIB_001` (404); `GenerationAlreadyInProgress` → `SYS_002` (409, the scheduled-task-already-running code); `InvalidSpriteFilename` → `VALID_001` (422); `Database` → `INTERNAL` (500). No new error codes registered.
- **Binary-serving endpoints follow playback domain pattern** — `get_storyboard_index` and `get_storyboard_sprite` return `Result<Response, AppError>` (not `Json<T>`) because they serve non-JSON content: `text/vtt; charset=utf-8` for the WebVTT index, `image/webp` for sprite sheets. This mirrors the playback domain's `stream_file` / `get_transcode_segment` handlers that serve HLS manifests and fMP4 segments.
- **Cache headers per content type** — WebVTT index: `Cache-Control: public, max-age=3600` (regenerable, may change if storyboard is re-generated). Sprite sheets: `Cache-Control: public, max-age=86400, immutable` (immutable once written — sprite filenames are stable per generation since they're keyed by `media_file_id` which doesn't change). This differs from the playback HLS pattern (`no-cache` for live transcode manifests) because storyboards are static derived data, not live session state.
- **`index.vtt` static route coexists with `{sprite}` capture** — Axum's matchit router prioritizes static path segments over dynamic captures at the same depth, so `GET /storyboard/index.vtt` routes to the index handler and `GET /storyboard/sprite_001.webp` routes to the sprite handler without conflict. No explicit disambiguation needed.
- **Authorization splits retrieval from mutation** — Retrieval endpoints (`GET storyboard`, `GET index.vtt`, `GET sprite`) require `AuthenticatedUser` only — any logged-in user can view seek previews during playback. Generation and deletion endpoints (`POST generate-storyboards`, `DELETE storyboard`) require `Require<CanManageLibraries>` — storyboard generation is CPU-intensive and cache eviction is an administrative action. This matches the segments domain convention where `analyze-segments` is admin-only but segment retrieval is open to all authenticated users.
- **`cache_dir` from `BootstrapConfig.data_dir`** — Handlers construct `state.bootstrap.data_dir.join("cache")` and pass as `&Path` to service functions. Storyboards live in `{data_dir}/cache/storyboards/{media_file_id}/` per the Storage Path spec. Service signatures use `&Path` (not `&PathBuf`) per clippy `ptr_arg` convention.
- **`GenerateStoryboardsResponse` is scope-agnostic** — Single response type `{ queued: bool, message: String }` serves both library and item trigger endpoints. The route context (`/libraries/{id}/` vs `/items/{id}/`) tells the client which scope was triggered. Follows the segments domain's `AnalyzeSegmentsResponse` minimal shape; avoids type duplication for two endpoints with identical response semantics.
- **`InvalidSpriteFilename` reserved for path traversal protection** — Task 4 service implementation will validate sprite filenames against the expected `sprite_NNN.webp` pattern before constructing disk paths, rejecting names containing `..`, `/`, `\`, or non-matching patterns. Mapped to `VALID_001` (422) — matches the playback domain's segment filename validation approach (`validate_segment_filename` rejects `..`, `/`, `\`, names >64 chars, non-`seg_` prefixed).

**Not yet implemented (deferred to Tasks 4 and 6):**

- ~~All six service functions are `todo!()` stubs~~ — Task 4 implements `get_storyboard`, `get_storyboard_index`, `get_storyboard_sprite`, and `delete_storyboard` (DB queries + disk reads); Task 6 (`workers/storyboard_generator.rs`) implements the generation triggers (`trigger_library_generation`, `trigger_item_generation`) by enqueuing work on the scheduler.
- ~~FFmpeg two-phase pipeline (frame extraction → sprite assembly)~~ — Task 4 (`services/storyboards.rs`) implements the generation library using a refined single-command-per-sheet filtergraph (see Task 4 notes below); Task 6 wires it into a scheduled task worker.
- ~~Adaptive interval selection~~ — `adaptive_interval()` function per the Generation Pipeline spec; landed in `services/storyboards.rs` (Task 4).
- Storyboard metadata in playback start response — When `start_playback` is updated to include the storyboard block per the "Integration with Playback" spec, the playback service will call `storyboards::service::get_storyboard` and embed the result in `PlaybackStartResponse`.
- `storyboard_generation` scheduled task already seeded (migration `20260530_070000_seed_default_data.sql`, daily 04:00) — Task 6 registers the executor on the scheduler in `main.rs`.

### Phase 10 Task 4 — Generation Library + Domain Service Implementation (Complete)

The generation pipeline (FFmpeg frame extraction, WebP sprite assembly, WebVTT index authoring) and the four read/delete domain service functions are now implemented. The trigger functions (Task 6 worker territory) remain `todo!()` stubs.

**Files built:**

| File | Purpose |
|---|---|
| `server/src/services/storyboards.rs` | Generation library: `GenerationConfig`, `SpriteLayout`, `GenerationResult`, `StoryboardPipelineError`; `adaptive_interval()`, `compute_sprite_layout()`, `generate_storyboard()` (one FFmpeg invocation per sprite sheet), `build_webvtt_index()` (pure function), `format_timecode_secs()`, `validate_sprite_filename()`, `sprite_filename()`, `inspect_sprite_height()` (WebP RIFF parser); 39 unit tests |
| `server/src/services/mod.rs` | Added `pub mod storyboards;` |
| `server/src/domains/storyboards/service.rs` | Replaced 4 of 6 `todo!()` stubs: `get_storyboard`, `get_storyboard_index`, `get_storyboard_sprite`, `delete_storyboard`. `trigger_library_generation`/`trigger_item_generation` remain for Task 6. 13 new domain tests |

**Decisions reconciled with this design doc:**

- **Single-command per-sheet filtergraph replaces two-phase pipeline** — The "Generation Pipeline" section describes a two-phase approach: (1) extract individual frames to a temp directory via FFmpeg, then (2) assemble sprite sheets from those frames via a second FFmpeg invocation per batch of 200. Research (June 2026) showed the modern best practice is a single FFmpeg filtergraph per sprite sheet: `fps=1/N,scale=W:trunc(ow/a/2)*2,tile=COLSxROWS -frames:v 1`. This eliminates temp-frame disk I/O entirely (no `/tmp/storyboards/{session_id}/frame_%08d.jpg` directory to manage), simplifies cleanup (FFmpeg owns its own temp state), and is the pattern used by the design's own Research Sources (Jellyfin 10.9+, the MTG and Id_rs implementations). For multi-sheet videos, the generator makes one FFmpeg call per sheet with `-ss <start> -t <window>` seek windows. The two-phase pipeline in the design doc is retained as historical context but the implementation uses the single-command refinement.
- **`-ss` before `-i` for fast keyframe-accurate seek** — The seek window `-ss <start_secs>` is placed *before* the `-i <source>` argument so the demuxer jumps directly to the keyframe at or before the start timestamp. Placing `-ss` after `-i` forces FFmpeg to decode every frame from the beginning of the file up to the seek point — catastrophic for long content. The masonwritescode reference implementation calls this out explicitly.
- **`-skip_frame nokey` placement** — When `keyframe_only = true` (the default), `-skip_frame nokey` is placed *before* `-i` so the demuxer skips non-keyframe packets during demuxing. The FFmpeg documentation's canonical spritesheet example uses this exact ordering: `ffmpeg -skip_frame nokey -i file.avi -vf 'scale=128:72,tile=8x8' -an -vsync 0 keyframes.png`. Placing it after `-i` still works but loses most of the speedup because frames are demuxed before being discarded.
- **Final WebVTT cue extends to `duration_seconds`** — The "no gap at the end" rule: the cue for the last thumbnail covers `[N*interval, duration)` rather than `[N*interval, (N+1)*interval)`. Without this, clients show a dead zone at the end of the seek bar where no thumbnail appears. Implemented via `duration.max((i + 1) * interval)` in `build_webvtt_index`.
- **Drift prevention** — Research identified "WebVTT interval must equal FFmpeg's `fps=1/N`" as the #1 source of preview drift (thumbnails wander away from the seek position as the user scrubs). Mitigated by generating the WebVTT index inside `generate_storyboard()` — the same call that invokes FFmpeg — so both consume the same `GenerationConfig.interval_seconds` value. The `build_webvtt_index` doc-comment carries a "Drift warning" callout.
- **Sprite filename validation enforces 1-based 1-4 digit numbers** — `sprite_NNN.webp` where NNN is `[1-9999]`. 3-digit zero-padding matches the WebVTT cue examples in this design doc; the validator accepts up to 4 digits so a 4-hour movie at 5s interval (~288 sheets) and pathological longer content still parse. `sprite_000` is rejected (1-based). Path separators (`/`, `\`), `..` traversal attempts, non-`.webp` suffixes, and non-`sprite_` prefixes all rejected with descriptive error strings. Mapped to `VALID_001` (422) at the HTTP boundary.
- **WebP RIFF header parser replaces image-library dependency** — The design stores thumbnail `height` in the DB row (computed from source aspect ratio). Rather than add a Rust image-library dependency, `inspect_sprite_height()` parses the WebP container directly: VP8 lossy (`b"VP8 "`) reads 16-bit LE width/height at byte offsets 26/28; VP8 lossless (`b"VP8L"`) reads the 14-bit packed width/height from bytes 22-24; falls back to 180px (16:9 at 320 wide) for unparseable headers. Same approach as services/subtitles.rs OCR engine detection — avoid heavy dependencies for trivial parsing.
- **Grid shape recovered from `metadata.columns`/`metadata.rows` JSONB** — The `storyboards` table has no explicit columns/rows fields, but `SpriteResponse` includes them per the design's Response Format. The worker (Task 6) writes `metadata.columns` and `metadata.rows` when creating the row; the domain service's `read_grid_shape()` recovers them, defaulting to the design's 10×20 when missing (handles externally-authored or future-config rows). Validation rejects zero/negative so a malformed metadata payload cannot break URL construction.
- **`resolve_primary_media_file` mirrors playback domain** — Same query (`is_healthy=true ORDER BY file_size DESC LIMIT 1`) as `domains::playback::service::resolve_media_file`. Storyboards correspond to the file the user will actually stream; keeping the selection identical means the storyboard is always for the right file. Multi-version items (4K + 1080p) get one storyboard for the primary file, matching the "media_file_id" rationale in the Storage Path section.
- **Delete is idempotent + best-effort disk cleanup** — DB deletion via `DELETE ... RETURNING` is the source of truth; if no row exists, returns `StoryboardNotFound` *after* still attempting on-disk cleanup (handles crashed-generation drift). On-disk `remove_dir_all` failures (except NotFound) are logged at WARN but do not invalidate the committed DB deletion — derived data can always regenerate.

**Not yet implemented (deferred to Task 6 / worker):**

- `trigger_library_generation` and `trigger_item_generation` — still `todo!()`; Task 6 will replace them with scheduler enqueue (mirroring `subtitle_auto_fetch` from Phase 9 Task 7 and `segment_analysis` from Phase 10 Task 5)
- `storyboard_generation` scheduled task already seeded (migration `20260530_070000_seed_default_data.sql`, daily 04:00) — Task 6 registers the executor on the scheduler in `main.rs`
- `RuntimeConfig.transcoding.storyboard_*` config fields — Task 6 expands `TranscodingConfig` with the 8 storyboard fields from the Configuration section and constructs `GenerationConfig` per-file
- Per-library config overrides (`libraries.metadata.storyboards_*`) — Task 6 worker reads these and overrides the server-wide config when constructing `GenerationConfig`
- Sandbox application — Task 6 worker calls `services::sandbox::apply_sandbox` before each FFmpeg invocation (Linux landlock + seccomp per [SECURITY.md](../security/SECURITY.md); no-op on Windows/macOS)
- Web client `SeekPreview.svelte` — Task 8 consumes the `/storyboard/index.vtt` endpoint via hls.js or a custom seek-bar component

---



### Platform Approaches
- Plex Support — Video Preview Thumbnails (BIF format, 2-second intervals, 10-50 MB per item)
- Jellyfin 10.9+ Trickplay — Sprite sheet grids, configurable resolution, keyframe-only mode
- Netflix — BIF format internally, served as blob URIs to client
- Roku — BIF (Base Index Format) specification for video preview thumbnails

### Web Standards
- WebVTT — W3C Web Video Text Tracks Format specification, thumbnail cue support
- hls.js — Thumbnail track support via `THUMBNAILS_LOADED` event and cue rendering
- MDN — HTMLMediaElement.textTracks API for programmatic track access

### FFmpeg
- FFmpeg Documentation — `fps` filter (frame rate conversion), `scale` filter, `tile` filter
- FFmpeg Documentation — `skip_frame` option for keyframe-only extraction
- FFmpeg Documentation — WebP encoder (`libwebp`), quality and compression settings

### Sprite Sheet Approaches
- Jellyfin Trickplay implementation — individual JPEG → montage grid (github.com/jellyfin/jellyfin)
- MTG — FFmpeg sprite sheet generation for video thumbnails (September 2023)
- Id_rs — Video thumbnail spritesheet generation with FFmpeg (March 2025)
