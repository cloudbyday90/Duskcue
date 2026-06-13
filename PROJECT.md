# Duskcue

A self-hosted media streaming platform — a modern, open-source alternative to Plex.

## Overview

Duskcue is a server+client media streaming platform designed for personal and family use. It supports both local and remote streaming with full transcoding capabilities.

## Project Scale & Documentation Scope

This project is an **open-source, self-hosted application** intended first for **single-admin or small-family deployments**, not a commercial hosted service.

That means the default design target is:

- one server admin
- local-first or simple remote access
- straightforward release and backup flows
- secure defaults without enterprise-style operational overhead

Documentation should stay right-sized to that scope:

- **Baseline**: required for a normal self-hosted install or for the core product to be safe and usable
- **Advanced**: useful for internet exposure, stronger supply-chain separation, or privileged automation, but optional for many users and not required for the first working release
- **Future / Enterprise-style**: multi-admin governance, delegated operations, and larger-team controls that should not drive the core architecture unless the project actually grows into that model

When a topic mainly exists because of self-hosted runners, privileged CI, or multi-actor incident response, it should be treated as **advanced guidance**, not as a baseline requirement for the product.

The repo-wide labeling pattern for those topics is defined in [DOCUMENTATION_SCOPE_LABELING.md](docs/governance/DOCUMENTATION_SCOPE_LABELING.md).
The repo-wide ownership and review-cadence pattern for those topics is defined in [DOCUMENTATION_REVIEW_OWNERSHIP.md](docs/governance/DOCUMENTATION_REVIEW_OWNERSHIP.md).
The current audit for which advanced docs stay active versus deferred is defined in [ADVANCED_DOC_DEFER_POLICY.md](docs/governance/ADVANCED_DOC_DEFER_POLICY.md).
The lightweight checklist for bringing deferred advanced guidance back to active use is defined in [ADVANCED_DOC_ACTIVATION_CHECKLIST.md](docs/governance/ADVANCED_DOC_ACTIVATION_CHECKLIST.md).
The maintainer-facing index for the current trusted-automation document set is [TRUSTED_AUTOMATION_INDEX.md](docs/ci/TRUSTED_AUTOMATION_INDEX.md).
The release-blocking re-review policy for trusted automation is defined in [TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md](docs/ci/TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md).
The manual validation policy for release-blocking trusted-automation changes is defined in [TRUSTED_AUTOMATION_MANUAL_VALIDATION.md](docs/ci/TRUSTED_AUTOMATION_MANUAL_VALIDATION.md).

## Media Types

- Movies
- TV Shows

_(Music, photos, and books to be considered in future phases.)_

## Architecture

**Server+Client Model**

- **Server** — Manages the media library, metadata, transcoding, and serves content to clients via a REST/WebSocket API. Must run on **Windows, Linux, macOS, Synology NAS, and Docker**.
- **Client(s)** — Consumes media from the server. Multiple client types supported across all major platforms.

### Platform Targeting: x86_64 + ARM64

The server binary targets two architectures: **x86_64 (amd64)** and **ARM64 (aarch64)**. No 32-bit, no ARMv7.

**Rationale:**
- Rust cross-compilation is first-class — zero code changes, only build config
- Apple Silicon Macs (M1/M2/M3/M4) are ARM64 — native desktop app via Tauri
- ARM SBCs and NAS devices (Odroid, NanoPi, Zimaboard) are growing
- AWS Graviton and Oracle ARM cloud instances are cost-effective
- Multi-arch Docker images are industry standard (single manifest, auto-selects correct variant)
- Minimal additional effort — CI config + Dockerfile only, no application code changes

**Build targets:**

| Component | x86_64 | ARM64 |
|---|---|---|
| Docker image | `linux/amd64` on Alpine | `linux/arm64` on Alpine |
| Server binary | `x86_64-unknown-linux-musl` | `aarch64-unknown-linux-musl` |
| macOS desktop | `x86_64-apple-darwin` | `aarch64-apple-darwin` |
| Windows desktop | `x86_64-pc-windows-msvc` | N/A (no ARM64 Windows desktop market) |
| CI strategy | Native runner | Cross-compilation via `cargo-zigbuild` |
| Transcoding | NVENC / VAAPI / AMF / VideoToolbox | VideoToolbox (Apple) / V4L2 M2M (Pi/Rockchip) / software fallback |

**Cross-compilation toolchain:**
- `cargo-zigbuild` 0.22 — uses Zig as the C linker; fast; no Docker-in-Docker; handles OpenSSL and other C deps
- `cross` 0.2.5 — fallback; uses Docker containers with pre-built cross-compilation toolchains

**Docker base image: Alpine Linux** — minimal footprint (~5MB base), musl libc (statically linked Rust binary), fast builds, small attack surface. Multi-arch Alpine images are first-class supported.

**ARM64 hardware transcoding:**
| Platform | Acceleration | Notes |
|---|---|---|
| Apple Silicon | VideoToolbox (H.264, H.265, ProRes) | Excellent FFmpeg support |
| Raspberry Pi 4/5 | V4L2 M2M (H.264 only) | Limited but functional |
| Rockchip RK3588 | MPP (H.264, H.265, AV1) | Via `rkmpp` in FFmpeg |
| AWS Graviton | None | Software transcoding fallback |
| Generic ARM64 | None | Software transcoding fallback |

The existing `transcoding.hardware_accel` config already handles architecture detection and graceful fallback — no code changes needed.

### Client Platform Strategy

A **shared API + hybrid client** approach — the server exposes a unified API that all clients consume. No single framework covers all platforms (especially TVs), so clients are split by platform family:

**Tier 1 — Shared Web Client**
- Primary web-based client built with **Svelte + SvelteKit**
- Wrapped with **Tauri 2** for desktop (Windows, macOS, Linux)
- This is the reference client and highest priority

**Tier 2 — Mobile**
- **Flutter** for shared Android + iOS codebase
- Native (Kotlin / Swift) if playback performance demands it

**Tier 3 — Smart TVs**
- **Android TV / Google TV** — Native (Leanback) or React Native
- **Apple TV (tvOS)** — Swift native app
- **Samsung (Tizen)** — Tizen Web SDK
- **LG (webOS)** — webOS Web SDK

> TV apps require platform-specific development. This is unavoidable due to vendor SDKs.

## Product Identity & Client UI

Product naming and branding direction are documented in [NAME_BRANDING.md](docs/branding/NAME_BRANDING.md).

Baseline client look and feel, navigation language, and reusable UI surfaces are documented in [UI_FOUNDATIONS.md](docs/branding/UI_FOUNDATIONS.md).

### API Layer

- **REST API** — Core CRUD operations, media management, user auth
- **WebSocket** — Real-time events (transcode progress, playback sessions, notifications)
- **HLS / DASH** — Adaptive streaming protocols for video delivery

## Core Features

| Feature | Description |
|---|---|
| **Transcoding** | On-the-fly and offline transcoding to support varying bandwidth and device capabilities |
| **Metadata Scraping** | Automatic fetching of metadata, artwork, and ratings from external sources (TMDB primary; TVDB/Fanart.tv/OMDb supplementary) |
| **Offline Downloads** | Download media to client devices for offline playback |
| **Live TV / DVR** | Stream and record live television with an EPG guide |
| **Subtitle Support** | Embedded, external, fetched (OpenSubtitles/SubDL), OCR (PaddleOCR) for PGS/VobSub, voice activity alignment, server-side sync |

## Streaming & Transcoding

Streaming and transcoding design is documented in [STREAMING.md](docs/design/STREAMING.md).

**Key decisions:**
- **HLS with fMP4 segments** — sole adaptive streaming protocol; 6-second segments; hls.js fallback
- **Three-tier streaming decision** — Direct Play → Direct Stream/Remux → Transcode
- **Streaming policy system** — reusable `streaming_policies` table with per-user overrides; 5 seeded defaults
- **ABR ladder** — 4 rungs (480p/720p/1080p/1080p HQ); smart rung selection

## Video Formats

Video codec, container, HDR, bit depth, and color format support is documented in [VIDEO_FORMATS.md](docs/design/VIDEO_FORMATS.md).

**Key decisions:**
- **All major codecs supported as source** — never rejects a file based on codec
- **H.264 is universal transcode target** — HEVC Main 10 for HDR; AV1 for future
- **All HDR formats supported** — HDR10, HDR10+, DV Profiles 5/7/8.1/8.4, HLG
- **Dolby Vision is profile-aware** — client-side fallback for Profile 7; never transcode just for DV

## Audio Formats

Audio codec, channel configuration, spatial audio, and transcode targets are documented in [AUDIO_FORMATS.md](docs/design/AUDIO_FORMATS.md).

**Key decisions:**
- **Passthrough-first for lossless audio** — TrueHD, DTS-HD MA, FLAC passed through unmodified
- **AAC is the universal transcode target** — E-AC-3 for surround; Opus for web/mobile
- **Same-codec downmix preferred** — TrueHD 7.1 → TrueHD 5.1 over cross-codec transcode
- **Spatial audio metadata preserved** — Atmos/DTS:X passthrough untouched

## Subtitle Domain

Subtitle discovery, conversion, synchronization, fetching, and delivery is documented in [SUBTITLES.md](docs/design/SUBTITLES.md).

**Key decisions:**
- **Server does all the work** — clients never perform sync, conversion, or offset calculation
- **PaddleOCR for PGS/VobSub OCR** — one-time background task; cached forever
- **Three-tier sync correction** — server-side offset, FPS adjustment, voice activity alignment
- **WebVTT sidecar for HLS** — simple, universally supported

## Metadata Providers

Metadata provider integration — provider selection, tier architecture, API key management, data flow, caching, rate limiting, attribution — is documented in [METADATA_PROVIDERS.md](docs/design/METADATA_PROVIDERS.md).

**Key decisions:**
- **TMDB v3 is the sole primary provider** — movies AND TV; deepest coverage; free with attribution; ~40 req/s; `append_to_response` batching; daily ID exports; CC BY 4.0 images; OpenAPI 3.0 spec
- **Three-tier provider architecture** — Tier 1: TMDB (primary, always active); Tier 2: TVDB/Fanart.tv/OMDb (supplementary, opt-in with user API key); Tier 3: SubDL/OpenSubtitles (subtitles, opt-in)
- **Trait-based provider abstraction** — `MetadataProvider`, `ArtworkProvider`, `RatingsProvider` traits; `ProviderRegistry` orchestrates; providers are interchangeable
- **API keys encrypted at rest** — AES-256-GCM with `encrypted:` prefix; admin-only access; masked in API responses; validated on save
- **Graceful degradation** — primary failure defers to next refresh; supplementary failures are silent; items always get TMDB data at minimum
- **Rate limit management** — per-provider `governor` token buckets; exponential backoff on 429; daily budget tracking for OMDb/SubDL
- **SubDL as primary subtitle source** — TMDB ID search; 2,000 req/day free; SRT/ASS/VTT; OpenSubtitles secondary due to paywall
- **4 new LIB error codes** (LIB_011–LIB_014) for provider failures

## Segment Detection

Intro, credit, recap, and preview detection is documented in [SEGMENT_DETECTION.md](docs/design/SEGMENT_DETECTION.md).

**Key decisions:**
- **4-method pipeline** — chapter markers → chromaprint → black frame → silence
- **Safety first** — 2s padding, duration caps, confidence scoring ≥0.7
- **Skip button by default** — auto-skip is opt-in per-user

## Storyboards (Seek Preview Thumbnails)

Seek-preview thumbnail grids documented in [STORYBOARDS.md](docs/design/STORYBOARDS.md).

**Key decisions:**
- **WebVTT + WebP sprite sheets** — keyframe-only mode (100x faster); adaptive interval
- **~3-5 MB per movie** — background scheduled task; incremental generation

## Build Order

The implementation sequence is documented in [BUILD_ORDER.md](BUILD_ORDER.md). Covers: 16 phases from project scaffolding through desktop/mobile clients, dependency-ordered, each referencing authoritative design documents and specific tasks.

**Phase summary:**
1. Project Scaffolding → 2. Database Schema → 3. Core Server Infrastructure → 4. Auth & Users → 5. Libraries & Media → 6. Metadata Providers → 7. Streaming & Playback → 8. Web Client Core → 9-14. Independent domains (Subtitles, Segments, Analytics, Kometa, System Ops, Migration) → 15. Docker & Deployment → 16. Desktop & Mobile Clients

