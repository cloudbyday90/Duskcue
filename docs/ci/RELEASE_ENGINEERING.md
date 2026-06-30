# Release Engineering & Upgrade Safety

## Overview

This document defines how the platform is versioned, released, upgraded, and rolled back. The primary design goal is **data safety first**: an abrupt shutdown, container kill, host crash, or failed release must not turn a routine operational event into database loss.

This document complements:

- [BACKUP_RECOVERY.md](../operations/BACKUP_RECOVERY.md) — backup, PITR, and integrity strategy
- [DOCKER_DEPLOYMENT.md](../operations/DOCKER_DEPLOYMENT.md) — container shutdown behavior and embedded PostgreSQL deployment
- [MIGRATION_STRATEGY.md](../design/MIGRATION_STRATEGY.md) — schema migration rules
- [MEMORY.md](../design/MEMORY.md) — runtime shutdown orchestration
- [TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md](TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md) — which trusted-automation changes stop release work until the advanced doc set is re-reviewed
- [TRUSTED_AUTOMATION_MANUAL_VALIDATION.md](TRUSTED_AUTOMATION_MANUAL_VALIDATION.md) — when release-blocking trusted-automation changes require a dedicated manual validation step before protected publication
- [RELEASE_ARTIFACT_RETENTION.md](RELEASE_ARTIFACT_RETENTION.md) — durable release evidence, retention windows, and rollback-proof artifact rules
- [CLIENT_PACKAGING.md](CLIENT_PACKAGING.md) — desktop/mobile package smoke, signing placeholders, privacy notes, and client release-gate checks

## Goals

1. Make routine application updates safe for self-hosted operators.
2. Ensure abrupt shutdown recovery is automatic in the common case.
3. Define when rollback is binary-only and when restore/PITR is required.
4. Separate application release classes from PostgreSQL release classes.
5. Keep the default path secure for internet-exposed deployments.

## Official Research Findings (May 2026)

### PostgreSQL durability and crash recovery

- PostgreSQL official docs state that turning `fsync` off can lead to **unrecoverable data corruption** after a power failure or system crash. It is not acceptable for a durable production system.
- PostgreSQL official docs state that `full_page_writes` protects against torn pages after checkpoints and is part of the default crash-safety posture.
- PostgreSQL official docs state that `wal_level=replica` is required for continuous archiving and PITR.
- PostgreSQL official docs note that larger `max_wal_size` can increase crash-recovery time.
- PostgreSQL official docs define **fast shutdown** as the normal administrative stop and **immediate shutdown** as an emergency mode that causes recovery by WAL replay on next startup.

### PostgreSQL backup verification and corruption checks

- PostgreSQL 18 generates a backup manifest for `pg_basebackup` by default unless explicitly disabled.
- PostgreSQL provides `pg_verifybackup` to validate backup contents against the manifest and parse required WAL records.
- PostgreSQL provides `pg_amcheck` to detect corruption in live databases, with warnings that some stronger checks require heavier locks.
- PostgreSQL official versioning policy says minor upgrades only require stop, replace binaries, restart; major upgrades require dump/restore or `pg_upgrade`.

### Docker stop behavior

- Docker sends `SIGTERM` to the main container process, then `SIGKILL` after the timeout unless the process exits first.
- Docker Compose defaults to a 10 second stop timeout for Linux containers unless overridden.
- Docker docs recommend exec-form `ENTRYPOINT`/`CMD`, explicit signal handling, or a lightweight init process such as `tini`.
- Docker Compose pre-stop hooks do **not** run when the container is killed suddenly, so they are not a crash-safety mechanism.

### Backup hardening

- CISA guidance recommends offline backups, encrypted backups, immutable backups where possible, physically separate storage, and routine restoration testing.

## Versioning Model

### Decision: Semantic Versioning 2.0.0

The project uses **Semantic Versioning 2.0.0** for the server, web client bundle, desktop wrapper, and mobile/TV client artifacts.

Format:

```text
MAJOR.MINOR.PATCH
```

Examples:

- `1.0.0` — first stable release
- `1.3.0` — backward-compatible feature release
- `1.3.2` — bug/security fix release
- `2.0.0` — breaking API or operational change

### Why SemVer over CalVer

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **SemVer** | Communicates compatibility intent; maps well to API and schema expectations; ecosystem standard | Requires discipline around deprecations and breaking changes | **Selected** |
| CalVer | Conveys recency well; good for infrastructure products | Does not communicate compatibility by itself | Rejected |

### Channel tags

| Channel | Purpose | Stability |
|---|---|---|
| `alpha` | Internal and design validation builds | Unstable |
| `beta` | Feature-complete preview builds | Upgrade path not guaranteed |
| `rc` | Release candidate | Upgrade path expected to match GA |
| `stable` | General availability | Full support path |

Pre-release identifiers follow SemVer, e.g. `1.2.0-beta.1`, `1.2.0-rc.2`.

## Compatibility Contract

### API compatibility

- `/api/v1` remains stable for all releases within server major version `1`.
- Removing or behaviorally breaking a public API requires a new server major version.
- Deprecations must ship in at least one minor release before removal.

