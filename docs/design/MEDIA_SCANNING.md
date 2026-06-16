# Media Scanning Strategy

## Overview

A hybrid scanning pipeline that combines real-time filesystem watching with periodic scheduled scans to detect, identify, probe, and catalog media files. Uses mtime-based diffing to skip unchanged files, making even full scans fast on large libraries.

Library folder structure, file naming conventions, and sub-folder design are documented in [LIBRARY_ORGANIZATION.md](LIBRARY_ORGANIZATION.md). This document covers the scanning pipeline mechanics; LIBRARY_ORGANIZATION.md covers what the scanner expects to find on disk.

## Crate Selection

| Crate | Version | Maintainer | Role |
|---|---|---|---|
| `ignore` | 0.4.25 | BurntSushi (ripgrep) | Parallel directory walking with filtering (full scans) |
| `walkdir` | 2.5.0 | BurntSushi | Single-directory walks (targeted re-scans) |
| `notify` | 8.2.0 | notify-rs | Cross-platform filesystem watching (real-time detection) |
| `notify-debouncer-full` | 0.7.0 | notify-rs | Debounced FS events with rename stitching and dedup |
| `blake3` | 1.x | oconnor663 | Partial file hashing (first+last 1MB); fastest non-crypto hash |
| `regex` | 1.x | rust-lang | SXXEXX filename parsing, provider ID tag extraction |
| `quick-xml` | 0.40.x | tafia | NFO XML parsing (streaming StAX; 50x faster than xml-rs) |
| `croner` | 3.x | hexagon | Cron expression evaluation for scheduled scans |
| FFmpeg `ffprobe` | existing | FFmpeg project | Media file probing (codecs, resolution, duration, streams) |

### Why These Crates

| Crate | Strength | Limitation | Our Use |
|---|---|---|---|
| **ignore** | Parallel walking; `.gitignore` filtering; ripgrep-grade performance | Slightly more complex API than walkdir | Full library scans (Phase 1 discovery) |
| **walkdir** | Simple; well-tested; low overhead | Single-threaded | Targeted single-directory re-scans |
| **notify** | Cross-platform (inotify/FSEvents/ReadDirectoryChangesW); PollWatcher fallback | NFS/SMB may not emit events; Linux inotify watch limits | Real-time file detection |
| **notify-debouncer-full** | Rename stitching; event dedup; file ID tracking; only emits settled events | Adds latency (debounce timeout) | Processing FS watch events |
| **quick-xml** | Fastest Rust XML parser (50x xml-rs); streaming StAX; near-zero alloc | Low-level API; no DOM tree | NFO file parsing (Layer 2 identification) |

### Rejected Alternatives

| Approach | Why Not |
|---|---|
| **watch-only** | Unreliable on NFS/SMB mounts, Docker volumes, and under heavy I/O; inotify has watch limits |
| **scan-only (scheduled)** | Delayed detection; user adds a movie and waits for next scan; wastes I/O on unchanged files |
| **Content hash (full SHA-256)** | Too slow for GB-sized video files; a 50GB remux takes seconds to hash |
| **mediainfo** | Redundant with ffprobe which is already in our stack |
| **video fingerprinting** | No public database; very slow; privacy concerns; overkill for personal Duskcue |

---

## Architecture: Hybrid Scanning Pipeline

