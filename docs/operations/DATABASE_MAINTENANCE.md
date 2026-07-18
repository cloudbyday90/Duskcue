# Database Maintenance & Bloat Management

## Overview

Strategy for preventing and managing database bloat across all PostgreSQL tables, tailored to the specific update patterns of a self-hosted Duskcue (1-50 users). Covers autovacuum tuning, HOT updates via fillfactor, index maintenance via REINDEX CONCURRENTLY, partitioned table ANALYZE, and configurable admin settings.

**Core principle:** Prevent bloat proactively via per-table autovacuum tuning and HOT updates, rather than reacting to bloat after it accumulates. This avoids the need for heavy tools like pg_repack or pg_squeeze.

---

## Bloat Risk Analysis

Every table in the schema was analyzed for update frequency and bloat risk:

| Risk | Table | Update Pattern | Bloat Driver |
|---|---|---|---|
| **CRITICAL** | `user_item_data` | `resume_position_ms` updated every 10-30s during playback | Highest UPDATE frequency in the entire system |
| **HIGH** | `server_config` | Single row updated on any admin setting change | 100% page churn on every update |
| **HIGH** | `users` | `last_login_at`, `failed_login_attempts`, streaming policy overrides | Moderate UPDATE frequency |
| **HIGH** | `user_sessions` | `last_active_at` updated on every authenticated request | High UPDATE frequency |
| **MEDIUM** | `media_items` | `updated_at`, `match_state`, `search_vector` (trigger), metadata enrichment | Moderate UPDATE during scans |
| **MEDIUM** | `play_sessions` | Active session rows updated on stop/pause | Moderate UPDATE (current month partition) |
| **MEDIUM** | `scheduled_tasks` | `state`, `next_run_at`, `last_run_*` after every task execution | Moderate UPDATE |
| **LOW** | `storyboards`, `media_segments`, `media_fingerprints` | INSERT-heavy; rarely updated after creation | Append-only |
| **LOW** | `libraries`, `movies`, `series`, `seasons`, `episodes` | INSERT-heavy; metadata refresh is infrequent | Mostly append |
| **NONE** | `play_events`, `audit_log` | Append-only; old partitions dropped via DETACH | INSERT only |

---

## Strategy 1: Per-Table Autovacuum Tuning

### Why Not Default Settings?

PostgreSQL's default autovacuum scale factors are too conservative for production use:

- Default `autovacuum_vacuum_scale_factor = 0.2` (20%) — a table with 1M rows needs 200K dead tuples before autovacuum fires
- Default `autovacuum_vacuum_threshold = 50` — negligible for large tables
- Default `autovacuum_vacuum_cost_limit = 200` — throttles vacuum too aggressively

At our scale, this means `user_item_data` (which receives ~4 updates/minute during active playback) would accumulate significant bloat before autovacuum notices.

### Global Defaults

Applied via `postgresql.conf` or `ALTER DATABASE`:

```
autovacuum_vacuum_scale_factor = 0.1
autovacuum_analyze_scale_factor = 0.05
autovacuum_vacuum_threshold = 500
autovacuum_analyze_threshold = 500
autovacuum_vacuum_cost_limit = 1000
autovacuum_freeze_max_age = 1000000000
```

### Per-Table Overrides

Applied via `ALTER TABLE ... SET (...)` in migrations:

#### `user_item_data` (CRITICAL)

```sql
ALTER TABLE user_item_data SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_analyze_scale_factor = 0.01,
    autovacuum_vacuum_cost_delay = 1,
    autovacuum_vacuum_cost_limit = 2000
);
```

Vacuum triggers at ~2% dead tuples. For a 10K-row table, that's ~200 dead tuples — fires within minutes of active playback. Cost limit doubled for faster cleanup. Cost delay halved.

#### `user_sessions` (HIGH)

```sql
ALTER TABLE user_sessions SET (
    autovacuum_vacuum_scale_factor = 0.05,
    autovacuum_analyze_scale_factor = 0.02
);
```

#### `server_config` (HIGH — single row)

```sql
ALTER TABLE server_config SET (
    autovacuum_vacuum_scale_factor = 0.0,
    autovacuum_vacuum_threshold = 1,
    autovacuum_analyze_scale_factor = 0.0,
    autovacuum_analyze_threshold = 1
);
```

Single-row table: vacuum after every single change. Scale factor 0 means only threshold applies.

#### `scheduled_tasks` (MEDIUM)

