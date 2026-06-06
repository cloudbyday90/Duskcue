# CPU Management

## Overview

Strategy for CPU utilization management across two target architectures: **x86_64** (Intel, AMD) and **ARM64** (Apple Silicon, RK3588, AWS Graviton, Synology NAS). Covers FFmpeg threading, process priority, big.LITTLE handling, hardware acceleration detection, software encoding optimization, CPU watchdog, and Docker CPU management per architecture.

Memory management is a separate domain documented in [MEMORY.md](MEMORY.md).

## Architecture Profiles

| Profile | CPUs | HW Accel | SW Encode Performance | Typical Use Case |
|---|---|---|---|---|
| **x86_64 + NVIDIA GPU** | 4-16 cores | NVENC/NVDEC | Excellent (AVX2/SSE) | Desktop server with GPU |
| **x86_64 + Intel iGPU** | 4-24 cores | Quick Sync (VAAPI) | Excellent (AVX2/SSE) | N100, Intel NUC, NAS |
| **x86_64 + AMD iGPU** | 4-32 cores | VAAPI / AMF | Excellent (AVX2/SSE) | AMD desktop/server |
| **x86_64 software only** | 2-64 cores | None | Excellent | VPS, cloud |
| **Apple Silicon (M1-M4)** | 4-16 cores (P+E) | VideoToolbox | Good (NEON) | Mac Mini, Mac Studio |
| **RK3588 (ARM SBC)** | 4x A76 + 4x A55 | RKMPP / V4L2-request | Moderate (NEON) | Orange Pi 5, Rock 5B |
| **Generic ARM64 (NAS)** | 2-4 cores | VAAPI (rare) or None | Limited (NEON) | Synology, QNAP ARM models |
| **AWS Graviton** | 1-96 cores (homogeneous) | None | Good (NEON) | Cloud VPS |

### ARM64 big.LITTLE Topology

ARM SoCs common in NAS/SBC hardware use heterogeneous cores. The Linux scheduler is big.LITTLE-aware (kernel 4.14+) and generally routes compute-heavy tasks to big cores automatically.

| SoC | Big Cores | LITTLE Cores | Market |
|---|---|---|---|
| RK3588 | 4x Cortex-A76 @ 2.4 GHz | 4x Cortex-A55 @ 1.8 GHz | Orange Pi 5, Radxa Rock 5B |
| Synology ARM (Realtek) | Varies | Varies | DS224+, DS423 |
| Apple M1 | 4x Firestorm | 4x Icestorm | Mac Mini |
| Apple M4 | 4x Avalanche | 6x Blizzard | Mac Mini |
| AWS Graviton 4 | 96x Neoverse-V2 (homogeneous) | — | Cloud |

**Impact on transcoding:** If FFmpeg auto-detects 8 cores on an RK3588, it may schedule threads on LITTLE cores that are 40-60% slower than big cores. OS-managed scheduling handles this well for most users. Power users can pin FFmpeg to big cores via `cpu_affinity` config.

---

## Crate Selection

| Crate | Version | Role |
|---|---|---|
| `sysinfo` | 0.34 | Cross-platform per-core CPU utilization, process CPU tracking |
| `nix` | 0.31 | Unix-specific: `sched_setaffinity` for CPU pinning, `kill()` for SIGTERM (Unix only) |

### Why These Crates

| Crate | Strength | Limitation | Our Use |
|---|---|---|---|
| **sysinfo** | Per-core CPU usage; cross-platform (Linux, macOS, Windows, ARM); no C deps | CPU usage requires two refreshes to compute delta | CPU watchdog, per-core metrics, Prometheus gauges |
| **nix** | Native Unix API bindings; `sched_setaffinity` for CPU pinning | Unix only (no Windows); low-level | CPU affinity for FFmpeg on Linux/macOS; SIGTERM (not SIGKILL) for FFmpeg |

---

## FFmpeg Threading Configuration

### Threading Options (from FFmpeg official docs)