### Client compatibility

- The web client and Tauri desktop shell are released with the server and therefore always match the server version.
- Mobile and TV apps target the same API major version as the server.
- The server may reject clients from a different API major version with a forced-upgrade response.

### Desktop and mobile package smoke

Phase 16a adds a dedicated client packaging smoke workflow for the Tauri desktop shell and Flutter mobile app. These builds prove that platform package assembly still works, but debug CI artifacts are not durable release payloads. Public distribution still requires protected signing, notarization, store metadata, and privacy declarations as documented in [CLIENT_PACKAGING.md](CLIENT_PACKAGING.md).

### Database compatibility

- Application patch and minor releases may include additive or backward-compatible schema changes.
- Destructive schema changes require a server major release and a prior deprecation window.
- After a migration runs successfully, binary rollback is only allowed if the new binaries are still schema-compatible. Otherwise rollback requires PITR or restore.
- Retaining prior binaries and release evidence does not change that rollback boundary; durable evidence retention is governed separately by [RELEASE_ARTIFACT_RETENTION.md](RELEASE_ARTIFACT_RETENTION.md).

## Release Classes

### 1. Application patch release

Examples:

- Bug fix
- Security fix
- Logging or UI correction

Rules:

- No breaking API changes
- No destructive migrations
- Fully automatic upgrade allowed

### 2. Application minor release

Examples:

- New feature
- Additive API endpoints
- Additive tables or columns

Rules:

- Backward-compatible API within the same major version
- Additive migrations allowed
- Automatic upgrade allowed after preflight passes

### 3. Application major release

Examples:

- Breaking API change
- Destructive schema cleanup
- Auth or storage behavior change requiring operator action

Rules:

- Explicit upgrade notes required
- Verified backup required before upgrade proceeds
- Rollback plan must be documented before release

### 4. PostgreSQL minor release

PostgreSQL official policy: minor releases are binary replacements while the server is stopped; no `pg_upgrade` is required.

Rules:

- Stop PostgreSQL cleanly
- Replace binaries/packages
- Restart
- Read upstream release notes for required post-update actions

### 5. PostgreSQL major release

PostgreSQL official policy: major upgrades require dump/restore or `pg_upgrade`.

Rules:

- Offline, operator-visible maintenance flow
- Mandatory verified backup before upgrade
- Run `pg_upgrade --check` first
- Default path: `pg_upgrade`
- Fallback path: full restore to previous major version from verified backup

## Database-Safe Defaults

These settings are non-negotiable for production durability:

```ini
fsync = on
synchronous_commit = on
full_page_writes = on
wal_level = replica
archive_mode = on
```

### Why this stack

| Setting | Benefit | Cost | Decision |
|---|---|---|---|
| `fsync=on` | Prevents unrecoverable corruption after OS/power crash | Write latency | Required |
| `synchronous_commit=on` | Preserves acknowledged commits across crash | Commit latency | Required by default |
| `full_page_writes=on` | Protects against torn-page corruption | More WAL volume | Required |
| `wal_level=replica` | Enables PITR and archive recovery | More WAL than `minimal` | Required |
| `archive_mode=on` | Enables continuous WAL archival | Operational complexity | Required |

For noncritical one-shot maintenance work, `synchronous_commit=off` may be used selectively at the transaction level, but not as the system default.

## Abrupt Shutdown Posture

### Design principle

An abrupt shutdown is treated as a **routine fault**, not an exceptional disaster.

Expected examples:

- Host power loss
- Kernel panic
- Docker `SIGKILL` after timeout
- OOM kill
- Hypervisor reset

### Expected behavior

1. Container or service is restarted.
2. PostgreSQL performs crash recovery from WAL.
3. The application waits for PostgreSQL readiness.
4. The server resumes normal operation.

### What this guarantees

- Committed transactions remain durable.
- Uncommitted transactions are lost.
- Temporary transcode artifacts are disposable.

### What this does not guarantee

- Protection from storage corruption after the fact
- Recovery from operator error such as `DROP TABLE`
- Recovery from malicious deletion without a separate backup copy

## Preflight Gates Before Any Upgrade

An upgrade is blocked unless all of the following pass:

1. PostgreSQL is healthy and accepting connections.
2. Last verified base backup is fresh.
3. WAL archive health is green.
4. `pg_amcheck` passed within the configured integrity window.
5. Sufficient disk headroom exists for WAL growth and upgrade scratch space.
6. No failed migrations are pending manual intervention.
7. Backup encryption key material is available.

Recommended default thresholds:

| Gate | Default |
|---|---|
| Last verified base backup age | <= 24 hours |
| Last `pg_amcheck` success | <= 7 days |
| WAL archive health | No failed archival events since last successful check |
| Free disk on `/data` | >= 20% |
| Free disk on `/cache` | >= 20% |

## Verification Stack

### Decision: layered verification

