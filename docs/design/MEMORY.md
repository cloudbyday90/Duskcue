# Memory Management

## Overview

Strategy for runtime memory management: memory budgets, memory pressure watchdog, PostgreSQL connection pool tuning, crash recovery, and process lifecycle. Designed for NAS hardware (2 GB ARM) through dedicated servers (16 GB+ x86_64).

CPU management is a separate domain documented in [CPU.md](CPU.md).

## Crate Selection

| Crate | Version | Role |
|---|---|---|
| `tokio` | 1.52 | Multi-threaded async runtime with I/O + time drivers |
| `tokio_util` | 0.7 | `CancellationToken` for graceful shutdown; `TaskTracker` for waiting on tasks |
| `sysinfo` | 0.34 | Cross-platform system metrics (memory %, per-process memory) |
| `mimalloc` | 0.1 | Global memory allocator — replaces musl/glibc default |

### Why These Crates

| Crate | Strength | Limitation | Our Use |
|---|---|---|---|
| **tokio** | De facto async runtime; work-stealing scheduler; process spawning | Complex configuration surface | All async I/O, task spawning, subprocess management |
| **tokio_util::CancellationToken** | Tree-structured cancellation; cloneable; zero-cost when not cancelled | Requires cooperative task design | Signal all long-lived tasks to shut down gracefully |
| **tokio_util::TaskTracker** | Wait for all tracked tasks to complete; `close()` + `wait()` pattern | None significant | Ensure clean shutdown — no task left behind |
| **sysinfo** | Cross-platform (Linux, macOS, Windows, ARM); no C deps; refresh-on-demand | Slightly stale data (refresh interval) | Memory watchdog, Prometheus gauges |
| **mimalloc** | Lowest RSS of any mainstream allocator; thread-local sharding; security hardening | Slightly higher latency on very large (>64KB) allocations | Global allocator replacing musl/glibc default |

### Rejected Alternatives

| Crate | Why Not |
|---|---|
| **async-std** | Less ecosystem support; tokio is the standard for Axum |
| **smol** | Minimalist; insufficient ecosystem for a production Duskcue |
| **heim** | Unmaintained; sysinfo covers all platforms |
| **psutil (via bindings)** | Python-centric; sysinfo is pure Rust |
| **tikv-jemallocator** | Higher base RSS (~9MB vs ~4MB); was abandoned in 2025 (revived March 2026 but uncertain); our workload is primarily small allocations where mimalloc excels |
| **tcmalloc** | Not commonly used in Rust ecosystem; Google-maintained but no official Rust crate |

---

## Global Allocator: mimalloc v3

### Decision

Replace the system default allocator (musl on Alpine, ptmalloc on glibc) with **mimalloc v3** via the `mimalloc` crate.

```rust
#[cfg(not(target_env = "msvc"))]
use mimalloc::MiMalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
```

### Why Not the Default Allocator

| Platform | Default | Problem for Long-Running Servers |
|---|---|---|
| **Alpine (musl)** | musl malloc | Described as "beyond abysmal" for long-running services — high fragmentation, poor multi-threaded performance |
| **Linux (glibc)** | ptmalloc | Well-known fragmentation in multi-threaded contexts; arena-based allocation wastes memory; cannot release memory back to OS effectively |
| **macOS** | libmalloc | Decent but higher overhead per allocation vs mimalloc |

Reddit r/rust (March 2026): multiple developers confirm switching from system allocator to mimalloc v3 fixed creeping RSS and reduced CPU usage from 30-40% to ~10% under fragmentation conditions.

### Why mimalloc Over jemalloc

| Factor | mimalloc v3 | tikv-jemallocator |
|---|---|---|
| Base RSS overhead | ~4 MB | ~9 MB |
| Small allocation performance | Better | Worse |
| Large allocation (>64KB) performance | Acceptable | Better |
| Memory fragmentation | Lowest | Good |
| Security | Guard pages, encrypted free lists, randomization | Minimal hardening |
| Maintenance status | Active (Microsoft Research) | Revived March 2026 (Meta); was abandoned 2025 |
| Profiling | `MIMALLOC_SHOW_STATS` env var | `jeprof` full heap profiling |
| ARM64 support | First-class | First-class |
| musl compatibility | Yes | Yes |

**Our workload:** Primarily small, short-lived allocations (HTTP requests, JSON parsing, DB rows, cache entries). FFmpeg subprocess memory is outside the Rust allocator entirely (separate process). mimalloc excels at exactly this pattern.

### Runtime Stats

Set `MIMALLOC_SHOW_STATS=1` environment variable to dump allocator statistics on exit. Useful for debugging memory issues:

```
heap stats:        peak      total      current   block   total#
reserved:       1.0 GiB    1.0 GiB    1.0 GiB
committed:     11.1 MiB   11.5 MiB   11.1 MiB
...
process: user: 4.010 s, system: 0.030 s, faults: 94, rss: 6.7 MiB, commit: 11.1 MiB
```

### Debug Endpoint

An admin-only endpoint `GET /api/v1/debug/alloc` exposes current allocator stats. Only available when the `debug-alloc` Cargo feature is enabled (not in production builds by default).

---

## TLS Crypto Backend: ring (not aws-lc-rs)

### Decision

The workspace uses `ring` as the crypto backend for `rustls`, `tokio-rustls`, and `reqwest` instead of the default `aws-lc-rs`.

### Why Not aws-lc-rs

`aws-lc-sys` (the native dependency of `aws-lc-rs`) requires:
- NASM assembler (not installed by default on Windows)
- CMake build system
- MSVC environment variables (`VCINSTALLDIR`, `LIB`, `INCLUDE`) set in the shell

On a standard Windows development environment (Rust via scoop, MSVC Build Tools installed but not in a VS Developer Command Prompt), `aws-lc-sys` compilation fails with missing NASM and unset MSVC variables. This blocks all development on Windows.

### Why ring