## Tech Stack

**Server:** **Rust** with **Axum** — **PostgreSQL 18**, **FFmpeg** (with hardware acceleration) — x86_64 + ARM64, Docker on **Alpine Linux**

**Web Client:** **Svelte + SvelteKit**

**Desktop:** **Tauri 2** (wrapping web client)

**Mobile:** **Flutter** (Android + iOS)

**TVs:** Platform-specific native apps

## Project Structure

Full project structure is documented in [PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md). Covers: Cargo workspace layout, Rust server domain module pattern, SvelteKit web client conventions, Tauri desktop wrapper, Flutter mobile client, and development workflow.

**Key decisions:**
- **Monorepo + Cargo workspace** — single repo, single `Cargo.lock`, atomic cross-crate changes
- **`crates/types`** — zero-dependency shared types (only serde/uuid/chrono) used by server, Tauri, and potentially Flutter via `flutter_rust_bridge`
- **Domain modules** — each domain (auth, libraries, media, playback, etc.) is a self-contained directory with handlers, service, error, and types
- **Five-file pattern** — `mod.rs` (router), `handlers.rs` (HTTP), `service.rs` (logic), `error.rs` (domain errors), `types.rs` (DTOs)
- **SvelteKit + ESM** — `"type": "module"` in package.json; barrel exports; named function per endpoint; no direct `fetch` in components
- **API client layer** — modular service files (`libraries.js`, `media.js`, etc.) wrapping a shared `core.js` transport; stores consume API functions

## Cache & Storage Strategy

Cache and storage strategy is documented in [CACHE_STORAGE.md](docs/operations/CACHE_STORAGE.md). Covers: three-tier storage architecture (Hot/Warm/Cold), per-cache-type size limits with eviction policies, disk space monitoring with thresholds, admin storage dashboard, configurable per-type paths.