| Option | Values | Default | Scope |
|---|---|---|---|
| `-threads` | `auto` (0), or integer | `auto` | Codec-level; applies to both encode and decode |
| `-threads:v` | Same | `auto` | Video stream specific |
| `thread_type` | `slice`, `frame`, `slice+frame` | `slice+frame` | Which multithreading method to use |

### Threading Methods

| Method | How | Latency | Effectiveness |
|---|---|---|---|
| **Frame threading** | Decodes/encodes multiple frames in parallel | +1 frame per thread | Best for modern codecs (H.264, H.265). Our primary method for streaming |
| **Slice threading** | Processes multiple slices within a single frame | None | Only works when source was encoded with multiple slices (rare in modern content). Low benefit |

### Per-Architecture Thread Settings

| Architecture | `-threads` | `thread_type` | Rationale |
|---|---|---|---|
| **x86_64 (4+ cores)** | `0` (auto) | `frame` | Let FFmpeg use all cores; frame threading is optimal for streaming |
| **ARM64 (big.LITTLE, 8 cores)** | `0` (auto) or capped to big-core count | `frame` | OS scheduler routes to big cores; admin can cap threads if needed |
| **ARM64 (2-4 cores, NAS)** | Explicit: core count minus 1 | `frame` | Reserve 1 core for server (API, DB); FFmpeg gets remaining cores |
| **Apple Silicon** | `0` (auto) | `frame` | macOS scheduler handles P/E cores efficiently |

### FFmpeg Spawn with Threading

Threading args are added to the inner `tokio::process::Command` before wrapping with `tokio-process-tools`. See [MEMORY.md](MEMORY.md) for the full spawn configuration with tokio-process-tools.

```rust
let threads = match arch_profile {
    ArchProfile::NasArm { core_count: 2..=4 } => core_count - 1,
    _ => 0,
};

let mut args = vec![
    "-threads:v".to_string(), threads.to_string(),
    "-thread_type", "frame",
];
```

---

## FFmpeg Process Priority

### Scheduling Priority (nice)

| Platform | Command | Effect |
|---|---|---|
| Linux / macOS | `nice -n 10` prefix | FFmpeg runs at lower scheduling priority; server API + DB always take CPU first |
| Windows | `SetPriorityClass(BELOW_NORMAL_PRIORITY_CLASS)` | Windows equivalent of nice |
| Docker | `--cpu-shares 512` (default 1024) | Soft CPU limit during contention |

**Recommendation:** Spawn FFmpeg with `nice -n 10` on Unix. This is a config option (`ffmpeg_nice`) — disabled by default, enabled when admin sets it. Requires no extra capabilities in Docker since the server binary itself spawns the child process at its own priority level.

### I/O Priority (Linux only)

| Command | Class | Effect |
|---|---|---|
| `ionice -c 2 -n 7` | Best-effort, low priority | FFmpeg disk I/O (reading source files) doesn't starve PostgreSQL or cache writes |

**Recommendation:** `ionice -c 2 -n 7` applied when `ffmpeg_ionice` config is enabled (default: enabled on Linux). Requires `CAP_SYS_NICE` in Docker — off by default, documented as opt-in.

### Spawn with Priority

Priority is configured on the inner `Command` before wrapping with `tokio-process-tools`. See [MEMORY.md](MEMORY.md) for the full spawn configuration.

```rust
let mut command = Command::new("ffmpeg");
if config.cpu.ffmpeg_nice {
    #[cfg(unix)]
    {
        command = Command::new("nice");
        command.arg("-n").arg("10").arg("ffmpeg");
    }
}
```

---

## CPU Affinity (big.LITTLE / ARM SBCs)

### Problem

On ARM SoCs with big.LITTLE topology, FFmpeg may schedule transcode threads on LITTLE cores, wasting time and causing stuttering. The OS scheduler handles this well by default, but power users may want explicit pinning.

### Configuration

