# Poster Management

## Overview

The poster management system controls the complete artwork lifecycle: **source → select → customize → display**. TMDb's default poster is just one source option among many — users can upload custom art, use an asset directory, import community art, or generate overlays (see [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md)). This replaces the concept of static "default posters" with a fully customizable visual layer.

This document is one of three pillars in the artwork customization architecture:
- [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md) — overlay compositing engine (badges, text, dynamic content)
- [COLLECTIONS.md](COLLECTIONS.md) — static and dynamic collections with custom poster art
- **This document** — artwork lifecycle, sourcing, selection, locking, bulk operations

## Artwork Lifecycle

```
┌─────────────────────────────────────────────────────────────────────┐
│                      Artwork Lifecycle                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  1. SOURCE                                                          │
│     Where does the artwork come from?                               │
│     - TMDb API (primary metadata provider)                          │
│     - User upload (admin/custom artwork)                            │
│     - Asset directory (per-item custom art on disk)                 │
│     - Community templates (shared art packs)                        │
│     - Overlay engine (composited from source + overlays)            │
│                                                                      │
│  2. SELECT                                                          │
│     Which artwork is active for this item?                          │
│     - Auto: highest-rated TMDb artwork (by vote count)              │
│     - User choice: admin picks from available options               │
│     - Locked: prevents auto-refresh from changing the selection     │
│                                                                      │
│  3. CUSTOMIZE                                                       │
│     Transform the selected artwork:                                 │
│     - Overlay application (badges, ratings, resolution indicators)  │
│     - Cropping / resizing to standard canvas                       │
│     - Format conversion (JPEG → WebP for cache)                    │
│                                                                      │
│  4. DISPLAY                                                         │
│     Serve the final artwork to clients:                             │
│     - If overlays active: serve composited result from cache        │
│     - If no overlays: serve source artwork directly                │
│     - Clients receive via /api/v1/items/{id}/artwork endpoints      │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

> **Format policy:** All server-generated image content (resized variants, overlay composites, storyboard sprites, user-upload derivatives) is delivered as **WebP**. Source originals from upstream providers (JPEG/PNG) are preserved untouched. See [IMAGE_FORMATS.md](IMAGE_FORMATS.md) for the full format decision, platform support matrix, encoding settings, and edge cases.

## Artwork Sources

### TMDb (Primary)

TMDb provides the default artwork for all matched media items. During Phase 5 enrichment (see [MEDIA_SCANNING.md](MEDIA_SCANNING.md)), the server fetches:

| Artwork Type | TMDb Endpoint | Image Sizes Available |
|---|---|---|
| Poster | `/movie/{id}/images` or `/tv/{id}/images` | w92, w154, w185, w342, w500, w780, original (up to 2000×3000) |
| Backdrop | Same endpoint | w300, w780, w1280, original (up to 3840×2160) |
| Season poster | `/tv/{id}/season/{n}/images` | Same as poster |
| Logo | `/movie/{id}/images` or `/tv/{id}/images` | SVG or PNG (w300, w500, original) |
| Episode still | `/tv/{id}/season/{n}/episode/{e}/images` | w92, w185, w300, original |

**Image URL construction:**
```
{base_url}/{size}/{file_path}
```
Where `base_url` comes from `GET /configuration` API (e.g. `https://image.tmdb.org/t/p/`).

**Fetching strategy:**
- Download `original` size for local storage (best quality for overlay compositing)
- Store resized versions in cache for different client needs
- Respect TMDb's CC BY 4.0 attribution requirement (display "Powered by TMDb" in UI footer)
- Cache downloaded artwork locally — never re-download unless forced

**Language priority:** Fetch artwork in the library's `metadata_language` first, then English as fallback, then any language. Configurable via `server_config.metadata.artwork_language_priority`.

### User Upload

Admins can upload custom artwork for any media item via the admin UI or API.

- Accepted formats: JPEG, PNG, WebP
- Auto-cropped to standard canvas sizes on upload
- Stored in `/data/metadata/artwork/uploads/`
- `source_type = 'user_upload'` on the `artwork` row
- Automatically locked (`is_locked = true`) to prevent auto-refresh overwriting

### Asset Directory

Per-item custom artwork in a designated directory on disk. Inspired by Kometa's asset directory.

```
/data/assets/
├── movies/
│   ├── The Matrix (1999)/
│   │   ├── poster.jpg          ← custom poster
│   │   ├── poster.png          ← alt format (both work)
│   │   └── background.jpg      ← custom backdrop
│   └── Inception (2010)/
│       └── poster.jpg
├── tv/
│   ├── Breaking Bad (2008)/
│   │   ├── poster.jpg
│   │   ├── Season 01.jpg       ← season poster
│   │   ├── Season 02.jpg
│   │   └── background.jpg
│   └── The Bear (2022)/
│       └── poster.png
└── collections/
    ├── Marvel Cinematic Universe.jpg
    └── Studio Ghibli.jpg
```

