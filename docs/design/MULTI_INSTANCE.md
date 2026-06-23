# Multi-Instance / Horizontal Scaling

## Overview

This document is the authoritative design for Duskcue's multi-instance scaling posture — whether Duskcue is designed to run as a single process forever, or whether it must eventually support horizontal scaling across multiple concurrent instances. The decision shapes the design of every shared-state component going forward.

The decision documented here: **Duskcue is single-instance by design, matching the architecture of every peer media server (Plex, Jellyfin, Emby).** No horizontal scaling is planned. High availability is achieved at the infrastructure layer (container migration, VM failover), not at the application layer. New features should not introduce distributed-coordination dependencies, but should prefer PostgreSQL-backed state when trivial to do so.

## Scope

**Covers:**

- Scaling posture decision (single-instance vs multi-instance vs "designed-for-future")
- Inventory of in-memory mutable state that would block horizontal scaling
- High-availability strategy for self-hosted deployments
- Design guidelines for new features (when to use PG state vs in-memory state)
- Hypothetical migration path if the decision were ever revisited
- Deployment topology recommendations (Phase 15 Docker alignment)

**Does NOT cover:**

- Read replicas for PostgreSQL analytics — that's a separate concern (analytic dashboards could use a read replica without Duskcue itself being multi-instance)
- The embedded PostgreSQL HA story — see [BACKUP_RECOVERY.md](../operations/BACKUP_RECOVERY.md) and [DOCKER_DEPLOYMENT.md](../operations/DOCKER_DEPLOYMENT.md)
- Multi-tenancy (multiple isolated libraries per user) — already supported via `library_id` foreign keys; orthogonal to multi-instance

## Decision — Single-Instance by Design

**Duskcue is designed as a single-instance application. There are no plans to support horizontal scaling across multiple concurrent Duskcue processes.** This is a deliberate, permanent design choice — not a v1.0 deferral — and it matches the architecture of every comparable self-hosted media server.

### Why Single-Instance Is the Right Posture

#### 1. Every Peer Media Server Is Single-Instance

The three open-source/commercial media servers that Duskcue is directly comparable to are all single-instance by design:

| Server | Architecture | Multi-instance? |
|---|---|---|
| **Plex Media Server** | Single server process, local metadata DB | ❌ Not supported |
| **Jellyfin** | Forked from Emby; single server process, SQLite metadata DB | ❌ Not supported — SQLite locks block concurrent writers |
| **Emby** | Single server process, SQLite metadata DB | ❌ Explicitly documented as "personal media server"; Emby team cites Terms of Service when users request clustering |

The community consensus in the Emby/Jellyfin ecosystem is that multi-instance is contrary to the personal-media-server use case. Users requesting load-balanced clusters are told to use VM/container migration for high availability instead.

#### 2. The Deployment Target Is a Single Node

Duskcue's deployment target is explicitly single-node consumer hardware:

- **NAS devices** (Synology, QNAP, Asustor) — single CPU package, 2–8 GB RAM
- **Single-board computers** (Raspberry Pi 5) — 4–8 GB RAM, ARM CPU
- **Mini-PCs** (Intel N100, Beelink, Intel NUC) — 8–16 GB RAM, x86
- **Personal workstation / home server** — single Linux box

A use case that needs multiple nodes for capacity is well beyond the personal-media-server scope Duskcue targets. The realistic concurrency ceiling is 5–20 simultaneous streams (family + friends); a single modern mini-PC with hardware transcoding handles this comfortably.

#### 3. Distributed Coordination Cost Is High

The Nextcloud scaling guide (linked in Research Sources) illustrates the cost: 1000-user Nextcloud requires load balancer, 3+ application servers, PostgreSQL primary + replicas, Redis Sentinel cluster, dedicated cron worker, S3 object storage. That complexity is justified for enterprise file collaboration with thousands of users; it is wildly disproportionate for a media server serving a household.

For Duskcue, the distributed-coordination cost would manifest as:

- **Redis or NATS JetStream dependency** for shared rate-limiting state, WebAuthn challenge state, SSE event bus
- **Distributed lock manager** for scheduled task single-execution (only one instance runs each scheduled task)
- **Sticky sessions or shared transcode session state** so playback start→heartbeat→stop lands on the same instance that owns the FFmpeg process
- **Coordinated filesystem watcher** (only one instance can hold kernel `inotify` handles per directory; others must receive events via pub/sub)
- **PgBouncer + connection-pool tuning** to avoid connection storms from N application servers
- **Stateful load balancer** for transcode affinity, or shared transcode session state