`cpu_affinity` in `server_config.cpu` JSONB — optional comma-separated list of core IDs. When set, FFmpeg processes are spawned with `taskset -c <cores>` wrapper on Linux, or via `sched_setaffinity` programmatically.

### big.LITTLE Detection

On Linux ARM64, detect big cores by parsing `/proc/cpuinfo`:

| `CPU part` | Core Type |
|---|---|
| `0xd0b` | Cortex-A55 (LITTLE) |
| `0xd41` | Cortex-A76 (big) |
| `0xd44` | Cortex-X2 (ultra) |
| `0xd05` | Cortex-A73 (big, older) |
| `0xd03` | Cortex-A53 (LITTLE, older) |

Or via `/sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq` — big cores run at higher frequencies.

**Recommendation:** Auto-detect big cores at startup on ARM64 Linux. If detected, auto-populate `cpu_affinity` with big-core IDs. Admin can override. No detection on x86_64 (homogeneous cores).

### Docker cpuset

For ARM SBCs running in Docker, recommend `cpuset-cpus` to pin the entire container to big cores:

```yaml
# RK3588 example: pin to big cores only (cores 4-7 on most RK3588 boards)
cpuset-cpus: "4-7"
```

---

## Hardware Acceleration Detection

### Runtime Probe

At startup, the server probes available hardware acceleration:

```rust
fn detect_hw_accel() -> HwAccelMethod {
    // 1. Check for NVIDIA: /dev/nvidia* or nvidia-smi
    // 2. Check for Intel/AMD VAAPI: /dev/dri/renderD128 + vainfo
    // 3. Check for Apple VideoToolbox: cfg!(target_os = "macos")
    // 4. Check for RKMPP: /dev/dri/card* + rkmpp decoder presence
    // 5. Check FFmpeg -hwaccels output
    // 6. Fallback: software encoding
}
```

### Per-Architecture Acceleration

| Platform | Method | FFmpeg Encoder | Availability |
|---|---|---|---|
| x86_64 + NVIDIA | NVENC/NVDEC | `h264_nvenc`, `hevc_nvenc` | Requires `nvidia-container-toolkit` in Docker |
| x86_64 + Intel | Quick Sync (VAAPI) | `h264_vaapi`, `hevc_vaapi` | Requires `/dev/dri` device mount |
| x86_64 + AMD | VAAPI / AMF | `h264_vaapi`, `hevc_amf` | Requires `/dev/dri` device mount |
| Apple Silicon | VideoToolbox | `h264_videotoolbox`, `hevc_videotoolbox` | macOS only; not available in Docker |
| RK3588 (vendor kernel) | RKMPP | `h264_rkmpp`, `hevc_rkmpp` | Requires `ffmpeg-rockchip` build; vendor kernel 5.10/6.1 |
| RK3588 (mainline kernel) | V4L2-request | `v4l2-request` (FFmpeg 8.0+) | In development; kernel 7.0+ |
| Generic ARM64 | Software | `libx264`, `libx265` | Always available |

### Detection Results

Detection results are cached in memory and exposed via `/health` and Prometheus. Re-detection on config reload or manual trigger via admin API.

---

## ARM64 Software Encoding Optimization

### x264 vs x265 on ARM

From community benchmarks and FFmpeg developer consensus:

| Codec | NEON Optimization | ARM Performance | Recommendation |
|---|---|---|---|
| **libx264** | Extensive hand-tuned NEON assembly | Good — NEON provides 1.6-2.5x speedup over scalar ARM | **Preferred for ARM software encoding** |
| **libx265** | Less NEON optimization than x264 | Moderate — significantly slower than x86 per-watt | Avoid on ARM unless HW accel unavailable and HEVC is required |

### Preset Selection on ARM

| ARM Profile | Recommended Preset | Reasoning |
|---|---|---|
| 2-4 core NAS | `ultrafast` | Only option for real-time 1080p on slow CPUs |
| RK3588 (big cores only) | `veryfast` | Balanced quality/speed; handles 1080p in real-time |
| Apple Silicon | `fast` or `medium` | Powerful enough for higher quality |
| AWS Graviton | `fast` | Good CPU performance; balanced |

