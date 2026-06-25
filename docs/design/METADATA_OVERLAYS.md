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
| `ab_glyph` | 0.2 | Text rendering with TTF/OTF fonts | Pure Rust; no system font dependency; glyph rasterization |
| `resvg` | 0.47 | SVG overlay template rendering | When overlay images are SVG-based; re-exports `usvg` + `tiny-skia`; pure Rust |

All crates are pure Rust with no system dependencies — consistent with our Alpine Docker + cross-platform requirements.

**`image-overlay` and `fontdb` dropped from the original table** — `image-overlay`'s advanced blend modes (multiply, screen, overlay, etc.) are not needed; the `image` crate's built-in `imageops::overlay()` source-over alpha compositing covers all documented overlay use cases. `fontdb`'s CSS-like family/weight/style queries are overkill for a self-hosted server with bundled fonts; the compositing service resolves `font_family` by matching the filename stem in `/data/fonts/` (e.g., `Inter.ttf` ↔ `font_family: "Inter"`), falling back to the first available font. This avoids version-matching with `usvg`'s transitive `fontdb` dependency while keeping font resolution simple and predictable. See Task 2 Implementation Notes for details.

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

## Implementation Notes

### Phase 12 Task 1 — Domain Scaffolding (Complete)

Created `server/src/domains/overlays/` following the project's domain five-file pattern (`mod.rs`, `error.rs`, `types.rs`, `service.rs`, `handlers.rs`). This is the scaffolding task — all service and handler bodies are `todo!()` stubs with concrete return types so the project compiles and routes are wired. The compositing pipeline (Task 2), condition evaluation (Task 3), and clean-art preservation (Task 4) replace the stubs in subsequent tasks.

