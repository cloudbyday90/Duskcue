# Database Defensibility & Recovery Design

## Overview

This document defines the database backup, recovery, and integrity strategy. It addresses the single biggest complaint with Plex — SQLite database corruption with no recovery path — by building defense in depth using PostgreSQL 18's native capabilities plus WAL-G for continuous archiving.

## The Problem We're Solving

Plex uses SQLite. The recurring failure modes:

1. **Silent corruption** — `database disk image is malformed` with no warning
2. **No point-in-time recovery** — Last manual backup only. All data between backup and corruption is lost
3. **Zero observability** — No health monitoring. Users discover corruption only on failure
4. **Fragile repair** — Requires custom SQLite binary, dump/reload, often results in data loss
5. **No off-site story** — Backups stored alongside the database. Hardware failure takes both

Our architecture solves all five by design.

## Architecture: Three Layers of Defense

```
┌─────────────────────────────────────────────────────────────────┐
│  Layer 1: Prevention                                            │
│                                                                  │
│  - PostgreSQL 18 (ACID, WAL, crash recovery)                    │
│  - fsync=on (required for crash-safe durability)                │
│  - synchronous_commit=on (preserve acknowledged commits)        │
│  - data_checksums=on (page-level corruption detection)          │
│  - full_page_writes=on (torn-page protection)                  │
│  - Connection pool limits (prevent resource exhaustion)         │
│  - Statement timeouts (kill runaway queries)                    │
│  - wal_level=replica (enables all recovery options)             │
├─────────────────────────────────────────────────────────────────┤
│  Layer 2: Detection                                              │
│                                                                  │
│  - pg_stat_database metrics (dead tuples, cache hit, checksums) │
│  - WAL archival health monitoring (alert if pg_wal/ grows)      │
│  - Backup freshness alerts (alert if last backup is stale)      │
│  - pg_verifybackup on native base backups                        │
│  - WAL-G wal-verify for archive continuity + timeline health     │
│  - Scheduled pg_amcheck corruption checks                        │
│  - All via existing scheduled_tasks + notifications tables      │
├─────────────────────────────────────────────────────────────────┤
│  Layer 3: Recovery                                               │
│                                                                  │
│  Tier 1: WAL-G continuous archiving + PITR                      │
│    - RPO: seconds to minutes                                    │
│    - RTO: minutes                                               │
│    - Daily base backups + continuous WAL archival               │
│                                                                  │
│  Tier 2: pg_dump logical backups                                │
│    - RPO: last backup time                                      │
│    - RTO: minutes to hours                                      │
│    - Cross-version portable, table-level selective restore      │
│                                                                  │
│  Tier 3: Built-in monitoring + alerting                         │
│    - Scheduled integrity checks                                 │
│    - Backup verification after each backup                      │
│    - WAL archival health monitoring                             │
│    - All surfaced via notification system                       │
└─────────────────────────────────────────────────────────────────┘
```

## Tool Selection

### Research Summary (May 2026)

**pgBackRest is dead.** The maintainer announced obsolescence in April 2026 after 13 years. GitHub repo archived. Do not use for new deployments.

| Tool | Maintainer | License | Interface | PITR | Incremental | Encryption | Status |
|---|---|---|---|---|---|---|---|
| **WAL-G** | Community | Apache 2.0 | CLI | Yes | Delta | AES | Active |
| Barman | EnterpriseDB | GPL 3 | CLI | Yes | File-level | GPG | Active |
| pg_probackup | Postgres Pro | Own license | CLI | Yes | Block-level | Optional | Active |
| pgmoneta | Community | BSD-3 | CLI daemon | Via WAL | Yes | AES | Active |
| Databasus | Community | Apache 2.0 | Web UI | WAL-streaming | Yes | AES-256-GCM | Active |
| pgBackRest | Unmaintained | MIT | CLI | Yes | Block-level | Yes | **Dead** |