### x86_64 Software Encoding

No special considerations — x86_64 with AVX2/SSE handles both x264 and x265 well. Preset `veryfast` is the default for transcoding.

---

## CPU Watchdog

### Monitoring (shared with MEMORY.md watchdog loop)

The CPU watchdog runs in the same `tokio::spawn` loop as the memory watchdog (every 60s). CPU-specific behavior:

```
Every 60 seconds:
  1. sys.refresh_cpu_usage()  (requires two refreshes for delta)
  2. let cpu_percent = average of sys.cpus().cpu_usage() over 5-minute window

  if cpu_percent > 90%:
    CRITICAL
    → reject all new transcodes (PLAY_003 + SYS_010)
    → log critical CPU pressure
    → metrics: counter!("system.cpu.pressure_events", "level" => "critical")

  elif cpu_percent > 80%:
    WARNING
    → reject new transcodes if active > 1
    → log warning CPU pressure
    → metrics: counter!("system.cpu.pressure_events", "level" => "warning")
```

### Thermal Throttling Detection (ARM64)

ARM SoCs (especially SBCs) can thermal-throttle under sustained load. Detect via:

```rust
// Linux: read thermal zone
let temp = fs::read_to_string("/sys/class/thermal/thermal_zone0/temp");
// temp is in millidegrees Celsius; divide by 1000 for °C
```

| Threshold | Action |
|---|---|
| > 80°C | Log warning; reduce max concurrent transcodes by 1 |
| > 85°C | Reject new transcodes until temperature drops below 75°C |
| > 90°C | Kill oldest active transcode session; `server_alert` notification |

Not available in Docker (no `/sys/class/hwmon` or `/sys/class/thermal` access). Documented as host-level monitoring.

---

## CpuConfig Configuration

### Rust Struct

```rust
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CpuConfig {
    pub transcode_cpu_threshold_percent: u8,
    pub cpu_warning_percent: u8,
    pub cpu_critical_percent: u8,
    pub ffmpeg_threads: Option<u32>,
    pub ffmpeg_thread_type: String,
    pub ffmpeg_nice: bool,
    pub ffmpeg_ionice: bool,
    pub cpu_affinity: Option<String>,
    pub hw_accel_auto_detect: bool,
    pub thermal_throttle_enabled: bool,
    pub thermal_warning_celsius: u8,
    pub thermal_critical_celsius: u8,
}

impl Default for CpuConfig {
    fn default() -> Self {
        Self {
            transcode_cpu_threshold_percent: 90,
            cpu_warning_percent: 80,
            cpu_critical_percent: 90,
            ffmpeg_threads: None,
            ffmpeg_thread_type: "frame".to_string(),
            ffmpeg_nice: true,
            ffmpeg_ionice: true,
            cpu_affinity: None,
            hw_accel_auto_detect: true,
            thermal_throttle_enabled: true,
            thermal_warning_celsius: 80,
            thermal_critical_celsius: 85,
        }
    }
}
```

### JSONB Example

Stored in `server_config.cpu`:

```json
{
    "transcode_cpu_threshold_percent": 90,
    "cpu_warning_percent": 80,
    "cpu_critical_percent": 90,
    "ffmpeg_threads": null,
    "ffmpeg_thread_type": "frame",
    "ffmpeg_nice": true,
    "ffmpeg_ionice": true,
    "cpu_affinity": null,
    "hw_accel_auto_detect": true,
    "thermal_throttle_enabled": true,
    "thermal_warning_celsius": 80,
    "thermal_critical_celsius": 85
}
```

### Field Reference

