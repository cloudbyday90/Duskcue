# Library Organization

Authoritative document for the library folder structure, file naming conventions, and sub-folder design. Defines how media is organized on disk, how the scanner traverses directories, and how libraries map to filesystem paths.

## Design Principles

1. **Each sub-folder is a library** — a `libraries` row with its own `root_path`, `media_type`, scan schedule, metadata settings, and access control; the parent container (`TV Shows/`, `Movies/`) is a filesystem convention only, not a server entity
2. **Library names are agnostic** — the server assigns no semantic meaning to library names; "Kids TV", "Documentaries", "Family Films", "Stand-Up" are user-chosen labels
3. **Per-item folders recommended** — each movie and each TV series lives in its own named folder; flat files supported as fallback
4. **Metadata-driven categorization** — genres, ratings, franchises, and content types are metadata concerns, not folder concerns; the UI provides smart collections for browsing
5. **Separate libraries for access control** — if users need restricted content (e.g. kids-only), that category is its own library with its own access rules

## Top-Level Layout

```
/media                              ← mount point (user-defined)
├── TV Shows/                       ← parent container (not a library)
│   ├── Kids TV/                    ← Library "Kids TV"       (media_type: tvshows)
│   ├── Documentaries/              ← Library "Documentaries"  (media_type: tvshows)
│   ├── Drama/                      ← Library "Drama"         (media_type: tvshows)
│   └── Anime/                      ← Library "Anime"         (media_type: tvshows)
└── Movies/                         ← parent container (not a library)
    ├── Family Films/               ← Library "Family Films"   (media_type: movies)
    ├── Stand-Up Comedy/            ← Library "Stand-Up"       (media_type: movies)
    ├── Documentaries/              ← Library "Documentaries"  (media_type: movies)
    └── Action/                     ← Library "Action"         (media_type: movies)
```

Each sub-folder is a **library** — a `libraries` row with its own `root_path` and `media_type`. The top-level `TV Shows/` and `Movies/` directories are parent containers only; they group related libraries together on disk but have no representation in the database.

Sub-folder names like "Kids TV", "Family Films", "Stand-Up Comedy", "Documentaries", "Anime", "Action" are **agnostic examples**. Users create whatever libraries make sense for their collection. The server assigns no semantic meaning to library names — they are purely user-chosen labels.

### Primary Pattern: Parent Container → Library Sub-Folders → Items

```
/media/TV Shows/                    ← parent container (not a library)
├── Kids TV/                        ← Library "Kids TV" (root_path)
│   ├── Bluey (2018)/
│   │   └── Season 01/
│   │       └── Bluey (2018) - S01E01.mkv
│   └── Sesame Street (1969)/
│       └── Season 01/
│           └── Sesame Street (1969) - S01E01.mkv
├── Documentaries/                  ← Library "Documentaries" (root_path)
│   ├── Planet Earth (2006)/
│   │   └── Season 01/
│   │       └── Planet Earth (2006) - S01E01 - From Pole to Pole.mkv
│   └── Cosmos (2014)/
│       └── Season 01/
│           └── Cosmos (2014) - S01E01.mkv
├── Drama/                          ← Library "Drama" (root_path)
│   ├── Breaking Bad (2008)/
│   │   └── Season 01/
│   │       └── Breaking Bad (2008) - S01E01 - Pilot.mkv
│   └── The Wire (2002)/
│       └── Season 01/
│           └── The Wire (2002) - S01E01 - The Target.mkv
└── Anime/                          ← Library "Anime" (root_path)
    └── Attack on Titan (2013)/
        └── Season 01/
            └── Attack on Titan (2013) - S01E01.mkv

/media/Movies/                      ← parent container (not a library)
├── Family Films/                   ← Library "Family Films" (root_path)
│   ├── Encanto (2021)/
│   │   └── Encanto (2021).mkv
│   ├── The Lion King (1994)/
│   │   └── The Lion King (1994).mkv
│   └── Coco (2017)/
│       └── Coco (2017).mkv
├── Stand-Up Comedy/                ← Library "Stand-Up" (root_path)
│   ├── Dave Chappelle - Sticks & Stones (2019)/
│   │   └── Dave Chappelle - Sticks & Stones (2019).mkv
│   └── John Mulaney - Kid Gorgeous (2018)/
│       └── John Mulaney - Kid Gorgeous (2018).mkv
├── Documentaries/                  ← Library "Documentaries" (root_path)
│   ├── Free Solo (2018)/
│   │   └── Free Solo (2018).mkv
│   └── Jiro Dreams of Sushi (2011)/
│       └── Jiro Dreams of Sushi (2011).mkv
└── Action/                         ← Library "Action" (root_path)
    └── The Matrix (1999)/
        └── The Matrix (1999).mkv
```