**Discovery rules:**
- Asset directory path configured in `server_config.metadata.asset_directory` (default: `/data/assets/`)
- Matches by item folder name (exact match) or TMDb ID in filename
- Season posters: `Season XX.jpg` or `SeasonXX.jpg` or `Season_XX.jpg`
- Collection posters: `/collections/{collection_name}.jpg`
- When asset art exists: takes priority over TMDb artwork; sets `is_locked = true`

### Community Art Packs

Curated sets of artwork distributed as JSON + image archives. Importable via the admin UI template browser.

```json
{
  "name": "Minimalist Movie Posters",
  "version": 1,
  "author": "CommunityMember",
  "artwork": [
    {
      "tmdb_id": 603,
      "title": "The Matrix",
      "poster": "posters/603.jpg",
      "source": "user_submission"
    }
  ]
}
```

- Matched by `tmdb_id` during import
- Imported images stored in `/data/metadata/artwork/community/`
- `source_type = 'community'` on the `artwork` row
- Can be locked or unlocked (TMDb refresh overwrites if unlocked)

### Overlay Engine (Composited)

The overlay engine (see [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md)) generates composited artwork by applying badge/text overlays to the source artwork. This is not a separate "source" but a transformation of an existing source.

- Stored in `/cache/images/overlays/`
- Tracked in `artwork_overlay_state` table
- Refreshed on overlay definition change or source artwork change

## Artwork Selection Priority

When a client requests artwork for a media item, the server resolves the active artwork in this order:

| Priority | Source | Condition |
|---|---|---|
| 1 | Locked artwork | `artwork.is_locked = true` — never overwritten |
| 2 | Asset directory | File exists in configured asset path |
| 3 | User upload | `source_type = 'user_upload'` |
| 4 | Community import | `source_type = 'community'` and not superseded |
| 5 | TMDb (highest voted) | `source_type = 'tmdb'`, ordered by TMDb vote count |

**For display** (when overlays are active):

| Priority | Source | Condition |
|---|---|---|
| 1 | Overlaid result | `artwork_overlay_state` row exists → serve `/cache/images/overlays/` |
| 2 | Source artwork | No overlays applied → serve directly from `artwork.local_path` |

## Poster Locking

Locking prevents automatic metadata refreshes from changing the selected artwork.

**When artwork is automatically locked:**
- User uploads a custom image (`source_type = 'user_upload'`)
- Asset directory image is discovered for an item
- Admin manually locks via the UI

**When artwork is NOT locked:**
- TMDb artwork downloaded during scan/refresh
- Community artwork (unless explicitly locked by admin)

**Lock behavior:**
- Locked artwork is never overwritten by metadata refresh
- Lock does NOT prevent overlay re-compositing — overlays are always applied to the locked artwork
- Admin can unlock to allow TMDb refresh to update it
- Lock state is stored in `artwork.is_locked` column

## Bulk Operations

The admin UI provides bulk artwork operations:

| Operation | Description | Scope |
|---|---|---|
| Refresh All Artwork | Re-download TMDb artwork for all items | Library or server-wide |
| Refresh Missing Artwork | Download artwork only for items with no artwork | Library or server-wide |
| Apply Overlays Now | Force overlay re-compositing for all items | Library or server-wide |
| Remove All Overlays | Strip overlays, restore source artwork | Library or server-wide |
| Reset to Default | Unlock and re-download TMDb artwork | Per-item |
| Import Asset Directory | Scan asset directory and apply discovered art | Server-wide |
| Import Community Pack | Import a JSON + image archive | Server-wide |

## TMDb Artwork API Integration

### Configuration Endpoint

```
GET https://api.themoviedb.org/3/configuration?api_key={KEY}
```

Returns `images.base_url`, `images.secure_base_url`, `images.poster_sizes`, `images.backdrop_sizes`, `images.logo_sizes`.

### Artwork Fetching

```
GET https://api.themoviedb.org/3/movie/{id}/images?api_key={KEY}&include_image_language=en,null
```

Returns all available artwork for an item. Multiple posters may exist — the server downloads all and stores them in the `artwork` table with different `order` values. The highest-voted poster is set as `order = 0` (primary).

### Rate Limiting

| Aspect | Limit | Strategy |
|---|---|---|
| TMDb API | ~40 requests/10 seconds | Token bucket; batch artwork fetches during enrichment |
| Image downloads | Reasonable use | Download `original` size once; cache locally |

### Attribution

TMDb requires attribution (CC BY 4.0). The server displays "Powered by TMDb" in the web client footer and API documentation.

## Database Schema