| Factor | ring | aws-lc-rs |
|---|---|---|
| Build prerequisites | None (precompiled assembly) | NASM + CMake + MSVC env |
| Windows support | Builds out of the box | Requires VS Developer Command Prompt |
| Performance | Excellent (BoringSSL-derived) | Slightly better on modern hardware |
| Security audit | Extensive (BoringSSL lineage) | Extensive (AWS-maintained) |
| rustls default | Was default before 0.23 | Default since rustls 0.23 |
| HMAC signing | Same library (`ring` 0.17) | Different library |
| ARM64 support | First-class | First-class |

### Implementation

All three crates configured in workspace `Cargo.toml`:

```toml
rustls = { version = "0.23", default-features = false, features = ["ring", "std", "tls12"] }
tokio-rustls = { version = "0.26", default-features = false, features = ["ring"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
ring = "0.17"
```

Using the same `ring` library for both TLS and HMAC signing (SECURITY.md) keeps the crypto dependency tree minimal.

---

## cgroup-Aware Memory Detection

### Problem

In Docker with `memory: 4G`, `sysinfo` reports the host's total memory (e.g. 32 GB), not the container's 4 GB limit. The watchdog's 80%/90% thresholds fire against the wrong baseline, making them useless.

### Solution

On Linux, detect cgroup v2 memory limits and use them when available. Fall back to sysinfo host memory when not in a container.

```rust
fn detect_memory_limit() -> (u64, u64) {
    #[cfg(target_os = "linux")]
    {
        if let Ok(max) = fs::read_to_string("/sys/fs/cgroup/memory.max") {
            let limit = max.trim();
            if limit != "max" {
                if let Ok(limit_bytes) = limit.parse::<u64>() {
                    if let Ok(current) = fs::read_to_string("/sys/fs/cgroup/memory.current") {
                        if let Ok(used) = current.trim().parse::<u64>() {
                            return (used, limit_bytes);
                        }
                    }
                }
            }
        }
    }

    let sys = System::new_all();
    (sys.used_memory(), sys.total_memory())
}
```

### Detection Logic

```
On startup:
  1. Check if /sys/fs/cgroup/memory.max exists (Linux cgroup v2)
  2. If exists and value is numeric (not "max"):
     → Running in a container with memory limit
     → Use cgroup memory.current / memory.max for watchdog
  3. If not exists or value is "max":
     → Running bare metal or unlimited container
     → Fall back to sysinfo host memory
  4. Store detection result for watchdog loop
```

### Where It's Used

- Memory watchdog (60s interval) — uses detected limits for threshold checks
- Health check endpoint — reports container-aware memory usage
- Prometheus gauges — `system.memory.usage_bytes` and `system.memory.total_bytes` reflect container limits
- Startup log — prints whether using cgroup or host memory detection

---

## PSI: Pressure Stall Information

### What

cgroup v2 exposes Pressure Stall Information (PSI) files that report real-time memory pressure. Unlike interval-based polling, PSI is kernel-maintained and provides sub-second granularity.

### Files

| File | Location | Format |
|---|---|---|
| `memory.pressure` | `/sys/fs/cgroup/memory.pressure` | `some avg10=5.20 avg60=3.10 avg300=2.50 total=12345678` |
| `cpu.pressure` | `/sys/fs/cgroup/cpu.pressure` | Same format |
| `io.pressure` | `/sys/fs/cgroup/io.pressure` | Same format |

- `some` = at least one task stalled (partial contention)
- `full` = all non-idle tasks stalled (full contention — worst case)
- `avg10/60/300` = percentage over 10s / 60s / 300s windows

### Integration

PSI is read in the watchdog loop alongside memory/CPU checks (Linux + Docker only). Not available on bare metal without cgroups or on macOS/Windows.

```
Every 60 seconds (in watchdog loop):
  if /sys/fs/cgroup/memory.pressure exists:
    read "full avg10" value
    gauge!("system.memory.pressure_stall_percent", avg10)
    if avg10 > 20:
      WARN: "Memory pressure stall detected: {avg10}% of tasks stalled"
```

No action thresholds initially — PSI metrics are informational for admin dashboards. Admins can set Grafana alerts on `system.memory.pressure_stall_percent`.

---

## Graceful Shutdown

### Signal Handling

The server handles two termination signals plus an internal shutdown trigger:

| Signal | Source | Behavior |
|---|---|---|
| `SIGINT` | Ctrl+C in terminal | Begins graceful shutdown |
| `SIGTERM` | `docker stop`, Kubernetes, systemd | Begins graceful shutdown |
| Internal | Fatal error, admin API command | Begins graceful shutdown |

**Double-signal protection**: If a second signal is received during shutdown, the server forces an immediate `std::process::exit(1)`. This prevents indefinite hangs — the admin can always press Ctrl+C twice or run `docker stop` twice to force exit.

### Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Shutdown Triggers                                       │
│                                                          │
│  SIGINT (Ctrl+C) / SIGTERM (Docker stop)                 │
│  Internal shutdown (fatal error, admin command)           │
│                                                          │
│  tokio::select! {                                        │
│      _ = signal::ctrl_c() => { ... }                     │
│      _ = signal_unix(SignalKind::terminate()) => { ... } │
│      _ = internal_shutdown.recv() => { ... }             │
│  }                                                       │
│                                                          │
│  Second signal → std::process::exit(1)                   │
└───────────────┬──────────────────────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────────────────────┐
│  Phase 1: Signal (immediate)                             │
│                                                          │
│  1. CancellationToken::cancel()                          │
│     → All long-lived tasks begin cooperative shutdown    │
│  2. Stop accepting new HTTP connections                  │
│     → Axum graceful shutdown via with_graceful_shutdown  │
│  3. Stop scheduled task runner                           │
│     → No new tasks dispatched                            │
└───────────────┬──────────────────────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────────────────────┐
│  Phase 2: Drain (up to 30s)                              │
│                                                          │
│  1. Gracefully terminate all active FFmpeg processes     │
│     → tokio-process-tools GracefulShutdown (SIGTERM/     │
│       CTRL_BREAK → 10s grace → SIGKILL fallback)        │
│     → Each ProcessHandle terminated via terminate()      │
│  2. Wait for in-flight HTTP requests to complete         │
│  3. Flush log buffers (WorkerGuard drop)                 │
│  4. TaskTracker::wait() for all background tasks         │
└───────────────┬──────────────────────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────────────────────┐
│  Phase 3: Cleanup (up to 90s)                            │
│                                                          │
│  1. Close PostgreSQL connection pool                     │
│     → sqlx Pool::close() drains in-flight queries       │
│  2. Stop embedded PostgreSQL (embedded mode only)        │
│     → pg_ctl -m fast stop                               │
│     → Fast mode: aborts open transactions, performs      │
│       shutdown checkpoint, ensures clean next startup    │
│     → Up to 60s for PG to flush dirty pages              │
│  3. Remove startup lockfile                              │
│  4. Runtime::shutdown_timeout(30s) as hard deadline      │
│     → If anything still alive, force terminate           │
└─────────────────────────────────────────────────────────┘
```

**Total shutdown budget**: ~120s (30s drain + 90s cleanup). Docker `stop_grace_period` must be set to at least 120s to avoid SIGKILL during PG checkpoint. See [DOCKER_DEPLOYMENT.md](../operations/DOCKER_DEPLOYMENT.md).

### PostgreSQL Shutdown Modes

| Signal | Mode | Checkpoint? | Next Startup |
|---|---|---|---|
| `SIGTERM` | Smart | Yes (after all sessions exit) | Clean — no WAL replay |
| `SIGINT` | **Fast** (our choice) | **Yes** (immediately aborts transactions) | **Clean — no WAL replay** |
| `SIGQUIT` | Immediate | No | WAL replay required on next startup |
| `SIGKILL` | Kill | No | WAL replay required; shared memory leaks |

We use **Fast mode** (`pg_ctl -m fast stop`) because:
1. It performs a **shutdown checkpoint** — all dirty pages flushed to disk, WAL consistent
2. Next startup is instant — no WAL replay needed
3. It aborts in-flight transactions immediately — doesn't wait for sessions to finish
4. All committed transactions are durable (fsync + synchronous_commit guarantee this)

### Shutdown Code Pattern

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::signal;
use tokio::signal::unix::SignalKind;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

static SHUTDOWN_STARTED: AtomicBool = AtomicBool::new(false);

#[tokio::main]
async fn main() {
    let shutdown = CancellationToken::new();
    let tracker = TaskTracker::new();
    let server_shutdown = shutdown.clone();

    tracker.spawn(async move {
        tokio::select! {
            result = axum::serve(listener, app) => {
                result.expect("server error");
            }
            _ = server_shutdown.cancelled() => {}
        }
    });

    let mut sigterm = signal::unix::signal(SignalKind::terminate()).unwrap();

    tokio::select! {
        _ = signal::ctrl_c() => {
            tracing::info!("Received SIGINT (Ctrl+C)");
        }
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM");
        }
    }

    if SHUTDOWN_STARTED.swap(true, Ordering::SeqCst) {
        tracing::warn!("Second shutdown signal received, forcing exit");
        std::process::exit(1);
    }

    shutdown.cancel();

    tracing::info!("Phase 2: Draining in-flight requests (up to 30s)");
    tracker.close();
    let drain_result = tokio::time::timeout(
        Duration::from_secs(30),
        tracker.wait(),
    ).await;

    if drain_result.is_err() {
        tracing::warn!("Phase 2: Drain timed out after 30s");
    }

    tracing::info!("Phase 3: Cleanup (up to 90s)");
    {
        let close_result = tokio::time::timeout(Duration::from_secs(60), async {
            pool.close().await;
        })
        .await;
        if close_result.is_err() {
            tracing::warn!("Phase 3: PG pool close timed out after 60s");
        }
    }
}
```

**Implementation note:** The server task uses `tokio::select!` with `server_shutdown.cancelled()` instead of `with_graceful_shutdown(shutdown.cancelled())` because `CancellationToken::cancelled()` borrows `&self`, producing a non-`'static` future that cannot be passed to `with_graceful_shutdown` (which requires `F: Future<Output = ()> + Send + 'static`). Wrapping the server in `tokio::select!` inside the spawned task achieves the same effect — when the token is cancelled, the server future is dropped, stopping the accept loop.

---

## FFmpeg Subprocess Lifecycle

Managed via `tokio-process-tools` v0.11.2 — a correctness-focused async subprocess library that provides explicit control over process groups, graceful shutdown, bounded output, zombie prevention, and process naming. Every lifecycle decision is a required argument at the spawn call site, making it visible in code review.

### Spawn Configuration

```rust
use tokio::process::Command;
use tokio_process_tools::{
    AutoName, CollectionOverflowBehavior, DEFAULT_MAX_BUFFERED_CHUNKS,
    DEFAULT_OUTPUT_EOF_TIMEOUT, DEFAULT_READ_CHUNK_SIZE, GracefulShutdown,
    LineCollectionOptions, LineOutputOptions, LineParsingOptions, NumBytesExt,
    Process,
};

let mut process = Process::new(
    Command::new("ffmpeg")
        .args(["-progress", "pipe:1", "-i", input_path, "-c:v", "libx264", ...])
        .stdin(std::process::Stdio::null())
)
.name(format!("transcode-{}", session_id))
.stdout_and_stderr(|stream| {
    stream
        .single_subscriber()
        .lossy_without_backpressure()
        .replay_last_bytes(64.kilobytes())
        .read_chunk_size(DEFAULT_READ_CHUNK_SIZE)
        .max_buffered_chunks(DEFAULT_MAX_BUFFERED_CHUNKS)
})
.spawn()
.expect("failed to spawn FFmpeg");
```