Each library is a distinct entity in the `libraries` table with its own scan schedule, metadata settings, and access control. Items live directly under the library's `root_path` — no additional nesting between the library folder and the item-level folder.

### Other Library Organization Patterns

Users may organize their libraries however they like. The parent container has no special meaning — libraries can be flat, nested, or scattered across disks.

```
# Flat — libraries directly under media root
/media/
├── Movies/
│   └── Inception (2010).mkv
├── Kids Movies/
│   └── Encanto (2021).mkv
└── TV Shows/
    └── Breaking Bad (2008)/

# By quality
/media/Movies/
├── 4K/
│   └── Dune (2021).mkv
└── 1080p/
    └── Inception (2010).mkv

# Across multiple disks
/disk1/Movies/          ← Library "Movies" (root_path)
/disk2/4K Movies/       ← Library "4K Movies" (root_path)
/nas/Archive Movies/    ← Library "Archive" (root_path)
```

A library's `root_path` can point anywhere — the parent container is a filesystem convention, not a server requirement.

## Movie Folder Structure

### Recommended: Per-Movie Folder

```
/media/Movies/Family Films/         ← Library "Family Films" (root_path)
├── Encanto (2021)/
│   ├── Encanto (2021).mkv
│   ├── Encanto (2021).en.srt
│   └── poster.jpg
└── The Lion King (1994)/
    └── The Lion King (1994).mkv
```

Rules:
- Folder name: `Movie Name (Year)`
- Video filename must match folder name: `Movie Name (Year).ext`
- Release year in parentheses is strongly recommended (disambiguates remakes like "The Thing" 1982 vs 2011)
- Supported extensions: `.mkv`, `.mp4`, `.m4v`, `.avi`, `.wmv`, `.ts`, `.mpg`, `.mpeg`

### Metadata Provider ID Tags (Optional)

For guaranteed correct matching, include a provider ID in curly braces:

```
/media/Movies/Action/               ← Library "Action" (root_path)
├── Batman Begins (2005) {tmdb-272}/
│   └── Batman Begins (2005) {tmdb-272}.mkv
├── Casino Royale (2006) {imdb-tt0381061}/
│   └── Casino Royale (2006) {imdb-tt0381061}.mkv
└── The Matrix (1999) {tvdb-603}/
    └── The Matrix (1999) {tvdb-603}.mp4
```

Supported ID formats (interoperable with Plex, Jellyfin, and Emby conventions):

| Format | Example | Providers |
|---|---|---|
| `{tmdb-XXXXX}` | `{tmdb-272}` | TheMovieDB |
| `{imdb-ttXXXXXXX}` | `{imdb-tt0381061}` | IMDb |
| `{tvdb-XXXXX}` | `{tvdb-73244}` | TheTVDB |
| `[tmdbid=XXXXX]` | `[tmdbid=272]` | TheMovieDB (Emby-style) |
| `[imdbid-ttXXXXXXX]` | `[imdbid-tt0381061]` | IMDb (Emby-style) |

The scanner attempts `{tmdb-XXX}` and `{imdb-ttXXX}` formats first (curly braces), then `[tmdbid=XXX]` and `[imdbid-XXX]` formats (square brackets), ensuring compatibility with both Plex/Jellyfin and Emby naming conventions.

### Multiple Editions

Multiple versions of the same movie in one folder:

```
/media/Movies/Action/               ← Library "Action" (root_path)
└── Blade Runner (1982)/
    ├── Blade Runner (1982) - Theatrical.mkv
    ├── Blade Runner (1982) - Directors Cut.mkv
    └── Blade Runner (1982) - Final Cut.mkv
```

Edition name follows ` - ` (space-dash-space) after the base filename. Up to 8 editions per movie.

### Split Files

Movies split across multiple files (discouraged, but supported):

```
/media/Movies/Action/               ← Library "Action" (root_path)
└── The Dark Knight (2008)/
    ├── The Dark Knight (2008) - pt1.mkv
    └── The Dark Knight (2008) - pt2.mkv
```

Supported split labels: `cdX`, `discX`, `diskX`, `dvdX`, `partX`, `ptX` (where X is 1-8 or a-d).