### Decision: WAL-G

| Factor | WAL-G | Why |
|---|---|---|
| **Single-node fit** | Single binary, no separate server | We're a self-hosted single-node app. Barman's multi-server model is overkill |
| **License** | Apache 2.0 | No GPL concerns. Can bundle freely |
| **Deployment** | Single Go binary | Easy to bundle in our Docker image. No Python runtime |
| **PITR** | Yes | Continuous WAL archival + base backups = point-in-time recovery |
| **Encryption** | AES built-in | Encrypt before leaving the machine |
| **Cloud storage** | S3, GCS, Azure, S3-compatible | Users can push off-site to any S3-compatible endpoint |
| **Community standing** | Recommended pgBackRest replacement | Ex-Citus/Microsoft lineage, active community, battle-tested |
| **Local storage** | Yes | Works with local paths for users without cloud storage |

Users who prefer a Web UI can optionally run Databasus alongside our server — it's compatible with the same PostgreSQL instance.

## Official Best-Practice Adjustments (May 2026)

The original design direction remains valid, but the official PostgreSQL, Docker, and CISA guidance sharpens several operational rules:

1. `fsync=on` remains mandatory for any durable deployment. PostgreSQL docs explicitly warn that disabling it can cause unrecoverable corruption after an OS or power crash.
2. `synchronous_commit=on` remains the default for normal application traffic. Per-transaction relaxation is acceptable for noncritical maintenance work, not as a system-wide default.
3. Abrupt shutdown must be assumed. Docker pre-stop hooks are not a crash-safety mechanism because they do not run on sudden kills.
4. Backup success is not enough. Verified restoreability requires a layered approach: base backup verification, WAL archive continuity checks, and periodic restore drills.
5. Live corruption checks belong in the design. PostgreSQL now ships `pg_amcheck`; it should be part of the scheduled integrity posture.

## Native PostgreSQL Verification Layer

In addition to WAL-G, we use PostgreSQL's native verification tools where they fit best.

### Base backup verification: `pg_verifybackup`

When the system produces a native `pg_basebackup` artifact for verification or operator export, it must keep the default backup manifest enabled and run `pg_verifybackup` against the resulting backup.

What this adds:

- Confirms expected files are present
- Confirms file sizes and checksums match the manifest
- Confirms required WAL can be parsed for recovery

### Live corruption checks: `pg_amcheck`

`pg_amcheck` is the scheduled live-cluster corruption check. It complements `data_checksums`:

- `data_checksums` catches on-read page checksum failures
- `pg_amcheck` proactively scans relations and indexes for structural corruption

Recommended default posture:

- Weekly full-cluster `pg_amcheck`
- Daily targeted checks on the most write-heavy relations when the database is large enough to justify it
- Avoid stronger blocking options during peak usage windows

### Archive continuity: `wal-g wal-verify`

WAL-G's `wal-verify` is used to ensure WAL storage continuity and timeline sanity. This protects against a backup set that exists but cannot be replayed end-to-end.

## Tier 1: WAL-G Continuous Archiving + PITR

### How It Works

WAL-G operates in two modes simultaneously:

1. **Base backups** — Full physical snapshots of the database cluster, taken daily
2. **WAL archival** — Every 16MB WAL segment is archived as generated, enabling recovery to any point in time

### PostgreSQL Configuration

```ini
# postgresql.conf — required for PITR
fsync = on
synchronous_commit = on
wal_level = replica
archive_mode = on
archive_command = 'wal-g wal-push %p'
archive_timeout = 60
data_checksums = on
full_page_writes = on
```

- `archive_timeout = 60` — Forces a WAL segment switch every 60 seconds. Ensures no transaction is more than 60 seconds from being archived
- `data_checksums = on` — Must be set at `initdb` time. ~5% write overhead, detects silent page corruption
- `fsync = on` — Required for durable crash recovery; PostgreSQL docs explicitly warn that turning it off can cause unrecoverable corruption after power/OS failure
- `synchronous_commit = on` — Keeps acknowledged commits durable by default; do not disable globally