### Key Decisions

| Setting | Value | Rationale |
|---|---|---|
| Library | `tokio-process-tools` v0.11.2 | Handles process groups, graceful shutdown, bounded output, zombie prevention. Replaces custom `TranscodeManager` boilerplate with a correctness-focused API. MSRV 1.89.0 |
| Process name | `transcode-{session_id}` | Human-readable identifier in logs, error messages, and metrics. Debuggable at 3am |
| stdout consumer | Progress parser | FFmpeg `-progress pipe:1` writes structured `key=value` lines to stdout. Parsed by dedicated consumer for real-time progress tracking |
| stderr consumer | Log collector | FFmpeg log output (errors, warnings, info). Collected for debugging and crash diagnostics. Bounded to prevent OOM from verbose FFmpeg output |
| Replay policy | `replay_last_bytes(64 KB)` | Closes spawn-to-attach timing gap. If FFmpeg writes output before the consumer attaches, the replay buffer delivers it retroactively |
| Backpressure | `lossy_without_backpressure()` | Never blocks FFmpeg's stdout/stderr writes. If our consumer falls behind, chunks are dropped rather than stalling the transcode. FFmpeg output is non-critical — dropping is acceptable |
| Buffer bounds | Default (16 KB reads, 128 chunks) | Bounded memory consumption. Total per-stream buffer: ~2 MB. Prevents OOM from unexpectedly verbose FFmpeg output |
| Panic on drop | Built-in (armed handle) | Dropping a `ProcessHandle` without calling `wait()`, `terminate()`, `kill()`, or `must_not_be_terminated()` **panics**. Loud failure mode catches leaked children in development |
| `terminate_on_drop` | Configured per-process | Long-lived transcode processes opt in: on drop, gracefully terminates via `GracefulShutdown`. Requires multithreaded Tokio runtime |

FFmpeg CPU-specific settings (threading, priority, affinity) are documented in [CPU.md](CPU.md).

### Graceful Shutdown

```rust
let shutdown = GracefulShutdown::builder()
    .unix_sigterm(Duration::from_secs(10))
    .windows_ctrl_break(Duration::from_secs(10))
    .build();
```

On cancel / session end / stall detection:
1. `SIGTERM` (Unix) / `CTRL_BREAK_EVENT` (Windows) sent to FFmpeg's **process group** (not just PID)
2. Wait up to 10s for clean exit (FFmpeg flushes current segment, closes file handles)
3. If still alive → `SIGKILL` (implicit fallback in `GracefulShutdown`)
4. Handle disarmed on successful exit — no panic

Platform differences handled by `GracefulShutdown`:
- **Unix**: `process_group(0)` always applied by library; signals dispatched via `killpg` to entire group
- **Windows**: `CREATE_NEW_PROCESS_GROUP` + anonymous Job Object; `CTRL_BREAK_EVENT` → `TerminateJobObject` escalation
- **Signal choice**: `SIGTERM` is the standard shutdown signal (what Docker, systemd, Kubernetes all use)

### Progress Parsing (FFmpeg `-progress pipe:1`)

FFmpeg's `-progress pipe:1` global option writes machine-readable `key=value` lines to stdout approximately every second and at encoding end. This is separate from stderr (logs/errors), providing two clean, distinct channels.

**Progress keys (per burst):**

| Key | Type | Example | Description |
|---|---|---|---|
| `frame` | int | `frame=12345` | Total frames processed |
| `fps` | float | `fps=24.5` | Current encoding speed |
| `bitrate` | string | `bitrate=6234.5kbits/s` | Current output bitrate |
| `total_size` | int | `total_size=12345678` | Total bytes written |
| `out_time_ms` | int | `out_time_ms=57080000` | Output time in **microseconds** |
| `speed` | string | `speed=1.25x` | Encoding speed ratio |
| `progress` | string | `progress=continue` / `progress=end` | Sentinel — always last key in burst |

**Channel separation:**

| Channel | FD | Content | Consumer |
|---|---|---|---|
| Progress | stdout (`pipe:1`) | Structured `key=value` lines | `ProgressParser` consumer — updates `TranscodeSession` state |
| Logs | stderr | FFmpeg log lines (`[info]`, `[warning]`, `[error]`) | `LogCollector` consumer — bounded tail for crash diagnostics |

**Parsing approach:**

```rust
let progress_consumer = stdout_stream.consume(
    ProgressParser::new(|update: ProgressUpdate| {
        session.update_progress(update);
    })
);
```

Each line is split on `=`. Lines with known keys update the session state. The `progress=end` sentinel marks transcode completion. Lines that don't match the `key=value` pattern are ignored (defensive parsing — FFmpeg output is not always predictable).

**Why not `-progress pipe:2` (stderr) or `pipe:3` (extra FD)?**
- `pipe:2` would mix progress with log lines, requiring fragile heuristics to distinguish them
- `pipe:3` requires pre-creating an extra pipe FD — more complexity, no benefit over `pipe:1`
- `pipe:1` is cleanest: stdout is free because HLS output goes to files (not stdout), so the channel is exclusively for progress

### Zombie Avoidance

| Concern | Mitigation |
|---|---|
| Child exits but parent hasn't called `wait()` | tokio-process-tools panics on drop of armed handle — loud failure catches this in dev. Production uses `terminate_on_drop` for graceful cleanup |
| Parent crashes mid-transcode | On restart: scan `/cache/hls/` for orphaned session dirs; mark `play_sessions` as `abandoned`. tmpfs `/data/transcode` is auto-purged |
| Spawn-to-attach timing gap | Replay policy (`replay_last_bytes`) buffers output during the window between spawn and consumer attachment |
| Multiple FFmpeg children | Each tracked as a named `ProcessHandle` in `TranscodeManager`. Names like `transcode-{session_id}` appear in all error messages |
| Grandchildren / detached processes | FFmpeg is spawned in its own process group. Any grandchild that calls `setsid`/`setpgid` is by definition outside the group. Docker's `init: true` (tini as PID 1) handles these |