**Key decisions:**
- **Three-tier storage** — Hot (SSD: `/data` for DB, config, metadata), Warm (SSD/HDD: `/cache` for storyboards, images, HLS, search), Cold (HDD: `/media` for source files, read-only)
- **Per-cache-type size limits** — transcode tmpfs (2 GB, TTL on session end), HLS (4 GB, TTL + orphan cleanup), storyboards (no limit by default, LRU + size cap when configured), images (2 GB, LRU), search (auto-managed)
- **Disk space monitoring** — 90% threshold on `/data` and `/cache`; 80% on transcode tmpfs (kills oldest session); `disk_space_check` scheduled task every 30 minutes; `server_alert` notifications to admins
- **LRU eviction for storyboards** — least-recently-played items evicted first; priority retention for items played in last 30 days; auto-regeneration on next playback or scheduled task
- **No automatic deletion on threshold** — disk space alerts notify admins; only transcode overflow auto-kills sessions; admin decides whether to adjust limits or add storage
- **Configurable per-type paths** — power users can relocate storyboards to HDD while keeping database on NVMe via `server_config.storage` JSONB overrides
- **Plex/Jellyfin problems solved** — Plex's monolithic data directory SSD overflow is the #1 support issue; Jellyfin's transcode directory grows unbounded (GitHub #3929, open since 2020); our per-type limits and eviction prevent both

## Database Maintenance & Bloat Management

Database bloat prevention and maintenance strategy is documented in [DATABASE_MAINTENANCE.md](docs/operations/DATABASE_MAINTENANCE.md). Covers: per-table autovacuum tuning, HOT updates via fillfactor, REINDEX CONCURRENTLY scheduled task, ANALYZE for partitioned parent tables, partition retention, admin-configurable thresholds, bloat risk analysis per table.

**Key decisions:**
- **Per-table autovacuum tuning** — `user_item_data` at 2% scale factor (critical — `resume_position_ms` updated every 10-30s during playback); `server_config` at threshold=1 (single row); `user_sessions`, `users`, `scheduled_tasks`, `media_items` at 5%; global defaults tightened from 20% to 10%
- **Fillfactor 85 on `user_item_data`** — enables HOT updates for playback heartbeats; `resume_position_ms` updates never touch indexed columns, so ~95%+ of updates become HOT (heap-only, no index bloat); 15% page reservation = ~150 KB extra for a 10K-row table
- **REINDEX CONCURRENTLY scheduled task** — weekly Sunday 02:00; only targets indexes with >30% bloat above 10 MB; zero downtime; configurable threshold and schedule via `server_config.maintenance` JSONB
- **ANALYZE partitioned parent tables** — daily at 03:00; `play_sessions`, `play_events`, `audit_log`; required because autovacuum does not process partitioned parent tables (PG docs mandate manual ANALYZE)
- **pg_repack / pg_squeeze rejected** — unnecessary at our scale (1-50 users); adds Docker complexity (`shared_preload_libraries`, client binaries, replication slots); proper autovacuum + fillfactor + REINDEX CONCURRENTLY prevents bloat proactively
- **`pgstattuple` extension** — used for accurate bloat measurement by the reindex_maintenance task
- **`server_config.maintenance` JSONB** — configurable autovacuum tuning toggle, reindex schedule/threshold, partition retention per table, parent table ANALYZE toggle; exposed via admin UI as toggles, sliders, and dropdowns
- **Unnecessary trim guards** — storyboard LRU priority retention (30-day warm / 90-day cold); image cache 2 GB generous default; `user_item_data` HOT updates prevent accumulation; REINDEX threshold prevents reindexing healthy indexes

## Library Organization

Library folder structure, file naming conventions, sub-folder design, and the identification pipeline are documented in [LIBRARY_ORGANIZATION.md](docs/design/LIBRARY_ORGANIZATION.md). Covers: parent container → library sub-folder → item hierarchy, per-movie and per-series folder structures, metadata provider ID tags, multiple editions, extras folders, episode naming patterns, transparent container detection, smart collections, access control per library, multi-path libraries, Docker volume mapping, and the 5-layer cascading identification pipeline. Multi-edition support (theatrical, director's cut, extended, etc.) is documented separately in [MULTI_EDITION.md](docs/design/MULTI_EDITION.md).

**Key decisions:**
- **Each sub-folder is a library** — each `libraries` row maps to a user-chosen sub-folder (e.g. `/media/TV Shows/Kids TV/`, `/media/Movies/Family Films/`); the parent container (`TV Shows/`, `Movies/`) is a filesystem convention only, not a database entity
- **Library names are agnostic** — the server assigns no semantic meaning to library names; "Kids TV", "Documentaries", "Family Films" are user-chosen labels
- **5-layer identification pipeline** — solves Plex's #1 pain point (wrong/missed matches): Layer 1: `.media-match` sidecar file (100% exact, zero filename dependency); Layer 2: NFO file (100% exact, cross-platform); Layer 3: provider ID tag in folder/filename (100% exact); Layer 4: structured filename parse + API search with confidence scoring (~90%); Layer 5: unmatched queue + admin interactive fix
- **`.media-match` file format** — new universal sidecar format (inspired by Plex `.plexmatch` but works for movies AND TV, human-writable key-value, not XML); overrides folder/filename for identification; supports per-episode mapping and pattern matching
- **Auto-write `.media-match` on manual fix** — when admin confirms a match via the UI, server writes `.media-match` to the item's folder so corrections survive re-scans, migrations, and database rebuilds
- **Per-item folders** — each movie in its own `Movie Name (Year)/` folder; each series in its own `Show Name (Year)/` folder with `Season XX/` sub-folders; flat files supported as fallback
- **Metadata provider ID tags** — `{tmdb-XXX}`, `{imdb-ttXXX}`, `{tvdb-XXX}` in folder/filenames for guaranteed correct matching; interoperable with Plex, Jellyfin, and Emby conventions
- **Specials support** — `S00EXX` or `SPXX` consistently across all identification layers; `Season 00` / `Specials` folders; `airsbefore_season`/`airsbefore_episode` from metadata providers for in-season placement
- **Transparent container detection** — scanner recurses through arbitrary intermediate directories that don't match item-level folder patterns
- **Smart collections** — metadata-driven categorization (genre, rating, franchise, director) independent of folder structure; auto-maintained
- **Multi-path libraries** — `library_paths` table allows one library to span multiple directories or disks
- **Reserved folder names** — `Season XX`, `Specials`, extras folders (`behind the scenes`, `trailers`, etc.), `VIDEO_TS`, `BDMV` have special scanner meaning; all other folder names are transparent

## Memory Management

Memory management strategy is documented in [MEMORY.md](docs/design/MEMORY.md). Covers: Tokio runtime configuration, graceful shutdown (CancellationToken + TaskTracker, SIGINT + SIGTERM handling, double-signal protection, 3-phase shutdown with PG Fast mode checkpoint), FFmpeg subprocess lifecycle (SIGTERM-first), PostgreSQL connection pool tuning (sqlx PoolOptions), health checks, watchdogs (memory, zombie, stale sessions), crash recovery (crash-only hardening, PostgreSQL WAL replay guarantees, zero data loss for committed transactions), startup lockfile (prevents concurrent instances), PostgreSQL settings validation (fsync, data_checksums, wal_level), memory budgets per subsystem, resource limits configuration, **mimalloc v3 global allocator** (replaces musl/glibc default; lowest RSS; critical for Alpine Docker), **cgroup-aware memory detection** (container limits vs host memory), **PSI memory pressure metrics** (cgroup v2 Pressure Stall Information for proactive monitoring), and **TLS crypto backend decision** (ring over aws-lc-rs for cross-platform build compatibility).

## CPU Management

CPU management strategy is documented in [CPU.md](docs/design/CPU.md). Covers: FFmpeg threading model, process priority (nice/ionice), ARM64 big.LITTLE handling, hardware acceleration runtime detection, software encoding optimization per architecture, CPU watchdog with thermal throttling, Docker CPU management per architecture, and CPU configuration.

**Key decisions:**
- **FFmpeg threading** — `-threads 0` (auto) by default; capped to `cores - 1` on 2-4 core ARM NAS; `thread_type=frame` for all streaming
- **Process priority** — `nice -n 10` + `ionice -c 2 -n 7` on Linux; lowers FFmpeg CPU and I/O priority so server API + DB always take precedence
- **ARM64 big.LITTLE** — OS-managed by default (Linux kernel 4.14+ is big.LITTLE-aware); optional `cpu_affinity` config to pin FFmpeg to big cores; auto-detect big cores at startup via `/proc/cpuinfo`
- **Hardware acceleration detection** — runtime probe at startup: NVIDIA (`/dev/nvidia*`), VAAPI (`/dev/dri/renderD128` + `vainfo`), VideoToolbox (macOS), RKMPP (Rockchip); cached in memory; re-detect on config reload
- **ARM64 software encoding** — x264 preferred over x265 (better NEON optimization); `veryfast` preset on SBCs, `fast`/`medium` on Apple Silicon
- **CPU watchdog** — same 60s loop as memory watchdog; 80% warning / 90% critical thresholds; rejects transcodes when CPU is high
- **Thermal throttling (ARM64)** — monitors `/sys/class/thermal/thermal_zone0/temp`; 80°C warning, 85°C critical; reduces/kills transcodes to prevent thermal damage
- **`server_config.cpu` JSONB** — 11 configurable fields: thresholds, thread count, thread type, nice/ionice, CPU affinity, HW accel auto-detect, thermal throttle temps
- **Docker CPU management** — per-architecture examples: `cpuset-cpus` for ARM SBCs, `cpu-shares` for multi-container hosts, `cpus` hard cap for VPS
- **Crate selection** — `sysinfo` 0.34 (cross-platform per-core CPU metrics) + `nix` 0.31 (Unix `sched_setaffinity` for CPU pinning)
- **Security & remote access** — Three-tier opt-in model (local/VPN/exposed); rustls for TLS; HMAC-SHA256 signed streaming URLs; BREACH mitigation (compression disabled on sensitive endpoints); timing attack resistance via ring; Cloudflare cannot be used for video (CDN TOS); documented alternatives (Tailscale/Pangolin/Rathole); security event monitoring with admin quick actions; full design in SECURITY.md
- **OS hardening** — Docker Engine >= v28.0.0 (recommended v29.4.3+ for CVE-2026-31431 mitigation); Alpine 3.22 base image; Debian 12+/Ubuntu 22.04+/AlmaLinux 9+/Rocky Linux 9+/Windows 11 23H2+ minimums; read-only OS detection at startup + 24h; full design in OS_HARDENING.md
- **API security** — OWASP API Top 10 (2023) coverage; `validator` crate input validation; BOLA prevention via service-layer ownership checks; three-type DTO pattern (Row/Request/Response); SSRF URL allowlisting; request body size limits; admin endpoint isolation; outbound API response validation; `cargo audit` + `cargo deny` + `cargo vet` + `cargo cyclonedx` in CI; SBOM per release; full design in API_SECURITY.md

## Quality Management

Quality management — device capability detection, network quality measurement, transcoding decision engine, Dolby Vision handling, HDR tone mapping, audio passthrough, subtitle strategy, version selection, and automatic quality mode — is documented in [QUALITY_MANAGEMENT.md](docs/design/QUALITY_MANAGEMENT.md). Covers: three-layer quality decision architecture, device capability profiles with empirical wizard, passive + active network measurement, transcoding decision flow, QoE metrics, client-side DV fallback, BT.2390 tone mapping, audio passthrough-first, smart subtitle strategy, intelligent version selection, and automatic quality mode.

**Key decisions:**
- **Three-layer architecture** — Layer 1: Device Capability Profile (static, what formats the device CAN play); Layer 2: Network Quality Assessment (dynamic, what bitrate the network CAN sustain); Layer 3: Transcoding Decision Engine (combines both + media file properties → direct play or transcode)
- **Capability wizard** — empirical testing that plays 5-second sample clips in different formats; solves the #1 Jellyfin community pain point (devices misreporting capabilities); results override client self-reports; cached per device model
- **Passive + active network measurement** — segment download telemetry (ongoing, zero overhead) + periodic probe downloads (every 5 min); harmonic mean of last 5 segment throughputs; validates and detects network changes
- **Network tier classification** — 6 tiers from `excellent` (>25 Mbps) to `critical` (<0.5 Mbps); used for starting rung selection, user warnings, and admin analytics
- **Transcoding decision flow** — 10-factor evaluation per stream: codec → profile/level → bit depth → resolution → HDR → container → bitrate → audio codec → audio channels → subtitle format; 6 outcomes (direct play, remux, video transcode, audio transcode, burn-in subtitles, tone-map HDR)
- **Client-side DV fallback (Plex-parity)** — `allow_client_side_dv_fallback` flag in device profiles; when DV Profile 7 file has HDR10 base layer and device supports HDR10, allow direct play (trust client's decoder); never transcode video just because of DV Profile 7; remux at most (strip DV layer via `hevc_metadata=remove_dovi=1`)
- **BT.2390 + libplacebo tone mapping** — BT.2390 is the only acceptable tone mapping algorithm (never Hable/Mobius/Reinhard); libplacebo via Vulkan preferred (best quality, GPU-accelerated); OpenCL + BT.2390 as fallback; CPU + BT.2390 as last resort; DV RPU always stripped before tone mapping
- **Audio passthrough-first** — never deprioritize TrueHD/DTS for HLS; fMP4 segments support all audio codecs (not MPEG-TS); when client reports TrueHD/DTS-HD MA support, pass through unmodified; prefer downmix within same codec over cross-codec transcode; Opus > AAC > AC3 for fallback transcode
- **Smart subtitle strategy** — three-tier: passthrough (SRT/WebVTT always, ASS if client supports) → convert (ASS→SRT, text processing only) → burn-in (PGS/VobSub only as last resort, requires video transcode); never burn in text-based subtitles
- **Intelligent version selection** — auto-select based on device + network; prefer lower-resolution source for transcoding (1080p→720p is 4x faster than 4K→720p); simple quality picker (Auto / Maximum / 4K / 1080p / 720p / 480p)
- **Automatic quality mode** — three modes: Auto (default, Netflix-like adaptation), Maximum (always best quality), Manual (user picks resolution); Auto uses network quality history for instant quality selection on session start
- **`server_config.quality` JSONB** — 17 configurable fields: wizard toggle, probe intervals, throughput window, safety factor, transcode defaults, DV fallback, tone mapping algorithm/peak nits, audio passthrough, subtitle burn-in policy, quality mode

## Docker Deployment

Docker deployment strategy is documented in [DOCKER_DEPLOYMENT.md](docs/operations/DOCKER_DEPLOYMENT.md). Covers: hybrid embedded/external database, container architecture, volume strategy, internal directory structure, security hardening, network configuration, hardware acceleration, NAS deployment, and operational procedures.

**Key decisions:**
- **Hybrid database strategy** — embedded PostgreSQL by default (single container); external PostgreSQL optional (set `DUSKCUE_DATABASE_URL`); same binary works both ways
- **Embedded PostgreSQL** — PG18 Alpine packages inside the container; entrypoint manages init/start/stop; listens on Unix socket only (no network exposure); trust auth (local only); `data_checksums=on`; influenced by Classifarr's production-proven all-in-one pattern
- **Non-Docker embedded PG** — `postgresql_embedded` crate (theseus-rs) manages PG binaries at runtime for native Windows/macOS/Linux; zero external dependencies
- **Single container, single volume** — `/data` holds config, metadata, logs, AND PostgreSQL data; backup one volume = backup everything
- **PG Unix socket on tmpfs** — `/var/run/postgresql` as tmpfs; 3-5x faster than TCP; uid/gid/mode match runtime user
- **`read_only: true` + `no-new-privileges`** — enabled by default; all writable paths are volumes or tmpfs
- **PUID/PGID user mapping** — container runs as non-root user with configurable UID/GID
- **`cap_drop: ALL`** — only CHOWN, SETUID, SETGID added
- **`stop_grace_period: 120s`** — gives PostgreSQL time to perform shutdown checkpoint before Docker sends SIGKILL
- **tmpfs for transcode + PG socket + /tmp** — RAM-backed; no disk wear; auto-cleaned

## License

Duskcue is licensed under the [GNU Affero General Public License v3](LICENSE) (AGPL-3.0-or-later). This is the strongest OSI-approved copyleft license — it closes the SaaS loophole by requiring anyone who hosts a modified version over a network to make the source code available to users. See [COPYRIGHT.md](COPYRIGHT.md) for the copyright notice.

The Duskcue name and logo are covered by a separate [trademark policy](TRADEMARK.md).

## Code Standards

- **ES Modules** — All JavaScript/TypeScript code uses ES Modules (`import`/`export`), not CommonJS (`require`/`module.exports`)

## Metadata Overlays

Overlay compositing engine, poster badge system, text overlays, and dynamic visual content are documented in [METADATA_OVERLAYS.md](docs/design/METADATA_OVERLAYS.md). Covers: three overlay types (image, text, backdrop), canvas standards (1000×1500 posters, 1920×1080 backdrops), positioning system, groups (mutual exclusion), queues (auto-stacking), suppress rules, special text variables (resolution, ratings, runtime, codecs), condition-based filtering, compositing pipeline (pure Rust via `image` + `ab_glyph` + `resvg`), clean art management, community templates.

**Key decisions:**
- **Pure Rust compositing** — `image` crate for alpha blending, `ab_glyph` for text rendering, `resvg` for SVG templates; no Python or external dependencies
- **Standard canvas** — 1000×1500 for posters, 1920×1080 for backdrops; all artwork scaled to standard dimensions before compositing
- **Groups and queues** — groups provide mutual exclusion (highest-weight wins); queues provide auto-stacking with configurable spacing
- **Condition-based application** — JSONB filter rules per overlay definition; evaluated against `media_items` and `media_files` metadata
- **Clean art preservation** — source artwork never modified; clean backups stored separately; overlaid results cached in `/cache/images/overlays/`
- **Community templates** — JSON export/import for sharing overlay definitions; built-in template browser in admin UI
- **11 built-in default overlays** — resolution badge, audio codec, content rating, critic rating, Dolby Vision, HDR10/HDR10+, 4K HDR, episode info, versions, streaming; seeded as system overlays (can be disabled, not deleted)
- **6 OVERLAY error codes** (OVERLAY_001–OVERLAY_006)

## Collections

Static, dynamic, and smart collections with builder sources from local metadata and external APIs are documented in [COLLECTIONS.md](docs/design/COLLECTIONS.md). Covers: three collection types (static manual, dynamic builder-populated, smart filter-evaluated), 14 internal builders (genre, decade, actor, director, franchise, etc.), 13 external builders (TMDb popular/top_rated/trending, Trakt trending/popular/recommended, IMDb top 250), dynamic collection configuration, naming customization, built-in default collections, collection templates, smart filter syntax.

**Key decisions:**
- **Server-level collections** — visible to all users with library access (not user-specific like playlists)
- **14 internal builders** — genre, country, decade, content_rating, actor, director, studio, network, franchise, original_language, year, resolution, audio_codec, streaming_service
- **13 external builders** — TMDb (popular, top_rated, trending, now_playing, upcoming, collection), Trakt (trending, popular, recommended, user_lists), IMDb (top_250), custom URL
- **Dynamic collection naming** — template-based with `<<key_name>>`, `<<library_type>>`, `<<limit>>` variables; `key_name_override`, `remove_prefix`/`remove_suffix` for customization
- **Sync mode** — `sync` (add + remove based on current matches) or `append` (add only, never remove)
- **Missing item tracking** — external builder items not in local library tracked as `is_missing`; reported to admin
- **Collection templates** — JSON export/import for community sharing; built-in template browser
- **8 built-in default collections** — TMDb Popular, TMDb Top Rated, TMDb Trending, New Releases, Recently Added, Genre Collections, Decade Collections, Holiday/Seasonal
- **8 COLL error codes** (COLL_001–COLL_008)

## Poster Management

Artwork lifecycle (source → select → customize → display), multi-source artwork management, poster locking, and bulk operations are documented in [POSTER_MANAGEMENT.md](docs/design/POSTER_MANAGEMENT.md). Covers: five artwork sources (TMDb API, user upload, asset directory, community packs, overlay compositing), selection priority, poster locking, bulk operations, TMDb artwork API integration, asset directory conventions, community art packs, disk storage layout, MetadataConfig Rust struct.

**Key decisions:**
- **Five artwork sources** — TMDb (primary, auto-downloaded during scan), user upload (drag-and-drop in admin UI), asset directory (per-item custom art on disk), community packs (importable JSON + image archives), overlay compositing (transforms any source)
- **Selection priority** — locked artwork > asset directory > user upload > community > TMDb highest-voted
- **Poster locking** — prevents auto-refresh from overwriting user-selected artwork; auto-locked on upload or asset directory discovery
- **Asset directory** — `/data/assets/` with per-item folders matching library item names; auto-discovered during scan
- **TMDb image fetching** — `original` size downloaded for best quality; resized versions generated server-side; CC BY 4.0 attribution
- **MetadataConfig** — 22 configurable fields covering artwork, overlays, collections, and provider configuration (TMDB/TVDB/Fanart/OMDb) in `server_config.metadata` JSONB; includes `ProviderConfig` with `TmdbProviderConfig` and `OptionalProviderConfig` for each supplementary provider

## API Conventions

API design conventions — URL structure, versioning, pagination, rate limiting, authentication headers, filtering, async operations — are documented in [API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md). Covers: REST endpoint naming, URI prefix versioning, hybrid cursor/offset pagination, governor rate limiting tiers, session cookie + bearer token auth, CORS, 202 Accepted for async jobs, ETag conditional requests, WebSocket events, health check, OpenAPI documentation.

**Key decisions:**
- **URI prefix versioning** (`/api/v1/`) — simple, cacheable, established in PROJECT_STRUCTURE.md; self-hosted context means coordinated version bumps
- **Cursor pagination by default** — constant-time performance at any depth using UUIDv7 primary keys; offset available for small admin tables
- **governor v0.6 for rate limiting** — pure Rust, Tower-compatible, in-process (no Redis); 5 tiers (global, auth, authenticated, streaming, admin); configurable via `server_config.auth`
- **Dual auth: cookie + bearer token** — web client uses HttpOnly session cookies; mobile/desktop/API use `Authorization: Bearer` header; same session token
- **JSON-only** — `Content-Type: application/json; charset=utf-8`; RFC 9457 Problem Details for errors

## OS Hardening & Platform Requirements

Operating system hardening and platform compatibility requirements are documented in [OS_HARDENING.md](docs/operations/OS_HARDENING.md). Covers: minimum OS versions (Linux/Windows/macOS), Docker Engine minimum version (v28.0.0, recommended v29.4.3+), Alpine Linux base image strategy (pin `alpine:3.22`), OS update detection at startup and every 24h, container hardening measures, Docker Hardened Images guidance.

**Key decisions:**
- **Docker Engine minimum v28.0.0, recommended v29.4.3+** — v29.4.3 mitigates CVE-2026-31431 ("Copy Fail" kernel privilege escalation); v28+ includes 2024 runc/BuildKit fixes
- **Alpine 3.22 base image** — pin minor version (`alpine:3.22`), not patch — auto-tracks security patches; 14 default packages vs 89+ for Ubuntu
- **Linux minimums** — Debian 12+, Ubuntu 22.04 LTS+, AlmaLinux/Rocky Linux 9+, Synology DSM 7.1+
- **Windows minimums** — Windows 11 23H2 (build 22631) minimum; Windows 10 Home/Pro EOL October 2025
- **Read-only OS detection** — startup + 24h periodic; parse `/etc/os-release`, `uname -r`, Docker version; never auto-update
- **Admin dashboard warnings** — show OS version, Docker Engine version, pending updates; log at `warn!` level; never block startup
- **No new tables or error codes** — detection is runtime-only, like health checks
- **Docker Hardened Images (DHI)** — documented as advanced option; not used by default (distroless = no shell for debugging)
- **Container hardening** — already using `read_only`, `no-new-privileges`, `cap_drop: ALL`; documented additional limits (`pids_limit`, `mem_limit`, `cpus`)

## Security & Remote Access

Security and remote access architecture is documented in [SECURITY.md](docs/security/SECURITY.md). Covers: three-tier network model (local/VPN/exposed), TLS via rustls with ACME auto-cert, HMAC-SHA256 signed streaming URLs, HTTP security headers (HSTS/CSP/X-Frame-Options), HTTP compression and BREACH mitigation, timing attack resistance, WebSocket security (future), security event monitoring, FFmpeg per-process sandboxing (Landlock + seccomp), Cloudflare TOS analysis and alternatives, remote access guidance (Tailscale/Headscale/Pangolin/Rathore/WireGuard).

**Key decisions:**
- **Local-first, opt-in security** — local network has no TLS, no signed URLs, optional auth; security activates progressively when admin enables remote access
- **Three network tiers** — Local (default, HTTP, no auth required), Remote VPN (opt-in, VPN tunnel = trusted LAN), Exposed (opt-in, HTTPS + signed URLs + mandatory auth)
- **Cloudflare cannot be used for video streaming** — CDN-specific terms prohibit serving video hosted outside Cloudflare storage; 100MB upload limit, no public UDP
- **rustls for TLS** — pure Rust, memory-safe, no OpenSSL dependency; ACME (Let's Encrypt) auto-cert when exposed
- **HMAC-SHA256 signed streaming URLs** — session-bound (not IP-bound, mobile-friendly), 60s manifest TTL, 300s segment TTL, 24h key rotation with dual-key validation
- **No embedded WireGuard** — cross-platform TUN device issues (Linux/macOS/Windows/Synology); instead provide first-class setup guides for external VPNs
- **Security headers as Tower middleware** — HSTS preload, CSP (strict for exposed, relaxed for local), X-Frame-Options DENY, X-Content-Type-Options nosniff, Referrer-Policy, Permissions-Policy
- **BREACH mitigation** — HTTP compression disabled on authentication and admin endpoints (responses contain secrets); enabled on static assets and public API (no secrets); standard industry approach
- **Timing attack resistance** — all secret comparisons use `ring` constant-time operations; standard `==` used only for non-secret values (UUIDs, names); documented as deliberate choice
- **Security event monitoring** — built-in admin dashboard shows failed logins, rate limit triggers, invalid signatures, new devices; one-click actions for session revocation, key rotation, account lock; no external SIEM needed
- **WebSocket security (future)** — not currently used; requirements documented for when real-time features are added (authenticated handshake, same-origin, message limits)
- **FFmpeg per-process sandboxing** — Landlock LSM (filesystem isolation, unprivileged, Linux 5.13+) + seccomp-BPF (syscall allow-list, `seccompiler` crate); applied in child `pre_exec`; gracefully degrades on unsupported platforms
- **tokio-process-tools v0.11.2** for FFmpeg subprocess lifecycle — replaces custom boilerplate with correctness-focused API: graceful shutdown (SIGTERM/SIGKILL), bounded output, zombie prevention, process naming
- **ring 0.17 for HMAC signing** — same crypto library used by rustls internally; HMAC-SHA256 key generation and validation
- **No dedicated error codes** — security failures map to existing PLAY_005 and SYS_001 codes
- **`server_config.security` JSONB** — TLS config, stream signing config, VPN detection; no new tables needed

## API Security

Application-layer API security is documented in [API_SECURITY.md](docs/security/API_SECURITY.md). Covers: input validation via `validator` crate, BOLA prevention (object ownership checks), response DTO separation (three-type pattern), request payload limits, admin endpoint isolation, SSRF prevention (URL allowlisting), outbound API response validation, business flow abuse prevention, dependency auditing (`cargo audit` + `cargo deny` + `cargo vet` + `cargo cyclonedx`), secret scanning, error response sanitization. Mapped against OWASP Top 10:2025 and OWASP API Security Top 10 (2023).

**Key decisions:**
- **`validator` 0.20 crate** — declarative `#[derive(Validate)]` on all request DTOs; validation at deserialization boundary
- **BOLA prevention via service-layer ownership checks** — every data access validates `user_id` ownership; UUIDv7 prevents enumeration but not authorized-user-cross-access
- **Three-type pattern** — `XxxRow` (DB model, no Serialize), `XxxRequest` (Deserialize + Validate), `XxxResponse` (Serialize only safe fields); prevents mass assignment and excessive data exposure
- **SSRF prevention via URL allowlisting** — only known metadata provider domains; DNS pinning; redirect disabled; private IP ranges blocked
- **Request body size limits** — 1MB default via `RequestBodyLimitLayer`; 50MB for upload endpoints; 30s request timeout
- **Admin endpoints under `/api/v1/admin/*`** — dedicated router with `require_capability("can_manage_server")` middleware; default-deny
- **Outbound API response validation** — all Trakt/TMDb/TVDb/OpenSubtitles responses validated against schemas before processing
- **Business flow rate limits** — invite codes 5/IP/15min, device linking 10/IP/hour, re-auth 3/user/day
- **`cargo audit` + `cargo deny` + `cargo vet` + `cargo cyclonedx` in CI** — dependency vulnerability scanning, license compliance, human-reviewed crate audit trail, and SBOM generation for every release
- **No secrets in source code** — env vars only; admin config endpoint returns masked values
- **Error response sanitization** — never leak `sqlx::Error`, stack traces, file paths, or SQL to clients
- **No new tables or error codes** — all measures are application-layer patterns

## Analytics Security

Security-focused analytics — IP geolocation, impossible travel detection, trust scoring, and behavioral analysis — are documented in [ANALYTICS_SECURITY.md](docs/security/ANALYTICS_SECURITY.md). Covers: MaxMind GeoLite2 offline geolocation, Haversine-based impossible travel algorithm, 5-layer false positive suppression, notification-first automated response, GeoIP database update pipeline, trusted IP management, user location baseline tracking.

**Key decisions:**
- **MaxMind GeoLite2 City (offline MMDB)** — free, 95-99% country accuracy, fully offline (no API calls during lookups, no data sent to third parties), ~70 MB file, updated weekly via scheduled task
- **`maxminddb` 0.28 crate with `mmap` feature** — memory-mapped file access; thread-safe; hot-reload via `ArcSwap` on weekly update
- **Haversine great-circle distance + velocity threshold** — standard algorithm (same approach as Microsoft Defender, WorkOS, CrowdSec); threshold: 1,000 km/h (commercial aviation ~900 km/h + margin)
- **5-layer false positive suppression** — (1) LAN/VPN connection suppression, (2) trusted IP allowlist, (3) same-country suppression, (4) user location baseline (90-day history), (5) same-device detection
- **Notification-first, not blocking** — the system surfaces alerts in the admin dashboard; the admin decides whether to act (revoke sessions, lock account, trust IP); no automatic user blocking
- **User location history** — `user_location_history` table tracks per-user countries with session counts and timestamps; powers baseline suppression; never deleted (full history for analytics)
- **`server_config.analytics` JSONB** — 10 configurable fields: GeoIP toggle, update schedule, impossible travel toggle, velocity threshold, min distance, lookback window, same-country suppression, trusted IPs/CIDRs
- **`geoip_database_update` scheduled task** — weekly download from MaxMind; atomic file swap; hot-reload without restart; fallback to existing file on failure
- **Graceful degradation** — if MMDB file is missing, geolocation is skipped and impossible travel detection is disabled; server continues normally with a dashboard warning
- **Existing schema fully utilized** — `play_sessions.geo_*` columns, `user_trust_events` (6 rule types), `user_trust_scores` (0-100 score with decay) all designed ahead in DATABASE.md

## Vulnerability Reporting

Vulnerability reporting policy is documented in [VULNERABILITY_DISCLOSURE.md](docs/security/VULNERABILITY_DISCLOSURE.md). Covers: how to report security problems, response timeline commitments, supported versions, responsible disclosure expectations, out-of-scope items.

**Key decisions:**
- **GitHub Security Advisories** — private reporting via the repository Security tab; only maintainers see reports
- **72-hour acknowledgment** — confirmed receipt within 3 days; initial assessment within 7 days; fix target 30 days
- **Good-faith protection** — no legal action against security researchers; credit in fix release (unless anonymous)
- **Latest + previous release supported** — critical fixes backported one version; older versions unsupported

## Platform Migration

Migration of users and watch data from Plex, Jellyfin, and Emby is documented in [MIGRATIONS.md](docs/design/MIGRATIONS.md). Covers: source platform connections (Jellyfin/Emby REST API, Plex SQLite upload), user mapping via invite code display names, provider ID matching (TMDb/IMDb/TVDb), watch state import with merge strategy, admin migration wizard UI, progress tracking, error handling.

**Key decisions:**
- **Three source platforms** — Plex (SQLite DB upload), Jellyfin (REST API), Emby (REST API)
- **Invite code display names** link source users to platform users — existing `invitations.display_name` column; admin maps during wizard
- **Provider ID matching** — primary: TMDb/IMDb/TVDb IDs; fallback: title + year + type
- **Merge strategy on conflict** — `is_watched` OR, `play_count` MAX, `resume_position_ms` MAX, `last_played_at` MAX
- **Plex via SQLite upload only** — no official bulk watch history API; admin uploads `com.plexapp.plugins.library.db`; read via `rusqlite`
- **Import targets** — `user_item_data` only (watch times and resume positions); no favorites, ratings, or playlists
- **3 new tables** — `migration_sources`, `migration_user_mapping`, `migration_import_log`
- **10 MIGR error codes** (MIGR_001–MIGR_010)

## Current Implementation Status

| Phase | Status | Commit |
|---|---|---|
| Phase 1: Project Scaffolding | **Complete** | `aaedc05` |
| Phase 2: Database Schema | **Complete** | `dd3f201` |
| Phase 3: Core Server Infrastructure | **Complete** | — |
| Phase 4: Auth & Users | **Complete** | — |
| Phase 5: Libraries & Media Items | **Complete** | — |
| Phase 6: Metadata Providers | **Complete** (Tasks 1–15) | — |
| Phase 7: Streaming & Playback | **In Progress** (Tasks 1–12) | — |
| Phase 8–16 | Not started | — |

**Phase 1 delivered:** Bootable `duskcue` binary on port 48027 with `/health` endpoint, clap CLI with `DUSKCUE_` env vars, config-rs layered merge (defaults → TOML → env → CLI), mimalloc allocator, tracing-subscriber, graceful shutdown with double-signal protection, `ring` TLS backend. See [BUILD_ORDER.md](BUILD_ORDER.md) for details.

**Phase 2 delivered:** 15 migration files covering all domains from DATABASE.md — core media, trakt integration, activity analytics, playback, auth, system, cross-cutting concerns (extensions, audit, FTS), seed data, analytics security, migration domain, quality domain, overlays/collections, segments/storyboards. All migrations use idempotent patterns (`IF NOT EXISTS`, `DO $$ ... $$`). Not yet verified against a live PostgreSQL instance. See [BUILD_ORDER.md](BUILD_ORDER.md) for details.

**Phase 3 complete:** All 12 tasks done. See [BUILD_ORDER.md](BUILD_ORDER.md) Phase 3 for full details.

**Phase 4 complete:** All 11 tasks done. See [BUILD_ORDER.md](BUILD_ORDER.md) Phase 4 for full details. Auth domain with WebAuthn passkeys, invite codes, device linking (RFC 8628), re-auth codes, session management, capability-based access control with `Require<C>` trait-based generic extractor. Users domain with full CRUD (list, get, update, soft-delete). `AuthenticatedUser` extractor wired to DB-backed session validation. 12 capability marker types replace all inline `check_capability()` calls. `AdminOnly` preserved as type alias for `Require<CanManageServer>`.

**Phase 5 complete:** All 10 tasks done — libraries domain (CRUD, slug uniqueness, multi-path), media domain (five-file pattern with cursor pagination), library scanner (`workers/library_scanner.rs`, 6-phase pipeline: discover→diff→probe→identify→enrich stub→cleanup), scheduled task runner (`services/scheduler.rs`, `croner` v3 cron evaluation, 8 seeded default tasks), FS watcher (`services/fs_watcher.rs`, `notify` 8.2 + `notify-debouncer-full` 0.7), media matching service (`services/media_matching.rs`, 5-layer identification cascade with `.media-match` pattern tokens, episode overrides, season-level cascading, multi-ID provider tag extraction from both folder names and filenames), NFO parser (`services/nfo_parser.rs`, `quick-xml` 0.40 streaming StAX), provider ID tag parsing (Layer 3 — `parse_provider_id_tags()` extracts all IDs from `{tmdb-XXX}`/`[tmdbid=XXX]` formats in folder names and filenames with curly-brace priority). See [BUILD_ORDER.md](BUILD_ORDER.md) Phase 5 for details.

**Phase 6 complete (Tasks 1–15):** `ProviderRegistry` + `EnrichmentOrchestrator` in `services/metadata.rs` with 3 async traits (`MetadataProvider`, `ArtworkProvider`, `RatingsProvider`), per-provider rate limiters, and rich data types. Full `TmdbClient` in `services/tmdb_client.rs` — Bearer token auth, `reqwest::Client` with redirect disabled per SSRF hardening, `append_to_response` batching (credits+videos+external_ids+images in 1 request), search, details, find by IMDb ID, configuration caching with `ArcSwap<TmdbConfig>` for hot-reload. Artwork download service in `services/artwork_downloader.rs` — downloads TMDB images, saves to `{data_dir}/metadata/artwork/tmdb/`, inserts `artwork` table rows with deduplication. Enrichment persistence service in `services/enrichment_persistence.rs` — persists enrichment results in transactions (genre upsert, person deduplication, top-N credit linking, metadata JSONB merge). Full `TvdbClient` in `services/tvdb_client.rs` — JWT auth via `/login` with automatic token refresh (1-month TTL per v4 spec, `Arc<Inner>` pattern for Clone, double-checked locking), search (`/search`, `/search/remoteid/{id}`), series details (`/series/{id}/extended?meta=episodes`), movie details (`/movies/{id}`), artwork (`/series/{id}/artworks` with type ID mapping: 1=poster, 2=banner, 3=backdrop, 4=clearlogo, 5=thumbnail). TvdbClient registered in both `supplementary_metadata` and `artwork` slots sharing same token state. 20 TVDB response deserialization types with `Option<T>` throughout (v4 spec marks no fields required). Full `FanartClient` in `services/fanart_client.rs` — simple API key query param auth, movie artwork by TMDB ID, TV artwork by TVDB ID, 9 movie + 11 TV artwork types mapped to internal types, likes-based sorting, defensive relative URL handling. Full `OmdbClient` in `services/omdb_client.rs` — simple API key query param auth, ratings lookup by IMDb ID via `/?i={id}&apikey={key}`, extracts IMDb rating (f64), votes, Rotten Tomatoes (from `Ratings` array), Metacritic, Rated, Awards; handles OMDb's HTTP 200 + `"Response": "False"` error pattern and `"N/A"` missing value strings. `urlencoding` 2, `async-trait` 0.1 added to workspace. No new dependencies for TVDB/Fanart/OMDB clients. See [BUILD_ORDER.md](BUILD_ORDER.md) Phase 6 for details.

**Task 12 — Provider API key validation:** `POST /api/v1/settings/providers/validate` endpoint in new `domains/system/` five-file pattern (admin-only via `Require<CanManageServer>`). Creates temporary client instances with provided credentials, tests connectivity, discards. `TmdbClient`/`TvdbClient` use existing `MetadataProvider::test_connection()` trait method. `FanartClient` and `OmdbClient` use new inherent `test_connection()` methods (fetched well-known IDs, only auth-specific errors = invalid key). `SystemError` with `SYS_013` (InvalidProvider, 400) and `SYS_014` (MissingCredential, 400). Validation result returned in response body as `{valid, error}`, not HTTP status.

**Task 13 — API key encryption at rest:** `services/encryption.rs` — AES-256-GCM via `ring::aead::LessSafeKey` with `encrypted:` + base64(nonce||ciphertext||tag) wire format. `encryption_key` added to `BootstrapConfig` (hex-encoded 256-bit key in `config.toml` or `DUSKCUE_ENCRYPTION_KEY` env var). Auto-generated on first run and written to config file. `load_runtime_config()` decrypts provider keys after JSONB deserialization via `decrypt_provider_config()`. `encrypt_provider_config()` for future settings save endpoints. `mask_secret()` for admin API responses. `Arc<EncryptionKey>` in `AppState`. 18 unit tests. No new workspace dependencies.

**Tasks 14–15 — Daily exports and metadata refresh:** `TmdbClient` gained `fetch_changed_movie_ids()`/`fetch_changed_tv_ids()` methods for TMDB `/changes` API (paginated, 14-day max range). `metadata_refresh` worker in `workers/metadata_refresh.rs` — downloads daily ID export `.json.gz` files from `files.tmdb.org` to `{cache_dir}/metadata/exports/`, cleans up files older than 7 days, queries TMDB `/changes` for items modified since last refresh, cross-references with DB items, calls `re_enrich_item()` for targeted re-enrichment. `re_enrich_item()` added to `enrichment_persistence.rs` — calls orchestrator's `enrich_movie()`/`enrich_tv()` directly and persists. Executor registered on scheduler in `main.rs` with `enrichment` and `cache_dir` captures. Added `flate2 = "1"` to workspace deps for gzip decompression.

**Phase 7 in progress (Tasks 1–12):** Playback domain module (`domains/playback/`) with five-file pattern — 22 handler stubs, 20 service stubs, 24 error variants (PLAY_001–013 + domain-specific), full DTO set (Row/Request/Response types for sessions, user item data, bookmarks, playlists), 20 routes wired into router. `AppError::Playback(#[from] PlaybackError)` added to central error enum with `playback_error_to_http()` mapping. Transcoding service (`services/transcoding.rs`) — FFmpeg subprocess management via `tokio-process-tools` v0.11, `TranscodeManager` with `Arc<DashMap>` sessions + `Semaphore` capacity, `HwAccelMethod` enum with `ffmpeg_encoder()` mapping, 4-rung ABR ladder (480p–1080p HQ), structured progress parsing (`-progress pipe:1`), seek-as-stop-restart, cfg-conditional HW accel auto-detection, graceful shutdown (SIGTERM/Ctrl-Break → SIGKILL). `TranscodingConfig` expanded to 13 fields, `CpuConfig` to 12 fields. `Arc<TranscodeManager>` in `AppState`. **Task 3:** FFmpeg sandbox (`services/sandbox.rs`) — Landlock LSM filesystem isolation (ABI V3, RO: `/usr`/`/lib`/`/etc`/`/dev/dri`/media, RW: transcode dir/`/tmp`) + seccomp-BPF syscall filtering (62-syscall allow-list, `KillProcess` on mismatch) via `seccompiler` v0.4, platform-gated to Linux, graceful degradation on failure. `spawn_ffmpeg` applies sandbox via `pre_exec`. **Task 4:** Quality domain (`domains/quality/`) five-file pattern — 4 Row, 7 Request, 8 Response types; `QualityError` with 12 variants (QUALITY_001–012); 13 routes wired into router covering device capabilities, wizard, network probing, telemetry, QoE, admin endpoints. **Task 5:** Device capability detection — `report_capabilities` (upsert on `device_identifier`), `get_device_profile` (conservative baseline on missing), `start_capability_wizard` (10-test matrix from `WIZARD_TEST_MATRIX`), `submit_capability_test` (auto-completes wizard), `get_capabilities`/`list_capability_tests` (query by `device_identifier`), `derive_capabilities_from_wizard` (maps test results to full capability profile). 6 handlers working, 7 remain `todo!()`. **Task 6:** Network quality assessment — segment telemetry with per-segment throughput + harmonic mean across configurable window + 6-tier network classification, bandwidth probe with 100KB static payload + server-side throughput, QoE reports with 5 industry-standard metrics, admin dashboards (network summary with LATERAL join, device capability summary per-platform, QoE summary with DISTINCT ON, transcode breakdown from play_sessions). **Task 7:** Transcoding decision engine (`services/decision_engine.rs`) — pure shared service with 10-factor evaluation (quality_mode → codec → bit_depth → resolution → HDR/DV → container → bitrate → manual cap); 6 video outcomes (DirectPlay, Remux, Transcode, ToneMap, Convert, Error); DV Profile 5/7/8 handling with client-side fallback; codec alias system for tolerant matching; target codec selection (HEVC for 4K/10-bit, Opus→EAC3→AC3 for audio); resolution normalization to standard tiers; bitrate ladder via `TranscodeRendition::smart_ladder()`; 21 unit tests. See [BUILD_ORDER.md](BUILD_ORDER.md) Phase 7 for details. **Task 8:** Streaming policy system - CRUD for streaming_policies table (list, get, create, update, delete), all admin-only (Require<CanManageServer>); esolve_streaming_limits implements 3-tier cascade (user overrides -> policy values -> defaults); get_effective_streaming_limits endpoint resolves merged limits per user; is_default flag managed atomically in transactions; system policies protected from deletion; 5 new error variants (PLAY_014-018); 3 new route groups (/api/v1/streaming-policies, /api/v1/streaming-policies/{id}, /api/v1/users/{id}/streaming-limits). No new workspace dependencies. **Task 9:** HLS manifest generation and segment serving — `stream_file` handler serves media files via Direct Play with HTTP 206 Partial Content and RFC 7233 Range header parsing (3 formats: `bytes=N-`, `bytes=-N`, `bytes=N-M`), content type detection for 12 video containers. `get_transcode_manifest` reads FFmpeg-generated `manifest.m3u8` from transcode session directory. `get_transcode_playlist` resolves per-rendition playlists from master manifest (handles both single-rendition and multi-rendition manifests). `get_transcode_segment` serves fMP4 segment bytes with path traversal protection (rejects `..`, `/`, `\`, names >64 chars, non-`seg_` prefixed). All four handlers return `Response` (not `Json`) for binary/text content types. `RangeSpec` struct for Range header parsing, `generate_master_manifest` for ABR ladder manifest generation. Cache headers: manifest/playlist `no-cache`, segments `max-age=3600`. No new workspace dependencies. **Task 10:** Three-tier playback dispatch implemented — `start_playback()` in playback service replaces `todo!()` stub with full flow: fetch media item + select media file from DB, build `MediaFileInfo` from `media_files` row (resolution parsed via `parse_resolution_string`, bit depth from JSONB, frame rate defaulted to 24.0), build `DeviceCapabilities` from client device profile JSON or conservative defaults (H.264/AAC/MP4+MKV/1080p/2ch/8-bit), build `NetworkConditions` from latest `client_network_reports` or `max_streaming_bitrate`, build `DecisionEngineConfig` from `RuntimeConfig` quality/transcoding fields, call `decision_engine::decide()`, dispatch to DirectPlay (direct file URL) / DirectStream (`start_remux_session()` with `-c:v copy -c:a copy`) / Transcode (`start_session()` with full encoding), create `play_sessions` row. `start_remux_session()` added to `TranscodeManager` — reuses session tracking, progress monitoring, semaphore, and sandboxing. `force_transcode` override supported. Static SQL strings per sqlx 0.9 `SqlSafeStr`. No new workspace dependencies. **Task 11:** HW accel runtime detection (`services/hw_accel.rs`) — dedicated module with `HwAccelDetectionResult` struct (method, platform flags, verified encoders, source). `detect_hw_accel_runtime()` performs multi-step detection: config forcing with encoder verification → FFmpeg `-encoders`/`-hwaccels` probing via `std::process::Command` → platform checks (NVIDIA: `/dev/nvidia*` or `nvidia-smi`; VAAPI: `/dev/dri/renderD*` with driver detection via sysfs; QSV: Intel i915 driver + FFmpeg qsv; VideoToolbox: macOS; AMF: FFmpeg amf). Priority order: NVENC > QSV > VAAPI > VideoToolbox > AMF > Software. Respects `hw_accel_auto_detect` and `hardware_accel` config. Prometheus `system.cpu.hw_accel` gauge emitted per method label. Health endpoint enriched with `hardware_acceleration` object. `TranscodeManager` stores full `HwAccelDetectionResult` with `get_hw_detection()` accessor. No new workspace dependencies. **Task 12:** Play session tracking — `heartbeat()` updates `play_sessions` metadata (position, state, transcode_session_id) via PostgreSQL JSONB `||` merge, detects state transitions and emits `play_events` (pause/resume/buffer_start/buffer_end), upserts `user_item_data` with HOT-update friendly pattern. `stop_playback()` kills transcode session, emits stop event, marks session ended, upserts `user_item_data` with final position/play_count/is_watched (90% threshold clears resume). `seek()` delegates to transcode seek (returns new session ID) or direct-play passthrough (client-side). `get_playback_info()` returns live session state from metadata. 3 new DTOs (`StopPlaybackRequest`/`StopPlaybackResponse`/`SeekResponse`). Ownership verification returns `SessionNotFound` for both not-found and not-owned to prevent info leakage. No new workspace dependencies or error variants.

## Open Questions

- [x] Final product name — **Duskcue**
- [x] Server language and framework — Rust + Axum
- [x] Database choice — PostgreSQL 18
- [x] Surrogate key strategy — UUIDv7 (see [DATABASE.md](docs/design/DATABASE.md))
- [x] Database schema design — All domains complete (see [DATABASE.md](docs/design/DATABASE.md))
- [x] Transcoding engine — FFmpeg with hardware acceleration (NVENC/NVDEC, VAAPI, VideoToolbox, AMF)
- [x] Web client framework — Svelte + SvelteKit
- [x] Mobile framework — Flutter
- [x] Desktop wrapper — Tauri 2
- [x] UI foundations — baseline visual direction, navigation language, and core reusable surfaces (see [UI_FOUNDATIONS.md](docs/branding/UI_FOUNDATIONS.md))
- [x] Error handling strategy — thiserror + anyhow + RFC 9457 (see below)
- [x] Database backup & recovery — WAL-G + pg_dump + monitoring (see below)
- [x] Migration strategy — sqlx-cli 0.9 with timestamp-based naming (see below)
- [x] Release engineering & upgrade safety — SemVer, preflight backup gates, abrupt-shutdown recovery posture, rollback rules, PostgreSQL minor/major upgrade handling (see below)
- [x] CI & testing strategy — restore drills, migration verification, media fixtures, workflow security, and release quality gates (see below)
- [x] Docker build & release process — BuildKit/buildx, multi-stage images, GHCR publication, attestations, and protected tag workflows (see below)
- [x] Base-image refresh policy — digest pinning, supported branch cadence, and CVE response rules for release images (see below)
- [x] Builder trust boundary & private dependency ingress — trusted runner tiers, OIDC-scoped access, and self-hosted runner exceptions (see below)
- [x] Privileged artifact handoff — metadata-only cross-boundary transfer, trusted rebuild defaults, and attested trusted-to-trusted promotion rules (see below)
- [x] Build cache trust boundary — PR-visible versus trusted-only cache policy, poisoning prevention, and safe-to-persist rules (see below)
- [x] Registry cache retention & garbage collection — dedicated cache package design, retention windows, and trusted cleanup ownership (see below)
- [x] Release artifact retention & rollback evidence — durable GHCR and release-asset evidence, checksum manifests, and clear separation from workflow artifact expiry (see below)
- [x] Configuration bootstrap — Two-tier (TOML bootstrap + DB runtime), see below
- [x] Logging & observability — tracing ecosystem + metrics + Prometheus + optional OTel, see below
- [x] Media scanning — hybrid FS watch + scheduled scan + mtime diff, see below
- [x] Platform targeting — x86_64 + ARM64 (aarch64); Alpine Linux for Docker; cross-compilation via cargo-zigbuild
- [x] Streaming & transcoding design — HLS with fMP4; three-tier decision flow; FFmpeg pipeline; HW accel (see STREAMING.md)
- [x] Docker deployment — hybrid embedded/external PostgreSQL; single container by default; PUID/PGID; read_only + no-new-privileges (see DOCKER_DEPLOYMENT.md)
- [x] Segment detection — 4-method pipeline; chromaprint-next; safety-first design; skip buttons (see SEGMENT_DETECTION.md)
- [x] Storyboards (seek previews) — WebVTT + WebP sprite sheets; FFmpeg generation; keyframe-only; adaptive interval (see STORYBOARDS.md)
- [x] Cache & storage strategy — three-tier storage; per-type size limits; LRU eviction; disk monitoring (see CACHE_STORAGE.md)
- [x] Database maintenance & bloat management — per-table autovacuum; HOT updates via fillfactor; REINDEX CONCURRENTLY; partition ANALYZE (see DATABASE_MAINTENANCE.md)
- [x] Memory management — Tokio runtime config; FFmpeg subprocess lifecycle via tokio-process-tools v0.11.2; structured progress parsing (-progress pipe:1); PG connection pool tuning; health checks & watchdogs; crash recovery; memory budgets; mimalloc v3 allocator; cgroup-aware detection; PSI pressure metrics (see MEMORY.md)
- [x] CPU management — FFmpeg threading; process priority; ARM64 big.LITTLE; HW accel detection; thermal throttling (see CPU.md)
- [x] Library organization — folder structure; naming conventions; sub-folder-as-library design; metadata ID tags; scanner traversal; collections (see LIBRARY_ORGANIZATION.md)
- [x] Quality management — device capability detection; network quality measurement; transcoding decision engine; QoE metrics (see QUALITY_MANAGEMENT.md)
- [x] Subtitle domain — OCR (PaddleOCR), sync, fetching (OpenSubtitles), delivery; full design in SUBTITLES.md
- [x] Video formats domain — codecs, containers, HDR, bit depth, color; full design in VIDEO_FORMATS.md
- [x] Audio formats domain — codecs, channels, spatial audio, containers, downmixing; full design in AUDIO_FORMATS.md
- [x] Metadata overlays — overlay compositing engine, badges, text, groups, queues, conditions; full design in METADATA_OVERLAYS.md
- [x] Collections — static, dynamic, smart collections; 14 internal builders; 13 external builders; templates; full design in COLLECTIONS.md
- [x] Poster management — artwork lifecycle, multi-source, locking, asset directory, community packs; full design in POSTER_MANAGEMENT.md
- [x] API conventions — REST endpoint naming, URI versioning, hybrid cursor/offset pagination, governor rate limiting, dual auth (cookie + bearer), CORS, async 202, ETag; full design in API_CONVENTIONS.md
- [x] Platform migration — Plex/Jellyfin/Emby watch history import; user mapping via invite code display names; provider ID matching; merge strategy; full design in MIGRATIONS.md
- [x] Security & remote access — three-tier opt-in model; Cloudflare alternatives; rustls TLS; HMAC signed URLs; security headers; FFmpeg per-process sandboxing (Landlock + seccomp); full design in SECURITY.md
- [x] OS hardening — Docker Engine v28+ minimum (v29.4.3+ recommended); host minimums; container hardening; base-image lifecycle cross-linked; full design in OS_HARDENING.md
- [x] API security — OWASP API Top 10 coverage; input validation; BOLA prevention; SSRF prevention; response DTO separation; admin endpoint isolation; dependency auditing; supply chain hardening; full design in API_SECURITY.md
- [x] Analytics security — impossible travel detection; IP geolocation (MaxMind GeoLite2); trust scoring; false positive suppression; full design in ANALYTICS_SECURITY.md
- [x] Metadata provider integration — TMDB v3 primary; TVDB/Fanart.tv/OMDb supplementary; SubDL primary subtitles; trait-based abstraction; encrypted API keys; graceful degradation; full design in METADATA_PROVIDERS.md
- [x] TLS crypto backend — **ring** (not aws-lc-rs). `rustls`, `tokio-rustls`, and `reqwest` configured with `default-features = false` + `features = ["ring"]` to avoid `aws-lc-sys` which requires NASM on Windows. See workspace `Cargo.toml`.
- [x] WebAuthn server library — **webauthn-rs** (kanidm, server-side Relying Party); `passkey-auth` rejected (client-side only, not suitable for server). See [AUTH.md](docs/design/AUTH.md) WebAuthn Crate section.
- [x] API key encryption at rest — AES-256-GCM via `ring::aead`; `encrypted:` prefix wire format; master key in bootstrap config; auto-generated on first run; shared with backup encryption per BACKUP_RECOVERY.md. See `services/encryption.rs`.
- [ ] Live TV tuner hardware support

## Database Design Decisions

All database design is documented in [DATABASE.md](docs/design/DATABASE.md). Summary of completed domains:

| Domain | Status | Key Tables | Authoritative Doc |
|---|---|---|---|
| Core Media | Complete | `libraries`, `media_items`, `movies`, `series`, `seasons`, `episodes`, `media_files`, `subtitle_files`, `artwork`, `genres`, `tags`, `people`, `credits` | [DATABASE.md](docs/design/DATABASE.md) |
| Trakt.tv Integration | Complete | `trakt_accounts`, `trakt_sync_state` | [DATABASE.md](docs/design/DATABASE.md) |
| Activity & Analytics | Complete | `play_sessions` (partitioned), `play_session_streams`, `play_events` (partitioned), `user_trust_events`, `user_trust_scores` | [DATABASE.md](docs/design/DATABASE.md) |
| Classifarr Integration | Complete | No dedicated tables — passive read-only API | [DATABASE.md](docs/design/DATABASE.md) |
| Playback | Complete | `user_item_data`, `bookmarks`, `playlists`, `playlist_items` | [DATABASE.md](docs/design/DATABASE.md) |
| User & Authentication | Complete | `users`, `user_passkeys`, `user_totp`, `user_capabilities`, `user_library_access`, `user_sessions`, `api_keys`, `invitations`, `device_linking_codes`, `reauth_codes`, `streaming_policies` | [AUTH.md](docs/design/AUTH.md) |
| Segment Detection | Complete | `media_segments`, `media_fingerprints` | [SEGMENT_DETECTION.md](docs/design/SEGMENT_DETECTION.md) |
| Storyboards | Complete | `storyboards` | [STORYBOARDS.md](docs/design/STORYBOARDS.md) |
| System | Complete | `server_config`, `scheduled_tasks`, `scheduled_task_runs`, `notification_types`, `notifications`, `user_notification_preferences` | [DATABASE.md](docs/design/DATABASE.md) |
| Cache & Storage | Complete | `server_config.storage` JSONB; no dedicated tables | [CACHE_STORAGE.md](docs/operations/CACHE_STORAGE.md) |
| Database Maintenance | Complete | Per-table autovacuum, `fillfactor`, `pgstattuple`; `server_config.maintenance` JSONB | [DATABASE_MAINTENANCE.md](docs/operations/DATABASE_MAINTENANCE.md) |
| Memory | Complete | `server_config.resource_limits` JSONB; mimalloc v3 allocator | [MEMORY.md](docs/design/MEMORY.md) |
| CPU | Complete | `server_config.cpu` JSONB | [CPU.md](docs/design/CPU.md) |
| Library Organization | Complete | `library_paths` | [LIBRARY_ORGANIZATION.md](docs/design/LIBRARY_ORGANIZATION.md) |
| Multi-Edition | Complete | `media_files.edition_name`, `movies.edition_count`, `episodes.edition_count` | [MULTI_EDITION.md](docs/design/MULTI_EDITION.md) |
| Quality Management | Complete | `device_profiles`, `device_capability_tests`, `client_network_reports`, `qoe_reports`; `server_config.quality` JSONB | [QUALITY_MANAGEMENT.md](docs/design/QUALITY_MANAGEMENT.md) |
| Subtitle | Complete | `subtitle_files`, `subtitle_ocr_cache`, `subtitle_sync_data`; `server_config.subtitles` JSONB | [SUBTITLES.md](docs/design/SUBTITLES.md) |
| Video Formats | Complete | Uses `media_files` columns; no dedicated tables | [VIDEO_FORMATS.md](docs/design/VIDEO_FORMATS.md) |
| Audio Formats | Complete | Uses `media_files` columns; no dedicated tables | [AUDIO_FORMATS.md](docs/design/AUDIO_FORMATS.md) |
| Metadata Overlays | Complete | `overlay_definitions`, `artwork_overlay_state`; `artwork` extensions | [METADATA_OVERLAYS.md](docs/design/METADATA_OVERLAYS.md) |
| Collections | Complete | `collections`, `collection_items`, `collection_templates` | [COLLECTIONS.md](docs/design/COLLECTIONS.md) |
| Poster Management | Complete | `artwork` extensions (`is_locked`, `source_type`); `server_config.metadata` JSONB | [POSTER_MANAGEMENT.md](docs/design/POSTER_MANAGEMENT.md) |
| Metadata Providers | Complete | `server_config.metadata.providers` JSONB; no new tables | [METADATA_PROVIDERS.md](docs/design/METADATA_PROVIDERS.md) |
| API Conventions | Complete | No tables — middleware, extractors, rate limit config | [API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md) |
| Platform Migration | Complete | `migration_sources`, `migration_user_mapping`, `migration_import_log` | [MIGRATIONS.md](docs/design/MIGRATIONS.md) |
| Security & Remote Access | Complete | `server_config.security` JSONB; no new tables | [SECURITY.md](docs/security/SECURITY.md) |
| OS Hardening & Platform Requirements | Complete | No tables — runtime detection only | [OS_HARDENING.md](docs/operations/OS_HARDENING.md) |
| API Security | Complete | No tables — application-layer patterns | [API_SECURITY.md](docs/security/API_SECURITY.md) |
| Backup Encryption | Complete | `server_config.backup` JSONB extensions; key in bootstrap config | [BACKUP_RECOVERY.md](docs/operations/BACKUP_RECOVERY.md) |
| Vulnerability Reporting | Complete | No tables — policy document | [VULNERABILITY_DISCLOSURE.md](docs/security/VULNERABILITY_DISCLOSURE.md) |
| Analytics Security | Complete | `user_location_history`; `server_config.analytics` JSONB; `geoip_database_update` task | [ANALYTICS_SECURITY.md](docs/security/ANALYTICS_SECURITY.md) |
| Cross-Cutting | Complete | Soft delete, partitioning, full-text search, audit trail | [DATABASE.md](docs/design/DATABASE.md) |

### Cross-Cutting Concerns

- **Soft delete:** `deleted_at` on `libraries`, `users`, `playlists` only; 30-day purge via scheduled task; partial unique indexes for business keys
- **Partitioning:** Three tables partitioned by month (`play_sessions`, `play_events`, `audit_log`); application-level partition management; DETACH CONCURRENTLY for retention
- **Full-text search:** Trigger-maintained `search_vector` on `media_items` with cross-table data (title, overview, cast, genres, tags); GIN index; `pg_trgm` fuzzy fallback; field weighting (A-D); `websearch_to_tsquery()` for user input
- **Audit trail:** Trigger-based `audit_log` table with JSONB before/after; application context via session variables; sensitive field redaction; range-partitioned monthly; 1-year retention

## Error Handling

Error handling strategy is documented in [ERROR_HANDLING.md](docs/design/ERROR_HANDLING.md). Covers: thiserror v2 + anyhow v1, three-layer architecture, RFC 9457 Problem Details (obsoletes RFC 7807), environment-aware error responses (dev/staging/prod), error code registry (121 codes across 14 domains), Retry-After headers for external dependencies, and implementation rules.

## Database Backup & Recovery

Database defensibility strategy is documented in [BACKUP_RECOVERY.md](docs/operations/BACKUP_RECOVERY.md). Covers: three layers of defense (prevention, detection, recovery), WAL-G continuous archiving with PITR (primary), pg_dump logical backups (secondary), built-in integrity monitoring via scheduled tasks and notifications, 3-2-1 storage strategy, and recovery objectives.

**Key decisions:**
- **WAL-G** for continuous WAL archiving + PITR — single Go binary, Apache 2.0, pgBackRest is dead (April 2026)
- **pg_dump** for portable logical backups — cross-version, table-level selective restore
- **`data_checksums=on`** — page-level silent corruption detection
- **Built-in monitoring** — scheduled integrity checks, backup verification, WAL archival health alerts via existing notification system
- **3-2-1 storage** — production + local NAS + optional S3 off-site
- **Backup encryption** — WAL-G built-in AES-256-GCM; auto-enabled for S3 storage (backups leave your network); optional for local; key stored in bootstrap config (not database — database is inside the backup); key rotation without downtime; full design in BACKUP_RECOVERY.md

## Migration Strategy

Migration strategy is documented in [MIGRATION_STRATEGY.md](docs/design/MIGRATION_STRATEGY.md). Covers: sqlx-cli 0.9.0 tool selection, timestamp-based naming (`YYYYMMDD_HHMMSS_name.sql`), idempotency requirements, migration lifecycle rules, Classifarr pattern comparison, and operational procedures.

**Key decisions:**
- **sqlx-cli 0.9** — same ecosystem as our query layer; embedded migrations; built-in checksums; advisory locking; released May 2026
- **Timestamp-based naming** — no merge conflicts; natural ordering; industry standard (Flyway, Prisma, Classifarr all use timestamps)
- **Append-only** — applied migrations are immutable; fixes are new migrations; production rollbacks use PITR
- **Fail-fast** — migration failure stops server startup; no partial schema states
- **Idempotent** — all migrations use `IF NOT EXISTS` / `IF EXISTS` / `DO $$ ... $$` patterns

## Release Engineering & Upgrade Safety

Release engineering and upgrade safety are documented in [RELEASE_ENGINEERING.md](docs/ci/RELEASE_ENGINEERING.md). Covers: SemVer policy, release channels, API/database compatibility rules, abrupt-shutdown recovery posture, upgrade preflight gates, rollback boundaries, and PostgreSQL minor vs major upgrade procedures.

**Key decisions:**
- **Semantic Versioning 2.0.0** — stable `MAJOR.MINOR.PATCH` scheme with `alpha`, `beta`, `rc`, and `stable` channels
- **Crash-safe durability baseline** — `fsync=on`, `synchronous_commit=on`, `full_page_writes=on`, `wal_level=replica`, `archive_mode=on`
- **Layered verification** — WAL-G archive checks + PostgreSQL `pg_verifybackup` + scheduled `pg_amcheck` + periodic restore drills
- **Minor vs major DB upgrades** — PostgreSQL minor updates are stop/replace/restart; PostgreSQL major upgrades use offline `pg_upgrade` after verified backup
- **Rollback boundary is explicit** — binary rollback is only allowed before incompatible migrations or cluster upgrades; otherwise rollback is PITR/restore

## CI & Testing Strategy

CI and testing strategy are documented in [CI_TESTING.md](docs/ci/CI_TESTING.md). Covers: fast PR validation, mainline regression coverage, restore drills, migration verification, media-fixture corpus design, workflow security posture, and release quality gates.

**Key decisions:**
- **Four validation lanes** — fast PR, mainline, scheduled operations, and release workflows each own different failure modes
- **SQLx drift is a merge blocker** — `cargo sqlx prepare --check --workspace -- --all-targets --all-features` is part of the baseline contract
- **Restore drills are mandatory** — backup verification is layered from `pg_verifybackup` up to full restore and PITR rehearsal
- **Fixture corpus is tiered** — tiny synthetic fixtures in git; larger trusted corpora and restore snapshots outside git
- **Release publication is gated** — protected release workflows generate SBOMs and provenance attestations only after restore and migration evidence is green

## Docker Build & Release Process

Docker build and release strategy are documented in [DOCKER_BUILD_RELEASE.md](docs/operations/DOCKER_BUILD_RELEASE.md). Covers: BuildKit/buildx usage, multi-stage Dockerfile rules, multi-architecture publication, GitHub Actions publication design, cache strategy, secret handling, OCI metadata, and image attestations.

**Key decisions:**
- **BuildKit/buildx is the Linux default** — legacy builder is not the design center; release publication uses Buildx and protected GitHub Actions workflows
- **GHCR is the source of truth** — release images publish to GHCR first with optional future Docker Hub mirroring
- **Multi-arch manifest publication** — one operator-facing image name publishes `linux/amd64` and `linux/arm64`
- **Explicit supply-chain evidence** — published images ship with SBOMs, provenance, and GitHub artifact attestations
- **Secret-safe builds** — build credentials use BuildKit secret and SSH mounts, never plain build args

## Base-Image Refresh Policy

Base-image refresh policy is documented in [BASE_IMAGE_REFRESH_POLICY.md](docs/ci/BASE_IMAGE_REFRESH_POLICY.md). Covers: trusted base-image sources, tag-plus-digest pinning, Alpine stable-branch adoption rules, weekly and monthly refresh cadence, fixable-CVE response windows, and versioning rules for base-image-only rebuilds.

**Key decisions:**
- **Digest pinning is mandatory** — release Dockerfiles use `tag@sha256:digest`, never tag-only base-image references
- **Supported branches only** — runtime images use Alpine stable branches; `edge` is never a production baseline
- **Freshness is scheduled, not implicit** — publishable builds use `--pull`, weekly freshness review, and monthly clean rebuilds
- **CVE response is fix-driven** — critical and high fixable base-image vulnerabilities have explicit rebuild windows
- **Stable releases are immutable** — a base-image refresh does not silently reuse an existing stable application version tag

## Builder Trust Boundary & Private Dependency Ingress

Builder trust boundary and private dependency ingress are documented in [BUILDER_TRUST_BOUNDARY.md](docs/ci/BUILDER_TRUST_BOUNDARY.md). Covers: runner trust tiers, fork PR isolation, self-hosted runner exception rules, OIDC trust scoping, BuildKit secret and SSH mounts, and private registry or package ingress patterns.

**Key decisions:**
- **GitHub-hosted is the default trust boundary** — untrusted and most trusted builds run on ephemeral GitHub-hosted runners
- **Secret-bearing jobs are separated** — validation jobs that process untrusted code do not share the same trust boundary as publish-capable jobs
- **OIDC is preferred for private ingress** — cloud and registry access should use short-lived credentials and restrictive trust claims where possible
- **Self-hosted is an exception path** — self-hosted runners are allowed only for trusted workflows with isolation and cleanup controls
- **Docker build secrets are explicit** — private build inputs use BuildKit secret mounts, SSH mounts, or Git auth secrets, never ARG or persistent ENV

## Secret Brokerage & Rotation for Trusted Release and Maintenance Workflows

Secret brokerage and rotation for trusted release and maintenance workflows are documented in [SECRET_BROKERAGE_ROTATION.md](docs/ci/SECRET_BROKERAGE_ROTATION.md). Covers: credential-source precedence, `GITHUB_TOKEN` versus direct OIDC versus Vault-style brokerage, reusable-workflow and environment gating for privileged jobs, static-secret exception rules, and rotation or revocation expectations.

**Key decisions:**
- **GitHub-native auth is first** — `GITHUB_TOKEN` remains the default for GitHub-native publication, package, and release operations
- **Direct OIDC is the external default** — providers that can validate GitHub identity claims should use short-lived workload identity instead of stored cloud secrets
- **Vault is the exception broker, not the baseline** — brokered issuance is used where targets cannot trust GitHub directly or where dynamic credentials materially improve security
- **Protected environments own high-privilege release** — production publication and maintenance jobs require reviewed reusable workflows plus environment approval before credentials are released
- **Static secrets are narrowly contained** — long-lived GitHub secrets are allowed only as documented exception paths and should be retired when ephemeral alternatives become available

## Trusted Runner State Disposal

Trusted runner state disposal is documented in [TRUSTED_RUNNER_STATE_DISPOSAL.md](docs/ci/TRUSTED_RUNNER_STATE_DISPOSAL.md). Covers: one-job self-hosted runner guarantees, JIT and `--ephemeral` disposal models, pre- and post-job cleanup hooks, external log forwarding for ephemeral runners, Docker builder and cache cleanup, and the difference between destroying a compute boundary versus merely deleting a workspace.

**Key decisions:**
- **Ephemeral is the only strong exception path** — privileged self-hosted jobs should run on one-job ephemeral runners or ARC-managed ephemeral runner pods, not on shared persistent pools
- **Reused hardware still needs reprovisioning** — JIT registration alone is not enough if the underlying host is not returned to a known-clean baseline
- **Hooks are for hygiene, not teardown** — pre-job hooks fail closed on residue, post-job hooks scrub local state, but runner destruction must happen outside the post-job hook
- **Local Docker state is explicitly disposable** — builder instances, BuildKit cache, stopped containers, unused networks, and disposable volumes must not accumulate between privileged jobs
- **Logs leave before the runner dies** — ephemeral runner diagnostics and cleanup telemetry must be forwarded to external storage rather than left on the destroyed runner

## Trusted Runner Compromise Response

Trusted runner compromise response is documented in [TRUSTED_RUNNER_COMPROMISE_RESPONSE.md](docs/ci/TRUSTED_RUNNER_COMPROMISE_RESPONSE.md). Covers: quarantine steps after suspected privileged-runner exposure, off-box evidence capture, workflow-log and artifact cleanup after secret exposure, GitHub and Vault credential revocation, and rebuild or re-admission requirements.

**Key decisions:**
- **Quarantine before analysis** — suspect runners are taken offline, active privileged runs are canceled, and the runner is removed from GitHub before deeper investigation
- **Evidence is external-first** — workflow logs, audit logs, runner `_diag` logs, and control-plane records are exported to trusted storage instead of relying on the compromised host
- **Blast radius starts at last known-clean** — on reused trusted hosts, credential review and rotation begin at the last attested clean baseline rather than only the final observed job
- **Vault revocation is part of containment** — incident response revokes auth tokens, leases, and if needed entire issuance prefixes, not just GitHub-side secrets
- **Rebuild beats cleanup** — compromised trusted runners are reimaged or reprovisioned from a known-clean baseline and re-registered fresh instead of being returned to service after manual scrubbing

## Environment & Runner-Group Emergency Governance

Environment and runner-group emergency governance is documented in [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](docs/ci/ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md). Covers: who may freeze privileged workflows, who may change privileged runner-group access during incidents, which change-window bypasses are allowed for containment, and which production controls must remain fail-closed.

**Key decisions:**
- **Containment beats convenience** — emergency authority is for disabling workflows, rejecting pending jobs, canceling runs, and isolating runner groups, not for pushing production changes through faster
- **Production stays fail-closed** — production environments keep required reviewers, prevent self-review, and disable administrator bypass even during incident handling
- **Repo and org controls are split** — environment custody stays repository-side while runner-group and org Actions policy changes stay organization-side
- **Owner break-glass stays small** — use custom organization roles where available, otherwise keep the owner set minimal and reserve it for the few controls GitHub cannot delegate cleanly
- **Every emergency action is evidence-bearing** — workflow, environment, runner-group, and org policy changes must be captured in the audit log and exported with the incident record

## Privileged Artifact Handoff

Privileged artifact handoff is documented in [PRIVILEGED_ARTIFACT_HANDOFF.md](docs/ci/PRIVILEGED_ARTIFACT_HANDOFF.md). Covers: `workflow_run` privilege separation, artifact digest validation limits, metadata-only untrusted handoff, trusted rebuild defaults, and attested trusted-to-trusted promotion.

**Key decisions:**
- **Untrusted outputs are evidence, not release payloads** — pull request workflows do not create publishable production artifacts
- **Trusted release rebuild is the default** — release workflows rebuild from protected refs rather than promoting PR-built binaries or images
- **Artifact digests are necessary but limited** — artifact digest validation checks transport integrity, not artifact trustworthiness
- **Promotion requires trusted provenance** — artifact reuse without rebuild is allowed only between trusted workflows with verified attestations
- **Reusable trusted build workflows are the fast path later** — central reusable workflows plus attestation verification are the preferred optimization when trusted promotion becomes necessary

## Build Cache Trust Boundary

Build cache trust boundary is documented in [BUILD_CACHE_TRUST_BOUNDARY.md](docs/ci/BUILD_CACHE_TRUST_BOUNDARY.md). Covers: PR-visible versus trusted-only cache classes, backend policy for `gha`, `registry`, `local`, and `inline`, safe-to-persist content rules, and cache poisoning or invalidation controls.

**Key decisions:**
- **Caches are performance hints, not trust proofs** — trusted release decisions do not rely on cache hits or cache-origin claims
- **PR-visible cache content is constrained** — anything persisted in GitHub Actions cache must be safe for pull-request visibility and contain no secrets or private dependency material
- **Trusted persistence uses separate registry refs** — protected-branch and release jobs use dedicated registry-backed caches rather than sharing PR-visible cache storage
- **Transient sensitive inputs stay out of cache** — BuildKit secrets, SSH mounts, and bind-mounted large inputs do not become persisted cache state
- **Cache namespaces rotate on suspicion** — scopes or refs are versioned so maintainers can cut over quickly after poisoning, leakage, or major toolchain change

## Registry Cache Retention & Garbage Collection

Registry cache retention and garbage collection are documented in [REGISTRY_CACHE_RETENTION.md](docs/ci/REGISTRY_CACHE_RETENTION.md). Covers: dedicated GHCR cache package separation, bounded branch cache refs, cleanup ownership, local builder versus remote registry cleanup, and the boundary between cache retention and release rollback evidence.

**Key decisions:**
- **Cache packages are separate from release packages** — automated cache deletion must not target the same GHCR namespace that carries production images
- **Active cache refs stay small and branch-scoped** — one mutable cache ref per active protected branch family, target, and epoch is the default model
- **Trusted scheduled workflows own cleanup** — registry cache pruning runs only from trusted maintenance workflows with package-admin scope limited to the cache package
- **Rollback does not depend on cache retention** — deleting cache artifacts may slow rebuilds but does not remove deployable release artifacts or evidence
- **Local and remote cleanup are different jobs** — `buildx du` and `buildx prune` manage builder disks, while package-version deletion manages remote registry cache storage

## Release Artifact Retention & Rollback Evidence

Release artifact retention and rollback evidence are documented in [RELEASE_ARTIFACT_RETENTION.md](docs/ci/RELEASE_ARTIFACT_RETENTION.md). Covers: durable release anchors, evidence-class separation, checksum and attestation retention, release evidence manifests, and the boundary between supported rollback evidence and disposable workflow artifacts.

**Key decisions:**
- **Workflow artifacts are not the durable archive** — run deletion and short retention windows make GitHub workflow artifacts unsuitable as the only home for rollback evidence
- **GHCR digests are the container anchor** — retained digest-addressed package versions and exact SemVer tags define the durable container release identity
- **Every stable release gets a durable evidence manifest** — release version, commit, digest, checksums, SBOM and provenance references, trusted workflow IDs, and rollback classification are retained with the release record
- **Stable evidence is kept indefinitely by default** — supported stable release evidence is not subject to routine cleanup until a later archival policy explicitly replaces that rule
- **Cache and CI cleanup stay out of scope** — cache pruning and normal workflow-artifact expiration must not remove the evidence needed to verify or understand a supported release

## Configuration Strategy

Configuration strategy is documented in [CONFIGURATION.md](docs/operations/CONFIGURATION.md). Covers: two-tier architecture (bootstrap pre-DB + runtime post-DB), config-rs + clap library selection, TOML format, layered merge (CLI > ENV > TOML > defaults), file discovery per platform, first-run setup wizard, Docker/Synology integration, hot-reload policy.

**Key decisions:**
- **Two-tier config** — bootstrap (TOML + ENV + CLI, 5 fields) to reach the database; runtime (`server_config` table) for everything else
- **Single source of truth** — the database is the source of truth for all server behavior; no file/DB sync issues
- **TOML format** — human-readable, commentable, Rust-native, no YAML type coercion surprises
- **Layered merge** — CLI args override ENV vars override TOML file override built-in defaults (via config-rs 0.15)
- **Bootstrap fields** — `database_url`, `data_dir`, `cache_dir`, `log_level`, `environment`, `encryption_key`; everything else is in `server_config`
- **First-run setup wizard** — fresh DB seeds defaults, launches browser-based setup for admin account, server name, networking, first library
- **Docker-friendly** — single env var `DUSKCUE_DATABASE_URL` is all that's required; setup wizard handles the rest

## Logging & Observability

Logging and observability strategy is documented in [LOGGING_OBSERVABILITY.md](docs/operations/LOGGING_OBSERVABILITY.md). Covers: tracing ecosystem (tracing + subscriber + appender + error), tower-http request tracing, metrics facade with Prometheus exporter, OpenTelemetry optional integration, structured JSON file logging, rolling file appender, log sanitization.

**Key decisions:**
- **`tracing` ecosystem** — tracing 0.1.44 + tracing-subscriber 0.3.23 + tracing-appender 0.2.5 + tracing-error 0.2.1; tokio-rs maintained; async-aware spans
- **Dual output** — pretty (console) + JSON (file) via composable subscriber layers
- **Non-blocking file writes** — tracing-appender writes on dedicated thread; WorkerGuard flushes on shutdown
- **tower-http TraceLayer** — automatic HTTP request/response spans with method, path, status, latency
- **`tracing-error`** — SpanTrace captures span context in error chains; displayed in development error responses
- **`metrics` facade** — counters, gauges, histograms with zero-cost noop; Prometheus exporter embedded in axum router
- **Prometheus `/metrics` endpoint** — embedded in existing HTTP server; subnet-restricted access via `server_config.network`
- **OpenTelemetry optional** — `otel` Cargo feature flag; zero overhead when disabled; for users running Jaeger/Tempo
- **Log sanitization** — passwords, tokens, API keys never written to log files
- **Personal information handling** — email addresses masked (`use***@***`) at info+ level; IP addresses mask last octet; session/device IDs truncated to 8 chars; invite codes never logged; watch history never combined with user identity in same log line; custom tracing-subscriber layer enforces rules automatically
- **`server_config.logging` JSONB** — level, max file size, max files, format; hot-reloadable via admin API
- **8 metric categories** — HTTP, playback, library, database, system, Trakt, transcode, analytics

## Media Scanning

Media scanning strategy is documented in [MEDIA_SCANNING.md](docs/design/MEDIA_SCANNING.md). Covers: hybrid scanning (FS watch + scheduled + manual), 6-phase pipeline (discover → diff → probe → identify → enrich → cleanup), file change detection, filename parsing, metadata provider matching, partial hashing, scan configuration.

**Key decisions:**
- **Hybrid approach** — FS watch (notify) for real-time detection + periodic scheduled scan as safety net; handles NFS/SMB/Docker where FS events are unreliable
- **`notify` + `notify-debouncer-full`** — cross-platform FS watching (inotify/FSEvents/ReadDirectoryChangesW) with rename stitching, event dedup, 3-second debounce
- **`ignore` (ripgrep) for parallel walking** — full library scans use multi-threaded directory traversal; `walkdir` for targeted re-scans
- **mtime-based diff** — skip unchanged files (Phase 2); only probe new/modified files; 2-second mtime tolerance for FAT32/SMB
- **Blake3 partial hash** — first 1MB + last 1MB for ambiguous change detection; fast and reliable for video files
- **ffprobe for file probing** — extract codecs, resolution, duration, streams; concurrent probe queue with configurable limit (default: 2)
- **Path-based parsing + TMDB/TVDB lookup** — parse title/year/season/episode from filenames; search provider API for identification
- **4 match states** — `unmatched`, `auto_matched`, `confirmed`, `manual`; new `media_items.match_state` column
- **6-phase pipeline** — discover → diff → probe → identify → enrich → cleanup; each phase logged with file counts
- **3 trigger sources** — FS watch (real-time), scheduled scan (periodic), manual scan (admin API)
- **Graceful degradation** — if FS watcher fails (watch limits, NFS), fallback to PollWatcher or scheduled-only scanning
