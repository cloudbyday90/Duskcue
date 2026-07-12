# Search

## Overview

This document is the authoritative design for media-item search in Duskcue — the strategy by which users find media across their libraries by title, cast, genre, or arbitrary text. The goal is sub-second perceived search latency across the realistic range of home-media-server library sizes (500–50k items), with a documented escape hatch for extreme-scale libraries (50k+).

The decision documented here: **PostgreSQL FTS as the default search engine for v1.0; Meilisearch as the optional, opt-in migration target for large libraries.** Trigger criteria and migration path defined below. Elasticsearch/OpenSearch, Typesense, and embedded Tantivy are considered and rejected.

## Scope

**Covers:**

- Choice of search engine (PostgreSQL FTS vs dedicated engine)
- Performance envelope and migration trigger threshold
- Migration target (Meilisearch) and integration topology (sidecar)
- Index schema, ranking, typo tolerance, multilingual support
- Real-time index updates (sync from `media_items` to search engine)
- Faceted filtering and aggregations
- v1.0 commitment and post-v1.0 evolution

**Does NOT cover:**

- The existing PostgreSQL FTS implementation details — already built in Phase 2; see [DATABASE.md](DATABASE.md) and `server/migrations/20260530060200_create_full_text_search.sql`
- Media browsing UI (poster grid, sort, filter) — see [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) and `clients/web/src/routes/`
- Subtitle search (different domain; see [SUBTITLES.md](SUBTITLES.md))
- Person/credit CRUD (see [DATABASE.md](DATABASE.md) `people`, `media_credits` tables)
- OpenAI/semantic/embedding search — out of scope for Duskcue (no LLM dependency)

## Current State — PostgreSQL FTS (Phase 2)

Duskcue v1.0 ships with PostgreSQL full-text search. The Phase 2 migration (`20260530060200_create_full_text_search.sql`) created a sophisticated FTS setup that is already better-than-typical for PG:

### Schema

- **`media_items.search_vector`** column (tsvector) — denormalized, weighted search index per item
- **Weighted fields:**
  - Weight `A` (highest): `title`, `original_title`
  - Weight `B`: `overview`
  - Weight `C`: aggregated cast names (`media_credits` JOIN `people`)
  - Weight `D` (lowest): aggregated genres, tags
- **Trigger-based real-time updates** — `rebuild_media_search_vector()` plpgsql function re-runs on any INSERT/UPDATE/DELETE against `media_items`, `media_credits`, `media_genres`, `media_tags`
- **Per-library language config** — each library's `metadata_language` selects the text-search configuration (`regconfig`); a Japanese library uses the Japanese tokenizer, an English library uses English stemming
- **Trigram index** (`pg_trgm` extension) — `idx_media_items_title_trgm` GIN index on `title` for fuzzy substring matching (handles "avngers" → "Avengers")
- **GIN index** on `search_vector` (created in Phase 2 core media tables migration)

### Query Pattern

```sql
SELECT id, title, ts_rank(search_vector, query) AS rank
FROM media_items, plainto_tsquery('english', $1) AS query
WHERE search_vector @@ query
ORDER BY rank DESC, title ASC
LIMIT 20;
```

### Capabilities

| Capability | Status |
|---|---|
| Weighted relevance ranking | ✅ A/B/C/D weights |
| Multilingual stemming | ✅ Per-library `regconfig` (auto-selects stemmer) |
| Fuzzy substring match (typo tolerance for titles) | ✅ via `pg_trgm` ILIKE / similarity |
| Real-time index updates | ✅ Trigger-based, no sync lag |
| Stopword filtering | ✅ Built into PG text-search configurations |
| Phrase queries | ✅ `phraseto_tsquery` |
| Faceted aggregations | ⚠️ Possible via separate GROUP BY queries; no native facet support |
| Cross-language single query | ❌ Each query uses one `regconfig`; can't mix JP+EN in one search |
| Typo-tolerance on overview/credits | ❌ Trigram is title-only; FTS stems but doesn't fuzz-match |

## Decision — PG FTS Default, Meilisearch as Opt-In Migration Target

