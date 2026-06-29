# Platform Migration

## Overview

This document defines the authoritative design for migrating users and their watch data from Plex, Jellyfin, and Emby into our platform. Covers: migration source configuration, user mapping via invite code display names, data extraction per source platform, item matching via provider IDs, watch state import, progress tracking, error handling, and rollback.

## Migration Flow

```
┌─────────────────────────────────────────────────────────────────┐
│  Phase 1: Setup                                                 │
│                                                                  │
│  Admin creates platform accounts + invite codes with names       │
│  Admin configures migration source (URL + credentials)           │
└───────────────────────────────┬─────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────┐
│  Phase 2: Discovery                                              │
│                                                                  │
│  System connects to source platform                              │
│  System discovers source users                                   │
│  Admin maps source users → platform users (by invite code name)  │
│  System discovers source items (movies, episodes)                │
└───────────────────────────────┬─────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────┐
│  Phase 3: Matching                                               │
│                                                                  │
│  System matches source items → our media_items                   │
│  Primary: provider IDs (TMDb, IMDb, TVDb)                       │
│  Fallback: title + year + type                                   │
│  Reports unmatched items to admin                                │
└───────────────────────────────┬─────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────┐
│  Phase 4: Import                                                 │
│                                                                  │
│  For each mapped user:                                           │
│    For each matched item with watch data:                        │
│      INSERT or UPDATE user_item_data                             │
│        (is_watched, play_count, resume_position_ms,              │
│         last_played_at)                                          │
│  Reports results: matched, unmatched, imported, skipped          │
└─────────────────────────────────────────────────────────────────┘
```

## Prerequisites

Before migration begins:

1. **Platform is set up** — admin account created, first-run wizard complete
2. **Libraries are scanned** — all media files are imported and identified in our `media_items` table with provider IDs populated
3. **User accounts exist** — admin has created accounts for all users who will migrate (via invite codes)
4. **Invite codes have display names** — each invite code has a `display_name` that identifies who the user is (e.g., "Dad", "Mom", "Alice", "Bob")
5. **Source platform is accessible** — admin can provide URL + credentials for Jellyfin/Emby, or upload the Plex SQLite database file

## Invite Code Display Names

The existing `invitations` table has a `display_name` column. During migration setup, this name serves as the link between a source platform user and our platform user.

### How It Works

```
Admin creates invite code:
  display_name: "Dad"       →  Creates platform user "Dad"
  display_name: "Mom"       →  Creates platform user "Mom"
  display_name: "Alice"     →  Creates platform user "Alice"

Admin configures migration from Plex:
  System discovers Plex users: ["DadPlex", "MomPlex", "AlicePlex"]

Admin maps:
  Plex "DadPlex"     →  Invite code "Dad"     (display_name match)
  Plex "MomPlex"     →  Invite code "Mom"     (admin selects)
  Plex "AlicePlex"   →  Invite code "Alice"   (admin selects)
```

The admin sees a mapping UI with:
- Left column: source platform users (discovered via API or DB)
- Right column: our platform users (from `invitations.display_name` and `users.username`)
- Admin selects which source user maps to which platform user
- System stores the mapping in `migration_user_mapping` table

## Source Platforms

### Jellyfin

**Connection:** REST API over HTTP(S).

| Requirement | Value |
|---|---|
| URL | `http://jellyfin-host:8096` (or HTTPS with reverse proxy) |
| Authentication | API key from Dashboard → API Keys |
| Header | `X-Emby-Token: {api_key}` |

**Data extraction endpoints:**

| Data | Endpoint | Notes |
|---|---|---|
| Users | `GET /Users` | Returns all users with IDs |
| Watched items | `GET /Users/{UserId}/Items?Filters=IsPlayed&Recursive=true&Fields=ProviderIds,UserData` | All played items with provider IDs |
| In-progress items | `GET /Users/{UserId}/Items/Resume?Fields=ProviderIds,UserData` | Resume position in `UserData.PlaybackPositionTicks` |
| Item details | `GET /Items/{Id}` | `ProviderIds: {"Tmdb": "123", "Imdb": "tt123", "Tvdb": "456"}` |

**Jellyfin response format (watched items):**

```json
{
    "Items": [
        {
            "Id": "abc123",
            "Name": "The Matrix",
            "Type": "Movie",
            "ProductionYear": 1999,
            "ProviderIds": {
                "Tmdb": "603",
                "Imdb": "tt0133093"
            },
            "UserData": {
                "PlaybackPositionTicks": 0,
                "PlayCount": 3,
                "IsFavorite": false,
                "Played": true,
                "LastPlayedDate": "2026-04-15T20:30:00Z"
            }
        }
    ],
    "TotalRecordCount": 1420
}
```

For TV episodes, Jellyfin returns each episode as a separate item with `Type: "Episode"`, including `SeriesName`, `ParentIndexNumber` (season), and `IndexNumber` (episode).

### Emby

**Connection:** REST API over HTTP(S). Nearly identical to Jellyfin (common ancestry).

| Requirement | Value |
|---|---|
| URL | `http://emby-host:8096` (or HTTPS) |
| Authentication | API key |
| Header | `X-Emby-Token: {api_key}` |