### Concurrent Transcode Limits

| Limit | Mechanism | Default |
|---|---|---|
| Max concurrent transcodes | `Semaphore` in `TranscodeManager` | 2 |
| Per-user transcode limit | Streaming policy (`streaming_policies` table) | Per policy |
| Memory guard | Reject new if system memory > 85% | Configurable |
| CPU guard | Reject new if system CPU > 90% | See [CPU.md](CPU.md) |

### FFmpeg Per-Process Sandboxing

FFmpeg processes are sandboxed with two complementary Linux security mechanisms. Both are applied in the child process via `Command::pre_exec()` (between fork and exec) so only FFmpeg is restricted — not the parent server. Both gracefully degrade on unsupported platforms.

See [SECURITY.md](../security/SECURITY.md) for full design and policy details.

**Landlock LSM** (`landlock` crate) — filesystem sandboxing:
- Restricts FFmpeg to read media files + write transcode segments only
- Read-only: `/data/media/`, codec/font paths (`/usr/`, `/lib/`, `/etc/`)
- Read-write: `/cache/transcodes/{session_id}/` only
- Unprivileged — no root required
- Linux 5.13+ (Alpine 3.22 ships kernel 6.x — satisfied)
- Silently skipped on unsupported kernels (log warning, continue)

**Seccomp-BPF** (`seccompiler` crate) — syscall filtering:
- Allow-list of ~40 syscalls FFmpeg needs; deny everything else
- Blocks dangerous syscalls: `execve`, `fork`, `clone` (beyond FFmpeg needs), `ptrace`, `mount`, `chroot`
- Applied via `seccompiler::apply_filter()` in `pre_exec`
- Linux only, x86_64 + aarch64 (our targets)
- Feature-gated `#[cfg(target_os = "linux")]` — not compiled on Windows/macOS

---

## PostgreSQL Connection Pool

### Pool Configuration

```rust
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

let pool = PgPoolOptions::new()
    .max_connections(20)
    .min_connections(2)
    .acquire_timeout(Duration::from_secs(5))
    .max_lifetime(Duration::from_secs(30 * 60))
    .idle_timeout(Duration::from_secs(10 * 60))
    .test_before_acquire(true)
    .after_connect(|conn, _meta| Box::pin(async move {
        sqlx::Executor::execute(conn, "SET application_name = 'duskcue'").await?;
        Ok(())
    }))
    .connect(&database_url)
    .await?;
```

### Setting Rationale

| Setting | Value | Default | Rationale |
|---|---|---|---|
| `max_connections` | 20 | No default (required) | Duskcue is not a high-QPS web service. 20 handles peak concurrent API + background tasks + scheduled tasks. Leaves room in PG's default `max_connections=100` for maintenance connections |
| `min_connections` | 2 | 0 | Keep 2 warm connections for low-latency queries on an otherwise idle server. Eliminates connection-establishment latency on first request after idle |
| `acquire_timeout` | 5s | 30s | Fail fast if pool is exhausted. Clients get a quick 503 rather than hanging for 30s |
| `max_lifetime` | 30 min | None (infinite) | sqlx docs explicitly recommend periodic retirement to prevent PG-side memory bloat (parse trees, query metadata caches, thread-local storage). 30 min balances rotation overhead vs memory hygiene |
| `idle_timeout` | 10 min | None (infinite) | Close idle connections to free PG backend memory. On a quiet server, only `min_connections` (2) remain |
| `test_before_acquire` | true | true | Detect stale connections after network blips. Cost: one round-trip per checkout. Worth it for reliability |
| `after_connect` | `SET application_name` | — | Makes PG monitoring (`pg_stat_activity`) identifiable. See which connections belong to our server |

### Pool Monitoring

sqlx exposes pool metrics via `Pool::size()`, `Pool::num_idle()`, and `Pool::is_closed()`. These are exported as Prometheus gauges:

| Metric | Type | Description |
|---|---|---|
| `db.pool.connections.active` | gauge | Currently checked-out connections |
| `db.pool.connections.idle` | gauge | Connections sitting in the pool |
| `db.pool.connections.max` | gauge | Configured max_connections |
| `db.pool.acquire.duration` | histogram | Time to acquire a connection from pool |
| `db.pool.acquire.timeouts_total` | counter | Number of acquire_timeout exceeded events |

---

## Health Checks & Memory Watchdog

### Health Check Endpoint

`GET /health` — used by Docker HEALTHCHECK, load balancers, and monitoring.

#### Response Tiers

| Status | Meaning | Response |
|---|---|---|
| `200` | All subsystems healthy | `{ "status": "healthy", "checks": { "database": "ok", "disk": "ok", "ffmpeg": "ok" } }` |
| `200` with warnings | Degraded but functional | `{ "status": "degraded", "checks": { "database": "ok", "disk": "ok", "ffmpeg": "unavailable" } }` |
| `503` | Critical subsystem down | `{ "status": "unhealthy", "checks": { "database": "failed", "disk": "ok", "ffmpeg": "ok" } }` |

#### Subsystem Checks

| Check | What | Timeout | Failure |
|---|---|---|---|
| Database | `SELECT 1` via pool | 2s | 503 — server cannot function without DB |
| Disk space | Check `/data`, `/cache` not at threshold | 500ms | 503 if critical (>95%), 200 with warning if warning (>90%) |
| FFmpeg | `ffmpeg -version` subprocess | 5s | 200 with `degraded` — transcoding disabled, direct play still works |

### Watchdog Tasks

Background monitoring tasks that run on intervals independent of scheduled tasks. Implemented as `tokio::spawn` loops with `tokio::time::interval`. CPU watchdog is documented in [CPU.md](CPU.md).