Each of these adds operational complexity, failure modes, and code that exists only to support a deployment topology that no realistic Duskcue user needs.

#### 4. In-Memory State Is Already Pervasive

The current codebase has substantial in-process mutable state that assumes a single process. Migrating this to distributed state is non-trivial:

| Component | Current State | Migration Cost |
|---|---|---|
| `TranscodeManager.sessions` | `Arc<DashMap<Uuid, TranscodeSession>>` — active FFmpeg processes | High — sessions reference local OS processes; would need shared state + sticky routing |
| `LibraryWatcherManager` | `notify` debouncer + kernel `inotify` handles | Very high — kernel resources are per-process; requires pub/sub fan-out |
| `Scheduler` | Single-executor-per-task-type registry; tasks fire once per tick | High — needs distributed lock or leader election |
| `RateLimitState` | `governor` direct rate limiters (in-memory counters) | Medium — Redis-backed rate limiter alternative |
| `webauthn_challenges` | `Arc<DashMap<String, WebauthnChallenge>>` — 5-minute TTL ceremonies | Medium — Redis or PG table |
| `EventBus` (SSE) | `DashMap<Uuid, broadcast::Sender>` per user | High — Redis pub/sub or NATS |
| `TvdbClient.token_state` | `RwLock<TokenState>` — JWT token cache | Low — per-instance cache is fine (token is per-client; each instance fetches its own) |

Migrating all of this is multiple person-months of work for zero user-visible benefit on the realistic deployment target.

#### 5. Container/VM Migration Covers the HA Use Case

The legitimate concern behind multi-instance requests is usually high availability: "if my server crashes, my family loses media access until I get home to reboot it." This is solvable at the infrastructure layer without application-level horizontal scaling:

| Pattern | How it works | Complexity |
|---|---|---|
| **Docker container restart policy** | `docker compose` restarts the container automatically on crash; embedded PG WAL recovery brings the DB current | Trivial — already the default |
| **VM live migration** (Proxmox, KVM, Hyper-V) | The Duskcue VM moves to another physical host on hardware failure; storage is on shared NFS/iSCSI | Operator-managed; no Duskcue changes |
| **Kubernetes StatefulSet** | `replicas: 1` with `PodDisruptionBudget`, persistent volume, node anti-affinity; k8s reschedules on node failure | Operator-managed; Phase 15 docker example shows single-replica pattern |
| **Active-passive with shared storage** | Two Duskcue instances, only one active (lockfile prevents dual-active); failover promotes the passive | Future enhancement — see "Hypothetical HA Path" below |

These patterns achieve the user's goal (no manual intervention on hardware failure) without application-level multi-instance complexity.

### Why Not "Design for Future Multi-Instance Just in Case"

A common temptation is to design every component for distributed coordination from day one, "in case we ever need it." This is rejected for Duskcue because:

1. **YAGNI** — Six years of Plex, Jellyfin, and Emby history show that personal media servers never outgrow single-instance. The use case doesn't materialize.
2. **Distributed-by-default adds 30–50% to feature cost** — Every new stateful feature requires Redis/PG schema design, distributed failure-mode analysis, and integration tests for split-brain scenarios. That cost is paid by every feature forever, for a deployment topology that may never ship.
3. **It's reversible** — If multi-instance demand genuinely emerges (which would require Duskcue to become popular in a use case we don't currently target, like a hosted multi-tenant Plex-as-a-service), the migration is expensive but bounded: enumerate the in-memory state, add a Redis/NATS layer, add sticky routing for transcode, add distributed scheduler locking. Estimated 2–3 person-months of focused work. Not impossible, just not pre-paid.
4. **Premature distributed design often gets it wrong** — Distributed systems designed speculatively tend to encode assumptions that don't match the eventual use case. When real multi-instance demand arrives, the speculative abstractions are usually wrong and get rewritten anyway. Better to design for single-instance explicitly and migrate deliberately.

## High-Availability Strategy

Single-instance does not mean single-point-of-failure. Duskcue deployments achieve HA via infrastructure patterns, not application clustering:

### Recommended HA Patterns (by deployment complexity)

| Tier | Pattern | Recovery Time | Complexity |
|---|---|---|---|
| **Basic** (default) | Docker `restart: unless-stopped`; embedded PG crash recovery via WAL replay | Seconds to ~1 minute | Zero — already the default |
| **Standard** | Docker Compose on a NAS or mini-PC with healthcheck + auto-restart; data on RAID/ZFS | Seconds to ~1 minute | Operator sets up Docker |
| **Enhanced** | Proxmox/Kubernetes with persistent volume on NFS; VM/pod auto-migrates to another physical node on failure | 1–5 minutes | Operator runs the cluster |
| **Maximum** | Active-passive failover (future — see below) | <30 seconds with shared storage | Requires Duskcue HA mode feature |

For 99% of self-hosted deployments, **Basic** or **Standard** is sufficient. The server crashes, the container restarts, embedded PG replays WAL, family reconnects in under a minute. This matches the actual user pain threshold — losing media for 60 seconds during an unexpected reboot is acceptable.

### Hypothetical Active-Passive Path (Not Planned for v1.0)

If Duskcue ever adds an HA mode (active-passive, not active-active), the design would be:

1. **Lockfile-based leader election** — Both instances point at the same data directory (NFS/shared storage); the lockfile (existing `Lockfile::acquire()` in `server/src/lockfile.rs`) determines the active instance
2. **PostgreSQL as the state authority** — All session, config, and playback state is already PG-backed; passive instance can take over without state loss
3. **Heartbeat-based failover** — Active instance writes heartbeat to a `leader_election` table every 5 seconds; passive promotes itself if heartbeat is >15 seconds stale
4. **Transcode sessions lost on failover** — Active FFmpeg processes die with the failed instance; clients reconnect and the new instance creates fresh transcode sessions. Acceptable because transcodes are inherently resumable ( HLS segments are cached).
5. **No stateful load balancer needed** — DNS or VIP points at whichever instance is active

This is **not planned for v1.0 or any currently-foreseeable release**. Documenting it here only to show that the single-instance decision doesn't preclude HA entirely — it just defers it to a deliberate, scope-controlled future enhancement rather than baking speculative abstractions into every component today.

## Design Guidelines for New Features

The single-instance decision guides how to design new stateful features:

### Prefer PG-Backed State When Trivial

When a new feature needs mutable state and storing it in PostgreSQL is roughly the same code complexity as storing it in a `DashMap`, **prefer PG**. Examples:

- ✅ User preferences → `users.metadata` JSONB (already the pattern)
- ✅ Notification state → `notifications` table (already the pattern)
- ✅ Scheduled task state → `scheduled_tasks` / `scheduled_task_runs` tables (already the pattern)
- ✅ Subtitle offset → `user_item_data.metadata` JSONB (already the pattern)

These features are already PG-backed because that was the natural design. No change needed.

### Use In-Memory State When PG Is Genuinely Costly

When a feature's state is ephemeral, high-frequency, or references local OS resources, **in-memory is correct** and should not be forced into PG:

- ✅ Active FFmpeg transcode sessions → in-memory (sessions reference local OS processes)
- ✅ WebAuthn ceremony challenges (5-minute TTL) → in-memory (short-lived, high-frequency create/lookup/delete)
- ✅ Filesystem watcher state → in-memory (kernel handles are local)
- ✅ Rate-limiter counters → in-memory (high-frequency reads/writes per request)
- ✅ SSE event-bus channels → in-memory (per-connection state)
- ✅ TVDB JWT cache → in-memory (per-instance auth token)

These are correct in-memory uses. They would only need to change IF multi-instance were adopted, which this doc rules out.

### Anti-Pattern: Speculative Distributed Coordination

Do NOT add Redis/NATS/PG-state for features that work fine in-memory, "in case we scale later." Examples:

- ❌ Storing WebAuthn challenges in Redis instead of `DashMap` (no benefit; only adds a dependency)
- ❌ Distributed lock for scheduled tasks when the existing single-instance assumption is correct
- ❌ Pub/sub for filesystem watcher events (only useful if multiple instances watch the same paths)

These speculative abstractions add complexity without value.

### Document Multi-Instance Implications

