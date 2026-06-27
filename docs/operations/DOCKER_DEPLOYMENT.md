# Docker Deployment

## Overview

Production deployment strategy covering container architecture, volume management, internal directory structure, database deployment, security hardening, and network configuration. Designed for self-hosted users on bare metal, NAS devices (Synology, Unraid), and cloud VPS.

Build and publication mechanics are documented separately in [DOCKER_BUILD_RELEASE.md](DOCKER_BUILD_RELEASE.md) so runtime deployment guidance stays distinct from release-pipeline design.

## Architecture

### Default: Embedded Database (Single Container)

```
┌──────────────────────────────────────────────────────────┐
│  Docker Host                                             │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │  duskcue (single container)                     │  │
│  │                                                     │  │
│  │  ┌─────────────┐  ┌──────────────────────────────┐ │  │
│  │  │ Rust server  │  │ PostgreSQL 18 (embedded)     │ │  │
│  │  │  :48027       │←│  localhost + Unix socket only │ │  │
│  │  └─────────────┘  └──────────────────────────────┘ │  │
│  │                                                     │  │
│  │  /data ← volume (config, metadata, logs, PG data)  │  │
  │  │  /cache ← volume (HLS, images, storyboards)              │  │
│  │  /media/tv ← bind:ro                                │  │
│  │  /media/movies ← bind:ro                            │  │
│  │                                                     │  │
│  │  /var/run/postgresql ← tmpfs (Unix socket)         │  │
│  │  /data/transcode ← tmpfs (transcode files)         │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

PostgreSQL runs inside the same container as the duskcue, managed by the entrypoint script. It listens only on localhost and a Unix socket — never exposed to the network. The server connects via Unix socket for maximum performance.

### Optional: External Database

Power users can run PostgreSQL separately for resource isolation, WAL-G sidecars, or shared database instances. See [External Database Mode](#external-database-mode-optional).

## Database Strategy: Hybrid Embedded/External

### Decision Logic

```
DUSKCUE_DATABASE_URL set?
├── Yes → External mode: skip embedded PG, connect to specified URL
└── No  → Embedded mode: entrypoint manages PG lifecycle automatically
```

### Why Embedded PostgreSQL

| Aspect | Separate Container | Embedded (chosen default) |
|---|---|---|
| **User experience** | Must manage two services, two volumes, a network | Single container, single volume, zero config |
| **Image size** | ~30MB (app only) | ~80MB (app + PG18 packages) |
| **Deployment complexity** | compose file with 2 services + healthchecks | `docker run` works; compose optional |
| **NAS users** | Must understand Docker networking | Works with Synology Container Manager GUI |
| **Non-Docker users** | Must install and configure PostgreSQL separately | Zero external dependencies |
| **Performance** | TCP loopback or bridge network | Unix socket (3-5x faster than TCP) |
| **Resource isolation** | Independent cgroups; dedicated memory/CPU | Shared cgroup; simpler but less isolated |
| **Backup** | `docker compose exec db pg_dump` | `docker exec duskcue pg_dump` |
| **WAL-G / PITR** | Runs in separate container or sidecar | Runs alongside app in same container |
| **PG upgrades** | Pull new `postgres:18` image | Entrypoint auto-detects version mismatch |

**Design influenced by:** Classifarr's production-proven all-in-one container pattern. Classifarr ships PostgreSQL 17 + 18 inside a single Alpine container with automatic `pg_upgrade`, embedded pgvector, pg_stat_statements, and a comprehensive entrypoint that handles init, upgrades, and stale PID cleanup.

### Non-Docker (Windows, macOS, Linux Native)

For non-Docker deployments, the `postgresql_embedded` crate (theseus-rs, v0.20.2) manages PostgreSQL binaries at runtime:

- On first run, PG binaries are downloaded (or bundled at compile time) and initialized
- PG data stored at `{data_dir}/postgres`
- User never installs or configures PostgreSQL
- For advanced users, setting `DUSKCUE_DATABASE_URL` skips embedded mode entirely

### Embedded PostgreSQL Configuration

| Setting | Value | Rationale |
|---|---|---|
| `listen_addresses` | `''` (empty — Unix socket only) | No network exposure; maximum security |
| `unix_socket_directories` | `/var/run/postgresql` | tmpfs-backed; fast; ephemeral |
| `auth` | `trust` (local only) | No password needed; PG is not network-accessible |
| `encoding` | `UTF8` | Universal text support |
| `data_checksums` | `on` | Silent corruption detection (same as `data_checksums=on` in DATABASE.md) |
| PGDATA | `/data/postgres` | Inside the data volume; survives container recreation |

### Automatic Major-Version Upgrades (Future)

Following Classifarr's pattern, the image can ship both the current and previous PostgreSQL major version. The entrypoint detects `PG_VERSION` mismatch and runs `pg_upgrade --link` automatically. This is planned for future implementation when PG19 is released.

## Internal Container Directory Structure

```
/data/                              # DUSKCUE_DATA_DIR (named volume)
├── config/
│   └── config.toml                 # Bootstrap config (optional; env vars sufficient)
├── postgres/                       # Embedded PostgreSQL data (PGDATA)
│   ├── PG_VERSION                  # Major version marker
│   ├── postgresql.conf             # Auto-generated PG config
│   └── base/                       # Database files
├── metadata/                       # Persistent metadata, artwork, thumbnails
│   ├── artwork/                    # Downloaded poster/backdrop images
│   └── thumbnails/                 # Generated video thumbnails
├── logs/                           # Application + PostgreSQL logs
│   ├── duskcue.json            # Rolling JSON log (tracing-appender)
│   └── postgres.log                # PostgreSQL log (entrypoint-managed)
├── transcode/                      # Temporary transcode files (tmpfs, purged on restart)
└── backups/                        # pg_dump logical backups (local storage target)