| Layer | Tool | Purpose |
|---|---|---|
| Physical backup integrity | `pg_verifybackup` | Validate base backup files and required WAL readability |
| Archive continuity | `wal-g wal-verify` | Check WAL segment continuity and timeline health in storage |
| Live corruption check | `pg_amcheck` | Detect relation/index corruption in the running cluster |
| Functional restore drill | isolated restore boot | Prove that backup + WAL can actually start a cluster |

### Why not rely on only one layer

| Approach | Pros | Cons |
|---|---|---|
| Only WAL-G backup success | Fast, simple | Does not prove restoreability by itself |
| Only `pg_verifybackup` | Strong for base backup integrity | Does not replace archive continuity checks or restore drills |
| Only restore drill | Strong end-to-end signal | More time and infrastructure cost |
| **Layered stack** | Detects different failure modes | More moving parts |

**Selected:** layered stack.

## Upgrade Workflow

### Application patch/minor

```text
1. Run preflight gates
2. Graceful application drain
3. Fast PostgreSQL shutdown
4. Install new binaries/image
5. Start PostgreSQL
6. Run automatic migrations
7. Start application
8. Run health and readiness checks
9. Mark upgrade complete
```

### PostgreSQL minor

```text
1. Run preflight gates
2. Stop application and PostgreSQL cleanly
3. Replace PostgreSQL binaries/packages
4. Start PostgreSQL
5. Verify crash-free startup and readiness
6. Start application
7. Review upstream-required post-update actions
```

### PostgreSQL major

```text
1. Run preflight gates
2. Take fresh verified backup
3. Stop application and PostgreSQL cleanly
4. Run pg_upgrade --check
5. Run pg_upgrade
6. Start upgraded PostgreSQL
7. Reconfigure WAL archival / backup verification if paths changed
8. Start application
9. Run post-upgrade validation
10. Retain old cluster until validation window closes
```

## Rollback Rules

### Binary rollback is allowed when

- No schema migration ran, or
- All executed migrations are backward-compatible with the previous binary

### PITR or restore is required when

- A destructive migration ran
- A new release writes data the old version cannot read safely
- PostgreSQL major upgrade already converted the cluster

### Final rule

**Never promise in-place binary rollback after an incompatible schema or PostgreSQL major upgrade.** The supported rollback mechanism is verified restore or PITR.

## Secure Secret Handling for Recovery

Backup encryption keys and restore credentials must not be treated as normal runtime configuration.

Required posture:

- Prefer a file-backed secret or separate mounted bootstrap config over plain environment variables
- Keep bootstrap secret material outside the database volume when possible
- Never expose encryption keys in admin API responses
- Never write key material to logs

## Final Recommendation Stack

1. **Versioning:** SemVer 2.0.0 with `alpha`, `beta`, `rc`, and `stable` channels.
2. **Durability baseline:** `fsync=on`, `synchronous_commit=on`, `full_page_writes=on`, `wal_level=replica`, `archive_mode=on`.
3. **Backup strategy:** WAL-G continuous archive plus verified base backups.
4. **Verification:** `pg_verifybackup` + `wal-g wal-verify` + scheduled `pg_amcheck` + periodic restore drills.
5. **Shutdown model:** graceful stop first, but design assumes sudden kill can happen at any time.
6. **Upgrade model:** automatic for patch/minor app releases; offline `pg_upgrade` flow for PostgreSQL major releases.
7. **Rollback model:** binary rollback only before incompatible migrations; otherwise PITR/restore only.

## Official Sources

- PostgreSQL Versioning Policy: https://www.postgresql.org/support/versioning
- PostgreSQL Upgrading a Cluster: https://www.postgresql.org/docs/current/upgrading.html
- PostgreSQL `pg_upgrade`: https://www.postgresql.org/docs/current/pgupgrade.html
- PostgreSQL Continuous Archiving and PITR: https://www.postgresql.org/docs/current/continuous-archiving.html
- PostgreSQL Write Ahead Log settings: https://www.postgresql.org/docs/current/runtime-config-wal.html
- PostgreSQL `pg_basebackup`: https://www.postgresql.org/docs/current/app-pgbasebackup.html
- PostgreSQL `pg_verifybackup`: https://www.postgresql.org/docs/current/app-pgverifybackup.html
- PostgreSQL `pg_amcheck`: https://www.postgresql.org/docs/current/app-pgamcheck.html
- PostgreSQL Shutting Down the Server: https://www.postgresql.org/docs/current/server-shutdown.html
- Docker `container stop`: https://docs.docker.com/reference/cli/docker/container/stop/
- Docker Compose lifecycle hooks: https://docs.docker.com/compose/how-tos/lifecycle/
- Docker Compose FAQ (signal handling): https://docs.docker.com/compose/support-and-feedback/faq/
- CISA Back Up Government Data: https://www.cisa.gov/audiences/state-local-tribal-and-territorial-government/secure-us-sltt/back-government-data
- CISA Medusa ransomware advisory: https://www.cisa.gov/news-events/cybersecurity-advisories/aa25-071a
- WAL-G for PostgreSQL: https://wal-g.readthedocs.io/PostgreSQL