**Data extraction endpoints:** Same patterns as Jellyfin. Emby uses the same API structure:

| Data | Endpoint |
|---|---|
| Users | `GET /Users/Public` + `GET /Users/Query` |
| Watched items | `GET /Users/{UserId}/Items?Filters=IsPlayed&Recursive=true&Fields=ProviderIds,UserData` |
| In-progress items | `GET /Users/{UserId}/Items/Resume` |

**Emby response format:** Same structure as Jellyfin. `ProviderIds` object with `Tmdb`, `Imdb`, `Tvdb` keys. `UserData.PlaybackPositionTicks` for resume position.

### Plex

**Connection:** SQLite database file upload. Plex has no bulk watch history API.

| Requirement | Value |
|---|---|
| Source file | `com.plexapp.plugins.library.db` from `/Plug-in Support/Databases/` |
| File size | 1-10 GB typical |
| Upload method | Admin uploads via migration wizard UI |
| Database type | SQLite 3 |

**Admin instructions (displayed during migration):**

1. Stop Plex Media Server
2. Locate the data directory:
   - Linux: `/var/lib/plexmediaserver/Library/Application Support/Plex Media Server/`
   - Docker: `/config/` inside the container
   - Windows: `%LOCALAPPDATA%\Plex Media Server\`
   - macOS: `~/Library/Application Support/Plex Media Server/`
3. Navigate to `Plug-in Support/Databases/`
4. Copy `com.plexapp.plugins.library.db` (not the `-wal` or `-shm` files)
5. Upload via the migration wizard

**Plex SQLite schema (relevant tables):**

```sql
-- User accounts
SELECT id, name FROM accounts;

-- Watch state per user per item
SELECT
    mis.account_id,
    mis.guid,
    mis.view_count,
    mis.view_offset,
    mis.last_viewed_at,
    mi.rating_key,
    mi.metadata_type
FROM metadata_item_settings mis
JOIN metadata_items mi ON mis.guid = mi.guid
WHERE mis.view_count > 0 OR mis.view_offset > 0;

-- Item provider IDs (from metadata_items)
SELECT
    rating_key,
    title,
    `year`,
    metadata_type,
    guid             -- e.g. "com.plexapp.agents.imdb://tt0133093?lang=en"
FROM metadata_items;
```

**Plex GUID parsing:** Plex stores provider IDs as URL-like strings in the `guid` column:

| Plex GUID | Provider | Extracted ID |
|---|---|---|
| `com.plexapp.agents.imdb://tt0133093?lang=en` | IMDb | `tt0133093` |
| `com.plexapp.agents.themoviedb://603?lang=en` | TMDb | `603` |
| `com.plexapp.agents.thetvdb://78874?lang=en` | TVDb | `78874` |
| `plex://movie/5d776885e6d5c9001dcecb72` | Plex internal | No external ID (unmatchable) |

For the newer Plex Movie/Series agents, GUIDs may use `plex://` scheme. These items require fallback matching by title + year.

Plex also stores secondary provider IDs in the `metadata_item_providers` table (or as additional GUIDs in newer versions). The migration engine parses all available GUID fields.

**Plex user identification:** The `accounts` table has an `id` column (integer) and a `name` column. The admin maps Plex account names to our invite code display names.

## Item Matching

The matching engine converts source platform items into our `media_items` rows.

### Primary Match: Provider IDs

```
Source item has ProviderIds: {"Tmdb": "603", "Imdb": "tt0133093"}
                                       │
                    ┌──────────────────┼──────────────────┐
                    ▼                  ▼                  ▼
         SELECT id FROM media_items WHERE tmdb_id = 603
                    │
                    └──→ Found: media_item UUID
                         └──→ Match confidence: HIGH
```

Each provider ID is queried independently. The first match wins. Order of precedence:

1. **TMDb ID** (`media_items.tmdb_id`) — most reliable, always numeric
2. **IMDb ID** (`media_items.imdb_id`) — stable, text format `tt\d+`
3. **TVDb ID** (`media_items.tvdb_id`) — used for TV series/episodes

### Fallback Match: Title + Year + Type

When no provider IDs match (or source item has none):

```sql
SELECT id
FROM media_items
WHERE LOWER(REGEXP_REPLACE(BTRIM(title), '[[:space:]]+', ' ', 'g')) = $1
  AND EXTRACT(YEAR FROM premiere_date)::INT = $2
  AND type = $3    -- 'movie' or 'episode'
LIMIT 1;
```

For TV episodes, the fallback also matches on show title + season + episode number:

```sql
SELECT episode_item.id
FROM media_items episode_item
JOIN episodes e ON e.id = episode_item.id
JOIN seasons sn ON e.season_id = sn.id
JOIN media_items series_item ON series_item.id = e.series_id
WHERE LOWER(REGEXP_REPLACE(BTRIM(series_item.title), '[[:space:]]+', ' ', 'g')) = $1
  AND sn.season_number = $2            -- season number
  AND e.episode_number = $3            -- episode number
LIMIT 1;
```

### Matching Confidence