```
┌─────────────────────────────────────────────────────────────────┐
│                    Media Scanning Pipeline                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Trigger Sources:                                                │
│    1. FS Watch (notify) → debounced → enqueue scan job          │
│    2. Scheduled full scan (cron via scheduled_tasks)             │
│    3. Manual scan (admin API → POST /api/v1/libraries/{id}/scan)│
│                                                                  │
│  Phase 1: Discovery (ignore::WalkParallel)                      │
│    Walk library root_path in parallel                            │
│    Filter by media extensions (.mkv, .mp4, .avi, .ts, etc.)     │
│    Output: Set<DiscoveredFile { path, size, mtime }>            │
│                                                                  │
│  Phase 2: Diff (compare to database)                            │
│    Load known files from media_files for this library            │
│    New files: in discovered but not in DB → Phase 3             │
│    Modified files: path matches but size/mtime changed → Phase 3│
│    Unchanged files: path + size + mtime match → skip            │
│    Deleted files: in DB but not on disk → remove                │
│                                                                  │
│  Phase 3: Probe (ffprobe per file)                              │
│    Run ffprobe to extract codec, resolution, duration, streams   │
│    Compute partial hash (first 1MB + last 1MB)                  │
│    Store in media_files                                          │
│                                                                  │
│  Phase 4: Identify (filename parsing + TMDB/TVDB lookup)        │
│    Parse directory/filename → title, year, season, episode      │
│    Search provider API (TMDB for movies, TVDB for TV)           │
│    Create media_item + type-specific child (movie/series/...)   │
│    Link media_file to media_item                                 │
│                                                                  │
│  Phase 5: Enrich (metadata + artwork)                           │
│    Fetch full metadata from provider (cast, crew, genres, etc.) │
│    Download artwork (posters, backdrops, thumbnails)            │
│    Store in genres, tags, people, credits, artwork tables       │
│    Update search_vector for full-text search                     │
│                                                                  │
│  Phase 6: Cleanup                                               │
│    Mark deleted files (in DB but not on disk)                    │
│    Orphan detection: media_items with no media_files            │
│    Report scan results (new/modified/deleted/unmatched counts)  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Trigger Sources

### 1. Real-Time Filesystem Watch

Uses `notify` + `notify-debouncer-full` to watch library root paths for changes.

```rust
use notify_debouncer_full::{new_debouncer, notify::*, DebounceEventResult};
use std::time::Duration;

let (tx, rx) = std::sync::mpsc::channel();

let mut debouncer = new_debouncer(
    Duration::from_secs(3),
    None,
    move |result: DebounceEventResult| {
        match result {
            Ok(events) => {
                for event in events {
                    let _ = tx.send(event);
                }
            }
            Err(errors) => {
                for error in errors {
                    tracing::warn!(?error, "filesystem watch error");
                }
            }
        }
    },
)?;

debouncer.watch(library.root_path, RecursiveMode::Recursive)?;
```

**Debounce timeout:** 3 seconds. Media files are large — writes take time. A 3-second window ensures we don't process partially-written files.

**Platform backends:**
- Linux: `inotify`
- macOS: `FSEvents`
- Windows: `ReadDirectoryChangesW`
- Fallback: `PollWatcher` (for NFS/SMB where native events are unavailable)

**Limitations and mitigations:**

| Problem | Mitigation |
|---|---|
| NFS/SMB don't emit events | Automatic fallback to PollWatcher for network mounts |
| Linux inotify watch limits | Log warning with `sysctl` instructions; fallback to scheduled scan |
| Docker volume inotify doesn't propagate | Scheduled periodic scan catches missed files |
| Heavy I/O causes missed events | Periodic full scan as safety net |
| Partially-written files | 3-second debounce; also check file size stability before probing |

**Filesystem watcher lifecycle:**
- Watchers are started when libraries are created or server starts
- Watchers are stopped when libraries are deleted or server shuts down
- If watcher fails (watch limit exceeded, permission denied), log error and disable real-time watching for that library — scheduled scans still work

### 2. Scheduled Full Scan

Uses the existing `scheduled_tasks` infrastructure. The `library_scan` task type (already defined in DATABASE.md) handles periodic full scans.

**Implementation:** The scheduler (`services/scheduler.rs`) polls `scheduled_tasks` every 30 seconds. The `library_scan` executor fetches all non-deleted libraries and runs `scan_library()` for each sequentially. Default schedule: `0 3 * * *` (daily at 03:00) via cron expression on the `scheduled_tasks` row. Task config supports `{"mode": "full"|"quick"}` (default: `"full"`).

The scheduled scan runs Phase 1-6 for all enabled libraries. Each library is scanned sequentially to avoid I/O saturation.

### 3. Manual Scan

Admin API endpoint triggers an immediate scan:

```
POST /api/v1/libraries/{id}/scan
POST /api/v1/libraries/{id}/scan?full=true
```

Without `?full=true`, only runs a quick diff scan (Phase 1-2) to detect new/deleted files. With `full=true`, re-probes all files including unchanged ones (Phase 1-6).

---

## Phase 1: Discovery

### Parallel Directory Walk

Uses `ignore::WalkParallel` for maximum throughput on large libraries:

```rust
use ignore::WalkBuilder;

let walker = WalkBuilder::new(&library.root_path)
    .hidden(false)
    .git_ignore(false)
    .git_exclude(false)
    .overrides(build_media_extension_overrides()?)
    .build_parallel();

let discovered = DashSet::new();