/cache/                             # DUSKCUE_CACHE_DIR (named volume or tmpfs)
├── hls/                            # HLS segment cache during playback
├── images/                         # Processed/resized image cache
├── storyboards/                    # Seek preview thumbnail sprite sheets + WebVTT index
└── search/                         # Search index artifacts

/media/                             # Mount point for library bind mounts (read-only)
├── tv/                             # → host:/path/to/tv (bind mount, ro)
├── movies/                         # → host:/path/to/movies (bind mount, ro)
└── music/                          # → host:/path/to/music (bind mount, ro)
```

### Directory Purpose and Lifecycle

| Path | Volume Type | Lifecycle | Contents |
|---|---|---|---|
| `/data/config/` | Named volume (media-data) | Persistent | Bootstrap config.toml |
| `/data/postgres/` | Named volume (media-data) | Persistent | PostgreSQL database files; survives container recreation |
| `/data/metadata/` | Named volume (media-data) | Persistent | Artwork, thumbnails |
| `/data/logs/` | Named volume (media-data) | Persistent | JSON app logs + PostgreSQL logs |
| `/data/transcode/` | tmpfs | Ephemeral | Purged on restart; configurable size |
| `/data/backups/` | Named volume (media-data) | Persistent | pg_dump output if using local backup target |
| `/cache/` | Named volume (media-cache) | Semi-persistent | HLS segments, processed images, storyboards; safe to delete |
| `/media/` | Bind mounts | External | User's media files, mounted read-only |
| `/var/run/postgresql` | tmpfs | Ephemeral | Unix socket; recreated on startup |

### Design Rationale

| Decision | Rationale |
|---|---|
| Single `/data` volume | Backup one volume = backup all state (config, DB, metadata, logs). Classifarr proved this works at scale. |
| PG inside `/data/postgres` | PG data travels with server data. No separate volume to manage. |
| Separate `/data` and `/cache` | Cache is high-write, ephemeral — can go on faster storage (SSD vs HDD). Safe to delete. |
| `/media` as mount point | Clean namespace; libraries reference `/media/{name}` internally. Users map arbitrary host paths. |
| Media mounted read-only | Server never modifies source files. Prevents accidental corruption. |
| Transcode as tmpfs | RAM-backed, auto-cleaned on restart, no disk wear. |
| PG Unix socket on tmpfs | Fastest possible DB connection; no TCP overhead; ephemeral (recreated on startup). |

## Volume Strategy

| Data Type | Strategy | Rationale |
|---|---|---|
| Server data + DB | Named volume (`media-data`) | Docker-managed, portable; single volume contains all persistent state |
| Cache | Named volume (`media-cache`) | Semi-persistent; safe to delete; can be tmpfs for speed |
| Transcode files | tmpfs | Ephemeral; RAM-backed; auto-cleaned |
| PG Unix socket | tmpfs | Ephemeral; recreated on startup; fastest DB connection |
| Media libraries | Bind mounts (read-only) | User controls location; NAS shared folders; pre-existing files |

## Docker Compose

### Production Compose File (Embedded Database)

```yaml
# docker-compose.yml
#
# Production deployment for duskcue with embedded PostgreSQL.
# Copy .env.example to .env and customize before running:
#   cp .env.example .env
#   docker compose up -d