When adding a new in-memory state component, **document the multi-instance implication in the docstring**:

```rust
/// Active transcode sessions.
/// MULTI-INSTANCE: Would require sticky-session routing or shared state (Redis)
/// if Duskcue ever adopted horizontal scaling. See MULTI_INSTANCE.md.
sessions: Arc<DashMap<Uuid, TranscodeSession>>,
```

This makes future audit easier if the decision is ever revisited.

## Deployment Topologies

Phase 15 (Docker & Deployment) defines the canonical deployment patterns. This section aligns them with the single-instance decision:

### Default: Single Docker Container (Phase 15 Default)

```yaml
# docker-compose.yml (canonical single-instance)
services:
  duskcue:
    image: ghcr.io/duskcue/duskcue:latest
    ports:
      - "48027:48027"
    volumes:
      - duskcue-data:/data
      - duskcue-cache:/cache
    tmpfs:
      - /transcode:size=4G
    environment:
      DUSKCUE_DATABASE_URL: postgresql://duskcue@localhost/duskcue
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "wget", "--spider", "-q", "http://localhost:48027/health"]
      interval: 30s
      timeout: 5s
      retries: 3
volumes:
  duskcue-data:
  duskcue-cache:
```

**Embedded PostgreSQL** runs in the same container. **No `replicas: >1`, no service mesh, no load balancer.** This is the canonical topology and what 95%+ of deployments should use.

### External PostgreSQL (Optional, Same Duskcue Instance)

Operators who already run PostgreSQL (e.g., for other self-hosted apps) can point Duskcue at it via `DUSKCUE_DATABASE_URL`. Still single-instance — Duskcue + external PG = one Duskcue process. No multi-instance concern.

### Kubernetes (Operator Choice)

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: duskcue
spec:
  replicas: 1  # MUST be 1
  serviceName: duskcue
  template:
    spec:
      containers:
        - name: duskcue
          image: ghcr.io/duskcue/duskcue:latest
          # ...
  volumeClaimTemplates:
    - metadata:
        name: data
      spec:
        resources:
          requests:
            storage: 100Gi
