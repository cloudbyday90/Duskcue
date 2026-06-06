# Migration Strategy

## Overview

This document defines the database migration strategy for the server. It covers tool selection, file naming conventions, migration lifecycle, idempotency requirements, and operational procedures.

## Tool Selection

### Research Summary (May 2026)

| Tool | Version | Language | Async | PITR-aware | Time-based Naming | Checksums | Status |
|---|---|---|---|---|---|---|---|
| **sqlx-cli** | 0.9.0 | Rust | Yes | No | Yes (timestamps) | Yes | Active, just released May 2026 |
| **refinery** | 0.9.0 | Rust | Yes | No | Both (V/U prefix) | Yes | Active |
| **Prisma Migrate** | 7.8.0 | Node/TS | Yes | No | Yes (timestamps) | Yes | Active |
| **Flyway** | 10.10+ | Java | No | No | Both | Yes | Active |
| Custom runner | — | Any | Any | No | Any | Any | Build-your-own |

### How Classifarr Does It

Classifarr uses a **custom ESM migration runner** (`server/src/config/migrations.mjs`) that:

1. Tracks applied migrations in a `schema_migrations` table
2. Supports two naming conventions: legacy numeric (`XXX_name.sql`) and timestamp (`YYYYMMDD_HHMMSS_name.sql`)
3. Runs migrations in deterministic order on application startup
4. Uses a fail-fast approach — stops on first error
5. Requires all migrations to be idempotent (`IF NOT EXISTS`, `IF EXISTS`, `DO $$ ... $$`)
6. Has CI validation (`npm run migration:check`) enforcing naming conventions
7. Provides a generator script (`npm run migration:create`) for timestamp-based filenames
8. Bootstrap mode: uses `database/schema/current.sql` for fresh installs instead of replaying all migrations

### Evaluation

| Criterion | sqlx-cli 0.9 | refinery 0.9 | Custom (Classifarr-style) |
|---|---|---|---|
| **Rust-native** | Yes | Yes | No (would need Rust impl) |
| **Embedded in binary** | Yes (`include_str!`) | Yes (`embed_migrations!`) | No (filesystem) |
| **Time-based naming** | Yes | Yes (V prefix + number) | Yes |
| **Checksum verification** | Yes | Yes | Must implement |
| **No external binary** | No (CLI tool) | No (CLI tool) | Yes (in-process) |
| **Compile-time query checking** | Yes (with macros) | No | No |
| **PostgreSQL 18 support** | Yes | Yes | Yes |
| **sqlx.toml (0.9)** | Yes (new) | No | N/A |
| **Offline mode** | Yes (`.sqlx` cache) | No | N/A |
| **Complexity** | Medium | Low | High |
| **Maintenance burden** | Low (community) | Low (community) | High (us) |

### Decision: sqlx-cli 0.9

sqlx-cli 0.9.0 released May 6, 2026 with significant improvements:

- **`sqlx.toml`** — Per-crate configuration for DATABASE_URL renaming, type overrides, migration table naming, and more
- **`SqlSafeStr`** — Security speedbump against naive `format!()` query building
- **Removed `Cargo.lock`** from tracking — eliminates merge conflicts
- **Advisory lock** improvements for Postgres migration safety
- **Deterministic migration order** fix (PR #4136)
- **Transferred to new org** (transact-rs) — no longer LaunchBadge

We're already using SQLx as our query layer. Using sqlx-cli gives us:
- Single ecosystem for queries + migrations
- Compile-time query verification against actual schema
- Migration files embedded in the binary at compile time
- Built-in checksum verification prevents silent drift
- No custom migration runner to maintain

## Naming Convention: Timestamp-based

### Format

```
YYYYMMDD_HHMMSS_descriptive_name.sql
```

Examples:
```
20260530_030000_create_core_media_tables.sql
20260530_030100_create_trakt_integration.sql
20260530_030200_create_activity_analytics.sql
20260530_030300_create_playback_domain.sql
20260530_040000_create_auth_domain.sql
20260530_050000_create_system_domain.sql
20260530_060000_create_cross_cutting_concerns.sql
20260530_060100_create_audit_triggers.sql
20260530_060200_create_full_text_search.sql
20260530_070000_seed_default_data.sql
```

### Why Timestamps Over Sequential Numbers

| Factor | Sequential (001, 002...) | Timestamp |
|---|---|---|
| **Merge conflicts** | Frequent — two devs pick the same number | Rare — timestamps are unique per second |
| **Branch ordering** | Broken — branch B may have `005` that should run before branch A's `004` | Natural — chronological order is correct |
| **Code review** | Confusing — "is this migration before or after the one in main?" | Clear — timestamp shows exactly when it was created |
| **Auditability** | Weak — number tells you nothing | Strong — filename shows creation time |
| **Team scaling** | Requires coordination | No coordination needed |
| **Industry trend** | Legacy | Modern — Flyway, Prisma, Rails, Django all support timestamps |
| **Classifarr pattern** | Legacy (phased out) | Current standard (enforced by CI) |

This matches the Classifarr convention and the broader industry trend. The sqlx-cli `migrate add` command generates timestamp-prefixed files by default.

## Migration Architecture

### Directory Structure

```
server/
├── migrations/
│   ├── 20260530_030000_create_core_media_tables.sql
│   ├── 20260530_030100_create_trakt_integration.sql
│   ├── 20260530_030200_create_activity_analytics.sql
│   ├── 20260530_030300_create_playback_domain.sql
│   ├── 20260530_040000_create_auth_domain.sql
│   ├── 20260530_050000_create_system_domain.sql
│   ├── 20260530_060000_create_cross_cutting_concerns.sql
│   ├── 20260530_060100_create_audit_triggers.sql
│   ├── 20260530_060200_create_full_text_search.sql
│   └── 20260530_070000_seed_default_data.sql
├── src/
│   └── ...
├── sqlx.toml
└── Cargo.toml
```

### sqlx.toml Configuration (New in 0.9)

```toml
[migrate]
# Custom table name (matches Classifarr convention)
table-name = "schema_migrations"

# Don't run migrations during tests
# (tests use their own database setup)
```

### Migration Tracking Table

sqlx automatically creates and manages `_sqlx_migrations` (or whatever is configured in `sqlx.toml`). The table tracks:

| Column | Type | Purpose |
|---|---|---|
| `version` | BIGINT | Timestamp from filename (e.g. `20260530030000`) |
| `description` | TEXT | Human-readable name from filename |
| `installed_on` | TIMESTAMPTZ | When the migration was applied |
| `success` | BOOLEAN | Whether it completed successfully |
| `checksum` | BYTEA | SHA-256 of migration file content |

### Application Startup Flow

```
1. Application starts
2. Connect to PostgreSQL
3. Run `sqlx::migrate!("./migrations").run(&pool).await`
4. sqlx compares applied migrations (from tracking table) with embedded files
5. Pending migrations run in timestamp order
6. Each migration runs in a transaction (PostgreSQL supports transactional DDL)
7. Checksums verified — fails if a previously-applied migration was modified
8. Application is ready
```

## Migration Lifecycle Rules

### 1. Migrations Are Append-Only

Once a migration is applied to any environment, its file is **immutable**. Never edit an applied migration. If a change is wrong, create a new migration that fixes it.

### 2. Every Migration Must Be Idempotent

Use defensive SQL patterns:

```sql
-- Tables
CREATE TABLE IF NOT EXISTS my_table (...);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_name ON table(col);

-- Columns
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'my_table' AND column_name = 'new_col'
    ) THEN
        ALTER TABLE my_table ADD COLUMN new_col TEXT;
    END IF;
END $$;

-- Data seeding
INSERT INTO config (key, value)
SELECT 'setting', 'value'
WHERE NOT EXISTS (SELECT 1 FROM config WHERE key = 'setting');
```

This is critical because:
- Fresh installs run all migrations from scratch
- Failed-and-retried migrations must be safe to re-run
- Development environments reset frequently

### 3. One Migration Per Logical Change

Each migration should represent a single, atomic schema change:

- Good: `20260615_120000_add_user_preferences_table.sql`
- Good: `20260615_120100_add_theme_column_to_users.sql`
- Bad: `20260615_120000_add_prefs_and_fix_indexes_and_seed_data.sql`

### 4. Data Migrations Are Separate From Schema Migrations

If a migration involves significant data transformation (e.g. backfilling a new column), split into:
1. Schema migration: add column, add index
2. Data migration: populate column in batches (separate migration or scheduled task)

For large data migrations, prefer using the `scheduled_tasks` system instead of inline SQL.

### 5. No Down Migrations in Production

Following Flyway and refinery's philosophy: production rollbacks are new forward migrations. The undo path is:

1. Create a new migration that reverses the change
2. Test in development
3. Apply to production

For emergencies, use PITR (see [BACKUP_RECOVERY.md](../operations/BACKUP_RECOVERY.md)) to restore to a point before the bad migration.

## Creating Migrations

### CLI Command

```bash
# Create a new migration
sqlx migrate add -r create_user_preferences

# Creates:
#   migrations/20260615_120000_create_user_preferences.up.sql
#   migrations/20260615_120000_create_user_preferences.down.sql
```

The `-r` flag creates both up and down files for development convenience. The down file is used during development only (`sqlx migrate revert`). It is never run in production.

### Development Workflow

```bash
# Create migration
sqlx migrate add add_user_avatar_url

# Write the SQL
# edit migrations/20260615_120000_add_user_avatar_url.up.sql

# Apply to local database
sqlx migrate run

# Verify
psql -c "\d users"

# If wrong, revert and fix
sqlx migrate revert

# Re-apply
sqlx migrate run
```

### Production Deployment

```bash
# In the server binary — migrations run automatically on startup
# Or via CLI:
sqlx migrate run

# Migrations are embedded in the binary — no external files needed
```

## Fresh Install Bootstrap

On a fresh install, all migrations run from scratch in timestamp order. This is the expected behavior and why idempotency matters.

For performance on fresh installs (e.g. Docker containers), the application could optionally detect a fresh database and apply a "schema snapshot" — a single SQL file representing the current complete schema. This is an optimization, not a requirement.

## Integration with Existing Systems

### Scheduled Tasks

The `database_maintenance` scheduled task handles:
- `VACUUM ANALYZE` on high-churn tables
- Partition creation and cleanup
- Statistics refresh after bulk operations

It does **not** handle schema migrations — those are the domain of sqlx.

### Audit Trail

Schema changes made by migrations are NOT tracked in the `audit_log` table. The `schema_migrations` tracking table serves this purpose. Audit logging is for application-level data changes.

### Backup Coordination

By default, migrations run on application startup before the server accepts connections. WAL-G captures the resulting schema changes through normal WAL archival. No special backup coordination is needed.

For large migrations (adding indexes on large tables, backfilling data):
1. Consider using `CREATE INDEX CONCURRENTLY` in a non-transactional migration (add `-- no-transaction` comment at the top)
2. Schedule during a maintenance window
3. Verify backup health before and after

## Migration Hardening

### Checksum Verification

sqlx computes a SHA-256 checksum of each migration file at compile time and stores it in the tracking table. If a previously-applied migration file is modified, sqlx will refuse to run and report the drift. This prevents:

- Accidental edits to applied migrations
- Schema drift between environments
- Partial application of changes

### Transaction Safety

PostgreSQL supports transactional DDL — `CREATE TABLE`, `ALTER TABLE`, `CREATE INDEX`, etc. can all run inside transactions. sqlx wraps each migration in a transaction by default:

- If any statement fails, the entire migration rolls back
- The tracking table is not updated for failed migrations
- The migration can be safely retried

For migrations that need `CONCURRENTLY` or other non-transactional operations, add as the first line:

```sql
-- no-transaction
CREATE INDEX CONCURRENTLY idx_media_items_title ON media_items(title);
```

### Advisory Locking

sqlx 0.9 uses PostgreSQL advisory locks during migration to prevent concurrent migration execution. If two server instances start simultaneously, only one runs migrations — the other waits.

### Fail-Fast Behavior

If a migration fails:
1. The transaction rolls back
2. The server logs the error with the migration filename and SQL error detail
3. The server exits with a non-zero status code
4. No subsequent migrations run
5. The administrator must fix the issue and restart

This matches Classifarr's fail-fast philosophy. It is better to have a server that won't start than a server running with a partially-applied schema.

## Comparison: Our Strategy vs Classifarr

| Aspect | Classifarr | Our Server |
|---|---|---|
| **Runner** | Custom (`migrations.mjs`) | sqlx (embedded) |
| **Naming** | `YYYYMMDD_HHMMSS_name.sql` | Same (sqlx default) |
| **Tracking table** | `schema_migrations` | `_sqlx_migrations` (customizable) |
| **Checksums** | Custom implementation | Built-in (SHA-256) |
| **Ordering** | Legacy numeric first, then timestamp | Timestamp only |
| **Idempotency** | Required (enforced by convention) | Required (enforced by convention) |
| **Down migrations** | Not supported | Development only (`sqlx migrate revert`) |
| **Fail-fast** | Yes | Yes |
| **Bootstrap** | `current.sql` snapshot | All migrations replay (future: optional snapshot) |
| **CI validation** | `npm run migration:check` | `cargo sqlx prepare` + build check |
| **Language** | ESM (Node.js) | Rust |
| **Embedded** | No (filesystem) | Yes (compiled into binary) |

## Research Sources

- sqlx 0.9.0 CHANGELOG — Released May 6, 2026: https://docs.rs/crate/sqlx/latest/source/CHANGELOG.md
- refinery 0.9.0 — Rust SQL Migration Toolkit: https://github.com/rust-db/refinery
- Rust ORMs in 2026 (Diesel vs SQLx vs SeaORM vs Rusqlite): https://aarambhdevhub.medium.com/rust-orms-in-2026-diesel-vs-sqlx-vs-seaorm-vs-rusqlite-which-one-should-you-actually-use-706d0fe912f3
- Flyway timestamp-based naming best practices: https://dev.to/deployhq/master-your-database-migrations-with-flyway-a-comprehensive-guide-for-all-projects-1een
- Reddit — Timestamp vs sequential migration naming: https://www.reddit.com/r/AskProgramming/comments/1dum62c/what_is_a_good_choice_for_the_version_string/
- OneUptime — Building a Database Migration System in Node.js (January 2026): https://oneuptime.com/blog/post/2026-01-22-nodejs-database-migration-system/view
- Classifarr migration directory and conventions: https://github.com/cloudbyday90/Classifarr/tree/main/database/migrations
- Prisma Migrate timestamp naming: https://www.prisma.io/docs/orm/prisma-migrate/getting-started