| Field | Type | Default | Description |
|---|---|---|---|
| `transcode_cpu_threshold_percent` | u8 | 90 | Reject new transcodes when system CPU exceeds this percentage (5-min average) |
| `cpu_warning_percent` | u8 | 80 | Log warning at this CPU threshold |
| `cpu_critical_percent` | u8 | 90 | Log critical + reject transcodes at this CPU threshold |
| `ffmpeg_threads` | Option\<u32\> | `None` (auto) | Override FFmpeg `-threads` count. `None` = auto-detect. Set to `cores - 1` on 2-4 core ARM |
| `ffmpeg_thread_type` | String | `"frame"` | FFmpeg `thread_type`: `"frame"`, `"slice"`, or `"slice+frame"`. Frame-only recommended for streaming |
| `ffmpeg_nice` | bool | `true` | Run FFmpeg with `nice -n 10` on Unix. Lowers scheduling priority |
| `ffmpeg_ionice` | bool | `true` | Run FFmpeg with `ionice -c 2 -n 7` on Linux. Lowers I/O priority |
| `cpu_affinity` | Option\<String\> | `None` | Comma-separated core IDs for CPU pinning (e.g. `"4-7"` for RK3588 big cores). Auto-populated on ARM64 if big cores detected |
| `hw_accel_auto_detect` | bool | `true` | Auto-detect hardware acceleration at startup. Disable to force software encoding |
| `thermal_throttle_enabled` | bool | `true` | Monitor CPU temperature and reduce transcode load when hot. ARM64 Linux only |
| `thermal_warning_celsius` | u8 | 80 | Temperature warning threshold in °C |
| `thermal_critical_celsius` | u8 | 85 | Temperature critical threshold — reject/kill transcodes above this |

---

## Docker CPU Management

### Per-Architecture Examples

#### x86_64 Desktop / Server

```yaml
deploy:
  resources:
    limits:
      cpus: "${CPU_LIMIT:-4.0}"
      memory: ${MEMORY_LIMIT:-4G}
```

#### ARM64 NAS (2-4 cores, homogeneous)

```yaml
deploy:
  resources:
    limits:
      cpus: "${CPU_LIMIT:-1.5}"
      memory: ${MEMORY_LIMIT:-1.5G}
```

#### ARM64 SBC (big.LITTLE, e.g. RK3588)

```yaml
# Pin to big cores only (cores 4-7 on RK3588)
cpuset-cpus: "${CPUSET:-0-7}"
deploy:
  resources:
    limits:
      memory: ${MEMORY_LIMIT:-4G}
```

#### Synology NAS

```yaml
# Synology Container Manager does not expose cpuset in GUI.
# Use Project (docker-compose) feature for cpuset-cpus.
# Most Synology ARM NAS have homogeneous cores — no pinning needed.
deploy:
  resources:
    limits:
      memory: ${MEMORY_LIMIT:-3G}
```

### CPU Priority in Docker

| Option | Effect | When to Use |
|---|---|---|
| `--cpu-shares 512` | Soft priority; FFmpeg gets less share during contention | Multi-container hosts |
| `--cpus 2.0` | Hard cap on CPU time | Prevent FFmpeg from monopolizing |
| `--cpuset-cpus 4-7` | Pin to specific cores | ARM big.LITTLE SBCs |
| `nice -n 10` inside container | Process-level priority | Default (via `ffmpeg_nice` config) |
| `--cap-add SYS_NICE` | Allow nice/renice inside container | Only needed for `ionice` or custom priorities |

---

## Prometheus Metrics

### CPU Metrics

| Metric | Type | Labels | Description |
|---|---|---|---|
| `system.cpu.usage_percent` | gauge | `core` (per-core) | Per-core CPU utilization percentage |
| `system.cpu.usage_average_percent` | gauge | | System-wide CPU utilization (5-min rolling average) |
| `system.cpu.pressure_events` | counter | `level` (warning, critical) | CPU pressure events triggered |
| `system.cpu.thermal_celsius` | gauge | | CPU temperature in Celsius (ARM64 Linux only) |
| `system.cpu.cores_total` | gauge | | Total CPU cores detected |
| `system.cpu.big_cores` | gauge | | Number of big (performance) cores detected (ARM64 only) |
| `system.cpu.hw_accel` | gauge | `method` (nvenc, vaapi, videotoolbox, rkmpp, software) | Active hardware acceleration method (1=active, 0=inactive) |
| `transcode.rejections_total` | counter | `reason` (capacity, cpu, thermal) | New transcodes rejected due to CPU limits |
| `transcode.ffmpeg_threads` | gauge | | Number of threads configured for FFmpeg |