```sql
ALTER TABLE scheduled_tasks SET (
    autovacuum_vacuum_scale_factor = 0.05,
    autovacuum_analyze_scale_factor = 0.02
);
```

#### `users` (HIGH)

```sql
ALTER TABLE users SET (
    autovacuum_vacuum_scale_factor = 0.05,
    autovacuum_analyze_scale_factor = 0.02
);
```

#### `media_items` (MEDIUM)

```sql
ALTER TABLE media_items SET (
    autovacuum_vacuum_scale_factor = 0.05,
    autovacuum_analyze_scale_factor = 0.02
);
```

### Why Not Tune Every Table?

Tables in the LOW and NONE risk categories are append-heavy or insert-only. Default autovacuum handles them adequately. Over-tuning small or static tables wastes I/O for no benefit. The tuning philosophy is: tune aggressively only where bloat risk is proven.

---

## Strategy 2: Fillfactor + HOT Updates

### What Are HOT Updates?

Heap-Only Tuple (HOT) updates occur when PostgreSQL can store the new version of a row on the same data page as the old version, without updating any indexes. This eliminates index bloat from UPDATE operations entirely.

**Two conditions for HOT updates:**
1. The `UPDATE` does not modify any indexed column
2. There is enough free space on the same data page for the new row version

### Why `user_item_data` Is a Perfect Candidate

During playback, the server updates `resume_position_ms` and `updated_at` every 10-30 seconds. Neither of these columns has an index on it directly. The indexed columns (`user_id`, `media_item_id`) are never updated. This means every playback heartbeat UPDATE is eligible for HOT — if there's space on the page.

### Implementation

```sql
ALTER TABLE user_item_data SET (fillfactor = 85);
```

`fillfactor = 85` reserves 15% of each data page for future HOT updates. With a typical row size of ~100 bytes and 8KB pages, this provides space for ~12 HOT updates per page before fallback to regular updates.

### Impact

| Metric | Without fillfactor | With fillfactor 85 |
|---|---|---|
| HOT update rate during playback | ~0% (pages full from inserts) | ~95%+ |
| Index bloat from playback heartbeats | Significant over time | Near zero |
| VACUUM workload | Must reclaim dead tuples from many pages | HOT chains self-prune during normal reads |
| Table disk space | Baseline | +15% (~negligible: 10K rows × 100 bytes × 0.15 = ~150 KB) |
| Read performance | Normal | Marginally more pages to scan (imperceptible) |

**Note:** Lowering fillfactor increases measured "bloat" in monitoring tools (pgstattuple estimates unused space). This is expected and not real bloat — it's reserved space working as designed. The `reindex_maintenance` task accounts for this by excluding fillfactor-reserved space from its bloat calculations.

### Monitoring HOT Update Rate

```sql
SELECT
    n_tup_upd AS total_updates,
    n_tup_hot_upd AS hot_updates,
    CASE WHEN n_tup_upd > 0
        THEN round(100.0 * n_tup_hot_upd / n_tup_upd, 1)
        ELSE 0
    END AS hot_percent
FROM pg_stat_user_tables
WHERE relname = 'user_item_data';
```

Target: >90% HOT update rate during active playback. If the rate drops below 80%, consider lowering fillfactor to 80.

---

## Strategy 3: REINDEX CONCURRENTLY Scheduled Task

### Why Indexes Bloat

Even with aggressive autovacuum, indexes accumulate empty pages over time from:
- Updates to indexed columns (not covered by HOT)
- Deleted rows leaving gaps in B-tree leaf pages
- Bulk operations (library scans inserting thousands of rows)

### Task Design

A new `reindex_maintenance` scheduled task that:

1. Queries `pg_stat_user_indexes` and `pgstattuple` to estimate bloat per index
2. Filters to indexes exceeding the configurable bloat threshold (default 30%)
3. Filters to indexes above a minimum size (default 10 MB) to avoid wasting time on tiny indexes
4. Runs `REINDEX INDEX CONCURRENTLY` on each qualifying index
5. Logs results to `scheduled_task_runs.stats`

### Configuration

Stored in `server_config.maintenance` JSONB (see [CONFIGURATION.md](CONFIGURATION.md)):

```json
{
    "reindex_enabled": true,
    "reindex_schedule": "0 2 * * 0",
    "reindex_bloat_threshold_percent": 30,
    "reindex_min_index_size_mb": 10
}
```

### Default Schedule