| Method | Confidence | Auto-applied |
|---|---|---|
| TMDb ID match | HIGH | Yes |
| IMDb ID match | HIGH | Yes |
| TVDb ID match | HIGH | Yes |
| Title + Year + Type exact | MEDIUM | Yes (if no provider ID available) |
| Series title + season + episode exact | LOW | Yes, but surfaced for manual review |
| Admin manual override | HIGH | Yes, explicit admin decision |
| Unmatched | UNMATCHED | No |

### Unmatched Items

Items that cannot be matched are reported to the admin:

```json
{
    "unmatched": [
        {
            "source_title": "Some Movie",
            "source_year": 2024,
            "source_type": "Movie",
            "source_provider_ids": {"Imdb": "tt1234567"},
            "reason": "No media_item found with imdb_id = 'tt1234567'"
        }
    ]
}
```

Common reasons for unmatched items:
- Item exists in source but not in our library (different media files)
- Item was identified differently (wrong TMDb match on one platform)
- Source item has only Plex internal IDs (`plex://` GUID) and title differs

Admin can resolve unmatched items by:
- Adding the media to our library and re-running migration
- Manually matching the source row to a specific `media_item_id`
- Skipping or ignoring rows that should not be imported
- Exporting the review queue to CSV for offline audit or bulk investigation

## Data Import

### What Gets Imported

For each matched item, per mapped user:

| Source Field | Our Field | Transformation |
|---|---|---|
| `Played` / `view_count > 0` | `user_item_data.is_watched` | `true` if played |
| `PlayCount` / `view_count` | `user_item_data.play_count` | Direct copy |
| `PlaybackPositionTicks` / `view_offset` | `user_item_data.resume_position_ms` | Jellyfin/Emby: `ticks / 10_000`; Plex: already in ms |
| `LastPlayedDate` / `last_viewed_at` | `user_item_data.last_played_at` | Direct copy (ISO 8601) |

### What Is NOT Imported

- Favorites (`is_favorite`) — out of scope
- Ratings (`user_rating`) — out of scope
- Audio/subtitle track preferences — out of scope
- Playlists — out of scope
- Custom artwork — out of scope

### Import Logic

```sql
INSERT INTO user_item_data (id, user_id, media_item_id, is_watched, play_count,
                            resume_position_ms, last_played_at, updated_at)
VALUES (
    uuidv7(),
    $user_id,
    $media_item_id,
    $is_watched,
    $play_count,
    $resume_position_ms,
    $last_played_at,
    now()
)
ON CONFLICT (user_id, media_item_id) DO UPDATE SET
    is_watched = EXCLUDED.is_watched OR user_item_data.is_watched,
    play_count = GREATEST(user_item_data.play_count, EXCLUDED.play_count),
    resume_position_ms = CASE
        WHEN EXCLUDED.resume_position_ms > user_item_data.resume_position_ms
        THEN EXCLUDED.resume_position_ms
        ELSE user_item_data.resume_position_ms
    END,
    last_played_at = GREATEST(user_item_data.last_played_at, EXCLUDED.last_played_at),
    updated_at = now()
```

**Merge strategy:** If a row already exists (user already watched something on our platform), the import takes the "best of both worlds":
- `is_watched`: OR — if either platform says watched, it's watched
- `play_count`: MAX — higher play count wins
- `resume_position_ms`: MAX — further progress wins (but resets to 0 if is_watched is true)
- `last_played_at`: MAX — most recent play date wins

## Database Schema