### Artwork Table Extension

The existing `artwork` table gains two new columns:

```sql
ALTER TABLE artwork ADD COLUMN is_locked BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE artwork ADD COLUMN source_type TEXT
    CHECK (source_type IS NULL OR source_type IN ('tmdb', 'user_upload', 'asset_directory', 'community'));
```

Full `artwork` table definition with extensions:

```sql
CREATE TABLE artwork (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_item_id UUID REFERENCES media_items(id) ON DELETE CASCADE,
    artwork_type TEXT NOT NULL CHECK (artwork_type IN ('poster', 'backdrop', 'thumbnail', 'logo', 'banner', 'season_poster')),
    source_url TEXT,
    local_path TEXT,
    width INT,
    height INT,
    language TEXT,
    provider TEXT,
    "order" INT NOT NULL DEFAULT 0,

    is_locked BOOLEAN NOT NULL DEFAULT false,
    source_type TEXT
        CHECK (source_type IS NULL OR source_type IN ('tmdb', 'user_upload', 'asset_directory', 'community')),

    UNIQUE(media_item_id, artwork_type, "order")
);

CREATE INDEX idx_artwork_media_item_id ON artwork (media_item_id);
```

### Artwork Storage on Disk

```
/data/metadata/artwork/
├── tmdb/                    ← downloaded from TMDb
│   ├── posters/
│   └── backdrops/
├── uploads/                 ← admin uploads via UI
│   ├── posters/
│   └── backdrops/
├── community/               ← imported from community packs
│   ├── posters/
│   └── backdrops/
└── assets/                  ← symlink or reference to asset directory

/cache/images/
├── clean/                   ← scaled source artwork (overlay base)
│   ├── posters/
│   └── backdrops/
├── overlays/                ← composited results
│   ├── posters/
│   └── backdrops/
└── resized/                 ← resized versions for different clients
    ├── w500/
    ├── w300/
    └── original/
```

The `/data/metadata/artwork/` directory is persistent storage. The `/cache/images/` directory is regenerable cache (can be deleted and rebuilt).

## MetadataConfig Rust Struct

The `server_config.metadata` JSONB column maps to this struct:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetadataConfig {
    pub artwork_language_priority: Vec<String>,
    pub artwork_auto_download: bool,
    pub artwork_download originals_only: bool,
    pub asset_directory: Option<String>,
    pub overlays_enabled: bool,
    pub overlay_apply_schedule: String,
    pub overlay_image_format: String,
    pub overlay_image_quality: i32,
    pub overlay_max_image_size_mb: i32,
    pub overlay_default_font: String,
    pub overlay_reapply_on_artwork_change: bool,
    pub collections_enabled: bool,
    pub collection_sync_schedule: String,
    pub collection_default_poster_source: String,
    pub collection_max_items_default: i32,
    pub collection_track_missing: bool,
    pub collection_external_rate_limit_per_minute: i32,
}

