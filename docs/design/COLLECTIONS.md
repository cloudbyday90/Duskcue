# Collections

## Overview

The collections system provides server-level media groupings visible to all users — both manually curated static collections and dynamically generated smart collections that auto-populate from metadata, external charts, and library analysis. This replaces Kometa's collection system with a built-in, admin-UI-driven approach.

This document is one of three pillars in the artwork customization architecture:
- [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md) — overlay compositing engine (badges, text, dynamic content)
- [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md) — artwork sourcing, selection, locking, bulk operations
- **This document** — static and dynamic collections with custom poster art

## Collection Types

### Static Collections

Manually curated by the admin. Items are explicitly added and removed. Posters and metadata can be customized.

Use cases: "James Bond Collection", "Studio Ghibli", "Christmas Movies", "Staff Picks"

### Dynamic Collections

Auto-generated based on rules. The collection definition specifies a builder source and criteria; the server populates the collection automatically on a schedule. Items are synced — added when they match, removed when they no longer match.

Use cases: "Top 10 Trending on TMDb", "Action Movies", "Best of 2020s", "All Steven Spielberg Films", "Marvel Cinematic Universe"

### Smart Collections

Filter-based collections that evaluate at query time (no stored items). Similar to smart playlists (see [DATABASE.md](DATABASE.md) `playlists.smart_filter`).

Use cases: "Unwatched Movies", "Highly Rated But Unwatched", "Recently Added 4K"

## Builder Sources

Dynamic collections pull items from builder sources — either local library metadata or external APIs.

### Internal Builders (Library Metadata)

Query the local database using `media_items`, `media_files`, `genres`, `credits`, and related tables.

| Builder | Description | Source |
|---|---|---|
| `genre` | One collection per genre in the library | `genres` table |
| `country` | One collection per country of origin | `media_items.metadata` |
| `decade` | One collection per decade represented | `media_items.premiere_date` |
| `content_rating` | One collection per content rating | `media_items.content_rating` |
| `actor` | Top N actors by appearance count | `media_credits` where `credit_type = 'cast'` |
| `director` | Top N directors by film count | `media_credits` where `department = 'Directing'` |
| `studio` | One collection per studio/production company | `media_items.metadata` |
| `network` | One collection per TV network | `media_items.metadata` |
| `franchise` | Auto-detected franchises/universes | `media_items.metadata` (TMDb collection data) |
| `original_language` | One collection per original language | `media_items.metadata` |
| `year` | One collection per year or year range | `media_items.premiere_date` |
| `resolution` | One collection per resolution tier | `media_files.video_resolution` |
| `audio_codec` | One collection per audio codec | `media_files.audio_codec` |
| `streaming_service` | One collection per streaming platform | `media_items.metadata` (TMDb watch providers) |

### External Builders (API Sources)

Fetch lists from external metadata providers and match against the local library.

| Builder | API Source | Endpoint | Match By |
|---|---|---|---|
| `tmdb_popular` | TMDb | `/movie/popular`, `/tv/popular` | `media_items.tmdb_id` |
| `tmdb_top_rated` | TMDb | `/movie/top_rated`, `/tv/top_rated` | `media_items.tmdb_id` |
| `tmdb_trending` | TMDb | `/trending/movie/day`, `/trending/tv/day` | `media_items.tmdb_id` |
| `tmdb_now_playing` | TMDb | `/movie/now_playing` | `media_items.tmdb_id` |
| `tmdb_upcoming` | TMDb | `/movie/upcoming` | `media_items.tmdb_id` |
| `tmdb_collection` | TMDb | `/collection/{id}` | `media_items.tmdb_id` (auto-groups by TMDb collection ID) |
| `trakt_trending` | Trakt | `/movies/trending`, `/shows/trending` | `media_items.trakt_id` |
| `trakt_popular` | Trakt | `/movies/popular`, `/shows/popular` | `media_items.trakt_id` |
| `trakt_recommended` | Trakt | `/recommendations/movies`, `/recommendations/shows` | `media_items.trakt_id` |
| `trakt_user_lists` | Trakt | `/users/{id}/lists/{list_id}/items` | `media_items.trakt_id` |
| `imdb_top_250` | OMDb / TMDb external IDs | Cross-reference IMDb IDs | `media_items.imdb_id` |
| `custom_url` | Any | User-provided JSON endpoint | Configurable |