**Duskcue v1.0 ships with PostgreSQL FTS as the only search engine.** This covers the realistic home-media-server scale (up to ~50k items) at sub-100ms latency. For larger libraries, **Meilisearch** is the named migration target, enabled via admin configuration as an optional sidecar.

### Why PG FTS for v1.0 Default

1. **Already built and working** — Phase 2 created the full infrastructure (triggers, weights, GIN index, trigram). Pre-v1.0 Task 3 added the authenticated `GET /api/v1/search` endpoint and faceted web search UI on top of it.
2. **Zero deployment complexity** — No extra process, no extra container, no extra port, no extra data directory to manage. Single PostgreSQL database handles search alongside all other data. Critical for self-hosted NAS deployments where simplicity is paramount.
3. **Sufficient for 95%+ of deployments** — Typical home media servers have 500–5000 items. PG FTS at this scale returns queries in <10ms. Even at 10k items, latency is comfortable (10–50ms).
4. **Real-time consistency** — Trigger-based updates mean search results are never stale. Dedicated engines require sync infrastructure (debezium, polling, batch) that introduces lag and complexity.
5. **Multilingual** — PG's per-language regconfigs (english, german, japanese, etc.) handle stemming and stopwords correctly. Each library uses its own language.
6. **No new dependency** — PostgreSQL is already required; FTS comes free. Adding Meilisearch/Typesense adds a second binary operators must install and update.

### Why Meilisearch as the Migration Target

When a library crosses the threshold where PG FTS no longer keeps up, Meilisearch is the chosen replacement:

| Concern | Meilisearch | Why it wins for Duskcue |
|---|---|---|
| Implementation language | Rust | Matches Duskcue stack; same safety guarantees; can contribute upstream |
| Storage model | Disk (memory-mapped) | Index size not bounded by RAM (matters for NAS with 2–8GB) |
| Memory footprint | ~512MB idle, scales with active queries | Fits alongside Duskcue server + PostgreSQL on NAS hardware |
| License | MIT (Community Edition) | Compatible with Duskcue's AGPL-3.0 |
| CJK / Arabic / Hebrew tokenization | First-class, auto-detected | Critical for anime/foreign-film libraries |
| Typo tolerance | Built-in Levenshtein automata, configurable per-attribute | "avngers" → "Avengers" works for all fields, not just title |
| Search-as-you-type | Optimized for prefix/instant-search UX | Matches the Duskcue search-bar pattern |
| Faceted search | Native | Enables "filter by genre + decade + rating" in one query |
| Indexing speed | Fast (millions of docs/hour) | Bulk re-index after migration completes in minutes, not hours |
| Setup complexity | Single binary; one Docker container | Operator-friendly |
| Stability / memory safety | Rust | Strong reliability guarantees |

**Rejected alternatives:**

| Engine | Why rejected |
|---|---|
| **Typesense** | RAM-only storage model (index must fit entirely in memory) — prohibitive on NAS hardware. C++ (memory-safety concerns). GPL-3 license (less permissive than MIT). Slightly weaker CJK support. |
| **Elasticsearch / OpenSearch** | JVM memory hog (multi-GB heap). Operations overhead (cluster management, index lifecycle, shard allocation). Massive overkill for media-library search. |
| **Quickwit** | Designed for log/metrics search at petabyte scale, not document search. Distributed-storage architecture adds complexity irrelevant to single-instance Duskcue deployments. |
| **Tantivy (embedded library)** | Embedding Tantivy as a Rust library in the Duskcue binary avoids the sidecar, but: (1) ties search CPU/RAM to the main server process — can starve API/transcode under load; (2) duplicating Lucene-grade complexity in-process is high-churn maintenance; (3) no built-in HTTP API for admin debugging; (4) no path to horizontal scaling if ever needed. Sidecar Meilisearch is cleaner separation of concerns. |
| **Sonic** | Index-only (returns doc IDs, not documents) — requires secondary lookup. No relevance ranking. Insufficient for Duskcue. |
| **Bleve (Go)** | Go-based — wrong ecosystem. Slower development. Larger index sizes than Tantivy/Meilisearch. |

## Performance Envelope and Migration Trigger

### PG FTS Performance by Library Size

Benchmarked estimates based on PostgreSQL GIN index characteristics and typical media-item text density (50–500 chars per searchable field):