### Sidecar Files

| File Type | Naming | Location |
|---|---|---|
| External subtitles | `Movie Name (Year).{lang}.{flags}.srt` | Inside movie folder |
| Poster image | `poster.jpg` / `folder.jpg` / `cover.jpg` | Inside movie folder |
| Backdrop image | `backdrop.jpg` / `fanart.jpg` | Inside movie folder |
| NFO metadata | `Movie Name (Year).nfo` | Inside movie folder |

### Extras

```
/media/Movies/Family Films/         ← Library "Family Films" (root_path)
└── Inception (2010)/
    ├── Inception (2010).mkv
    ├── behind the scenes/
    │   └── Making Of.mp4
    ├── deleted scenes/
    │   └── Alternate Ending.mp4
    ├── interviews/
    │   └── Nolan Interview.mp4
    ├── trailers/
    │   └── Trailer 1.mp4
    └── extras/
        └── Featurette.mp4
```

Supported extras folder names: `behind the scenes`, `deleted scenes`, `interviews`, `scenes`, `samples`, `shorts`, `featurettes`, `clips`, `extras`, `trailers`, `specials`.

### Fallback: Stand-Alone Files

```
/media/Movies/Action/               ← Library "Action" (root_path)
├── Avatar (2009).mkv
├── Batman Begins (2005).mp4
└── Inception (2010).mkv
```

Flat files in the library's `root_path` are supported. The scanner matches by filename alone. This mode does not support sidecar files, extras, or multiple editions.

## TV Show Folder Structure

### Recommended: Series → Season → Episodes

```
/media/TV Shows/Drama/              ← Library "Drama" (root_path)
└── Breaking Bad (2008)/
    ├── Season 00/
    │   └── Breaking Bad (2008) - S00E01 - Special Title.mkv
    ├── Season 01/
    │   ├── Breaking Bad (2008) - S01E01 - Pilot.mkv
    │   ├── Breaking Bad (2008) - S01E02 - Cat's in the Bag.mkv
    │   └── Breaking Bad (2008) - S01E03-E04 - Multi-Episode Title.mkv
    ├── Season 02/
    │   └── Breaking Bad (2008) - S02E01.mkv
    └── Specials/
        └── Breaking Bad (2008) - S00E01.mkv
```

Rules:
- Series folder: `Show Name (Year)` — year strongly recommended to disambiguate reboots
- Season folder: `Season XX` — zero-padded, English word "Season" regardless of content language; `Specials` or `Season 00` for specials
- Episode filename: `Show Name (Year) - SXXEXX - Optional Title.ext`
- The `SXXEXX` pattern is the critical parse target; separators between show name and episode code may be ` - `, `.`, `_`, or space
- Episode title is optional; the scanner matches by `SXXEXX` alone

### Metadata Provider ID Tags (Optional)

```
/media/TV Shows/Drama/              ← Library "Drama" (root_path)
├── The Office (US) (2005) {tmdb-2316}/
│   └── Season 01/
│       └── The Office (US) (2005) - S01E01 - Pilot.mkv
└── Breaking Bad (2008) {tvdb-81189}/
    └── Season 01/
        └── Breaking Bad (2008) - S01E01 - Pilot.mkv
```

Same ID tag formats as movies: `{tmdb-XXX}`, `{imdb-ttXXX}`, `{tvdb-XXX}`, `[tmdbid=XXX]`, etc.

### Supported Episode Naming Patterns

The scanner recognizes these filename patterns (interoperable with Plex, Jellyfin, and Emby):

| Pattern | Example |
|---|---|
| `Show - S01E01 - Title.ext` | `Breaking Bad - S01E01 - Pilot.mkv` |
| `Show S01E01 Title.ext` | `Breaking Bad S01E01 Pilot.mkv` |
| `Show S01E01.ext` | `Breaking Bad S01E01.mkv` |
| `Show - S01E01-E03 - Title.ext` | Multi-episode (spans E01-E03) |
| `Show - s01e01 - Title.ext` | Lowercase accepted |
| `Show - 1x01 - Title.ext` | Alternate `NxNN` format |
| `Show - 2011-11-15 - Title.ext` | Date-based (daily shows) |
| `S01E01.ext` | Minimal (series inferred from parent folder) |

### Date-Based Shows

For daily/late-night shows (news, talk shows), date-based naming is supported:

```
/media/TV Shows/Talk Shows/         ← Library "Talk Shows" (root_path)
└── The Daily Show (1996)/
    └── Season 2024/
        ├── The Daily Show (1996) - 2024-01-15.mkv
        ├── The Daily Show (1996) - 2024.01.16.mkv
        └── The Daily Show (1996) - 2024-01-17.mkv
```

Supported date formats: `YYYY-MM-DD`, `YYYY.MM.DD`, `YYYY MM DD`, `DD-MM-YYYY`, `DD.MM.YYYY`.

### Specials

Special episodes use `Season 00` or `Specials` folder:

```
/media/TV Shows/Drama/              ← Library "Drama" (root_path)
└── Doctor Who (2005)/
    ├── Specials/
    │   ├── Doctor Who (2005) - S00E01 - The Christmas Invasion.mkv
    │   └── Doctor Who (2005) - S00E02 - The Runaway Bride.mkv
    └── Season 01/
        └── Doctor Who (2005) - S01E01 - Rose.mkv
```

### Multi-Episode Files

```
/media/TV Shows/Drama/              ← Library "Drama" (root_path)
└── Breaking Bad (2008)/
    └── Season 02/
        └── Breaking Bad (2008) - S02E01-E03.mkv
```

Displayed as three separate episodes in the UI, all pointing to the same file.

### Split Episode Files

```
/media/TV Shows/Drama/              ← Library "Drama" (root_path)
└── Some Show (2020)/
    └── Season 01/
        ├── Some Show (2020) - S01E05 - pt1.mkv
        └── Some Show (2020) - S01E05 - pt2.mkv
```

Same split labels as movies: `cdX`, `discX`, `diskX`, `dvdX`, `partX`, `ptX`.

### Sidecar Files

| File Type | Naming | Location |
|---|---|---|
| External subtitles | `Show - S01E01.{lang}.{flags}.srt` | Inside season folder |
| Episode thumbnail | `Show - S01E01-thumb.jpg` | Inside season folder |
| Season poster | `season01-poster.jpg` | Inside series folder |
| Series poster | `poster.jpg` / `folder.jpg` / `show.jpg` | Inside series folder |
| Series backdrop | `backdrop.jpg` / `fanart.jpg` | Inside series folder |

### Extras

```
/media/TV Shows/Drama/              ← Library "Drama" (root_path)
└── Breaking Bad (2008)/
    ├── Season 01/
    │   ├── Breaking Bad (2008) - S01E01.mkv
    │   └── behind the scenes/
    │       └── Inside Episode 1.mp4
    ├── interviews/
    │   └── Bryan Cranston Interview.mp4
    └── behind the scenes/
        └── Making Of Breaking Bad.mp4
```

Extras folders at the series level apply to the show; at the season level apply to that season; at the episode level (inside a per-episode folder) apply to that episode.

### Folder vs Flat Episodes

```
# Recommended: episodes directly in season folder
/media/TV Shows/Drama/Show (2020)/Season 01/
├── Show (2020) - S01E01.mkv
└── Show (2020) - S01E02.mkv

# Also supported: per-episode folders (when extras per episode exist)
/media/TV Shows/Drama/Show (2020)/Season 01/
└── S01E01/
    ├── Show (2020) - S01E01.mkv
    └── behind the scenes/
        └── Inside S01E01.mp4
```

Per-episode folders are only recommended when the episode has their own extras.

## Identification Pipeline

The core problem with Duskcues: identification depends almost entirely on filename parsing. Wrong filenames produce wrong matches or missed files. Our server solves this with a **cascading 5-layer identification pipeline** — each layer is tried in order, and the first successful match wins. Users with well-named files get instant automatic matches. Users with messy filenames get progressively more interactive fallbacks.

### Layer 1: `.media-match` Sidecar File — 100% Exact, Zero Filename Dependency

A simple text file placed in any item-level folder (movie or series). The scanner checks for `.media-match` **before** parsing the folder or filename. If found, the folder/filename is completely ignored for identification.

**Format:**

```
# Lines starting with # are comments
# Blank lines are ignored
# Key: value pairs (colon + space separator)

# Provider ID (any one is sufficient)
tmdb: 272
imdb: tt0372784
tvdb: 73244

# Optional hints (used when provider ID not provided)
title: Batman Begins
year: 2005

# For TV shows — optional per-episode override
# Format: ep: <episode-ref>: <filename>
# Episode refs: E01, S02E05, SP01 (specials)
season: 1
ep: 01: some_weird_filename.mkv
ep: 02: another weird name.mkv
ep: SP01: christmas_special_2023.mkv

# Pattern-based episode matching (alternative to per-file)
# {s} or {season} = season number token
# {e} or {episode} = episode number token
# {sp} or {special} = special number token
# * = glob wildcard
pattern: Show.Part.{s}.-.{e}.-.*

# Edition info for movies (optional)
edition: Directors Cut
```

