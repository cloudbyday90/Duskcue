# Multi-Edition Support

## Purpose

This document is the authoritative design for multi-edition support in Duskcue — the ability to own and choose between multiple versions of the same movie or TV episode (theatrical cut, director's cut, extended edition, uncut, special edition, etc.).

This is distinct from "quality variants" (4K vs 1080p of the same cut), which are handled by the existing `media_files` multi-file mechanism and the streaming decision engine in [STREAMING.md](STREAMING.md).

## Definitions

| Term | Definition |
|---|---|
| **Edition** | A distinct release of a movie or episode with different content. Examples: Theatrical, Director's Cut, Extended Edition, Uncut, Final Cut, Special Edition, Unrated. |
| **Quality Variant** | The same edition encoded at different resolutions, codecs, or bitrates. Examples: 4K HEVC, 1080p H.264, 720p. These are NOT editions. |
| **Default Edition** | The edition with no explicit label in the filename. Typically the theatrical/original release. Always exists. |
| **Split File** | A single edition split across multiple files (pt1, pt2, cd1, cd2). These are parts of one edition, not separate editions. |

## Conceptual Model

```
media_item (Blade Runner)
├── edition: Default (Theatrical)
│   ├── media_file: Blade Runner (1982).mkv          (1080p H.264)
│   └── media_file: Blade Runner (1982) - 4K.mkv      (4K HEVC)
├── edition: Director's Cut
│   └── media_file: Blade Runner (1982) - Directors Cut.mkv
└── edition: Final Cut
    ├── media_file: Blade Runner (1982) - Final Cut - 1080p.mkv
    └── media_file: Blade Runner (1982) - Final Cut - 4K.mkv
```

Each `media_item` has one or more **editions**. Each edition has one or more **quality variants** (media_files). The streaming engine selects the best quality variant within the chosen edition.

## File Naming Convention

Editions are identified by the text following ` - ` (space-dash-space) after the base filename, as defined in [LIBRARY_ORGANIZATION.md](LIBRARY_ORGANIZATION.md).

### Edition labels

```
Blade Runner (1982)/
├── Blade Runner (1982).mkv                          → Default edition
├── Blade Runner (1982) - Directors Cut.mkv          → "Directors Cut" edition
├── Blade Runner (1982) - Final Cut.mkv              → "Final Cut" edition
└── Blade Runner (1982) - Theatrical.mkv             → "Theatrical" edition
```

The default edition has no label. All other editions have their name after ` - `.

### Edition + quality variant

When an edition has multiple quality variants, the edition label appears first, followed by any distinguishing text:

```
Blade Runner (1982)/
├── Blade Runner (1982).mkv                          → Default, quality 1
├── Blade Runner (1982) - 4K.mkv                     → Default, quality 2 (4K)
├── Blade Runner (1982) - Directors Cut - 1080p.mkv  → "Directors Cut", quality 1
└── Blade Runner (1982) - Directors Cut - 4K.mkv     → "Directors Cut", quality 2 (4K)
```

### Edition + split file

Split files within an edition use the standard split markers after the edition name:

```
Lord of the Rings - The Return of the King (2003)/
├── Lord of the Rings - The Return of the King (2003) - Extended Edition - pt1.mkv
└── Lord of the Rings - The Return of the King (2003) - Extended Edition - pt2.mkv
```

### Disambiguation rules

When parsing filenames, the scanner uses this priority to distinguish edition labels from quality labels and split markers:

1. **Split markers** (`pt1`, `cd1`, `disc1`, `part1`, etc.) are extracted first
2. **Known quality tokens** (`1080p`, `4K`, `720p`, `HEVC`, `H.264`, etc.) are classified as quality hints, not edition names
3. **Remaining text** after removing split markers and quality tokens is the edition name

Known split markers: `pt`, `cd`, `disc`, `disk`, `dvd`, `part` (followed by a number or letter a-d).

Known quality tokens (non-exhaustive, case-insensitive): resolution patterns (`1080p`, `2160p`, `720p`, `480p`, `4k`, `uhd`, `hd`, `sd`, `imax`), codec patterns (`hevc`, `h264`, `h265`, `av1`, `vp9`, `x264`, `x265`), source patterns (`bluray`, `webrip`, `webdl`, `hdtv`, `dvdrip`, `remux`).

If the text after ` - ` consists entirely of quality tokens and/or split markers, it is NOT an edition — it's a quality variant of the default edition.

## Database Schema

### `media_files` changes

Add `edition_name` column to the existing `media_files` table:

```sql
ALTER TABLE media_files ADD COLUMN edition_name TEXT;
```

| Column | Type | Description |
|---|---|---|
| `edition_name` | `TEXT` (nullable) | The parsed edition name. `NULL` = default edition. Populated during scan from filename. |

### `movies` table expansion

The `movies` table gains edition-relevant metadata:

```sql
ALTER TABLE movies ADD COLUMN edition_count INT NOT NULL DEFAULT 1;
ALTER TABLE movies ADD COLUMN default_edition_runtime_seconds INT;
```

| Column | Type | Description |
|---|---|---|
| `edition_count` | `INT` | Number of distinct editions for this movie. Maintained by scanner. 1 = single edition (default only). |
| `default_edition_runtime_seconds` | `INT` | Runtime of the default (unlabeled) edition. Used when displaying runtime before all files are scanned. |

### `episodes` table expansion

For TV episode editions (broadcast vs uncut):

```sql
ALTER TABLE episodes ADD COLUMN edition_count INT NOT NULL DEFAULT 1;
```

### No new tables

Editions are not modeled as a separate table. They are an implicit grouping of `media_files` rows sharing the same `(media_item_id, edition_name)` tuple. This keeps the schema simple and avoids join complexity for the common case (most movies have one edition).

### Index

```sql
CREATE INDEX idx_media_files_edition ON media_files (media_item_id, edition_name);
```

## Scanner Behavior

The library scanner (see [MEDIA_SCANNING.md](MEDIA_SCANNING.md)) is enhanced in Phase 2 (Diff) and Phase 3 (Probe) to:

1. **Parse edition name** from filename using the disambiguation rules above
2. **Write `edition_name`** to the `media_files` row during insert
3. **Update `edition_count`** on `movies`/`episodes` after scan completes:

```sql
SELECT COUNT(DISTINCT COALESCE(edition_name, '')) FROM media_files WHERE media_item_id = $1;
```

4. **Handle edition name changes** — if a file is renamed from `Movie.mkv` to `Movie - Directors Cut.mkv`, the diff detects the mtime change, re-parses the edition name, and updates the row

### `.media-match` edition directive

The existing `edition:` directive in `.media-match` files (see [LIBRARY_ORGANIZATION.md](LIBRARY_ORGANIZATION.md)) overrides the filename-parsed edition name:

```
# .media-match
edition: Directors Cut
```

When present, all video files in this directory/folder receive this edition name regardless of their filename.

## Streaming Integration

### Edition selection

When a user plays a media item with multiple editions:

1. **Check for remembered edition** — `user_item_data.last_played_media_file_id` references a specific `media_file`, which has an `edition_name`. Resume within the same edition.
2. **Default edition** — If no remembered file, use the default edition (`edition_name IS NULL`).
3. **Edition picker** — If the user explicitly chooses an edition, play from that edition's files.

### Quality variant selection (unchanged)

Within the selected edition, the streaming decision engine (see [STREAMING.md](STREAMING.md)) selects the best `media_file` based on client capabilities, exactly as it does today. The only change is scoping the candidate set:

```sql
-- Before: all files for the media item
SELECT * FROM media_files WHERE media_item_id = $1;

-- After: files for the selected edition
SELECT * FROM media_files WHERE media_item_id = $1 AND edition_name IS NOT DISTINCT FROM $2;
```

`IS NOT DISTINCT FROM` handles the NULL case (default edition has `edition_name = NULL`).

## Watch State

### Per-edition tracking

Watch state is tracked at the `media_file` level via the existing `user_item_data.last_played_media_file_id` column. This naturally provides per-edition tracking:

- Watching the "Director's Cut" records that file's ID
- Watching the "Theatrical" records that file's ID
- The "continue watching" row shows the last-played edition

### Progress within edition

Resume position is per `media_file`, so switching editions starts from the beginning of the new edition. This is the expected behavior — a user choosing a different cut expects to start fresh.

### Watched status

A media item is considered "watched" if the user has watched any edition to completion. The `user_item_data.is_watched` flag applies to the `media_item`, not a specific edition. If finer-grained per-edition watched status is needed, it can be derived from play session history.

## API Endpoints

### List editions for a media item

```
GET /api/v1/media/{id}/editions
```

Response:

```json
{
  "items": [
    {
      "name": null,
      "display_name": "Theatrical",
      "is_default": true,
      "runtime_seconds": 7020,
      "file_count": 2,
      "files": [
        {
          "id": "uuid",
          "resolution": "1080p",
          "codec": "h264",
          "file_size": 8589934592,
          "runtime_seconds": 7020
        },
        {
          "id": "uuid",
          "resolution": "2160p",
          "codec": "hevc",
          "file_size": 21474836480,
          "runtime_seconds": 7020
        }
      ]
    },
    {
      "name": "Directors Cut",
      "display_name": "Director's Cut",
      "is_default": false,
      "runtime_seconds": 7380,
      "file_count": 1,
      "files": [...]
    }
  ]
}
```

### Play with edition selection

```
POST /api/v1/media/{id}/play
```

Request (optional edition selection):

```json
{
  "edition_name": "Directors Cut"
}
```

If `edition_name` is omitted or `null`, the server uses the remembered edition (from `last_played_media_file_id`) or the default edition.

## User Interface

### Library grid

Multi-edition movies appear as a **single entry** in the library grid — no clutter. The poster, title, and year come from the `media_item`, not individual editions.

### Detail page edition picker

When a movie has `edition_count > 1`, the detail page shows an edition selector:

```
┌─────────────────────────────────────────┐
│  Blade Runner (1982)                    │
│                                         │
│  ▶ Play   ⬇ Download                   │
│                                         │
│  Edition: [Theatrical ▾]               │
│           ┌─────────────────┐           │
│           │ Theatrical      │  ← default│
│           │ Director's Cut  │           │
│           │ Final Cut       │           │
│           └─────────────────┘           │
│                                         │
│  Runtime: 1h 57m                        │
│  Overview: ...                          │
└─────────────────────────────────────────┘
```

Selecting an edition updates the runtime, any edition-specific metadata, and changes which file will be played.

### Edition badge on poster (optional)

Overlays (see [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md)) can use the `<<edition>>` template variable to display the edition name on posters when multiple editions exist. This helps distinguish editions in collection views.

### Player

The player does not need edition awareness — it receives a specific `media_file` from the server. The edition selection happens at the detail/play-request level.

## TV Show Episodes

### Scope

TV episode editions are scoped to individual episodes, not series or seasons. Common scenarios:

| Scenario | Example |
|---|---|
| Broadcast vs uncut | Anime episodes with broadcast edits vs Blu-ray uncut versions |
| Extended episodes | "The Extended Episodes" of a series |
| TV vs movie versions | Serenity (Firefly) as both an episode compilation and a standalone movie (these would be separate `media_items`, not editions) |

### Naming

```
Star Wars - Clone Wars (2003)/
└── Season 1/
    ├── Star Wars - Clone Wars (2003) S01E01.mkv              → Default (broadcast)
    ├── Star Wars - Clone Wars (2003) S01E01 - Uncut.mkv      → "Uncut" edition
    └── Star Wars - Clone Wars (2003) S01E02.mkv
```

### Episode edition count

The `episodes.edition_count` column tracks how many editions exist for each episode. The UI shows an edition picker on the episode detail page when `edition_count > 1`.

### Limitations

- TV show editions do NOT support different episode counts per edition (e.g., a "season" that has 13 episodes in one cut and 24 in another). Those are fundamentally different releases and should be separate `media_items`.
- Edition naming at the episode level follows the same conventions as movies.

## Metadata Providers

### TMDB

TMDB does not have a formal "edition" concept for movies. The TMDB movie entry represents the primary release. Duskcue maps all editions of a movie to the same TMDB ID on the `media_items` table.

Edition-specific metadata (different runtime, different rating for an unrated cut) is stored locally and may differ from TMDB's values. The `media_items.runtime_seconds` stores the default edition's runtime; edition-specific runtimes come from the `media_files.runtime_seconds` probe.

### Artwork

Each edition can have its own primary artwork, stored in the `artwork` table with a reference to the edition:

```sql
ALTER TABLE artwork ADD COLUMN edition_name TEXT;
```

When `artwork.edition_name` is set, that artwork is only displayed when viewing that edition. When `NULL`, the artwork is shared across all editions (the default behavior).

This enables:
- Director's Cut with its own poster
- Extended Edition with its own backdrop
- Default edition using the standard TMDB poster

## Overlays Integration

The existing `<<edition>>` template variable in [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md) is enhanced:

- When a movie has one edition: `<<edition>>` resolves to empty string (no overlay shown)
- When a movie has multiple editions: `<<edition>>` resolves to the current edition's display name
- Overlay conditions can filter by edition: `{ "field": "edition", "op": "matches", "value": "extended" }`

This is used for edition-specific poster badges (e.g., "EXTENDED" text overlay, "DIRECTOR'S CUT" ribbon).

## Configuration

No server-level configuration is needed for edition support. Editions are automatically detected from file naming during library scans.

Per-user settings that interact with editions:

| Setting | Behavior |
|---|---|
| Default edition preference | "Always play default edition" or "Always play the last-watched edition". Default: last-watched. |
| Show edition badge | Whether to display edition name on posters when multiple editions exist. Default: true. |

These are stored in `user_item_data` metadata JSONB and the runtime config, respectively.

## Edge Cases

### Edition with only split files

An edition that consists entirely of split files (pt1 + pt2) is still one edition. The `edition_name` is the same on both `media_files` rows. The player concatenates them.

### Files without edition labels are the default

If all files in a movie folder lack edition labels, there is one edition (the default) with N quality variants. This is the most common case.

### Identical files with different edition names

If the scanner detects two files with different edition names but identical content (same Blake3 hash, same runtime, same resolution), it logs a warning. Both files are kept — the user may have intentionally named them differently.

### Changing edition names

Renaming a file changes its edition. The scanner detects the filename change via mtime diff and updates `edition_name`. The old edition's watch state is preserved (it's keyed to `media_file_id`, not `edition_name`). If the renamed file's old `media_file_id` was referenced in `user_item_data.last_played_media_file_id`, the user will be prompted to select an edition on next play (since their remembered file now has a different edition name).