walker.visit(&mut |entry| {
    let entry = match entry {
        Ok(e) => e,
        Err(_) => return WalkState::Continue,
    };

    if !entry.file_type().map_or(false, |ft| ft.is_file()) {
        return WalkState::Continue;
    }

    let metadata = entry.metadata().unwrap();
    discovered.insert(DiscoveredFile {
        path: entry.path().to_path_buf(),
        size: metadata.len(),
        mtime: metadata.modified()
            .ok()
            .and_then(|t| t.into().ok()),
    });

    WalkState::Continue
});
```

### Supported Media Extensions

| Category | Extensions |
|---|---|
| Video | `.mkv`, `.mp4`, `.avi`, `.ts`, `.m2ts`, `.wmv`, `.flv`, `.webm`, `.mov`, `.mpg`, `.mpeg`, `.m4v`, `.3gp`, `.ogv` |
| Subtitle (external) | `.srt`, `.ass`, `.ssa`, `.vtt`, `.sub`, `.idx`, `.pgs` (`.sup`) |
| Disc | `.iso`, `.img`, `.m4v` |

### DiscoveredFile

```rust
#[derive(Debug, Clone)]
struct DiscoveredFile {
    path: PathBuf,
    size: u64,
    mtime: Option<SystemTime>,
}
```

---

## Phase 2: Diff

Compare discovered files against the `media_files` table to determine what's new, modified, or deleted.

```sql
SELECT file_path, file_size, file_modified_at, file_hash
FROM media_files
WHERE media_item_id IN (
    SELECT id FROM media_items WHERE library_id = $1
);
```

### Change Detection Logic

| Condition | Action |
|---|---|
| Path in discovered, not in DB | **New file** → Phase 3 (probe) |
| Path in both, size or mtime differs | **Modified file** → Phase 3 (probe) |
| Path in both, size + mtime match | **Unchanged** → skip |
| Path in DB, not on disk | **Deleted file** → mark for cleanup |

### mtime Comparison

mtime is compared with a 2-second tolerance to handle filesystem timestamp precision differences (FAT32 has 2-second resolution, some SMB mounts round differently).

### Partial Hash (for ambiguous cases)

When size matches but mtime is suspiciously close, compute a partial hash:

```rust
fn partial_hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let file_size = file.metadata()?.len();

    let mut hasher = Blake3::new();

    // Hash first 1MB
    let mut buf = vec![0u8; 1024 * 1024];
    let n = file.read(&mut buf)?;
    hasher.update(&buf[..n]);

    // Hash last 1MB (if file > 2MB)
    if file_size > 2 * 1024 * 1024 {
        file.seek(SeekFrom::End(-(1024 * 1024)))?;
        let n = file.read(&mut buf)?;
        hasher.update(&buf[..n]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}
```

**Blake3** is chosen for partial hashing — it's the fastest non-cryptographic hash available in Rust (2x faster than xxHash, 10x faster than SHA-256). The partial hash is stored in `media_files.file_hash`.

---

## Phase 3: Probe

### ffprobe

Each new or modified file is probed with ffprobe:

```bash
ffprobe -v quiet -print_format json -show_format -show_streams -show_chapters <file>
```

Output is parsed and mapped to `media_files` columns:

| ffprobe field | media_files column |
|---|---|
| `format.format_name` | `container_format` |
| `streams[video].codec_name` | `video_codec` |
| `streams[video].width × height` | `video_resolution` |
| `streams[video].bit_rate` | `video_bitrate` |
| `streams[video].color_transfer` | `video_dynamic_range` (derived: `smpte2084`→`hdr10`, `arib-std-b67`→`hlg`, etc.) |
| `streams[video].r_frame_rate` | `video_frame_rate` |
| `streams[audio].codec_name` | `audio_codec` |
| `streams[audio].channels` | `audio_channels` |
| `streams[audio].tags.language` | `audio_language` |
| `streams[audio].bit_rate` | `audio_bitrate` |
| `format.duration` | `runtime_seconds` |
| Additional streams | `additional_streams` (JSONB) |
| DV side data | `additional_streams.dolby_vision` (profile, level, compatibility_mode, enhancement_layer) |
| HDR10+ side data | `additional_streams.hdr10_plus` |
| `chapters` | Stored in `media_fingerprints.chapters_json` for segment detection |

The full video format catalog — codec profiles, HDR detection details, bit depth handling, and DV profile parsing — is documented in [VIDEO_FORMATS.md](VIDEO_FORMATS.md). The full audio format catalog — codec details, channel layouts, spatial audio detection (Dolby Atmos, DTS:X), sample rate, bit depth, and multi-track audio — is documented in [AUDIO_FORMATS.md](AUDIO_FORMATS.md).

Chapter data (`-show_chapters`) is extracted during probing and stored alongside the fingerprint in the `media_fingerprints` table (see SEGMENT_DETECTION.md). This avoids re-probing when the segment analysis task runs later — chapter titles are already available for regex matching.

### Probe Queue

Files are probed concurrently with a configurable limit (default: 2 concurrent ffprobe processes) to avoid I/O saturation, especially on spinning disks and NAS devices.

```rust
use tokio::sync::Semaphore;

let probe_semaphore = Arc::new(Semaphore::new(config.max_concurrent_probes));

for file in files_to_probe {
    let permit = probe_semaphore.clone().acquire_owned().await.unwrap();
    tokio::spawn(async move {
        probe_file(&file).await;
        drop(permit);
    });
}
```

`max_concurrent_probes` is stored in `server_config.transcoding` JSONB (related to transcode concurrency limits).

### Subtitle Discovery

External subtitle files are discovered during Phase 1 and matched to their parent media item by:

1. **Same directory, same base name:** `Movie.2024.mkv` → `Movie.2024.en.srt`
2. **Subtitle directory convention:** `Movie.2024/Subs/` or `subtitles/`
3. **Language suffix parsing:** `.en.srt`, `.eng.srt`, `.en.ssa`

Embedded subtitles (inside the container) are extracted from the ffprobe output during Phase 3.

**Implementation (Phase 9 Tasks 2–3):** Subtitle discovery is implemented in `server/src/services/subtitle_discovery.rs` and called after Phase 4 (Identify) in the scan pipeline. The service:

- **Loads video files** from `media_files` joined with `media_items` for the library, building a `HashMap<PathBuf, Vec<usize>>` directory map for O(1) video lookup
- **External subtitles** — Iterates all discovered files with subtitle extensions (`.srt`, `.ass`, `.ssa`, `.vtt`, `.sub`, `.sup`); `.idx` companion files are excluded. Each subtitle is matched to a video file by directory + base-name prefix. The `Subs/` and `subtitles/` directory convention is supported by searching the grandparent directory when the parent matches a known subtitle directory name. Language codes and flags (`forced`, `hi`, `sdh`, `cc`, `hearing_impaired`, `default`) are parsed from trailing filename segments after the video base name
- **Embedded subtitles** — Extracted from `media_files.additional_streams` JSONB `subtitles` array (populated during Phase 3 ffprobe). Each embedded subtitle is stored with synthetic path `{media_file_path}::embedded::{stream_index}` for uniqueness
- **Idempotent inserts** — All inserts use `INSERT ... ON CONFLICT (media_item_id, file_path) DO NOTHING`, so re-scans add new subtitles without duplicating existing ones. Stale subtitle rows (deleted sidecars) are not removed during scan; cleanup is deferred to a future task
- **`ScanResult.subtitles_discovered`** — The count of newly inserted subtitle rows is aggregated across scan paths and reported in the scan result JSON

The full subtitle domain — including OCR conversion, synchronization, external provider fetching, and delivery — is documented in [SUBTITLES.md](SUBTITLES.md).

---

## Phase 4: Identify

### Filename Parsing

Parse the directory structure and filename to extract identifying information:

```rust
#[derive(Debug)]
struct ParsedMediaName {
    title: String,
    year: Option<u16>,
    season: Option<u32>,
    episode: Option<u32>,
    episode_title: Option<String>,
    resolution: Option<String>,
    source: Option<String>,
    codec: Option<String>,
    group: Option<String>,
}
```

### Naming Convention Support

The parser handles common naming patterns:

**Movies:**
```
/Movies/The Matrix (1999)/The.Matrix.1999.1080p.BluRay.x264-SPARKS.mkv
/Movies/The Matrix (1999)/The.Matrix.1999.mkv
/Movies/The.Matrix.1999.1080p.BluRay.x264-SPARKS.mkv
```

**TV Shows:**
```
/TV Shows/Breaking Bad/Season 01/S01E01 - Pilot.mkv
/TV Shows/Breaking Bad/Season 01/Breaking.Bad.S01E01.1080p.BluRay.x264.mkv
/TV Shows/Breaking Bad/s01e01 - Pilot.mkv
/TV Shows/Breaking Bad/Breaking.Bad.1x01.Pilot.mkv
```

**Anime (optional):**
```
/Anime/Attack on Titan/Attack.on.Titan.-.S01E01.mkv
/Anime/Attack on Titan/[GroupName] Attack on Titan - 01 [1080p].mkv
```

### Online Lookup

The identification pipeline is a 5-layer cascade documented in [LIBRARY_ORGANIZATION.md](LIBRARY_ORGANIZATION.md) (Identification Pipeline section). This section covers Layers 3-4 implementation (structured parse + API search).

**Layer order (from LIBRARY_ORGANIZATION.md):**
1. `.media-match` sidecar file → if found, use provider ID directly → DONE
2. NFO file → if found with provider ID → DONE
3. Provider ID tag in folder/filename (`{tmdb-XXX}`) → if found → DONE
4. Structured filename parse + API search (this section) → confidence scoring
5. Unmatched queue → admin fixes → auto-writes `.media-match` file

**Layer 4 — structured parse + API search:**

After parsing, the title + year are searched against metadata providers:

1. **TMDB** (primary) — Search movie/TV endpoint with title + year
2. **TVDB** (secondary for TV) — Fallback if TMDB returns no results

```rust
async fn identify_movie(parsed: &ParsedMediaName) -> Result<MediaMatch> {
    let results = tmdb_client.search_movie(&parsed.title, parsed.year).await?;

    if results.is_empty() {
        return Ok(MediaMatch::Unmatched);
    }

    if results.len() == 1 {
        return Ok(MediaMatch::Confirmed(results[0].clone()));
    }

    let best = results.into_iter()
        .filter(|r| parsed.year.map_or(true, |y| r.year == Some(y)))
        .max_by_key(|r| r.popularity);

    match best {
        Some(matched) => Ok(MediaMatch::AutoMatched(matched)),
        None => Ok(MediaMatch::Unmatched),
    }
}
```

**Confidence scoring** (when multiple results return):

| Signal | Weight |
|---|---|
| Exact title match (case-insensitive) | 40 |
| Year matches | 30 |
| Provider ID matches | 100 (auto-confirmed) |
| Title contains search query | 20 |
| Popular result (high TMDB vote count) | 10 |

If top score >= 70, auto-confirm. Otherwise, queue for Layer 5 (unmatched queue).

### Match States

| State | Meaning | User Action Required |
|---|---|---|
| `unmatched` | No provider result found | User must manually identify |
| `auto_matched` | Single or high-confidence result | User can confirm or correct |
| `confirmed` | User confirmed the match | None |
| `manual` | User manually selected the match | None |

The `match_state` is stored in a new column on `media_items` (see Database Changes below).

### TV Show Grouping

For TV libraries, the scanner must group episodes into series and seasons (see [LIBRARY_ORGANIZATION.md](LIBRARY_ORGANIZATION.md) for expected folder structures):

1. Parse season/episode numbers from filenames
2. Group by series title (from parent directory name or filename)
3. Create one `media_items` row per series (type: `series`)
4. Create one `media_items` row per season (type: `season`) with `season_number`
5. Create one `media_items` row per episode (type: `episode`) with `season_id`, `episode_number`

---

## Phase 5: Enrich

**Implementation status: Stub.** The scanner logs "metadata provider integration deferred to Phase 6" for all items. Phase 6 (Metadata Providers) will add TMDB/TVDB search that upgrades `auto_matched` items to `confirmed` and populates titles, overviews, artwork, cast/crew, and external IDs.

### Metadata Fetching

After identification, full metadata is fetched from the provider:

| Data | Source | Stored In |
|---|---|---|
| Title, overview, tagline | TMDB/TVDB | `media_items` columns |
| Premiere date, content rating | TMDB/TVDB | `media_items` columns |
| Rating (TMDb/TVDB score) | TMDB/TVDB | `media_items.rating` |
| Genres | TMDB/TVDB | `genres` + `media_genres` |
| Cast and crew | TMDB/TVDB | `people` + `media_credits` |
| Provider IDs (TMDB, TVDB, IMDb) | TMDB/TVDB | `media_items.tmdb_id`, etc. |
| Full provider response | TMDB/TVDB | `media_items.metadata` (JSONB) |

### Artwork Downloading

| Type | Source | Size |
|---|---|---|
| Poster | TMDB image API | Original (up to 2000×3000) |
| Backdrop | TMDB image API | Original (up to 3840×2160) |
| Season poster | TMDB image API | Original |
| Thumbnail | Generated from file | Seeking to 10%, screenshot |

Artwork is stored in `/data/metadata/artwork/tmdb/` and tracked in the `artwork` table with `source_type = 'tmdb'`. Resized versions for client delivery are generated on demand and cached in `/cache/images/resized/`.

Full artwork lifecycle, multi-source management, and poster locking documented in [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md). Overlay compositing applied to source artwork documented in [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md).

### Full-Text Search Update

After metadata is stored, the `search_vector` column is updated via the existing trigger (see DATABASE.md — full-text search cross-cutting concern).

---

## Phase 6: Cleanup

### Deleted File Detection

Files in `media_files` that were not found during Phase 1:

```sql
UPDATE media_files SET is_healthy = false
WHERE media_item_id IN (SELECT id FROM media_items WHERE library_id = $1)
AND file_path NOT IN (SELECT unnest($2::text[]));
```

Deleted files are NOT immediately removed — they're marked `is_healthy = false`. The admin can see missing files in the UI and choose to:
- Remove the database entry (file was intentionally deleted)
- Re-scan (file was temporarily unavailable — network drive disconnected)

### Orphan Detection

`media_items` with no associated `media_files`:

```sql
SELECT mi.* FROM media_items mi
LEFT JOIN media_files mf ON mf.media_item_id = mi.id
WHERE mi.library_id = $1
AND mf.id IS NULL;
```

These are reported in the scan results for admin review.

### Scan Results

Every scan produces a result stored in `scheduled_task_runs` config:

```rust
#[derive(Serialize)]
struct ScanResult {
    library_id: Uuid,
    library_name: String,
    scan_type: ScanType, // full, quick, watch_triggered
    started_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
    files_discovered: u64,
    files_new: u64,
    files_modified: u64,
    files_unchanged: u64,
    files_deleted: u64,
    items_unmatched: u64,
    items_auto_matched: u64,
    items_confirmed: u64,
    errors: Vec<ScanError>,
}
```

---

## Filesystem Watcher Integration

### Library Watcher Lifecycle

| Event | Watcher Action |
|---|---|
| Server starts | Start watchers for all libraries with `scan_enabled = true` |
| Library created | Start watcher for new library's `root_path` |
| Library `root_path` updated | Stop old watcher, start new watcher |
| Library `scan_enabled` set to false | Stop watcher |
| Library deleted (soft delete) | Stop watcher |
| Server shuts down | Drop all watchers (graceful stop) |

### Watch Event Processing

When the debouncer emits a settled event:

1. Filter to media file extensions only
2. Determine which library the path belongs to (match against library `root_path` prefixes)
3. Enqueue a targeted scan job for that library (not a full walk — just the affected directory)
4. Debounce further events for the same library for 10 seconds (avoid re-triggering on bulk imports)

### Bulk Import Detection

If the watcher detects more than 10 new files in a single directory within the debounce window:
- Cancel individual processing
- Enqueue a full library scan instead
- Log at INFO level: `"Bulk import detected: {count} new files in {path}, triggering full scan"`

---

## Database Changes

### New Column: `media_items.match_state`

```sql
ALTER TABLE media_items ADD COLUMN match_state TEXT NOT NULL DEFAULT 'confirmed'
    CHECK (match_state IN ('unmatched', 'auto_matched', 'confirmed', 'manual'));
```

For existing data, default is `confirmed` (already identified). New files from scanning start as `unmatched` or `auto_matched`.

### New Column: `media_files.file_modified_at`

```sql
ALTER TABLE media_files ADD COLUMN file_modified_at TIMESTAMPTZ;
```

Stores the filesystem mtime for change detection in Phase 2.

### New Column: `libraries.last_scan_at`

```sql
ALTER TABLE libraries ADD COLUMN last_scan_at TIMESTAMPTZ;
```

Tracks when the last scan completed, for display in the admin UI and for determining scan staleness.

### New Index: `media_files` path lookup

The existing `UNIQUE(media_item_id, file_path)` constraint supports Phase 2 lookups. An additional index on `file_path` alone enables cross-library file deduplication:

```sql
CREATE INDEX idx_media_files_file_path ON media_files (file_path);
```

---

## Scanning Configuration

Stored in `libraries.metadata` JSONB (existing column) and `server_config`:

### Per-Library Scan Config

```json
{
  "scan_watch_enabled": true,
  "scan_realtime_fallback": "poll",
  "scan_poll_interval_seconds": 300,
  "scan_exclude_patterns": ["*.tmp", ".DS_Store", "Thumbs.db", "._*"],
  "scan_season_detection": "directory"
}
```

| Field | Type | Default | Description |
|---|---|---|---|
| `scan_watch_enabled` | bool | `true` | Enable real-time filesystem watching |
| `scan_realtime_fallback` | string | `"poll"` | Fallback when native watching unavailable: `poll` or `disable` |
| `scan_poll_interval_seconds` | u32 | `300` | Poll interval for PollWatcher fallback |
| `scan_exclude_patterns` | string[] | `["*.tmp", ".DS_Store", "Thumbs.db", "._*"]` | Glob patterns to skip |
| `scan_season_detection` | string | `"directory"` | How to detect seasons: `directory` (Season XX folders), `filename` (S01E01 parsing), or `both` |

### Server-Wide Scan Config

Stored in `server_config.metadata` JSONB:

```json
{
  "providers": ["tmdb"],
  "auto_refresh_hours": 6,
  "max_concurrent_probes": 2,
  "artwork_sizes": ["original", "w500", "w300"]
}
```

---

## Integration with Existing Systems

### Scheduled Tasks (DATABASE.md)

The existing `library_scan` scheduled task type triggers full scans. The scheduler (`services/scheduler.rs`) manages task lifecycle:
- `state = 'running'` during scan
- `state = 'completed'` on success with `ScanResult` stats in `scheduled_task_runs.stats` JSONB
- `state = 'failed'` on error with error details in `scheduled_task_runs.error_message`
- `consecutive_failures` incremented on failure, reset to 0 on success
- Auto-disabled after `max_retries` consecutive failures (default: 3)
- `next_run_at` computed from `cron_expression` via `croner` crate or `interval_seconds`

The scheduler seeds 8 default tasks on first run via `seed_default_tasks()`: Library Scan (daily 03:00), Metadata Refresh (daily 04:00), Database Maintenance (weekly Sun 05:00), Session Cleanup (every 1h), Notification Cleanup (daily 02:00), Disk Space Check (every 30min), Media Health Check (weekly Sun 06:00), Soft Delete Purge (daily 01:00).

The `metadata_refresh` task type handles Phase 5 (re-enriching metadata for existing items).

### Error Handling (ERROR_HANDLING.md)

The scanner uses `ScannerError` (internal) mapped through `AppError::Internal` for HTTP responses. Scan errors are collected per-file in `ScanResult.errors` as `ScanError` structs (path, phase, message) — batch operations use partial success rather than RFC 9457 problem details for individual files.

Existing LIB error codes registered in ERROR_HANDLING.md:
- `LIB_006` (409): Scan already in progress for this library — not yet wired (scan is synchronous; will be enforced when async background scan is implemented)
- `LIB_007` (503): Filesystem watcher failed to start — implemented in Task 7; watcher failures logged but not surfaced to API

### Logging (LOGGING_OBSERVABILITY.md)

- Scan phases logged at `INFO` level with library name, phase, file counts
- Individual file probe errors logged at `WARN` level
- Unmatched files logged at `DEBUG` level with parsed name
- Metrics: `library.scan.duration` histogram, `library.files.total` gauge, `library.scan.errors.total` counter

### Analytics (DATABASE.md — Activity & Analytics Domain)

Scan results feed into the existing analytics infrastructure:
- `play_sessions` and `play_events` track what's being watched
- Scanner tracks what's available — the UI shows "newly added" content based on `media_items.created_at` from recent scans

### Backup & Recovery (BACKUP_RECOVERY.md)

The `media_files` table (including `file_hash`, `file_modified_at`) is part of the PostgreSQL database backed up by WAL-G. If the database is restored, the scanner can verify file health by re-checking `is_healthy` status.

---

## Implementation Status

**Phase 5 Tasks 5-7 (complete):**

- `workers/library_scanner.rs` — 6-phase pipeline implemented: Discover, Diff, Probe, Identify, Enrich (stub), Cleanup
- `services/scheduler.rs` — Scheduled task runner with `croner` cron evaluation, 30s tick, 8 seeded defaults
- `services/fs_watcher.rs` — Cross-platform FS watcher with `notify` 8.2 + `notify-debouncer-full` 0.7; 3-second debounce, media extension filtering, bulk import detection, per-library cooldown, channel-based event processing
- Crates added: `ignore` 0.4, `blake3` 1, `regex` 1, `croner` 3, `notify` 8, `notify-debouncer-full` 0.7, `quick-xml` 0.40
- Handler `scan_library` wired to scanner for synchronous manual scans
- Scheduler wired in `main.rs` with `library_scan` executor for periodic scheduled scans
- FS watcher wired in `main.rs` startup, library/path CRUD handlers for dynamic watch/unwatch lifecycle
- `LibraryWatcherManager` in `AppState` for shared access between handlers and main.rs
- `ScannerError` mapped via `AppError::Internal`; per-file errors in `ScanResult.errors` array

**Phase 5 Task 8 (complete):**

- `services/media_matching.rs` — Dedicated service module for the 5-layer identification cascade; extracted from monolithic scanner
- `.media-match` parser enhanced: `pattern:` line with token interpolation (`{s}`, `{season}`, `{e}`, `{episode}`, `{sp}`, `{special}`), `edition:` field, season-level cascading
- `pattern:` tokens converted to regex capture groups via existing `regex` crate — no new dependencies
- Episode overrides from `ep:` lines now wired into TV show identification pipeline
- Season-level `.media-match` cascading: series folder file applies to all seasons; season folder file overrides for that season only
- NFO parsing and provider ID tag parsing moved from scanner into service module
- Scanner `resolve_identification_layers()` replaced by `media_matching::resolve_identification()`

**Phase 5 Task 9 (complete):**

- `services/nfo_parser.rs` — Dedicated NFO parsing module using `quick-xml` 0.40 streaming StAX parser
- Supports all NFO tag formats found in the wild:
  - Modern Kodi v19+ `<uniqueid type="tmdb|imdb|tvdb" default="...">` format
  - Legacy flat tags: `<tmdbid>`, `<imdbid>`, `<imdb_id>`, `<tvdbid>`
  - URL-only format: `https://www.themoviedb.org/movie/...`, `https://www.imdb.com/title/...`
- Supports all root elements: `<movie>`, `<tvshow>`, `<episodedetails>`
- Supports episode NFO: extracts `<season>` and `<episode>` from `<episodedetails>`
- Discovers NFO files: `movie.nfo`, `tvshow.nfo`, `<filename>.nfo` (video basename match)
- Graceful degradation: stops at closing root tag, ignores trailing content after `</movie>` (common Jellyfin bug)
- `NfoData` expanded: added `season` and `episode` fields for episode-level NFO
- Replaced regex-based `parse_nfo_file()` in `media_matching.rs` with call to `nfo_parser::parse_nfo()`
- Crate added: `quick-xml` 0.40

**Phase 5 Task 10 (complete):**

- `parse_provider_id_tag()` refactored to `parse_provider_id_tags()` — extracts ALL provider IDs from a string using `captures_iter()` instead of single `captures()`
- Multi-ID extraction: `{tmdb-272}{imdb-tt0381061}` now returns both IDs instead of only the first
- Curly braces (`{tmdb-XXX}`) take priority over square brackets (`[tmdbid=XXX]`) for the same provider per LIBRARY_ORGANIZATION.md; different providers are merged
- `resolve_identification()` now accepts `filename: Option<&str>` parameter — checks both folder name and filename for provider ID tags; folder name IDs take priority, filename IDs fill in missing providers
- Movie scanner passes `file.path.file_stem()` as filename; TV series scanner passes `None` (tags go on series folder)
- Regex patterns compiled via `std::sync::LazyLock` statics (`CURLY_TAG_RE`, `BRACKET_TAG_RE`) — avoids recompilation per call
- No new workspace dependencies — uses existing `regex` crate and `std::sync::LazyLock` (Rust edition 2024 stable)

**Not yet implemented:**

- Phase 5 (Enrich) is a stub — metadata provider integration deferred to Phase 6
- TMDB API search deferred to Phase 6 (Layer 4 API lookup)
- `walkdir` not yet used (targeted re-scans deferred)
- `LIB_006` scan-in-progress guard not yet enforced (scan is synchronous; needs async background with 202 response)
- `LIB_007` watcher failure logged but not surfaced to API
