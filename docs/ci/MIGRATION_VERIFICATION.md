# Migration Verification

## Overview

Duskcue validates SQL migrations against a disposable PostgreSQL 18 database before they are trusted for merge or release. Static review and Rust compilation are not enough for this project because migrations use PostgreSQL-specific DDL, extensions, generated columns, partitions, triggers, and seed data.

The local implementation is:

```powershell
.\scripts\verify-migrations.ps1
```

Optional full server test pass against the same disposable database:

```powershell
.\scripts\verify-migrations.ps1 -RunTests
```

## Official Research Findings (June 2026)

- Docker Compose `down` removes containers and networks by default, but volumes are preserved unless `-v` is supplied. Migration verification must use `docker compose down -v --remove-orphans` for cleanup.
- Docker Compose supports service health checks, and containerized PostgreSQL should be gated by `pg_isready` instead of fixed sleeps.
- The official PostgreSQL Docker image initializes a database only when `PGDATA` is empty, supports `POSTGRES_DB`, `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_INITDB_ARGS`, and applies init-time arguments such as `--data-checksums`.
- PostgreSQL's `pg_isready` is the official readiness probe for accepting connections.
- SQLx migrations should run through the same embedded migration path used by the application startup code, so local verification does not depend on a globally installed SQLx CLI.

## Recommendation Matrix

| Option | Pros | Cons | Decision |
|---|---|---|---|
| Long-lived local PostgreSQL | Fast after initial setup; useful for manual development | Drift-prone; hard to prove fresh-install behavior; hidden state can mask broken seeds | Not used for migration verification |
| Disposable Docker Compose PostgreSQL | Reproducible; works on Docker Desktop; easy cleanup; mirrors CI shape | Requires Docker Desktop; slower than static checks | **Selected** |
| Testcontainers from Rust tests | Good for integration tests; scoped to Rust test process | Adds test dependency and runtime complexity; less convenient for manual SQLx CLI checks | Deferred |
| Custom migration runner | Full control | Duplicates SQLx; higher maintenance burden | Rejected |

## Security Decisions

- The PostgreSQL port binds to `127.0.0.1` only.
- The verifier generates a random PostgreSQL password for each run and passes it through process environment only.
- No database credentials are written to tracked files.
- The Compose project name is unique per run, limiting collisions with developer services.
- Cleanup is targeted by Compose project name and volume ownership. The script does not run broad `docker system prune` or `docker volume prune`.
- `POSTGRES_INITDB_ARGS=--data-checksums` matches Duskcue's recovery-safety posture.
- Verification databases are disposable and must not contain production data.

## Implementation

Files:

| File | Purpose |
|---|---|
| `docker/compose.migrations.yml` | Disposable PostgreSQL 18 service with loopback-only port, health check, data checksums, and named volume |
| `scripts/verify-migrations.ps1` | Windows/Docker Desktop-friendly orchestration script with preflight checks, readiness polling, embedded migration execution, optional tests, and cleanup |
| `server/src/bin/verify_migrations.rs` | Small Rust utility that connects to `DUSKCUE_DATABASE_URL` and runs `sqlx::migrate!().run(&pool)` |

Default behavior:

1. Generate a unique Compose project name.
2. Generate a one-time PostgreSQL password.
3. Start PostgreSQL 18 Alpine via Docker Compose.
4. Wait for `pg_isready`.
5. Run `cargo run -p duskcue --bin verify_migrations` from the repo root.
6. Optionally run `cargo test -p duskcue` from the repo root.
7. Always clean up with `docker compose down -v --remove-orphans` unless `-KeepAlive` is supplied for debugging.

## Local Commands

Fresh migration verification:

```powershell
.\scripts\verify-migrations.ps1
```

Use a different host port:

```powershell
.\scripts\verify-migrations.ps1 -Port 55433
```

Run migrations and tests:

```powershell
.\scripts\verify-migrations.ps1 -RunTests
```

Keep the disposable database alive for inspection:

```powershell
.\scripts\verify-migrations.ps1 -KeepAlive
```

When `-KeepAlive` is used, the script prints the project name and `DATABASE_URL`. The operator must clean up manually:

```powershell
docker compose -f docker\compose.migrations.yml -p <project-name> down -v --remove-orphans
```

## CI Integration

Fast PR lane:

- Run `.\scripts\verify-migrations.ps1` after Rust formatting/linting and before expensive browser/mobile jobs.
- Do not use `-KeepAlive`.
- Upload migration logs only to trusted artifacts if needed.

Mainline lane:

- Run `.\scripts\verify-migrations.ps1 -RunTests` when runtime budget allows.
- Preserve the successful migration evidence in the mainline validation record.

Scheduled/release lanes:

- Use migration verification as the precondition before backup restore drills and release publication.
- Pair with `cargo sqlx prepare --check --workspace -- --all-targets --all-features` once offline query metadata is part of the merge contract.

## Failure Handling

If migration verification fails:

1. The script exits non-zero.
2. The disposable PostgreSQL instance is removed unless `-KeepAlive` was used.
3. Review SQLx output for the failed migration filename and PostgreSQL error.
4. Fix by adding or editing only unapplied local migrations. Do not edit migrations that have already shipped to any shared environment.

## Sources

- Docker Compose `down`: https://docs.docker.com/reference/cli/docker/compose/down/
- Docker Compose service healthcheck: https://docs.docker.com/reference/compose-file/services/#healthcheck
- Docker Compose volumes: https://docs.docker.com/reference/compose-file/volumes/
- PostgreSQL Docker Official Image: https://hub.docker.com/_/postgres
- PostgreSQL `pg_isready`: https://www.postgresql.org/docs/current/app-pg-isready.html
- SQLx migration macro: https://docs.rs/sqlx/latest/sqlx/macro.migrate.html
