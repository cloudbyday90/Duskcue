# Metadata Overlays

## Overview

The overlay engine composites badges, text, and visual indicators onto poster artwork — replacing static "default" posters with a fully customizable, dynamic visual layer. Every poster in the library passes through the overlay pipeline, which evaluates configured overlay definitions against each media item's metadata and composites the applicable ones onto the source artwork.

This system is one of three pillars in the artwork customization architecture:
- **This document** — overlay compositing engine (badges, text, dynamic content)
- [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md) — artwork sourcing, selection, locking, bulk operations
- [COLLECTIONS.md](COLLECTIONS.md) — static and dynamic collections with custom poster art

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                      Overlay Application Pipeline                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  1. Load source artwork (from artwork table / asset directory)      │
│     ↓                                                                │
│  2. Scale to standard canvas (1000×1500 poster / 1920×1080 bg)     │
│     ↓                                                                │
│  3. Evaluate overlay definitions against media item metadata        │
│     - Check conditions (resolution, codec, rating, language, etc.) │
│     - Resolve groups (highest-weight overlay wins per group)        │
│     - Resolve queues (stack overlays with spacing)                  │
│     - Apply suppress rules                                          │
│     ↓                                                                │
│  4. Composite applicable overlays onto canvas                        │
│     - Image overlays: alpha-blended PNG compositing                │
│     - Text overlays: font rendering + backdrop fill                │
│     - Backdrop overlays: solid/gradient background fills           │
│     ↓                                                                │
│  5. Store result in cache directory                                  │
│     ↓                                                                │
│  6. Update artwork_overlay_state (track applied overlays)           │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

The pipeline runs as a scheduled background task. Overlays are not applied during playback or browsing — they are pre-computed and cached. When overlay definitions change, only affected items are reprocessed.

## Canvas Standards

| Artwork Type | Canvas Size | Aspect Ratio | Notes |
|---|---|---|---|
| Poster | 1000 × 1500 | 2:3 | Movie posters, season posters |
| Backdrop | 1920 × 1080 | 16:9 | Background/fanart |
| Episode thumbnail | 1920 × 1080 | 16:9 | Episode images |

All source artwork is scaled to these standard dimensions before overlay application. Overlay positioning is always relative to the standard canvas — regardless of the source image's original size.

## Overlay Types

### Image Overlay

A PNG image with alpha transparency composited onto the artwork at a specified position.

```
┌──────────────────────┐
│       Poster         │
│                      │
│                      │
│    ┌─────┐           │
│    │ 4K  │  ← badge  │
│    └─────┘           │
│                      │
│                      │
└──────────────────────┘
```

- Source: local PNG file in `/data/overlays/`, bundled default, or URL (downloaded and cached)
- Must support transparency (PNG format)
- Can be any size — positioned via offset/alignment attributes

### Text Overlay

Dynamically rendered text based on media item metadata. Supports template variables.

```
┌──────────────────────┐
│       Poster         │
│  ┌────────────────┐  │
│  │ ★ 8.5/10      │  │  ← text with backdrop
│  └────────────────┘  │
│                      │
└──────────────────────┘
```

- Template syntax: `text(<<variable>>)` — e.g. `text(<<critic_rating>>/10)`, `text(S<<season_number0>>E<<episode_number0>>)`
- Font: TTF/OTF via `ab_glyph`; bundled fonts in `/data/fonts/`
- Full styling: font family, size, color, stroke, backdrop

### Backdrop Overlay

A solid or semi-transparent background fill, typically used behind text overlays.

- Color: hex with optional alpha (`#00000099` for 60% black)
- Can stand alone or pair with text overlay
- Auto-sizes to content or explicit width/height

## Positioning System

All overlays use a consistent positioning model:

| Attribute | Values | Description |
|---|---|---|
| `horizontal_align` | `left`, `center`, `right` | Anchor point on canvas |
| `horizontal_offset` | Integer (0–1000) or percentage string | Pixels from anchor point |
| `vertical_align` | `top`, `center`, `bottom` | Anchor point on canvas |
| `vertical_offset` | Integer (0–1500) or percentage string | Pixels from anchor point |

Common positions:

| Position | horizontal | vertical | Notes |
|---|---|---|---|
| Top-left | align: left, offset: 0 | align: top, offset: 0 | Rating badges |
| Top-center | align: center, offset: 0 | align: top, offset: 30 | Top banner text |
| Bottom-right | align: right, offset: 25 | align: bottom, offset: 25 | Resolution badge |
| Bottom-center | align: center, offset: 0 | align: bottom, offset: 50 | "Direct Play" text |

## Groups

Overlays in the same **group** are mutually exclusive — only the one with the highest `weight` is applied.

Example: Resolution group

```
group: resolution
├── "4K HDR"    weight: 40  ← wins for 4K+HDR items
├── "4K"        weight: 30
├── "HDR"       weight: 20
├── "1080P"     weight: 10
└── "720P"      weight: 5
```

A 4K HDR movie gets only the "4K HDR" badge. A 1080p SDR movie gets the "1080P" badge. A 720p movie gets "720P".

Use case: avoid stacking redundant resolution/HDR badges — show the most specific one.

## Queues

Overlays in the same **queue** stack vertically (or horizontally) with configurable spacing. The highest-weight overlay occupies the first queue position, the next highest occupies the second, and so on.

```
Queue: bottom-right (vertical stacking)
Position 1: ┌──────────┐  ← highest weight
            │ TMDb 8.5 │
            └──────────┘
Position 2: ┌──────────┐  ← second highest
            │ 4K HDR   │
            └──────────┘
Position 3: ┌──────────┐  ← third highest
            │ Dolby    │
            └──────────┘
```

Queue configuration:

| Attribute | Description |
|---|---|
| `queue_name` | Named queue (e.g. `bottom_right_ratings`) |
| `queue_direction` | `vertical` or `horizontal` |
| `queue_spacing` | Pixels between stacked overlays |
| `queue_initial_offset_h` | Starting horizontal offset |
| `queue_initial_offset_v` | Starting vertical offset |

Dynamic queues auto-calculate positions based on the number of qualifying overlays. If an overlay stops qualifying (e.g. rating changes), the remaining overlays shift up automatically.

## Suppress Rules

One overlay can suppress others when it applies to an item. This prevents visual clutter from overlapping information.

Example:

```
"4K HDR" overlay:
  suppresses: ["4k_badge", "hdr_badge"]

"Direct Play" overlay:
  suppresses: []  (never suppresses)
```

When the "4K HDR" overlay matches an item, the individual "4K" and "HDR" overlays are not applied — even if they also match. This works independently of groups.

## Special Text Variables

Text overlays support template variables resolved at application time from media item metadata:

### Media Properties

| Variable | Description | Example Output |
|---|---|---|
| `<<title>>` | Item title | "The Matrix" |
| `<<year>>` | Release year | "1999" |
| `<<runtime>>` | Runtime in minutes | "136" |
| `<<runtimeH>>` | Hours portion of runtime | "2" |
| `<<runtimeM>>` | Minutes remaining | "16" |
| `<<content_rating>>` | Content rating | "R" |

### Video Properties (from `media_files`)

| Variable | Description | Example Output |
|---|---|---|
| `<<resolution>>` | Video resolution label | "4K", "1080P", "720P" |
| `<<video_codec>>` | Video codec name | "HEVC", "H.264" |
| `<<video_dynamic_range>>` | HDR/SDR label | "HDR10", "DV", "SDR" |
| `<<audio_codec>>` | Primary audio codec | "TrueHD", "DTS-HD MA" |
| `<<audio_channels>>` | Audio channel count | "7.1", "5.1", "2.0" |
| `<<bitrate>>` | Video bitrate (first file) | "25000" |
| `<<bitrateH>>` | Highest bitrate across versions | "45000" |
| `<<container>>` | Container format | "MKV", "MP4" |

### Ratings (from `media_items`)

| Variable | Description | Example Output |
|---|---|---|
| `<<critic_rating>>` | Critic rating | "8.7" |
| `<<critic_rating/>>` | Critic rating scaled to 5 | "4.4" |
| `<<audience_rating>>` | Audience/user rating | "8.5" |
| `<<rating_vote_count>>` | Number of votes | "12543" |

### TV-Specific (episodes, seasons)