```

**`replicas: 1` is mandatory.** Kubernetes will reschedule the pod on node failure, achieving HA without Duskcue being multi-instance. The persistent volume ensures no data loss across rescheduling.

**What not to do:** `replicas: 3` with a `Deployment` and a shared `PersistentVolumeClaim`. The lockfile will prevent two instances from starting, and even if it didn't, the in-memory state would diverge immediately (two schedulers running the same task, two filesystem watchers tripping over each other, transcode sessions failing because the FFmpeg process is on a different pod).

## In-Memory State Inventory

This is the authoritative list of in-process mutable state in Duskcue. Future audits (if the single-instance decision is ever revisited) start here.

| Component | Location | State Type | Multi-Instance Migration |
|---|---|---|---|
| **WebAuthn challenges** | `AppState.webauthn_challenges` (`DashMap<String, WebauthnChallenge>`) | Ephemeral (5-min TTL), per-ceremony | Redis hash with TTL OR PG table with `expires_at` |
| **Transcode sessions** | `TranscodeManager.sessions` (`DashMap<Uuid, TranscodeSession>`) | Ephemeral (lifetime of FFmpeg process), references local OS process | Sticky routing required; state in Redis is insufficient because the FFmpeg process is local |
| **HW accel detection** | `TranscodeManager.hw_detection` (`RwLock<HwAccelDetectionResult>`) | Cached at startup, rarely changes | Per-instance (each node has different hardware) — no migration needed |
| **Filesystem watcher** | `LibraryWatcherManager` internals (`Mutex<HashMap>`, debouncer) | Long-lived, references kernel handles | Only one instance can hold kernel handles; requires pub/sub fan-out to others |
| **Scheduler executors** | `Scheduler.executors` (`Vec<(String, TaskExecutor)>`) | Static registration; task execution is the concern | Distributed lock (PG advisory lock, Redis SETNX, or dedicated leader election) |
| **Rate limiters** | `RateLimitState` (governor in-memory state stores) | Per-key counters, high frequency | Redis-backed governor alternative (`governor-redis`) |
| **EventBus (SSE)** | `DashMap<Uuid, broadcast::Sender>` per user | Long-lived per connection | Redis pub/sub or NATS JetStream |
| **TVDB JWT cache** | `TvdbClient.token_state` (`RwLock<TokenState>`) | Per-client auth; each instance has its own | No migration needed — each instance fetches its own token |
| **Process-wide constants** | `pub static VALID_*` arrays, `LazyLock<Regex>` | Read-only after init | No migration needed |
| **Server start time** | `OnceLock<Instant> START_TIME` | Per-process | No migration needed (each instance has its own uptime) |
| **Environment string** | `OnceLock<String> ENVIRONMENT` | Per-process, set at startup | No migration needed |
| **Shutdown flag** | `AtomicBool SHUTDOWN_STARTED` | Per-process | No migration needed |

**The four hard problems** (transcode sessions, FS watcher, scheduler, rate limiters) are the blockers. Everything else is either trivially per-instance or trivially PG/Redis-backed.

## Edge Cases

### Operator Accidentally Runs Two Instances

Despite the lockfile, an operator could circumvent it (e.g., different data directories, or `--no-lockfile` flag if we ever add one). The result would be:

- Two schedulers running the same tasks (double library scans, double metadata refresh, double subtitle auto-fetch)
- Two filesystem watchers fighting over the same kernel handles (one will get events, the other won't)
- Transcode sessions failing randomly (load balancer routes start→heartbeat to different instances)
- Database contention (both instances write to the same `media_items` rows)

**Mitigation:** The lockfile (`server/src/lockfile.rs`) prevents this in the default configuration. The startup error message is clear: "Another Duskcue instance is running (PID X). Stop it first or use a different data directory."

### Database Connection Pool Exhaustion

Single-instance Duskcue uses `max_connections(20)` (see `main.rs`). Multi-instance with N instances × 20 connections = 20N connections; PG default `max_connections = 100` is quickly exhausted. This is yet another reason multi-instance without explicit support doesn't work.

### Split-Brain on Network Partition (Hypothetical HA Mode)

If active-passive HA mode is ever added, a network partition between active and passive could cause both to think they're active. Mitigation: PG advisory lock (`pg_try_advisory_lock`) as the source of truth for leadership — only one session can hold the lock, regardless of network state. Documented for the future HA-mode work; not relevant to single-instance v1.0.

### Embedded PG and External PG Mode Interaction

Embedded PG runs in the same container as Duskcue — single-instance by definition. External PG mode lets the operator point at any PG, but Duskcue is still single-instance; multiple Duskcue instances against the same external PG would hit all the in-memory state problems described above.

## Implementation Status

| Component | Status | Notes |
|---|---|---|
| `Lockfile` (single-instance enforcement) | ✅ Implemented | `server/src/lockfile.rs` — PID file with stale-lock detection via `sysinfo` |
| `Scheduler` single-executor pattern | ✅ Implemented | Single `Scheduler` instance per process; executors fire once per tick |
| `TranscodeManager` in-memory sessions | ✅ Implemented | `DashMap` keyed by session UUID |
| `LibraryWatcherManager` (singleton) | ✅ Implemented | One debouncer per process; kernel handle ownership |
| Documented single-instance posture | ✅ This document | New stateful features reference this doc for design guidance |
| Active-passive HA mode | Not planned | Future enhancement; path documented in "Hypothetical Active-Passive Path" |
| Distributed lock for scheduler | Not needed | Single-instance assumption makes this unnecessary |
| Redis/NATS shared state | Not needed | Single-instance assumption makes this unnecessary |

No implementation work is required for v1.0 — single-instance is the natural state of the current codebase. This document exists to make the decision explicit and to guide future feature design.

## Key Decisions

1. **Single-instance by design, not by v1.0 deferral** — Permanent posture, not "we'll add multi-instance in v2.0." Matches Plex, Jellyfin, Emby architecture; matches the deployment target (single-node consumer hardware); avoids the distributed-coordination cost that no realistic user needs.
2. **HA via infrastructure, not application clustering** — Container restart, VM migration, or Kubernetes StatefulSet rescheduling handle the actual user concern (server-down recovery) without multi-instance complexity.
3. **Active-passive HA is a possible future, not a v1.0 commitment** — Documented path exists (lockfile-based leader election + PG-backed state), but is not planned. If demand materializes, it's a bounded 2–3 month project.
4. **Prefer PG-backed state for new features when trivial** — Features like user preferences, notification state, and scheduled task state already use PG because it's the natural design. Continue this pattern.
5. **Use in-memory state when PG is genuinely costly** — Ephemeral/high-frequency state (transcodes, WebAuthn challenges, rate limiters, FS watcher) stays in-memory. This is correct design, not a "limitation to fix later."
6. **Reject speculative distributed-by-default design** — Adding Redis/NATS/PG state for features that work fine in-memory is premature complexity. The migration is bounded and reversible if demand ever materializes.
7. **Lockfile enforces single-instance at startup** — Already implemented; clear error message on dual-startup attempt.
8. **Kubernetes deployments MUST use `replicas: 1` StatefulSet** — Documented in Phase 15 deployment guide; operator runs k8s for orchestration convenience, not for multi-instance capacity.
9. **Document multi-instance implications in code** — Docstrings on in-memory state components note the assumption (`/// MULTI-INSTANCE: ...`). Makes future audit easier.
10. **The four hard problems (transcodes, FS watcher, scheduler, rate limiters) are the migration blockers** — Everything else is trivially per-instance or trivially PG/Redis-backed. If multi-instance demand ever materializes, the work is bounded to these four components.