| Parameter | Value |
|---|---|
| Task type | `reindex_maintenance` |
| Schedule | `0 2 * * 0` (weekly Sunday 02:00) |
| Timeout | 2 hours |
| Config | `{ "bloat_threshold_percent": 30, "min_index_size_mb": 10 }` |

### Bloat Detection Query

The task uses `pgstattuple` for accurate bloat measurement:

```sql
CREATE EXTENSION IF NOT EXISTS pgstattuple;

SELECT
    schemaname,
    tablename,
    indexrelname,
    pg_size_pretty(pg_relation_size(indexrelid)) AS index_size,
    round(100.0 - avg_leaf_density, 2) AS bloat_percent
FROM pg_stat_user_indexes
CROSS JOIN LATERAL pgstatindex(indexrelid::regclass::text)
WHERE schemaname = 'public'
    AND pg_relation_size(indexrelid) > ($min_size_mb * 1024 * 1024)
    AND (100.0 - avg_leaf_density) > $bloat_threshold
ORDER BY (100.0 - avg_leaf_density) DESC;
```

`avg_leaf_density` below 70% indicates significant bloat. The default 30% threshold means: only reindex when bloat exceeds 30% of index size.

### Exclusions

The task skips:
- Indexes on partitioned parent tables (they store no data)
- Indexes smaller than `min_index_size_mb` (not worth the overhead)
- Indexes where the table has `fillfactor < 100` and bloat is within the fillfactor margin (expected reserved space)

### REINDEX CONCURRENTLY Behavior

- No blocking locks — reads and writes continue normally
- Builds a new index in parallel, then swaps atomically
- Requires temporary disk space (~2x index size during operation)
- Cannot run inside a transaction
- Takes longer than regular REINDEX but zero downtime

### Task 7 Implementation Notes

Phase 13a Task 7 implements the scheduled worker at `server/src/workers/reindex_maintenance.rs`.

Implementation details:

- `MaintenanceConfig` and `PartitionRetention` are now typed runtime config structs in `server/src/state.rs`; empty `server_config.maintenance = {}` rows deserialize to the documented defaults.
- The worker reads defaults from `server_config.maintenance` and lets the scheduled-task `config` JSON override `enabled`, `bloat_threshold_percent`, and `min_index_size_mb`.
- Candidate discovery uses `pgstatindex(idx.oid::regclass)` and filters to public-schema B-tree indexes that are valid, ready, above the configured size threshold, and above the configured bloat threshold.
- Partitioned parent indexes are skipped by filtering for normal index relkind (`idx.relkind = 'i'`), and exclusion-constraint backing indexes are skipped.
- Reindexing is executed as individual `REINDEX INDEX CONCURRENTLY "schema"."index"` statements. Identifiers are double-quoted after escaping embedded quotes, and no explicit transaction is opened.
- Each run writes structured candidate results to `scheduled_task_runs.stats`, including action (`reindexed`, `failed`, or `skipped_expected_fillfactor`), bloat percentage, size, table fillfactor, and per-index error text.
- Failed candidates do not stop the remaining candidates from running. If any candidate fails, the worker returns an error after persisting stats so the scheduler records the task run as failed.
- The worker emits `maintenance_reindex_total` and `maintenance_reindex_bloat_before` metrics for successfully reindexed candidates.

Verification performed: `cargo check -p duskcue` and `cargo test -p duskcue reindex_maintenance`.

---

## Strategy 4: ANALYZE Partitioned Parent Tables

### The Problem

PostgreSQL autovacuum processes individual partitions but **not partitioned parent tables**. The parent table `play_sessions` has no rows of its own, so autovacuum never touches it. Without fresh statistics on the parent, the query planner cannot accurately estimate row counts across partitions, leading to:
- Suboptimal partition pruning (scanning more partitions than necessary)
- Bad join order decisions
- Incorrect index vs sequential scan choices

### Solution

A new `analyze_parents` scheduled task that runs `ANALYZE` on each partitioned parent table daily:

```sql
ANALYZE play_sessions;
ANALYZE play_events;
ANALYZE audit_log;
```

### Default Schedule

| Parameter | Value |
|---|---|
| Task type | `analyze_parents` |
| Schedule | `0 3 * * *` (daily 03:00, after library scan) |
| Timeout | 5 minutes |
| Config | `{}` |

ANALYZE is fast (statistical sampling, not a full scan) — typically completes in under 1 second per table even with millions of rows.