### Maximum editions

Per [LIBRARY_ORGANIZATION.md](LIBRARY_ORGANIZATION.md), up to 8 editions per movie. This is a soft limit enforced by the scanner — a 9th distinct edition name logs a warning but is still accepted.

## Implementation Phases

### Phase 5 (Libraries & Media Items)

- Scanner parses edition names from filenames
- `edition_name` column populated on `media_files` inserts
- `edition_count` maintained on `movies` and `episodes`

### Phase 7 (Streaming & Playback)

- Play request accepts optional `edition_name` parameter
- Streaming decision engine scopes file selection to chosen edition
- Resume playback respects `last_played_media_file_id` edition

### Phase 8 (Web Client)

- Edition picker on movie/episode detail page
- Edition badge overlay on posters
- Edition display in "Other Editions" section

### Phase 12 (Kometa-Like System)

- Edition-aware overlay conditions
- `<<edition>>` template variable in text overlays
- Edition-specific artwork selection

## Relationship to Other Documents

| Document | Relationship |
|---|---|
| [LIBRARY_ORGANIZATION.md](LIBRARY_ORGANIZATION.md) | Defines file naming conventions for editions. This document references those conventions and adds scanner/database behavior. |
| [DATABASE.md](DATABASE.md) | Defines `media_files`, `movies`, `episodes` table schemas. This document specifies additions to those tables. |
| [MEDIA_SCANNING.md](MEDIA_SCANNING.md) | Defines the 6-phase scanning pipeline. Edition parsing occurs in Phase 2 (Diff) and is persisted in Phase 3 (Probe). |
| [STREAMING.md](STREAMING.md) | Defines the streaming decision engine. This document adds edition-scoping to file selection. |
| [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md) | Defines overlay template variables including `<<edition>>`. This document enhances that variable's behavior for multi-edition items. |
| [STORYBOARDS.md](STORYBOARDS.md) | Storyboards are per `media_file_id`, which naturally supports per-edition sprite sheets. |
| [SUBTITLES.md](SUBTITLES.md) | Different editions may cause subtitle sync drift. Subtitle alignment handles this at the file level. |