| Monitor | Interval | Warning Threshold | Critical Threshold | Action on Critical |
|---|---|---|---|---|
| System memory | 60s | 80% used | 90% used | Emergency cache eviction; reject new transcodes; `server_alert` notification |
| Zombie processes | 60s | Any zombies detected | >5 zombies | Log warning; tokio-process-tools panic-on-drop catches most; tokio best-effort reaping as fallback |
| Database connectivity | 30s | Acquire timeout hit | Pool exhausted | Log error; `server_alert` notification; health check returns 503 |
| Stale transcode sessions | 60s | No progress update for 5 min (stdout `-progress pipe:1`) | No progress for 10 min | Graceful shutdown via tokio-process-tools → mark session `abandoned` |
| Orphaned HLS segments | 5 min | Sessions expired >1h ago | Segments >4 GB total | Delete orphaned session dirs |

### Memory Watchdog Flow

```
Every 60 seconds:
  1. Refresh sysinfo::System
  2. let mem_percent = sys.used_memory() / sys.total_memory() * 100

  if mem_percent > 90%:
    CRITICAL
    → reject all new transcodes (PLAY_003 + SYS_011)
    → emergency cache eviction (clear HLS cache, clear image cache)
    → server_alert notification to admins
    → metrics: counter!("system.memory.pressure_events", "level" => "critical")

  elif mem_percent > 80%:
    WARNING
    → reject new transcodes if active > 1
    → begin LRU cache eviction (storyboards first, then images)
    → metrics: counter!("system.memory.pressure_events", "level" => "warning")

  elif mem_percent > 70%:
    INFO
    → log system.memory.usage_bytes gauge
    → no action
```

---

## Crash Recovery

### Crash-Only Hardening Principle

Every component must survive an unclean shutdown without data corruption. Graceful shutdown is a best-effort optimization for faster restarts and cleaner resource cleanup — it is not a correctness requirement. The system must be correct even when shutdown never happens (power loss, OOM kill, `SIGKILL`, kernel panic).

**Why this matters**: In Docker, `docker kill` sends SIGKILL (uncatchable). On a NAS, power outages skip shutdown entirely. The kernel OOM killer sends SIGKILL without warning. Any of these can happen at any time.

### PostgreSQL Crash Recovery Guarantees

PostgreSQL provides **zero data loss for committed transactions** after any crash, assuming:

| Setting | Default | Our Setting | Guarantee |
|---|---|---|---|
| `fsync` | `on` | `on` | Every WAL write hits durable storage before commit returns |
| `synchronous_commit` | `on` | `on` | Every `COMMIT` waits for WAL flush to disk |
| `full_page_writes` | `on` | `on` | Torn-page protection — full page image written on first modification after checkpoint |
| `wal_level` | `replica` | `replica` | Sufficient WAL for any recovery operation |
| `data_checksums` | `off` | `on` | Detects silent page corruption at read time |

**After an unclean shutdown (no checkpoint):**
1. PostgreSQL starts automatically (Docker restart policy, systemd, or manual)
2. The startup process reads `pg_control` to find the last checkpoint LSN
3. Replays WAL forward from that checkpoint (automatic, no admin action needed)
4. Performs an "end-of-recovery" checkpoint
5. Server begins accepting connections

**Recovery time**: Typically seconds to minutes depending on `max_wal_size` and checkpoint interval. The `pg_isready` wait in the entrypoint blocks until recovery completes — the server never connects to a mid-recovery database.

**What survives any crash**:
- All committed transactions — zero data loss, guaranteed by ACID + WAL
- All watch history, resume positions, user accounts, server config
- All migration state (DDL is transactional in PostgreSQL)

**What does NOT survive a crash**:
- In-flight HTTP requests (client must retry — idempotent)
- Active transcode sessions (ephemeral — tmpfs, auto-purged)
- Uncommitted transactions (by definition — they were not committed)

### Recovery Scenarios

| Scenario | Detection | Recovery |
|---|---|---|
| Server crash / OOM kill / `SIGKILL` | Docker restart (`unless-stopped`) | On restart: PostgreSQL auto-replays WAL; scan for orphaned transcodes; mark abandoned sessions; tmpfs purged automatically |
| PG crash | sqlx pool `test_before_acquire` fails | Auto-reconnect via pool; health check returns 503 during outage; `server_alert` notification |
| Power loss (no shutdown) | Hardware restart | PostgreSQL WAL replay on startup; all committed data intact; orphaned transcodes cleaned up |
| FFmpeg hung | Watchdog: no progress update for 5-10 min (via `-progress pipe:1` stdout) | Graceful shutdown via `tokio-process-tools` → mark session `abandoned` |
| Disk full | `disk_space_check` scheduled task | `server_alert` notification; admin action required (except transcode overflow which auto-kills) |
| Transcode corrupt | FFmpeg exit code != 0 | Client retries with lower quality rung or direct play fallback |

### Startup Recovery Sequence

```
1. Acquire startup lockfile (prevent concurrent instances)
2. Connect to PostgreSQL (or start embedded PG)
   → If unclean shutdown: PG auto-replays WAL (pg_isready waits)
3. Validate PostgreSQL settings (fsync, data_checksums, wal_level)
   → Warn if settings don't match expectations (never block startup)
4. Run pending migrations
5. Scan /cache/hls/ for orphaned session directories
   → Compare against play_sessions table (state = 'active')
   → Mark sessions with no matching directory as 'abandoned'
   → Delete directories with no matching session (orphaned from crash)
6. Scan /data/transcode/ (tmpfs — already empty, skip)
7. Start watchdog tasks (memory, CPU, zombie, stale transcode)
8. Start HTTP listener
9. Ready
```

---

## Startup Lockfile

### Purpose

Prevents concurrent server instances from sharing the same PostgreSQL data directory. Without this, two instances could corrupt the database by both writing to the same PGDATA.

### Mechanism