### Implementation Contract

The `analyze_parents` worker reads `analyze_parent_tables_enabled` from the
typed `MaintenanceConfig`; a scheduled-task `enabled` value overrides it for a
single task. When enabled, it runs plain `ANALYZE` (without `ONLY`) for each
partitioned parent so PostgreSQL refreshes both the parent inheritance
statistics and the child-partition statistics. It persists the per-parent
results in `scheduled_task_runs.stats` and returns an error if any parent
cannot be analyzed, allowing the scheduler's normal failure handling to retry
the task.

`SKIP LOCKED` is intentionally not used. PostgreSQL documents that a conflicting
lock on a partitioned table can make `ANALYZE SKIP LOCKED` skip all of its
partitions, which would report a successful but stale maintenance run. The
five-minute timeout and daily cadence instead provide a bounded, observable
failure path. Sources rechecked on 2026-07-18: [PostgreSQL routine
vacuuming](https://www.postgresql.org/docs/current/routine-vacuuming.html) and
[PostgreSQL ANALYZE](https://www.postgresql.org/docs/current/sql-analyze.html).

---

## Strategy 5: Partition Retention via DETACH

Already designed in DATABASE.md. Documented here for completeness.

### Retention Policy

| Table | Retention | Rationale |
|---|---|---|
| `play_sessions` | 24 months | Two full years of watch history for analytics and Trakt sync reconciliation |
| `play_events` | 12 months | Granular event data loses value faster than session summaries |
| `audit_log` | 12 months | Compliance; configurable via `server_config.maintenance` |

### How DETACH Prevents Bloat

```sql
ALTER TABLE play_sessions DETACH PARTITION play_sessions_2024_05 CONCURRENTLY;
DROP TABLE play_sessions_2024_05;
```

- Near-instant operation — no VACUUM needed
- Zero dead tuples — the partition is dropped as a whole table
- Zero index bloat — the partition's indexes are dropped with it
- `CONCURRENTLY` (PG14+) avoids blocking reads or writes on the parent table

This is the primary reason we partitioned these tables — retention management without bloat.

---

## Why NOT pg_repack / pg_squeeze

Both extensions rebuild tables to eliminate bloat with minimal locking. Evaluated and rejected:

| Tool | Rejection Reason |
|---|---|
| **pg_repack** | Requires client-side binary (`pg_repack` CLI); requires `shared_preload_libraries`; requires 2x disk space during operation; adds Docker image complexity for zero benefit at our scale |
| **pg_squeeze** | Requires `wal_level = logical`; requires `shared_preload_libraries`; requires replication slots; adds Docker image complexity; designed for multi-GB tables with severe bloat |

**Why not needed:** Our database serves 1-50 users. The tables will never reach the scale (multi-GB single tables with 50%+ bloat) where these tools provide value. Proper autovacuum tuning + fillfactor + REINDEX CONCURRENTLY prevents bloat from accumulating. If bloat does occur, `VACUUM FULL` during a maintenance window is acceptable for a home server — it's not a 24/7 enterprise SLA system.

---

## Guard Against Unnecessary Trims

| Data | Trim Risk | Guard |
|---|---|---|
| Storyboard sprites | LRU eviction removes items that might be requested next | Priority retention for items played in last 30 days; first evicted: 90+ day cold items |
| Image cache | Resized images deleted then immediately re-requested | 2 GB LRU cache generous for most libraries; eviction only triggers at size cap |
| `user_item_data` dead tuples | VACUUM reclaims space, but new heartbeats immediately create more | Fillfactor 85 + per-table autovacuum at 2% ensures steady-state, not accumulation |
| Search vectors | Trigger rebuilds vector on every `media_items` change | No trim needed; trigger handles automatically |
| Partitioned data | DETACH drops entire partitions | Already designed; not a "trim" — instant table drop |
| Index bloat | REINDEX could run on already-healthy indexes | Bloat threshold (default 30%) + minimum size (default 10 MB) prevent unnecessary work |

---

## Admin Configuration

### `server_config.maintenance` JSONB

New JSONB column on `server_config` (see DATABASE.md for DDL):

```json
{
    "autovacuum_tuning_enabled": true,
    "reindex_enabled": true,
    "reindex_schedule": "0 2 * * 0",
    "reindex_bloat_threshold_percent": 30,
    "reindex_min_index_size_mb": 10,
    "partition_retention_months": {
        "play_sessions": 24,
        "play_events": 12,
        "audit_log": 12
    },
    "analyze_parent_tables_enabled": true,
    "analyze_parent_schedule": "0 3 * * *"
}
```

### MaintenanceConfig Rust Struct

```rust
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct MaintenanceConfig {
    pub autovacuum_tuning_enabled: bool,
    pub reindex_enabled: bool,
    pub reindex_schedule: String,
    pub reindex_bloat_threshold_percent: u8,
    pub reindex_min_index_size_mb: u32,
    pub partition_retention_months: PartitionRetention,
    pub analyze_parent_tables_enabled: bool,
    pub analyze_parent_schedule: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct PartitionRetention {
    pub play_sessions: u32,
    pub play_events: u32,
    pub audit_log: u32,
}
```

### Admin UI Controls

| Setting | UI Element | Range | Default |
|---|---|---|---|
| Auto-tune autovacuum | Toggle | on/off | On |
| Reindex maintenance | Toggle | on/off | On |
| Reindex schedule | Cron input | any valid cron | Sun 02:00 |
| Reindex bloat threshold | Slider | 10-50% | 30% |
| Reindex min index size | Slider | 1-100 MB | 10 MB |
| Play sessions retention | Dropdown | 6/12/24/36 months | 24 months |
| Play events retention | Dropdown | 6/12/24 months | 12 months |
| Audit log retention | Dropdown | 6/12/24 months | 12 months |
| Analyze parent tables | Toggle | on/off | On |

When `autovacuum_tuning_enabled` is toggled off, the server resets all per-table autovacuum overrides back to PostgreSQL defaults. When toggled on, it re-applies the recommended settings. This gives admins an escape hatch if autovacuum behavior is unexpected.

---

## New Scheduled Tasks

The scheduled task registry includes the following maintenance task types:

### partition_management

The scheduled `partition_management` worker creates the current monthly
partition plus a bounded horizon of one to twelve future months for
`play_sessions`, `play_events`, and `audit_log`. Its default horizon is two
months, read from task config as `create_ahead_months`. Existing partitions are
left unchanged; each run writes per-table/month actions and failures to
`scheduled_task_runs.stats`, and any creation failure marks the scheduled run
failed so the scheduler can retry it. This worker deliberately creates
partitions only: automatic retention detaches/drops remain a separate,
destructive maintenance change requiring its own safety and recovery contract.

PostgreSQL documents creating empty `PARTITION OF` tables for future data and
notes that concurrent partition detach has additional lifecycle restrictions.
The selected creation-only design therefore prevents missing-partition write
failures without silently deleting retained history. Sources rechecked on
2026-07-18: [PostgreSQL table partitioning](https://www.postgresql.org/docs/current/ddl-partitioning.html)
and [PostgreSQL ALTER TABLE](https://www.postgresql.org/docs/current/sql-altertable.html).

### reindex_maintenance

| Parameter | Value |
|---|---|
| Task type | `reindex_maintenance` |
| Cron | `0 2 * * 0` (weekly Sunday 02:00) |
| Timeout | 2 hours |
| Config | `{ "bloat_threshold_percent": 30, "min_index_size_mb": 10 }` |

### analyze_parents

| Parameter | Value |
|---|---|
| Task type | `analyze_parents` |
| Cron | `0 3 * * *` (daily 03:00) |
| Timeout | 5 minutes |
| Config | `{}` |

### Updated Complete Task Schedule

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
| Notification Cleanup | Every 86400s (daily) | 5m | `{ "max_age_days": 90, "stale_device_days": 30 }` |
| Trust Score Recalculation | Every 3600s (hourly) | 5m | `{}` |
| Segment Analysis | `0 3 * * *` (daily 03:00) | 4h | `{ "max_concurrent_analyses": 1 }` |
| Storyboard Generation | `0 4 * * *` (daily 04:00) | 4h | `{ "max_concurrent_analyses": 1, "interval_mode": "adaptive" }` |
| Disk Space Check | Every 1800s (30 min) | 1m | `{ "check_paths": true }` |
| Reindex Maintenance | `0 2 * * 0` (weekly Sun 02:00) | 2h | `{ "bloat_threshold_percent": 30, "min_index_size_mb": 10 }` |
| Analyze Parents | `0 3 * * *` (daily 03:00) | 5m | `{}` |

---

## Metrics

### Bloat Monitoring Metrics

Exposed via Prometheus `/metrics` endpoint:

| Metric | Type | Labels | Description |
|---|---|---|---|
| `db_table_bloat_bytes` | gauge | table | Estimated bloat per table |
| `db_table_bloat_ratio` | gauge | table | Bloat as fraction of table size |
| `db_index_bloat_bytes` | gauge | table, index | Estimated bloat per index |
| `db_dead_tuples` | gauge | table | Current dead tuple count from `pg_stat_user_tables` |
| `db_hot_update_ratio` | gauge | table | HOT updates as percentage of total updates |
| `db_autovacuum_last_run` | gauge | table | Timestamp of last autovacuum per table |
| `db_last_analyze` | gauge | table | Timestamp of last ANALYZE per table |

### Maintenance Task Metrics

| Metric | Type | Labels | Description |
|---|---|---|---|
| `maintenance_reindex_total` | counter | table, index | Indexes reindexed |
| `maintenance_reindex_bloat_before` | gauge | table, index | Bloat % before reindex |
| `maintenance_parent_analyze_total` | counter | parent_table | Successful parent-table ANALYZE operations |
| `maintenance_parent_analyze_failures_total` | counter | parent_table | Failed parent-table ANALYZE operations |
| `maintenance_partitions_detached` | counter | table | Partitions detached for retention |

---

## Integration with Existing Systems

### Database Scanning (MEDIA_SCANNING.md)

Library scans bulk-INSERT into `media_items`, `media_files`, etc. These are append-only operations — autovacuum handles them via the insert threshold. The `analyze_parents` task does not affect scan performance.

### Streaming (STREAMING.md)

Playback heartbeats UPDATE `user_item_data.resume_position_ms` every 10-30 seconds. The fillfactor + HOT updates + aggressive autovacuum on this table directly prevent the most common source of bloat.

### Cache & Storage (CACHE_STORAGE.md)

The `disk_space_check` task monitors `/data` volume usage. If autovacuum or REINDEX operations consume significant temporary space, the disk monitoring will alert admins. The worker is implemented in `server/src/workers/disk_space_check.rs` (Phase 13a Task 8); see [CACHE_STORAGE.md](CACHE_STORAGE.md) §Phase 13a Task 8 Implementation Notes for the monitoring design and Prometheus metrics.

### Backup & Recovery (BACKUP_RECOVERY.md)

REINDEX CONCURRENTLY is safe during WAL-G continuous archiving. The new indexes are logged in WAL. No interaction with backup timing.

---

## Research Sources

### Autovacuum Tuning
- PostgreSQL 18 Official Documentation — Routine Vacuuming (Section 24.1): https://www.postgresql.org/docs/current/routine-vacuuming.html
- PostgreSQL 18 Official Documentation — Autovacuum Configuration (Section 19.10): https://www.postgresql.org/docs/current/runtime-config-vacuum.html
- Snowflake Engineering — "Postgres Vacuum Explained: Autovacuum, Bloat and Tuning" (March 2026)
- Keith F4 — "Per-Table Autovacuum Tuning" (industry-standard reference for per-table autovacuum configuration)

### HOT Updates & Fillfactor
- CYBERTEC — "HOT Updates in PostgreSQL for Better Performance" (updated 2023)
- Crunchy Data — "Postgres Performance Boost: HOT Updates and Fill Factor" (March 2024)
- Reddit r/PostgreSQL — "Postgres with high update workload" case study (February 2026): achieved 100% HOT updates with fillfactor tuning

### Index Maintenance
- PostgreSQL 18 Official Documentation — REINDEX: https://www.postgresql.org/docs/current/sql-reindex.html
- OneUptime — "How to Build PostgreSQL Index Maintenance Strategy" (January 2026)

### pg_repack / pg_squeeze (Evaluated and Rejected)
- CYBERTEC PostgreSQL — pg_squeeze GitHub repository: https://github.com/cybertec-postgresql/pg_squeeze
- CYBERTEC — pg_squeeze product page: https://www.cybertec-postgresql.com/en/products/pg_squeeze/
- Microsoft Azure — "Full vacuum using pg_repack in Azure Database for PostgreSQL" (December 2025)

### Partitioning
- Medium — "PostgreSQL partitioning — 4 strategies for managing large tables" (February 2026)
- Medium — "When to Consider Postgres Partitioning in 2026" (February 2026)
- OneUptime — "How to Implement Table Partitioning in PostgreSQL" (January 2026)
