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
/cache/storyboards/{media_file_id}/{artifact_id}/
    index.vtt              — WebVTT index for this complete sprite set
    sprite_001.webp        — First sprite sheet (thumbnails 1-200)
    sprite_002.webp        — Second sprite sheet (thumbnails 201-400)
    sprite_003.webp        — etc.
```

`media_file_id` is used (not `media_item_id`) because multi-version items (e.g. 4K + 1080p) may have different aspect ratios, requiring separate storyboards per file. `artifact_id` is a UUIDv7 written to `storyboards.artifact_id`; it points at the only complete artifact set the server may serve.

Generation writes into a new artifact directory under the media-file directory while holding a transaction-scoped PostgreSQL advisory lock derived from `media_file_id`. It upserts the row and its `artifact_id` only after FFmpeg completes, so failed or cancelled work leaves the last complete set live. Rows created before artifact versioning retain a null `artifact_id` and resolve to the legacy `{media_file_id}/` layout until the next successful regeneration. After scheduled or manual generation, reconciliation acquires the same non-blocking lock per media file and removes only unreferenced directories and legacy files; an active generation is skipped rather than touched.

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
      - Changed files (`file_hash` differs with null-safe comparison) → regenerate
      - Changed normalized generation configuration → regenerate
      - Matching source + configuration fingerprint → skip (cached)

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
SELECT mf.id, mf.media_item_id, mf.file_path, mf.file_hash, mf.runtime_seconds,
       sb.file_hash AS storyboard_file_hash,
       sb.config_fingerprint
FROM media_files mf
JOIN media_items mi ON mf.media_item_id = mi.id
LEFT JOIN storyboards sb ON sb.media_file_id = mf.id
WHERE mi.library_id = $1
AND mf.is_healthy = true;
```