| Variable | Description | Example Output |
|---|---|---|
| `<<season_number>>` | Season number (no padding) | "1" |
| `<<season_number0>>` | Season number (zero-padded) | "01" |
| `<<episode_number>>` | Episode number (no padding) | "5" |
| `<<episode_number0>>` | Episode number (zero-padded) | "05" |
| `<<series_title>>` | Parent series title | "Breaking Bad" |

### File Analysis

| Variable | Description | Example Output |
|---|---|---|
| `<<file_size>>` | File size (human-readable) | "42.3 GB" |
| `<<edition>>` | Edition name from filename | "Extended", "Remux" |
| `<<video_format>>` | Inferred format | "Remux", "BluRay", "WEB-DL" |

Modifiers: Append `/` to scale ratings (÷2 for /5 scale), `0` suffix for zero-padding.

## Conditions

Each overlay definition has a `conditions` JSONB field that determines when it applies. The overlay engine evaluates these against media item metadata.

### Condition Schema

```json
{
  "operator": "and",
  "rules": [
    { "field": "video_resolution", "op": "eq", "value": "4K" },
    { "field": "video_dynamic_range", "op": "in", "values": ["hdr10", "dolby_vision_p7", "dolby_vision_p8.1"] }
  ]
}
```

### Supported Fields

| Field | Source Table | Type | Examples |
|---|---|---|---|
| `video_resolution` | `media_files` | Text | "4K", "1080P", "720P", "480P" |
| `video_codec` | `media_files` | Text | "HEVC", "H.264", "AV1" |
| `video_dynamic_range` | `media_files` | Text | "sdr", "hdr10", "dolby_vision_p5" |
| `audio_codec` | `media_files` | Text | "TrueHD", "DTS-HD MA", "AAC" |
| `audio_channels` | `media_files` | Integer | 2, 6, 8 |
| `container_format` | `media_files` | Text | "MKV", "MP4" |
| `content_rating` | `media_items` | Text | "G", "PG", "R" |
| `media_type` | `media_items` | Text | "movie", "episode", "series" |
| `library_id` | `media_items` | UUID | Specific library |
| `genre` | `genres` (via `media_genres`) | Text | "Action", "Comedy" |
| `has_dolby_vision` | `media_files.additional_streams` | Boolean | true |
| `has_multiple_versions` | derived (count `media_files`) | Boolean | true |
| `critic_rating_above` | `media_items.rating_average` | Numeric | 8.0 |
| `streaming_on` | `media_items.metadata` | Text | "netflix", "disney+" |
| `original_language` | `media_items.metadata` | Text | "en", "ja", "ko" |
| `edition` | `media_files.metadata` | Text | "extended", "remux" |

### Operators

| Operator | Description | Example |
|---|---|---|
| `eq` | Equals | `{ "field": "video_resolution", "op": "eq", "value": "4K" }` |
| `neq` | Not equals | `{ "field": "video_dynamic_range", "op": "neq", "value": "sdr" }` |
| `in` | In list | `{ "field": "audio_codec", "op": "in", "values": ["TrueHD", "DTS-HD MA"] }` |
| `gt` / `gte` | Greater than / or equal | `{ "field": "audio_channels", "op": "gte", "value": 6 }` |
| `lt` / `lte` | Less than / or equal | `{ "field": "critic_rating_above", "op": "gte", "value": 8.0 }` |
| `exists` | Field has a value | `{ "field": "has_dolby_vision", "op": "exists", "value": true }` |
| `matches` | Regex match | `{ "field": "edition", "op": "matches", "value": "remux" }` |

Logical operators `and` / `or` nest rules for complex conditions.

## Compositing Pipeline

### Crate Selection

| Crate | Version | Role | Why |
|---|---|---|---|
| `image` | 0.25 | Core image I/O, resizing, `imageops::overlay()` | De facto standard; built-in alpha compositing; PNG/JPEG/WebP |
| `image-overlay` | 0.3 | Advanced blend modes (26+) | Optional; for non-standard blend effects |
| `ab_glyph` | 0.2 | Text rendering with TTF/OTF fonts | Pure Rust; no system font dependency; glyph rasterization |
| `fontdb` | 0.18 | Font discovery and loading | System and bundled font enumeration |
| `resvg` | 0.44 | SVG overlay template rendering | When overlay images are SVG-based; optional |

All crates are pure Rust with no system dependencies — consistent with our Alpine Docker + cross-platform requirements.

