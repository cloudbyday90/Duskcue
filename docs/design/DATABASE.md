# Database Design

## Database Platform

**PostgreSQL 18** (released September 25, 2025)

### Key PostgreSQL 18 Features Leveraged

| Feature | Relevance |
|---|---|
| **Native `uuidv7()`** | Primary key generation — timestamp-ordered, B-tree friendly, no extensions |
| **Virtual generated columns** | Derive `created_at` from UUIDv7 without storage overhead (default in PG18) |
| **Asynchronous I/O** | Up to 2-3x read performance improvement via `io_uring` (Linux) or worker fallback |
| **B-tree skip scan** | Multicolumn indexes usable without specifying all leading columns — reduces total indexes needed |
| **`NOT NULL NOT VALID`** | Add constraints on large tables without full table scan or exclusive lock |
| **Enhanced `RETURNING`** | Access `old`/`new` row values in single statement — useful for audit trails |
| **Temporal constraints** | `WITHOUT OVERLAPS` for preventing overlapping date ranges |
| **Preserved planner stats on upgrade** | No post-upgrade ANALYZE marathon |
| **Data checksums by default** | Corruption detection out of the box |
| **OAuth 2.0 auth** | Native OIDC integration for database authentication |

---

## Surrogate Key Strategy: UUIDv7

### Decision

All tables use **UUIDv7** as the surrogate primary key, generated natively by PostgreSQL 18's `uuidv7()` function.

```sql
CREATE TABLE example (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED
);
```

### Why UUIDv7

| Requirement | UUIDv7 Fit |
|---|---|
| Unguessable IDs (API exposure) | 74 bits of randomness — no enumeration attacks |
| Good B-tree index performance | Time-ordered inserts append sequentially — near-zero page splits |
| Client-side ID generation | Rust server or offline clients can generate IDs without DB round-trips |
| Globally unique (distributed) | No coordination needed across server instances |
| Self-timestamping | `uuid_extract_timestamp(id)` — eliminates or supplements `created_at` column |
| PostgreSQL native | `uuidv7()` built into PG18 — no extensions, no app-side generation required |
| RFC 9562 standard | Published May 2024 — industry-standard format, not vendor-specific |

### UUIDv7 Structure (128 bits)

```
| 48 bits         | 12 bits  | 62 bits               | 6 bits           |
| Unix timestamp  | Sub-ms   | Random                | Version/Variant  |
| (milliseconds)  | fraction | (uniqueness)          | (RFC 9562)       |
```

- Monotonic within the same PG backend session (sub-millisecond counter)
- Extractable timestamp via `uuid_extract_timestamp()`
- Version inspectable via `uuid_extract_version()`

### Alternatives Evaluated

| Option | Size | Time-sortable | B-tree Performance | PG18 Native | Verdict |
|---|---|---|---|---|---|
| **BIGINT IDENTITY** | 8 bytes | Yes | Best | Yes | Too guessable for API-exposed IDs; leaks record counts |
| **UUIDv4** | 16 bytes | No | Poor (random inserts, 24%+ index bloat, 11x slower index builds) | Yes | Obsoleted by UUIDv7 for primary keys |
| **UUIDv7** | 16 bytes | Yes | Excellent (sequential append) | **Yes** | **Selected** |
| **ULID** | 26 chars (text) | Yes | Good | No (extension) | Redundant with native UUIDv7; 43% slower than native `uuidv7()` |
| **CUID2** | 24+ chars (text) | No | Poor | No (app-side) | Better for URL slugs, not primary keys |
| **NanoID** | 21+ chars (text) | No | Poor | No (app-side) | Better for transient tokens, not primary keys |
| **Snowflake** | 8 bytes (bigint) | Yes | Excellent | No (app-side) | Requires coordination/machine ID config; not PG-native |

### Trade-offs Accepted

- **33% larger indexes** vs BIGINT (16 bytes vs 8 bytes per key) — acceptable for the security and distributed benefits
- **Timestamp is extractable** from the ID — not a concern for a Duskcue (not PII)
- **Millisecond precision** — sub-ms ordering guaranteed within a session, but cross-session same-ms IDs are randomly ordered

---

## Standards & Conventions

### Table Design Pattern

```sql
CREATE TABLE media (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- domain columns
    title TEXT NOT NULL,
    -- ...

    -- unique business keys where applicable
    CONSTRAINT media_path_unique UNIQUE (file_path)
);

-- indexes on foreign keys and common query patterns
CREATE INDEX idx_media_library_id ON media (library_id);
```

### Naming Conventions

| Element | Convention | Example |
|---|---|---|
| Table names | `snake_case`, plural | `media_items`, `users`, `libraries` |
| Column names | `snake_case` | `created_at`, `file_path` |
| Primary key | `id` | `id UUID DEFAULT uuidv7() PRIMARY KEY` |
| Foreign key | `{referenced_table_singular}_id` | `library_id UUID REFERENCES libraries(id)` |
| Join tables | `{table_a}_{table_b}` | `media_genres` |
| Indexes | `idx_{table}_{columns}` | `idx_media_items_library_id` |
| Unique constraints | `{table}_{column}_unique` | `users_email_unique` |
| Timestamps | `created_at`, `updated_at` | `TIMESTAMPTZ` |

### Foreign Keys

All foreign keys use `UUID` type referencing the parent's UUIDv7 primary key:

```sql
library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE
```

### Default Columns

Every table includes:

- `id` — UUIDv7 primary key
- `created_at` — Virtual or stored generated column from `uuid_extract_timestamp(id)`
- `updated_at` — `TIMESTAMPTZ NOT NULL DEFAULT now()`, updated via trigger or application logic

---

## Core Media Domain: Schema Design

### Inheritance Strategy: Class Table Inheritance (CTI)

Media items use **Class Table Inheritance** — a shared `media_items` parent table with type-specific child tables (`movies`, `series`, `seasons`, `episodes`). Each child table shares the same primary key as the parent via a foreign key.

### Why CTI Over Alternatives

| Approach | Pros | Cons | Verdict |
|---|---|---|---|
| **Single Table Inheritance** | Simplest schema; no JOINs; one table for all queries | Wide sparse table; many NULLs; no per-type constraint enforcement; grows unwieldy | Rejected |
| **Class Table Inheritance** | No NULL waste; strong referential integrity per type; clean separation; 3NF normalized | Requires JOINs to reconstruct full objects | **Selected** |
| **Concrete Table Inheritance** | Self-contained tables; no JOINs | Column duplication; schema changes applied everywhere; UNIONs for cross-type queries | Rejected |

CTI was selected because:
- Movies don't have `season_number`; seasons don't have `runtime_seconds` — no NULL waste
- An episode MUST have `series_id` and `season_id` — enforced by FK constraints that STI can't express
- Query patterns are type-specific (browsing movies vs browsing TV shows) — JOIN cost is negligible
- Adding new media types means adding a new child table, no ALTER on the parent

### Metadata Storage: Hybrid (Columns + JSONB)

| Storage | Used For | Rationale |
|---|---|---|
| **Real columns** | Frequently queried, filtered, sorted, or displayed fields (title, premiere_date, content_rating, rating, provider IDs) | Best query performance; indexable; constraint-enforced |
| **JSONB `metadata`** | Provider-specific raw data, rare attributes, evolving fields (full TMDB/TVDB/Trakt response, taglines, alt titles, production companies) | Schema flexibility without migrations; GIN-indexable when needed |

Promote JSONB fields to real columns when they become frequently queried.

### Entity-Relationship Overview

```
 libraries ──< library_paths
 libraries ──< media_items >── movies          [search_vector: FTS index]
                    │
                    └── series ──< seasons ──< episodes
                                  │
 media_items >── media_files
              >── subtitle_files
              >── artwork
              >── media_genres >── genres
              >── media_tags >── tags
              >── media_credits >── people
              >── artwork_overlay_state

 collections ──< collection_items >── media_items
 collections ──> artwork (poster, backdrop)
 collection_templates (standalone)

 users ──< trakt_accounts                      [soft delete: deleted_at]
        ──< trakt_sync_state >── media_items
        ──< play_sessions >── media_items      [partitioned by month]
                           >── play_session_streams (1:1)
                           >── play_events      [partitioned by month]
        ──< user_trust_events
        ──< user_trust_scores (1:1)
        ──< user_item_data >── media_items
        ──< bookmarks >── media_items
        ──< playlists ──< playlist_items >── media_items  [soft delete: deleted_at]
        ──< user_passkeys
        ──< user_totp (0..1)
        ──< user_capabilities
        ──< user_library_access >── libraries  [soft delete: deleted_at]
        ──< user_sessions
        ──< api_keys
        ──< invitations (created_by)
        ──< notifications >── notification_types
         ──< user_notification_preferences >── notification_types
         ──< user_push_devices
        ──< client_network_reports >── play_sessions
        ──< qoe_reports >── play_sessions

device_profiles ──< device_capability_tests

scheduled_tasks ──< scheduled_task_runs

server_config (single row)

audit_log (partitioned by month, trigger-based)  [tracks: users, libraries, config, etc.]
```

### Schema DDL

#### Libraries

```sql
CREATE TABLE libraries (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    media_type TEXT NOT NULL CHECK (media_type IN ('movies', 'tvshows')),
    root_path TEXT NOT NULL,
    scan_enabled BOOLEAN NOT NULL DEFAULT true,
    scan_interval_seconds INT NOT NULL DEFAULT 86400,
    metadata_language TEXT NOT NULL DEFAULT 'en',
    metadata JSONB NOT NULL DEFAULT '{}',
    last_scan_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX libraries_slug_active ON libraries (slug) WHERE deleted_at IS NULL;
```

`metadata` JSONB for per-library scanning configuration. Example: `{ "scan_watch_enabled": true, "scan_realtime_fallback": "poll", "scan_poll_interval_seconds": 300, "scan_exclude_patterns": ["*.tmp", ".DS_Store", "Thumbs.db", "._*"], "scan_season_detection": "directory" }`. Full schema documented in [MEDIA_SCANNING.md](MEDIA_SCANNING.md).

`last_scan_at` is updated by the scanner after each completed scan. Used by the admin UI to show scan freshness and by the scheduled task system to determine scan staleness.

Library folder structure, naming conventions, and sub-folder-as-library design are documented in [LIBRARY_ORGANIZATION.md](LIBRARY_ORGANIZATION.md). That document defines the expected filesystem hierarchy, reserved folder names, and scanner traversal behavior.

#### Library Paths

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

Enables a single library to span multiple directories or disks. A library must have at least one path (`is_default = true`). Each path is scanned independently. `scan_enabled` per path allows disabling scanning on offline/network drives. Full design documented in [LIBRARY_ORGANIZATION.md](LIBRARY_ORGANIZATION.md).

Migration from `libraries.root_path`:

```sql
INSERT INTO library_paths (library_id, path, is_default, scan_enabled, last_scan_at)
SELECT id, root_path, true, scan_enabled, last_scan_at
FROM libraries;
```

`libraries.root_path` and `libraries.last_scan_at` columns are retained for backward compatibility but deprecated. The scanner reads from `library_paths`.

#### Media Items (Parent Table — CTI)

```sql
CREATE TABLE media_items (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    type TEXT NOT NULL CHECK (type IN ('movie', 'series', 'season', 'episode')),

    title TEXT NOT NULL,
    sort_title TEXT NOT NULL,
    original_title TEXT,
    overview TEXT,

    premiere_date DATE,
    end_date DATE,
    content_rating TEXT,
    runtime_seconds INT,

    tmdb_id BIGINT,
    imdb_id TEXT,
    tvdb_id BIGINT,
    trakt_id BIGINT,

    rating_average REAL,
    rating_vote_count INT,

    search_vector TSVECTOR,
    metadata JSONB NOT NULL DEFAULT '{}',

    match_state TEXT NOT NULL DEFAULT 'confirmed'
        CHECK (match_state IN ('unmatched', 'auto_matched', 'confirmed', 'manual')),
    identification_source TEXT
        CHECK (identification_source IS NULL OR identification_source IN (
            'media_match', 'nfo', 'provider_id_tag', 'filename_parse', 'manual'
        ))
);

CREATE INDEX idx_media_items_library_id ON media_items (library_id);
CREATE INDEX idx_media_items_type ON media_items (type);
CREATE INDEX idx_media_items_sort_title ON media_items (sort_title);
CREATE INDEX idx_media_items_premiere_date ON media_items (premiere_date DESC NULLS LAST);
CREATE INDEX idx_media_items_tmdb_id ON media_items (tmdb_id) WHERE tmdb_id IS NOT NULL;
CREATE INDEX idx_media_items_imdb_id ON media_items (imdb_id) WHERE imdb_id IS NOT NULL;
CREATE INDEX idx_media_items_tvdb_id ON media_items (tvdb_id) WHERE tvdb_id IS NOT NULL;
CREATE INDEX idx_media_items_trakt_id ON media_items (trakt_id) WHERE trakt_id IS NOT NULL;
CREATE INDEX idx_media_items_match_state ON media_items (match_state) WHERE match_state != 'confirmed';
CREATE INDEX idx_media_items_metadata ON media_items USING GIN (metadata jsonb_path_ops);
CREATE INDEX idx_media_items_search ON media_items USING GIN (search_vector) WHERE search_vector IS NOT NULL;
```

`match_state` tracks identification confidence: `unmatched` (failed all layers, needs admin action), `auto_matched` (Layer 4 API search succeeded but below auto-confirm threshold), `confirmed` (auto-confirmed or admin confirmed), `manual` (admin manually selected match).

`identification_source` records which pipeline layer succeeded: `media_match` (`.media-match` sidecar file), `nfo` (NFO XML file), `provider_id_tag` (`{tmdb-XXX}` in folder/filename), `filename_parse` (structured parse + API search), `manual` (admin selected via UI). NULL for pre-existing items. Full pipeline documented in [LIBRARY_ORGANIZATION.md](LIBRARY_ORGANIZATION.md).

`metadata` JSONB stores the full provider API response and identification details. Example: `{ "identification": { "source": "media_match", "matched_at": "2026-05-31T10:00:00Z", "confidence": 100 }, "scan": { "discovered_via": "watch", "first_seen": "2026-05-31T09:59:00Z" } }`.

#### Movies (CTI Child)

```sql
CREATE TABLE movies (
    id UUID PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

#### Series (CTI Child)

```sql
CREATE TABLE series (
    id UUID PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    status TEXT NOT NULL DEFAULT 'continuing' CHECK (status IN ('continuing', 'ended', 'upcoming', 'canceled'))
);
```

#### Seasons (CTI Child)

```sql
CREATE TABLE seasons (
    id UUID PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    series_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    season_number INT NOT NULL,

    UNIQUE(series_id, season_number)
);

CREATE INDEX idx_seasons_series_id ON seasons (series_id);
```

#### Episodes (CTI Child)

```sql
CREATE TABLE episodes (
    id UUID PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    series_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    season_id UUID NOT NULL REFERENCES seasons(id) ON DELETE CASCADE,
    episode_number INT,
    absolute_episode_number INT,

    UNIQUE(season_id, episode_number)
);

CREATE INDEX idx_episodes_series_id ON episodes (series_id);
CREATE INDEX idx_episodes_season_id ON episodes (season_id);
```

#### Media Files (Physical File Tracking)

```sql
CREATE TABLE media_files (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    file_path TEXT NOT NULL,
    file_size BIGINT NOT NULL,
    file_hash TEXT,
    file_modified_at TIMESTAMPTZ,

    container_format TEXT NOT NULL,

    video_codec TEXT,
    video_resolution TEXT,
    video_bitrate INT,
    video_dynamic_range TEXT,
    video_frame_rate NUMERIC(6,3),

    audio_codec TEXT,
    audio_channels INT,
    audio_language TEXT,
    audio_bitrate INT,

    runtime_seconds INT NOT NULL,

    last_scanned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    is_healthy BOOLEAN NOT NULL DEFAULT true,

    additional_streams JSONB DEFAULT '{}',

    UNIQUE(media_item_id, file_path)
);

CREATE INDEX idx_media_files_media_item_id ON media_files (media_item_id);
CREATE INDEX idx_media_files_video_resolution ON media_files (video_resolution);
CREATE INDEX idx_media_files_file_path ON media_files (file_path);
```

`video_dynamic_range` values: `sdr`, `hdr10`, `dolby_vision_p5`, `dolby_vision_p7`, `dolby_vision_p8.1`, `dolby_vision_p8.4`, `hdr10_plus`, `hlg`. Derived from ffprobe color_transfer + DV side data during Phase 3 probe. Full format catalog documented in [VIDEO_FORMATS.md](VIDEO_FORMATS.md).

`additional_streams` stores full ffprobe stream data including HDR metadata, DV profile/level/compatibility mode, HDR10+ detection, bit depth, and chroma subsampling for all streams. Used by the transcoding decision engine when detailed format inspection is needed.

#### Subtitle Files

```sql
CREATE TABLE subtitle_files (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    file_path TEXT NOT NULL,
    language TEXT NOT NULL,
    subtitle_type TEXT NOT NULL CHECK (subtitle_type IN ('embedded', 'external', 'fetched')),
    is_forced BOOLEAN NOT NULL DEFAULT false,
    is_hearing_impaired BOOLEAN NOT NULL DEFAULT false,
    source_provider TEXT,

    UNIQUE(media_item_id, file_path)
);

CREATE INDEX idx_subtitle_files_media_item_id ON subtitle_files (media_item_id);
```

#### Subtitle OCR Cache

Cached OCR results for image subtitles (PGS/VobSub). One-time conversion from bitmap subtitles to SRT text. Re-used on every playback, never re-run unless source file changes. Full design documented in [SUBTITLES.md](SUBTITLES.md).

```sql
CREATE TABLE subtitle_ocr_cache (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    subtitle_stream_index INT NOT NULL,
    source_hash TEXT NOT NULL,

    ocr_engine TEXT NOT NULL CHECK (ocr_engine IN ('paddleocr', 'tesseract')),
    confidence_score NUMERIC(3,2),

    srt_content TEXT NOT NULL,

    UNIQUE(media_item_id, subtitle_stream_index)
);

CREATE INDEX idx_subtitle_ocr_cache_media_item_id ON subtitle_ocr_cache (media_item_id);
```

`source_hash` — Blake3 hash of the subtitle stream data. If the source file changes (re-rip, different version), the hash changes and OCR is re-run.

`confidence_score` — average OCR confidence across all frames. Below 0.80, the admin is warned that the OCR result may contain errors.

#### Subtitle Sync Data

Stores per-subtitle-track synchronization corrections. Applied at delivery time with zero runtime cost (timestamp arithmetic only). Full design documented in [SUBTITLES.md](SUBTITLES.md).

```sql
CREATE TABLE subtitle_sync_data (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    subtitle_file_id UUID NOT NULL REFERENCES subtitle_files(id) ON DELETE CASCADE,

    sync_method TEXT NOT NULL CHECK (sync_method IN ('voice_activity', 'fps_adjust', 'manual')),
    offset_ms INT NOT NULL DEFAULT 0,
    confidence NUMERIC(3,2),

    fps_source NUMERIC(8,4),
    fps_target NUMERIC(8,4),

    UNIQUE(media_item_id, subtitle_file_id, sync_method)
);

CREATE INDEX idx_subtitle_sync_data_media_item_id ON subtitle_sync_data (media_item_id);
```

`confidence` — for voice activity alignment, how well the speech timeline correlates with subtitle timestamps. Below 0.60, the offset is not applied automatically.

`fps_source` / `fps_target` — populated only for `fps_adjust` sync method. The ratio `fps_target / fps_source` is the scale factor applied to all timestamps.

#### Genres & Tags

```sql
CREATE TABLE genres (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    name TEXT NOT NULL UNIQUE,
    slug TEXT NOT NULL UNIQUE
);