**Rules:**
- File must be named exactly `.media-match` (with leading dot)
- Placed in the item-level folder (movie folder or series folder)
- Provider ID (`tmdb:`, `imdb:`, `tvdb:`) takes priority — if present, `title:` and `year:` are ignored for matching
- For TV: `ep:` lines override filename parsing for individual files
- For TV: `pattern:` provides a glob-based mapping alternative to per-file `ep:` lines
- For TV: a `.media-match` in a series folder applies to all seasons beneath it; a `.media-match` in a season folder applies to that season only (same cascading as Plex `.plexmatch`)

**Why a new format instead of `.plexmatch` or `.nfo`:**
- `.plexmatch` is Plex-only, requires Plex Pass, and only works for TV shows (not movies)
- `.nfo` is XML (verbose), Kodi-originated, and has inconsistent tag names across implementations
- `.media-match` is: universal (movies + TV), minimal (key-value, not XML), human-writable, and covers all identification scenarios

### Layer 2: NFO File — 100% Exact, Cross-Platform Compatibility

If no `.media-match` exists, the scanner checks for NFO files. Reads provider IDs from the XML tags. Compatible with NFO files generated by Kodi, Jellyfin, Emby, and media managers.

| Media Type | NFO Filename | Location |
|---|---|---|
| Movies | `movie.nfo` or `<filename>.nfo` | Inside movie folder |
| TV Series | `tvshow.nfo` | Inside series folder |
| TV Episodes | `<filename>.nfo` | Inside season folder |

**Tags read for identification:**

```xml
<!-- Movie NFO -->
<movie>
  <tmdbid>272</tmdbid>
  <imdbid>tt0372784</imdbid>
  <title>Batman Begins</title>
  <year>2005</year>
</movie>

<!-- TV Series NFO -->
<tvshow>
  <tmdbid>2316</tmdbid>
  <tvdbid>73244</tvdbid>
  <imdb_id>tt0381061</imdb_id>
  <title>The Office</title>
</tvshow>
```

If a provider ID is found in the NFO, it's used directly — no fuzzy matching needed.

### Layer 3: Provider ID in Folder/Filename — 100% Exact

Parse provider ID tags from the folder name or filename:

```
/Movies/Batman Begins (2005) {tmdb-272}/
/TV Shows/Breaking Bad (2008) {tvdb-81189}/
/Movies/Casino Royale (2006) [imdbid-tt0381061]/
```

Supported formats (interoperable with Plex, Jellyfin, and Emby):

| Format | Example | Style |
|---|---|---|
| `{tmdb-XXXXX}` | `{tmdb-272}` | Plex/Jellyfin curly braces |
| `{imdb-ttXXXXXXX}` | `{imdb-tt0381061}` | Plex/Jellyfin curly braces |
| `{tvdb-XXXXX}` | `{tvdb-73244}` | Plex/Jellyfin curly braces |
| `[tmdbid=XXXXX]` | `[tmdbid=272]` | Emby square brackets |
| `[imdbid-ttXXXXXXX]` | `[imdbid-tt0381061]` | Emby square brackets |
| `[tvdbid=XXXXX]` | `[tvdbid=73244]` | Emby square brackets |

The scanner attempts curly braces first, then square brackets.

### Layer 4: Structured Filename Parse + API Search — ~90% Reliable

Parse the item-level folder name for structured components, then search TMDB/TVDB API.

**Movies:** Parse `Title (Year)` → search TMDB `/search/movie?query=Title&year=Year`

**TV Shows:** Parse series folder name for `Title (Year)` → search TMDB `/search/tv?query=Title&first_air_date_year=Year`

**Episodes:** Parse `SXXEXX` regex from filenames within season folders.

**Minimum viable parse targets:**

| Media Type | Must Have | Minimum Parseable |
|---|---|---|
| Movie | Title + Year | `Batman Begins (2005).mkv` or folder name |
| TV Episode | Season + Episode | Any file containing `S01E01` or `1x01` |
| TV Special | Season 0 + Episode | `S00E01` or `Specials` folder + episode number |