These supplement the memory metrics in [MEMORY.md](MEMORY.md).

---

## Integration with Existing Systems

### Configuration (CONFIGURATION.md)

`CpuConfig` is part of `RuntimeConfig`, loaded from `server_config.cpu` JSONB. Admin changes via `PUT /api/v1/server/config` trigger cache reload.

### Error Handling (ERROR_HANDLING.md)

CPU limit errors return standard error codes:

| Condition | Error Code | HTTP |
|---|---|---|
| System CPU too high for transcode | `SYS_010` | 503 |
| Thermal throttle — transcode rejected | `SYS_012` | 503 |

### Database (DATABASE.md)

- `server_config.cpu` JSONB column for CPU-specific configuration
- `transcode_health_check` scheduled task (every 60s) monitors CPU pressure alongside memory and stale sessions

### Logging (LOGGING_OBSERVABILITY.md)

- CPU pressure events logged at `WARN` (warning) and `ERROR` (critical) with structured fields
- Thermal throttle events logged at `WARN` with temperature
- Hardware acceleration detection logged at `INFO` at startup

### Docker (DOCKER_DEPLOYMENT.md)

- Per-architecture CPU management examples (cpuset, cpu-shares, cpu limits)
- `SYS_NICE` capability documented as opt-in for `ionice` support

### Streaming (STREAMING.md)

- FFmpeg `-threads` and `thread_type` configured from `CpuConfig` before spawn
- CPU guard checked before FFmpeg spawn in the streaming decision flow
- Hardware acceleration method selected based on runtime detection

---

## Research Sources

- FFmpeg official documentation (codec options, threading, cpuflags): https://ffmpeg.org/ffmpeg.html
- FFmpeg `-threads`, `thread_type`, `-cpuflags`, `-cpucount` codec options (extracted from official docs)
- ARM big.LITTLE technology overview: https://www.arm.com/technologies/big-little
- ARM NEON optimization for video encoding: https://developer.arm.com/Additional%20Resources/Video%20Tutorials/DevHub/Efficient%20Video%20encoding%20on%20the%20cloud%20with%20Arm%20servers
- RK3588 RKMPP hardware transcoding with Jellyfin: https://www.reddit.com/r/selfhosted/comments/1khty8r/
- Rockchip FFmpeg hardware acceleration: https://docs.armsom.org/advanced-manual/ffmpeg
- FFmpeg VideoToolbox on Apple Silicon: https://stackoverflow.com/questions/64924728/
- FFmpeg ARM NEON build and performance: https://github.com/BtbN/FFmpeg-Builds/issues/395
- x264 vs x265 ARM performance comparison: https://www.reddit.com/r/ffmpeg/comments/1ge3224/
- Docker CPU priority (nice, cpu-shares, cpuset): https://stackoverflow.com/questions/54392310/
- Docker resource constraints documentation: https://docs.docker.com/config/containers/resource_constraints/
- sysinfo crate (cross-platform CPU/memory metrics): https://docs.rs/sysinfo/latest/sysinfo/
- core_affinity crate (CPU pinning): https://docs.rs/core_affinity/latest/core_affinity/
- nix crate (Unix API bindings): https://docs.rs/nix/latest/nix/
- Raspberry Pi 5 H.264 encoding performance: https://pip-assets.raspberrypi.com/categories/685-app-notes-guides-whitepapers/documents/RP-010033-WP-1-H.264%20encoding%20performance%20on%20Raspberry%20Pi%205_series%20computers.pdf