CREATE TABLE media_genres (
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    genre_id UUID NOT NULL REFERENCES genres(id) ON DELETE CASCADE,

    PRIMARY KEY (media_item_id, genre_id)
);

CREATE TABLE tags (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    name TEXT NOT NULL UNIQUE
);

CREATE TABLE media_tags (
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    tag_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,

    PRIMARY KEY (media_item_id, tag_id)
);
```

#### People & Credits

```sql
CREATE TABLE people (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    name TEXT NOT NULL,
    sort_name TEXT NOT NULL,
    tmdb_person_id BIGINT,
    imdb_person_id TEXT,
    trakt_person_id BIGINT,
    image_url TEXT,
    metadata JSONB DEFAULT '{}'
);

CREATE TABLE media_credits (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    person_id UUID NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    credit_type TEXT NOT NULL CHECK (credit_type IN ('cast', 'crew')),
    role TEXT,
    department TEXT,
    "order" INT NOT NULL DEFAULT 0,

    UNIQUE(media_item_id, person_id, credit_type, role)
);

CREATE INDEX idx_media_credits_media_item_id ON media_credits (media_item_id);
CREATE INDEX idx_media_credits_person_id ON media_credits (person_id);
```

#### Artwork

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

    UNIQUE(media_item_id, artwork_type, "order")
);

CREATE INDEX idx_artwork_media_item_id ON artwork (media_item_id);
```

#### Trakt.tv Integration (Native)

Trakt.tv is a first-class integration, not a plugin. Each user can link their own Trakt account. Watched history is bidirectional today; ratings and collection are pulled into Duskcue, while watchlist sync and additional push categories remain follow-up work.

##### API Summary (as of May 2026)

| Aspect | Detail |
|---|---|
| **Base URL** | `https://api.trakt.tv` |
| **Auth** | OAuth2 (standard + device code flow for headless clients) |
| **Access token** | 90-day expiry, refreshable |
| **Rate limits** | POST: 1/sec authed; GET: 1000/5min authed; GET: 1000/5min unauthed |
| **Pagination** | Required on most endpoints; max page size reducing to 250 on June 15, 2026 |
| **Media IDs** | `trakt` (int), `slug` (text), `tmdb` (int), `imdb` (text), `tvdb` (int) |
| **Trakt API categories** | watched history, watchlist, collection, ratings, favorites, playback progress |
| **User limits** | Free: 100K history, 250 watchlist, 1K digital library; VIP: higher caps |

##### Design Decisions

- **Per-user sync** — Every sync state row belongs to a user. A shared media item can have independent watch states for each user who watched it.
- **Two-table model** — `trakt_accounts` stores OAuth credentials; `trakt_sync_state` tracks per-item sync status. This separates auth concerns from sync concerns.
- **Incremental sync** — The `synced_at` timestamp on each row enables incremental pulls instead of full re-syncs.
- **Minimal users table** — A stub `users` table is introduced here because Trakt is per-user. The full user/auth domain schema will be expanded in a later section.

##### Users (Stub — Expanded in Auth Domain)

The `users` table is introduced here as a minimal stub because Trakt integration requires per-user identity. The full schema with authentication, roles, capabilities, and session management is defined in the **User & Authentication Domain** section below.

```sql
CREATE TABLE users (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    username TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    email TEXT UNIQUE,
    is_active BOOLEAN NOT NULL DEFAULT true,
    metadata JSONB NOT NULL DEFAULT '{}'
);
```

##### Trakt Accounts (Per-User OAuth)

```sql
CREATE TABLE trakt_accounts (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,

    trakt_username TEXT NOT NULL,
    trakt_user_id BIGINT NOT NULL,

    access_token TEXT NOT NULL,
    refresh_token TEXT NOT NULL,
    token_expires_at TIMESTAMPTZ NOT NULL,
    token_scope TEXT,

    last_full_sync_at TIMESTAMPTZ,
    last_sync_attempt_at TIMESTAMPTZ,
    last_sync_error TEXT,
    sync_enabled BOOLEAN NOT NULL DEFAULT true,

    sync_watched BOOLEAN NOT NULL DEFAULT true,
    sync_watchlist BOOLEAN NOT NULL DEFAULT false,
    sync_collection BOOLEAN NOT NULL DEFAULT true,
    sync_ratings BOOLEAN NOT NULL DEFAULT true,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_trakt_accounts_user_id ON trakt_accounts (user_id);
CREATE INDEX idx_trakt_accounts_trakt_user_id ON trakt_accounts (trakt_user_id);
```

`metadata` stores Trakt user profile data (VIP status, avatar, limits, etc.) and sync configuration details.

##### Trakt Sync State (Per-User, Per-Item)

```sql
CREATE TABLE trakt_sync_state (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    trakt_id BIGINT,
    trakt_history_id BIGINT,

    is_watched BOOLEAN NOT NULL DEFAULT false,
    watched_at TIMESTAMPTZ,
    plays INT NOT NULL DEFAULT 0,

    is_in_watchlist BOOLEAN NOT NULL DEFAULT false,
    watchlist_added_at TIMESTAMPTZ,

    is_in_collection BOOLEAN NOT NULL DEFAULT false,
    collected_at TIMESTAMPTZ,

    rating INT CHECK (rating BETWEEN 1 AND 10),
    rated_at TIMESTAMPTZ,

    sync_error TEXT,
    synced_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE(user_id, media_item_id)
);

CREATE INDEX idx_trakt_sync_state_user_id ON trakt_sync_state (user_id);
CREATE INDEX idx_trakt_sync_state_media_item_id ON trakt_sync_state (media_item_id);
CREATE INDEX idx_trakt_sync_state_trakt_id ON trakt_sync_state (trakt_id);
CREATE INDEX idx_trakt_sync_state_synced_at ON trakt_sync_state (synced_at DESC);
CREATE INDEX idx_trakt_sync_state_watched ON trakt_sync_state (user_id) WHERE is_watched = true;
CREATE INDEX idx_trakt_sync_state_watchlist ON trakt_sync_state (user_id) WHERE is_in_watchlist = true;
```

`trakt_id` is nullable because Trakt accepts TMDB, IMDb, and TVDB identifiers on sync writes. A row may therefore represent a confirmed local item even when the source `media_items` row has no numeric Trakt identifier.

`sync_watchlist` is retained for migration compatibility but defaults to `false` and is no longer part of the public account or sync-settings contract until watchlist sync is implemented.

---

## Activity & Analytics Domain: Schema Design

### Overview

A built-in Tautulli-equivalent analytics dashboard for monitoring, session tracking, stream analytics, and user activity — no external tools required.

### Key Requirements (from research)

| Requirement | Source | Design Decision |
|---|---|---|
| Real-time session monitoring | Tautulli, Tracearr | In-memory session state via server, persisted on session end |
| Full watch history per user | Tautulli, Trakt | `play_sessions` table, append-only, range-partitioned by month |
| Transcode vs direct play analytics | Tautulli, Tracearr | `stream_decisions` enum; original vs stream columns on `play_sessions` |
| Bandwidth tracking | Tautulli | `bandwidth_bps` column on active sessions |
| IP geolocation & location type (LAN/WAN) | Tautulli, Tracearr | `ip_address INET`, `location_type`, `geo_*` columns |
| Platform/device/player tracking | Tautulli | `client_name`, `client_platform`, `client_product`, `client_version` |
| Per-user statistics | Tautulli, Tracearr | Materialized views aggregating from `play_sessions` |
| Library growth & storage analytics | Tracearr | Aggregation queries against `media_files` + `media_items` |
| Sharing detection & trust scoring | Tracearr | `user_trust_scores` table, rule engine in application layer |
| High-volume time-series data | Tracearr, PG partitioning research | Range partitioning by month on `play_sessions` |

### Why Not TimescaleDB?

Tracearr uses TimescaleDB (a PostgreSQL extension) for session history. However:
- Our server is a single monolith, not a multi-server monitoring platform
- PG18 native range partitioning provides the same partition pruning benefit without an extension dependency
- We target Synology NAS and Docker — fewer dependencies is better
- TimescaleDB can be added later if needed (extension, not a fork)

**Decision:** Use PG18 native range partitioning on `play_sessions` with monthly partitions.

### Partitioning Strategy

| Table | Strategy | Key | Granularity | Rationale |
|---|---|---|---|---|
| `play_sessions` | Range | `started_at` | Monthly | High-volume append-only; queries always filter by date; enables instant partition drop for retention |

Application-level partition management: create next month's partition before the month starts via a scheduled task.

### Entity-Relationship Overview

```
users ──< play_sessions >── media_items
                      >── play_session_streams (1:1)
                      >── play_events

users ──< user_trust_events
       ──< user_trust_scores (1:1)
```

### Schema DDL

#### Play Sessions (Range-Partitioned by Month)

The core analytics table. Every completed or in-progress playback session gets a row here. Active sessions are written on start and updated on stop; the row is never deleted.

```sql
CREATE TABLE play_sessions (
    id UUID DEFAULT uuidv7(),
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,

    started_at TIMESTAMPTZ NOT NULL,
    stopped_at TIMESTAMPTZ,
    paused_seconds INT NOT NULL DEFAULT 0,
    duration_seconds INT NOT NULL DEFAULT 0,

    ip_address INET,
    location_type TEXT CHECK (location_type IN ('lan', 'wan', 'relay')),
    geo_city TEXT,
    geo_region TEXT,
    geo_country TEXT,
    geo_lat REAL,
    geo_lon REAL,

    client_name TEXT NOT NULL,
    client_product TEXT,
    client_platform TEXT,
    client_version TEXT,
    client_device TEXT,

    is_secure BOOLEAN NOT NULL DEFAULT false,
    bandwidth_bps BIGINT,
    quality_profile TEXT,

    stream_decision TEXT NOT NULL CHECK (stream_decision IN ('direct_play', 'direct_stream', 'transcode')),
    percent_complete REAL,
    plays_in_session INT NOT NULL DEFAULT 1,

    metadata JSONB NOT NULL DEFAULT '{}'
) PARTITION BY RANGE (started_at);

CREATE INDEX idx_play_sessions_user_id ON play_sessions (user_id);
CREATE INDEX idx_play_sessions_id ON play_sessions (id);
CREATE INDEX idx_play_sessions_media_item_id ON play_sessions (media_item_id);
CREATE INDEX idx_play_sessions_library_id ON play_sessions (library_id);
CREATE INDEX idx_play_sessions_started_at ON play_sessions (started_at DESC);
CREATE INDEX idx_play_sessions_stream_decision ON play_sessions (stream_decision);
CREATE INDEX idx_play_sessions_ip_address ON play_sessions (ip_address);
CREATE INDEX idx_play_sessions_location_type ON play_sessions (location_type);
CREATE INDEX idx_play_sessions_metadata ON play_sessions USING GIN (metadata jsonb_path_ops);
```

Example monthly partition:
```sql
CREATE TABLE play_sessions_2026_06 PARTITION OF play_sessions
    FOR VALUES FROM ('2026-06-01') TO ('2026-07-01');
```

`metadata` stores additional session context (Trakt sync state, notification history, buffer counts, session grouping references).

#### Play Session Streams (1:1 — Technical Stream Details)

Stores the full original media vs transcode vs output stream comparison for each session. Kept as a separate table to keep the core `play_sessions` row narrow for common queries.

`play_session_id` is an application-level UUID join to the partitioned `play_sessions` table. PostgreSQL cannot enforce a simple foreign key to `play_sessions(id)` unless the parent has a unique constraint that also includes the range partition key.

```sql
CREATE TABLE play_session_streams (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    play_session_id UUID NOT NULL UNIQUE,

    source_video_codec TEXT,
    source_video_resolution TEXT,
    source_video_bitrate INT,
    source_video_dynamic_range TEXT,
    source_video_frame_rate NUMERIC(6,3),
    source_video_scan_type TEXT,
    source_video_bit_depth INT,

    source_audio_codec TEXT,
    source_audio_channels INT,
    source_audio_bitrate INT,
    source_audio_language TEXT,

    source_container TEXT,
    source_total_bitrate INT,

    transcode_protocol TEXT,
    transcode_container TEXT,
    transcode_video_codec TEXT,
    transcode_audio_codec TEXT,
    transcode_audio_channels INT,
    transcode_video_width INT,
    transcode_video_height INT,
    transcode_hw_decode TEXT,
    transcode_hw_encode TEXT,
    transcode_hw_accelerated BOOLEAN NOT NULL DEFAULT false,

    stream_video_codec TEXT,
    stream_video_resolution TEXT,
    stream_video_bitrate INT,
    stream_video_dynamic_range TEXT,
    stream_video_frame_rate NUMERIC(6,3),

    stream_audio_codec TEXT,
    stream_audio_channels INT,
    stream_audio_bitrate INT,
    stream_audio_language TEXT,

    stream_container TEXT,
    stream_total_bitrate INT,

    subtitle_codec TEXT,
    subtitle_language TEXT,
    subtitle_forced BOOLEAN NOT NULL DEFAULT false,

    additional_streams JSONB DEFAULT '{}'
);

CREATE INDEX idx_play_session_streams_play_session_id ON play_session_streams (play_session_id);
```

`additional_streams` stores full FFprobe-like stream data for all streams (multiple audio tracks, subtitle tracks, etc.) when detailed debugging is needed.

#### Play Events (Append-Only Event Log)

Granular playback events within a session — play, pause, stop, resume, buffer, seek, error. Used for timeline visualization, buffer analytics, and binge detection.

```sql
CREATE TABLE play_events (
    id UUID DEFAULT uuidv7(),
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    play_session_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    event_type TEXT NOT NULL CHECK (event_type IN (
        'play', 'pause', 'stop', 'resume', 'buffer_start', 'buffer_end',
        'seek', 'error', 'transcode_change', 'heartbeat'
    )),
    event_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    position_seconds INT,
    details JSONB DEFAULT '{}'
) PARTITION BY RANGE (event_at);

CREATE INDEX idx_play_events_play_session_id ON play_events (play_session_id);
CREATE INDEX idx_play_events_id ON play_events (id);
CREATE INDEX idx_play_events_user_id ON play_events (user_id);
CREATE INDEX idx_play_events_event_type ON play_events (event_type);
CREATE INDEX idx_play_events_event_at ON play_events (event_at DESC);
```

Example monthly partition:
```sql
CREATE TABLE play_events_2026_06 PARTITION OF play_events
    FOR VALUES FROM ('2026-06-01') TO ('2026-07-01');
```

`details` stores event-specific data (seek target, error message, transcode reason, buffer duration).

#### User Trust Events (Sharing Detection)

Records trust-affecting events detected by the rule engine. Supports the Tracearr-style sharing detection model.

```sql
CREATE TABLE user_trust_events (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    play_session_id UUID,

    rule_type TEXT NOT NULL CHECK (rule_type IN (
        'impossible_travel', 'simultaneous_locations', 'device_velocity',
        'concurrent_streams', 'geo_restriction', 'account_inactivity'
    )),
    severity TEXT NOT NULL CHECK (severity IN ('low', 'medium', 'high')),
    score_impact INT NOT NULL DEFAULT 0,
    details JSONB NOT NULL DEFAULT '{}',
    acknowledged BOOLEAN NOT NULL DEFAULT false,
    acknowledged_at TIMESTAMPTZ
);

CREATE INDEX idx_user_trust_events_user_id ON user_trust_events (user_id);
CREATE INDEX idx_user_trust_events_rule_type ON user_trust_events (rule_type);
CREATE INDEX idx_user_trust_events_created_at ON user_trust_events (created_at DESC);
CREATE INDEX idx_user_trust_events_unack ON user_trust_events (user_id) WHERE acknowledged = false;
```

#### User Trust Scores (1:1 Per User)

Current trust state per user. Updated when trust events fire.

```sql
CREATE TABLE user_trust_scores (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,

    score INT NOT NULL DEFAULT 100 CHECK (score BETWEEN 0 AND 100),
    total_violations INT NOT NULL DEFAULT 0,
    last_violation_at TIMESTAMPTZ,
    last_good_session_at TIMESTAMPTZ,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_user_trust_scores_score ON user_trust_scores (score);
```

Score starts at 100 and decreases with violations. `metadata` stores rule configuration overrides per user.

#### User Location History (Per User Per Country)

Tracks which countries each user has streamed from, powering the user baseline suppression layer for impossible travel detection. Updated automatically during play session geolocation enrichment.

```sql
CREATE TABLE user_location_history (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    country_code TEXT NOT NULL,

    first_seen_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    session_count INT NOT NULL DEFAULT 1,

    CONSTRAINT uq_user_location_country UNIQUE (user_id, country_code)
);

CREATE INDEX idx_user_location_history_user_id ON user_location_history (user_id);
CREATE INDEX idx_user_location_history_last_seen ON user_location_history (last_seen_at DESC);
```

`country_code` — ISO 3166-1 alpha-2 (e.g., `US`, `GB`, `JP`). Derived from GeoIP lookup during play session enrichment.

`session_count` — incremented on each session from this country. Distinguishes regular travelers from one-off appearances.

The unique constraint on `(user_id, country_code)` means each user has at most one row per country. On repeat visits, `last_seen_at` and `session_count` are updated; `first_seen_at` is preserved.

Old rows are never deleted — the full history provides value for the admin analytics dashboard. The suppression layer considers only rows where `last_seen_at` is within the last 90 days.

---

## Classifarr Integration: Schema Design

### What is Classifarr?