**Confidence scoring:** When multiple search results return, score them:

| Signal | Weight |
|---|---|
| Exact title match (case-insensitive) | 40 |
| Year matches | 30 |
| Provider ID matches | 100 (auto-confirmed) |
| Title contains search query | 20 |
| Popular result (high TMDB vote count) | 10 |

If the top score exceeds a confidence threshold (default: 70), auto-confirm the match. Otherwise, queue for Layer 5.

### Layer 5: Unmatched Queue + Interactive Fix — Last Resort

Files that fail layers 1-4 appear in the admin UI's **Unmatched** section. The admin can:

1. **Search manually** — enter title, year, or provider ID
2. **Browse results** — see top matches with posters and metadata for confirmation
3. **Confirm match** — select the correct item

**Critical design:** When the admin confirms a match, the server **automatically writes a `.media-match` file** to the item's folder. This ensures:
- Manual corrections survive re-scans permanently
- Corrections survive server migrations and database rebuilds
- Corrections are filesystem-visible and portable
- No database-only state that's lost on rebuild

### Pipeline Summary

```
For each discovered item-level folder:
  1. Check for .media-match file → if found, use provider ID → DONE
  2. Check for NFO file → if found with provider ID → DONE
  3. Check for {tmdb-XXX} / {imdb-ttXXX} in folder name → if found → DONE
  4. Parse folder/filename for Title + Year + SXXEXX
     → search TMDB/TVDB API
     → if confidence >= threshold → auto-confirm → DONE
     → if confidence < threshold → queue for manual review
  5. Admin reviews unmatched queue → selects correct match
     → server writes .media-match file → DONE (persists forever)
```

### Specials Support

TV specials are handled consistently across all layers:

| Method | How Specials Are Identified |
|---|---|
| `.media-match` | `ep: SP01: filename.mkv` or `ep: S00E01: filename.mkv` |
| Folder structure | Files in `Season 00/` or `Specials/` folder |
| Filename | `S00E01` or `SP01` in the filename |
| NFO | `<season>0</season><episode>1</episode>` in episode NFO |

Specials display within their season when the metadata provider supplies `airsbefore_season` / `airsbefore_episode` data (TMDB and TVDB both support this).

## Scanner Traversal

### How the Scanner Walks Each Library

```
For each library in libraries table:
  1. Read library.root_path (or library_paths rows)
  2. Walk root_path recursively (using `ignore` crate parallel walker)
  3. At each directory, check: does this look like an item-level folder?
     - Movie folder: contains video files directly (no Season XX sub-folders)
     - Series folder: contains Season XX / Specials sub-folders, or video files with SXXEXX patterns
  4. If neither pattern matches, the directory is a transparent container — keep recursing
  5. Parse the item-level folder name for: Title, (Year), {provider-id}
  6. Parse video filenames for: SXXEXX patterns (TV only), edition names, split markers
```

### Item-Level Folder Detection

A directory is identified as an item-level folder when:

**For movie libraries (`media_type = 'movies'`):**
- Contains one or more video files directly (no Season sub-folders)
- Folder name matches the pattern: `Title (Year) [optional-id]`

**For TV libraries (`media_type = 'tvshows'`):**
- Contains `Season XX` or `Specials` sub-directories
- OR contains video files with `SXXEXX` patterns in filenames (series folder without explicit seasons)
- Folder name matches the pattern: `Title (Year) [optional-id]`

### Transparent Container Detection

A directory is treated as a transparent container when:
- It does not match item-level folder patterns above
- It contains no video files at the top level
- It contains sub-directories

This handles edge cases like a user who creates an extra nesting level inside a library:

```
/media/Movies/Family Films/         ← Library "Family Films" (root_path)
├── Animated/                        ← transparent container
│   └── Encanto (2021)/
│       └── Encanto (2021).mkv
└── Live Action/                     ← transparent container
    └── Paddington (2014)/
        └── Paddington (2014).mkv
```

The scanner recurses through `Animated/` and `Live Action/` transparently to find the item-level folders beneath.

## Multiple Root Paths

A library may span multiple `root_path` entries. This enables:

- Spreading media across multiple drives
- Merging category sub-folders into one library
- Adding new content locations without moving existing files

### Schema: `library_paths` Table