### Migration Sources

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
```

**`connection_config` JSONB by platform:**

Plex:
```json
{
    "method": "sqlite_upload",
    "original_filename": "com.plexapp.plugins.library.db",
    "uploaded_at": "2026-05-31T14:30:00Z",
    "file_size_bytes": 2147483648
}
```

Jellyfin:
```json
{
    "method": "api",
    "base_url": "http://192.168.1.100:8096",
    "api_key_hash": "sha256:abcdef...",
    "api_key_prefix": "jk3m",
    "credential_mode": "hash_only",
    "auth_header": "X-Emby-Token",
    "ssrf_policy": {
        "redirects": "blocked",
        "timeout_seconds": 10,
        "max_response_bytes": 1048576,
        "private_networks": "allowed_in_local_mode"
    }
}
```

Emby:
```json
{
    "method": "api",
    "base_url": "http://192.168.1.101:8096",
    "api_key_hash": "sha256:ghijkl...",
    "api_key_prefix": "pq7x",
    "credential_mode": "hash_only",
    "auth_header": "X-Emby-Token",
    "ssrf_policy": {
        "redirects": "blocked",
        "timeout_seconds": 10,
        "max_response_bytes": 1048576,
        "private_networks": "allowed_in_local_mode"
    }
}
```

API keys are hashed (SHA-256) like our own short-lived code patterns. Only the prefix (first 4 chars) is stored in plaintext. The full key is session-only input and is not written to `migration_sources.connection_config`; later source clients must receive a fresh key or use a future encrypted credential path if resumable remote API pulls require it.

### Migration User Mapping

```sql
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
```

The `source_user_id` is the platform-specific user identifier:
- **Plex:** integer `account_id` from SQLite `accounts` table
- **Jellyfin:** UUID string from `GET /Users` response
- **Emby:** string from `GET /Users/Query` response

The admin maps `source_user_name` (e.g. "DadPlex") to our `platform_user_id` by selecting from a dropdown of existing platform users (identified by `invitations.display_name` and `users.username`).

### Migration Import Log

```sql
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
    status TEXT NOT NULL CHECK (status IN ('discovered', 'matched', 'unmatched', 'imported', 'skipped', 'error')),
    error_detail TEXT,

    UNIQUE(migration_user_mapping_id, source_item_id)
);
```

Every source item encountered during migration is logged. This provides:
- Full audit trail of what was attempted
- Unmatched items report for admin review
- Ability to re-run migration for failed items
- Evidence of what was imported per user

## Migration Wizard (Admin UI)

### Step 1: Choose Source

Admin selects: Plex, Jellyfin, or Emby.

### Step 2: Connect

**Jellyfin/Emby:**
- Enter server URL
- Enter API key
- System tests connection: `GET /System/Info` (Jellyfin) or `GET /System/Info/Public` (Emby)
- System displays: platform name, version, user count

**Plex:**
- Instructions displayed for locating `com.plexapp.plugins.library.db`
- Admin uploads the file
- System validates: SQLite 3 header, expected tables present (`accounts`, `metadata_items`, `metadata_item_settings`)
- File is stored temporarily in `/data/migrations/{id}/plex.db` during import, deleted after completion

### Step 3: Map Users

System displays discovered source users alongside our platform users:

```
┌──────────────────────────────────────────────────┐
│  Source User          →   Platform User            │
│  ─────────────────────────────────────────────────│
│  DadPlex              →   [Dad          ▼]        │
│  MomPlex              →   [Mom          ▼]        │
│  AlicePlex            →   [Alice        ▼]        │
│  GuestUser            →   [Skip         ▼]        │
│                                                    │
│  [Skip Unmapped Users]                             │
└──────────────────────────────────────────────────┘
```

- Dropdown shows all platform users from `users` table
- Display name from `invitations.display_name` shown in parentheses
- Admin can skip users they don't want to migrate
- At least one mapping is required to proceed

### Step 4: Review

System shows a summary before importing:

```
Migration Source: Jellyfin (10.10.6) at http://192.168.1.100:8096
Users mapped: 3
Total source items with watch data: 1,847
  Movies: 423
  Episodes: 1,424

Our library:
  Movies: 400
  Episodes: 1,380

Estimated matches: ~1,700 (92%)
Items requiring review: ~147 (8%)

[Start Migration]
```

### Step 5: Import

Progress bar with real-time updates:

```
Importing watch history...
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ 73%

User: Dad (DadPlex)
  Items processed: 612 / 847
  Matched: 589
  Unmatched: 23
  Imported: 589
  Skipped (already exists): 0
```

### Step 6: Results

```
Migration Complete

Summary:
  Total items processed: 1,847
  Matched: 1,703 (92.2%)
  Imported: 1,703
  Unmatched: 144 (7.8%)
  Errors: 0

Per User:
  Dad:     612 items → 589 matched → 589 imported
  Mom:     823 items → 761 matched → 761 imported
  Alice:   412 items → 353 matched → 353 imported

Unmatched Items (144):
  [View Report]  [Export CSV]