Items not in the local library are tracked as "missing" and reported to the admin (optional Radarr/Sonarr integration hint — future feature).

## Dynamic Collection Configuration

Each dynamic collection definition specifies:

### Core Settings

| Setting | Description | Default |
|---|---|---|
| `builder_type` | Builder source (e.g. `genre`, `tmdb_popular`) | Required |
| `builder_data` | Builder-specific config (limits, filters) | `{}` |
| `sync_mode` | `sync` (add + remove) or `append` (add only) | `sync` |
| `schedule` | Cron expression for when to refresh | `0 6 * * *` |
| `limit` | Max items per collection | 100 |
| `minimum_items` | Don't create collection if fewer than N items match | 1 |
| `sort_by` | Sort order within the collection | `title.asc` |

### Naming Customization

| Setting | Description | Example |
|---|---|---|
| `title_format` | Template for collection names | `"Top <<key_name>> <<library_type>>s"` |
| `key_name_override` | Rename specific keys | `{ "France": "French" }` |
| `remove_prefix` | Remove prefixes from auto-discovered keys | `["The", "A"]` |
| `remove_suffix` | Remove suffixes from auto-discovered keys | `["Collection"]` |

Template variables: `<<key_name>>` (the dynamic key, e.g. "Action"), `<<library_type>>` ("movie"/"show"), `<<limit>>`.

### Filtering

| Setting | Description |
|---|---|
| `include` | Only create collections for these keys |
| `exclude` | Skip these keys |
| `addons` | Merge multiple keys into one (e.g. `{"MTV": ["MTV2", "MTV3", "MTV (UK)"]}`) |

### Poster and Metadata

| Setting | Description |
|---|---|
| `poster_source` | `auto` (TMDb collection art), `custom` (admin-specified URL), `overlay` (generated via overlay engine) |
| `poster_url` | URL or file path for custom poster |
| `summary` | Collection description (static or template) |

## Dynamic Collection Types in Detail

### Genre Collections

Auto-discovers all genres present in the library. Creates one collection per genre, sorted by rating.

```json
{
  "builder_type": "genre",
  "title_format": "Top <<key_name>> <<library_type>>s",
  "exclude": ["Talk Show"],
  "limit": 50,
  "sort_by": "rating_average.desc"
}
```

Creates: "Top Action Movies", "Top Comedy Movies", "Top Drama Shows", etc.

### Decade Collections

Groups items by decade based on `premiere_date`.

```json
{
  "builder_type": "decade",
  "title_format": "Best of the <<key_name>>s",
  "key_name_override": { "2020": "2020s (so far)" }
}
```

Creates: "Best of the 1990s", "Best of the 2000s", "Best of the 2020s (so far)", etc.

### TMDb Collection (Franchise)

Auto-discovers TMDb collections for items in the library. One collection per franchise.

```json
{
  "builder_type": "tmdb_collection",
  "remove_suffix": "Collection",
  "title_override": { "10": "Star Wars Universe" },
  "poster_source": "auto"
}
```

Creates: "Star Wars Universe", "Harry Potter", "Marvel Cinematic Universe", etc. TMDb provides canonical franchise posters.

### Actor/Director Collections

Top N people by number of appearances.

```json
{
  "builder_type": "actor",
  "builder_data": { "top_n": 25, "minimum_appearances": 3 },
  "title_format": "Best <<key_name>> Movies"
}
```

Creates: "Best Tom Hanks Movies", "Best Meryl Streep Movies", etc.

### External Chart Collections

```json
{
  "builder_type": "tmdb_trending",
  "builder_data": { "limit": 20, "time_window": "day" },
  "title_format": "Trending on TMDb",
  "sync_mode": "sync",
  "schedule": "0 */6 * * *"
}
```

Creates a single "Trending on TMDb" collection refreshed every 6 hours.

