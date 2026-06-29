# Cache & Storage Strategy

## Overview

A three-tier storage architecture with per-cache-type size limits, LRU eviction, disk space monitoring, and admin-configurable paths. Designed to scale from a single-user NAS (2 TB HDD) to a large shared server (40+ TB, multiple storage tiers).

This strategy directly addresses the most common storage problems in existing self-hosted media servers:
- **Plex:** Monolithic data directory fills SSD boot drives; BIF thumbnails consume 10-50 MB per item with no eviction; no per-library cache control
- **Jellyfin:** Transcode directory grows unbounded (GitHub issue #3929, open since 2020, 54+ likes); no cache size limits; users report 200 GB metadata directories locking up servers on HDDs

---

## Three-Tier Storage Architecture

### Tier Model

| Tier | Storage Media | Contents | Path | Admin Configurable |
|---|---|---|---|---|
| **Hot** | SSD / NVMe | PostgreSQL, config, active metadata, logs | `/data` | `data_dir` (bootstrap) |
| **Warm** | SSD or HDD | Storyboards, image cache, HLS segments, search index | `/cache` | `cache_dir` (bootstrap) + `server_config.storage` per-type overrides |
| **Cold** | HDD / NAS | Source media files (read-only) | `/media` | Library root paths |

### Why Three Tiers

Industry research (Everpure, OpenMetal, StarWind — 2026) shows that 85% of production data is inactive; only 10-20% is actively used. Tiered storage yields up to 98% cost savings vs. untiered storage by matching storage investment to access patterns.

| Approach | Problem |
|---|---|
| **Single volume (Plex-style)** | SSD overflow is Plex's #1 support issue; can't tier across storage media; no eviction without deleting actual data |
| **Automatic tiering (SSD↔HDD)** | Over-engineered for single-server self-hosted; requires dual-mount awareness |
| **No limits (Jellyfin-style)** | Users report complete server lockups from disk-full transcode directories |
| **All RAM (tmpfs)** | Storyboards are too large for RAM (25+ GB for large libraries); only appropriate for ephemeral transcode data |

### Design Rationale

| Decision | Rationale |
|---|---|
| Separate `/data` and `/cache` | Cache is high-write, regenerable — can go on slower storage. Safe to delete. Database needs SSD speed (10-20x faster queries per Jellyfin community reports). |
| Storyboards in `/cache` | Regenerable derived data (~5 MB per movie); belongs on warm tier; power users can relocate to HDD |
| Transcodes as tmpfs | RAM-backed, auto-cleaned on restart, zero disk wear; community consensus across Plex and Jellyfin |
| Media mounted read-only | Server never modifies source files; prevents accidental corruption |
| Per-type path overrides | Power users can put storyboards on HDD while keeping database on NVMe |

---

## Cache Types

### Overview

| Cache Type | Directory | Default Limit | Eviction Policy | Lifecycle |
|---|---|---|---|---|
| **Transcode segments** | `/data/transcode` (tmpfs) | tmpfs size (default 2 GB) | TTL: delete on session end; orphan cleanup on startup | Ephemeral — purged on restart |
| **HLS segments** | `/cache/hls` | 4 GB | TTL: delete after session end; orphan cleanup on startup | Ephemeral — safe to delete anytime |
| **Storyboards** | `/cache/storyboards` | No limit (configurable) | LRU + size cap: evict least-recently-accessed items | Semi-persistent — regenerable |
| **Image cache** | `/cache/images` | 2 GB | LRU: evict oldest resized images | Semi-persistent — regenerable |
| **Search index** | `/cache/search` | Auto-managed | Rebuild on content change | Semi-persistent — regenerable |

### Transcode Segments

**Path:** `/data/transcode` (tmpfs in Docker)

Transcode segments are the most write-intensive cache — a single 4K transcode can write 100 MB in 20 seconds. They must be RAM-backed to prevent SSD wear and ensure fast I/O.

| Setting | Default | Config |
|---|---|---|
| Storage type | tmpfs (RAM) | Docker compose: `tmpfs: /data/transcode:size=2G` |
| Max disk space | 2 GB (tmpfs) | `TRANSCODE_TMPFS_SIZE` env var |
| Cleanup trigger | Session end (stop, timeout, error) | Automatic |
| Orphan cleanup | Server startup | Automatic — scans and deletes stale segments |
| Overflow action | Kill oldest transcode session; return `PLAY_010` | `server_config.transcoding.max_disk_space_mb` |

**Disk space estimation per concurrent stream:**

| Quality | Bitrate | RAM per stream (5 min buffer) |
|---|---|---|
| 480p | 1.5 Mbps | ~60 MB |
| 720p | 3 Mbps | ~120 MB |
| 1080p | 6 Mbps | ~240 MB |
| 1080p HQ | 10 Mbps | ~400 MB |
| 4K (direct play) | 0 (no transcode) | 0 |

With a 2 GB tmpfs, the server handles ~5-8 concurrent 1080p transcodes. For 4K transcoding, increase to 4 GB.

### HLS Segments

**Path:** `/cache/hls`

When the server remuxes or transcodes to HLS, segments are written here. Unlike the transcode working directory, HLS segments are served to clients via HTTP.

| Setting | Default | Config |
|---|---|---|
| Max cache size | 4 GB | `server_config.storage.hls_cache_max_size_mb` |
| Cleanup trigger | Session end + orphan cleanup | Automatic |
| Orphan cleanup | Every 15 minutes | Scheduled check |

HLS segments for completed sessions are deleted immediately. Orphaned segments (from crashed sessions) are cleaned up every 15 minutes by a background task.

### Storyboards (Seek Preview Thumbnails)

**Path:** `/cache/storyboards/{media_file_id}/`

The largest regenerable cache. Storage scales linearly with library size.

| Library Size | 10s interval, 320px | 5s interval, 320px | 10s interval, 640px |
|---|---|---|---|
| 100 movies | ~500 MB | ~1 GB | ~2 GB |
| 1,000 movies | ~5 GB | ~10 GB | ~20 GB |
| 5,000 movies | ~25 GB | ~50 GB | ~100 GB |
| 10,000 movies | ~50 GB | ~100 GB | ~200 GB |

**Eviction strategy: LRU with size cap**

| Setting | Default | Config |
|---|---|---|
| Max cache size | No limit | `server_config.storage.storyboard_max_cache_gb` |
| Eviction policy | LRU (least recently played) | `storyboard_eviction_policy` |
| Priority retention | Items played in last 30 days | Automatic |
| First evicted | Items not played in 90+ days | Automatic |
| Regeneration | On-demand (next playback) or scheduled task | Automatic |

When the size cap is reached:
1. Query `storyboards` table joined with `user_item_data.last_played_at` to find least-recently-accessed items
2. Delete sprite sheets and WebVTT index from disk
3. Delete the `storyboards` row from the database
4. Log eviction for admin dashboard
5. Evicted items are auto-regenerated on next playback or during the next scheduled `storyboard_generation` task

### Image Cache

**Path:** `/cache/images`

Processed/resized versions of artwork images. When a client requests a poster at 300px width, the resized version is cached here.

| Setting | Default | Config |
|---|---|---|
| Max cache size | 2 GB | `server_config.storage.image_cache_max_size_mb` |
| Eviction policy | LRU (least recently accessed) | Automatic |
| Cleanup trigger | Size cap exceeded | Background check |

Original artwork is stored in `/data/metadata/artwork/` (persistent hot tier). Only resized derivatives go in the image cache.

**Implementation status:** The image cache is populated on-demand by the artwork delivery endpoint (`GET /api/v1/items/{id}/artwork/{type}?size={size}`, Phase 10 Task 10) via `services/image_pipeline.rs`. Variant files are stored as `{cache_root}/webp/{category}/{variant_label}/{artwork_id}.webp`. A future `artwork_variant_generator` scheduled task will pre-warm the cache after library scans per IMAGE_FORMATS.md "Background-first strategy".

**Overlay cache subdirectories (Phase 12 Task 4):** The image cache also hosts overlay-derived artifacts under two additional subdirectories:
- `/cache/images/clean/{type_subdir}/{artwork_id}.webp` — scaled-to-canvas source artwork (clean backups), content-addressed by the source artwork UUID so source changes auto-invalidate. See [METADATA_OVERLAYS.md](../design/METADATA_OVERLAYS.md) Clean Art Management.
- `/cache/images/overlays/{type_subdir}/{media_item_id}.webp` — composited results served to clients when overlays are active.
- `/cache/images/overlays/previews/` — one-off editor preview renders (not persisted in `artwork_overlay_state`).

Both are regenerable — deleting them triggers re-creation on the next overlay application or preview request. The clean backup and overlaid result are tracked in `artwork_overlay_state` (see [DATABASE.md](../design/DATABASE.md)).

### Search Index

**Path:** `/cache/search`

Full-text search is powered by PostgreSQL TSVECTOR (see DATABASE.md). The `/cache/search` directory stores any additional search-related artifacts. This is auto-managed and rarely exceeds 500 MB.

---

## Disk Space Monitoring

### Monitoring Task

A new scheduled task (`disk_space_check`) monitors storage health:

| Parameter | Value |
|---|---|
| Task type | `disk_space_check` |
| Default schedule | Every 1800 seconds (30 minutes) |
| Timeout | 1 minute |
| Config | `{ "check_paths": true }` |

### Monitored Paths & Thresholds

| Path | Default Threshold | Alert Type | Priority |
|---|---|---|---|
| `/data` volume | 90% usage | `server_alert` | High |
| `/cache` volume | 90% usage | `server_alert` | Medium |
| `/data/transcode` (tmpfs) | 80% of allocation | Kill oldest session | Critical |
| PostgreSQL tablespace | 90% of `/data` disk | `server_alert` | High |

### Threshold Configuration

Stored in `server_config.storage` JSONB:

```json
{
    "disk_space_warnings": {
        "data_threshold_percent": 90,
        "cache_threshold_percent": 90,
        "transcode_threshold_percent": 80,
        "check_interval_seconds": 1800,
        "notify_on_warning": true
    }
}
```

When a threshold is exceeded:
1. Create a `server_alert` notification for all admin users
2. Log at `WARN` level with current usage details
3. For transcode overflow: kill the oldest active transcode session, return `PLAY_010` to the affected client
4. Do not auto-delete storyboards or image cache — only alert (admin decides whether to adjust limits or add storage)

### Storage Metrics

Exposed via Prometheus `/metrics` endpoint:

| Metric | Type | Labels | Description |
|---|---|---|---|
| `storage_usage_bytes` | gauge | path (`data`, `cache`, `transcode`) | Current usage in bytes |
| `storage_capacity_bytes` | gauge | path | Total capacity in bytes |
| `storage_usage_percent` | gauge | path | Usage as percentage |
| `cache_evictions_total` | counter | cache_type (`storyboard`, `image`, `hls`) | Items evicted by cache type |
| `cache_size_bytes` | gauge | cache_type | Current cache size |
| `cache_items` | gauge | cache_type | Number of cached items |

---

## Admin Storage Dashboard

A dedicated section in the admin UI providing:

### Storage Overview

| Display | Source |
|---|---|
| `/data` volume usage (current / total / %) | `storage_usage_bytes` metric |
| `/cache` volume usage (current / total / %) | `storage_usage_bytes` metric |
| Per-cache-type breakdown (storyboards, images, HLS, search) | `cache_size_bytes` metric |

### Per-Cache-Type Details

| Cache Type | Current Size | Limit | Items | Last Eviction |
|---|---|---|---|---|
| Storyboards | 4.2 GB | 10 GB | 842 | 2 hours ago |
| Images | 1.1 GB | 2 GB | 3,204 | 1 day ago |
| HLS | 0 MB | 4 GB | 0 | — |
| Search | 45 MB | Auto | — | — |

### Actions

| Action | Effect |
|---|---|
| Clear storyboard cache | Delete all `/cache/storyboards/` + truncate `storyboards` table |
| Clear image cache | Delete all `/cache/images/` |
| Clear HLS cache | Delete all `/cache/hls/` |
| Clear all cache | Delete all `/cache/*` (triggers regeneration on next access) |

### Storage Trend

30-day trend chart showing growth per cache type, powered by Prometheus data.

---

## Path Configuration

### Bootstrap Paths (config.toml / ENV / CLI)

```toml
[server]
data_dir = "/data"      # Hot tier: DB, config, metadata, logs
cache_dir = "/cache"     # Warm tier: storyboards, images, HLS
```

### Runtime Path Overrides (server_config.storage JSONB)

Power users can relocate individual cache types to different storage:

```json
{
    "storyboard_path": "/cache/storyboards",
    "image_cache_path": "/cache/images",
    "hls_cache_path": "/cache/hls",
    "transcode_path": "/data/transcode",
    "backup_path": "/data/backups",
    "log_path": "/data/logs",
    "metadata_path": "/data/metadata"
}
```

This enables configurations like:

| Scenario | `/data` (SSD) | `/cache` (HDD) | Custom Override |
|---|---|---|---|
| Default | DB + config + metadata + logs + transcode | Storyboards + images + HLS + search | None |
| Power user | DB + config + logs + transcode | Storyboards + images + HLS + search | `metadata_path: "/mnt/hdd/metadata"` |
| NAS with multiple pools | DB + config + logs | Images + HLS + search | `storyboard_path: "/volume2/storyboards"`, `metadata_path: "/volume1/metadata"` |

### StorageConfig Rust Struct

```rust
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct StorageConfig {
    pub storyboard_path: PathBuf,
    pub image_cache_path: PathBuf,
    pub hls_cache_path: PathBuf,
    pub transcode_path: PathBuf,
    pub backup_path: PathBuf,
    pub log_path: PathBuf,
    pub metadata_path: PathBuf,

    pub storyboard_max_cache_gb: Option<u32>,
    pub image_cache_max_size_mb: u32,
    pub hls_cache_max_size_mb: u32,

    pub storyboard_eviction_policy: EvictionPolicy,

    pub disk_space_warnings: DiskSpaceWarnings,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum EvictionPolicy {
    Lru,
    Fifo,
    None,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DiskSpaceWarnings {
    pub data_threshold_percent: u8,
    pub cache_threshold_percent: u8,
    pub transcode_threshold_percent: u8,
    pub check_interval_seconds: u32,
    pub notify_on_warning: bool,
}
```

---

## Integration with Existing Systems

### Media Scanning (MEDIA_SCANNING.md)

No interaction — scanning reads source files from `/media` and writes metadata to `/data/metadata` and the database.

### Streaming (STREAMING.md)

Transcode segments written to `/data/transcode` (tmpfs). HLS segments served from `/cache/hls`. Disk space monitoring kills oldest transcode session if tmpfs fills.

### Storyboards (STORYBOARDS.md)

Sprite sheets and WebVTT index stored in `/cache/storyboards/`. The `storyboard_generation` scheduled task respects the size cap — before generating new storyboards, it checks the current cache size and evicts LRU items if the cap would be exceeded.

### Scheduled Tasks (DATABASE.md)

Two new task types:
- `disk_space_check` — monitors storage health (every 30 minutes)
- Cache cleanup is integrated into existing `session_cleanup` task (HLS orphan cleanup)

### Docker Deployment (DOCKER_DEPLOYMENT.md)

The three-tier architecture maps to Docker volumes:
- `/data` → `duskcue-data` named volume (SSD recommended)
- `/cache` → `duskcue-cache` named volume (SSD or HDD)
- `/media` → bind mounts (read-only, HDD/NAS storage)
- `/data/transcode` → tmpfs (RAM)
- `/var/run/postgresql` → tmpfs (RAM)

### Error Handling (ERROR_HANDLING.md)

`PLAY_010` (507) already exists for transcode disk space exhaustion. No new error codes needed — disk space alerts use the existing `server_alert` notification type.

---

## Storage Estimation by Library Size

### Small Library (100 movies, 20 TV seasons)

| Component | SSD (`/data`) | HDD/SSD (`/cache`) |
|---|---|---|
| PostgreSQL | ~50 MB | — |
| Config | <1 MB | — |
| Metadata/artwork | ~200 MB | — |
| Logs (30 days) | ~50 MB | — |
| Backups | ~100 MB | — |
| Storyboards | — | ~500 MB |
| Image cache | — | ~200 MB |
| Search | — | ~10 MB |
| **Total** | **~400 MB** | **~710 MB** |

SSD requirement: **1 GB minimum**. Comfortable on a 16 GB SSD.

### Medium Library (1,000 movies, 100 TV seasons)

| Component | SSD (`/data`) | HDD/SSD (`/cache`) |
|---|---|---|
| PostgreSQL | ~500 MB | — |
| Config | <1 MB | — |
| Metadata/artwork | ~2 GB | — |
| Logs (30 days) | ~100 MB | — |
| Backups | ~1 GB | — |
| Storyboards | — | ~5 GB |
| Image cache | — | ~1 GB |
| Search | — | ~50 MB |
| **Total** | **~3.6 GB** | **~6.1 GB** |

SSD requirement: **8 GB minimum** (16 GB recommended). Cache can go on HDD.

### Large Library (5,000 movies, 500 TV seasons)

| Component | SSD (`/data`) | HDD/SSD (`/cache`) |
|---|---|---|
| PostgreSQL | ~2 GB | — |
| Config | <1 MB | — |
| Metadata/artwork | ~10 GB | — |
| Logs (30 days) | ~200 MB | — |
| Backups | ~4 GB | — |
| Storyboards | — | ~25 GB |
| Image cache | — | ~2 GB |
| Search | — | ~200 MB |
| **Total** | **~16.2 GB** | **~27.2 GB** |

SSD requirement: **32 GB minimum** (64 GB recommended). Cache should go on HDD.

### Very Large Library (10,000+ movies, 1,000+ TV seasons)

At this scale, metadata and artwork become the dominant storage consumer on the hot tier. Admin should consider:
- Moving artwork to the warm tier via `metadata_path` override
- Setting storyboard cache limits (e.g. 50 GB)
- Using dedicated SSD for database only

---

## Phase 13a Task 8 Implementation Notes

### Worker

`server/src/workers/disk_space_check.rs` implements the `disk_space_check` scheduled task (Phase 13a Task 8). The task row was already seeded by `20260530070000_seed_default_data.sql` (interval 1800s, timeout 60s, config `{"check_paths":true}`) and is included in `seed_default_tasks`; Task 8 only registers the executor and implements the Rust worker — no seed migration is required.

### Disk-stats backend

The worker uses the **`sysinfo` 0.34** crate (already in the workspace, used by `lockfile.rs` for PID liveness) rather than `nix::sys::statvfs`, the `statvfs`/`fs2` crates, or raw `libc`/Win32 calls. `sysinfo` is cross-platform (Windows + Linux + macOS), has no `unsafe` surface, and exposes `Disks::new_with_refreshed_list()` → `Disk::mount_point() / total_space() / available_space()`. The disk enumeration call is wrapped in `tokio::task::spawn_blocking` to avoid blocking the scheduler thread on syscall-heavy enumeration.

### Path → disk resolution

`sysinfo` has no "free space for this path" helper (confirmed via docs.rs, June 2026, and Rust users forum). The worker resolves a path to its backing disk by selecting the disk whose `mount_point()` is the **longest prefix** of the (canonicalized) target path. This naturally handles:

- **tmpfs shadowing** — a `tmpfs` mounted at `/data/transcode` is a longer prefix than `/data`, so the transcode tier reports its RAM allocation (2 GB default), not the host `/data` volume.
- **Windows drive letters** — `C:\Users\...` matches the `C:\` disk.
- **Custom overrides** — admin-configured cache paths on separate volumes resolve to that volume's disk.

If the path does not exist (common in dev without Docker volumes) or no disk matches, the tier is recorded with `status: "unavailable"` rather than failing the run.

### Tier resolution

| Tier | Source | Default |
|---|---|---|
| `data` | `bootstrap.data_dir` | `/data` |
| `cache` | `bootstrap.cache_dir` | `/cache` |
| `transcode` | `RuntimeConfig.transcoding.transcode_path` | `/cache/transcodes` |

The transcode tier reads from `transcoding.transcode_path` (not a separate `storage.transcode_path`) because `TranscodingConfig` already owns that path and the transcode manager writes segments there.

### Config expansion

`StorageConfig` was expanded from an empty placeholder to hold `DiskSpaceWarnings` (the `disk_space_warnings` JSONB group). `#[serde(default)]` on both structs ensures existing `{}` storage JSONB rows deserialize into the CACHE_STORAGE.md defaults (90/90/80 thresholds, 1800s interval, `notify_on_warning: true`). The remaining `StorageConfig` fields from the design (paths, cache limits, eviction policy) are deferred to the future cache-eviction task — Task 8 only needs the warning thresholds.

### Notification boundary (Phase 13b complete)

CACHE_STORAGE.md specifies "Create a `server_alert` notification for all admin users" on threshold breach. Phase 13b is now complete — notification **dispatch** (Fluent templates, SSE + webhook fan-out, push channel) is fully implemented in `services/notification_dispatch.rs`. The disk-space worker (`workers/disk_space_check.rs`) currently logs WARN + records Prometheus metrics + persists run stats; integrating the dispatch pipeline to create `server_alert` notifications on threshold breach is a follow-up wiring task that wraps the existing worker findings. This mirrors the backup domain precedent (Task 4 read-only status preceded Task 5 execution).

### Metrics

Per §Storage Metrics, the worker emits three gauges with a `path` label (`data`/`cache`/`transcode`):

| Metric | Type | Description |
|---|---|---|
| `storage_usage_bytes` | gauge | `total - available` |
| `storage_capacity_bytes` | gauge | `total_space()` |
| `storage_usage_percent` | gauge | `usage / total * 100` |

The `cache_evictions_total` / `cache_size_bytes` / `cache_items` metrics belong to the future LRU eviction task and are out of scope.

### Fallible executor semantics

Threshold breach is a **finding, not a failure** — the worker returns `Ok(())` with `status: "threshold_exceeded"` (or `"healthy"`) in run stats. Only infrastructure errors (DB write failure, stats serialization) return `Err` so the scheduler marks the run failed. This matches `reindex_maintenance` and `backup_runner`.

---

## Research Sources

### Media Server Storage Issues
- Plex Support — "Why is my Plex Media Server directory so large?" (January 2021): BIF thumbnails are "one of the most common reasons for unexpectedly-large data directories"
- Plex Support — "Where is the Plex Media Server data directory located?" (July 2025): Monolithic data directory across all platforms
- TRaSH Guides — "Suggested Plex Media Server Settings" (August 2021): Video preview thumbnails warning about storage; database cache size recommendations; transcode directory recommendations
- Jellyfin GitHub Issue #3929 — "Set limit for transcode cache" (August 2020, 54+ likes): Users report complete server lockups from unbounded transcache growth; open since 2020
- Reddit r/jellyfin — "Jellyfin very slow with huge library" (2026): 200 GB metadata on HDD causing 15-20 second page loads; resolved by moving to SSD

### Tiered Storage Architecture
- Everpure — "Tiered Storage: Best Practices for Optimal Data Management" (2026): Four-tier model (Tier 0-3); 85% of production data is inactive; up to 98% cost savings with tiered storage
- OpenMetal — "Enterprise Storage Tier Offerings and Architecture" (January 2026): NVMe hot tier + HDD warm tier + erasure-coded cold tier; hybrid SSD+HDD architecture
- StarWind — "Data tiering strategy 2026: how to balance performance, cost, and scalability" (2026): Hot/warm/cold/archive tiers; automated tiering vs caching

### Cache Eviction Policies
- GeeksforGeeks — "Cache Eviction Policies | System Design" (May 2026): LRU, LFU, FIFO, Random Replacement, TTL; use case analysis per policy
- Medium — "Unlocking Cache Eviction Policies" (October 2024): LRU best for Duskcues (recently watched content likely re-watched); TTL for time-sensitive data; LFU for trending/popular content

### Transcode Storage
- Reddit r/PleX — "Transcoding to RAM disk — is it worth it?" (October 2023): Community consensus: ~200 MB per concurrent stream; RAM disk eliminates SSD wear; tmpfs is standard practice
- Jellyfin Docker setup guides (2026): Separate `/config` and `/cache` directories; config (DB) on SSD; cache can go on HDD