```

## API Endpoints

| Method | Endpoint | Auth | Description |
|---|---|---|---|
| `POST` | `/api/v1/migrations` | Admin (`can_manage_users`) | Create a new migration source |
| `GET` | `/api/v1/migrations` | Admin | List all migrations |
| `GET` | `/api/v1/migrations/{id}` | Admin | Get migration status + results |
| `DELETE` | `/api/v1/migrations/{id}` | Admin | Delete migration and its log |
| `POST` | `/api/v1/migrations/{id}/connect` | Admin | Test connection to source |
| `POST` | `/api/v1/migrations/{id}/discover` | Admin | Discover users and items from source |
| `POST` | `/api/v1/migrations/{id}/match` | Admin | Match discovered source items to local media |
| `POST` | `/api/v1/migrations/{id}/upload` | Admin | Upload and validate Plex SQLite database |
| `GET` | `/api/v1/migrations/{id}/map-users` | Admin | Get saved mappings and platform-user options |
| `POST` | `/api/v1/migrations/{id}/map-users` | Admin | Save user mappings |
| `POST` | `/api/v1/migrations/{id}/preflight` | Admin | Run no-write readiness checks and return blockers/warnings |
| `POST` | `/api/v1/migrations/{id}/start` | Admin | Begin the import |
| `GET` | `/api/v1/migrations/{id}/progress` | Admin | Get real-time progress |
| `GET` | `/api/v1/migrations/{id}/review` | Admin | List unmatched and low-confidence review rows |
| `POST` | `/api/v1/migrations/{id}/review/{item_id}` | Admin | Manually match, skip, or ignore a review row |
| `GET` | `/api/v1/migrations/{id}/review.csv` | Admin | Export the current review queue as CSV |
| `GET` | `/api/v1/migrations/{id}/unmatched` | Admin | Get unmatched items report |
| `POST` | `/api/v1/migrations/{id}/cancel` | Admin | Cancel in-progress migration |

All endpoints require admin capability (`can_manage_users`).

## Error Handling

### Migration Error Codes

| Code | HTTP | Description |
|---|---|---|
| `MIGR_001` | 404 | Migration not found |
| `MIGR_002` | 409 | Migration already in progress |
| `MIGR_003` | 502 | Source platform unreachable (Jellyfin/Emby connection failed) |
| `MIGR_004` | 422 | Invalid source configuration (bad URL, missing API key) |
| `MIGR_005` | 422 | Invalid Plex database file (not SQLite, missing tables, corrupted) |
| `MIGR_006` | 409 | User mapping conflict (same source user mapped twice) |
| `MIGR_007` | 422 | No user mappings provided (at least one required) |
| `MIGR_008` | 422 | No watch data found on source platform |
| `MIGR_009` | 413 | Plex database file too large (max 10 GB) |
| `MIGR_010` | 507 | Insufficient disk space for Plex database upload |
| `MIGR_011` | 501 | Migration API scaffold is wired but the requested operation is not implemented yet |

`MIGR_011` is a temporary scaffold code used during Phase 14 task sequencing. Remove it from reachable paths as CRUD, discovery, preflight, import, and review behavior is implemented.

As of Phase 14 Task 2, the migration service no longer returns `MIGR_011` from the wired endpoints. The enum mapping remains until all later task seams are filled, but current CRUD/status paths return concrete `MIGR_001`-`MIGR_010` errors or successful action responses.

### Error Scenarios

| Scenario | Behavior |
|---|---|
| Source API times out during discovery | Retry 3 times with backoff (1s, 5s, 15s); fail with `MIGR_003` |
| Plex DB upload interrupted | Temporary `.uploading` file is discarded; resumable/range upload remains a future upload-pipeline extension |
| Item match fails | Log as `unmatched`, continue processing remaining items |
| Import of single item fails (DB error) | Log error, skip item, continue processing |
| All items fail | Mark migration as `failed`; report all errors |
| Migration cancelled mid-import | Stop processing; keep already-imported data; mark as `pending` for resume |

## File Handling (Plex)

### Upload

1. Admin uploads `com.plexapp.plugins.library.db` via multipart form upload
2. Server validates:
   - File size ≤ 10 GB (`MIGR_009`)
   - SQLite 3 header (first 16 bytes: `SQLite format 3\000`)
   - Required tables exist: `accounts`, `metadata_items`, `metadata_item_settings`
   - If any validation fails: `MIGR_005`
3. File streamed to `/data/migrations/{migration_id}/plex.db.uploading`, then atomically moved to `/data/migrations/{migration_id}/plex.db` after validation
4. Available disk space checked before upload (`MIGR_010`)

### Reading

Using the `rusqlite` crate (pure Rust SQLite bindings):

```rust
use rusqlite::Connection;

fn extract_plex_users(db_path: &Path) -> Result<Vec<PlexUser>> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
    )?;
    let mut stmt = conn.prepare(
        "SELECT id, name FROM accounts WHERE id > 0"
    )?;
    // ...
}