```
On startup:
  1. Check for /data/postgres/postmaster.pid
     → If exists and PG process is running: FAIL — another instance is active
     → If exists but PG process is not running: stale PID (crash), remove and continue
  2. Check for /data/.duskcue.lock
     → If exists: read PID, check if process is alive
     → If process alive: FAIL — "Another instance is already running (PID {pid})"
     → If process dead: stale lock (crash), remove and continue
  3. Create /data/.duskcue.lock with current PID
  4. Continue startup

On shutdown (Phase 3):
  Remove /data/.duskcue.lock
```

### Concurrent Instance Protection

The lockfile prevents two scenarios:
1. **Docker user starts two containers with the same `/data` volume** — second instance fails with clear message
2. **Systemd + Docker both trying to start** — second instance fails

PostgreSQL's own `postmaster.pid` provides additional protection at the database level — PG refuses to start if another postmaster has the same PGDATA.

---

## PostgreSQL Settings Validation

### Purpose

Detect misconfigured PostgreSQL settings that compromise data defensibility. This is a startup check, not a blocking gate — the server warns but continues if settings don't match expectations.

### Checked Settings

| Setting | Expected | Warning Message |
|---|---|---|
| `fsync` | `on` | "fsync is disabled — committed transactions may be lost on crash. Set fsync=on in postgresql.conf." |
| `full_page_writes` | `on` | "full_page_writes is disabled — torn pages may cause corruption after crash. Set full_page_writes=on." |
| `data_checksums` | `on` | "data_checksums is disabled — silent corruption will not be detected. Reinitialize with initdb --data-checksums." |
| `wal_level` | `replica` or higher | "wal_level is '{actual}' — PITR and WAL-G backups will not work. Set wal_level=replica." |

### Implementation

```sql
SELECT name, setting
FROM pg_settings
WHERE name IN ('fsync', 'full_page_writes', 'data_checksums', 'wal_level');
```

Results are checked against expected values. Mismatches log a `WARN` with the specific setting and remediation. Settings are checked once at startup (after DB connection) and not re-checked during runtime — they require a PG restart to change.

**For embedded PostgreSQL**: These settings are configured by the entrypoint script and should never be wrong. The validation catches manual `postgresql.conf` edits by advanced users.

**For external PostgreSQL**: The DBA may have different requirements. The warning is informational — the server continues regardless.

---

## Memory Budget

### Per-Subsystem Estimates

| Subsystem | Typical | Peak | Notes |
|---|---|---|---|
| Rust binary (base) | 20 MB | 50 MB | Axum router, config cache, static data |
| Tokio runtime | 10 MB | 30 MB | Worker threads (2 MB stack * CPU count), blocking pool |
| PG connection pool | 5 MB | 20 MB | Rust-side buffers for 20 connections |
| FFmpeg subprocess | 100 MB | 500 MB | Per transcode; 4K with deinterlace hits 500 MB |
| HLS segment cache | Configurable | — | Evicted by CACHE_STORAGE.md policy |
| Image / storyboard cache | Configurable | — | Evicted by CACHE_STORAGE.md policy |
| sysinfo refresh | 2 MB | 5 MB | Periodic system metrics snapshot |
| Tracing / metrics buffers | 5 MB | 15 MB | Non-blocking writer, Prometheus registry |
| Embedded PostgreSQL | 128 MB shared_buffers | 256 MB | Configurable in `postgresql.conf`; `work_mem` per connection |
| **Total (idle, no transcode)** | **~175 MB** | **~350 MB** | Server serving API requests, no active playback |
| **Per concurrent transcode** | **+100 MB** | **+500 MB** | FFmpeg process memory; varies by resolution and codec |

### Docker Resource Recommendations

| Hardware Profile | Memory Limit | Max Concurrent Transcodes | Notes |
|---|---|---|---|
| NAS (2 GB RAM, ARM) | 1.5 GB | 1 | Shared_buffers 64 MB; min_connections 1 |
| NAS (4 GB RAM, ARM) | 3 GB | 2 | Shared_buffers 128 MB |
| Desktop (8 GB RAM) | 4 GB | 2-3 | Shared_buffers 128 MB |
| Server (16 GB+ RAM) | 8 GB | 4+ | Shared_buffers 256 MB |

These are guidelines. The admin configures `max_concurrent_transcodes` via `server_config.resource_limits` JSONB.

---

## ResourceLimits Configuration

### Rust Struct

```rust
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ResourceLimitsConfig {
    pub max_concurrent_transcodes: u32,
    pub transcode_mem_threshold_percent: u8,
    pub ffmpeg_idle_timeout_secs: u64,
    pub ffmpeg_shutdown_grace_secs: u64,
    pub watchdog_interval_secs: u64,
    pub memory_warning_percent: u8,
    pub memory_critical_percent: u8,
    pub stale_session_timeout_secs: u64,
}

impl Default for ResourceLimitsConfig {
    fn default() -> Self {
        Self {
            max_concurrent_transcodes: 2,
            transcode_mem_threshold_percent: 85,
            ffmpeg_idle_timeout_secs: 300,
            ffmpeg_shutdown_grace_secs: 10,
            watchdog_interval_secs: 60,
            memory_warning_percent: 80,
            memory_critical_percent: 90,
            stale_session_timeout_secs: 600,
        }
    }
}
```

CPU-specific limits (`transcode_cpu_threshold_percent`) are in `server_config.cpu` JSONB — see [CPU.md](CPU.md).

### JSONB Example

Stored in `server_config.resource_limits`:

```json
{
    "max_concurrent_transcodes": 2,
    "transcode_mem_threshold_percent": 85,
    "ffmpeg_idle_timeout_secs": 300,
    "ffmpeg_shutdown_grace_secs": 10,
    "watchdog_interval_secs": 60,
    "memory_warning_percent": 80,
    "memory_critical_percent": 90,
    "stale_session_timeout_secs": 600
}
```

### Field Reference