```sql
CREATE TABLE library_paths (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    is_default BOOLEAN NOT NULL DEFAULT false,
    scan_enabled BOOLEAN NOT NULL DEFAULT true,
    last_scan_at TIMESTAMPTZ,
    UNIQUE(library_id, path)
);

CREATE INDEX idx_library_paths_library ON library_paths (library_id);
```

- A library must have at least one path (`is_default = true`)
- Each path is scanned independently
- `scan_enabled` per path allows disabling scanning on offline/network drives without removing the path
- `last_scan_at` per path for scan staleness tracking

### Migration: `libraries.root_path` → `library_paths`

The existing `libraries.root_path` column becomes the default path in `library_paths`:

```sql
-- On migration:
INSERT INTO library_paths (library_id, path, is_default, scan_enabled, last_scan_at)
SELECT id, root_path, true, scan_enabled, last_scan_at
FROM libraries;
```

`libraries.root_path` and `libraries.last_scan_at` columns are retained for backward compatibility but deprecated. The scanner reads from `library_paths`.

### Example: Multi-Path Library

```
Library: "All Movies"
  Path 1 (default): /media/Movies/         (is_default=true)
  Path 2:           /media2/4K Movies/      (scan_enabled=true)
  Path 3:           /nas/Archive Movies/    (scan_enabled=false, offline)
```

All three paths contribute items to the "All Movies" library. The scanner skips Path 3 until `scan_enabled` is set back to true.

## Collection Design

Collections group items across libraries by metadata, not by folder location.

### Smart Collections (Metadata-Driven)

Defined by filter rules evaluated at query time:

| Filter | Example |
|---|---|
| Genre | `WHERE genre = 'Documentary'` |
| Content rating | `WHERE content_rating IN ('G', 'PG')` |
| TMDB collection ID | `WHERE tmdb_collection_id = 119050` (MCU) |
| Director | `WHERE director = 'Christopher Nolan'` |
| Decade | `WHERE premiere_date BETWEEN '1990-01-01' AND '1999-12-31'` |
| Watch status | `WHERE NOT EXISTS (watched entry)` |
| User-defined tags | `WHERE tag IN ('kids-safe', 'christmas')` |

Smart collections are auto-maintained — new items matching filters appear automatically.

### Manual Collections

Curated by the admin — arbitrary grouping of items. A manual collection can contain items from any library (cross-library).

### Collection UI

Collections appear as rows on the home screen and as a tab within each library. Both smart and manual collections support custom posters, sort order, and visibility per user.

## Access Control Pattern

Each library (sub-folder) has its own access control via `user_library_access`. The parent container has no access rules — only the libraries within it do.

```
Libraries:
  "Kids TV"          → /media/TV Shows/Kids TV/
  "Documentaries"    → /media/TV Shows/Documentaries/
  "Drama"            → /media/TV Shows/Drama/
  "Family Films"     → /media/Movies/Family Films/
  "Stand-Up"         → /media/Movies/Stand-Up Comedy/
  "Movies"           → /media/Movies/Action/

Admin configures:
  User "Kids Profile"  → access: Kids TV, Family Films
  User "Adults"        → access: all libraries
  User "Guest"         → access: Drama, Documentaries, Movies (no Stand-Up)
```

## Scanner Integration

For scanner implementation details (phases, hashing, watching, debouncing), see [MEDIA_SCANNING.md](MEDIA_SCANNING.md).

Key scanner behaviors related to folder structure:

| Behavior | Detail |
|---|---|
| Per-library scanning | Each `libraries` row has its own `root_path` (or `library_paths` rows); scanned independently |
| Recursive walking | `ignore` crate walks entire tree under each library path |
| Transparent containers | Intermediate directories not matching item-level patterns are recursed through |
| Item detection | Folder name parsed for Title + Year + Provider ID |
| Season detection | `Season XX` or `Specials` folders under series folder |
| Episode detection | `SXXEXX` regex on filenames |
| mtime-based diffing | Unchanged files skipped (Phase 2 in MEDIA_SCANNING.md) |
| Filesystem watching | `notify` crate watches each library path recursively |

## Reserved Names

### Reserved File Names

These filenames have special meaning when found inside any media folder:

| Name | Context | Meaning |
|---|---|---|
| `.media-match` | In any item-level folder | Identification sidecar — overrides folder/filename for matching (see Identification Pipeline, Layer 1) |
| `movie.nfo` | In movie folder | NFO metadata file — provider ID read for identification (Layer 2) |
| `tvshow.nfo` | In series folder | NFO metadata file — provider ID read for identification (Layer 2) |
| `season.nfo` | In season folder | NFO metadata for season |
| `<filename>.nfo` | In any folder | NFO metadata for specific file |
| `poster.jpg` / `folder.jpg` / `cover.jpg` | In any item folder | Primary poster image |
| `backdrop.jpg` / `fanart.jpg` | In any item folder | Background image |
| `theme.mp3` / `theme.ext` | In any item folder | Theme music |

### Reserved Folder Names

These folder names have special meaning when found inside an item-level folder and are **not** treated as transparent containers:

| Name | Context | Meaning |
|---|---|---|
| `Season XX` / `Season X` | Under series folder | Season grouping |
| `Specials` | Under series folder | Season 00 specials |
| `behind the scenes` | Under any item | Extras |
| `deleted scenes` | Under any item | Extras |
| `interviews` | Under any item | Extras |
| `scenes` | Under any item | Extras |
| `samples` | Under any item | Extras |
| `shorts` | Under any item | Extras |
| `featurettes` | Under any item | Extras |
| `clips` | Under any item | Extras |
| `extras` | Under any item | Extras |
| `trailers` | Under any item | Extras |
| `theme-music` | Under any item | Theme audio |
| `backdrops` | Under any item | Theme video / extra backdrops |
| `VIDEO_TS` | Under any item | DVD structure |
| `BDMV` | Under any item | Blu-ray structure |
| `SXXEXX` (e.g. `S01E05`) | Under season folder | Per-episode container for extras |

All other folder names at any level are treated as transparent user organizational folders.

## Docker Volume Mapping

```
docker run \
  -v /mnt/user/media:/media:ro \
  -v /mnt/user/data:/data \
  ...
```

- `/media` is read-only — the server never writes to media files
- Parent containers (`/media/TV Shows/`, `/media/Movies/`) are visible inside the container
- Library `root_path` values point to sub-folders (`/media/TV Shows/Kids TV/`, `/media/Movies/Family Films/`, etc.)
- Users add libraries through the admin UI, selecting the appropriate sub-folder as the `root_path`

## Research Sources

- Plex naming and organizing Movie files: https://support.plex.tv/articles/naming-and-organizing-your-movie-media-files/
- Plex naming and organizing TV Show files: https://support.plex.tv/articles/naming-and-organizing-your-tv-show-files/
- Plex .plexmatch match hinting: https://support.plex.tv/articles/plexmatch/
- Plex Fix Match feature: https://support.plex.tv/articles/201018497-fix-match-match/
- Plex Collections: https://support.plex.tv/articles/201273953-collections/
- Jellyfin Movies documentation: https://jellyfin.org/docs/general/server/media/movies/
- Jellyfin TV Shows documentation: https://jellyfin.org/docs/general/server/media/shows/
- Jellyfin NFO metadata: https://jellyfin.org/docs/general/server/metadata/nfo/
- Jellyfin metadata guide (2026): https://jellywatch.app/blog/jellyfin-metadata-fix-tmdb-nfo-artwork-guide-2026
- Emby Movie Naming: https://emby.media/support/articles/Movie-Naming.html
- Emby TV Naming: https://emby.media/support/articles/TV-Naming.html
- TRaSH Guides Sonarr naming scheme: https://trash-guides.info/Sonarr/Sonarr-recommended-naming-scheme/
- TRaSH Guides Radarr naming scheme: https://trash-guides.info/Radarr/Radarr-recommended-naming-scheme/
- Reddit r/PleX library organization discussion (March 2024): https://www.reddit.com/r/PleX/comments/1bn1cc8/
- Reddit r/PleX collections best practices (January 2026): https://www.reddit.com/r/PleX/comments/1qb7k3x/
- Reddit r/unRAID categories vs individual libraries (July 2024): https://www.reddit.com/r/unRAID/comments/1dww75x/
- Reddit r/PleX separate libraries vs filters (December 2023): https://www.reddit.com/r/PleX/comments/18ajhkm/
- Reddit r/PleX fix match issues (August 2024): https://www.reddit.com/r/PleX/comments/1ei4rw9/
- AcoustID video identification discussion (Google Groups): https://groups.google.com/g/acoustid/c/C0QPEqkkpxk
- TMDB hash-based identification discussion (Kodi Forum, 2009): https://forum.kodi.tv/showthread.php?tid=58031
- Sonarr .plexmatch episode mapping request (GitHub #5784): https://github.com/Sonarr/Sonarr/issues/5784