fn extract_plex_watch_data(
    db_path: &Path,
    account_id: i64,
) -> Result<Vec<PlexWatchEntry>> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
    )?;
    let mut stmt = conn.prepare(
        "SELECT mis.guid, mis.view_count, mis.view_offset, mis.last_viewed_at,
                mi.title, mi.year, mi.metadata_type, mi.rating_key
         FROM metadata_item_settings mis
         JOIN metadata_items mi ON mis.guid = mi.guid
         WHERE mis.account_id = ?1
           AND (mis.view_count > 0 OR mis.view_offset > 0)"
    )?;
    // ...
}
```

### Cleanup

After migration completes (or fails), the uploaded Plex database file is deleted from `/data/migrations/{id}/`. Admin can choose to keep the file for re-runs until they explicitly delete the migration.

## Scheduled Task

A scheduled task `migration_cleanup` runs daily at 05:00 to:
- Delete Plex database uploads for completed migrations older than 24 hours
- Delete migration sources with `completed` status older than 90 days
- Delete `migration_import_log` rows older than 90 days

Phase 14 Task 1 registers the `migration_cleanup` row for existing deployments with the daily 05:00 schedule, 30-minute timeout, 3 retries, and retention config:

```json
{
    "delete_plex_uploads_after_hours": 24,
    "delete_completed_sources_after_days": 90,
    "delete_import_logs_after_days": 90
}
```

The row is seeded disabled until Phase 14 Task 14 adds the executor. This avoids a scheduled failure before cleanup behavior exists while still making the task visible in scheduled-task management.

## Phase 14 Task 1 Implementation Notes

- Added migration-domain query indexes for `migration_sources.status`, `migration_user_mapping.migration_source_id`, `migration_import_log.migration_source_id`, `migration_import_log.status`, and `migration_import_log.matched_media_item_id`.
- Added `cancelled` to `migration_sources.status`; cancelled migrations remain auditable and can be resumed later by returning to the pending/import path from persisted import logs.
- Registered `Migration Cleanup` as a `migration_cleanup` scheduled task for existing deployments, disabled until the cleanup executor lands in Task 14.

## Phase 14 Task 2 Implementation Notes

- `POST /api/v1/migrations` persists migration sources with validated platform values and returns the created source row.
- `GET /api/v1/migrations` supports platform/status filtering, newest-first ordering, and offset pagination.
- `GET` and `DELETE /api/v1/migrations/{id}` use real source lookup and return `MIGR_001` for missing sources; delete is blocked while a source is in `discovering`, `matching`, or `importing`.
- `POST /api/v1/migrations/{id}/map-users` replaces all mappings for the source in one transaction after validating at least one non-skipped mapping, duplicate source users, duplicate platform users, skip conflicts, and platform-user existence.
- `GET /api/v1/migrations/{id}/progress` aggregates discovered, matched, unmatched, imported, skipped, and processed counts from `migration_import_log`.
- `GET /api/v1/migrations/{id}/unmatched` paginates unmatched/import-log rows for the admin review workflow.
- `POST /api/v1/migrations/{id}/cancel` records `cancelled` for active source states and is a no-op action response for inactive states.
- Connection, discovery, and start endpoints are now DB-backed action boundaries: they validate source existence and active-state safety but deliberately do not perform source network checks, SQLite inspection, preflight, or runner dispatch until Tasks 3-7.

## Phase 14 Task 3 Implementation Notes

- `POST /api/v1/migrations` now sanitizes and normalizes `connection_config` before insertion.
- Jellyfin/Emby configs require `method = "api"`, a valid `http` or `https` `base_url`, and an API key supplied as session-only input. The stored config contains only `api_key_hash`, `api_key_prefix`, `credential_mode = "hash_only"`, `auth_header = "X-Emby-Token"`, and outbound-policy metadata.
- URL validation rejects embedded credentials, fragments, unsupported schemes, unresolvable hosts, cloud metadata service addresses, and always-invalid reserved addresses. In `network_mode = "exposed"`, resolved private, loopback, link-local, and unique-local targets are rejected; local mode allows LAN/loopback media servers.
- The stored API source policy records disabled redirects, 10-second timeout, and 1 MiB max response size for later Jellyfin/Emby clients.
- Plex configs require `method = "sqlite_upload"` and the canonical `com.plexapp.plugins.library.db` filename; declared file sizes over 10 GiB are rejected and declared sizes must fit with 2x headroom on the `data_dir/migrations` volume.
- `validate_plex_database_file()` verifies the SQLite 3 header and required Plex tables (`accounts`, `metadata_items`, `metadata_item_settings`) for the later multipart upload path.

## Phase 14 Task 4 Implementation Notes

- Added `POST /api/v1/migrations/{id}/preflight` as the no-write admin review endpoint.
- The report returns `is_ready`, structured blockers, warnings, per-check status, library readiness, user mapping readiness, source readiness, disk readiness, and estimated counts.
- Library readiness counts active libraries, scanned libraries, movie/episode import targets, provider-ID coverage, and warns below 80% provider-ID coverage.
- User mapping readiness counts total, valid, and invalid mappings and blocks when no mappings exist or mapped platform users have been deleted.
- Jellyfin/Emby source readiness performs a lightweight `GET /System/Info/Public` with redirects disabled, no proxy, the 10-second timeout from Task 3, and the 1 MiB response-size policy. HTTP 2xx, 401, and 403 prove reachability; other statuses or network errors become blockers.
- Plex source readiness is no-write and reports whether upload metadata exists. Full SQLite readability validation remains tied to the upload path through `validate_plex_database_file()`.
- Disk readiness checks 2x declared Plex upload size against the `data_dir/migrations` volume when a Plex file size is known.
- Estimated source item counts and match rates are derived from `migration_import_log` when discovery data exists; before discovery, the report returns a warning instead of fabricating match-rate estimates.

## Phase 14 Task 5 Implementation Notes

- Added `server/src/workers/migration_runner.rs` as the service-owned background runner used by `POST /api/v1/migrations/{id}/start`.
- Active migration runs are tracked in `AppState.migration_runs` with one `CancellationToken` per migration source, preventing duplicate in-process starts and giving cancel requests an immediate signal path.
- Dry-run starts remain no-write and return a preflight summary. Real starts require user mappings, a blocker-free preflight, and existing `migration_import_log` rows; if discovery has not produced watch-data rows yet, the API returns `MIGR_008`.
- The runner persists lifecycle state through `migration_sources.status`, recalculates `migration_user_mapping` counters from `migration_import_log`, and derives completion/failure from terminal durable row statuses.
- Resume and crash-safety use `migration_import_log` as the source of truth. A restarted run skips rows already marked `imported`, `skipped`, `unmatched`, or `error`; rows still marked `matched` remain pending for the Task 11 import/merge implementation.
- Cancel requests update `migration_sources.status = 'cancelled'` and signal any live runner token. The runner checks both the token and persisted source status before committing final state.

## Phase 14 Task 6 Implementation Notes

- Added `20260629030000_migration_api_extraction_task6.sql` to persist extracted source watch state on `migration_import_log` and to add the pre-match `discovered` row status.
- Jellyfin/Emby `/connect` verifies `GET /System/Info` with a session-supplied API key. The raw key is never stored; it must hash to the stored `api_key_hash` before the server sends any source request.
- Jellyfin/Emby `/discover` returns source users from `GET /Users` or `GET /Users/Query`. If no mappings exist yet, the response stops after user discovery so the wizard can map users.
- Once mappings exist, `/discover` extracts watched items from `GET /Users/{UserId}/Items` with `IsPlayed=true`/`Filters=IsPlayed` and resume items from `GET /Users/{UserId}/Items/Resume`, limited to Movie/Episode rows with `ProviderIds,UserData`.
- Source requests preserve the Task 3 network policy: redirect blocking, no proxy, 10-second timeout, 1 MiB response limit, and `X-Emby-Token` authentication. Retry backoff is 1s, 5s, and 15s; mapped users are extracted with a four-user concurrency cap.
- Extracted rows are upserted as `discovered`, keyed by `(migration_user_mapping_id, source_item_id)`, with normalized `tmdb`/`imdb`/`tvdb` provider IDs, raw provider payload, episode metadata, source watch state, resume milliseconds from Jellyfin/Emby ticks, and latest played timestamp.
- Progress and preflight counts treat `discovered` rows as source watch data but not as matched/imported rows. Task 9 owns transition from `discovered` to `matched` or `unmatched`.

## Phase 14 Task 7 Implementation Notes

- Enabled axum multipart support and added `POST /api/v1/migrations/{id}/upload` for Plex SQLite uploads with a route-scoped 10 GiB + overhead body limit.
- Uploads must use multipart field `file` and the canonical `com.plexapp.plugins.library.db` filename. The server streams to a temporary `.uploading` file, enforces the 10 GiB limit during streaming, validates SQLite header/table requirements, removes invalid temporary files, then stores `/data/migrations/{id}/plex.db`.
- Successful upload updates `migration_sources.connection_config` with `stored_path`, `file_size_bytes`, `uploaded_at`, and validation metadata. `stored_path` is canonicalized and must remain under the migration upload directory before it is read.
- Plex `/discover` now reads the stored database through a read-only, query-only `rusqlite` connection. It discovers source users from `accounts` and, once mappings exist, extracts mapped Movie/Episode rows from `metadata_item_settings` joined to `metadata_items`.
- Extracted Plex rows use `view_count > 0` as watched state, `view_offset` as resume milliseconds, `last_viewed_at` as Unix seconds, and `metadata_type` 1/4 as movie/episode. Watched rows reset resume to 0 to match the import merge strategy.
- Provider IDs are parsed from primary Plex GUIDs and optional secondary `metadata_item_guids` / `metadata_item_providers` rows when those tables exist. IMDb, TMDb, and TVDb IDs are normalized into the same `source_provider_ids` shape used by Jellyfin/Emby extraction.
- Resumable/range upload is not wired yet because the current client/server path only supports a single multipart request; interrupted uploads leave no durable partial import state.

## Phase 14 Task 8 Implementation Notes

- Added `20260629040000_migration_user_mapping_task8.sql` so skipped source users are persisted as `migration_user_mapping.status = 'skipped'` with `platform_user_id = NULL`, while mapped rows must still have a platform user.
- Added a partial unique index on `(migration_source_id, platform_user_id)` for non-null platform users to prevent mapping the same platform user to multiple source users in one migration.
- `GET /api/v1/migrations/{id}/map-users` returns saved mapping decisions and platform-user options. Options include `users.username`, `users.display_name`, email/status, the latest linked `invitations.display_name` and invitation email when present, and a ready-to-display label.
- `POST /api/v1/migrations/{id}/map-users` accepts mapped rows with `platform_user_id` or skipped rows with `skip = true`; duplicate source users, duplicate mapped platform users, missing platform users, and skip+platform conflicts return `MIGR_006`.
- All-skipped submissions return `MIGR_007`. Preflight, start, and extraction require at least one non-skipped mapping and ignore skipped rows when extracting source watch data.

## Phase 14 Task 9 Implementation Notes

- Added `20260629050000_migration_matching_task9.sql` with `migration_import_log.match_confidence`, a confidence index, and the `series_episode` match method.
- Added `POST /api/v1/migrations/{id}/match` plus `matchMigrationItems()` in the web API client. The endpoint is admin-only, blocks concurrent active migration states, sets the source to `matching` during the pass, and returns processed/matched/unmatched counts plus high/medium/low confidence totals.
- Matching runs against durable `migration_import_log` rows in `discovered` or `unmatched` status. It tries TMDb, IMDb, and TVDb provider IDs first, then exact normalized title + `premiere_date` year + type, then episode fallback by source series title + season + episode through `episodes`, `seasons`, and the series `media_items` row.
- Matched rows are persisted with `matched_media_item_id`, `match_method`, `match_confidence`, `status = 'matched'`, and cleared error detail. Unmatched rows are persisted with `match_method = 'unmatched'`, `match_confidence = 'unmatched'`, `status = 'unmatched'`, and an audit reason describing attempted identifiers/fallbacks.
- Preflight estimates count low-confidence rows from `match_confidence = 'low'`, and unmatched report rows include `match_confidence` for the Task 10 manual review workflow.

## Phase 14 Task 10 Implementation Notes

- Added `20260629060000_migration_manual_review_task10.sql` so `migration_import_log.match_method` accepts `manual` for admin decisions.
- Added `GET /api/v1/migrations/{id}/review` for the review queue. The default `status = needs_review` filter returns rows with `status = 'unmatched'` plus matched rows with `match_confidence = 'low'`; `unmatched`, `low_confidence`, and `all` filters are also supported.
- Added `POST /api/v1/migrations/{id}/review/{item_id}`. `action = match` requires a `media_item_id`, verifies that the target local media row exists and has the same importable type (`movie` or `episode`), then sets `matched_media_item_id`, `match_method = 'manual'`, `match_confidence = 'high'`, and `status = 'matched'`. `action = skip` and `action = ignore` clear the match and set `status = 'skipped'` with an audit reason.
- Added `GET /api/v1/migrations/{id}/review.csv`, served as `text/csv; charset=utf-8` with `Content-Disposition: attachment`, using RFC 4180-compatible quoting for commas, quotes, and embedded line breaks.
- The settings migration page now exposes a Match Review panel with migration-source selection, review filters, recent local movie/episode candidate dropdowns from `GET /api/v1/media-items`, direct `media_item_id` entry, Match/Skip/Ignore actions, and CSV export. Manual matches return rows to the pending `matched` state so the import runner can process them when Task 11 lands.

## Security Considerations

| Concern | Mitigation |
|---|---|
| Plex DB contains all library metadata | Read-only access; file deleted after import |
| Jellyfin/Emby API keys stored | Hashed (SHA-256) like our own `api_keys`; only prefix stored; key held in memory during migration only |
| Migration endpoints are admin-only | `can_manage_users` capability required |
| Imported watch data overwrites existing | Merge strategy (MAX/GREATEST) never loses data — only improves it |
| Large file upload (Plex) | Size limit 10 GB; disk space pre-check; streamed upload |

## Limitations

| Limitation | Reason |
|---|---|
| Plex requires DB file upload | Plex has no bulk watch history REST API |
| Plex `plex://` GUID items may not match | Newer Plex agents use internal IDs; fallback to title+year may fail |
| Jellyfin/Emby must be reachable during migration | Real-time API pull, no offline mode |
| Only movies and TV episodes migrated | Music, photos, books not in current scope |
| No playlist import | Platform-specific format differences |
| No artwork/metadata import | Our server re-fetches from TMDb during library scan |
| Partially watched items use resume position only | No full playback session history imported (only `user_item_data`, not `play_sessions`) |