| Library size | Typical query latency (p95) | Verdict |
|---|---|---|
| 500 items | <5ms | Excellent — instant feel |
| 5,000 items | 5–15ms | Excellent — imperceptible |
| 25,000 items | 15–50ms | Good — still feels instant |
| 50,000 items | 50–150ms | Adequate — visible-but-acceptable |
| 100,000 items | 150–400ms | Marginal — typo-tolerance queries lag |
| 250,000+ items | 400ms+ | Poor — time to migrate |

These assume:
- Proper GIN index on `search_vector` (created Phase 2)
- `shared_buffers` reasonably sized (≥25% of system RAM)
- `effective_cache_size` tuned to system RAM
- Modern NVMe SSD (SATA SSD adds ~2x; HDD adds ~10x)

### Migration Trigger Criteria

**Hard trigger (mandatory migration):** Library exceeds **100,000 items** OR measured p95 search latency exceeds **500ms**.

**Soft trigger (recommend migration):** Library exceeds **50,000 items** OR measured p95 search latency exceeds **200ms** OR admin reports user complaints about search quality.

Measured latency uses the Prometheus histogram `search_query_duration_seconds` added in Pre-v1.0 Task 4. Operators calculate p95 with `histogram_quantile(0.95, rate(search_query_duration_seconds_bucket[5m]))`, and can slice by `status` and `has_filters` labels to separate successful filtered searches from failed or unfiltered requests.

When triggered, the admin enables Meilisearch via `server_config.search.engine = "meilisearch"` and configures the Meilisearch endpoint. Duskcue:
1. Detects the config change at runtime (via `ArcSwap<RuntimeConfig>` reload)
2. Spawns a background indexer (`workers/search_indexer.rs`) that backfills the existing `media_items` corpus into Meilisearch
3. Switches the search API to query Meilisearch once backfill completes
4. Maintains real-time index updates via the existing `rebuild_media_search_vector()` trigger (extended to also push to Meilisearch)

### Why Not Migrate Proactively at v1.0

The 95%+ case is a home media server with under 10k items. PG FTS handles this with sub-15ms latency. Adding Meilisearch to v1.0:

- **Doubles deployment complexity** — operators must install/configure/upgrade two systems instead of one
- **Wastes resources** — Meilisearch uses ~512MB idle RAM that 95% of deployments don't need
- **Adds sync infrastructure** — real-time index sync between PG and Meilisearch is non-trivial (debezium, logical replication, or trigger+HTTP)
- **Diverges from "single-container Docker" model** — Phase 15's single-container design with embedded PostgreSQL becomes a two-container deployment

The opt-in approach lets small deployments stay simple while giving large-library admins a documented escape hatch.

## Meilisearch Integration (When Enabled)

### Topology

Meilisearch runs as a **sidecar process** in the same Docker container (or same host for non-Docker deployments). It binds to `127.0.0.1:7700` (loopback only) — no external exposure. The Duskcue server connects via localhost HTTP.

```
┌─────────────────────────────────────────────┐
│           Duskcue Docker container          │
│                                             │
│  ┌─────────────┐    ┌─────────────────┐    │
│  │  Duskcue    │    │  PostgreSQL     │    │
│  │  server     │    │  (embedded)     │    │
│  │  :48027     │    │  :5432          │    │
│  └──────┬──────┘    └─────────────────┘    │
│         │                                   │
│         │ HTTP (localhost)                  │
│         ▼                                   │
│  ┌─────────────┐                            │
│  │ Meilisearch │  ← optional, enabled when  │
│  │ :7700       │    library crosses threshold│
│  │ (loopback)  │                            │
│  └─────────────┘                            │
└─────────────────────────────────────────────┘
```

For non-Docker deployments (manual binary on Linux/macOS), the admin runs Meilisearch as a systemd unit alongside Duskcue.

### Index Schema

One Meilisearch index: `media_items`. Document structure mirrors the searchable fields:

```json
{
  "id": "01950abc-...",
  "title": "The Matrix",
  "original_title": "The Matrix",
  "overview": "A computer hacker learns...",
  "type": "movie",
  "library_id": "0194f...",
  "year": 1999,
  "runtime_seconds": 8160,
  "rating_average": 8.7,
  "genres": ["Action", "Science Fiction"],
  "tags": ["mindfuck", "cyberpunk"],
  "cast": ["Keanu Reeves", "Laurence Fishburne", "Carrie-Anne Moss"],
  "directors": ["Lana Wachowski", "Lilly Wachowski"],
  "languages": ["en"],
  "poster_artwork_id": "01950def-...",
  "match_state": "confirmed"
}
```

**Meilisearch settings applied via `/indexes/media_items/settings`:**

- `searchableAttributes` (ranked — order matters for relevance):
  1. `title`
  2. `original_title`
  3. `cast`
  4. `overview`
  5. `directors`
  6. `genres`
  7. `tags`
- `displayedAttributes`: `*` (all — server enriches with artwork URLs before returning to client)
- `filterableAttributes`: `type`, `library_id`, `year`, `genres`, `tags`, `rating_average`, `match_state`
- `sortableAttributes`: `title`, `year`, `rating_average`, `runtime_seconds`
- `rankingRules`: Meilisearch defaults (`["words", "typo", "proximity", "attribute", "sort", "exactness"]`)
- `typoTolerance`: enabled (default), `minWordSizeForTypos: { oneTypo: 4, twoTypos: 8 }`
- `searchCutoffMs`: 200 (matches our soft-trigger latency target)

### Index Sync Strategy

Real-time sync from PostgreSQL to Meilisearch via the existing trigger infrastructure:

1. **Modify `rebuild_media_search_vector()` trigger** — after updating `search_vector` in PG, also `NOTIFY` a channel (`media_item_changed`) with the affected `media_item_id`
2. **`workers/search_indexer.rs`** — long-running task that `LISTEN`s on `media_item_changed`, fetches the full document from PG, and `PUT /indexes/media_items/documents` to Meilisearch. Batches updates with a 500ms debounce for bulk-scan scenarios.
3. **Initial backfill** — when Meilisearch is first enabled, the worker iterates `media_items` in batches of 1000, sends to Meilisearch's `/indexes/media_items/documents?primaryKey=id` batch endpoint. Marks the index as "ready" once backfill + catch-up (drain of NOTIFY backlog) completes.
4. **Hard delete handling** — when a media item is deleted, the worker sends `DELETE /indexes/media_items/documents/{id}` to Meilisearch.
5. **Failure recovery** — if Meilisearch is unreachable, NOTIFY events queue in PG (up to `wal_sender_timeout` / slot size). Worker catches up on reconnect. If queue overflows, full backfill is triggered.

### Search API Abstraction

The search handler in `domains/media/` (or a new `domains/search/`) abstracts the backend:

```rust
pub enum SearchBackend {
    Postgres,
    Meilisearch { client: MeilisearchClient },
}

pub async fn search(
    pool: &PgPool,
    backend: &SearchBackend,
    query: &SearchRequest,
) -> Result<SearchResponse, SearchError> {
    match backend {
        SearchBackend::Postgres => postgres_search(pool, query).await,
        SearchBackend::Meilisearch { client } => client.search(query).await,
    }
}
```

The backend is selected at startup based on `RuntimeConfig.search.engine`. Switching backends requires a server restart (no hot-swap) to avoid partial-index inconsistencies.

## Faceted Filtering

Faceted search (e.g., "show me all Action movies from the 1990s with rating ≥ 7.0") works in both backends:

### PostgreSQL

PG FTS handles the text-search portion; facets are computed via separate `GROUP BY` queries:

```sql
-- Main search
SELECT id, title, ts_rank(search_vector, query) AS rank
FROM media_items, plainto_tsquery('english', $1) AS query
WHERE search_vector @@ query AND deleted_at IS NULL
ORDER BY rank DESC LIMIT 20;

-- Genre facet (separate query)
SELECT g.name, COUNT(*) FROM media_items mi
JOIN media_genres mg ON mg.media_item_id = mi.id
JOIN genres g ON g.id = mg.genre_id
WHERE mi.search_vector @@ query
GROUP BY g.name ORDER BY COUNT(*) DESC;

-- Decade facet
SELECT FLOOR(year / 10) * 10 AS decade, COUNT(*) FROM media_items
WHERE search_vector @@ query AND deleted_at IS NULL
GROUP BY decade ORDER BY decade;
```