## Relationship to Other Domains

| Document | Relationship |
|---|---|
| [CONFIGURATION.md](../operations/CONFIGURATION.md) | 14-step startup sequence includes lockfile acquisition as step 5 — enforces single-instance |
| [MEMORY.md](MEMORY.md) | Memory budgets assume single-instance; PG pool sizing (`max_connections(20)`) is single-instance-tuned |
| [DOCKER_DEPLOYMENT.md](../operations/DOCKER_DEPLOYMENT.md) | Phase 15 single-container deployment topology is the canonical single-instance pattern |
| [BACKUP_RECOVERY.md](../operations/BACKUP_RECOVERY.md) | Backup assumes single-writer; WAL-G continuous archiving is single-instance |
| [LOGGING_OBSERVABILITY.md](../operations/LOGGING_OBSERVABILITY.md) | Per-instance metrics; no distributed tracing needed |
| [REAL_TIME_PUSH.md](REAL_TIME_PUSH.md) | SSE EventBus is in-memory (`DashMap<Uuid, broadcast::Sender>`); multi-instance would require Redis pub/sub |
| [SEARCH.md](SEARCH.md) | PG FTS works single-instance; Meilisearch sidecar is per-Duskcue-instance (loopback) |
| [BUILD_ORDER.md](../../BUILD_ORDER.md) | Phase 15 (Docker & Deployment) — single-container canonical topology |

## Research Sources

- **[Emby community: Load balancing more than one Emby server](https://emby.media/community/topic/141361-load-balancing-more-than-one-emby-server/)** — Definitive peer-server discussion. Emby team and community consensus: single-instance by design; SQLite locks block multi-instance; Emby Terms of Service explicitly describe "personal media server" use case. Multi-instance is contrary to design intent.
- **[Scaling Nextcloud to 1,000+ Users](https://www.massivegrid.com/blog/nextcloud-scale-1000-users-enterprise-architecture/)** — Reference architecture for what horizontal scaling actually requires (load balancer, multiple app servers, PostgreSQL primary + replicas, Redis Sentinel cluster, dedicated cron worker, S3 object storage, PgBouncer). Illustrates the disproportionate complexity relative to a media server use case.
- **[Jellyfin hardware acceleration docs](https://jellyfin.org/docs/general/administration/hardware-acceleration/)** — Confirms Jellyfin single-server architecture (forked from Emby, same SQLite foundation)
- **[Vaultwarden architecture](https://github.com/dani-garcia/vaultwarden)** — Bitwarden-compatible server; single-instance by design, like Duskcue
- **[Home Assistant architecture](https://www.home-assistant.io/docs/configuration/)** — Single-instance Python process; community consensus that multi-instance is unsupported
- **[governor crate: distributed rate limiting](https://docs.rs/governor)** — Documents the in-memory state store assumption and the (theoretical) path to Redis-backed rate limiting
- **[PostgreSQL advisory locks](https://www.postgresql.org/docs/18/functions-admin.html#FUNCTIONS-ADVISORY-LOCKS)** — `pg_try_advisory_lock` for hypothetical future leader election in active-passive HA mode