| Field | Type | Default | Description |
|---|---|---|---|
| `max_concurrent_transcodes` | u32 | 2 | Maximum concurrent FFmpeg transcode processes. New requests get `PLAY_003` when exceeded |
| `transcode_mem_threshold_percent` | u8 | 85 | Reject new transcodes when system memory usage exceeds this percentage |
| `ffmpeg_idle_timeout_secs` | u64 | 300 | Kill FFmpeg process if no progress output (stdout `-progress pipe:1`) for this duration |
| `ffmpeg_shutdown_grace_secs` | u64 | 10 | Seconds to wait after SIGTERM before SIGKILL (configured via `GracefulShutdown` builder in tokio-process-tools) |
| `watchdog_interval_secs` | u64 | 60 | How often the memory/CPU/zombie watchdog runs |
| `memory_warning_percent` | u8 | 80 | Begin cache eviction at this memory threshold |
| `memory_critical_percent` | u8 | 90 | Emergency cache eviction + reject all transcodes at this threshold |
| `stale_session_timeout_secs` | u64 | 600 | Mark transcode session as abandoned if no client activity for this duration |

---

## Prometheus Metrics

### Memory Metrics

| Metric | Type | Labels | Description |
|---|---|---|---|
| `system.memory.usage_bytes` | gauge | | Total system memory in use |
| `system.memory.total_bytes` | gauge | | Total system physical memory |
| `system.memory.usage_percent` | gauge | | Memory usage as percentage |
| `system.memory.pressure_events` | counter | `level` (warning, critical) | Memory pressure events triggered |
| `system.memory.pressure_stall_percent` | gauge | | PSI memory stall percentage (avg10, full) — cgroup v2 / Docker only |
| `transcode.rejections_total` | counter | `reason` (capacity, memory) | New transcodes rejected due to memory limits |
| `transcode.kill_total` | counter | `reason` (idle, shutdown, stall, orphan) | FFmpeg processes killed via tokio-process-tools graceful shutdown |
| `transcode.zombie_reaped_total` | counter | | Zombie child processes reaped |
| `transcode.progress_updates_total` | counter | | FFmpeg `-progress pipe:1` updates received |
| `transcode.sandbox violations_total` | counter | `layer` (landlock, seccomp) | FFmpeg sandbox violations detected (Landlock access denied / seccomp SIGSYS) |
| `watchdog.stale_sessions_killed` | counter | | Transcode sessions killed by stale watchdog |

CPU metrics are documented in [CPU.md](CPU.md).

These supplement the existing metrics in LOGGING_OBSERVABILITY.md:

| Existing Metric | Category |
|---|---|
| `transcode.jobs.active` | Transcode |
| `transcode.jobs.duration` | Transcode |
| `system.uptime_seconds` | System |
| `db.pool.connections.active` | Database |
| `db.pool.connections.idle` | Database |

---

## Integration with Existing Systems

### Configuration (CONFIGURATION.md)

`ResourceLimitsConfig` is part of `RuntimeConfig`, loaded from `server_config.resource_limits` JSONB. Admin changes via `PUT /api/v1/server/config` trigger cache reload.

### Error Handling (ERROR_HANDLING.md)

Memory limit errors return standard error codes:

| Condition | Error Code | HTTP |
|---|---|---|
| Max concurrent transcodes reached | `PLAY_003` | 503 |
| System memory pressure — transcode rejected | `SYS_011` | 503 |

### Logging (LOGGING_OBSERVABILITY.md)

- Watchdog events logged at `WARN` (pressure) and `ERROR` (critical) with structured fields
- FFmpeg kill events logged at `INFO` with session ID, reason, exit status
- Prometheus metrics emitted by watchdog loop

### Database (DATABASE.md)

- `server_config.resource_limits` JSONB column
- Scheduled task: `transcode_health_check` (every 60s, timeout 30s)

### Docker (DOCKER_DEPLOYMENT.md)

- Resource recommendation table (see [DOCKER_DEPLOYMENT.md](../operations/DOCKER_DEPLOYMENT.md))
- `deploy.resources.limits` example in compose file

### Streaming (STREAMING.md)

- `PLAY_003` already covers transcode capacity reached
- Resource limits checked before FFmpeg spawn in the streaming decision flow
- FFmpeg SIGTERM-first lifecycle replaces naive `kill_on_drop`

---

## Research Sources

- Tokio 1.52.3 Runtime shutdown: https://docs.rs/tokio/1.52.3/tokio/runtime/struct.Runtime.html
- Tokio 1.52.3 Command (process): https://docs.rs/tokio/1.52.3/tokio/process/struct.Command.html
- Tokio official "Graceful Shutdown" guide: https://tokio.rs/tokio/topics/shutdown
- tokio_util CancellationToken: https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html
- tokio_util TaskTracker: https://docs.rs/tokio-util/latest/tokio_util/task/task_tracker/struct.TaskTracker.html
- sqlx 0.9.0 PoolOptions: https://docs.rs/sqlx/0.9.0/sqlx/pool/struct.PoolOptions.html
- sysinfo crate: https://docs.rs/sysinfo/latest/sysinfo/
- mimalloc crate: https://crates.rs/crates/mimalloc
- mimalloc v3 benchmark comparison (Reddit r/rust, March 2026): https://www.reddit.com/r/rust/comments/1riwbqv/
- Allocator comparison: throughput, latency, memory overhead (dev.to, December 2025): https://dev.to/frosnerd/libmalloc-jemalloc-tcmalloc-mimalloc-exploring-different-memory-allocators-4lp3
- tikv-jemallocator crate: https://crates.io/crates/tikv-jemallocator
- Docker cgroup v2 memory management: https://rawkode.academy/read/cgroups-from-chaos-to-control
- Linux kernel cgroup v2 documentation: https://docs.kernel.org/admin-guide/cgroup-v2.html
- PSI (Pressure Stall Information) in cgroup v2: https://docs.kernel.org/accounting/psi.html
- Rust memory profiling tools (users.rust-lang.org, March 2026): https://users.rust-lang.org/t/production-grade-runtime-profiling-of-rust-applications/139230