Three queries instead of one — slower, but workable for v1.0. The web client can issue these in parallel.

### Meilisearch

Meilisearch returns facets natively in a single query response:

```json
{
  "hits": [...],
  "facetDistribution": {
    "genres": {"Action": 142, "Science Fiction": 87, ...},
    "decade": {"1990": 234, "2000": 412, ...}
  }
}
```

This is a key UX advantage for large libraries — instant facet counts without separate queries.

## Multilingual Search

### PG FTS Limitation

Each PG query uses one `regconfig`. A Japanese library (`regconfig = 'japanese'`) and an English library (`regconfig = 'english'`) cannot be searched together in one query — Japanese stemming rules don't apply to English text and vice versa.

For v1.0, this is acceptable: users searching across mixed-language libraries get results from whichever language wins the stemming. The trigram title index partially compensates (substring match is language-agnostic).

### Meilisearch Advantage

Meilisearch auto-detects document language at index time and applies the appropriate tokenizer per-document. A single search query returns Japanese and English results correctly tokenized. This is a meaningful improvement for anime/foreign-film collectors.

## v1.0 Commitment

| Commitment | Status |
|---|---|
| PostgreSQL FTS as default search | ✅ Already built (Phase 2), works for v1.0 |
| Search API (`GET /api/v1/search?q=...`) | ✅ Implemented (Pre-v1.0 Task 3) |
| Trigram fuzzy title matching | ✅ Already built (Phase 2 `pg_trgm` index) |
| Weighted relevance (A/B/C/D) | ✅ Already built (Phase 2 trigger) |
| Per-library language stemming | ✅ Already built (Phase 2 `metadata_language` → `regconfig`) |
| Faceted filtering UI | ✅ Implemented (Pre-v1.0 Task 3) |
| Meilisearch integration | Post-v1.0 — enabled when trigger threshold crossed |
| Search backend abstraction layer | Post-v1.0 — added when Meilisearch integration lands |
| `server_config.search.engine` config field | Post-v1.0 — added alongside Meilisearch integration |

**v1.0 ships search built entirely on PostgreSQL.** No search engine dependency. No Meilisearch binary to install. No index sync to manage.

## Edge Cases

### Bulk Import (Phase 14 Migration from Plex/Jellyfin)

A Phase 14 migration importing 10k+ items triggers 10k+ `rebuild_media_search_vector()` calls. Each call does a JOIN-aggregate for credits/genres/tags — expensive at scale.

**Mitigation:** The trigger should detect bulk-load patterns (multiple INSERTs in a single transaction) and defer the search-vector rebuild to a post-transaction batch job. Phase 14 implementation detail — for v1.0, the trigger runs per-row, which is acceptable for typical scan increments (hundreds of items, not thousands).

### Library Soft-Delete

When a library is soft-deleted (`libraries.deleted_at` set), all its `media_items` disappear from search results because search queries join the library and require `l.deleted_at IS NULL`. Media items themselves use hard deletion.

### Re-scan After Metadata Refresh

When TMDB `/changes` refresh updates a movie's overview (Phase 6 `metadata_refresh` worker), the trigger fires and rebuilds the search vector. Search results reflect the new overview immediately. No sync lag.

### Typo Tolerance for Non-Title Fields

PG FTS with trigram index only fuzz-matches the title. Searching "Keaenu Reeves" (typo) won't find Keanu Reeves via the cast field — FTS stems but doesn't Levenshtein-match.

**Mitigation:** Document this limitation. Power users on large libraries can enable Meilisearch for full-text typo tolerance. For v1.0, the trigram title index covers the most common typo case (users mistype titles far more than cast names).

### Search While Indexer Is Behind

If the Meilisearch indexer falls behind (server was offline, NOTIFY queue overflowed), search results may miss recently-added items. The web client shows a subtle "indexing in progress" indicator when the indexer lag exceeds 60 seconds.

### Empty Query

The search endpoint returns an empty result set (not an error) for empty queries. The web client's search page shows "Start typing to search" placeholder. Autocomplete/suggestions are not implemented (would require dedicated suggest index; out of scope).