The worker calculates the effective per-file interval, then compares nullable source hashes as values and compares the normalized `v1` generation fingerprint (interval, width, quality, keyframe mode, and grid). Ordinary PostgreSQL equality yields unknown for null operands, so this deliberately avoids a `file_hash = file_hash` freshness predicate; [PostgreSQL's comparison-predicate documentation](https://www.postgresql.org/docs/current/functions-comparison.html) describes the required null-safe semantics. A null hash is valid when both the source and stored row are null; legacy empty hashes are migrated to null. A missing fingerprint causes exactly one regeneration, so settings changes do not silently retain old sprites.

### Manual Trigger

Admins can manually trigger storyboard generation per library or per item:

- `POST /api/v1/libraries/{id}/generate-storyboards` — generate for all missing items in library
- `POST /api/v1/items/{id}/generate-storyboards` — generate for a specific item (force regen)

---

## API Endpoints

### Storyboard Retrieval

| Method | Endpoint | Description |
|---|---|---|
| `GET` | `/api/v1/items/{id}/storyboard?media_file_id={id}` | Get storyboard metadata for the playback file; omitting the optional query uses the primary healthy file |
| `GET` | `/api/v1/items/{id}/storyboard/index.vtt?media_file_id={id}` | Serve the WebVTT index for the playback file |
| `GET` | `/api/v1/items/{id}/storyboard/{sprite}?media_file_id={id}` | Serve a sprite sheet image for the playback file |
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
    "index_url": "/api/v1/items/{id}/storyboard/index.vtt?media_file_id={media_file_id}",
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

After playback starts, the client fetches storyboard metadata with the exact
`media_file_id` selected for that stream:

```json
{
    "index_url": "/api/v1/items/{id}/storyboard/index.vtt?media_file_id={media_file_id}",
    "interval_seconds": 10,
    "width": 320,
    "height": 180
}
```

The server validates that the requested file belongs to the item and is healthy.
It rewrites WebVTT sprite references with that same file ID, so the index and
sprites cannot drift to a different cut or rendition. The client loads the
WebVTT index once for the player session; the HTTP responses use
`Cache-Control: private, no-store` because profile access can change during a
shared browser session.

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
| `storyboard_sprite_columns` | u32 | `10` | Thumbnails per row in sprite sheet; range: 1-20 |
| `storyboard_sprite_rows` | u32 | `20` | Thumbnails per column in sprite sheet; range: 1-40 |

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
| `storyboard_files_processed_total` | counter | `outcome` (`generated`, `already_running`, `missing_source`, `pipeline_error`, `database_error`) | File-level generation attempts, including the resolved terminal result |
| `storyboard_generation_duration_seconds` | histogram | — | FFmpeg generation duration for successfully published storyboard artifacts |
| `storyboard_sprites_created_total` | counter | — | Sprite sheet images made visible by successful artifact publication |
| `storyboard_storage_bytes` | gauge | — | Current byte size of the complete storyboard cache tree after reconciliation |
| `storyboard_served_total` | counter | `asset` (`index`, `sprite`), `outcome` (`success`, `error`) | Authenticated WebVTT and WebP asset reads |
| `storyboard_generation_errors_total` | counter | `kind` (`missing_source`, `pipeline`, `database`) | Generation failures by bounded operational class |

### Observability Decision (2026-07-18)

| Option | Advantages | Limitations | Decision |
|---|---|---|---|
| Label every metric with `library_id`, media ID, file path, or raw FFmpeg/database error text | Direct per-object drill-down from one metric. | Each new library, file, path, or error string creates a Prometheus time series; it is unbounded, private, and unsuitable for a long-lived self-hosted server. | Rejected. |
| Emit only success counts | Minimal implementation. | Cannot distinguish a healthy no-op run from missing sources, FFmpeg failures, publication failures, or serving errors. | Rejected. |
| Use fixed outcome/asset/error-kind vocabularies, an unlabeled cache-size gauge, and tracing/SSE/scheduled-run records for object-level investigation | Supports rate, error, duration, cache-growth, and serving dashboards while keeping label cardinality bounded and private. | Per-library diagnosis remains in the existing task history, structured logs, and admin progress events rather than Prometheus labels. | Selected. |

The worker records a file exactly once for each terminal attempt: `generated`, `already_running`, `missing_source`, `pipeline_error`, or `database_error`. A successful publication contributes its FFmpeg duration and sprite count; an unsuccessful publication does not claim generated output. Cache storage is measured from the reconciled filesystem tree, not a per-library database estimate, so it includes the VTT and actual WebP bytes. Index and sprite reads record only the bounded `asset` and `outcome` labels.

Prometheus recommends a single meaning and base unit per metric and cautions that every unique label combination is a time series, so IDs and other unbounded values must not become labels. The Rust `metrics` facade maps counters, gauges, and histograms directly to this contract. Sources rechecked on 2026-07-18: [Prometheus metric and label naming](https://prometheus.io/docs/practices/naming/), [Prometheus histograms](https://prometheus.io/docs/practices/histograms/), and [Rust `metrics` metric types](https://docs.rs/metrics/latest/metrics/).

---

## Integration with Existing Systems

### Media Scanning (MEDIA_SCANNING.md)

Storyboard generation runs after media scanning and segment analysis complete. No interaction with the scanning pipeline — it only reads `media_files` to find new/changed files.

### Streaming (STREAMING.md)

After playback chooses a healthy media-file version, the client requests that
version's storyboard metadata through the authenticated storyboard endpoint,
then loads the protected VTT and sprite assets for seek previews. Keeping the
preview request separate from playback start avoids embedding media URLs or
credentials in the playback response.

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

Per-file generation failures are logged while the worker continues with other
files. A scheduled run returns a failure after cleanup when any targeted
library or file failed, so `scheduled_task_runs` can retry and surface the
real outcome. Manual triggers return their generated/skipped/error summary.

---

## Implementation Notes

### Post-Phase 10 Hardening Task 1 — Media Schema Contract (Complete)

`media_items` intentionally uses hard deletion. The profile, playback,
metadata-refresh, subtitle, and ambient-channel queries now rely on the
canonical parent-table schema and use library soft-delete state where needed.
The disposable migration verifier prepares representative media queries after
applying migrations so a stale column or CTI field reference fails before
release.

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
- **Profile-safe cache headers** — Metadata, WebVTT indexes, and sprite sheets all use `Cache-Control: private, no-store`. They are protected media representations, and a shared browser can switch from an adult profile to a Kids profile without changing its session cookie. Reusing a public or private cached preview across that switch could disclose restricted imagery.
- **Bearer-authenticated preview loading** — The web client retrieves the VTT and individual sprite sheets through the shared API client, which supplies the active bearer token and selected server origin. It parses sprite filenames from the protected VTT, fetches the corresponding WebP through the protected route, and renders only a bounded in-memory cache of `blob:` object URLs. Tokens never appear in VTT URLs, sprite URLs, CSS, or browser history. Each request is abortable and object URLs are revoked when evicted, replaced, or destroyed.
- **`index.vtt` static route coexists with `{sprite}` capture** — Axum's matchit router prioritizes static path segments over dynamic captures at the same depth, so `GET /storyboard/index.vtt` routes to the index handler and `GET /storyboard/sprite_001.webp` routes to the sprite handler without conflict. No explicit disambiguation needed.
- **Authorization splits retrieval from mutation** — Retrieval endpoints (`GET storyboard`, `GET index.vtt`, `GET sprite`) require `AuthenticatedUser` and enforce the active profile's media scope before serving previews. Generation and deletion endpoints (`POST generate-storyboards`, `DELETE storyboard`) require `Require<CanManageLibraries>` — storyboard generation is CPU-intensive and cache eviction is an administrative action. This matches the segments domain convention where `analyze-segments` is admin-only but segment retrieval is open to all authenticated users.
- **Configured cache root + reconciliation** — Handlers and worker entry points use `BootstrapConfig.cache_dir`, honoring the CLI/TOML/environment cache-root setting rather than deriving a sibling of `data_dir`. Storyboards resolve through the row's optional `artifact_id`, using `{cache_dir}/storyboards/{media_file_id}/{artifact_id}/` for current artifacts and the legacy per-file directory for pre-versioning rows. After generation, a lock-aware reconciler removes unreferenced artifact directories, obsolete legacy files, and media-file directories with no database row; it skips a directory if another generation holds that file's transaction-scoped lock. Service signatures use `&Path` (not `&PathBuf`) per clippy `ptr_arg` convention.
- **`GenerateStoryboardsResponse` is scope-agnostic** — Single response type `{ queued: bool, message: String }` serves both library and item trigger endpoints. The route context (`/libraries/{id}/` vs `/items/{id}/`) tells the client which scope was triggered. Follows the segments domain's `AnalyzeSegmentsResponse` minimal shape; avoids type duplication for two endpoints with identical response semantics.
- **`InvalidSpriteFilename` reserved for path traversal protection** — Task 4 service implementation will validate sprite filenames against the expected `sprite_NNN.webp` pattern before constructing disk paths, rejecting names containing `..`, `/`, `\`, or non-matching patterns. Mapped to `VALID_001` (422) — matches the playback domain's segment filename validation approach (`validate_segment_filename` rejects `..`, `/`, `\`, names >64 chars, non-`seg_` prefixed).

**Not yet implemented (deferred to Tasks 4 and 6):**

- ~~All six service functions are `todo!()` stubs~~ — Task 4 implements `get_storyboard`, `get_storyboard_index`, `get_storyboard_sprite`, and `delete_storyboard` (DB queries + disk reads); Task 6 (`workers/storyboard_generator.rs`) implements the generation triggers (`trigger_library_generation`, `trigger_item_generation`) by enqueuing work on the scheduler.
- ~~FFmpeg two-phase pipeline (frame extraction → sprite assembly)~~ — Task 4 (`services/storyboards.rs`) implements the generation library using a refined single-command-per-sheet filtergraph (see Task 4 notes below); Task 6 wires it into a scheduled task worker.
- ~~Adaptive interval selection~~ — `adaptive_interval()` function per the Generation Pipeline spec; landed in `services/storyboards.rs` (Task 4).
- Playback-start embedding — intentionally not used. The player requests
  storyboard metadata after selecting its healthy media-file version, which
  keeps protected preview assets independent from playback-session startup.
- `storyboard_generation` scheduled task already seeded (migration `20260530070000_seed_default_data.sql`, daily 04:00) — Task 6 registers the executor on the scheduler in `main.rs`.

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
- **Playback file selection takes precedence** — When a player has selected an explicit healthy `media_file_id`, every storyboard endpoint resolves that exact file. When absent, the primary-file fallback remains `is_healthy=true ORDER BY file_size DESC LIMIT 1`. Multi-version items therefore keep previews aligned with the rendition or cut actually being watched.
- **Delete is idempotent + best-effort disk cleanup** — DB deletion via `DELETE ... RETURNING` is the source of truth; if no row exists, returns `StoryboardNotFound` *after* still attempting on-disk cleanup (handles crashed-generation drift). On-disk `remove_dir_all` failures (except NotFound) are logged at WARN but do not invalidate the committed DB deletion — derived data can always regenerate.

### Phase 10 Task 6 — Background Worker (Complete)

The background worker that orchestrates per-library and per-item storyboard
generation is now implemented. The two remaining `todo!()` trigger stubs in
the domain service are wired to the worker, the scheduler runs the task daily
at 04:00, and FFmpeg invocations are sandboxed on Linux.

**Files built:**

| File | Purpose |
|---|---|
| `server/src/workers/storyboard_generator.rs` | Background worker: `run_storyboard_generation` (scheduler entry — iterates all non-deleted, scan-enabled libraries), `generate_for_library_one` (synchronous per-library API entry), `generate_for_item_one` (synchronous per-item API entry — forces regeneration), `generate_for_library` (per-library pipeline), `resolve_generation_config` (merges server-wide config with per-library overrides), `fetch_files_needing_storyboards` (incremental candidate query), `persist_storyboard_row` (upsert with file_hash change detection) |
| `server/src/workers/mod.rs` | Added `pub mod storyboard_generator;` |
| `server/src/services/storyboards.rs` | `invoke_ffmpeg_for_sheet` now applies the Linux sandbox via `pre_exec` (landlock + seccomp) — same pattern as `services::transcoding::spawn_ffmpeg`. The sandbox config is built from the source path (read-only) and output dir (read-write). Non-Linux platforms are no-ops. |
| `server/src/state.rs` | `TranscodingConfig` expanded with 8 storyboard fields per the Configuration table: `storyboards_enabled`, `storyboard_interval_mode`, `storyboard_fixed_interval_seconds`, `storyboard_width`, `storyboard_quality`, `storyboard_keyframe_only`, `storyboard_sprite_columns`, `storyboard_sprite_rows` |
| `server/src/domains/storyboards/service.rs` | Replaced `trigger_library_generation` and `trigger_item_generation` `todo!()` stubs with synchronous implementations that call the worker (matching the segment domain's `trigger_library_analysis` pattern). Signatures changed from `&PgPool` to `&AppState` to give the worker access to runtime config and cache_dir. |
| `server/src/domains/storyboards/handlers.rs` | Updated `generate_library_storyboards` and `generate_item_storyboards` to pass `&state` instead of `&state.pool` |
| `server/src/main.rs` | Registered `storyboard_generation` executor on scheduler (5th executor — `library_scan`, `metadata_refresh`, `subtitle_auto_fetch`, `segment_analysis`, `storyboard_generation`) |
| `server/src/services/scheduler.rs` | Added "Storyboard Generation" to `seed_default_tasks` (daily 04:00, enabled) |
| `server/migrations/20260621040000_seed_storyboard_generation_task.sql` | Seeds `storyboard_generation` scheduled task for existing deployments (the original Phase 2 seed already creates this row for fresh installs; this migration is idempotent insurance for deployments that skipped the seed) |

**Decisions reconciled with this design doc:**

- **Synchronous per-library API + scheduled iteration of all libraries** — Mirrors the segment detector pattern (Task 5) and the library scanner pattern (Phase 5 Task 5) exactly. The `POST /api/v1/libraries/{id}/generate-storyboards` endpoint runs `generate_for_library_one()` synchronously and returns a summary. The `storyboard_generation` scheduled task iterates all non-deleted, scan-enabled libraries via `run_storyboard_generation()`. The design doc's "enqueue on the scheduler" language was prescriptive but the established precedent (synchronous API + scheduled iteration) is more pragmatic, avoids background-queue infrastructure that doesn't exist, and keeps the `GenerateStoryboardsResponse.queued` field honest (always `false` in this implementation, matching `AnalyzeSegmentsResponse.queued`). HTTP timeout risk for large libraries is accepted per the library_scan precedent; the worker logs per-file progress so partial completion is observable.
- **Per-library enablement is respected (Jellyfin bug #14558 lesson)** — The worker checks three gates before processing a library: (1) global `TranscodingConfig.storyboards_enabled` must be `true`; (2) the library must be non-deleted with `scan_enabled = true`; (3) per-library `libraries.metadata->>'storyboards_enabled'` must NOT be `"false"` (defaults to enabled when the key is absent). This avoids the Jellyfin user complaint where the scheduled task ran despite per-library disable.
- **Per-library config overrides via `libraries.metadata` JSONB** — The worker reads `metadata->>'storyboard_width'`, `metadata->>'storyboard_fixed_interval_seconds'`, and `metadata->>'storyboards_enabled'` and overrides the server-wide config for that library. This allows different resolutions or intervals for different library types (e.g., 640px for a 4K movie library, 160px for a TV show library) per the design's "Per-Library Storyboard Config" section. Missing keys fall back to server-wide config (graceful degradation — no per-library override means use defaults).
- **Adaptive interval resolved at file time, not config time** — When `interval_mode = "adaptive"`, the worker calls `services::storyboards::adaptive_interval(runtime_seconds)` per-file using the file's actual runtime from `media_files.runtime_seconds`. This means a TV library with 22-min episodes and 100-min movies gets 5s intervals for episodes and 10s intervals for movies, even though they share a library. When `interval_mode = "fixed"`, the worker uses `storyboard_fixed_interval_seconds` from config. Task config `interval_mode` overrides server-wide config (enables one-off runs with a different mode).
- **Validated normalized generation settings** — The system config endpoint validates interval mode, 2-120 second fixed interval, allowed widths, 50-100 quality, booleans, and the 1-20 by 1-40 grid bounds before a transcoding group is stored. The web settings page uses the same bounds and offers only supported widths. `GenerationConfig::validate` applies the same limits before FFmpeg is invoked.
- **Incremental candidate query with null-safe source and configuration freshness** — `fetch_files_needing_storyboards` loads the storyboard row with each healthy file, resolves the actual per-file interval, and compares nullable source hashes plus a normalized generation fingerprint. This avoids PostgreSQL's ordinary `NULL = NULL` unknown result and regenerates when the output-affecting effective configuration changes. A matching null source hash is cacheable; a missing fingerprint makes legacy rows regenerate once.
- **DB row upsert with `ON CONFLICT (media_file_id) DO UPDATE`** — `persist_storyboard_row` upserts the storyboards row on each successful generation. The `media_file_id` UNIQUE constraint is the conflict target. On update, all fields are refreshed (interval, width, height, sprite_count, total_thumbnails, total_size_bytes, keyframe_only, quality, generated_at, generation_duration_ms, metadata, file_hash, artifact_id, config_fingerprint). This handles both first-time generation and forced regeneration cleanly.
- **Grid shape stored in `metadata.columns` and `metadata.rows`** — The worker writes `metadata = jsonb_build_object('columns', $N, 'rows', $N)` when creating the row so `domains::storyboards::service::read_grid_shape()` can recover the per-generation grid. This closes the loop with the Task 4 design decision.
- **Atomic forced regeneration for every healthy item version** — `trigger_item_generation` calls `generate_for_item_one`, which processes every healthy media-file version in deterministic order. Each version uses `pg_try_advisory_xact_lock`, generates an unreferenced UUIDv7 artifact directory, and switches `storyboards.artifact_id` in the same transaction only after successful generation. A manual request returns `SYS_002` only when all versions are already locked; scheduled work counts locked versions as skipped. Existing previews remain available when generation or persistence fails. Deletion obtains the same locks and removes every version's row and cache directory together.
- **Sandbox applied via `pre_exec` on each FFmpeg invocation** — `services::storyboards::invoke_ffmpeg_for_sheet` now uses `command.pre_exec(move || apply_sandbox(&SandboxConfig { media_path, transcode_dir: output_dir }))` on Linux, matching the `services::transcoding::spawn_ffmpeg` pattern. The sandbox restricts FFmpeg to read-only access on `/usr`, `/lib`, `/etc`, `/dev/dri`, and the source media path; read-write access on the per-file storyboard output directory and `/tmp`. Seccomp filters to a 62-syscall allow-list with `KillProcess` on violation. Sandbox failures are non-fatal (logged at WARN, FFmpeg continues without sandbox) per SECURITY.md graceful degradation model. Non-Linux platforms are no-ops.
- **Per-file error isolation with truthful scheduler status** — `generate_for_library` catches per-file errors and continues to the next file (matching the segment detector's per-file error pattern). Failed files are counted and logged at WARN so one corrupt file does not prevent useful work. The scheduled wrapper aggregates those errors after all libraries and returns an error to the scheduler, preserving retries and accurate run history.
- **Movie/episode-only filtering** — The candidate query filters `mi.type IN ('movie', 'episode')` (same as the segment detector) because series and seasons are container types without direct `media_files`; storyboards correspond to actual video files.
- **Healthy media_files required** — `mf.is_healthy = true` guard ensures we only generate storyboards for files that exist on disk and are playable. Without this, the worker would spawn FFmpeg on missing files and every invocation would fail.
- **Configured task timeout and cancellation are enforced** — The scheduler derives its timeout from each task's `timeout_seconds` value (the Storyboards default is 14400 seconds) and records expiry as a timed-out run, so large libraries use their declared four-hour budget rather than a hidden one-hour wrapper. Tokio cancels the worker future on timeout; storyboard FFmpeg commands use `kill_on_drop(true)`, so an in-flight process is killed when that command future is dropped. Sources rechecked on 2026-07-18: [Tokio `timeout`](https://docs.rs/tokio/latest/tokio/time/fn.timeout.html) and [Tokio process cancellation](https://docs.rs/tokio/latest/tokio/process/struct.Command.html#method.kill_on_drop).
- **No new workspace dependencies** — All functionality uses existing `sqlx`, `serde_json`, `tokio::process::Command`, and the already-built `services::storyboards` and `services::sandbox` modules.

**Not yet implemented (deferred to later tasks/phases):**

- ~~Web client `SeekPreview.svelte` — Task 8 consumes the `/storyboard/index.vtt` endpoint via hls.js or a custom seek-bar component~~ — **Complete (Task 8)**
- ~~Prometheus metrics from the Metrics table (`storyboard_files_processed_total`, `storyboard_generation_duration_seconds`, etc.)~~ — **Complete (Storyboard metrics follow-up):** the worker emits one bounded terminal outcome per attempted file, successful publications emit duration and sprite counts, reconciliation measures the cache tree, and authenticated index/sprite handlers record bounded serving outcomes.
- `outro` segment type via silence-gap detection — unrelated to Task 6; deferred to a follow-up of Task 5

### Phase 10 Task 8 — Seek Preview Component (Complete)

The web client seek-preview tooltip that displays storyboard thumbnails when the user hovers or scrubs the seek bar. The component lazily fetches the WebVTT index, resolves sprite references, and renders the correct sprite-sheet region via CSS background positioning.

**Files built:**

| File | Purpose |
|---|---|
| `clients/web/src/lib/api/storyboards.js` | Full storyboard API client: `getStoryboard`, `storyboardIndexUrl`, `storyboardSpriteUrl`, `generateLibraryStoryboards`, `generateItemStoryboards`, `deleteStoryboard` |
| `clients/web/src/lib/utils/storyboards.js` | Pure-function WebVTT utilities: `parseTimecodeToMs`, `parseStoryboardVtt` (cue extraction with sprite URL + xywh region), `findCueForTime` (binary search) |
| `clients/web/src/lib/components/SeekPreview.svelte` | Thumbnail tooltip — lazy VTT fetch, CSS background-image sprite rendering, edge-clamped positioning, time label |
| `clients/web/src/lib/api/index.js` | Added `storyboards.js` to barrel export |
| `clients/web/src/lib/components/Player.svelte` | Wired SeekPreview: storyboard fetch in onMount, hover/touch tracking, 20px seek-bar hit area |

**Decisions reconciled with this design doc:**

- **Custom seek-bar component over hls.js native thumbnail tracks** — The design doc's "hls.js Integration" section describes `hls.addTrack({ kind: 'thumbnails', url })` for HLS transcoded streams. However, the player uses a custom seek bar across all playback modes (direct play, remux, transcode), so a custom seek-preview component works uniformly for all stream types. hls.js thumbnail tracks only apply when hls.js manages the stream (transcode/remux), not for direct play. The design doc's own guidance ("For native HLS (Safari, Chrome 142+), the client uses a custom seek bar component") confirms this as the cross-platform approach. The hls.js `addTrack` integration path is documented for future consideration if the project adds native hls.js-managed thumbnail rendering.
- **CSS background-image + background-position for sprite rendering** — The industry-standard approach (confirmed by JW Player, Video.js, FluidPlayer, Radiant Media Player). The `#xywh=X,Y,W,H` fragment from each WebVTT cue maps to negative `background-position` offsets; `background-size` scales the full sprite sheet to the display thumbnail dimensions. No canvas or clip-path needed.
- **WebVTT cue payload parsing** — The parser extracts `spriteUrl`, `x`, `y`, `w`, `h` from each cue's `sprite_NNN.webp#xywh=X,Y,W,H` payload. Relative sprite references are resolved to absolute URLs via `new URL(ref, absoluteIndexUrl)` so CSS `background-image: url(...)` works correctly across dev (Vite proxy) and production (reverse proxy / same-origin).
- **Binary search cue lookup** — `findCueForTime(cues, timeMs)` uses O(log n) binary search. For a 2-hour movie at 10s intervals (~720 cues), this is ~10 comparisons. Cues before the first timestamp clamp to the first cue; cues after the last timestamp clamp to the last cue — no dead zones on the seek bar.
- **Lazy VTT fetch with race protection** — The WebVTT index is fetched on first storyboard availability via `$effect`. A `fetchId` counter discards stale responses if the user switches items mid-fetch.
- **CSS clamp() for edge-aware positioning** — `left: clamp(half-width, ratio × 100%, 100% − half-width)` with `transform: translateX(-50%)` prevents tooltip overflow without JavaScript measurement.
- **Preview during hover AND active seek** — Matches YouTube/Netflix behavior where the preview follows the thumb during scrubbing. During `isSeeking`, the preview tracks `seekValue` (range input); during hover, it tracks the mouse position.
- **Graceful degradation** — 404 (MEDIA_007) from `getStoryboard` when no storyboard exists is caught silently; the `{#if storyboard}` guard prevents SeekPreview from rendering. No visual regression for items without storyboards.

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