### Streaming Service Collections

```json
{
  "builder_type": "streaming_service",
  "builder_data": { "country": "US", "providers": ["netflix", "disney_plus", "amazon_prime"] },
  "title_format": "Streaming on <<key_name>>",
  "poster_source": "overlay"
}
```

## Built-in Default Collections

Seeded on first run as system collections (can be disabled, not deleted):

| Collection | Builder | Schedule | Description |
|---|---|---|---|
| TMDb Popular | `tmdb_popular` | Every 6 hours | Currently popular on TMDb |
| TMDb Top Rated | `tmdb_top_rated` | Daily | Highest rated on TMDb |
| TMDb Trending | `tmdb_trending` | Every 6 hours | Trending today on TMDb |
| New Releases | `year` (current year) | Daily | Items from current year |
| Recently Added | Smart filter | Real-time | Items added in last 30 days |
| Genre Collections | `genre` | Weekly | One per genre, top 50 by rating |
| Decade Collections | `decade` | Weekly | One per decade |
| Holiday/Seasonal | `genre` + seasonal keywords | Monthly | Christmas, Halloween, etc. |

## Collection Templates

Reusable definitions for common collection patterns. Admin can duplicate and customize templates.

```json
{
  "name": "Award Winner Collections",
  "version": 1,
  "collections": [
    {
      "name": "Oscar Best Picture Winners",
      "builder_type": "custom_url",
      "builder_data": {
        "url": "https://example.com/oscar-best-picture.json",
        "match_field": "imdb_id"
      },
      "poster_url": "https://example.com/oscar-poster.jpg"
    }
  ]
}
```

Templates can be imported from the admin UI template browser, shared as JSON files, or contributed by the community.

## Database Schema

### Collections

```sql
CREATE TABLE collections (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    library_id UUID REFERENCES libraries(id) ON DELETE CASCADE,

    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    description TEXT,

    collection_type TEXT NOT NULL DEFAULT 'static' CHECK (collection_type IN ('static', 'dynamic', 'smart')),
    visibility TEXT NOT NULL DEFAULT 'visible' CHECK (visibility IN ('visible', 'hidden', 'featured')),

    is_dynamic BOOLEAN NOT NULL DEFAULT false,
    dynamic_config JSONB,

    is_smart BOOLEAN NOT NULL DEFAULT false,
    smart_filter JSONB,

    poster_artwork_id UUID REFERENCES artwork(id) ON DELETE SET NULL,
    backdrop_artwork_id UUID REFERENCES artwork(id) ON DELETE SET NULL,

    sort_order INT NOT NULL DEFAULT 0,
    sort_by TEXT NOT NULL DEFAULT 'title.asc',

    item_count INT NOT NULL DEFAULT 0,
    total_duration_seconds INT NOT NULL DEFAULT 0,

    sync_mode TEXT NOT NULL DEFAULT 'sync' CHECK (sync_mode IN ('sync', 'append')),
    schedule TEXT NOT NULL DEFAULT '0 6 * * *',
    last_synced_at TIMESTAMPTZ,
    last_sync_result JSONB,

    is_enabled BOOLEAN NOT NULL DEFAULT true,
    is_system BOOLEAN NOT NULL DEFAULT false,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE UNIQUE INDEX collections_slug_library ON collections (slug, library_id) WHERE library_id IS NOT NULL;
CREATE UNIQUE INDEX collections_slug_global ON collections (slug) WHERE library_id IS NULL;
CREATE INDEX idx_collections_library ON collections (library_id) WHERE library_id IS NOT NULL;
CREATE INDEX idx_collections_type ON collections (collection_type);
CREATE INDEX idx_collections_visibility ON collections (visibility);
CREATE INDEX idx_collections_enabled ON collections (is_enabled) WHERE is_enabled = true;
CREATE INDEX idx_collections_dynamic ON collections (is_dynamic) WHERE is_dynamic = true;
CREATE INDEX idx_collections_schedule ON collections (last_synced_at) WHERE is_dynamic = true AND is_enabled = true;
```

`library_id` — when null, the collection spans all libraries. When set, scoped to that library.