### Special Characters and SQL Injection

Search input is parameterized (`$1` bind parameter) — SQL injection is not possible. Special characters (`'`, `"`, `\`, etc.) are passed verbatim to `plainto_tsquery`, which handles them safely. Meilisearch's HTTP API similarly escapes via JSON encoding.

### Concurrent Re-indexing

If an admin triggers a manual Meilisearch full-rebuild while the indexer is also processing real-time NOTIFY events, Meilisearch's document API handles concurrent writes safely (last-write-wins per document ID). No corruption; potential brief inconsistency window during rebuild.

## Implementation Status

| Component | Status | Notes |
|---|---|---|
| `search_vector` column + triggers + GIN index | ✅ Implemented | Phase 2 migration `20260530060200` |
| `idx_media_items_title_trgm` trigram index | ✅ Implemented | Phase 2 migration |
| `GET /api/v1/search` endpoint | ✅ Implemented | Pre-v1.0 Task 3; returns `{ items, facets }` |
| `SearchBackend` abstraction layer | Not started | Post-v1.0, lands with Meilisearch integration |
| `workers/search_indexer.rs` (Meilisearch sync) | Not started | Post-v1.0 |
| `server_config.search.engine` config field | Not started | Post-v1.0 |
| Meilisearch sidecar in Docker image | Not started | Phase 15 follow-up after search integration lands |
| Faceted search UI (genre/year/rating filters) | ✅ Implemented | Pre-v1.0 Task 3; URL-backed type/genre/year/rating filters |
| Search latency and volume metrics | ✅ Implemented | Pre-v1.0 Task 4; `search_query_duration_seconds{status,has_filters}` histogram + `search_queries_total{status,has_filters}` counter |

**First concrete post-v1.0 search work:** When an admin reports the soft-trigger threshold crossed (50k+ items or 200ms+ p95), implement the `SearchBackend` abstraction + Meilisearch sidecar + indexer worker. Until then, PG FTS is sufficient.

**Pre-v1.0 Task 3 implementation note:** `server/src/domains/search/` owns `GET /api/v1/search` using PostgreSQL FTS for v1.0. The response includes ranked `items` plus `facets` for type, genre, year, and rating thresholds. Facets run as parallel GROUP BY queries via `tokio::try_join!`, and the Svelte search page stores active filters in the URL so filtered search results are shareable.

**Pre-v1.0 Task 4 implementation note:** Search volume and latency are now observable through Prometheus. `search_queries_total{status,has_filters}` counts successful and failed searches, while `search_query_duration_seconds{status,has_filters}` provides p50/p95/p99 latency through `histogram_quantile()`. Labels deliberately avoid query text, user IDs, media IDs, and library IDs.

## Key Decisions

1. **PG FTS as v1.0 default** — Already built, zero deployment complexity, sufficient for 95%+ of home media servers (libraries under 50k items). Adding a dedicated engine to v1.0 would double deployment complexity for no benefit to typical users.
2. **Meilisearch as named migration target** — When triggered, Meilisearch is the chosen replacement. Rust matches Duskcue stack; disk-based storage (not RAM-bounded); best-in-class CJK support; MIT license; search-as-you-type optimization.
3. **Opt-in, not proactive** — Meilisearch enabled via `server_config.search.engine = "meilisearch"` when admin decides the threshold warrants it. Small deployments never pay the cost.
4. **Sidecar topology** — Meilisearch runs in the same Docker container, bound to loopback. No external exposure, no inter-container network, no separate DNS/TLS. Preserves single-container deployment model from Phase 15.
5. **Trigger-based real-time sync** — Existing `rebuild_media_search_vector()` trigger extended with `NOTIFY` channel; `search_indexer` worker LISTENs and pushes to Meilisearch. No debezium, no logical replication slots — uses PG's built-in NOTIFY/ LISTEN infrastructure.
6. **Soft trigger at 50k items, hard trigger at 100k items** — Soft trigger = admin-recommended migration; hard trigger = mandatory for acceptable UX. Based on benchmarked PG FTS latency curves with GIN index.
7. **Typesense rejected** — RAM-only storage model is prohibitive on NAS hardware (2–8GB typical). C++ memory-safety concerns. GPL-3 license friction. Meilisearch's disk-based storage wins decisively for Duskcue's deployment target.
8. **Elasticsearch/OpenSearch rejected** — JVM memory hog, operations overhead, massive overkill for media-library search. Duskcue targets self-hosted simplicity, not enterprise search infrastructure.
9. **Embedded Tantivy rejected** — Tying search CPU/RAM to the main server process risks starving API/transcode under load. Sidecar Meilisearch is cleaner separation of concerns. No path to horizontal scaling from embedded library.
10. **Search API abstracted behind `SearchBackend` enum** — Same REST endpoint serves both backends; client code unchanged. Backend swap is server-restart only (no hot-swap) to avoid partial-index inconsistencies.
11. **Faceted filtering works in both backends** — PG FTS via parallel GROUP BY queries (acceptable latency at small scale); Meilisearch via native `facetDistribution` (single query). Web client works with either.
12. **Multilingual limitation accepted for v1.0** — PG FTS uses one `regconfig` per query; mixed-language search is suboptimal but workable. Meilisearch's per-document language detection is a meaningful upgrade for anime/foreign-film collectors — another reason to migrate at scale.

## Relationship to Other Domains

| Document | Relationship |
|---|---|
| [DATABASE.md](DATABASE.md) | Authoritative source for the `media_items.search_vector` column, `rebuild_media_search_vector()` trigger, GIN index, trigram index — all documented there |
| [API_CONVENTIONS.md](API_CONVENTIONS.md) | `GET /api/v1/search?q=...` endpoint contract — see the search section |
| [MEDIA_SCANNING.md](MEDIA_SCANNING.md) | Scan writes to `media_items` → trigger fires → search vector updates. Phase 14 bulk-import mitigation documented here. |
| [METADATA_PROVIDERS.md](METADATA_PROVIDERS.md) | TMDB `/changes` refresh updates overviews → trigger fires → search reflects new metadata |
| [CONFIGURATION.md](../operations/CONFIGURATION.md) | Future `server_config.search.engine` field (post-v1.0) |
| [DOCKER_DEPLOYMENT.md](../operations/DOCKER_DEPLOYMENT.md) | Meilisearch sidecar addition when search engine enabled |
| [BUILD_ORDER.md](../../BUILD_ORDER.md) | v1.0 ships PG FTS (Phase 2 infrastructure + Pre-v1.0 Task 3 API/UI); post-v1.0 Meilisearch integration tracked as Phase 15 follow-up |

## Research Sources

- **[Meilisearch vs Typesense comparison](https://meilisearch.com/docs/resources/comparisons/typesense)** — Official Meilisearch comparison; key differentiators: disk vs RAM storage, CJK support, Rust vs C++
- **[Search engines & libraries: an overview (Alexander Reelsen, Elastic)](https://spinscale.de/posts/2020-10-20-search-engines-and-libraries-overview.html)** — Comprehensive comparison of Tantivy, Meilisearch, Bleve, Typesense, Sonic with indexing speed and index-size benchmarks
- **[PostgreSQL Full Text Search](https://www.postgresql.org/docs/18/textsearch.html)** — PG 18 text-search documentation; `tsvector`, `tsquery`, GIN/GiST indexes, weighted search
- **[pg_trgm extension](https://www.postgresql.org/docs/18/pgtrgm.html)** — Trigram-based fuzzy matching for typo tolerance
- **[Tantivy](https://github.com/quickwit-oss/tantivy)** — Rust full-text search library (Lucene-equivalent); embedded option considered and rejected
- **[Meilisearch documentation](https://www.meilisearch.com/docs)** — Settings API, filterable attributes, ranking rules, typo tolerance, faceting
- **[PostgreSQL NOTIFY/LISTEN](https://www.postgresql.org/docs/18/sql-notify.html)** — Real-time event notification used for PG → Meilisearch sync
- **[Reddit: Meilisearch raised $5M](https://www.reddit.com/r/rust/comments/se09y6/)** — Community perspective on Rust-based search engines (Meilisearch, Tantivy, Quickwit, Toshi)