impl Default for MetadataConfig {
    fn default() -> Self {
        Self {
            artwork_language_priority: vec!["en".to_string()],
            artwork_auto_download: true,
            artwork_download_originals_only: true,
            asset_directory: None,
            overlays_enabled: true,
            overlay_apply_schedule: "0 5 * * *".to_string(),
            overlay_image_format: "webp".to_string(),
            overlay_image_quality: 90,
            overlay_max_image_size_mb: 10,
            overlay_default_font: "Inter".to_string(),
            overlay_reapply_on_artwork_change: true,
            collections_enabled: true,
            collection_sync_schedule: "0 6 * * *".to_string(),
            collection_default_poster_source: "auto".to_string(),
            collection_max_items_default: 100,
            collection_track_missing: true,
            collection_external_rate_limit_per_minute: 30,
        }
    }
}
```

**Field semantics:**
- `artwork_language_priority` — ISO 639-1 language codes in priority order for TMDb artwork fetching. First language tried first, then fallbacks.
- `artwork_auto_download` — automatically download artwork during scan enrichment. When false, artwork is only downloaded on explicit admin request.
- `artwork_download_originals_only` — download only `original` size from TMDb (best quality, for overlay compositing). Resized versions are generated server-side as WebP variants per [IMAGE_FORMATS.md](IMAGE_FORMATS.md).
- `asset_directory` — path to the asset directory on disk. When null, asset directory is not used.
- `overlays_enabled` — master toggle for the overlay engine. When false, source artwork is served directly without compositing.
- `overlay_apply_schedule` — cron schedule for the overlay application task.
- `overlay_image_format` — output format for composited images. Options: `webp` (default, best compression), `png` (lossless), `jpeg`. See [IMAGE_FORMATS.md](IMAGE_FORMATS.md) for why WebP is the recommended default.
- `overlay_image_quality` — quality for lossy formats (1-100). Ignored for PNG.
- `overlay_max_image_size_mb` — maximum composited image size. Images exceeding this are quality-reduced.
- `overlay_default_font` — default font for text overlays. Must be a font file in `/data/fonts/`.
- `overlay_reapply_on_artwork_change` — automatically re-apply overlays when source artwork changes.
- `collections_enabled` — master toggle for the dynamic collection system.
- `collection_sync_schedule` — cron schedule for dynamic collection sync.
- `collection_default_poster_source` — default poster source for auto-generated collections: `auto` (TMDb collection art), `overlay` (generate via overlay engine), `none`.
- `collection_max_items_default` — default maximum items per dynamic collection.
- `collection_track_missing` — track items from external builders that aren't in the local library.
- `collection_external_rate_limit_per_minute` — rate limit for external API calls during collection sync.

## Scheduled Tasks

Two new scheduled tasks are registered in the existing `scheduled_tasks` table:

| Task | Schedule | Timeout | Config |
|---|---|---|---|
| Overlay Application | `0 5 * * *` (daily 05:00) | 2h | `{ "reapply_all": false, "max_concurrent": 2 }` |
| Collection Sync | `0 6 * * *` (daily 06:00) | 2h | `{ "sync_dynamic": true, "sync_external": true }` |
| Artwork Refresh | Every 21600s (6 hours) | 2h | `{ "refresh_missing": true, "refresh_max_age_hours": 168 }` |
| Asset Directory Scan | `0 3 * * *` (daily 03:00, before overlays) | 30m | `{ "path": null }` |

**Artwork Refresh** — checks for items with missing artwork and re-downloads from TMDb. Also refreshes artwork older than `refresh_max_age_hours` (default: 168 = 7 days).

**Asset Directory Scan** — scans the configured asset directory for new/changed custom artwork and applies it to matching items.

## Admin UI

The artwork management section provides:

1. **Artwork browser** — view all artwork for an item; poster grid with source badges (TMDb, upload, asset, community)
2. **Artwork selector** — click to set active artwork; lock/unlock toggle
3. **Upload interface** — drag-and-drop poster/backdrop upload with preview
4. **Asset directory viewer** — browse discovered asset art; match status
5. **Bulk operations toolbar** — refresh, apply overlays, reset, import
6. **Overlay preview** — live preview of what overlays look like on a selected poster
7. **Template browser** — community art packs and overlay templates

## Cross-References

- [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md) — overlay compositing uses source artwork managed here; `artwork_overlay_state` table; clean art preservation
- [COLLECTIONS.md](COLLECTIONS.md) — collection poster sourcing; artwork selection for collection posters
- [DATABASE.md](DATABASE.md) — `artwork` table extensions (`is_locked`, `source_type`); `overlay_definitions`; `artwork_overlay_state`
- [MEDIA_SCANNING.md](MEDIA_SCANNING.md) — Phase 5 enrichment triggers initial artwork download
- [CONFIGURATION.md](../operations/CONFIGURATION.md) — `MetadataConfig` Rust struct; `server_config.metadata` JSONB
- [CACHE_STORAGE.md](../operations/CACHE_STORAGE.md) — `/cache/images/` storage tier; artwork cache size limits
- [ERROR_HANDLING.md](ERROR_HANDLING.md) — OVERLAY_001–OVERLAY_006 and COLL_001–COLL_008 error codes

## Implementation Notes

### TMDB Artwork Download (Phase 6, Task 8)

- **Module:** `server/src/services/artwork_downloader.rs` — see [METADATA_PROVIDERS.md](METADATA_PROVIDERS.md) Artwork Downloader section for full details.
- **Storage layout implemented:** `{data_dir}/metadata/artwork/tmdb/{posters,backdrops,logos}/{tmdb_id}_{filename}` — matches the `/data/metadata/artwork/tmdb/` layout from the Disk Storage section above.
- **Download `original` size only** — per `artwork_download_originals_only = true` in `MetadataConfig`. URL constructed as `{secure_image_base_url}original{file_path}` using cached TMDB configuration.
- **Vote-sorted selection** — images sorted by `vote_count` desc, then `vote_average` desc; top 5 posters, 3 backdrops, 2 logos downloaded per item.
- **Deduplication** — `source_url` column checked before download; existing artwork rows are skipped.
- **`artwork` table rows** — inserted with `source_type = 'tmdb'`, `provider = 'tmdb'`, `order` by vote ranking (0 = primary), `width`/`height` from TMDB API, `language` from TMDB `iso_639_1`. Uses `ON CONFLICT DO NOTHING` on `(media_item_id, artwork_type, "order")`.
- **Not yet implemented:** User upload, asset directory scanning, community packs, overlay compositing, resized cache generation — deferred to Phases 8, 12, 13.