`collection_type` — `static` (manual items), `dynamic` (builder-populated), `smart` (filter-evaluated at query time).

`dynamic_config` — JSONB with the builder configuration (see Dynamic Collection Configuration section).

`smart_filter` — same JSONB filter format as smart playlists. Evaluated at query time. Example: `{"genres": ["action"], "year_min": 2020, "rating_min": 7.0, "is_watched": false}`.

`poster_artwork_id` / `backdrop_artwork_id` — FK to the `artwork` table. Can be TMDb-sourced, user-uploaded, or overlay-generated.

`visibility` — `visible` (shown in library), `hidden` (not shown but exists for Trakt/sync), `featured` (shown prominently on home screen).

`last_sync_result` — JSONB with sync statistics: `{ "added": 3, "removed": 1, "missing": 5, "total_matched": 42 }`.

`is_system` — built-in collections seeded by the server. Cannot be deleted.

### Collection Items

```sql
CREATE TABLE collection_items (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    collection_id UUID NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    position INT NOT NULL DEFAULT 0,

    is_missing BOOLEAN NOT NULL DEFAULT false,
    missing_reason TEXT,

    UNIQUE(collection_id, media_item_id)
);

CREATE INDEX idx_collection_items_collection ON collection_items (collection_id);
CREATE INDEX idx_collection_items_media_item ON collection_items (media_item_id);
CREATE INDEX idx_collection_items_position ON collection_items (collection_id, position);
CREATE INDEX idx_collection_items_missing ON collection_items (is_missing) WHERE is_missing = true;
```

`position` — integer spacing (1000, 2000, 3000) for reordering without renumbering.

`is_missing` — the item matched an external builder but is not in the local library. Shown as "missing" in the admin UI for optional follow-up.

`missing_reason` — why the item is missing: `not_in_library`, `no_match_found`.

### Collection Templates

```sql
CREATE TABLE collection_templates (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    name TEXT NOT NULL UNIQUE,
    description TEXT,

    template_type TEXT NOT NULL CHECK (template_type IN ('single', 'multi')),
    template_json JSONB NOT NULL,

    author TEXT,
    source_url TEXT,

    is_system BOOLEAN NOT NULL DEFAULT false,

    metadata JSONB NOT NULL DEFAULT '{}'
);
```

`template_type` — `single` (creates one collection), `multi` (creates multiple collections from a builder, like genre or decade).

`template_json` — the full collection definition(s) in JSON format, importable as-is.

## Scheduled Tasks

| Task | Schedule | Timeout | Config |
|---|---|---|---|
| Collection Sync | `0 6 * * *` (daily 06:00, after overlay application) | 2h | `{ "sync_dynamic": true, "sync_external": true, "max_external_requests_per_minute": 30 }` |
| Collection Cleanup | `0 7 * * 0` (weekly Sun 07:00) | 30m | `{ "remove_empty": true, "remove_disabled": false }` |

**Collection Sync** — refreshes all enabled dynamic collections:
1. Internal builders: query local DB, update `collection_items`
2. External builders: fetch from API, match against local library, update `collection_items`
3. Mark unmatched external items as `is_missing = true`
4. Update `item_count`, `total_duration_seconds`, `last_synced_at`, `last_sync_result`
5. If `sync_mode = 'sync'`: remove items that no longer match

**Collection Cleanup** — removes empty collections (if `remove_empty`), removes orphaned items, renumbers positions.

## External API Rate Limiting

External builder calls are rate-limited to avoid hitting TMDb/Trakt API limits:

| Provider | Rate Limit | Strategy |
|---|---|---|
| TMDb | ~40 requests/10 seconds | Token bucket; batch requests; cache for 6 hours |
| Trakt | 1000 requests/5 minutes (authed) | Sliding window; respect `Retry-After` header |
| OMDb | 1000/day (free tier) | Daily quota counter; fail gracefully |

The collection sync task tracks API usage in `last_sync_result` and respects provider rate limits automatically.

## Smart Filter Syntax

Smart collections and smart playlists share the same filter syntax:

```json
{
  "operator": "and",
  "rules": [
    { "field": "genre", "op": "in", "values": ["Action", "Adventure"] },
    { "field": "year", "op": "gte", "value": 2020 },
    { "field": "rating_average", "op": "gte", "value": 7.0 },
    { "field": "is_watched", "op": "eq", "value": false },
    { "field": "video_resolution", "op": "eq", "value": "4K" }
  ]
}
```

This is the same condition system used by overlay definitions (see [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md)). The condition evaluator is implemented as a shared service at `server/src/services/conditions.rs` — pure, stateless, no DB. Smart collections (Task 5+) and smart playlists will call `conditions::evaluate()` against a `MediaFilterContext` built from the media item's metadata.

## Admin UI

The collections management section provides:

1. **Collection list** — all collections with type badge, item count, last synced, enabled toggle
2. **Collection editor** — name, description, poster, visibility, sort order
3. **Dynamic builder config** — builder type dropdown, preview of generated collections, schedule picker
4. **Item manager** — for static collections: add/remove/reorder items; for dynamic: view matched items and missing items
5. **Template browser** — browse and import community templates with preview
6. **Sync dashboard** — last sync results, missing items, API usage

## Error Codes

| Code | HTTP | Description |
|---|---|---|
| `COLL_001` | 404 | Collection not found |
| `COLL_002` | 409 | Collection name already exists in this library |
| `COLL_003` | 409 | Collection sync already in progress |
| `COLL_004` | 422 | Invalid dynamic collection configuration |
| `COLL_005` | 422 | Invalid smart filter syntax |
| `COLL_006` | 503 | External builder source unavailable (TMDb, Trakt, etc.) |
| `COLL_007` | 429 | External API rate limit exceeded during collection sync |
| `COLL_008` | 404 | Collection template not found |

New domain: **COLL** (8 codes). Total error codes: **102** (88 existing + 6 OVERLAY + 8 COLL).

## Configuration

Collection settings are stored in `server_config.metadata` JSONB (shared with overlay settings):

```json
{
  "collections_enabled": true,
  "collection_sync_schedule": "0 6 * * *",
  "collection_default_poster_source": "auto",
  "collection_max_items_default": 100,
  "collection_track_missing": true,
  "collection_external_rate_limit_per_minute": 30
}
```

## Implementation Notes

### Phase 12 Task 5

The collections domain scaffold is implemented at `server/src/domains/collections/` using the standard five-file domain pattern:

| File | Implementation |
|---|---|
| `mod.rs` | Router assembly for collection CRUD, item management, sync dispatch, and template operations |
| `error.rs` | `CollectionsError` with registered `COLL_001`–`COLL_008` variants plus database catch-all |
| `types.rs` | Internal row DTOs for `collections`, `collection_items`, and `collection_templates`; request/response DTOs; validation statics |
| `service.rs` | Validation helpers and concrete service signatures; DB CRUD and builders deferred to Tasks 6–7 |
| `handlers.rs` | Axum handlers with validation and `Require<CanManageLibraries>` authorization |

All collection endpoints are wired into the top-level router under `/api/v1/collections`, and `AppError::Collections` maps `COLL_001`–`COLL_008` to RFC 9457 responses. Smart-filter structural validation calls the shared `services::conditions::validate_structure()` engine from Phase 12 Task 3 so overlays, smart collections, and future smart playlists use the same JSONB rule grammar.

## Cross-References

- [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md) — overlays can be applied to collection posters; shared condition/filter system
- [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md) — collection poster sourcing from TMDb, user uploads, overlays
- [DATABASE.md](DATABASE.md) — `collections`, `collection_items`, `collection_templates` tables
- [ERROR_HANDLING.md](ERROR_HANDLING.md) — COLL_001–COLL_008 error codes
- [CONFIGURATION.md](../operations/CONFIGURATION.md) — `server_config.metadata` JSONB
- [CACHE_STORAGE.md](../operations/CACHE_STORAGE.md) — cached external API responses
- [MEDIA_SCANNING.md](MEDIA_SCANNING.md) — Phase 5 enrichment populates metadata used by builders