### Compositing Steps

```
fn apply_overlays(item: MediaItem, artwork: RgbaImage, definitions: &[OverlayDefinition]) -> RgbaImage {
    let mut canvas = resize_to_standard(artwork);
    
    let applicable = evaluate_conditions(item, definitions);
    let (grouped, queued, standalone) = categorize(applicable);
    
    // 1. Resolve groups — pick highest-weight per group
    let group_winners = resolve_groups(grouped);
    
    // 2. Resolve suppress rules
    let final_overlays = apply_suppress_rules(group_winners, queued, standalone);
    
    // 3. Sort by layer order (backdrops first, then images, then text)
    let sorted = sort_by_layer_order(final_overlays);
    
    // 4. Resolve queue positions
    let positioned = resolve_queue_positions(sorted);
    
    // 5. Composite each overlay
    for overlay in positioned {
        match overlay.overlay_type {
            OverlayType::Image => composite_image(&mut canvas, overlay),
            OverlayType::Text => composite_text(&mut canvas, overlay, item),
            OverlayType::Backdrop => composite_backdrop(&mut canvas, overlay),
        }
    }
    
    canvas
}
```

### Output Format

Composited images are stored as WebP (lossless, supports transparency, smaller than PNG) in the cache directory:

> The WebP output format is the project-wide image format policy — see [IMAGE_FORMATS.md](IMAGE_FORMATS.md). Lossy WebP (q90) is used for photographic posters/backdrops; lossless WebP is used for logos/clearart with alpha. The `overlay_image_format` config field defaults to `webp` and aligns with the unified policy.

```
/cache/images/overlays/
├── posters/{media_item_id}.webp
├── backdrops/{media_item_id}.webp
└── season_posters/{media_item_id}.webp
```