## Crate Dependencies

```toml
rusqlite = { version = "0.32", features = ["bundled"] }
```

`rusqlite` with `bundled` feature compiles SQLite from C source — no system SQLite dependency. Used only for reading Plex database files during migration. The `bundled` feature is required because Alpine Linux and Synology NAS may not have SQLite development headers.

## Research Sources

- OWASP Cheat Sheet Series — Server-Side Request Forgery Prevention Cheat Sheet: https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html
- OWASP Cheat Sheet Series — File Upload Cheat Sheet: https://cheatsheetseries.owasp.org/cheatsheets/File_Upload_Cheat_Sheet.html
- Jellyfin API — official API documentation: https://api.jellyfin.org/
- Emby API — official API documentation: https://dev.emby.media/reference/RestAPI.html
- Plex Support — Plex Media Server data directory / database location: https://support.plex.tv/articles/202915258-where-is-the-plex-media-server-data-directory-located/
- Plex Support — Move Viewstate/Ratings from One Install to Another: https://support.plex.tv/articles/201154527-move-viewstate-ratings-from-one-install-to-another/
- Reddit r/PleX — Fresh install watch history restoration (SQLite export): https://www.reddit.com/r/PleX/comments/e8aox1/fresh_install_how_do_i_restore_my_watch_history/
- Reddit r/PleX — Export user watch history tools: https://www.reddit.com/r/PleX/comments/1pkavos/any_tools_to_let_me_export_a_users_watch_history/
- Plex Forums — Export/Import watch history discussion: https://forums.plex.tv/t/export-import-watch-history/808477
- Jellyfin API — Discussion #7259 on setting playback progress: https://github.com/orgs/jellyfin/discussions/7259
- IETF RFC 4180 — Common Format and MIME Type for CSV Files: https://www.rfc-editor.org/rfc/rfc4180
- Forceu/jellyfinmanager — Go CLI for Jellyfin watched status backup/restore with provider ID matching: https://github.com/Forceu/jellyfinmanager
- luigi311/JellyPlex-Watched — Multi-user watch sync between Plex and Jellyfin via provider IDs: https://github.com/luigi311/JellyPlex-Watched
- Florian Jensen — How to migrate from Plex to Jellyfin (August 2024): https://florianjensen.com/2024/08/21/how-to-migrate-from-plex-to-jellyfin/
- JellyWatch — Migrate from Plex to Jellyfin Guide (March 2026): https://jellywatch.app/blog/migrate-plex-to-jellyfin-guide-2026
- EmbyServerAPI — PyPI auto-generated Emby REST API client with full endpoint list: https://pypi.org/project/EmbyServerAPI/
- Emby Community — Get favourites and watch history by API (March 2026): https://emby.media/community/topic/146792-get-favourites-and-watch-history-by-api/
- Milan Jovanovic — Understanding Cursor Pagination (for large dataset handling): https://www.milanjovanovic.tech/blog/understanding-cursor-pagination-and-why-its-so-fast-deep-dive