### Base Backup Schedule

| Schedule | Type | Retention | Purpose |
|---|---|---|---|
| Daily at 03:00 | Full | 7 days | Primary recovery base |
| Weekly (Sunday) | Full | 4 weeks | Weekly recovery points |
| Monthly (1st) | Full | 12 months | Long-term recovery points |

WAL-G's `wal-g delete retain 7 --full` policy automatically prunes old backups while maintaining the retention window.

### Storage

WAL-G supports two storage backends, configured in `server_config.backup`:

| Backend | Path | Use Case |
|---|---|---|
| **Local** | `/data/backups/wal-g` | Default. NAS storage, local disks |
| **S3-compatible** | `s3://bucket/backups` | Off-site. Backblaze B2, MinIO, AWS S3, Cloudflare R2 |

Both can be used simultaneously (local + remote push via a post-backup hook).

### Recovery Procedures

#### Point-in-Time Recovery (PITR)

```
1. Stop the server
2. Restore base backup: wal-g backup-fetch /var/lib/postgresql/data LATEST
3. Configure recovery target in postgresql.conf:
   restore_command = 'wal-g wal-fetch %f %p'
   recovery_target_time = '2026-05-30 14:30:00'
4. Create recovery.signal in data directory
5. Start PostgreSQL — replays WAL up to target time
6. Verify data integrity
7. Remove recovery.signal
```

#### Full Recovery (to latest)

```
1. Stop the server
2. Restore base backup: wal-g backup-fetch /var/lib/postgresql/data LATEST
3. Configure: restore_command = 'wal-g wal-fetch %f %p'
4. Create recovery.signal
5. Start PostgreSQL — replays all WAL to current state
```

#### Selective Table Recovery

Use Tier 2 (pg_dump) for table-level restore. WAL-G operates at the cluster level.

## Tier 2: pg_dump Logical Backups

### Why Alongside WAL-G

| WAL-G | pg_dump |
|---|---|
| Physical — cluster-level only | Logical — table/database-level selective restore |
| Same PostgreSQL major version required | Cross-version portable |
| Binary format | Custom format (`-F c`) with `pg_restore --list` |
| Fast full restore | Slow but flexible |

### Schedule

| Schedule | Type | Retention | Command |
|---|---|---|---|
| Daily at 04:00 | Full database | 30 daily | `pg_dump -F c -f /data/backups/dump/db_$(date +%Y%m%d).dump` |
| Monthly (1st) | Full database | 12 monthly | Same command, rotated monthly |

### Storage

Logical backups are stored in `/data/backups/dump/` (local) or pushed to S3-compatible storage via post-backup scripts. Compression is built-in with custom format.

### Selective Restore

```bash
# List contents of a backup
pg_restore --list /data/backups/dump/db_20260530.dump

# Restore specific table
pg_restore -d dbname -t users /data/backups/dump/db_20260530.dump

# Restore specific schema
pg_restore -d dbname -n public /data/backings/dump/db_20260530.dump
```

## Tier 3: Built-in Monitoring & Alerting

All monitoring uses our existing `scheduled_tasks` and `notifications` infrastructure. No external monitoring stack required.

### Scheduled Integrity Tasks

| Task Type | Schedule | What It Does |
|---|---|---|
| `backup_database` | Daily 03:00 | Runs WAL-G base backup + pg_dump |
| `backup_verification` | Daily 04:30 | Runs `pg_verifybackup` on native verification backup and `wal-g wal-verify` on archive storage |
| `database_integrity_check` | Weekly | Checks pg_stat_database stats, WAL archival health, and runs `pg_amcheck` |
| `backup_retention_cleanup` | Weekly | Prunes old backups per retention policy |

### Notification Types