[Classifarr](https://github.com/cloudbyday90/Classifarr) (v0.47.1-beta as of May 2026) is an AI- and RAG-powered media classification and routing service. It sits between request sources (Overseerr/Jellyseerr webhooks, manual submissions) and your automation stack (Radarr/Sonarr), using metadata, policy rules, and AI/RAG signals to auto-route media to the correct library destination.

### The Flow

```
Overseerr/Jellyseerr ──request──> Classifarr ──route──> Radarr/Sonarr ──download──> Our Server
                                         │
                                         └── reads library state from ──> Our Server API
```

Our server is a **passive data source** for Classifarr. Classifarr queries our API to see what libraries exist and what's already in them — the same way it reads from Plex/Jellyfin/Emby. We do not call Classifarr, receive webhooks from Classifarr, or store classification decisions. Our only role is answering Classifarr's queries accurately so it can make better routing decisions.

### Why Native Integration?

Currently Classifarr supports Plex, Jellyfin, and Emby as media sources. With native support:

1. **Our server exposes a Classifarr-compatible media source API** — Classifarr queries our libraries, items, and metadata directly, the same way it queries Plex/Jellyfin
2. **Classifarr sees our library structure in real-time** — no stale sync state; it queries us live when classifying
3. **No separate import needed** — Classifarr connects to us as a first-class Duskcue

### How Classifarr Uses Our Data

Classifarr v0.37+ uses a Policy Engine — formula-first, AI-second. Our server's data is used in the **authoritative match short-circuit** step: when a requested item already exists in one of our libraries, Classifarr routes the request to the same destination without needing AI or scoring. The richer and more accurate our API responses, the more often Classifarr can short-circuit with high confidence.

### Our Media Source API

Our server exposes read-only API endpoints that Classifarr queries. These are the only touchpoint between Classifarr and our server.

| Endpoint | Purpose | Used By Classifarr |
|---|---|---|
| `GET /api/v1/libraries` | List all libraries with types and item counts | Map libraries to Radarr/Sonarr instances; authoritative match lookup |
| `GET /api/v1/libraries/:id/items` | Paginated items in a library | See what's already classified where |
| `GET /api/v1/items/:id` | Full item metadata (title, genres, keywords, certifications, studios, year, runtime, ratings, TMDB/IMDB IDs) | Classification signals for preset scoring and RAG |
| `GET /api/v1/items/:id/artwork` | Item poster/backdrop images | Optional CLIP image embeddings for visual similarity |

All endpoints require authentication via an API key (see `api_keys` table in Auth domain). The server admin creates an API key with read-only capabilities and configures it in Classifarr's media source settings.

### Configuration

Stored in `server_config.integrations` JSONB column (see System Domain). Configuration:

- `classifarr_enabled` (boolean) — enables or disables the Classifarr-compatible API endpoints

That's it. There is no webhook secret, no sync state, no decision storage. Our server answers queries; Classifarr does the rest.

### Schema DDL

No dedicated tables. The Classifarr integration uses existing tables (`libraries`, `media_items`, `artwork`) exposed through read-only API endpoints. Access is controlled by the existing `api_keys` table with read-only capability scoping.

---

## Playback Domain: Schema Design

### Overview

The playback domain handles per-user watch state, resume positions, bookmarks, and playlists — the data that powers "Continue Watching," "Up Next," and personalized library views.

### Key Requirements

| Requirement | Source | Design Decision |
|---|---|---|
| Resume playback where user left off | Plex, Jellyfin | `resume_position_ms` on `user_item_data` |
| Track watched/unwatched per user per item | Plex, Jellyfin | `is_watched`, `play_count` on `user_item_data` |
| Per-user favorites | Jellyfin | `is_favorite` on `user_item_data` |
| Per-user ratings | Plex, Jellyfin | `user_rating` on `user_item_data` |
| Remember preferred audio/subtitle tracks per item | Jellyfin | `audio_stream_index`, `subtitle_stream_index` on `user_item_data` |
| Remember which version was played (multi-version) | Plex | `last_played_media_file_id` on `user_item_data` |
| Continue Watching (in-progress items) | Plex, Jellyfin | Query over `user_item_data` with resume position + recency filter |
| Up Next (next unwatched TV episode) | Plex "On Deck" | Query over episode watch states per series |
| User bookmarks (timestamp markers) | Common request | Dedicated `bookmarks` table |
| User playlists (ordered collections) | Plex, Jellyfin | `playlists` + `playlist_items` tables |
| Smart playlists (auto-populated) | Plex, Jellyfin | `is_smart` + `smart_filter` JSONB on `playlists` |

### Design Decisions

- **Separate from `trakt_sync_state`** — `user_item_data` is our local ground truth for watch state. `trakt_sync_state` tracks what's been synced with Trakt. They share overlapping concepts (watched, play count) but have different update patterns, lifecycles, and additional fields. The application layer coordinates both (e.g. marking watched updates `user_item_data` and queues a Trakt sync).
- **Single `user_item_data` table** — One row per user per media item holds all watch state. This matches both Plex's `metadata_item_settings` (view_count, view_offset) and Jellyfin's `UserData` entity (PlaybackPositionTicks, PlayCount, IsFavorite, Played, LastPlayedDate). A single table avoids JOINs for the most common query patterns.
- **Resume position in milliseconds** — Integer milliseconds matches FFmpeg's time base and is the standard for media players. Plex uses milliseconds; Jellyfin uses ticks (10,000 per ms) but that's unnecessarily granular for resume positions.
- **`last_played_media_file_id`** — For multi-version items (e.g. 4K and 1080p of the same movie), remembering which version was last played avoids surprising the user with a different quality on resume.
- **Playlist ordering via integer position** — Simple, supports reordering, avoids gaps from deletions (renumbered periodically or on reorder).

### Relationship to Activity Domain

`user_item_data` stores the *current state* (is it watched? where did I stop?). `play_sessions` in the Activity domain stores the *history* (every individual playback session). They are complementary:

- When a play session starts: read `user_item_data.resume_position_ms` to seek
- During playback: periodically update `user_item_data.resume_position_ms`
- When a play session ends: update `user_item_data` (play_count++, is_watched, clear resume)
- The completed session is written to `play_sessions` for analytics

### Continue Watching Algorithm

**Purpose:** Show items the user started but hasn't finished, sorted by recency.

```sql
SELECT mi.*, uid.resume_position_ms, uid.last_played_at
FROM user_item_data uid
JOIN media_items mi ON uid.media_item_id = mi.id
WHERE uid.user_id = $1
  AND uid.is_watched = false
  AND uid.resume_position_ms > 0
  AND uid.last_played_at > now() - $2  -- configurable window, default 4 weeks
ORDER BY uid.last_played_at DESC
LIMIT 20;
```

**Rules:**
- Only unwatched items with a resume position (partially played)
- Filtered by configurable recency window (default 4 weeks)
- For TV: shows the in-progress *episode*, not the series
- Exclude items with no available media files (deleted/offline)

### Up Next Algorithm (TV Shows)

**Purpose:** For each TV series the user is watching, show the next unwatched episode.

```
For each series where user has watched ≥ 1 episode:
  1. Find the most recently watched episode's (season_number, episode_number)
  2. Look for the next episode in the same season (episode_number + 1)
  3. If none, look for the first episode of the next season
  4. If none, check for newly added seasons (season premieres)
  5. Return that episode as "Up Next" for this series
Sort by: last_played_at of the most recently watched episode (most recent first)
```

This is best implemented as an application-level query with short-lived caching (5 minutes) since it requires multiple steps and joins across `media_items`, `episodes`, `seasons`, `series`, and `user_item_data`.

### Entity-Relationship Overview

```
 users ──< user_item_data >── media_items
                                 │
 users ──< bookmarks >───────────┤
                                 │
 users ──< playlists ──< playlist_items >── media_items

 media_items ──< media_segments
  media_files ──< media_fingerprints (0..1)
  media_files ──< storyboards (0..1)
```

### Schema DDL

#### User Item Data (Per-User Per-Item Watch State)

The central table for all per-user watch state. One row per user per media item.

```sql
CREATE TABLE user_item_data (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    is_watched BOOLEAN NOT NULL DEFAULT false,
    play_count INT NOT NULL DEFAULT 0,
    last_played_at TIMESTAMPTZ,

    resume_position_ms INT NOT NULL DEFAULT 0,
    last_played_media_file_id UUID REFERENCES media_files(id) ON DELETE SET NULL,

    is_favorite BOOLEAN NOT NULL DEFAULT false,
    user_rating INT CHECK (user_rating BETWEEN 1 AND 10),

    audio_stream_index INT,
    subtitle_stream_index INT,

    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

    UNIQUE(user_id, media_item_id)
) WITH (fillfactor = 85);

CREATE INDEX idx_user_item_data_user_id ON user_item_data (user_id);
CREATE INDEX idx_user_item_data_media_item_id ON user_item_data (media_item_id);
CREATE INDEX idx_user_item_data_continue_watching ON user_item_data (user_id, last_played_at DESC)
    WHERE is_watched = false AND resume_position_ms > 0;
CREATE INDEX idx_user_item_data_favorites ON user_item_data (user_id, updated_at DESC)
    WHERE is_favorite = true;
CREATE INDEX idx_user_item_data_watched ON user_item_data (user_id)
    WHERE is_watched = true;
CREATE INDEX idx_user_item_data_user_rating ON user_item_data (user_id, user_rating DESC)
    WHERE user_rating IS NOT NULL;
```

`resume_position_ms` is updated frequently during playback (every 10-30 seconds via heartbeat). When an item is marked fully watched, `resume_position_ms` resets to 0 and `is_watched` becomes true.

The partial index `idx_user_item_data_continue_watching` is specifically optimized for the Continue Watching query — it only indexes rows that are in-progress, making the query a fast index scan.

The `metadata` JSONB column stores per-user per-item extensible data. Currently used for `subtitle_offset_ms` (per-user subtitle sync offset, see [SUBTITLES.md](SUBTITLES.md)). Uses PostgreSQL `||` operator for shallow merge on update.

`fillfactor = 85` reserves 15% of each data page for HOT updates. During playback, `resume_position_ms` is updated every 10-30 seconds but no indexed column changes — this makes every heartbeat update eligible for HOT (Heap-Only Tuple), keeping the new row version on the same page without touching indexes. This is the primary defense against index bloat on the highest-UPDATE-frequency table in the system. Full rationale documented in [DATABASE_MAINTENANCE.md](../operations/DATABASE_MAINTENANCE.md).

#### Bookmarks (User Timestamp Markers)

User-defined named markers within a media item. Useful for saving specific scenes, moments, or custom chapter points.

```sql
CREATE TABLE bookmarks (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    position_ms INT NOT NULL CHECK (position_ms >= 0),
    label TEXT NOT NULL,
    description TEXT,

    UNIQUE(user_id, media_item_id, position_ms)
);

CREATE INDEX idx_bookmarks_user_id ON bookmarks (user_id);
CREATE INDEX idx_bookmarks_media_item_id ON bookmarks (media_item_id);
CREATE INDEX idx_bookmarks_user_item ON bookmarks (user_id, media_item_id, position_ms);
```

The unique constraint on `(user_id, media_item_id, position_ms)` prevents duplicate bookmarks at the exact same timestamp. The application may snap nearby positions (within a few seconds) to avoid near-duplicates.

#### Playlists

User-created ordered collections of media items. Supports both manual and smart playlists.

```sql
CREATE TABLE playlists (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    name TEXT NOT NULL,
    description TEXT,
    visibility TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'shared', 'public')),

    is_smart BOOLEAN NOT NULL DEFAULT false,
    smart_filter JSONB,

    item_count INT NOT NULL DEFAULT 0,
    total_duration_seconds INT NOT NULL DEFAULT 0,

    metadata JSONB NOT NULL DEFAULT '{}',
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_playlists_user_id ON playlists (user_id);
CREATE INDEX idx_playlists_visibility ON playlists (visibility) WHERE visibility IN ('shared', 'public');
CREATE INDEX idx_playlists_smart_filter ON playlists USING GIN (smart_filter jsonb_path_ops) WHERE is_smart = true;
```

`smart_filter` stores filter criteria for smart playlists (e.g. `{"genres": ["action"], "year_min": 2020, "rating_min": 7}`). Smart playlists are auto-populated by the application layer; their items are NOT stored in `playlist_items` (the filter is evaluated at query time).

`item_count` and `total_duration_seconds` are denormalized counters updated when items are added/removed, avoiding expensive COUNT/SUM queries for display.

#### Playlist Items (Ordered Join Table)

```sql
CREATE TABLE playlist_items (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    playlist_id UUID NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    position INT NOT NULL,

    UNIQUE(playlist_id, position),
    UNIQUE(playlist_id, media_item_id)
);

CREATE INDEX idx_playlist_items_playlist_id ON playlist_items (playlist_id);
CREATE INDEX idx_playlist_items_media_item_id ON playlist_items (media_item_id);
```

`position` uses integer spacing (e.g. 1000, 2000, 3000) to allow insertions between items without renumbering the entire list. The application renumbers positions periodically or when gaps become small.

The `UNIQUE(playlist_id, media_item_id)` constraint prevents the same item from appearing twice in a playlist.

---

## Offline Downloads Domain: Schema Design

### Overview

Stores durable mobile download jobs, server-prepared package manifests/files, per-device local inventory state, and explicit download audit events. Offline download behavior is documented in [OFFLINE_DOWNLOADS.md](OFFLINE_DOWNLOADS.md).

### Entity-Relationship Overview

```
 users ──< download_jobs >── media_items
                  │
                  └── download_packages ──< download_package_files
                              │
                              └── download_device_state >── users

 download_events links users, jobs, packages, media_items, and devices for operational audit history.
```

### Tables

| Table | Purpose |
|---|---|
| `download_jobs` | Durable planning/preparation queue for mobile package generation. Stores user, session/device, media item, selected source file, selected quality, selected audio/subtitle/artwork, package strategy, progress, bytes, policy snapshot, failure reason, retries, cancellation marker, expiry, and cleanup eligibility. |
| `download_packages` | Server package record created from a ready job. Stores user/device/media ownership, package format, manifest version, logical storage key, relative manifest path, byte/file counts, hashes, selected streams, included artwork/storyboards, sync metadata, policy snapshot, serve timestamps, expiry, revocation, and cleanup eligibility. |
| `download_package_files` | Per-file manifest for package integrity and resumable repair. Stores relative package path, role, content type, byte size, SHA-256 checksum, segment index, track type/identifier, and required/optional status. |
| `download_device_state` | Per-user, per-device local inventory and sync state. Stores local status, bytes downloaded, verified file count, local manifest hash, last online/download/play timestamps, local resume position, pending sync queue, deletion marker, and local failure details. |
| `download_events` | Explicit operational event stream for create/start/ready/fail/cancel/serve/delete/expire/revoke/renew/quota/policy/checksum/sync/cleanup actions. |
| `server_config.downloads` | Runtime policy JSONB group for global enablement, quality ceiling, byte quotas, active job limits, retained package limits, LAN/remote restrictions, transcode-download allowance, package expiry, retention, and per-user/library override maps. |

### Key Constraints

- `client_platform` is restricted to `android` and `ios` for v1.
- `package_format` is restricted to `hls_fmp4` and `mp4`; HLS/fMP4 is canonical and MP4 is a direct-compatible optimization.
- Package storage uses logical `storage_key` plus package-relative file paths. Raw signed URLs, bearer tokens, refresh tokens, client secrets, and source filesystem paths are not stored.
- Download jobs and packages retain `access_policy_snapshot` JSONB for diagnostics and reconnect decisions.
- `download_device_state` is unique on `(user_id, device_identifier, download_package_id)` so switching users or servers cannot merge package inventory.
- Table-level audit triggers cover `download_jobs`, `download_packages`, and `download_device_state`; `download_events` stores explicit domain events such as quota denial or checksum mismatch.
- `server_config.downloads` is added by migration `20260701020000_add_download_policy_config.sql`; Rust deserializes missing/partial JSON through `DownloadsConfig::default()` for upgrade safety.
- `20260701030000_seed_download_package_worker_task.sql` extends the scheduled-task type constraint with `download_package_worker` and seeds the durable package worker for existing deployments. The worker populates `download_packages` and `download_package_files` from completed jobs and records cleanup/job-state events in `download_events`.
- `20260701040000_seed_download_notifications.sql` seeds `download_ready` and `download_failed` notification types for actionable offline-download terminal states; high-frequency progress remains in the process-local SSE EventBus, not persisted as notifications.
- `20260701050000_add_download_package_renewed_event.sql` extends the `download_events.event_type` constraint with `package_renewed` for package-expiry renewal audit rows.

### Indexing

Indexes cover:

- User inventory and media detail views: `download_jobs(user_id, status, created_at)`, `download_packages(user_id, status, media_item_id, created_at)`.
- Worker queues: partial index on queued/preparing `download_jobs`.
- Device inventory: `download_jobs(user_id, device_identifier, status)`, `download_packages(user_id, device_identifier, status, created_at)`, `download_device_state(user_id, device_identifier, local_status, updated_at)`.
- Expiry/cleanup scans: partial expiry indexes on jobs and packages plus deleted device-state rows.
- Integrity and repair: package-file lookup by package/role/segment and checksum.
- Audit/diagnostics: download events by user, job, package, and event type.

---

## Segment Detection Domain: Schema Design

### Overview

Stores detected skippable segments (intros, credits, recaps, previews) and cached audio fingerprints for media files. Segment detection is documented in [SEGMENT_DETECTION.md](SEGMENT_DETECTION.md).

### Design Decisions

- **Separate tables for segments and fingerprints** — segments are the user-facing data (timestamps); fingerprints are internal analysis artifacts (opaque byte arrays). Different lifecycles, different query patterns.
- **Per-media-file fingerprints** — one fingerprint per `media_files` row, keyed by file hash for cache invalidation. When the file changes (hash differs), the fingerprint is recomputed.
- **Per-media-item segments** — segments are attached to `media_items` (not `media_files`) because the intro/credits are properties of the content, not the specific file. If a user has both 1080p and 4K versions, segments apply to both.
- **Manual overrides** — manually created segments have `source = 'manual'` and are never overwritten by automatic analysis. The `is_manual` flag prevents the analysis task from touching user edits.

### Entity-Relationship Overview

```
media_files ──< media_fingerprints (0..1 per file)

media_items ──< media_segments (0..N per item)
```

### Schema DDL

#### Media Segments (Detected Skippable Timestamps)

```sql
CREATE TABLE media_segments (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    segment_type TEXT NOT NULL CHECK (segment_type IN (
        'intro', 'credits', 'recap', 'preview', 'outro'
    )),

    start_ms INT NOT NULL CHECK (start_ms >= 0),
    end_ms INT NOT NULL CHECK (end_ms > start_ms),

    skip_to_ms INT NOT NULL CHECK (skip_to_ms >= start_ms AND skip_to_ms <= end_ms),

    confidence REAL NOT NULL DEFAULT 1.0 CHECK (confidence BETWEEN 0 AND 1),
    source TEXT NOT NULL CHECK (source IN ('chapter', 'chromaprint', 'blackframe', 'silence', 'manual', 'combined')),

    is_manual BOOLEAN NOT NULL DEFAULT false,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_media_segments_media_item_id ON media_segments (media_item_id);
CREATE INDEX idx_media_segments_type ON media_segments (segment_type);
CREATE UNIQUE INDEX media_segments_item_type_unique ON media_segments (media_item_id, segment_type) WHERE is_manual = true;
```

`start_ms` / `end_ms` — the detected timestamp range of the segment, in milliseconds from the start of the media file.

`skip_to_ms` — the safe seek target for the client, accounting for padding. For intros, this is typically `end_ms - intro_end_padding_ms` so the user doesn't land in the content. For credits, this is typically `end_ms` (skip to the very end).

`confidence` — 0.0 to 1.0. Chapter markers are always 1.0. Chromaprint matches are typically 0.7–0.95. Black frame alone is capped at 0.5. Segments below the configured threshold (default 0.7) are not shown to users.

`source` — how the segment was detected. `combined` means multiple methods agreed (highest confidence). `manual` means user-created.

`is_manual` — manual segments are never overwritten by the analysis task. The unique partial index ensures at most one manual segment per type per item (a user can't create two manual intro segments).

`metadata` stores analysis details — `{ "matching_episodes": 5, "chromaprint_algorithm": "test2", "blackframe_amount": 95, "silence_db": -60 }`.

#### Media Fingerprints (Cached Chromaprint Data)

```sql
CREATE TABLE media_fingerprints (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_file_id UUID NOT NULL UNIQUE REFERENCES media_files(id) ON DELETE CASCADE,

    file_hash TEXT NOT NULL,

    fingerprint BYTEA NOT NULL,
    fingerprint_algorithm TEXT NOT NULL DEFAULT 'test2',
    fingerprint_duration_ms INT NOT NULL,

    chapters_json JSONB,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_media_fingerprints_media_file_id ON media_fingerprints (media_file_id);
CREATE INDEX idx_media_fingerprints_file_hash ON media_fingerprints (file_hash);
```

`file_hash` — the `media_files.file_hash` value at the time of fingerprinting. When the file changes (hash differs), the fingerprint is recomputed. This enables incremental analysis.

`fingerprint` — the raw chromaprint fingerprint as a byte array. Opaque to queries — used only by the analysis service for comparison.

`fingerprint_algorithm` — the Chromaprint algorithm variant used (default: `test2`, the standard 2-bit-per-element algorithm). Stored for future compatibility.

`fingerprint_duration_ms` — the duration of audio that was fingerprinted, in milliseconds. Used to validate that the full file was processed.

`chapters_json` — chapter data extracted by ffprobe during Phase 3 of scanning. Stored here alongside the fingerprint so the analysis task can process chapters without re-probing. Contains chapter titles, start times, and end times.

---

## Storyboards Domain: Schema Design

### Overview

Stores metadata for seek-preview thumbnail grids ("storyboards") generated for media files. Each storyboard is a set of WebP sprite sheet images with a WebVTT index file, stored on disk in `/cache/storyboards/`. Full design documented in [STORYBOARDS.md](STORYBOARDS.md).

### Design Decisions

- **Per-media-file storyboards** — multi-version items (4K + 1080p) may have different aspect ratios, requiring separate sprite sheets per `media_files` row
- **File-hash-based cache invalidation** — when the source file changes (hash differs), the storyboard is regenerated
- **No partitioning** — one row per media file; table size is proportional to library size (~1 row per file), not playback activity
- **Cache storage** — sprite sheet images live on disk in `/cache/storyboards/`; the database only stores metadata (paths, dimensions, timestamps)

### Entity-Relationship Overview

```
media_files ──< storyboards (0..1 per file)
```

### Schema DDL

#### Storyboards (Per-File Thumbnail Grid Metadata)

```sql
CREATE TABLE storyboards (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_file_id UUID NOT NULL UNIQUE REFERENCES media_files(id) ON DELETE CASCADE,

    file_hash TEXT NOT NULL,

    interval_seconds INT NOT NULL,
    width INT NOT NULL,
    height INT NOT NULL,
    sprite_count INT NOT NULL,
    total_thumbnails INT NOT NULL,
    total_size_bytes BIGINT NOT NULL,

    keyframe_only BOOLEAN NOT NULL DEFAULT true,
    quality INT NOT NULL DEFAULT 75,

    generated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    generation_duration_ms INT,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_storyboards_media_file_id ON storyboards (media_file_id);
CREATE INDEX idx_storyboards_file_hash ON storyboards (file_hash);
```

`file_hash` — the `media_files.file_hash` value at the time of generation. When the file changes (hash differs), the storyboard is regenerated.

`interval_seconds` — the interval between thumbnails used for this storyboard. May differ across files if adaptive interval is enabled.

`sprite_count` — number of sprite sheet image files generated.

`total_thumbnails` — total individual thumbnails across all sprite sheets.

`total_size_bytes` — total disk space consumed by all sprite sheets + WebVTT index for this file.

`keyframe_only` — whether keyframe-only extraction was used. Affects seek accuracy.

`generation_duration_ms` — how long generation took (for admin UI display and metrics).

`metadata` stores additional generation details — `{ "ffmpeg_version": "6.1.2", "adaptive_interval": true, "content_duration_seconds": 7200, "error_count": 0 }`.

---

## User & Authentication Domain: Schema Design

### Overview

The auth domain handles identity, authentication, authorization, and API access — purpose-built for a **self-hosted, self-contained** Duskcue. No external identity provider, no central auth server, no SSO, no OAuth, no federation. All user accounts live in the local PostgreSQL database. The server can operate in two modes: **local-only** (LAN/VPN, auth optional) or **exposed** (internet-facing, auth mandatory).

### Architecture Principles

**Self-contained identity.** This is an open-source, self-hosted platform. No user accounts are created on any external website. The server is the entire identity boundary. Admins create local users through the server's admin UI. The only onboarding mechanism is invite codes — no public registration.

**Two network modes.** The admin declares how the server is accessed, which determines security enforcement:

| Mode | Auth Required | HTTPS | Rate Limiting | Use Case |
|---|---|---|---|---|
| `local` (default) | Optional | Not enforced | Off | Home LAN, VPN, `localhost` |
| `exposed` | Mandatory | Enforced | Active | Internet-facing via reverse proxy |

In local mode, the admin can disable auth entirely — all requests run as the `owner` user. This is for trusted home networks where the convenience of no login outweighs the security risk.

**First-run setup wizard.** When the server starts with an empty `users` table, it enters setup mode. Only `POST /api/v1/setup` is accessible. The admin creates the `owner` account with a username and display name (password is optional — they can add a passkey after setup). Once the owner exists, normal auth enforcement begins.

**No external auth dependencies.** Unlike Authelia, Authentik, Keycloak, Pocket ID, or other self-hosted IdP tools, our server handles auth internally. We don't sit behind a reverse proxy auth gateway. We don't federate with LDAP. The application is the identity boundary. This is the same model Plex, Jellyfin, and Emby use — simple, self-contained, no external moving parts.

### Design Principles

Our auth system is designed around eight principles:

**1. Passkey-first authentication (WebAuthn/FIDO2)**

Passwords are the leading attack vector. OWASP recommends Argon2id for password hashing and MFA for all accounts. We go further: passkeys are the primary authentication method, with passwords available as a legacy fallback. Passkeys are phishing-resistant, require no memorization, and sync across devices via OS-level providers (iCloud Keychain, Google Password Manager, Windows Hello). Each passkey is a public-private key pair — we store only the public key and a credential ID. The private key never leaves the user's device.

**2. Capability-based access control (not RBAC, ABAC, or PBAC)**

RBAC causes role explosion when roles encode conditions. ABAC is cognitively complex for a small user base. PBAC requires a policy engine. Our model: **roles provide default capability bundles, and individual capabilities can be toggled per user.** This gives the simplicity of RBAC with the flexibility of fine-grained permissions, without any external dependencies.

**3. Library-scoped access**

Users see only the libraries they have been granted access to. This is a fundamental Duskcue concept — kids don't see R-rated libraries, guests don't see private collections. Access is per-user per-library.

**4. Device-aware session management**

Server-side sessions track device identity, client info, IP address, and last activity. Sessions are tied to our existing trust scoring system (`user_trust_scores`) — suspicious session patterns (impossible travel, concurrent streams from distant locations) automatically flag trust events.

**5. Scoped API keys for integrations**

Third-party tools (Classifarr, automation scripts, custom clients) authenticate via API keys. Each key is scoped to specific capabilities — a Classifarr key can read library data but not manage users. API keys use the same capability model as users.

**6. Invite-based user onboarding (no public registration)**

The server owner invites users via shareable codes. Each invitation pre-configures the new user's role, capabilities, and library access. No account creation on any external website. No public sign-up endpoint.

**7. Network-mode-aware security**

Security enforcement adapts to how the server is accessed. Local mode relaxes requirements for convenience. Exposed mode enforces HTTPS, secure cookies, rate limiting, and mandatory authentication.

**8. WebAuthn RP ID awareness**

Passkeys are cryptographically bound to a domain (Relying Party ID). The RP ID is stored in `server_config.auth.rp_id` and must be configured correctly when the server is exposed via a subdomain. Default: auto-detected from the Host header during setup. Admin can override to use a root domain (e.g. `example.com`) so passkeys work across all subdomains.

A *capability* is an atomic, named permission (e.g. `can_transcode`, `can_view_analytics`). A *role* is a pre-defined set of capabilities. If a user has explicit capability overrides, those take precedence over the role defaults.

### Roles & Capabilities

**Roles (default capability bundles):**

| Role | Description | Default Capabilities |
|---|---|---|
| `owner` | Server owner. Irrevocable. Full access. | All capabilities. Cannot be demoted. |
| `admin` | Trusted administrator. Manages users and server. | All except ownership transfer. |
| `member` | Standard user. Configurable access. | `play_media`, `download`, `share_content` |
| `guest` | Restricted access. Potentially time-limited. | `play_media` only |

**Capabilities (atomic permissions):**

| Capability | Description |
|---|---|
| `play_media` | Play any accessible media |
| `can_transcode` | Request transcoded streams (CPU-intensive) |
| `can_download` | Download media files |
| `can_delete_media` | Delete media from disk |
| `can_manage_libraries` | Create, edit, scan, and delete libraries |
| `can_manage_users` | Create, edit, and delete users |
| `can_view_analytics` | Access the analytics dashboard and play history |
| `can_manage_server` | Access server settings, configuration, and logs |
| `can_manage_scheduled_tasks` | Create, edit, and trigger scheduled tasks |
| `can_use_live_tv` | Access live TV features (future) |
| `can_share_content` | Share content links externally |
| `can_remote_control` | Remote control other users' playback sessions |

### Relationship to Other Domains

- `users` is referenced by `user_item_data`, `play_sessions`, `trakt_accounts`, `trakt_sync_state`, `user_trust_events`, `user_trust_scores`, `bookmarks`, `playlists`, `notifications`, `user_notification_preferences`, and `user_push_devices`
- `user_sessions` complements `play_sessions` — sessions track *authentication* (when/how a user logged in), play sessions track *activity* (what a user watched)
- `user_trust_scores` (Activity domain) integrates with session management — low trust scores can trigger session termination or MFA re-challenge

### Network Mode & Security Enforcement

The server operates in one of two network modes, declared by the admin in `server_config.auth.network_mode`:

**Local mode** (`"local"`, default):
- Auth is optional — the admin can disable it entirely (`auth_required: false`)
- When auth is disabled, all requests run as the `owner` user
- HTTPS not enforced (passkeys still work on `localhost`)
- No rate limiting
- Suitable for: home LAN, VPN, Tailscale, `localhost`

**Exposed mode** (`"exposed"`):
- Auth is mandatory — `auth_required` is forced to `true`
- HTTPS enforced — requests over plain HTTP are rejected with a redirect
- Secure cookie flags (`Secure`, `SameSite=Strict`)
- Rate limiting active (per-IP and per-user)
- CSRF protection enabled
- Suitable for: internet-facing via reverse proxy / subdomain

Mode transition: When the admin switches from local to exposed, the server validates that:
1. At least one user has a password or passkey set
2. The `owner` account has a password or passkey
3. HTTPS is configured (either directly via `ssl_*` columns or via a reverse proxy)

### First-Run Setup Wizard

When the server starts and `users` is empty, it enters **setup mode**:

1. All normal API endpoints return `503 Service Unavailable` with `X-Setup-Required: true`
2. Only `POST /api/v1/setup` is accessible (unauthenticated)
3. The admin submits: `username` (required), `display_name` (required), `password` (optional)
4. The server creates the `owner` account with `role = 'owner'`, `status = 'active'`
5. `server_config.auth.setup_complete` is set to `true`
6. `server_config.auth.rp_id` is auto-detected from the request's `Host` header
7. Normal auth enforcement begins

If no password is provided during setup, the owner account has `password_hash = null`. They can add a passkey on first login (the setup wizard redirects to a "register a passkey" prompt). In local mode with `auth_required: false`, no credential is needed — the owner simply accesses the server.

### WebAuthn Relying Party ID

Passkeys are cryptographically bound to a domain. The RP ID is critical to get right:

| Scenario | RP ID | RP Origin |
|---|---|---|
| Local: `http://localhost:48027` | `localhost` | `http://localhost:48027` |
| Local: `http://192.168.1.100:48027` | `192.168.1.100` | `http://192.168.1.100:48027` |
| Exposed: `https://media.example.com` | `example.com` | `https://media.example.com` |
| Exposed: `https://media.example.com:8443` | `example.com` | `https://media.example.com:8443` |

Rules:
- Auto-detected from the Host header during setup
- Admin can override in `server_config.auth.rp_id` — setting it to the root domain (`example.com`) allows passkeys to work across all subdomains
- Changing the RP ID **breaks all existing passkeys** — the server warns before allowing changes
- In exposed mode, the admin must ensure the RP ID matches the domain end-users will use

### Entity-Relationship Overview

```
users ──< user_passkeys
       ──< user_totp (0..1)
       ──< user_capabilities
       ──< user_library_access >── libraries
       ──< user_sessions
       ──< api_keys
       ──< invitations (created_by)
       ──> streaming_policies (optional, per-user)

invitations ──> users (user_id, after first use)
            ── use via invite code ──> new user + sessions on devices

device_linking_codes ──> users (approved_by)
                      ──> user_sessions (resulting_session)

reauth_codes ──> users (user_id, requested_by_user_id)
             ──> user_sessions (resulting_session)

streaming_policies (reusable policy templates)
```

### Schema DDL

#### Users (Expanded from Stub)

The core identity table. Every user has a unique identity with authentication credentials and a role.

```sql
CREATE TABLE users (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    username TEXT NOT NULL,
    display_name TEXT NOT NULL,
    email TEXT,
    avatar_url TEXT,

    password_hash TEXT,
    role TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('owner', 'admin', 'member', 'guest')),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled', 'locked', 'pending')),

    failed_login_attempts INT NOT NULL DEFAULT 0,
    locked_until TIMESTAMPTZ,

    last_login_at TIMESTAMPTZ,
    last_login_ip INET,

    has_all_library_access BOOLEAN NOT NULL DEFAULT true,
    streaming_policy_id UUID REFERENCES streaming_policies(id) ON DELETE SET NULL,
    max_streams INT,
    max_transcode_streams INT,
    bandwidth_limit_bps BIGINT,

    is_active BOOLEAN NOT NULL DEFAULT true,
    deleted_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE UNIQUE INDEX users_username_active ON users (username) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX users_email_active ON users (email) WHERE email IS NOT NULL AND deleted_at IS NULL;
CREATE INDEX idx_users_role ON users (role);
CREATE INDEX idx_users_status ON users (status);
CREATE INDEX idx_users_email ON users (email) WHERE email IS NOT NULL AND deleted_at IS NULL;
```

`password_hash` uses **Argon2id** (OWASP recommended, minimum 19 MiB memory, 2 iterations, 1 parallelism). Nullable — null means passkey-only authentication (no password set).

`has_all_library_access` — when true, the user can access all current and future libraries. When false, access is restricted to explicitly granted libraries via `user_library_access`.

`streaming_policy_id` — references a reusable streaming policy. When set, the policy's limits apply unless overridden by user-level columns below. Null means the global default policy is used (from `server_config.transcoding.default_streaming_policy_id`).

`max_streams`, `max_transcode_streams`, and `bandwidth_limit_bps` — per-user resource limits that override the assigned streaming policy's values. Null means inherit from the policy (or server default if no policy is assigned).

`metadata` stores user preferences (language, subtitle defaults, theme, notification preferences).

#### User Passkeys (WebAuthn Credentials)

Stores passkey registrations (public keys) for passwordless authentication via WebAuthn/FIDO2.

```sql
CREATE TABLE user_passkeys (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    credential_id BYTEA NOT NULL UNIQUE,
    public_key BYTEA NOT NULL,
    sign_count BIGINT NOT NULL DEFAULT 0,
    transports JSONB NOT NULL DEFAULT '[]',
    attestation_type TEXT,
    aaguid UUID,
    name TEXT NOT NULL,

    last_used_at TIMESTAMPTZ,

    UNIQUE(user_id, credential_id)
);

CREATE INDEX idx_user_passkeys_user_id ON user_passkeys (user_id);
CREATE INDEX idx_user_passkeys_credential_id ON user_passkeys (credential_id);
```

`credential_id` — the base64url-decoded credential ID from the WebAuthn registration ceremony.
`public_key` — the COSE-key-encoded public key (CBOR format).
`sign_count` — monotonic counter from the authenticator. Used for clone detection — if a subsequent authentication presents a lower sign count, the credential may have been cloned.
`transports` — array of transport types the authenticator supports (`["internal", "hybrid", "usb", "ble", "nfc"]`).
`aaguid` — Authenticator Attestation GUID. Identifies the authenticator model (e.g. "Apple iPhone" vs "YubiKey 5").
`name` — user-given name for this passkey (e.g. "iPhone 16 passkey", "YubiKey on keychain").

#### User TOTP (Time-Based One-Time Password)

Optional second-factor authentication via authenticator apps (Google Authenticator, Authy, 1Password, etc.).

```sql
CREATE TABLE user_totp (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,

    secret TEXT NOT NULL,
    backup_codes JSONB NOT NULL DEFAULT '[]',
    is_verified BOOLEAN NOT NULL DEFAULT false,

    last_used_at TIMESTAMPTZ
);

CREATE INDEX idx_user_totp_user_id ON user_totp (user_id);
```

`secret` — encrypted TOTP secret (base32-encoded, encrypted at rest with application-level encryption).
`backup_codes` — array of hashed recovery codes. Each code is single-use. Generated on setup and can be regenerated.
`is_verified` — must complete one successful TOTP challenge during setup before the factor is considered enabled. Prevents locking out users who mistype their secret.

#### User Capabilities (Per-User Permission Overrides)

Stores per-user capability overrides. If a capability is present in this table, the override value is used. If not present, the role's default is used.

```sql
CREATE TABLE user_capabilities (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    capability TEXT NOT NULL,
    is_granted BOOLEAN NOT NULL DEFAULT true,

    UNIQUE(user_id, capability)
);

CREATE INDEX idx_user_capabilities_user_id ON user_capabilities (user_id);
```

Capability evaluation logic:
1. Check `user_capabilities` for an explicit override
2. If found, use the override (`is_granted = true` or `false`)
3. If not found, use the role's default capability set
4. `owner` role always has all capabilities, regardless of overrides

This design means most users have zero rows in this table (they use role defaults). Only users with customized permissions need rows.

#### User Library Access (Per-User Per-Library Grants)

Controls which libraries a user can see and access. Only evaluated when `users.has_all_library_access = false`.

```sql
CREATE TABLE user_library_access (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,

    UNIQUE(user_id, library_id)
);

CREATE INDEX idx_user_library_access_user_id ON user_library_access (user_id);
CREATE INDEX idx_user_library_access_library_id ON user_library_access (library_id);
```

When `has_all_library_access = true` on the user, this table is ignored for that user. When false, only libraries with a row here are accessible.

#### User Sessions (Active Authentication Sessions)

Server-side session tracking. Every authenticated client holds an opaque session token. Sessions are invalidated on password change, role change, or explicit logout.

```sql
CREATE TABLE user_sessions (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    token_hash TEXT NOT NULL UNIQUE,
    device_id TEXT,
    device_name TEXT,
    client_name TEXT,
    client_version TEXT,
    client_platform TEXT,

    ip_address INET,
    user_agent TEXT,
    is_secure BOOLEAN NOT NULL DEFAULT false,

    expires_at TIMESTAMPTZ NOT NULL,
    last_active_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_user_sessions_user_id ON user_sessions (user_id);
CREATE INDEX idx_user_sessions_token_hash ON user_sessions (token_hash);
CREATE INDEX idx_user_sessions_expires_at ON user_sessions (expires_at);
CREATE INDEX idx_user_sessions_device ON user_sessions (user_id, device_id);
```

`token_hash` — the session token is a cryptographically random string generated by the server. Only the hash is stored (SHA-256). The raw token is sent to the client once and never stored.

`device_id` — client-generated stable identifier. Used to group sessions from the same physical device. Enables "sign out of my iPhone" functionality.

Sessions are cleaned up via a scheduled task that deletes expired rows. `last_active_at` is updated on each authenticated request (with throttling — not on every single request).

#### API Keys (Scoped Integration Tokens)

API keys for third-party integrations and automation. Uses the same capability model as users.

```sql
CREATE TABLE api_keys (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    name TEXT NOT NULL,
    key_prefix TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,

    capabilities JSONB NOT NULL DEFAULT '[]',
    is_active BOOLEAN NOT NULL DEFAULT true,

    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_api_keys_user_id ON api_keys (user_id);
CREATE INDEX idx_api_keys_key_hash ON api_keys (key_hash);
CREATE INDEX idx_api_keys_key_prefix ON api_keys (key_prefix);
CREATE INDEX idx_api_keys_active ON api_keys (is_active) WHERE is_active = true;
```

`key_prefix` — the first 8 characters of the raw key, stored in plaintext for identification in logs and UI (e.g. `mv_sk_a3f`). The full key is never stored.
`key_hash` — Argon2id hash of the full raw key. On authentication, the provided key is hashed and compared.
`capabilities` — JSONB array of capability strings (e.g. `["play_media", "can_manage_libraries"]`). API keys cannot exceed the capabilities of their owning user.

Key format: `mv_{type}_{random}` where type is `sk` (secret key) or `pk` (public key — read-only). Example: `mv_sk_a3f8b2c1d4e5f6g7h8i9j0k1l2m3n4o5`.

#### Invitations (Invite Code System)

Invite codes are the primary mechanism for onboarding users and authenticating devices. Admins create invite codes by entering a user's email address; the server sends the code to that email. Each code maps to a single user account. The admin can issue multiple codes to the same email address for different household members (each code creates a separate user account with separate watch history and preferences).

The invite code IS the user's primary authentication credential — it can be entered on any device to authenticate. For devices with constrained input (smart TVs, consoles), a separate device linking flow is used (see `device_linking_codes` below).

**Code format**: `mv_{scope}_{24 base-20 chars}` where scope is `invite`. Base-20 character set: `BCDFGHJKLMNPQRSTVWXZ` (consonants only, no ambiguous chars, per RFC 8628 Section 6.1). Formatted with dashes for readability: `mv_invite-BCDK-MJHT-WDJB-NPQR-STVW-XZBC`. Total entropy: ~103 bits.

```sql
CREATE TABLE invitations (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    created_by_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,

    code_hash TEXT NOT NULL UNIQUE,
    code_prefix TEXT NOT NULL,

    email TEXT NOT NULL,
    display_name TEXT,

    role TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('admin', 'member', 'guest')),

    capabilities JSONB NOT NULL DEFAULT '[]',
    library_ids JSONB NOT NULL DEFAULT '[]',
    has_all_library_access BOOLEAN NOT NULL DEFAULT false,
    streaming_policy_id UUID REFERENCES streaming_policies(id) ON DELETE SET NULL,

    max_uses INT NOT NULL DEFAULT 1,
    use_count INT NOT NULL DEFAULT 0,

    expires_at TIMESTAMPTZ,
    is_revoked BOOLEAN NOT NULL DEFAULT false,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_invitations_code_hash ON invitations (code_hash);
CREATE INDEX idx_invitations_code_prefix ON invitations (code_prefix);
CREATE INDEX idx_invitations_created_by ON invitations (created_by_user_id);
CREATE INDEX idx_invitations_user_id ON invitations (user_id);
CREATE INDEX idx_invitations_email ON invitations (email);
CREATE INDEX idx_invitations_expires ON invitations (expires_at) WHERE expires_at IS NOT NULL AND is_revoked = false;
```

`code_hash` — SHA-256 hash of the full raw code. The raw code is generated by the server, sent to the user's email, and never stored. On authentication, the provided code is hashed and compared.

`code_prefix` — first 4 characters of the raw code, stored in plaintext for identification in the admin UI (e.g. `BCDK`). Allows the admin to identify which code a user is referring to without exposing the full code.

`user_id` — populated after first use. Links the invite to the user account it created. Null until the code is used for the first time.

`email` — the email address the admin entered. Multiple invites can share the same email (for household members). The email is the delivery mechanism, not the account identifier.

`display_name` — optional display name for the user account. If not provided, the user sets their own during first login.

`is_revoked` — admin can revoke a code at any time. Revoked codes immediately invalidate all sessions created with that code.

`max_uses` — how many times the code can be used to authenticate. Default 1 (single device). Admin can increase for households that want one code shared among family members. Each use creates a new session on a new device.

**Invite code lifecycle:**

1. Admin creates invite: enters email, display name, role, capabilities, library access, max uses, optional expiry
2. Server generates a cryptographically random 24-char base-20 code, hashes it, stores the hash
3. Server sends the code to the email address (requires SMTP configured in `server_config.auth.smtp_*` or `server_config.notifications`)
4. User receives email, installs app, enters code + server address
5. Server validates: code hash matches, not revoked, not expired, use_count < max_uses
6. First use: server creates a `users` row with the invite's role, capabilities, library access
7. Server creates a `user_sessions` row, links `invitations.user_id`
8. Subsequent uses: server creates a new session for the existing user
9. Increment `use_count`

**Rate limiting:** Max 5 failed verification attempts per IP per 15 minutes. After 5 failures, the IP is blocked for 30 minutes. Per RFC 8628 Section 5.1.

#### Device Linking Codes (RFC 8628-Inspired Short Codes)

For devices with constrained input (smart TVs, game consoles, streaming sticks), the app displays a short linking code. The user enters this code on their already-authenticated phone/browser to authorize the device. This follows the RFC 8628 Device Authorization Grant pattern, adapted for our self-hosted architecture.

```sql
CREATE TABLE device_linking_codes (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_code TEXT NOT NULL UNIQUE,
    device_code TEXT NOT NULL UNIQUE,

    client_name TEXT,
    client_platform TEXT,
    client_version TEXT,

    ip_address INET,
    user_agent TEXT,

    expires_at TIMESTAMPTZ NOT NULL,
    is_approved BOOLEAN NOT NULL DEFAULT false,
    approved_by_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    approved_at TIMESTAMPTZ,

    resulting_session_id UUID REFERENCES user_sessions(id) ON DELETE SET NULL
);

CREATE INDEX idx_device_linking_user_code ON device_linking_codes (user_code);
CREATE INDEX idx_device_linking_device_code ON device_linking_codes (device_code);
CREATE INDEX idx_device_linking_expires ON device_linking_codes (expires_at) WHERE is_approved = false;
```

`user_code` — short 8-character code displayed to the user. Base-20 character set (`BCDFGHJKLMNPQRSTVWXZ`), formatted as `WDJB-MJHT`. Per RFC 8628 Section 6.1.

`device_code` — high-entropy internal code (32 bytes, hex-encoded). The device polls with this code. Not displayed to the user. Per RFC 8628 Section 5.2.

`expires_at` — 15 minutes from creation. Short lifetime limits phishing viability per RFC 8628 Section 5.4.

**Device linking flow (RFC 8628 adapted):**

1. Device app calls `POST /api/v1/device/code` with client info
2. Server generates `user_code` (8 chars, base-20) and `device_code` (32 bytes, hex)
3. Server returns: `{ "user_code": "WDJB-MJHT", "verification_uri": "https://media.example.com/link", "expires_in": 900, "interval": 5 }`
4. Device displays: "Visit `media.example.com/link` and enter code: `WDJB-MJHT`"
5. Device starts polling `POST /api/v1/device/token` with `device_code` every 5 seconds
6. User opens their authenticated browser/app, visits `/link`, enters `WDJB-MJHT`
7. Server shows device info (client name, platform), user approves
8. Server creates a `user_sessions` row, links it to `resulting_session_id`
9. Next poll returns the session token — device is now authenticated

**Polling responses** (per RFC 8628 Section 3.5):
- `authorization_pending` — user hasn't approved yet, continue polling
- `slow_down` — increase polling interval by 5 seconds
- `access_denied` — user denied the request, stop polling
- `expired_token` — code expired, request a new one

#### Re-Authentication Codes (Account Recovery / Compromised Account)

Short-lived, single-use codes for re-authentication after account compromise or "Sign Out Everywhere." Users (or admins on their behalf) request a re-auth code sent to their email. The code authenticates them on a single device, after which it is consumed.

```sql
CREATE TABLE reauth_codes (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    requested_by_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    code_hash TEXT NOT NULL UNIQUE,
    code_prefix TEXT NOT NULL,

    ip_address INET,

    expires_at TIMESTAMPTZ NOT NULL,
    is_used BOOLEAN NOT NULL DEFAULT false,
    used_at TIMESTAMPTZ,

    resulting_session_id UUID REFERENCES user_sessions(id) ON DELETE SET NULL,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_reauth_codes_user_id ON reauth_codes (user_id);
CREATE INDEX idx_reauth_codes_code_hash ON reauth_codes (code_hash);
CREATE INDEX idx_reauth_codes_expires ON reauth_codes (expires_at) WHERE is_used = false;
```

`code_hash` — SHA-256 hash of the full raw code. The raw code is sent to the user's email and never stored.

`code_prefix` — first 4 characters of the raw code for identification in the admin UI.

`requested_by_user_id` — the user who triggered the request. For self-service: same as `user_id`. For admin-initiated: the admin's user ID.

`expires_at` — 24 hours from creation (configurable via `server_config.auth.reauth_code_expiry_hours`).

`is_used` — consumed after first successful authentication. Single-use.

`resulting_session_id` — the session created when the code is used.

Code format: `mv_reauth-` prefix + 16 base-20 characters. Example: `mv_reauth-BCDK-MJHT-WDJB-NPQR`. Shorter than invite codes (16 vs 24 chars) since they're short-lived and rate-limited.

**Rate limiting:** Max 3 re-auth code requests per user per 24 hours. Prevents spam and abuse.

**"Sign Out Everywhere" flow:**
1. User (or admin) triggers "Sign Out Everywhere" for the target user
2. Server deletes all `user_sessions` for that user
3. Server revokes all active `invitations` linked to that user (`is_revoked = true`)
4. Server expires all active `device_linking_codes` for that user
5. Server generates a re-auth code, sends it to the user's email
6. User enters the re-auth code + server address on their trusted device
7. Server validates the code, creates a new session, marks the code as used
8. User repeats for each additional device

#### Streaming Policies (Reusable Streaming Restriction Templates)

Named, reusable policies that control how users can stream media. Each policy defines limits on concurrent streams, transcode sessions, bandwidth, and resolution restrictions. Users are assigned policies via `users.streaming_policy_id`; per-user override columns on `users` take precedence over policy values.

```sql
CREATE TABLE streaming_policies (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    name TEXT NOT NULL UNIQUE,
    description TEXT,

    max_streams INT,
    max_transcode_streams INT,
    bandwidth_limit_bps BIGINT,

    allow_direct_play BOOLEAN NOT NULL DEFAULT true,
    allow_direct_stream BOOLEAN NOT NULL DEFAULT true,
    allow_transcode BOOLEAN NOT NULL DEFAULT true,

    max_transcode_resolution TEXT CHECK (max_transcode_resolution IN ('480p', '720p', '1080p', '4k')),
    allow_transcode_4k BOOLEAN NOT NULL DEFAULT true,
    require_direct_play_4k BOOLEAN NOT NULL DEFAULT false,

    allowed_ip_ranges JSONB NOT NULL DEFAULT '[]',
    blocked_ip_ranges JSONB NOT NULL DEFAULT '[]',

    auto_terminate_paused_minutes INT,

    is_default BOOLEAN NOT NULL DEFAULT false,
    is_system BOOLEAN NOT NULL DEFAULT false,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_streaming_policies_is_default ON streaming_policies (is_default) WHERE is_default = true;
```

`max_streams` / `max_transcode_streams` — null means no limit. When a user starts a new stream, the server counts the user's active sessions against these limits.

`allow_direct_play` / `allow_direct_stream` / `allow_transcode` — coarse-grained streaming method gates. A guest policy might set `allow_transcode = false` to force direct play only (saves server resources).

`max_transcode_resolution` / `allow_transcode_4k` / `require_direct_play_4k` — resolution-aware transcode restrictions. `require_direct_play_4k = true` means 4K content cannot be transcoded (the client must support 4K direct play). This prevents bandwidth-intensive 4K transcodes for guest users.

`allowed_ip_ranges` / `blocked_ip_ranges` — JSONB arrays of CIDR strings (e.g. `["192.168.1.0/24", "10.0.0.0/8"]`). When `allowed_ip_ranges` is non-empty, only connections from those ranges are permitted. `blocked_ip_ranges` always takes precedence. Evaluated at session start time against the client IP.

`auto_terminate_paused_minutes` — null means no auto-termination. When set, paused sessions are terminated after this many minutes of continuous pause. Frees transcode resources.

`is_default` — at most one row can have this set to true. This is the policy assigned to users without an explicit `streaming_policy_id`.

`is_system` — system-seeded policies cannot be deleted (they can be modified). Prevents accidental removal of built-in policies.

**Seeded Default Policies:**

| Name | Purpose | Key Settings |
|---|---|---|
| Admin | Server admins — no restrictions | All allowed, no limits |
| Family | Trusted family members | `max_streams: 3`, `max_transcode_streams: 2` |
| Guest | Temporary/guest access | `max_streams: 1`, `allow_transcode: false`, `auto_terminate_paused_minutes: 30` |
| Remote Only | Restrict to remote/WAN streaming | `allowed_ip_ranges: []` (any), `blocked_ip_ranges: ["192.168.0.0/16", "10.0.0.0/8", "172.16.0.0/12"]` |
| LAN Only | Restrict to local network | `allowed_ip_ranges: ["192.168.0.0/16", "10.0.0.0/8", "172.16.0.0/12"]` |

**Policy Evaluation Precedence:**

When a user starts a stream, the server resolves limits in this order:

1. **User-level overrides** — `users.max_streams`, `users.max_transcode_streams`, `users.bandwidth_limit_bps` (non-null values take precedence)
2. **Assigned policy** — `users.streaming_policy_id` → `streaming_policies` row
3. **Global default policy** — the policy with `is_default = true`
4. **Server-level limits** — `server_config.transcoding` global caps

Higher-precedence sources override lower ones. Null values at any level fall through to the next level.

---

## System Domain: Schema Design

### Overview

The system domain handles server configuration, background task scheduling, and user notifications — the operational backbone that keeps the server running.

### Design Decisions

**Server Configuration — Hybrid Grouped**
- Typed columns for critical networking settings (server name, ports, SSL paths)
- JSONB groups for domain-specific configuration, each mapped to a typed Rust struct via serde
- Single-row table — application enforces only one config row exists
- Config loaded at startup, cached in memory, hot-reloaded on change
- `schema_version` enables config migrations when the JSONB structure evolves between releases
- Application-layer validation via strongly-typed Rust structs; DB stores the raw JSONB

**Scheduled Tasks — Database-Backed Task Queue**
- Task definitions and run history stored in PostgreSQL — survives restarts, visible in admin UI
- Two scheduling modes: cron expressions (time-specific, e.g. `0 3 * * *`) and fixed intervals (simple recurrence, e.g. every 900 seconds)
- Application polls every 30 seconds: `WHERE next_run_at <= now() AND is_enabled = true AND state != 'running'`
- Per-task JSONB config for task-specific parameters (scan mode, backup path, etc.)
- Full run history with duration, result, error details, and task-specific stats
- Users can manually trigger tasks from admin UI (creates a run with `trigger_type = 'manual'`)
- Auto-disable after consecutive failures to prevent runaway error loops

**Notifications — Event-Driven with Templates**
- `notification_types` define event categories with default templates per channel
- `notifications` hold individual instances per user with read state, delivery tracking, and polymorphic entity links
- In-app delivery initially; schema supports multi-channel from day one (email, webhook, push)
- Per-user per-type opt-in/out preferences; defaults from `notification_types.is_enabled_by_default`
- `user_push_devices` registers per-user mobile push tokens (FCM/APNs/UnifiedPush) with lifecycle (heartbeat, 30-day stale deactivation, manual revoke); Phase 13b Task 5
- No partitioning — volume is negligible at 1-50 users (~100-200 notifications/day max)
- Templates use Fluent message IDs (Phase 13b Task 1); rendered at notification creation time via `services/i18n.rs`, not delivery time
- Polymorphic `related_item_type` + `related_item_id` links notifications to any entity

### Default Scheduled Tasks

| Task | Schedule | Timeout | Config |
|---|---|---|---|
| Library Scan (Full) | `0 3 * * *` (daily 03:00) | 4h | `{ "mode": "full" }` |
| Library Scan (Quick) | Every 900s (15 min) | 30m | `{ "mode": "quick" }` |
| Metadata Refresh | Every 21600s (6 hours) | 2h | `{}` |
| Database Maintenance | `0 4 * * 0` (weekly Sun 04:00) | 1h | `{ "operations": ["vacuum", "analyze", "reindex"] }` |
| Partition Management | `0 0 1 * *` (monthly 1st 00:00) | 10m | `{ "create_ahead_months": 2 }` |
| Session Cleanup | Every 3600s (hourly) | 5m | `{}` |
| Trakt Sync | Every 1800s (30 min) | 30m | `{}` |
| Database Backup | `0 4 * * *` (daily 04:00) | 2h | `{}` |
| Media Health Check | `0 2 * * 0` (weekly Sun 02:00) | 4h | `{}` |
| Notification Cleanup | Every 86400s (daily) | 5m | `{ "max_age_days": 90 }` |
| Trust Score Recalculation | Every 3600s (hourly) | 5m | `{}` |
| Segment Analysis | `0 3 * * *` (daily 03:00) | 4h | `{ "max_concurrent_analyses": 1 }` |
| Storyboard Generation | `0 4 * * *` (daily 04:00) | 4h | `{ "max_concurrent_analyses": 1, "interval_mode": "adaptive" }` |
| Disk Space Check | Every 1800s (30 min) | 1m | `{ "check_paths": true }` |
| Reindex Maintenance | `0 2 * * 0` (weekly Sun 02:00) | 2h | `{ "bloat_threshold_percent": 30, "min_index_size_mb": 10 }` |
| Analyze Parents | `0 3 * * *` (daily 03:00) | 5m | `{}` |
| Transcode Health Check | Every 60s | 30s | `{ "stale_session_timeout_secs": 600 }` |
| System Requirement Check | Every 86400s (24 hours) | 30s | `{ "check_os": true, "check_docker": true }` |

### Notification Types

| Name | Category | Priority | Description |
|---|---|---|---|
| `new_media_added` | media | low | New item appeared in library |
| `library_scan_complete` | media | low | Scan finished with stats |
| `playback_started` | media | low | Another user started watching |
| `classifarr_decision` | media | low | Classification decision received |
| `server_alert` | system | high | System-level warning (disk space, errors) |
| `server_update` | system | low | New version available |
| `task_failed` | system | high | Background task error |
| `trust_alert` | security | high | Suspicious sharing detected |
| `new_login` | security | medium | New device/browser login |
| `user_invited` | user | low | Invitation created or used |
| `trakt_sync_error` | user | medium | Trakt sync failed for a user |
| `migration_completed` | task | medium | Platform migration completed |
| `migration_failed` | task | high | Platform migration failed |
| `download_ready` | media | medium | Offline package is ready to download |
| `download_failed` | task | high | Offline package preparation failed |

### Entity-Relationship Overview

```
server_config (single row)

scheduled_tasks ──< scheduled_task_runs

notification_types ──< notifications >── users
                   ──< user_notification_preferences >── users

users ──< user_push_devices
```

### Schema DDL

#### Server Configuration (Single-Row Hybrid)

Typed columns for critical networking settings. JSONB groups for domain-specific configuration.

```sql
CREATE TABLE server_config (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    server_name TEXT NOT NULL DEFAULT 'My Duskcue',
    base_url TEXT,
    http_port INT NOT NULL DEFAULT 48027,
    https_port INT,
    ssl_certificate_path TEXT,
    ssl_private_key_path TEXT,

    network JSONB NOT NULL DEFAULT '{}',
    transcoding JSONB NOT NULL DEFAULT '{}',
    metadata JSONB NOT NULL DEFAULT '{}',
    auth JSONB NOT NULL DEFAULT '{}',
    security JSONB NOT NULL DEFAULT '{}',
    notifications JSONB NOT NULL DEFAULT '{}',
    backup JSONB NOT NULL DEFAULT '{}',
    integrations JSONB NOT NULL DEFAULT '{}',
    logging JSONB NOT NULL DEFAULT '{}',
    storage JSONB NOT NULL DEFAULT '{}',
    maintenance JSONB NOT NULL DEFAULT '{}',
    resource_limits JSONB NOT NULL DEFAULT '{}',
    cpu JSONB NOT NULL DEFAULT '{}',
    quality JSONB NOT NULL DEFAULT '{}',
    subtitles JSONB NOT NULL DEFAULT '{}',
    analytics JSONB NOT NULL DEFAULT '{}',

    schema_version INT NOT NULL DEFAULT 2
);
```

JSONB column contents (structure documented here, validated by application):

- `network` — LAN/WAN settings, allowed subnets, SSL mode. Example: `{ "allowed_subnets": ["192.168.1.0/24"], "lan_only": false, "ssl_mode": "auto" }`
- `transcoding` — Hardware acceleration, transcode path, concurrency limits, global streaming limits, default policy. Example: `{ "hardware_accel": "nvenc", "transcode_path": "/tmp/transcodes", "max_concurrent_transcodes": 2, "global_max_concurrent_streams": 10, "global_max_concurrent_transcodes": 4, "global_internet_upload_speed_mbps": 50, "default_streaming_policy_id": "<uuid>", "max_downscale_resolution": "4k", "max_disk_space_mb": 4096, "segment_duration_seconds": 6 }`
- `metadata` — Default language, providers, refresh intervals, artwork, overlays, collections. Example: `{ "default_language": "en", "providers": ["tmdb", "tvdb"], "auto_refresh_hours": 6, "artwork_language_priority": ["en"], "artwork_auto_download": true, "artwork_download_originals_only": true, "asset_directory": null, "overlays_enabled": true, "overlay_apply_schedule": "0 5 * * *", "overlay_image_format": "webp", "overlay_image_quality": 90, "overlay_max_image_size_mb": 10, "overlay_default_font": "Inter", "overlay_reapply_on_artwork_change": true, "collections_enabled": true, "collection_sync_schedule": "0 6 * * *", "collection_default_poster_source": "auto", "collection_max_items_default": 100, "collection_track_missing": true, "collection_external_rate_limit_per_minute": 30 }`. Full schema documented in [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md), [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md), and [COLLECTIONS.md](COLLECTIONS.md)
- `security` — TLS config, stream signing, CORS origins, VPN detection. Example: `{ "allowed_origins": [], "tls": { "enabled": false, "port": 443, "acme_directory": "https://acme-v02.api.letsencrypt.org/directory", "acme_email": "", "challenge_type": "http-01", "cert_path": null, "key_path": null, "hsts_max_age_seconds": 63072000, "min_tls_version": "1.2" }, "stream_signing": { "enabled": false, "manifest_ttl_seconds": 60, "segment_ttl_seconds": 300, "key_rotation_hours": 24 }, "vpn_detection": { "auto_detect": true, "vpn_interfaces": ["tun0", "wg0", "utun", "tailscale0"] } }`. Full design documented in [SECURITY.md](../security/SECURITY.md)
- `auth` — Network mode, WebAuthn RP ID, first-run setup state, auth enforcement, invite code settings, device linking settings, re-auth settings, session timeouts, HTTP rate limits. Example: `{ "network_mode": "local", "rp_id": "media.example.com", "rp_origin": "https://media.example.com", "setup_complete": true, "auth_required": false, "require_https": false, "max_login_attempts": 5, "lockout_duration_minutes": 30, "invite_code_length": 24, "invite_code_default_expiry_days": 30, "invite_code_max_attempts_per_ip": 5, "invite_code_attempt_window_minutes": 15, "device_linking_code_length": 8, "device_linking_code_expiry_seconds": 900, "device_linking_poll_interval_seconds": 5, "reauth_code_length": 16, "reauth_code_expiry_hours": 24, "reauth_max_requests_per_user_per_day": 3, "session_absolute_timeout_days": 90, "session_idle_timeout_hours": null, "session_renewal_timeout_hours": 720, "rate_limits": { "global_per_minute": 100, "global_burst": 50, "auth_per_minute": 10, "auth_burst": 5, "authenticated_per_minute": 300, "authenticated_burst": 100, "streaming_per_minute": 600, "streaming_burst": 50, "admin_per_minute": 1000, "admin_burst": 200 } }`. Rate limit design documented in [API_CONVENTIONS.md](API_CONVENTIONS.md)
- `notifications` — Multi-channel dispatch configuration: webhook (URL, secret, format) + mobile push (enabled, provider). Example: `{ "webhook": { "url": "https://ntfy.example.com/duskcue", "secret": "<encrypted>", "format": "ntfy" }, "push": { "enabled": false, "provider": null } }`. Full schema documented in [MOBILE_PUSH.md](MOBILE_PUSH.md); the webhook secret is encrypted at rest via the existing `EncryptionKey` (AES-256-GCM)
- `backup` — WAL-G and pg_dump configuration, storage, retention. Example: `{ "wal_g_enabled": true, "wal_g_storage_type": "local", "wal_g_storage_path": "/data/backups/wal-g", "wal_g_retention_full": 7, "pg_dump_enabled": true, "pg_dump_storage_path": "/data/backups/dump", "pg_dump_retention_daily": 30, "archive_timeout_seconds": 60, "data_checksums": true, "verification_enabled": true }`. Full schema documented in [BACKUP_RECOVERY.md](../operations/BACKUP_RECOVERY.md)
- `integrations` — Classifarr and third-party integration settings. Example: `{ "classifarr_enabled": false, "subtitle_providers": { "opensubtitles": { "enabled": true, "api_key": "", "auto_fetch_enabled": true, "auto_fetch_languages": ["en"], "prefer_hearing_impaired": false }, "subdl": { "enabled": false, "api_key": "", "auto_fetch_enabled": false, "auto_fetch_languages": [] } } }`. Subtitle provider settings documented in [SUBTITLES.md](SUBTITLES.md)
- `logging` — Log level, file rotation, output format. Example: `{ "level": "info", "max_file_size_mb": 10, "max_files": 5, "format": "json" }`. Full design documented in [LOGGING_OBSERVABILITY.md](../operations/LOGGING_OBSERVABILITY.md)
- `storage` — Cache paths, size limits, eviction policies, disk space monitoring. Example: `{ "storyboard_path": "/cache/storyboards", "image_cache_path": "/cache/images", "hls_cache_path": "/cache/hls", "transcode_path": "/data/transcode", "storyboard_max_cache_gb": null, "image_cache_max_size_mb": 2048, "hls_cache_max_size_mb": 4096, "storyboard_eviction_policy": "lru", "disk_space_warnings": { "data_threshold_percent": 90, "cache_threshold_percent": 90, "transcode_threshold_percent": 80, "check_interval_seconds": 1800, "notify_on_warning": true } }`. Full design documented in [CACHE_STORAGE.md](../operations/CACHE_STORAGE.md)
- `maintenance` — Autovacuum tuning, REINDEX scheduling, partition retention, parent table ANALYZE. Example: `{ "autovacuum_tuning_enabled": true, "reindex_enabled": true, "reindex_schedule": "0 2 * * 0", "reindex_bloat_threshold_percent": 30, "reindex_min_index_size_mb": 10, "partition_retention_months": { "play_sessions": 24, "play_events": 12, "audit_log": 12 }, "analyze_parent_tables_enabled": true, "analyze_parent_schedule": "0 3 * * *" }`. Full design documented in [DATABASE_MAINTENANCE.md](../operations/DATABASE_MAINTENANCE.md)
- `resource_limits` — Concurrent transcode limits, CPU/memory thresholds, FFmpeg lifecycle timeouts, watchdog intervals. Example: `{ "max_concurrent_transcodes": 2, "transcode_cpu_threshold_percent": 90, "transcode_mem_threshold_percent": 85, "ffmpeg_idle_timeout_secs": 300, "ffmpeg_shutdown_grace_secs": 10, "watchdog_interval_secs": 60, "memory_warning_percent": 80, "memory_critical_percent": 90, "stale_session_timeout_secs": 600 }`. Full design documented in [MEMORY.md](MEMORY.md)
- `cpu` — FFmpeg threading, process priority, CPU affinity, hardware acceleration detection, thermal throttling. Example: `{ "transcode_cpu_threshold_percent": 90, "cpu_warning_percent": 80, "cpu_critical_percent": 90, "ffmpeg_threads": null, "ffmpeg_thread_type": "frame", "ffmpeg_nice": true, "ffmpeg_ionice": true, "cpu_affinity": null, "hw_accel_auto_detect": true, "thermal_throttle_enabled": true, "thermal_warning_celsius": 80, "thermal_critical_celsius": 85 }`. Full design documented in [CPU.md](CPU.md)
- `quality` — Device capability detection, network quality measurement, transcoding decision engine, QoE metrics, Dolby Vision handling, tone mapping, audio passthrough, subtitle strategy, version selection, quality mode. Example: `{ "capability_wizard_enabled": true, "network_probe_interval_minutes": 5, "network_probe_browsing_interval_minutes": 15, "network_probe_paused_interval_minutes": 10, "network_probe_bytes": 102400, "throughput_estimate_window": 5, "throughput_safety_factor": 0.8, "default_transcode_codec": "h264", "fallback_max_resolution": "1080p", "fallback_max_bitrate_bps": 6000000, "qoe_report_interval_seconds": 30, "allow_client_side_dv_fallback": true, "tone_mapping_algorithm": "bt2390", "tone_mapping_peak_nits": 100, "audio_passthrough_enabled": true, "subtitle_burn_in_policy": "last_resort", "default_quality_mode": "auto" }`. Full design documented in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md)
- `subtitles` — Subtitle OCR, synchronization, fetching, and delivery settings. Example: `{ "ocr_enabled": true, "ocr_engine": "paddleocr", "ocr_confidence_threshold": 0.80, "voice_activity_analysis": false, "voice_activity_schedule": "0 5 * * *", "default_subtitle_mode": "default", "default_subtitle_language": "en", "auto_fetch_enabled": true, "auto_fetch_languages": ["en"] }`. Full design documented in [SUBTITLES.md](SUBTITLES.md)
- `analytics` — IP geolocation, impossible travel detection, trust scoring, and location history settings. Example: `{ "geoip_enabled": true, "geoip_update_schedule": "0 3 * * 1", "impossible_travel_enabled": true, "velocity_threshold_kmh": 1000, "min_distance_km": 500, "lookback_hours": 24, "same_country_suppress": true, "trusted_ips": [], "trusted_cidrs": [] }`. Full design documented in [ANALYTICS_SECURITY.md](../security/ANALYTICS_SECURITY.md)

Each JSONB column maps to a typed Rust struct. The application deserializes on load, validates, and caches in memory. Changes write through to the database and trigger a cache reload.

#### Scheduled Tasks (Task Definitions)

```sql
CREATE TABLE scheduled_tasks (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    name TEXT NOT NULL UNIQUE,
    task_type TEXT NOT NULL CHECK (task_type IN (
        'library_scan', 'metadata_refresh', 'database_maintenance',
        'partition_management', 'session_cleanup', 'trakt_sync',
        'backup_database', 'backup_verification', 'database_integrity_check',
        'backup_retention_cleanup', 'media_health_check', 'notification_cleanup',
        'trust_recalculation', 'soft_delete_purge', 'segment_analysis',
        'storyboard_generation', 'disk_space_check', 'reindex_maintenance',
        'analyze_parents', 'transcode_health_check', 'subtitle_ocr',
        'subtitle_voice_analysis', 'subtitle_auto_fetch',
        'overlay_application', 'overlay_cleanup',
        'collection_sync', 'collection_cleanup',
        'artwork_refresh', 'asset_directory_scan',
        'migration_cleanup', 'system_requirement_check',
        'geoip_database_update',
        'backup_recovery_drill'
    )),

    cron_expression TEXT,
    interval_seconds INT,
    is_enabled BOOLEAN NOT NULL DEFAULT true,

    timeout_seconds INT NOT NULL DEFAULT 3600,
    max_retries INT NOT NULL DEFAULT 3,
    retry_delay_seconds INT NOT NULL DEFAULT 300,

    state TEXT NOT NULL DEFAULT 'idle' CHECK (state IN ('idle', 'queued', 'running', 'completed', 'failed', 'cancelled')),
    consecutive_failures INT NOT NULL DEFAULT 0,

    last_run_at TIMESTAMPTZ,
    last_run_duration_ms INT,
    last_run_result TEXT CHECK (last_run_result IN ('success', 'failure', 'timeout', 'cancelled')),
    last_error TEXT,
    next_run_at TIMESTAMPTZ,

    config JSONB NOT NULL DEFAULT '{}',
    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_scheduled_tasks_task_type ON scheduled_tasks (task_type);
CREATE INDEX idx_scheduled_tasks_state ON scheduled_tasks (state);
CREATE INDEX idx_scheduled_tasks_next_run_at ON scheduled_tasks (next_run_at) WHERE is_enabled = true;
```

Each task has either a `cron_expression` or `interval_seconds` — the application validates exactly one is set. A `cron_expression` like `0 3 * * *` means "daily at 03:00". An `interval_seconds` of `900` means "every 15 minutes from server start".

`consecutive_failures` — incremented on each failure, reset to 0 on success. After `max_retries` consecutive failures, the task is auto-disabled (`is_enabled = false`) and a `task_failed` notification is sent to admins. This prevents runaway error loops from silently consuming resources.

`next_run_at` — computed by the application. On task completion: next run = now + interval or next cron fire time. On task failure with retries remaining: next run = now + retry_delay. The scheduler queries `WHERE next_run_at <= now() AND is_enabled = true AND state != 'running'`.

`config` stores per-task parameters (e.g. `{ "mode": "full" }` for library scan, `{ "create_ahead_months": 2 }` for partition management).

#### Scheduled Task Runs (Execution History)

```sql
CREATE TABLE scheduled_task_runs (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    scheduled_task_id UUID NOT NULL REFERENCES scheduled_tasks(id) ON DELETE CASCADE,

    trigger_type TEXT NOT NULL CHECK (trigger_type IN ('scheduled', 'manual', 'retry')),
    state TEXT NOT NULL CHECK (state IN ('running', 'completed', 'failed', 'cancelled')),

    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    duration_ms INT,

    result TEXT CHECK (result IN ('success', 'failure', 'timeout', 'cancelled')),
    error_message TEXT,
    error_details JSONB,

    stats JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_scheduled_task_runs_task_id ON scheduled_task_runs (scheduled_task_id);
CREATE INDEX idx_scheduled_task_runs_started_at ON scheduled_task_runs (started_at DESC);
CREATE INDEX idx_scheduled_task_runs_state ON scheduled_task_runs (state);
CREATE INDEX idx_scheduled_task_runs_failed ON scheduled_task_runs (result) WHERE result = 'failure';
```

One row per task execution. The admin UI displays recent runs with results, durations, and error details. Completed runs are retained for auditing; a configurable retention policy (default 90 days) cleans old runs via the notification cleanup task.

`stats` stores task-specific metrics — e.g. `{ "items_scanned": 1234, "items_added": 5, "items_removed": 1, "items_updated": 3 }` for a library scan, or `{ "users_synced": 3, "items_pushed": 12, "errors": 0 }` for a Trakt sync.

`error_details` stores structured error information (e.g. `{ "step": "metadata_refresh", "provider": "tmdb", "status_code": 429, "message": "Rate limit exceeded" }`).

#### Notification Types (Event Definitions)

```sql
CREATE TABLE notification_types (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    name TEXT NOT NULL UNIQUE,
    category TEXT NOT NULL CHECK (category IN ('media', 'system', 'security', 'user', 'task')),
    priority TEXT NOT NULL DEFAULT 'low' CHECK (priority IN ('low', 'medium', 'high')),

    in_app_template TEXT NOT NULL,
    email_template TEXT,
    webhook_payload_template JSONB,

    is_enabled_by_default BOOLEAN NOT NULL DEFAULT true,

    metadata JSONB NOT NULL DEFAULT '{}'
);
```

Defines notification event types, seeded on first run. Templates store Fluent message IDs (kebab-case) resolved at render time via `services/i18n.rs` per the recipient's locale — see [I18N.md](I18N.md) "Phase 13 Notification Template Pattern".

`in_app_template` — required, stores a Fluent message ID (e.g., `new-media-added`). The renderer looks up the message in `server/locales/<lang>/notifications.ftl` and interpolates variables from the notification's `metadata` JSONB. Phase 13b Task 1 migration `20260628020000` converted the original English `{{variable}}` strings to Fluent IDs.

`email_template` — optional HTML email body. If null, this type is email-ineligible.

`webhook_payload_template` — optional JSON structure for webhook delivery. If null, this type is webhook-ineligible.

#### Notifications (Per-User Instances)

```sql
CREATE TABLE notifications (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    notification_type_id UUID NOT NULL REFERENCES notification_types(id) ON DELETE CASCADE,

    title TEXT NOT NULL,
    body TEXT NOT NULL,
    priority TEXT NOT NULL DEFAULT 'low' CHECK (priority IN ('low', 'medium', 'high')),

    link TEXT,

    is_read BOOLEAN NOT NULL DEFAULT false,
    read_at TIMESTAMPTZ,

    delivery_channels JSONB NOT NULL DEFAULT '["in_app"]',
    delivery_status JSONB NOT NULL DEFAULT '{}',

    related_item_type TEXT,
    related_item_id UUID,

    expires_at TIMESTAMPTZ,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_notifications_user_id ON notifications (user_id);
CREATE INDEX idx_notifications_type ON notifications (notification_type_id);
CREATE INDEX idx_notifications_unread ON notifications (user_id, created_at DESC) WHERE is_read = false;
CREATE INDEX idx_notifications_created_at ON notifications (created_at DESC);
CREATE INDEX idx_notifications_expires ON notifications (expires_at) WHERE expires_at IS NOT NULL;
```

One notification per user per event. The application creates notifications by:
1. Determining which users should receive this event (e.g. admins for `task_failed`, specific user for `trakt_sync_error`)
2. Checking each user's `user_notification_preferences` (or falling back to `is_enabled_by_default`)
3. Rendering the template with event variables
4. Inserting one row per recipient user
5. Delivering via enabled channels (in-app always, email/webhook if configured)

`delivery_channels` — array of channels attempted (e.g. `["in_app", "email"]`).
`delivery_status` — per-channel delivery state (e.g. `{ "in_app": "delivered", "email": "sent", "email_sent_at": "2026-06-01T10:30:00Z" }`).

`related_item_type` + `related_item_id` — polymorphic reference to the entity that triggered this notification (e.g. `("media_item", <uuid>)` for `new_media_added`, `("scheduled_task", <uuid>)` for `task_failed`). Enables deep-linking from notifications to the relevant admin page or media detail.

`expires_at` — auto-set based on priority. High priority: never expires. Medium: 30 days. Low: 90 days. The notification cleanup task removes expired rows.

#### User Notification Preferences (Per-User Per-Type Opt-In)

```sql
CREATE TABLE user_notification_preferences (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    notification_type_id UUID NOT NULL REFERENCES notification_types(id) ON DELETE CASCADE,

    in_app_enabled BOOLEAN NOT NULL DEFAULT true,
    email_enabled BOOLEAN NOT NULL DEFAULT false,
    webhook_enabled BOOLEAN NOT NULL DEFAULT false,

    UNIQUE(user_id, notification_type_id)
);

CREATE INDEX idx_user_notification_prefs_user ON user_notification_preferences (user_id);
```

Per-user per-notification-type channel preferences. If no row exists for a user + type, the application uses `notification_types.is_enabled_by_default`. Most users will have zero rows in this table — they accept defaults. Only explicit opt-in/out creates a row.

`push_enabled BOOLEAN NOT NULL DEFAULT false` was added by Phase 13b Task 2 migration `20260628030000` per [MOBILE_PUSH.md](MOBILE_PUSH.md) schema extension. Users opt into push per notification type via the preferences UI.

#### User Push Devices (Per-User Mobile Push Registration)

```sql
CREATE TABLE user_push_devices (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    provider TEXT NOT NULL CHECK (provider IN ('fcm', 'apns', 'unifiedpush')),

    token TEXT NOT NULL,

    device_name TEXT,
    platform TEXT,
    app_version TEXT,

    last_seen_at TIMESTAMPTZ,
    is_active BOOLEAN NOT NULL DEFAULT true,

    invalidated_at TIMESTAMPTZ,

    UNIQUE(user_id, provider, token)
);

CREATE INDEX idx_user_push_devices_user ON user_push_devices (user_id) WHERE is_active = true;
```

Created by Phase 13b Task 5 migration `20260629010000`. Per-user mobile push device registration for FCM/APNs/UnifiedPush tokens. Full design in [MOBILE_PUSH.md](MOBILE_PUSH.md).

- **`provider`** — push provider: `fcm` (Firebase Cloud Messaging, covers Android + iOS), `apns` (Apple Push Notification service, iOS-only direct), `unifiedpush` (Android-only, privacy-maximalist via distributor). CHECK constraint matches `PushDispatchConfig::is_configured()`.
- **`token`** — provider-specific opaque token. FCM: registration token (~152 chars, format may change per Google guidance — not pattern-validated). APNs: device token (historically 64 hex chars; Apple says "don't make assumptions about size"). UnifiedPush: endpoint URL (URL-validated at registration).
- **`UNIQUE(user_id, provider, token)`** — enables idempotent re-registration via `ON CONFLICT DO UPDATE`. Registration is an upsert: reactivates invalidated devices (`is_active = true, invalidated_at = NULL`) and refreshes `last_seen_at` + metadata.
- **`last_seen_at`** — updated on registration and heartbeat (`PUT /{id}`). The `notification_cleanup` scheduled task deactivates devices not seen in 30 days (`stale_device_days` config, default 30) by setting `is_active = false, invalidated_at = now()`.
- **`is_active` / `invalidated_at`** — lifecycle flags. `is_active = false` skips the device during push fan-out. Invalidation sources: (1) staleness (30-day no-heartbeat, server-side), (2) provider "token revoked" response (FCM `UNREGISTERED`, APNs `BadDeviceToken`/`Unregistered`, UnifiedPush 404/410), (3) manual revoke (`DELETE` — hard row removal, not soft-delete).
- **Partial index `WHERE is_active = true`** — the dispatch pipeline only fans out to active devices, so this index covers the hot path without indexing inactive historical rows.

---

## Quality Management Domain: Schema Design

### Overview

The quality management domain handles device capability detection, network quality measurement, transcoding decision logic, and quality of experience (QoE) metrics. This is the system that ensures optimal playback quality across diverse devices and network conditions. Full design documented in [QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md).

### Design Decisions

- **Per-device-model profiles** — identical devices share capability data; the `device_identifier` (platform + model + OS version) is the grouping key
- **Empirical capability wizard** — clients test actual playback of sample clips; results override self-reported capabilities; addresses the Jellyfin community's #1 pain point (devices misreporting capabilities)
- **Passive + active network measurement** — segment download telemetry (ongoing, zero overhead) plus periodic probe downloads (every 5 minutes); harmonic mean of last 5 segment throughputs for ABR estimation
- **QoE metrics per session** — five industry-standard metrics (startup time, rebuffer ratio, average bitrate, switches per minute, quality drops); reported by client every 30 seconds
- **No partitioning** — `device_profiles` and `device_capability_tests` are proportional to device count (not playback activity); `client_network_reports` and `qoe_reports` are moderate volume (one row per segment/interval per session)

### Entity-Relationship Overview

```
device_profiles ──< device_capability_tests

users ──< client_network_reports (per session, per segment/probe)
users ──< qoe_reports (per session, periodic)
```

### Schema DDL

#### Device Profiles (Per-Device-Model Capabilities)

```sql
CREATE TABLE device_profiles (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    device_identifier TEXT NOT NULL,

    platform TEXT NOT NULL,
    model TEXT,
    os_version TEXT,
    client_name TEXT,
    client_version TEXT,

    video_codecs JSONB NOT NULL DEFAULT '[]',
    audio_codecs JSONB NOT NULL DEFAULT '[]',
    subtitle_formats JSONB NOT NULL DEFAULT '[]',
    containers JSONB NOT NULL DEFAULT '[]',

    max_resolution TEXT,
    max_framerate INT,
    hdr_support JSONB NOT NULL DEFAULT '[]',

    max_audio_channels INT,
    spatial_audio BOOLEAN NOT NULL DEFAULT false,

    max_bitrate_bps BIGINT,

    allow_client_side_dv_fallback BOOLEAN NOT NULL DEFAULT true,

    profile_source TEXT NOT NULL DEFAULT 'client_report'
        CHECK (profile_source IN ('client_report', 'capability_wizard', 'known_device', 'manual')),

    wizard_completed_at TIMESTAMPTZ,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE UNIQUE INDEX device_profiles_identifier ON device_profiles (device_identifier);
CREATE INDEX device_profiles_platform ON device_profiles (platform);
CREATE INDEX device_profiles_source ON device_profiles (profile_source);
```

`device_identifier` — a stable key derived from the client's platform, model, and OS version. Example: `web_chrome_142_macos`, `app_android_tv_lg_c3_13`, `app_tvos_apple_tv_4k_17.5`. Devices with the same identifier share capabilities.

`video_codecs` — JSONB array of supported video codecs with profiles and levels. Example: `[{"codec": "h264", "profiles": ["baseline", "main", "high"], "max_level": 4.2, "max_bit_depth": 8}, {"codec": "hevc", "profiles": ["main"], "max_level": 5.1, "max_bit_depth": 10}]`.

`audio_codecs` — JSONB array of supported audio codecs with max channels. Example: `[{"codec": "aac", "max_channels": 6}, {"codec": "ac3", "max_channels": 6}, {"codec": "eac3", "max_channels": 6}]`.

`hdr_support` — JSONB array of supported HDR formats. Example: `["sdr", "hdr10", "dolby_vision"]`. Empty means SDR only.

`profile_source` — how the profile was created. `client_report` (default, client self-reported), `capability_wizard` (empirical test results override client report), `known_device` (from server-side device database), `manual` (admin override).

`wizard_completed_at` — when the capability wizard was last completed for this device model. Null if never run.

#### Device Capability Tests (Wizard Results)

```sql
CREATE TABLE device_capability_tests (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    device_profile_id UUID NOT NULL REFERENCES device_profiles(id) ON DELETE CASCADE,

    test_format TEXT NOT NULL,
    test_description TEXT NOT NULL,

    result TEXT NOT NULL CHECK (result IN ('success', 'failed', 'stuttered')),

    actual_codec TEXT,
    actual_resolution TEXT,
    actual_bit_depth INT,
    actual_dynamic_range TEXT,

    details JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_device_capability_tests_profile ON device_capability_tests (device_profile_id);
CREATE INDEX idx_device_capability_tests_format ON device_capability_tests (test_format);
```

`test_format` — identifier for the test clip. Example: `h264_8bit_1080p_mp4`, `hevc_10bit_4k_hdr10_mkv`, `av1_8bit_1080p_mp4`, `dolby_vision_p8_mp4`.

`result` — `success` (played without issues), `failed` (playback error or codec not supported), `stuttered` (played but with visible issues, possible decode performance problem).

`details` — additional test metadata. Example: `{ "drop_frames": 12, "decode_time_ms": 340, "audio_sync_drift_ms": 50 }`.

#### Client Network Reports (Per-Session Network Measurements)

```sql
CREATE TABLE client_network_reports (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_id UUID NOT NULL,

    report_type TEXT NOT NULL CHECK (report_type IN ('segment', 'probe')),

    segment_index INT,
    rung TEXT,

    payload_bytes BIGINT,
    download_start_ms BIGINT,
    download_end_ms BIGINT,
    throughput_bps BIGINT,

    buffer_seconds REAL,
    rebuffer_count INT,
    rebuffer_total_ms INT,

    estimated_throughput_bps BIGINT,
    network_tier TEXT CHECK (network_tier IN ('excellent', 'good', 'moderate', 'slow', 'very_slow', 'critical')),

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_client_network_reports_user ON client_network_reports (user_id);
CREATE INDEX idx_client_network_reports_session ON client_network_reports (session_id);
CREATE INDEX idx_client_network_reports_created ON client_network_reports (created_at DESC);
CREATE INDEX idx_client_network_reports_tier ON client_network_reports (network_tier);
```

`report_type` — `segment` (passive measurement from HLS segment download telemetry) or `probe` (active measurement from periodic bandwidth probe download).

`throughput_bps` — calculated throughput for this individual measurement. For segments: `payload_bytes * 8 / (download_end_ms - download_start_ms) * 1000`. For probes: same calculation.

`estimated_throughput_bps` — the running harmonic mean of the last 5 measurements at the time of this report. This is the value used for ABR decisions.

`network_tier` — classification based on `estimated_throughput_bps`. Updated on each report. Used for admin analytics and starting rung selection for new sessions.

Retention: reports older than 7 days are cleaned by the `notification_cleanup` scheduled task (extended to cover network reports). Aggregate statistics (per-user, per-session averages) are maintained in `play_sessions.metadata` at session end.

#### QoE Reports (Quality of Experience Metrics)

```sql
CREATE TABLE qoe_reports (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_id UUID NOT NULL,

    report_interval_seconds INT NOT NULL DEFAULT 30,

    startup_time_ms INT,
    rebuffer_ratio REAL,
    average_bitrate_bps BIGINT,
    switches_per_minute REAL,
    quality_drops INT,

    current_rung TEXT,
    current_buffer_seconds REAL,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_qoe_reports_user ON qoe_reports (user_id);
CREATE INDEX idx_qoe_reports_session ON qoe_reports (session_id);
CREATE INDEX idx_qoe_reports_created ON qoe_reports (created_at DESC);
```

Reports are sent by the client every 30 seconds during playback. Five industry-standard metrics:

- `startup_time_ms` — seconds from Play request to first frame rendered. Set only on the first report of a session.
- `rebuffer_ratio` — fraction of viewing time spent buffering. Target: < 0.5%.
- `average_bitrate_bps` — mean bitrate of played video. As high as network allows.
- `switches_per_minute` — ABR ladder switches per viewing minute. Target: < 0.5.
- `quality_drops` — count of downward quality switches. Target: < 2 per session.

Retention: same as `client_network_reports` — 7-day detail, aggregate at session end.

---

## Metadata Overlays Domain: Schema Design

### Overview

The overlay engine composites badges, text, and visual indicators onto poster artwork. Full design documented in [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md). Poster management and artwork lifecycle documented in [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md).

### Design Decisions

- **Pure Rust compositing** — `image` + `ab_glyph` + `resvg`; no Python or external dependencies for overlay generation
- **Standard canvas** — 1000×1500 posters, 1920×1080 backdrops; all source artwork scaled before compositing
- **Condition-based filtering** — JSONB conditions evaluated against `media_items` and `media_files` metadata
- **Incremental reprocessing** — only re-composite items whose overlay configuration has changed (tracked via `overlay_config_hash`)
- **Clean art preservation** — source artwork never modified; clean backups stored in `/cache/images/clean/`
- **Artwork locking** — `artwork.is_locked` prevents auto-refresh from overwriting user-selected artwork

### Entity-Relationship Overview

```
overlay_definitions ── (evaluated against) ── media_items

media_items ──< artwork_overlay_state (0..1 per artwork_type)

artwork ── (source for) ── artwork_overlay_state
```

### Schema DDL

#### Overlay Definitions

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

Full field descriptions and overlay type documentation in [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md).

#### Artwork Overlay State

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

#### Artwork Table Extension

```sql
ALTER TABLE artwork ADD COLUMN is_locked BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE artwork ADD COLUMN source_type TEXT
    CHECK (source_type IS NULL OR source_type IN ('tmdb', 'user_upload', 'asset_directory', 'community'));
```

---

## Collections Domain: Schema Design

### Overview

Server-level media groupings: manually curated static collections, dynamically generated smart collections, and builder-populated collections from local metadata and external APIs. Full design documented in [COLLECTIONS.md](COLLECTIONS.md).

### Design Decisions

- **Server-level collections** — visible to all users with library access (unlike playlists which are user-specific)
- **Three collection types** — static (manual), dynamic (builder-populated on schedule), smart (filter-evaluated at query time)
- **Builder sources** — 14 internal (genre, decade, actor, director, franchise, etc.) + 13 external (TMDb, Trakt, IMDb, custom URL)
- **Sync mode** — `sync` (add + remove) or `append` (add only, never remove)
- **Missing item tracking** — external builder items not in local library flagged for admin follow-up

### Entity-Relationship Overview

```
collections ──< collection_items >── media_items

collections ──> artwork (poster_artwork_id, backdrop_artwork_id)

collection_templates (standalone, for import/export)
```

### Schema DDL

#### Collections

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

#### Collection Items

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

#### Collection Templates

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

Full field descriptions and builder documentation in [COLLECTIONS.md](COLLECTIONS.md).

### Platform Migration Domain

Three tables track migration of watch data from Plex, Jellyfin, and Emby.

```sql
CREATE TABLE migration_sources (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    platform TEXT NOT NULL CHECK (platform IN ('plex', 'jellyfin', 'emby')),
    name TEXT NOT NULL,
    connection_config JSONB NOT NULL,

    last_run_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'discovering', 'matching', 'importing', 'completed', 'failed', 'cancelled'))
);

CREATE TABLE migration_user_mapping (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    migration_source_id UUID NOT NULL REFERENCES migration_sources(id) ON DELETE CASCADE,
    source_user_id TEXT NOT NULL,
    source_user_name TEXT NOT NULL,

    platform_user_id UUID REFERENCES users(id) ON DELETE CASCADE,

    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'skipped', 'imported', 'failed')),
    CHECK (
        (status = 'skipped' AND platform_user_id IS NULL)
        OR (status <> 'skipped' AND platform_user_id IS NOT NULL)
    ),

    items_matched INT NOT NULL DEFAULT 0,
    items_unmatched INT NOT NULL DEFAULT 0,
    items_imported INT NOT NULL DEFAULT 0,
    items_skipped INT NOT NULL DEFAULT 0,
    imported_at TIMESTAMPTZ,

    UNIQUE(migration_source_id, source_user_id)
);

CREATE TABLE migration_import_log (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    migration_source_id UUID NOT NULL REFERENCES migration_sources(id) ON DELETE CASCADE,
    migration_user_mapping_id UUID NOT NULL REFERENCES migration_user_mapping(id) ON DELETE CASCADE,

    source_item_id TEXT NOT NULL,
    source_item_title TEXT NOT NULL,
    source_item_type TEXT NOT NULL CHECK (source_item_type IN ('movie', 'episode')),
    source_item_year INT,
    source_provider_ids JSONB NOT NULL DEFAULT '{}',
    source_is_watched BOOLEAN NOT NULL DEFAULT FALSE,
    source_play_count INT NOT NULL DEFAULT 0 CHECK (source_play_count >= 0),
    source_resume_position_ms BIGINT NOT NULL DEFAULT 0 CHECK (source_resume_position_ms >= 0),
    source_last_played_at TIMESTAMPTZ,
    source_item_metadata JSONB NOT NULL DEFAULT '{}',

    matched_media_item_id UUID REFERENCES media_items(id) ON DELETE SET NULL,
    match_method TEXT CHECK (match_method IN ('tmdb_id', 'imdb_id', 'tvdb_id', 'title_year', 'series_episode', 'manual', 'unmatched')),
    match_confidence TEXT CHECK (match_confidence IS NULL OR match_confidence IN ('high', 'medium', 'low', 'unmatched')),

    imported_user_item_data_id UUID REFERENCES user_item_data(id) ON DELETE SET NULL,
    import_batch_id UUID,
    previous_user_item_data JSONB,
    imported_at TIMESTAMPTZ,
    rolled_back_at TIMESTAMPTZ,
    rollback_detail TEXT,
    status TEXT NOT NULL CHECK (status IN ('discovered', 'matched', 'unmatched', 'imported', 'rolled_back', 'skipped', 'error')),
    error_detail TEXT,

    UNIQUE(migration_user_mapping_id, source_item_id)
);
```

Phase 14 Task 1 hardens the migration domain with these supporting indexes:

```sql
CREATE INDEX idx_migration_sources_status ON migration_sources (status);
CREATE INDEX idx_migration_user_mapping_source ON migration_user_mapping (migration_source_id);
CREATE INDEX idx_migration_import_log_source ON migration_import_log (migration_source_id);
CREATE INDEX idx_migration_import_log_status ON migration_import_log (status);
CREATE INDEX idx_migration_import_log_matched_media
    ON migration_import_log (matched_media_item_id)
    WHERE matched_media_item_id IS NOT NULL;
CREATE INDEX idx_migration_import_log_import_batch
    ON migration_import_log (migration_source_id, import_batch_id)
    WHERE import_batch_id IS NOT NULL;
CREATE INDEX idx_migration_import_log_rollback
    ON migration_import_log (migration_source_id, status, imported_at)
    WHERE imported_user_item_data_id IS NOT NULL;
```

Full migration flow, JSONB schemas, and wizard documentation in [MIGRATIONS.md](MIGRATIONS.md).

---

## Cross-Cutting Concerns: Schema Design

### Overview

Four cross-cutting concerns span all domains: soft delete, partitioning, full-text search, and audit trail. These are implemented as infrastructure-level patterns applied consistently across the schema.

### 1. Soft Delete

#### Design Decisions

- **Hybrid approach:** soft-delete immediately via `deleted_at` column, then hard-purge expired rows via a scheduled task
- **`deleted_at` (TIMESTAMPTZ)** preferred over `is_deleted` (BOOLEAN) — records *when* the deletion occurred, enables time-based purge policies
- **Partial unique indexes** on business keys — unique constraints only apply to non-deleted rows, allowing the same username/slug to be reused after deletion
- **30-day recovery window** — soft-deleted rows are kept for 30 days, then hard-purged by the `soft_delete_purge` scheduled task
- **Only tables where users expect undo** — libraries, users, and playlists. All other tables use hard delete.

#### Tables with Soft Delete

| Table | Rationale | Unique Constraints Adjusted |
|---|---|---|
| `libraries` | Accidentally deleted libraries should be recoverable; cascading would destroy all media items | `slug` → partial unique `WHERE deleted_at IS NULL` |
| `users` | User accounts should be recoverable; audit trail needed for security | `username`, `email` → partial unique `WHERE deleted_at IS NULL` |
| `playlists` | Users expect trash/undo for playlists | No unique constraints affected |

#### Application Behavior

- All queries against soft-deletable tables include `WHERE deleted_at IS NULL` (enforced at the application/query layer)
- Soft delete sets `deleted_at = now()`, does not cascade to child rows (media items in a deleted library remain until purged)
- Restore sets `deleted_at = NULL`
- The `soft_delete_purge` scheduled task hard-deletes rows where `deleted_at < now() - interval '30 days'`
- Admin UI shows soft-deleted items in a "Trash" view with restore/permanent-delete actions

#### DDL Changes

The `deleted_at` columns and partial unique indexes are already included in the respective table DDL above (`libraries`, `users`, `playlists`).

### 2. Partitioning Strategy

#### Design Decisions

- **Range partitioning by timestamp, monthly granularity** — the standard for time-series and append-only data
- **Application-level partition management** — the `partition_management` scheduled task creates next month's partitions in advance and drops old partitions past retention
- **No `pg_partman` dependency** — our predictable monthly schedule is easily managed by a scheduled task; avoids extension dependency for Synology NAS and Docker deployments
- **DETACH PARTITION CONCURRENTLY** for dropping old data — near-instant, no VACUUM overhead, minimal locking

#### Partitioned Tables

| Table | Partition Key | Granularity | Retention | Rationale |
|---|---|---|---|---|
| `play_sessions` | `started_at` | Monthly | 2 years | High-volume append-only; queries always filter by date |
| `play_events` | `event_at` | Monthly | 1 year | Higher volume than sessions; granular event data |
| `audit_log` | `changed_at` | Monthly | 1 year | Audit logs grow fast; partition dropping is standard retention |

#### Partition Management

The `partition_management` scheduled task (runs monthly on the 1st):

1. **Create** next month's partitions for all partitioned tables (`create_ahead_months` config, default 2)
2. **Detach and drop** partitions older than the retention period for each table
3. **Report** stats: partitions created, partitions dropped, total rows affected

Example partition creation:
```sql
CREATE TABLE play_sessions_2026_07 PARTITION OF play_sessions
    FOR VALUES FROM ('2026-07-01') TO ('2026-08-01');

CREATE TABLE play_events_2026_07 PARTITION OF play_events
    FOR VALUES FROM ('2026-07-01') TO ('2026-08-01');

CREATE TABLE audit_log_2026_07 PARTITION OF audit_log
    FOR VALUES FROM ('2026-07-01') TO ('2026-08-01');
```

Example partition drop:
```sql
ALTER TABLE play_sessions DETACH PARTITION play_sessions_2024_05 CONCURRENTLY;
DROP TABLE play_sessions_2024_05;
```

### 3. Full-Text Search

#### Design Decisions

- **Regular `search_vector` column (not generated)** — needs cross-table data (cast names, genres, tags) that generated columns cannot access
- **Trigger-maintained** — triggers on `media_items`, `media_credits`, `media_genres`, and `media_tags` rebuild the parent item's search vector when relevant data changes
- **Field weighting** — title matches (weight A) rank higher than overview (B), cast names (C), and genres/tags (D)
- **GIN index** for fast tsvector lookups — preferred over GiST for read-heavy search workloads
- **`pg_trgm` extension** for fuzzy/typo-tolerant fallback — when FTS returns no results, trigram similarity catches typos
- **`websearch_to_tsquery()`** for user-facing search input — handles quotes, OR, negation in Google-like syntax
- **Language config per library** — uses `libraries.metadata_language` to select the appropriate text search configuration (default `'english'`)

#### Search Pipeline

```
User types query
    │
    ├─ 1. websearch_to_tsquery() → tsvector @@ tsquery with GIN index → ranked results
    │
    └─ 2. If no results: pg_trgm similarity on title → fuzzy fallback results
```

#### Query Example

```sql
SELECT mi.title, mi.overview,
    ts_rank_cd(mi.search_vector, query) AS rank,
    ts_headline('english', mi.overview, query,
        'StartSel=<mark>, StopSel=</mark>, MaxWords=35, MinWords=15') AS snippet
FROM media_items mi, websearch_to_tsquery('english', 'inception nolan') query
WHERE mi.search_vector @@ query
    AND mi.deleted_at IS NULL
ORDER BY rank DESC
LIMIT 20;
```

#### DDL: Extensions

```sql
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS pgstattuple;
```

#### DDL: Search Vector Trigger Function

```sql
CREATE OR REPLACE FUNCTION rebuild_media_search_vector()
RETURNS TRIGGER AS $$
DECLARE
    target_id UUID;
    lang TEXT;
    cfg REGCONFIG;
BEGIN
    IF TG_TABLE_NAME = 'media_items' THEN
        target_id := COALESCE(NEW.id, OLD.id);
    ELSIF TG_TABLE_NAME IN ('media_credits', 'media_genres', 'media_tags') THEN
        target_id := COALESCE(NEW.media_item_id, OLD.media_item_id);
    END IF;

    SELECT COALESCE(metadata_language, 'en') INTO lang
    FROM media_items mi JOIN libraries l ON mi.library_id = l.id
    WHERE mi.id = target_id;

    cfg := lang::REGCONFIG;

    UPDATE media_items SET search_vector =
        setweight(to_tsvector(cfg, COALESCE(title, '')), 'A') ||
        setweight(to_tsvector(cfg, COALESCE(original_title, '')), 'A') ||
        setweight(to_tsvector(cfg, COALESCE(overview, '')), 'B') ||
        setweight(to_tsvector(cfg, COALESCE(
            (SELECT string_agg(p.name, ' ')
             FROM media_credits mc JOIN people p ON mc.person_id = p.id
             WHERE mc.media_item_id = target_id), '')), 'C') ||
        setweight(to_tsvector(cfg, COALESCE(
            (SELECT string_agg(g.name, ' ')
             FROM media_genres mg JOIN genres g ON mg.genre_id = g.id
             WHERE mg.media_item_id = target_id), '')), 'D') ||
        setweight(to_tsvector(cfg, COALESCE(
            (SELECT string_agg(t.name, ' ')
             FROM media_tags mt JOIN tags t ON mt.tag_id = t.id
             WHERE mt.media_item_id = target_id), '')), 'D')
    WHERE id = target_id;

    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;
```

#### DDL: Search Triggers

```sql
CREATE TRIGGER media_items_search_vector
    AFTER INSERT OR UPDATE OF title, original_title, overview ON media_items
    FOR EACH ROW EXECUTE FUNCTION rebuild_media_search_vector();

CREATE TRIGGER media_credits_search_vector
    AFTER INSERT OR UPDATE OR DELETE ON media_credits
    FOR EACH ROW EXECUTE FUNCTION rebuild_media_search_vector();

CREATE TRIGGER media_genres_search_vector
    AFTER INSERT OR UPDATE OR DELETE ON media_genres
    FOR EACH ROW EXECUTE FUNCTION rebuild_media_search_vector();

CREATE TRIGGER media_tags_search_vector
    AFTER INSERT OR UPDATE OR DELETE ON media_tags
    FOR EACH ROW EXECUTE FUNCTION rebuild_media_search_vector();
```

#### DDL: Trigram Index for Fuzzy Search

```sql
CREATE INDEX idx_media_items_title_trgm ON media_items USING GIN (title gin_trgm_ops);
```

This index also accelerates `ILIKE '%pattern%'` queries on titles.

### 4. Audit Trail

#### Design Decisions

- **Trigger-based audit table** — captures ALL changes regardless of source (application, direct SQL, admin scripts, migrations)
- **Generic trigger function** — one function handles all audited tables; `to_jsonb(OLD)` / `to_jsonb(NEW)` captures complete row state
- **Application context via session variables** — the application sets `app.current_user_id` and `app.current_user_email` on each authenticated DB connection so the trigger knows who made the change
- **Sensitive field redaction** — password hashes, tokens, and secrets are stripped from JSONB before storage
- **Range-partitioned by month** — audit logs grow fast; partition dropping provides efficient retention management
- **1-year retention** — partitions older than 1 year are detached and dropped by the partition management task
- **Application-level audit for business events** — events like login, logout, and Trakt sync don't correspond to row changes; these are logged by the application into the same `audit_log` table

#### Audited Tables

| Table | Rationale |
|---|---|
| `users` | Account changes are security-sensitive |
| `user_passkeys` | Auth credential changes |
| `user_totp` | Auth credential changes |
| `user_capabilities` | Permission changes need audit trail |
| `user_library_access` | Access grant/revoke needs audit trail |
| `api_keys` | Key creation/rotation/revocation |
| `invitations` | Invitation creation and usage |
| `server_config` | All configuration changes should be auditable |
| `scheduled_tasks` | Task enable/disable/config changes |
| `libraries` | Library creation/modification/deletion |
| `media_segments` | Manual segment overrides (user edits to intro/credits timestamps) |

#### Schema DDL

##### Audit Log (Range-Partitioned by Month)

```sql
CREATE TABLE audit_log (
    id UUID DEFAULT uuidv7(),
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    table_name TEXT NOT NULL,
    row_id UUID NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('INSERT', 'UPDATE', 'DELETE')),

    old_data JSONB,
    new_data JSONB,
    changed_fields TEXT[],

    user_id UUID,
    db_user TEXT NOT NULL DEFAULT current_user,
    client_addr INET,
    application_name TEXT,

    changed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    transaction_id BIGINT NOT NULL DEFAULT txid_current()
) PARTITION BY RANGE (changed_at);

CREATE INDEX idx_audit_log_table_row ON audit_log (table_name, row_id, changed_at DESC);
CREATE INDEX idx_audit_log_id ON audit_log (id);
CREATE INDEX idx_audit_log_user ON audit_log (user_id, changed_at DESC) WHERE user_id IS NOT NULL;
CREATE INDEX idx_audit_log_time ON audit_log (changed_at DESC);
CREATE INDEX idx_audit_log_transaction ON audit_log (transaction_id);
```

Example monthly partition:
```sql
CREATE TABLE audit_log_2026_06 PARTITION OF audit_log
    FOR VALUES FROM ('2026-06-01') TO ('2026-07-01');
```

##### Generic Audit Trigger Function

```sql
CREATE OR REPLACE FUNCTION audit_trigger_fn()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
DECLARE
    v_old_data JSONB;
    v_new_data JSONB;
    v_changed TEXT[];
    v_row_id UUID;
    v_user_id UUID;
BEGIN
    BEGIN
        v_user_id := current_setting('app.current_user_id', TRUE)::UUID;
    EXCEPTION WHEN OTHERS THEN
        v_user_id := NULL;
    END;

    IF TG_OP = 'INSERT' THEN
        v_new_data := to_jsonb(NEW);
        v_old_data := NULL;
        v_row_id := NEW.id;
        v_changed := NULL;
    ELSIF TG_OP = 'UPDATE' THEN
        v_old_data := to_jsonb(OLD);
        v_new_data := to_jsonb(NEW);
        v_row_id := NEW.id;

        SELECT array_agg(key ORDER BY key) INTO v_changed
        FROM (
            SELECT key
            FROM jsonb_each(v_new_data) n
            WHERE n.value IS DISTINCT FROM (v_old_data -> n.key)
        ) changed;
    ELSIF TG_OP = 'DELETE' THEN
        v_old_data := to_jsonb(OLD);
        v_new_data := NULL;
        v_row_id := OLD.id;
        v_changed := NULL;
    END IF;

    IF TG_OP = 'UPDATE' AND (v_changed IS NULL OR array_length(v_changed, 1) = 0) THEN
        RETURN NULL;
    END IF;

    v_old_data := v_old_data
        - 'password_hash' - 'access_token' - 'refresh_token'
        - 'secret' - 'key_hash' - 'token_hash'
        - 'backup_codes';
    v_new_data := v_new_data
        - 'password_hash' - 'access_token' - 'refresh_token'
        - 'secret' - 'key_hash' - 'token_hash'
        - 'backup_codes';

    INSERT INTO audit_log (
        table_name, row_id, operation,
        old_data, new_data, changed_fields,
        user_id, db_user, client_addr, application_name
    ) VALUES (
        TG_TABLE_NAME, v_row_id, TG_OP,
        v_old_data, v_new_data, v_changed,
        v_user_id, session_user,
        inet_client_addr(),
        current_setting('application_name', TRUE)
    );

    RETURN NULL;
END;
$$;
```

##### Attaching Audit Triggers

```sql
CREATE TRIGGER audit_users
    AFTER INSERT OR UPDATE OR DELETE ON users
    FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();

CREATE TRIGGER audit_user_passkeys
    AFTER INSERT OR UPDATE OR DELETE ON user_passkeys
    FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();

CREATE TRIGGER audit_user_totp
    AFTER INSERT OR UPDATE OR DELETE ON user_totp
    FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();

CREATE TRIGGER audit_user_capabilities
    AFTER INSERT OR UPDATE OR DELETE ON user_capabilities
    FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();

CREATE TRIGGER audit_user_library_access
    AFTER INSERT OR UPDATE OR DELETE ON user_library_access
    FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();

CREATE TRIGGER audit_api_keys
    AFTER INSERT OR UPDATE OR DELETE ON api_keys
    FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();

CREATE TRIGGER audit_invitations
    AFTER INSERT OR UPDATE OR DELETE ON invitations
    FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();

CREATE TRIGGER audit_server_config
    AFTER INSERT OR UPDATE ON server_config
    FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();

CREATE TRIGGER audit_scheduled_tasks
    AFTER INSERT OR UPDATE ON scheduled_tasks
    FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();

CREATE TRIGGER audit_libraries
    AFTER INSERT OR UPDATE OR DELETE ON libraries
    FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
```

`server_config` and `scheduled_tasks` triggers use `INSERT OR UPDATE` (not DELETE) — these tables are not deleted, only updated.

##### Application Context Setup

The Rust application sets session variables on each authenticated DB connection so the trigger can attribute changes to the correct user:

```sql
SELECT set_config('app.current_user_id', $1, TRUE);
SELECT set_config('app.current_user_email', $2, TRUE);
```

The `TRUE` parameter scopes the variable to the current transaction. This is called at the start of each authenticated request within a transaction.

##### Querying the Audit Log

```sql
SELECT operation, old_data, new_data, changed_fields, changed_at
FROM audit_log
WHERE table_name = 'users' AND row_id = $1
ORDER BY changed_at DESC
LIMIT 50;

SELECT table_name, row_id, operation, changed_fields, changed_at
FROM audit_log
WHERE user_id = $1 AND changed_at > now() - interval '7 days'
ORDER BY changed_at DESC;
```

---

## Research Sources

### Surrogate Keys & PostgreSQL 18
- PostgreSQL 18 Release Notes: https://www.postgresql.org/docs/18/release-18.html
- PostgreSQL 18 UUID Functions: https://www.postgresql.org/docs/18/functions-uuid.html
- UUIDv7 RFC 9562: https://tools.ietf.org/html/rfc9562
- Bytebase — What's New in PostgreSQL 18 (April 2026)
- Neon — PostgreSQL 18 New Features (June 2025)
- Nile Postgres — UUIDv7 Comes to PostgreSQL 18 (May 2025)
- Supabase — Choosing a Postgres Primary Key
- Aiven — Exploring PostgreSQL 18 UUIDv7 Support (benchmarks)
- Avid Perf — UUID Benchmark War (February 2024)

### Core Media Domain Schema
- Table Inheritance Patterns: Single Table vs Class Table vs Concrete Table (Khrenov, October 2025)
- 10 Database Design Best Practices for 2025 (Automatic Nation)
- AWS — PostgreSQL as a JSON Database: Advanced Patterns and Best Practices (November 2025)
- Architecture Weekly — PostgreSQL JSONB: Powerful Storage for Semi-Structured Data (April 2025)
- Riven Media Automation — Architecture & Database Entity Model (riven.tv)
- Jellyfin — Library Organization & Content Types (jellyfin.org)

### Trakt.tv Integration
- Trakt API Official Documentation: https://trakt.docs.apiary.io/
- Trakt API Source Code (GitHub): https://github.com/trakt/trakt-api
- Trakt API Pagination & Sorting Updates Discussion (GitHub #681, January 2026)
- Trakt Forums — Updating Trakt Limits for 2026 (February 2026)
- Trakt Forums — Rate Limit Discussion (January 2025)

### Activity & Analytics Domain
- Tautulli Official Site & Feature List: https://tautulli.com/
- Tautulli Database Schema (GitHub): https://github.com/Tautulli/Tautulli/blob/master/plexpy/__init__.py
- Tracearr — Real-Time Monitoring for Plex, Jellyfin, and Emby: https://github.com/connorgallopo/Tracearr
- PostgreSQL Partitioning — 4 Strategies for Managing Large Tables (February 2026)
- LeanIX Engineering — PostgreSQL Partitioned Tables: A Practical Guide (May 2026)
- CrateDB — Best Time Series Databases for Real Time Workloads in 2026

### Classifarr Integration
- Classifarr GitHub Repository: https://github.com/cloudbyday90/Classifarr
- Classifarr Policy Engine Architecture: https://github.com/cloudbyday90/Classifarr/blob/main/docs/architecture/policy-engine.md
- Classifarr Reddit Introduction (r/selfhosted, December 2025) — initial announcement with RAG feature
- Classifarr Policy Engine Announcement (r/PleX, February 2026) — v0.37+ architecture, confidence-based routing
- Classifarr Image Embedding Service: https://github.com/cloudbyday90/classifarr-image-embedding-service
- Classifarr Unraid Community Apps listing

### Playback Domain
- Plex Database Structure & Schema (databasesample.com)
- Plex `metadata_item_settings` table — `view_count`, `view_offset` columns (GitHub Gist #3086896)
- Jellyfin 10.11 EF Core `UserData` entity — `PlaybackPositionTicks`, `PlayCount`, `IsFavorite`, `Played`, `LastPlayedDate`
- Jellyfin Database Optimization for Larger Libraries (Reddit r/jellyfin, January 2026) — N+1 query issues with UserData batch loading
- Plex Continue Watching/On Deck behavior analysis (Plex Forums, February 2024)
- YouTube "Up Next" recommendation system architecture — two-stage candidate generation + ranking model (Google Brain, 2016)

### User & Authentication Domain
- Google — Introduction to Server-Side Passkey Implementation (May 2025): https://developers.google.com/identity/passkeys/developer-guides/server-introduction
- OWASP Authentication Cheat Sheet (2024): https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html
- OWASP Password Storage Cheat Sheet (2024) — Argon2id recommendation: https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html
- OWASP Session Management Cheat Sheet (2024): https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html
- Cerbos — Best Open Source Auth Tools & Software for Enterprises (February 2026)
- Medium — RBAC, ABAC, and PBAC: What, Why, When and How (February 2026)
- Frontegg — RBAC vs ABAC vs PBAC: What Is the Difference? (March 2025)
- WorkOS — Best Practices for Secure User Authentication (April 2026)
- Clerk — User Management for React Apps: comprehensive auth scope analysis (March 2026)

### System Domain
- AWS — PostgreSQL as a JSON Database: Advanced Patterns and Best Practices (November 2025)
- Architecture Weekly — PostgreSQL JSONB: Powerful Storage for Semi-Structured Data (April 2025)
- OneUptime — Build a Task Scheduler in Rust: Cron-Based Job Scheduling Guide (January 2026)
- DesignGurus — Notification System Design: Complete System Design Interview Guide (March 2026)
- Medium — Notification System Design: Architecture, Components, and Best Practices (2025)
- .NET Background Job Processing Comparison — Hangfire vs Quartz vs Temporal (January 2026)
- RedwoodJS — Job Scheduling with PostgreSQL: Persistent Background Jobs (September 2025)

### Cross-Cutting Concerns
- Soft Delete vs Hard Delete: Battle of Deletion Strategies in Database (Medium, March 2026)
- Skemato — Soft Delete vs Hard Delete: Best Practices for Database Design (November 2025)
- Reddit r/PostgreSQL — Soft Delete: deleted_at column vs duplicated table discussion
- Tacnode — Full-Text Search in PostgreSQL: A Complete Guide (February 2026)
- DbVisualizer — PostgreSQL Full Text Search: The Definitive Guide
- OneUptime — How to Implement Full-Text Search in PostgreSQL (January 2026)
- OneUptime — How to Implement Audit Logging in PostgreSQL (January 2026)
- Viprasol — PostgreSQL Audit Logging with Triggers in 2026
- Medium — 15 PostgreSQL Extensions You Should Know in 2026 (March 2026)
- PostgreSQL 18 Official Documentation — Table Partitioning: https://www.postgresql.org/docs/current/ddl-partitioning.html
- OneUptime — How to Implement Table Partitioning in PostgreSQL (January 2026)

### Platform Migration Domain
- Plex Support — Move Viewstate/Ratings from One Install to Another: https://support.plex.tv/articles/201154527-move-viewstate-ratings-from-one-install-to-another/
- Reddit r/PleX — Watch history restoration via SQLite export: https://www.reddit.com/r/PleX/comments/e8aox1/fresh_install_how_do_i_restore_my_watch_history/
- luigi311/JellyPlex-Watched — Multi-user watch sync via provider IDs: https://github.com/luigi311/JellyPlex-Watched
- Forceu/jellyfinmanager — Go CLI for Jellyfin watched status backup/restore: https://github.com/Forceu/jellyfinmanager
- JellyWatch — Migrate from Plex to Jellyfin Guide (March 2026): https://jellywatch.app/blog/migrate-plex-to-jellyfin-guide-2026
- EmbyServerAPI — PyPI Emby REST API client: https://pypi.org/project/EmbyServerAPI/

### Quality Management Domain
- Fora Soft — Adaptive Bitrate Streaming Explained (May 2026): ABR algorithm families, bitrate ladder design, QoE metrics
- Reddit r/jellyfin — Client Capability Wizard Discussion (February 2026): Device misreporting problem, empirical testing proposal
- Jellyfin Documentation — Hardware Selection and Codec Support: Codec matrices, transcoding targets, device capability gaps
- webrtcHacks — Probing WebRTC Bandwidth Probing (May 2024): GCC algorithm, probe clusters, bandwidth estimation techniques
- Fora Soft — Bandwidth Estimation and Congestion Control in WebRTC (May 2026): GCC delay-based and loss-based estimators
- IETF RFC 8216 — HTTP Live Streaming: HLS multi-variant playlists, ABR protocol specification