Maximum composited image size: 10 MB (matching Plex's limit). If the result exceeds this, quality is reduced iteratively.

## Clean Art Management

The overlay engine preserves original artwork separately from composited results:

| Artwork State | Location | Purpose |
|---|---|---|
| Source (original from provider/upload) | `artwork.local_path` or `artwork.source_url` | Never modified |
| Clean backup (scaled to canvas) | `/cache/images/clean/{media_item_id}_{type}.webp` | Used as base for re-compositing |
| Overlaid result | `/cache/images/overlays/{media_item_id}_{type}.webp` | Served to clients |

**Re-compositing logic:**

1. On first overlay application: scale source artwork → save clean backup → composite overlays → save result
2. On overlay definition change: load clean backup → re-composite → save new result (no re-download needed)
3. On source artwork change (new TMDb artwork, user upload): scale new source → save clean backup → re-composite → save result

The clean backup ensures overlays can be updated without re-downloading source artwork or losing quality from repeated scale/overlay cycles.

## Database Schema

### Overlay Definitions

```sql
CREATE TABLE overlay_definitions (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    name TEXT NOT NULL,
    slug TEXT NOT NULL,

    library_id UUID REFERENCES libraries(id) ON DELETE CASCADE,

    overlay_type TEXT NOT NULL CHECK (overlay_type IN ('image', 'text', 'backdrop')),

    image_path TEXT,
    text_template TEXT,
    font_family TEXT NOT NULL DEFAULT 'Inter',
    font_size INT NOT NULL DEFAULT 63,
    font_color TEXT NOT NULL DEFAULT '#FFFFFF',
    stroke_color TEXT,
    stroke_width INT DEFAULT 0,

    back_color TEXT,
    back_width INT,
    back_height INT,
    back_radius INT DEFAULT 0,
    back_padding INT DEFAULT 0,

    horizontal_offset INT NOT NULL DEFAULT 0,
    horizontal_align TEXT NOT NULL DEFAULT 'left' CHECK (horizontal_align IN ('left', 'center', 'right')),
    vertical_offset INT NOT NULL DEFAULT 0,
    vertical_align TEXT NOT NULL DEFAULT 'top' CHECK (vertical_align IN ('top', 'center', 'bottom')),

    scale_width INT,
    scale_height INT,

    group_name TEXT,
    weight INT NOT NULL DEFAULT 0,

    queue_name TEXT,

    conditions JSONB NOT NULL DEFAULT '{}',
    suppresses TEXT[] NOT NULL DEFAULT '{}',

    applies_to TEXT NOT NULL DEFAULT 'poster' CHECK (applies_to IN ('poster', 'backdrop', 'season_poster', 'episode_thumb')),

    is_enabled BOOLEAN NOT NULL DEFAULT true,
    is_system BOOLEAN NOT NULL DEFAULT false,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_overlay_definitions_library ON overlay_definitions (library_id) WHERE library_id IS NOT NULL;
CREATE INDEX idx_overlay_definitions_group ON overlay_definitions (group_name) WHERE group_name IS NOT NULL;
CREATE INDEX idx_overlay_definitions_queue ON overlay_definitions (queue_name) WHERE queue_name IS NOT NULL;
CREATE INDEX idx_overlay_definitions_enabled ON overlay_definitions (is_enabled) WHERE is_enabled = true;
```

`library_id` — when null, the overlay applies to all libraries. When set, only items in that library.

`conditions` — JSONB filter rules (see Conditions section). Empty `{}` means "apply to all items."

`suppresses` — array of overlay slugs that are suppressed when this overlay applies.

`is_system` — built-in overlays seeded by the server. Cannot be deleted, but can be disabled or customized.

`metadata` — stores overlay source info, attribution, community template reference, etc.

### Artwork Overlay State

Tracks which overlays have been applied to each media item's artwork. Enables incremental reprocessing — only items whose applicable overlays have changed are re-composited.

```sql
CREATE TABLE artwork_overlay_state (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    artwork_type TEXT NOT NULL CHECK (artwork_type IN ('poster', 'backdrop', 'season_poster', 'episode_thumb')),

    applied_overlay_ids UUID[] NOT NULL DEFAULT '{}',
    overlay_config_hash TEXT NOT NULL,

    clean_art_path TEXT NOT NULL,
    overlaid_art_path TEXT,

    applied_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE(media_item_id, artwork_type)
);

CREATE INDEX idx_artwork_overlay_state_media_item ON artwork_overlay_state (media_item_id);
CREATE INDEX idx_artwork_overlay_state_hash ON artwork_overlay_state (overlay_config_hash);
```

`applied_overlay_ids` — the UUIDs of overlay definitions that were composited. Compared on next run to detect changes.

`overlay_config_hash` — hash of the resolved overlay configuration (IDs + conditions + visual properties). If the hash matches, re-compositing is skipped (performance optimization).

`clean_art_path` — path to the scaled source artwork (clean backup). Used as the base for re-compositing.

`overlaid_art_path` — path to the final composited image in `/cache/images/overlays/`.

### Artwork Table Extension

The existing `artwork` table (see [DATABASE.md](DATABASE.md)) gains one new column:

```sql
ALTER TABLE artwork ADD COLUMN is_locked BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE artwork ADD COLUMN source_type TEXT
    CHECK (source_type IS NULL OR source_type IN ('tmdb', 'user_upload', 'asset_directory', 'community'));
```

`is_locked` — when true, the artwork is user-selected and will not be overwritten by metadata refreshes or TMDb artwork updates. Full design in [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md).

`source_type` — provenance of the artwork. Null for legacy entries.

## Scheduled Tasks

| Task | Schedule | Timeout | Config |
|---|---|---|---|
| Overlay Application | `0 5 * * *` (daily 05:00, after library scan) | 2h | `{ "reapply_all": false, "max_concurrent": 2 }` |
| Overlay Cleanup | `0 6 * * 0` (weekly Sun 06:00) | 30m | `{ "remove_orphaned": true }` |

**Overlay Application** — evaluates all overlay definitions against all media items. Only re-composites items where:
1. No `artwork_overlay_state` row exists (new item)
2. `overlay_config_hash` has changed (overlay definitions modified)
3. Source artwork has changed (new `artwork` row or updated `local_path`)
4. `reapply_all: true` is set in task config (force re-apply all)

**Overlay Cleanup** — removes orphaned overlay images (where the media item has been deleted) and stale clean art backups.

## Built-in Default Overlays

The server seeds these system overlays on first run. They can be disabled or customized but not deleted.

| Name | Type | Position | Conditions | Group |
|---|---|---|---|---|
| Resolution Badge | Image | Bottom-right | Matches resolution from `media_files` | `resolution` |
| Audio Codec Badge | Image | Bottom-left | Matches audio codec from `media_files` | `audio_codec` |
| Content Rating | Image | Top-left | Matches `content_rating` | — |
| Critic Rating | Text | Top-right | `rating_average` exists | `ratings` |
| Dolby Vision | Image | Top-left | `has_dolby_vision` = true | `hdr` |
| HDR10 | Image | Top-left | `video_dynamic_range` = `hdr10` | `hdr` |
| HDR10+ | Image | Top-left | `video_dynamic_range` = `hdr10_plus` | `hdr` |
| 4K HDR | Image | Bottom-right | resolution = 4K AND dynamic_range != SDR | `resolution` |
| Episode Info | Text | Bottom-right | `media_type` = `episode` | — |
| Versions Badge | Image | Bottom-center | `has_multiple_versions` = true | — |
| Streaming | Image | Bottom-left | `streaming_on` exists in metadata | — |

Default overlay images are bundled in `/data/overlays/defaults/` and referenced by `image_path`. The admin can replace these images with custom ones.

## Community Templates

Overlay definitions can be exported and imported as JSON for sharing:

```json
{
  "name": "My Custom Rating Badges",
  "version": 1,
  "overlays": [
    {
      "name": "IMDb Rating",
      "overlay_type": "text",
      "text_template": "text(<<critic_rating>>)",
      "horizontal_align": "right",
      "vertical_align": "top",
      "font_size": 70,
      "back_color": "#00000099",
      "back_radius": 30,
      "conditions": { "operator": "and", "rules": [] }
    }
  ]
}
```

The admin UI includes a template browser for importing community-contributed overlay sets. Templates are validated against a JSON schema before import.

## Admin UI

The overlay management section of the admin UI provides:

1. **Overlay editor** — visual drag-and-drop positioning on a poster preview; live preview of text variables; color picker for backdrop/font/stroke
2. **Condition builder** — dropdown-based condition editor; live count of matching items
3. **Group/queue manager** — drag to reorder weights; visual queue preview
4. **Library assignment** — assign overlay sets to specific libraries or globally
5. **Bulk operations** — "Apply Overlays Now" button; "Reset All Overlays"; "Remove All Overlays"
6. **Template browser** — community-contributed overlay sets with preview and one-click import

## Error Codes

| Code | HTTP | Description |
|---|---|---|
| `OVERLAY_001` | 404 | Overlay definition not found |
| `OVERLAY_002` | 422 | Invalid overlay conditions (malformed JSONB filter) |
| `OVERLAY_003` | 422 | Invalid text template (unresolved variable or syntax error) |
| `OVERLAY_004` | 503 | Overlay image file not found or unreadable |
| `OVERLAY_005` | 409 | Overlay application already in progress |
| `OVERLAY_006` | 500 | Overlay compositing failed (image processing error) |

New domain: **OVERLAY** (6 codes). Total error codes: **94** (88 existing + 6 new).

## Configuration

Overlay settings are stored in `server_config.metadata` JSONB:

```json
{
  "overlays_enabled": true,
  "overlay_apply_schedule": "0 5 * * *",
  "overlay_image_format": "webp",
  "overlay_image_quality": 90,
  "overlay_max_image_size_mb": 10,
  "overlay_default_font": "Inter",
  "overlay_reapply_on_artwork_change": true
}
```

Full `MetadataConfig` Rust struct documented in [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md).

## Cross-References

- [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md) — artwork sourcing, selection, locking; `artwork` table extensions; artwork lifecycle
- [COLLECTIONS.md](COLLECTIONS.md) — collections use custom poster art from templates; overlay application on collection posters
- [DATABASE.md](DATABASE.md) — `overlay_definitions`, `artwork_overlay_state` tables; `artwork` table extensions
- [MEDIA_SCANNING.md](MEDIA_SCANNING.md) — Phase 5 enrichment downloads artwork that feeds the overlay pipeline
- [ERROR_HANDLING.md](ERROR_HANDLING.md) — OVERLAY_001–OVERLAY_006 error codes
- [CONFIGURATION.md](../operations/CONFIGURATION.md) — `server_config.metadata` JSONB; `MetadataConfig` struct
- [CACHE_STORAGE.md](../operations/CACHE_STORAGE.md) — `/cache/images/` storage tier for overlay results and clean art backups