**Route design** (base `/api/v1/overlays` per API_CONVENTIONS.md):

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/v1/overlays` | List overlay definitions (optional `library_id`, `enabled` filters) |
| POST | `/api/v1/overlays` | Create overlay definition |
| GET | `/api/v1/overlays/{id}` | Get overlay definition |
| PATCH | `/api/v1/overlays/{id}` | Update overlay definition (partial) |
| DELETE | `/api/v1/overlays/{id}` | Delete overlay definition |
| POST | `/api/v1/overlays/apply` | Trigger bulk overlay application (Task 8 worker integration) |
| POST | `/api/v1/overlays/preview` | Render a preview composite for the editor (Task 2 compositing) |
| GET | `/api/v1/overlays/templates` | List community templates |
| POST | `/api/v1/overlays/templates` | Import a community template (JSON) |

**Capability gate:** All overlay endpoints require `CanManageLibraries` (artwork customization is a library-management function, matching the libraries domain gate). Enforced via the generic `Require<CanManageLibraries>` extractor rather than inline `check_capability()` calls, consistent with the Phase 4 Task 11 extractor pattern.

**Error mapping** — `OverlayError` enum with exactly the 6 registered OVERLAY codes plus the `Database` catch-all (no invented codes, respecting the fixed registry of 94 total codes):

| Variant | Code | HTTP |
|---|---|---|
| `NotFound` | OVERLAY_001 | 404 |
| `InvalidConditions(String)` | OVERLAY_002 | 422 |
| `InvalidTextTemplate(String)` | OVERLAY_003 | 422 |
| `ImageFileNotFound(String)` | OVERLAY_004 | 503 |
| `ApplicationInProgress` | OVERLAY_005 | 409 |
| `CompositingFailed(String)` | OVERLAY_006 | 500 |
| `Database(#[from] sqlx::Error)` | INTERNAL | 500 |

The domain error converts to `AppError` via `#[from]` and maps in `overlay_error_to_http()` in `server/src/error.rs`, following the established per-domain mapping convention. System-overlay deletion protection (system overlays can be disabled but not deleted) is enforced in the CRUD implementation (later task) via `AppError::Conflict`, since no dedicated OVERLAY code exists for that business rule — consistent with how other domains reuse generic codes for policy violations.

**DTO design** — three-type pattern per API_SECURITY.md:
- `OverlayDefinitionRow` — internal row struct (no `Serialize`); mirrors all 30 columns of `overlay_definitions`
- `CreateOverlayRequest` / `UpdateOverlayRequest` — `Deserialize + Validate`; update uses all-`Option` fields for PATCH partial-update semantics
- `OverlayDefinitionResponse` / `OverlayListResponse` — `Serialize` only
- `ApplyOverlaysRequest`, `PreviewOverlayRequest`, `OverlayTemplateImport`, `OverlayTemplateResponse` — operation-specific DTOs
- Validation statics: `VALID_OVERLAY_TYPES` (`image`/`text`/`backdrop`), `VALID_APPLIES_TO` (`poster`/`backdrop`/`season_poster`/`episode_thumb`), `VALID_HORIZONTAL_ALIGN`, `VALID_VERTICAL_ALIGN` — sourced from the `CHECK` constraints in the DDL

**No new DB migration** — `overlay_definitions` and `artwork_overlay_state` tables (plus `artwork.is_locked`/`source_type` columns) were created in Phase 2 migration 14 (`20260530_070400_create_overlays_collections.sql`).

**No new workspace dependencies** — the domain scaffolding uses existing `axum`, `sqlx`, `serde`, `validator`, `uuid`, `chrono`. The compositing crates (`ab_glyph`, `fontdb`, `resvg`) are added in Task 2.

### Phase 12 Task 2 — Compositing Service (Complete)

Built `server/src/services/overlays.rs` as a stateless shared-service module (not a domain module), following the same convention as `services/image_pipeline.rs`, `services/segments.rs`, `services/storyboards.rs`, and `services/decision_engine.rs`. Pure library functions take typed inputs and return `RgbaImage` bytes; the domain layer and future `overlay_compositor` worker own disk I/O and DB state.

**Module location** — `services/overlays.rs` is a cross-cutting library consumed by the overlay domain (preview endpoint) and the future `overlay_compositor` scheduled worker (Task 8). It has no DB, no `AppState`, no HTTP coupling — fully unit-testable without a database. The domain `service.rs::preview_overlay` is the orchestration point that loads artwork bytes, loads overlay definitions, constructs the compositing inputs, calls `services::overlays::composite()`, and persists/returns the result.

**Crate additions (2, not 3):**

| Crate | Version | Notes |
|---|---|---|
| `ab_glyph` | `0.2` | Text glyph rasterization — `FontRef::try_from_slice()`, `outline_glyph().draw()` |
| `resvg` | `0.47` | SVG overlay rendering — re-exports `usvg` 0.47 + `tiny-skia` 0.12; `render(&tree, transform, &mut pixmap)` |

`fontdb` and `image-overlay` (listed in the original crate table) were dropped — see the updated Crate Selection section above for rationale.

**Compositing pipeline (`composite()` entry point):**

1. **Resolve fonts** — `FontRegistry::scan_dir()` enumerates `.ttf`/`.otf`/`.ttc` files in `/data/fonts/`, indexed by lowercased filename stem (without extension). `resolve(family)` returns the matching `FontArc`, falling back to the first scanned font, then a compiled-in minimal bitmap fallback so the pipeline never panics on a missing font.
2. **Scale source to standard canvas** — `resize_to_canvas()` scales the source artwork to the standard dimensions for the artwork type (poster 1000×1500, backdrop/episode_thumb 1920×1080) using Lanczos3, matching the Canvas Standards table. No upscaling.
3. **Resolve overlays** — caller passes already-resolved overlays (group winners + suppress-filtered + queue-positioned). The service itself is resolution-agnostic: it composites whatever `ResolvedOverlay` list it receives. Group/suppress/queue resolution lives in the domain service layer (which has DB access to load definitions and media-item context to evaluate conditions). The compositing service provides pure helpers (`resolve_groups()`, `apply_suppress_rules()`, `resolve_queue_positions()`) that the domain layer calls.
4. **Sort by layer order** — backdrops first (bottom), then images, then text (top), matching the design's `sort_by_layer_order()` step.
5. **Composite each overlay** onto the mutable canvas:
   - **Backdrop** — `fill_rounded_rect()` draws a solid/semi-transparent rounded rectangle via `imageops::fill()` on a sub-region. `back_radius` rounds corners; `back_padding` insets from the auto-sized text bounds.
   - **Image** — loads PNG/SVG bytes via `load_image_asset()`. PNG decodes via the `image` crate directly to `RgbaImage`. SVG renders via `resvg` to a `tiny_skia::Pixmap`, then converts to `RgbaImage` via `pixmap_to_rgba()` (un-premultiplies alpha). `imageops::overlay()` alpha-blends at the resolved position.
   - **Text** — `render_text()` lays out glyphs via `ab_glyph` (`Layout` single-line), rasterizes each glyph's outline with `.draw(|x, y, c| …)`, applies stroke via a second rasterization pass offset in 8 directions when `stroke_width > 0`, and composites the resulting glyph buffer with `imageops::overlay()`. Template variables (`<<title>>`, `<<resolution>>`, etc.) are resolved by the domain layer before reaching the service — the service receives a fully-resolved `text` string.
6. **Encode result** — `image::DynamicImage::ImageRgba8(canvas).to_rgba8()` returned to the caller, which persists it as WebP via the existing `services::image_pipeline`.

**Positioning math** — `compute_position()` translates `(horizontal_align, horizontal_offset, vertical_align, vertical_offset)` + the overlay's natural dimensions into absolute `(x, y)` top-left pixel coordinates on the canvas:
- `align: left` → `x = offset`; `align: center` → `x = (canvas_w - overlay_w) / 2 + offset`; `align: right` → `x = canvas_w - overlay_w - offset`
- Same for vertical with `top`/`center`/`bottom`
- All values clamped to `[0, canvas_dim]` so off-canvas offsets don't panic `imageops::overlay()`

**Queue auto-stacking** — `resolve_queue_positions()` takes a list of overlays sharing the same `queue_name`, sorts by `weight` descending, and assigns sequential positions. Vertical queues stack top-to-bottom; horizontal queues stack left-to-right. The first overlay uses its declared offset; subsequent overlays offset by the previous overlay's height/width + `queue_spacing` (default 8px). The resolved position overrides the declared `vertical_offset`/`horizontal_offset`.

**Group resolution** — `resolve_groups()` takes all applicable overlays, partitions by `group_name` (overlays with no group are standalone), and within each group selects only the highest-`weight` overlay. Returns the flat list of winners + standalones.

**Suppress rules** — `apply_suppress_rules()` removes any overlay whose slug appears in the `suppresses` list of any surviving overlay. Applied after group resolution.

**tiny-skia ↔ image bridge** — `resvg` renders onto `tiny_skia::Pixmap` (premultiplied alpha, `&mut [u8]` RGBA). `pixmap_to_rgba()` converts to `image::RgbaBuffer` (non-premultiplied) by iterating pixels: `a = p[3]; if a > 0 { r = p[0]*255/a; g = p[1]*255/a; b = p[2]*255/a }`. Fully-transparent pixels (`a == 0`) become `(0,0,0,0)`. This bridge is the standard interop pattern between tiny-skia and the `image` ecosystem.

**Text variable resolution** — the compositing service does NOT resolve `<<variable>>` tokens. The domain layer (`domains::overlays::service`) is responsible for substituting variables from media-item context before passing the `text` string to the compositing service. This keeps the compositing service free of DB/sqlx dependencies and makes it fully testable with static text. The variable resolver helper `resolve_text_variables()` lives in the domain service layer (Task 3, condition evaluation) where it has access to `media_items`/`media_files` data.

**Font fallback strategy** — if `resolve(family)` finds no matching font file, the service falls back to the first font in the registry. If the registry is empty (no fonts in `/data/fonts/`), the service returns `OverlayPipelineError::NoFontAvailable` rather than panicking. A future enhancement can bundle a default font at compile time via `include_bytes!` for guaranteed availability, but for now the operator must place at least one font file in `/data/fonts/`.

**Output format** — the service returns raw `RgbaImage` bytes. The caller (domain layer) encodes to WebP via the existing `services::image_pipeline::encode_webp()` or `image_pipeline::generate_variant()`, ensuring consistent output format policy across all image-producing services. The service does not write to disk directly.

**Preview endpoint wiring** — `domains::overlays::service::preview_overlay` now calls the compositing service: loads the media item's primary artwork bytes from the `artwork` table / disk, loads the requested overlay definitions (by `overlay_ids` or all enabled for the artwork type), resolves groups/suppress/queues, resolves text variables from media-item context, calls `services::overlays::composite()`, encodes the result to WebP, writes to the cache preview directory, and returns a `PreviewOverlayResponse` with a URL the client can fetch. The preview is a one-off render (not persisted in `artwork_overlay_state` — that's the worker's job in Task 8).

**Error model** — `OverlayPipelineError` enum with variants: `Decode`, `Encode`, `FontLoad`, `NoFontAvailable`, `SvgParse`, `InvalidColor`, `Io`. Separate from the domain `OverlayError` (which surfaces API-facing OVERLAY_001–006 codes). The worker/domain layer translates `OverlayPipelineError` to `OverlayError::CompositingFailed` for API responses, matching the `segments`/`storyboards` precedent of separate pipeline vs domain error types.

### Phase 12 Task 3 — Condition Evaluation (Complete)

Built `server/src/services/conditions.rs` as a pure, stateless condition evaluation engine — no DB, no `AppState`, no async. Takes a condition JSONB `Value` + a typed [`MediaFilterContext`] struct, returns `bool`. Shared between overlay definitions (this document §Conditions) and smart collections/playlists (COLLECTIONS.md §Smart Filter Syntax).

**Module location** — `services/conditions.rs` follows the established cross-cutting service convention (`decision_engine.rs`, `segments.rs`, `storyboards.rs`). The condition system is shared between overlays and collections — placing it in `services/` rather than `domains/overlays/` avoids coupling collections (Phase 12 Task 5+) to the overlay domain module.

**No external JSON rule engine crate** — Research (June 2026) evaluated `datalogic-rs` and `json-eval-rs` (JSONLogic implementations). Rejected because: (1) JSONLogic uses a different schema (`{"==": [...]}`) than Duskcue's documented schema (`{"operator": "and", "rules": [...]}`), requiring a translation layer; (2) existing crates are heavy form-validation engines with WASM/C#/React Native bindings — overkill for media filtering; (3) Duskcue's condition schema is simple enough (8 operators, 16 fields, nested AND/OR) for a hand-written recursive evaluator with zero new dependencies. The `regex` crate (already in workspace) handles the `matches` operator.

**Recursive evaluator** — `evaluate_group()` handles `{operator, rules}` objects; `evaluate_node()` dispatches between nested groups (have `operator` key) and leaf rules (have `field` key); `evaluate_leaf()` dispatches on the `op` string. Recursion confirmed as the recommended Rust pattern for JSON rule evaluation (per `datalogic-rs` author benchmarks). Nesting depth is bounded by the JSONB structure size (admin-authored, typically ≤3 levels).

**Condition semantics** — per the Conditions section above:

| Operator | Behavior |
|---|---|
| `eq` | Case-insensitive text equality; numeric equality; boolean equality; array-membership for `genre`/`streaming_on` |
| `neq` | Negation of `eq` |
| `in` | Case-insensitive membership in `values` array; array-membership for `genre`/`streaming_on` |
| `gt`/`gte`/`lt`/`lte` | Numeric comparison on `critic_rating`/`critic_rating_above`/`audio_channels` |
| `exists` | Field presence check: `value: true` → field must be present; `value: false` → field must be absent/null |
| `matches` | Regex match via the `regex` crate on text fields; invalid regex → no match (warning logged) |

**Case-insensitivity** — All text comparisons use `eq_ignore_ascii_case`. Admin-facing values like `"4k"` match DB-stored `"4K"`. The `matches` operator uses standard regex; admins add `(?i)` for case-insensitive regex.

**Malformed conditions** — At evaluation time, a malformed rule (missing `field`/`op` key, unknown field name, unknown operator) logs a warning and returns `false` (overlay not applied). The `validate_structure()` function provides structural validation for the create/update API path, returning `ConditionError` for malformed conditions to surface as `OVERLAY_002`.

**`MediaFilterContext` struct** — Carries all 16 condition-testable fields plus derived booleans:
- Text fields from `media_items`: `media_type`, `content_rating`
- Text fields from `media_files`: `video_resolution`, `video_codec`, `video_dynamic_range`, `audio_codec`, `container_format`
- Numeric fields: `critic_rating` (from `media_items.rating_average`), `audio_channels` (from `media_files`)
- UUID: `library_id` (from `media_items`)
- Array: `genres` (from `genres` via `media_genres`), `streaming_on` (from `media_items.metadata` JSONB)
- Boolean: `has_dolby_vision` (derived: `video_dynamic_range LIKE 'dolby_vision%'`), `has_multiple_versions` (derived: `COUNT(media_files) > 1`)
- From `media_items.metadata` JSONB: `original_language`, `edition`

**Domain integration** — `domains/overlays/service.rs::preview_overlay()` now filters overlay definitions by their `conditions` JSONB before group/suppress/queue resolution:
1. Load all enabled overlay definitions for the artwork type
2. Load `OverlayMediaContext` from DB (single query with `LEFT JOIN LATERAL` for primary media file + file count + genre aggregation)
3. Convert to `MediaFilterContext` via `to_filter_context()`
4. Filter definitions: `definitions.filter(|d| conditions::evaluate(&d.conditions, &filter_ctx))`
5. Convert surviving definitions to `ResolvedOverlay`s and proceed with compositing

**`OverlayMediaContext` vs `MediaFilterContext`** — The domain layer uses a richer `OverlayMediaContext` struct that includes both condition-testable fields and text-variable fields (`title`, `year`, `runtime_seconds`, `audience_rating`, `rating_vote_count`). `to_filter_context()` extracts the condition-relevant subset. This avoids a second DB query for text variable resolution while keeping `MediaFilterContext` focused on condition-testable fields.

**Expanded text variable resolution** — Task 2's `resolve_text_variables()` handled 6 variables. Task 3 expands to 16 variables per the Special Text Variables table: `<<title>>`, `<<year>>`, `<<resolution>>`, `<<video_codec>>`, `<<audio_codec>>`, `<<critic_rating>>`, `<<critic_rating/>>` (÷2 for /5 scale), `<<audience_rating>>`, `<<rating_vote_count>>`, `<<video_dynamic_range>>`, `<<container>>`, `<<audio_channels>>` (formatted as "5.1", "7.1", etc.), `<<content_rating>>`, `<<runtime>>`/`<<runtimeH>>`/`<<runtimeM>>`, `<<edition>>`.

**DB query** — `load_media_context()` uses a single query with three `LEFT JOIN LATERAL` subqueries: (1) primary media file (healthiest, largest), (2) healthy file count for `has_multiple_versions`, (3) genre aggregation via `media_genres` + `genres`. This replaces Task 2's three separate correlated subqueries with a more efficient join-based approach. Metadata JSONB fields (`original_language`, `streaming_on`, `edition`, `audience_rating`) are extracted from the `media_items.metadata` JSONB column via `serde_json::Value::get()`.

**64 unit tests** covering: empty/null/bool conditions, all 8 operators (eq, neq, in, gt/gte/lt/lte, exists, matches), case-insensitive text comparison, numeric comparison (integer and float, string-parsed numbers), boolean field equality, array-membership fields (genre, streaming_on), UUID field, nested AND/OR groups (including 3-level nesting), empty rules, malformed conditions (missing keys, unknown fields/operators), structural validation (valid/invalid structures, missing keys, invalid operators, `in` requires `values`), default context (all fields empty).

## Cross-References

- [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md) — artwork sourcing, selection, locking; `artwork` table extensions; artwork lifecycle
- [COLLECTIONS.md](COLLECTIONS.md) — collections use custom poster art from templates; overlay application on collection posters
- [DATABASE.md](DATABASE.md) — `overlay_definitions`, `artwork_overlay_state` tables; `artwork` table extensions
- [MEDIA_SCANNING.md](MEDIA_SCANNING.md) — Phase 5 enrichment downloads artwork that feeds the overlay pipeline
- [ERROR_HANDLING.md](ERROR_HANDLING.md) — OVERLAY_001–OVERLAY_006 error codes
- [CONFIGURATION.md](../operations/CONFIGURATION.md) — `server_config.metadata` JSONB; `MetadataConfig` struct
- [CACHE_STORAGE.md](../operations/CACHE_STORAGE.md) — `/cache/images/` storage tier for overlay results and clean art backups