services:
  duskcue:
    image: duskcue:latest
    container_name: duskcue
    init: true
    environment:
      # Leave DATABASE_URL unset to use embedded PostgreSQL.
      # Set it to connect to an external database instead:
      # DUSKCUE_DATABASE_URL: "postgresql://user:pass@db-host:5432/duskcue"
      DUSKCUE_DATA_DIR: "/data"
      DUSKCUE_CACHE_DIR: "/cache"
      DUSKCUE_LOG_LEVEL: "${LOG_LEVEL:-info}"
      DUSKCUE_ENVIRONMENT: "production"
      PUID: "${PUID:-1000}"
      PGID: "${PGID:-1000}"
      TZ: "${TZ:-Etc/UTC}"
    ports:
      - "${HTTP_PORT:-48027}:48027"
    volumes:
      - media-data:/data
      - media-cache:/cache
      - ${TV_PATH:-/dev/null}:/media/tv:ro
      - ${MOVIES_PATH:-/dev/null}:/media/movies:ro
      # - ${MUSIC_PATH:-/dev/null}:/media/music:ro
      # Hardware acceleration — uncomment for Intel/AMD GPU
      # - /dev/dri:/dev/dri
    tmpfs:
      - /data/transcode:size=${TRANSCODE_TMPFS_SIZE:-2G},mode=1777
      - /var/run/postgresql:uid=${PUID:-1000},gid=${PGID:-1000},mode=770
      - /tmp:size=64m,mode=1777
    stop_signal: SIGTERM
    stop_grace_period: 120s
    healthcheck:
      test: ["CMD", "wget", "--no-verbose", "--tries=1", "--spider", "http://localhost:48027/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 30s
    restart: unless-stopped
    read_only: true
    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL
    cap_add:
      - CHOWN
      - SETUID
      - SETGID

volumes:
  media-data:
    name: duskcue-data
  media-cache:
    name: duskcue-cache
```

### Environment File

```env
# .env.example
#
# Copy to .env and customize.
# All values have sensible defaults — only media paths must be set.

# ── Database ──────────────────────────────────────
# By default, PostgreSQL is embedded inside the container.
# To use an external database, set DUSKCUE_DATABASE_URL:
# DUSKCUE_DATABASE_URL=postgresql://user:pass@db-host:5432/duskcue

# ── Media Libraries (host paths) ─────────────────
# Set these to your media directories on the host.
# Mounted read-only inside the container.
TV_PATH=/path/to/your/tv
MOVIES_PATH=/path/to/your/movies
# MUSIC_PATH=/path/to/your/music

# ── Server ────────────────────────────────────────
HTTP_PORT=48027
# Bind address defaults to IPv4 all-interfaces. Use :: for native dual-stack
# where the host OS and Docker networking support IPv6.
DUSKCUE_BIND_ADDRESS=0.0.0.0
LOG_LEVEL=info
TZ=Etc/UTC

# ── User/Group IDs ───────────────────────────────
# Must match a user/group that has read access to your media.
# Find yours: run `id` in a terminal.
PUID=1000
PGID=1000

# ── Transcode ─────────────────────────────────────
# Size of RAM-backed tmpfs for transcode files.
# Increase for 4K content: 4G recommended.
TRANSCODE_TMPFS_SIZE=2G
```

### External Database Mode (Optional)

For users who want resource isolation, shared PG instances, or dedicated WAL-G sidecars:

```yaml
services:
  duskcue:
    image: duskcue:latest
    environment:
      DUSKCUE_DATABASE_URL: "postgresql://duskcue:${DB_PASSWORD}@db:5432/duskcue"
      # ... other env vars
    # ... other config (no PG tmpfs needed)

  db:
    image: postgres:18-alpine
    environment:
      POSTGRES_DB: duskcue
      POSTGRES_USER: duskcue
      POSTGRES_PASSWORD: "${DB_PASSWORD}"
    volumes:
      - pg-data:/var/lib/postgresql
    shm_size: 128mb
    restart: unless-stopped
```

When `DUSKCUE_DATABASE_URL` is set, the entrypoint **skips** embedded PostgreSQL startup entirely. The server connects directly to the specified database.

### Compose Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| `init: true` | tini as PID 1 | Signal forwarding (SIGTERM graceful shutdown), zombie reaping. |
| `read_only: true` | Immutable root filesystem | Maximum security; all writable paths are volumes or tmpfs. |
| `no-new-privileges` | Prevent privilege escalation | Container processes cannot gain more privileges than they start with. |
| `cap_drop: ALL` + minimal `cap_add` | Least privilege | Only CHOWN, SETUID, SETGID needed for PUID/PGID user creation. |
| `start_period: 30s` | Longer startup window | Embedded PG init + migration must complete before healthcheck. |
| `stop_signal: SIGTERM` | Explicit graceful-stop signal | Matches Docker default but makes shutdown intent unambiguous and aligns with the server's signal handler. |
| `stop_grace_period: 120s` | Database-safe shutdown | PostgreSQL Fast shutdown performs a checkpoint before exiting. Under load, this can take 30-60s. 120s ensures PG finishes checkpointing before Docker sends SIGKILL. Matches the server's 3-phase shutdown budget (30s drain + 90s cleanup). |
| `/var/run/postgresql` tmpfs | Unix socket directory | PG creates socket here; tmpfs is fast and ephemeral; uid/gid/mode match runtime user. |
| `/tmp` tmpfs | Temporary files | Required for `read_only: true`; PG and other tools need writable /tmp. |
| Named volumes with explicit names | `duskcue-data`, `duskcue-cache` | Easy to identify in `docker volume ls`; prevents collision with other projects. |
| No `db` service by default | Embedded PostgreSQL | Single container = simplest user experience; external mode available for power users. |

## Native IPv6 Support

Duskcue should support IPv6 as a first-class deployment mode, not only as a reverse-proxy side effect.

Planned Phase 15 behavior:

- Default bind remains IPv4 `0.0.0.0:48027` for maximum compatibility with older Docker/NAS setups.
- Operators can set `DUSKCUE_BIND_ADDRESS=::` to request an IPv6 listener. On platforms where dual-stack sockets are available, this serves IPv4 and IPv6 from one listener. Where the OS or Docker Engine uses IPv6-only sockets, documentation should show explicit IPv4 and IPv6 bindings.
- URL generation must format IPv6 literals with brackets, for example `http://[fd00::10]:48027`, including WebAuthn origins, redirects, and generated server URLs.
- Docker Compose examples should include an optional IPv6 binding:

```yaml
ports:
  - "${HTTP_PORT:-48027}:48027"
  # Optional explicit IPv6 host binding when Docker IPv6 is enabled:
  # - "[::]:${HTTP_PORT:-48027}:48027"
```

- Reverse proxy examples must keep IPv6 client IPs intact through `X-Forwarded-For` / `Forwarded`, and Duskcue must only trust those headers from configured trusted proxy CIDRs.
- Metrics allowlists and trusted proxy lists must accept IPv6 CIDR ranges such as `::1/128`, `fd00::/8`, and `2001:db8::/32`.
- Health checks should continue to use loopback. IPv6-only deployments can use `http://[::1]:48027/health` when the container image includes tooling that supports bracketed IPv6 literals.

Security requirements:

- Enabling IPv6 must not silently bypass the local/remote/exposed network-mode rules.
- Public IPv6 exposure should be treated the same as public IPv4 exposure: exposed mode requires authentication, TLS, signed streaming URLs, and correct trusted-proxy configuration.
- Unique local addresses (`fc00::/7`), link-local addresses (`fe80::/10`), loopback (`::1/128`), and IPv4-mapped IPv6 addresses must be classified deliberately rather than by string prefix.

### Abrupt Shutdown Design Rule

Container lifecycle hooks are an optimization, not a durability guarantee.

- The server must handle `SIGTERM` correctly.
- Embedded PostgreSQL must be stopped via `pg_ctl -m fast stop` during normal shutdown.
- The platform must still recover automatically if Docker reaches `SIGKILL` or the host loses power.
- Compose `pre_stop` hooks are not relied upon for database safety because Docker documents that they do not run on sudden kills.

## Entrypoint Script

The entrypoint handles PUID/PGID user creation, embedded PostgreSQL lifecycle, and privilege dropping:

```bash
#!/bin/sh
set -e

PUID=${PUID:-1000}
PGID=${PGID:-1000}
DATA_DIR="/data"
PG_DATA="$DATA_DIR/postgres"
PG_RUN="/var/run/postgresql"

# ── User/Group Setup ──────────────────────────────
if [ "$(id -u)" -eq 0 ]; then
    EXISTING_GROUP=$(getent group "$PGID" | cut -d: -f1)
    if [ -z "$EXISTING_GROUP" ]; then
        addgroup -g "$PGID" duskcue
        TARGET_GROUP="duskcue"
    else
        TARGET_GROUP="$EXISTING_GROUP"
    fi

    if ! id duskcue >/dev/null 2>&1; then
        adduser -D -u "$PUID" -G "$TARGET_GROUP" -h /data duskcue
    fi

    mkdir -p "$DATA_DIR"/{config,metadata,logs,transcode,backups}
    mkdir -p "$DATA_DIR"/metadata/{artwork,thumbnails}
    mkdir -p /cache/{hls,images,storyboards,search}
    chown -R "$PUID:$PGID" "$DATA_DIR" /cache
fi

run_as_user() {
    if [ "$(id -u)" -eq 0 ]; then
        su-exec duskcue "$@"
    else
        "$@"
    fi
}

# ── Database Setup ────────────────────────────────
if [ -n "$DUSKCUE_DATABASE_URL" ]; then
    echo "Using external PostgreSQL: $DUSKCUE_DATABASE_URL"
else
    echo "Starting embedded PostgreSQL..."

    if [ ! -f "$PG_DATA/PG_VERSION" ]; then
        echo "Initializing new PostgreSQL database..."
        mkdir -p "$PG_DATA"
        run_as_user initdb -D "$PG_DATA" --auth=trust --encoding=UTF8 --data-checksums
        echo "listen_addresses = ''" >> "$PG_DATA/postgresql.conf"
        echo "unix_socket_directories = '$PG_RUN'" >> "$PG_DATA/postgresql.conf"
        echo "log_destination = 'stderr'" >> "$PG_DATA/postgresql.conf"
        echo "logging_collector = on" >> "$PG_DATA/postgresql.conf"
        echo "log_directory = '$DATA_DIR/logs'" >> "$PG_DATA/postgresql.conf"
        echo "log_filename = 'postgres.log'" >> "$PG_DATA/postgresql.conf"
    fi

    rm -f "$PG_DATA/postmaster.pid"
    run_as_user pg_ctl -D "$PG_DATA" -l "$DATA_DIR/logs/postgres.log" start

    echo "Waiting for PostgreSQL..."
    ATTEMPTS=0
    until run_as_user pg_isready -q; do
        ATTEMPTS=$((ATTEMPTS + 1))
        if [ "$ATTEMPTS" -ge 60 ]; then
            echo "ERROR: PostgreSQL did not start within 60 seconds."
            tail -n 50 "$DATA_DIR/logs/postgres.log"
            exit 1
        fi
        sleep 1
    done

    run_as_user createdb duskcue 2>/dev/null || true

    export DUSKCUE_DATABASE_URL="postgresql:///duskcue?host=$PG_RUN"
    echo "Embedded PostgreSQL ready."
fi

# ── Start Server ──────────────────────────────────
echo "Starting duskcue..."

# Graceful shutdown handler — stops embedded PG before exit.
# Without this, Docker's SIGKILL after stop_grace_period would
# leave PG without a checkpoint, forcing WAL replay on next start.
if [ -z "$DUSKCUE_DATABASE_URL" ] || [ -z "${DUSKCUE_DATABASE_URL##*host=$PG_RUN*}" ]; then
    trap 'echo "Stopping embedded PostgreSQL..."; run_as_user pg_ctl -D "$PG_DATA" -m fast stop 2>/dev/null; exit 0' TERM INT
fi

exec run_as_user /usr/local/bin/duskcue
```

### Startup Sequence (Embedded Mode)

```
1. Entrypoint starts as root
2. Create PUID/PGID user/group
3. Create directory structure under /data and /cache
4. Check DUSKCUE_DATABASE_URL
   ├── Set → skip to step 9 (external DB)
   └── Not set → embedded mode (continue)
5. Check /data/postgres/PG_VERSION
   ├── Exists → remove stale PID, start PG
   └── Missing → initdb, configure, start PG, createdb
6. Wait for pg_isready (up to 60s)
7. Export DUSKCUE_DATABASE_URL (Unix socket connection)
8. Drop privileges (su-exec duskcue)
9. Server binary starts:
   a. Parse CLI/env → BootstrapConfig
   b. Connect to PostgreSQL
   c. Run pending migrations (sqlx embedded)
   d. Load server_config → RuntimeConfig (or seed defaults → setup wizard)
   e. Start scheduled tasks
   f. Bind HTTP listener on :48027
   g. Ready
```

## Security

### OS & Docker Engine Requirements

The platform detects the host OS and Docker Engine version at startup and every 24 hours. It warns the admin if requirements are not met, but never blocks startup or auto-updates. Full detection logic, minimum version matrix, and admin dashboard warnings documented in [OS_HARDENING.md](OS_HARDENING.md).

| Component | Minimum | Recommended |
|---|---|---|
| **Docker Engine** | v28.0.0 | v29.4.3+ (CVE-2026-31431 mitigation) |
| **Alpine base image** | 3.22 | 3.22 (pin minor, track patches) |
| **Linux host** | Debian 12 / Ubuntu 22.04 / AlmaLinux 9 / Rocky Linux 9 | Current stable of each |
| **Windows host** | Windows 11 23H2 (build 22631) | Windows 11 24H2 |

### Non-Root User (PUID/PGID Pattern)

| Approach | Pros | Cons |
|---|---|---|
| **PUID/PGID (chosen)** | Flexible — matches any host user; proven by LinuxServer.io | Requires entrypoint script |
| Fixed `USER 1000:1000` | Simple Dockerfile | Breaks if host uses different UID |
| Run as root | Everything works | Maximum attack surface |

**User must ensure:** The PUID/PGID has **read access** to media bind mounts and **read/write access** to `/data` and `/cache` volumes.

### Capability Restriction

```yaml
cap_drop:
  - ALL
cap_add:
  - CHOWN       # chown files in /data to runtime user
  - SETUID      # switch to PUID user
  - SETGID      # switch to PGID group
```

### Hardening Summary

| Feature | Status | Notes |
|---|---|---|
| Non-root runtime | Enabled | PUID/PGID with su-exec privilege drop |
| Read-only root filesystem | Enabled | All writable paths on volumes or tmpfs |
| Capability restriction | Enabled | Only CHOWN, SETUID, SETGID |
| `no-new-privileges` | Enabled | Prevents privilege escalation |
| PG network isolation | Enabled | `listen_addresses = ''` — Unix socket only |
| Setuid/setgid stripped | Enabled | `find / -perm /6000 -exec chmod a-s {} \;` in Dockerfile |
| OS version detection | Enabled | Startup + 24h periodic; warns admin on outdated host |
| Docker Engine detection | Enabled | Warns if below v28.0.0; recommends v29.4.3+ |

### Hardware Acceleration

```yaml
# Intel/AMD GPU (VAAPI, Quick Sync)
devices:
  - /dev/dri:/dev/dri

# NVIDIA GPU (NVENC, NVDEC) — requires nvidia-container-toolkit
deploy:
  resources:
    reservations:
      devices:
        - driver: nvidia
          count: all
          capabilities: [gpu]
environment:
  NVIDIA_VISIBLE_DEVICES: all
  NVIDIA_DRIVER_CAPABILITIES: compute,video,utility
```

## Network Configuration

### Default (No Network Needed)

With embedded PostgreSQL, there is no inter-service networking. The container only needs the HTTP port published.

### Host Mode (Optional)

For users who need device discovery (DLNA, Chromecast, AirPlay):

```yaml
duskcue:
  network_mode: host
  # Remove 'ports', 'networks' sections
```

## Synology / Unraid NAS Deployment

### Synology (Container Manager)

1. Pull the image: `duskcue:latest`
2. Configure environment variables in the UI
3. Map shared folders to `/media/*` (read-only)
4. Create named volume for `/data`
5. Set PUID/PGID to match a Synology user with media access

### Unraid

1. Add container via Community Applications or manual template
2. Set `/data` to an appdata share: `/mnt/user/appdata/duskcue`
3. Map media shares to `/media/*` (read-only)
4. Set PUID/PGID (typically `99:100` for Unraid `nobody` user)
5. No PostgreSQL configuration needed — embedded mode handles everything

**NAS-specific notes:**
- Shared folders are at `/volume1/shared_folder_name` (Synology) or `/mnt/user/share_name` (Unraid)
- Synology Container Manager supports docker-compose.yml via "Project" feature
- `/dev/dri` is available on Intel-based NAS models for Quick Sync
- Embedded PG data stays inside the appdata volume — follows NAS backup conventions

## Operational Procedures

### First Run

```bash
# 1. Copy and edit environment file
cp .env.example .env
# Edit .env — set media paths (only required config)

# 2. Start container
docker compose up -d

# 3. Watch logs (includes PG init + migration output)
docker compose logs -f duskcue

# 4. Open setup wizard
# Browser: http://<host-ip>:48027
```

### Backup

```bash
# Full backup (all data including embedded PostgreSQL)
docker run --rm -v duskcue-data:/data -v $(pwd):/backup \
  alpine tar czf /backup/duskcue-data.tar.gz /data

# Database-only backup
docker compose exec duskcue pg_dump -U duskcue duskcue > backup.sql
```

### Update

```bash
docker compose pull duskcue
docker compose up -d duskcue
docker compose logs -f duskcue
```

### Cleanup

```bash
# Transcode cache (tmpfs — purged on restart)
docker compose restart duskcue
```

## Disk Space Monitoring

The server monitors disk usage on critical paths and takes action when thresholds are exceeded. Full strategy documented in [CACHE_STORAGE.md](CACHE_STORAGE.md).

### Monitored Paths

| Path | Default Threshold | Action |
|---|---|---|
| `/data` volume | 90% usage | `server_alert` notification to admins |
| `/cache` volume | 90% usage | `server_alert` notification to admins |
| `/data/transcode` (tmpfs) | 80% of allocation | Kill oldest transcode session; return `PLAY_010` to client |

### Configuration

Thresholds are configured via `server_config.storage.disk_space_warnings` JSONB (see [CONFIGURATION.md](CONFIGURATION.md)). The `disk_space_check` scheduled task runs every 30 minutes by default.

### Metrics

Storage metrics are exposed on the Prometheus `/metrics` endpoint:

| Metric | Type | Labels |
|---|---|---|
| `storage_usage_bytes` | gauge | path (`data`, `cache`, `transcode`) |
| `storage_capacity_bytes` | gauge | path |
| `storage_usage_percent` | gauge | path |
| `cache_evictions_total` | counter | cache_type (`storyboard`, `image`, `hls`) |
| `cache_size_bytes` | gauge | cache_type |
| `cache_items` | gauge | cache_type |

## Resource Recommendations

Memory management strategy is documented in [MEMORY.md](../design/MEMORY.md). CPU management strategy is documented in [CPU.md](../design/CPU.md).

### Docker Memory Limits by Hardware Profile

| Hardware Profile | Memory Limit | CPU Limit | Max Concurrent Transcodes | PG shared_buffers |
|---|---|---|---|---|
| NAS (2 GB RAM, ARM) | 1.5 GB | 1.5 cores | 1 | 64 MB |
| NAS (4 GB RAM, ARM) | 3 GB | — | 2 | 128 MB |
| ARM SBC (RK3588, big.LITTLE) | 4 GB | `cpuset-cpus: 4-7` | 2 | 128 MB |
| Desktop (8 GB RAM) | 4 GB | 2.0 cores | 2-3 | 128 MB |
| Server (16 GB+ RAM) | 8 GB | 4.0 cores | 4+ | 256 MB |

### Resource Limits in Compose

Add to the `duskcue` service for production deployments:

```yaml
    deploy:
      resources:
        limits:
          memory: ${MEMORY_LIMIT:-4G}
          cpus: ${CPU_LIMIT:-2.0}
        reservations:
          memory: 512M
          cpus: 0.5
```

The server automatically detects Docker cgroup v2 memory limits via `/sys/fs/cgroup/memory.max` and uses container-aware thresholds in the memory watchdog. No configuration needed — detection is automatic on Linux. On bare metal or macOS/Windows, the watchdog falls back to host memory via sysinfo.

### ARM SBC CPU Pinning (big.LITTLE)

For ARM SBCs with big.LITTLE topology (e.g. RK3588), pin the container to big cores for better transcode performance:

```yaml
    # RK3588 example: pin to Cortex-A76 big cores (typically cores 4-7)
    cpuset-cpus: "${CPUSET:-0-7}"
    deploy:
      resources:
        limits:
          memory: ${MEMORY_LIMIT:-4G}
```

### Docker CPU Priority

| Option | Effect | When to Use |
|---|---|---|
| `--cpu-shares 512` | Soft priority; FFmpeg gets less share during contention | Multi-container hosts |
| `--cpus 2.0` | Hard cap on CPU time | Prevent FFmpeg from monopolizing |
| `--cpuset-cpus 4-7` | Pin to specific cores | ARM big.LITTLE SBCs |
| `--cap-add SYS_NICE` | Allow nice/renice inside container | Only needed for `ionice` |

These Docker limits are separate from the application-level `server_config.resource_limits` and `server_config.cpu` JSONB columns (which control transcode concurrency, CPU/memory thresholds, FFmpeg thread count, process priority, and hardware acceleration). Docker limits are a hard ceiling; application limits are enforced before hitting Docker limits.

See [MEMORY.md](../design/MEMORY.md) for memory management and [CPU.md](../design/CPU.md) for CPU management including FFmpeg subprocess lifecycle, connection pool tuning, health checks, watchdogs, and crash recovery.

### TLS and Remote Access in Docker

The container supports two TLS patterns:

1. **Platform-managed TLS** — the server binds port 443 directly with rustls + ACME. Requires port 80 and 443 exposed. Certificates stored in `/data/tls/`.
2. **Reverse proxy TLS** — Caddy/Traefik/Nginx in front of the container, handling TLS termination. The container runs HTTP only. Detected via `X-Forwarded-Proto` header.

For remote access without opening ports, the recommended pattern is a VPN (Tailscale, WireGuard, Pangolin). Full remote access guidance in [SECURITY.md](../security/SECURITY.md).

Cloudflare Tunnel is explicitly **not supported** for video streaming — CDN-specific terms prohibit serving video hosted outside Cloudflare storage. See [SECURITY.md](../security/SECURITY.md) for alternatives.

## Research Sources

- Docker volumes: https://docs.docker.com/engine/storage/volumes/
- Docker bind mounts: https://docs.docker.com/engine/storage/bind-mounts/
- Docker tmpfs mounts: https://docs.docker.com/engine/storage/tmpfs/
- Docker Compose depends_on with healthchecks: https://docs.docker.com/compose/how-tos/startup-order/
- Docker Compose services reference: https://docs.docker.com/reference/compose-file/services/
- Docker security: https://docs.docker.com/engine/security/
- Docker rootless mode: https://docs.docker.com/engine/security/rootless/
- PostgreSQL 18 Docker Hub (PGDATA change): https://hub.docker.com/_/postgres
- LinuxServer.io Plex container (PUID/PGID pattern): https://github.com/linuxserver/docker-plex
- Jellyfin Docker image (config/cache/env pattern): https://github.com/jellyfin/jellyfin-packaging/blob/master/docker/Dockerfile
- Classifarr all-in-one container (embedded PG + auto-upgrade + entrypoint pattern): https://github.com/cloudbyday90/Classifarr
- postgresql-embedded Rust crate (non-Docker embedded PG): https://github.com/theseus-rs/postgresql-embedded