| Type | Category | Trigger |
|---|---|---|
| `backup_completed` | system | Backup finished successfully |
| `backup_failed` | system | Backup failed |
| `backup_verification_failed` | security | Backup integrity check failed |
| `wal_archive_lag` | security | WAL archival falling behind |
| `database_checksum_failure` | security | Data checksum mismatch detected |

### Monitored Metrics

| Metric | Source | Alert Threshold |
|---|---|---|
| WAL archival lag | `pg_stat_archiver` | > 10 unarchived segments |
| WAL archival failures | `pg_stat_archiver.failed_count` | Any increase since last check |
| Last backup age | `scheduled_task_runs` | > 25 hours since last success |
| Checksum failures | `pg_stat_database` | Any > 0 |
| Cache hit ratio | `pg_stat_database` | < 95% |
| Dead tuple ratio | `pg_stat_user_tables` | > 20% on any table |
| `pg_wal/` directory size | Filesystem | > 2GB |

## server_config.backup JSONB Group

Updated definition for the `backup` JSONB column in `server_config`:

```json
{
    "wal_g_enabled": true,
    "wal_g_storage_type": "local",
    "wal_g_storage_path": "/data/backups/wal-g",
    "wal_g_s3_endpoint": "",
    "wal_g_s3_bucket": "",
    "wal_g_s3_prefix": "backups",
    "wal_g_s3_region": "",
    "wal_g_encryption_key_id": "",
    "wal_g_retention_full": 7,
    "wal_g_retention_weekly": 4,
    "wal_g_retention_monthly": 12,
    "pg_dump_enabled": true,
    "pg_dump_storage_path": "/data/backups/dump",
    "pg_dump_retention_daily": 30,
    "pg_dump_retention_monthly": 12,
    "archive_timeout_seconds": 60,
    "data_checksums": true,
    "verification_enabled": true
}
```

### Rust Struct Mapping

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub wal_g_enabled: bool,
    pub wal_g_storage_type: WalGStorageType,
    pub wal_g_storage_path: String,
    pub wal_g_s3_endpoint: String,
    pub wal_g_s3_bucket: String,
    pub wal_g_s3_prefix: String,
    pub wal_g_s3_region: String,
    pub wal_g_encryption_enabled: bool,
    pub wal_g_encryption_key_id: String,
    pub wal_g_encryption_auto_s3: bool,
    pub wal_g_retention_full: u32,
    pub wal_g_retention_weekly: u32,
    pub wal_g_retention_monthly: u32,
    pub pg_dump_enabled: bool,
    pub pg_dump_storage_path: String,
    pub pg_dump_retention_daily: u32,
    pub pg_dump_retention_monthly: u32,
    pub archive_timeout_seconds: u32,
    pub data_checksums: bool,
    pub verification_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WalGStorageType {
    Local,
    S3,
}
```

### Phase 13a Task 4 Implementation Notes

The backup domain is implemented as a read-only administrative status surface under `/api/v1/backups/*`:

| Endpoint | Purpose |
|---|---|
| `GET /api/v1/backups/status` | Returns typed `server_config.backup` settings, PostgreSQL recovery-safety settings, `pg_stat_archiver` status, backup scheduled tasks, recent backup runs, and a computed readiness state |
| `GET /api/v1/backups/tasks` | Lists backup-related rows from `scheduled_tasks` for `backup_database`, `backup_verification`, `database_integrity_check`, and `backup_retention_cleanup` |
| `GET /api/v1/backups/runs?limit=20` | Lists recent `scheduled_task_runs` for backup-related task types, capped at 100 |

Task 4 intentionally did not execute WAL-G, `pg_dump`, verification commands, or retention cleanup. That boundary was the domain five-file pattern, route wiring, typed runtime backup config, admin-only visibility, and readiness diagnostics.

### Phase 13a Task 5 Implementation Notes

Backup coordination is implemented through a reusable shared service plus admin endpoints:

| Endpoint | Purpose |
|---|---|
| `POST /api/v1/backups/wal-g/check` | Runs `wal-g --version` and `wal-g backup-list --json` using the configured WAL-G storage environment, returning command status and backup count |
| `POST /api/v1/backups/pg-dump` | Runs a manual logical backup with `pg_dump --format=custom --file <path> <database_url>` into `server_config.backup.pg_dump_storage_path`; verifies by default with `pg_restore --list` |
| `POST /api/v1/backups/verify` | Runs `wal-g wal-verify integrity` and/or `pg_restore --list` for the latest or specified pg_dump file |

Implementation decisions:

- The command coordinator lives in `server/src/services/backup.rs`, not only in the backup domain, so Phase 13a Task 6 can reuse the exact same WAL-G and pg_dump execution path from the scheduled worker.
- Commands are spawned directly with `tokio::process::Command`; no shell string is constructed. User-provided pg_dump labels are reduced to ASCII alphanumeric, `-`, and `_`, and verification paths must canonicalize under the configured pg_dump storage directory.
- A process-local async mutex prevents concurrent manual backup/verification operations in the single-instance runtime. Conflicts map to `SYS_007` (`Backup already in progress`).
- WAL-G environment variables are derived from `server_config.backup`: `WALG_FILE_PREFIX` for local storage, `WALG_S3_PREFIX` plus optional `AWS_ENDPOINT`/`AWS_REGION` for S3-compatible storage, and `WALG_LIBSODIUM_KEY` from the bootstrap encryption key when WAL-G encryption is active.
- API responses include bounded command stdout/stderr for operator diagnosis, capped at 4096 bytes, and never expose the database URL.
- Task 5 does not register scheduled backup executors or retention cleanup. `backup_database`, `backup_verification`, and retention execution remain Phase 13a Task 6.

### Phase 13a Task 6 Implementation Notes

Scheduled backup execution is implemented through `server/src/workers/backup_runner.rs` and the shared scheduler:

| Scheduled task | Executor | Behavior |
|---|---|---|
| `backup_database` | `run_backup_database` | Runs WAL-G `backup-push` for the configured PostgreSQL data directory when `wal_g_enabled` is true, adding `--verify` when `data_checksums` is true, then runs pg_dump custom-format backup when `pg_dump_enabled` is true |
| `backup_verification` | `run_backup_verification` | Skips when `verification_enabled` is false; otherwise verifies enabled tiers with `wal-g wal-verify integrity` and `pg_restore --list` |
| `backup_retention_cleanup` | `run_backup_retention_cleanup` | Runs WAL-G full-backup retention and prunes local generated `.dump` files by daily/monthly windows |

Implementation decisions:

- Backup workers use the scheduler's fallible executor path, so failed backup commands mark the scheduled run as `failure` and participate in the existing retry/auto-disable lifecycle.
- The scheduled worker does not build shell commands. It reuses `services::backup`, which launches WAL-G, `pg_dump`, and `pg_restore` through `tokio::process::Command`.
- WAL-G physical backup uses `PGDATA` when set; otherwise it uses `{data_dir}/postgres`, matching the embedded PostgreSQL layout from `DOCKER_DEPLOYMENT.md`. Missing PGDATA is treated as invalid backup configuration.
- `backup_database` runs both enabled backup tiers. This preserves the design split: WAL-G for physical/PITR recovery and pg_dump custom format for selective/table-level restore.
- `backup_verification` chooses verification targets from runtime config by default (`wal_g_enabled`, `pg_dump_enabled`) and can be narrowed by scheduled-task config keys `verify_wal_g` and `verify_pg_dump`.
- WAL-G retention uses `wal-g delete retain <wal_g_retention_full> --full --confirm`. Local pg_dump retention keeps all generated dumps inside the daily window, keeps the newest generated dump per month inside the monthly window, and leaves unknown `.dump` filenames untouched.
- The migration `20260627010000_seed_backup_scheduled_tasks.sql` seeds `backup_verification` and `backup_retention_cleanup`, normalizes `backup_database` to daily 03:00, and ensures backup tasks have `next_run_at` values.
- Backup workers persist structured command results and cleanup counts into `scheduled_task_runs.stats`; the scheduler preserves those stats when marking the run complete.

## 3-2-1 Storage Strategy

The recommended setup for production deployments:

| Copy | Location | Content |
|---|---|---|
| **Copy 1** | Running database | Production data |
| **Copy 2** | Local NAS storage | WAL-G base backups + WAL + pg_dump files |
| **Copy 3** | S3-compatible off-site | WAL-G push + pg_dump push (optional but recommended) |

For users running on Synology NAS: Copy 1 and Copy 2 are on the same machine (different volumes recommended). Copy 3 goes to Backblaze B2, Cloudflare R2, or similar.

### Security hardening for backup copies

Following current CISA guidance, the recommended posture is:

- At least one copy offline or operationally isolated from the live server
- Encryption for all off-machine backup copies
- Immutable or write-once retention where the storage backend supports it
- Routine restore testing, not just backup job success monitoring

## Crash Recovery Expectations

### What Happens After an Unclean Shutdown

PostgreSQL guarantees **zero data loss for committed transactions** after any crash (power loss, SIGKILL, kernel panic, Docker force-kill). This is a fundamental property of PostgreSQL's WAL-based architecture — not a feature that can be misconfigured away (assuming `fsync=on`, which is the default).

PostgreSQL's own shutdown model matters here:

- **Fast shutdown** is the normal administrative stop and should be used for regular upgrades and container stops
- **Immediate shutdown** is an emergency stop and intentionally causes crash recovery on the next startup
- A Docker or host-level abrupt kill should therefore be treated as equivalent to emergency interruption, not as an exotic edge case

**Recovery flow (automatic, no admin action required):**

```
Unclean shutdown (no checkpoint)
  → Docker restarts container (unless-stopped policy)
  → Entrypoint starts PostgreSQL
  → PG startup process reads pg_control
  → Finds last checkpoint LSN
  → Replays WAL forward from that checkpoint
  → Performs "end-of-recovery" checkpoint
  → pg_isready reports success
  → Server connects and begins normal operation
```

**What the admin sees**: The server takes slightly longer to start (WAL replay time depends on `max_wal_size` and checkpoint interval). The startup log shows:
```
INFO  duskcue::startup: Connecting to PostgreSQL...
INFO  duskcue::startup: PostgreSQL ready (crash recovery: 2.3s WAL replay)
```

**What survives**: All committed transactions — watch history, resume positions, user accounts, server configuration, migration state. Every `COMMIT` that returned success to the client is durable.

**What does NOT survive**: 
- In-flight (uncommitted) transactions — by definition they were not committed
- Active transcode sessions — ephemeral (tmpfs), cleaned up on restart
- In-flight HTTP requests — client retries, idempotent

### When Backups Are Needed (Not for Crashes)

WAL-G and pg_dump protect against scenarios PostgreSQL's crash recovery **cannot** handle:

| Scenario | Crash Recovery | Backup Needed |
|---|---|---|
| Power loss | Auto-recovered | No |
| SIGKILL / OOM kill | Auto-recovered | No |
| Docker force-kill | Auto-recovered | No |
| Accidental `DROP TABLE` | Not recoverable | **PITR to before the mistake** |
| Storage corruption (silent) | Checksums detect; may need restore | **WAL-G restore** |
| Hardware failure (disk dead) | No data to recover from | **Restore from backup** |
| Bad migration | DDL is transactional but may need undo | **PITR to before migration** |
| Ransomware / malicious deletion | Not recoverable | **Off-site backup restore** |

### Recovery Objectives Revisited

| Scenario | RPO | RTO | Method |
|---|---|---|---|
| Accidental row/table deletion | Seconds | Minutes | PITR to moments before the mistake |
| Database corruption | Seconds | Minutes | PITR to last known-good state |
| Hardware failure | Minutes (WAL lag) | Minutes–hours | Restore latest WAL-G backup to new hardware |
| Site-level disaster | Minutes | Hours | Restore from S3 off-site copy |
| Need specific table | Last pg_dump | Minutes | pg_restore selective |
| Bad migration | Seconds | Minutes | PITR to before migration started |
| Dropped entire database | Seconds | Minutes | WAL-G full restore + PITR |

## Backup Encryption

### Why Encryption Matters

Backups contain your entire database — user accounts, watch history, session tokens, server configuration. If someone gains access to your backup storage (NAS drive, S3 bucket, USB drive), they have everything. Encryption ensures that stolen backup files are useless without the encryption key.

### How It Works

WAL-G has built-in AES-256-GCM encryption. When enabled, every backup file and WAL segment is encrypted before it leaves the server. The encrypted files are unreadable without the encryption key, even if someone copies them directly from your storage.

### Encryption Key

The encryption key is a 256-bit (32-byte) random value generated on first setup. It is stored in the **bootstrap configuration file** (`config.toml`), not in the database — the database itself is inside the backup, so storing the key there would create a lockout loop.

Recommended secure posture:

- Prefer a separate file-backed secret or mounted bootstrap config over plain environment variables
- Keep the key outside the primary `/data/postgres` database volume when possible
- Restrict permissions on the bootstrap config to the runtime user only
- Never expose the key through admin APIs or logs

```toml
# config.toml — backup encryption key
[backup]
encryption_key = "hex-encoded-256-bit-key"
```

The key is generated automatically during first-run setup and written to the bootstrap config. The admin can regenerate it at any time from the admin UI.

### When Encryption Is Enabled

| Storage Type | Default | Why |
|---|---|---|
| **Local** (`/data/backups/`) | Optional | Backups on the same machine as the server; physical access = root access anyway |
| **S3 / Cloud** | **On by default** | Backups leave your network; encryption protects against bucket misconfiguration, leaked credentials, and unauthorized access |

The admin can toggle encryption for local backups via `server_config.backup.wal_g_encryption_enabled`.

### Key Rotation

Encryption keys can be rotated without downtime:

1. Generate a new key via admin UI
2. WAL-G takes the next backup using the new key
3. Old backups remain readable with the old key (both keys accepted during rotation)
4. After all old backups age out of retention, the old key can be discarded

Key rotation is manual — the admin clicks "Rotate Encryption Key" in the admin UI, confirms the action, and the server handles the rest. The admin is reminded to keep the old key until old backups expire.

### Losing the Encryption Key

If the encryption key is lost, encrypted backups cannot be recovered. There is no backdoor, no key escrow, no recovery mechanism. This is by design — a backdoor that anyone can use is not encryption.

**Key protection recommendations shown in the admin UI:**
- Keep `config.toml` on a separate volume from `/data` if possible
- If using Docker, mount `config.toml` from a separate bind mount
- Copy the encryption key to a safe location (password manager, offline USB)
- S3 users: the key is the only thing between your backups and the internet

### Configuration

The `server_config.backup` JSONB is extended with encryption fields:

```json
{
    "wal_g_encryption_enabled": false,
    "wal_g_encryption_key_id": "",
    "wal_g_encryption_auto_s3": true
}
```

| Field | Type | Default | Description |
|---|---|---|---|
| `wal_g_encryption_enabled` | bool | `false` | Master toggle for local backup encryption |
| `wal_g_encryption_key_id` | String | `""` | Identifier for the current encryption key (for rotation tracking) |
| `wal_g_encryption_auto_s3` | bool | `true` | Automatically enable encryption when S3 storage is configured |

The actual key value lives in bootstrap config (`config.toml`), not in this JSONB — this field only tracks whether encryption is active and which key version is current.

### WAL-G Encryption Environment Variables

When encryption is enabled, the server sets these environment variables for WAL-G:

```bash
WALG_LIBSODIUM_KEY="hex-encoded-key"    # AES-256-GCM via libsodium
```

WAL-G uses libsodium's AES-256-GCM for authenticated encryption. Each backup segment is independently encrypted, so corruption of one segment does not affect others.

---

## What We Do NOT Do

- **Streaming replication / hot standby** — Overkill for a single-family Duskcue. Doubles hardware requirements. If demand exists, this can be added later
- **Custom backup UI** — WAL-G is CLI-only. Users who want a Web UI for backup management can run Databasus independently against the same PostgreSQL instance
- **Snapshot-based backups (LVM/ZFS)** — These are filesystem-level and require specific storage setup. WAL-G provides equivalent functionality at the database level

## Research Sources

### Official Sources

- PostgreSQL 18 — Continuous Archiving and PITR: https://www.postgresql.org/docs/current/continuous-archiving.html
- PostgreSQL 18 — Write Ahead Log settings: https://www.postgresql.org/docs/current/runtime-config-wal.html
- PostgreSQL 18 — `pg_basebackup`: https://www.postgresql.org/docs/current/app-pgbasebackup.html
- PostgreSQL 18 — `pg_verifybackup`: https://www.postgresql.org/docs/current/app-pgverifybackup.html
- PostgreSQL 18 — `pg_amcheck`: https://www.postgresql.org/docs/current/app-pgamcheck.html
- PostgreSQL 18 — Shutting Down the Server: https://www.postgresql.org/docs/current/server-shutdown.html
- Docker — `container stop`: https://docs.docker.com/reference/cli/docker/container/stop/
- Docker — Compose lifecycle hooks: https://docs.docker.com/compose/how-tos/lifecycle/
- Docker — Compose FAQ (signal handling): https://docs.docker.com/compose/support-and-feedback/faq/
- CISA — Back Up Government Data: https://www.cisa.gov/audiences/state-local-tribal-and-territorial-government/secure-us-sltt/back-government-data
- CISA — Medusa ransomware advisory: https://www.cisa.gov/news-events/cybersecurity-advisories/aa25-071a
- WAL-G for PostgreSQL: https://wal-g.readthedocs.io/PostgreSQL

### Secondary / ecosystem sources

- PostgreSQL 18 Official Docs — Continuous Archiving and PITR: https://www.postgresql.org/docs/current/continuous-archiving.html
- Bytebase — Top Open-Source Postgres Backup Solutions in 2026 (April 2026)
- Kunal Ganglani — pgBackRest Is No Longer Maintained: 3 PostgreSQL Backup Tools Compared for Production (April 2026)
- Tomasz Gintowt — If not pgBackRest, then what? Rethinking PostgreSQL Backups in 2026 (April 2026)
- Medium — PostgreSQL Backup and Restore Complete Guide (January 2026)
- Reddit r/Hosting — Best Practices/Tools with Self-Host Postgres (January 2026)
- DEV Community — Top 5 PostgreSQL Backup Tools in 2026 (January 2026)
- Simplyblock — Best Open Source Tools for PostgreSQL Backup and Restore (February 2026)
- Reddit r/PleX — Frequent database corruption (October 2024)
- Reddit r/PleX — DB Corruption SQLite (November 2021)
- Reddit r/PleX — Rebuilding Corrupt Plex Database File (June 2024)

## Backup Encryption Sources
- WAL-G — Encryption documentation: https://github.com/wal-g/wal-g#encryption
- Stolos.io — Backup Encryption Best Practices for PostgreSQL (March 2026): https://stolos.io/guides/backup-encryption-best-practices-postgresql
- Bytebase — How to Encrypt PostgreSQL Backups (January 2026): https://www.bytebase.com/blog/how-to-encrypt-postgresql-backups/
