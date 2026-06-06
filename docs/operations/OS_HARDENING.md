# Operating System Hardening Domain

## Overview

This document is the authoritative design for operating system hardening and platform compatibility requirements. It covers: minimum OS versions, Docker Engine requirements, Alpine Linux base image strategy, OS update detection, container hardening, and admin-facing security warnings.

The platform is a self-hosted application — it does not manage or control the host operating system. Instead, it **detects** the host environment, **validates** minimum requirements, and **recommends** updates. No auto-updates are ever performed.

---

## Platform Compatibility Matrix

### Linux Distributions

| Distribution | Minimum Version | Recommended Version | Kernel (min) | EOL |
|---|---|---|---|---|
| **Debian** | 12 (bookworm) | 13 (trixie) | 6.1 | 12: ~2028, 13: ~2029 |
| **Ubuntu** | 22.04 LTS (jammy) | 24.04 LTS (noble) | 5.15 | 22.04: 2027, 24.04: 2029 |
| **AlmaLinux** | 9.0 | 9.x latest | 5.14 | 2032 |
| **Rocky Linux** | 9.0 | 9.x latest | 5.14 | 2032 |
| **Synology DSM** | 7.1 | 7.2+ | (Synology managed) | Active |

**Why these versions:**
- Debian 12 is the oldest Debian still receiving security updates; Debian 13 is current stable (May 2026)
- Ubuntu 22.04 LTS has 5 years of standard support (to 2027); 24.04 LTS is current
- AlmaLinux/Rocky Linux 9 are RHEL 9 compatible, supported through 2032
- Synology DSM 7.1 is the minimum that supports current Docker packages

### Windows

| Version | Minimum Build | Status |
|---|---|---|
| Windows 10 22H2 Enterprise | 19045 | EOL October 2026 (ESU only) |
| **Windows 11 23H2** | **22631** | **Minimum recommended** |
| Windows 11 24H2 | 26100 | Current, recommended |

**Why Windows 11 23H2 minimum:**
- Windows 10 Home/Pro reached EOL October 2025 — no security updates
- Windows 10 Enterprise reaches EOL October 2026 — only if covered by ESU
- Windows 11 23H2 is the oldest Windows 11 version still receiving active security updates
- Windows 11 24H2 is current with latest security features (VBS, HVCI, Credential Guard improvements)

### macOS

| Version | Minimum |
|---|---|
| macOS 13 (Ventura) | Minimum for Tauri 2 desktop app |
| macOS 14 (Sonoma) | Recommended |
| macOS 15 (Sequoia) | Current |

---

## Docker Requirements

### Docker Engine

| Component | Minimum | Recommended | Rationale |
|---|---|---|---|
| **Docker Engine** | **28.0.0** | **29.4.3+** | v29.4.3 mitigates CVE-2026-31431 ("Copy Fail"); v28+ has runc/BuildKit fixes from 2024 |
| Docker Compose | 2.24+ | 2.35+ | Compose v2.24+ required for `depends_on` health conditions |
| containerd | 1.7+ | 2.0+ | Bundled with Docker Engine |

### Why Docker Engine >= 28.0.0

| CVE | Severity | Fixed In | Impact |
|---|---|---|---|
| CVE-2026-31431 | Critical | v29.4.3 | Kernel AF_ALG page cache corruption; container-to-host escape. Mitigated by seccomp + AppArmor + SELinux defaults in v29.4.3 |
| CVE-2025-9074 | High | Desktop 4.44.3 | Container accessed Docker Engine without socket mount |
| CVE-2024-21626 | High | Engine 25.0.2+ | runc container escape via leaked file descriptors |
| CVE-2024-23651/52/53 | High | BuildKit 0.12.5+ | BuildKit race conditions, file removal, privilege elevation |
| CVE-2024-24557 | Medium | Engine 25.0.2+ | Classic builder cache poisoning |

Docker Engine v28.0.0 includes all 2024 CVE fixes. Docker Engine v29.4.3+ additionally mitigates CVE-2026-31431 without requiring a kernel patch.

### Docker Version Detection

The platform reads `docker version --format '{{.Server.Version}}'` at startup (when running inside Docker) to determine the host Docker Engine version. This is available inside the container via the Docker API socket — but the platform does **not** mount the Docker socket by default. Instead, it detects the container runtime environment from `/proc/1/cgroup` and `/run/.containerenv`.

For bare-metal/non-Docker deployments, Docker version detection is skipped.

---

## Alpine Linux Base Image Strategy

Authoritative lifecycle rules for the container base image now live in [BASE_IMAGE_REFRESH_POLICY.md](../ci/BASE_IMAGE_REFRESH_POLICY.md).

### Decision: use the current Alpine stable branch with digest pinning

- Release Dockerfiles use Alpine's current stable branch, pinned as `tag@sha256:digest`, not a tag-only reference.
- As of the May 2026 research snapshot, Alpine `3.23` is the current stable branch.
- Alpine `edge` is not allowed for production or deterministic containerized builds.
- Remaining on a previous stable branch requires a documented exception and a time-bounded migration plan.
- Refresh cadence, branch adoption timing, and CVE-response SLAs are defined in [BASE_IMAGE_REFRESH_POLICY.md](../ci/BASE_IMAGE_REFRESH_POLICY.md).

### Alpine Security Profile

| Metric | Alpine | Ubuntu | Debian |
|---|---|---|---|
| Image size | ~5 MB | ~64 MB | ~50 MB |
| Default packages | 14 | 89 | ~70 |
| Historical CVEs | 2 | 2007 | ~660 |
| Attack surface | Minimal | Moderate | Moderate |
| Package manager | apk | apt | apt |
| libc | musl | glibc | glibc |

Alpine's minimal package count directly reduces attack surface. The 14 default packages include only: alpine-baselayout, apk-tools, busybox, ca-certificates, musl, and a handful of core utilities.

### musl libc Considerations

Alpine uses musl libc instead of glibc. This is relevant because:
- Rust targets `x86_64-unknown-linux-musl` natively — zero compatibility issues
- Some pre-built native crates may assume glibc — always test on musl
- DNS resolution differences: musl does not support `resolv.conf` options like `options rotate` — not relevant for our use case
- Thread stack size: musl default is 128 KB vs glibc's 8 MB — set `RUST_MIN_STACK` if needed (our FFmpeg subprocesses are unaffected)

---

## Container Hardening

### Current Hardening (Already in DOCKER_DEPLOYMENT.md)

The Docker Compose configuration already applies:

```yaml
read_only: true
security_opt:
  - no-new-privileges:true
cap_drop:
  - ALL
cap_add:
  - CHOWN
  - SETUID
  - SETGID
  - FOWNER
  - DAC_OVERRIDE
```

### Additional Hardening Recommendations

These are documented as recommendations for the user's `docker-compose.yml`, not enforced by the platform:

| Measure | Purpose | How |
|---|---|---|
| `pids_limit: 100` | Prevent fork bombs | Limits process count inside container |
| `mem_limit: 4g` | Prevent OOM host impact | Set based on `resource_limits` config |
| `cpus: 2.0` | Prevent CPU starvation | Set based on available cores |
| `restart: unless-stopped` | Auto-recovery | Already in our Compose file |
| Seccomp profile | Syscall filtering | Docker Engine v29 default profile blocks `AF_ALG` |
| AppArmor profile | LSM enforcement | Docker Engine v29 default blocks `AF_ALG` via AppArmor |
| `tmpfs` for writable paths | No persistent writes | Already in our Compose file |

### Docker Hardened Images (DHI)

Docker released Hardened Images (DHI) in 2026 — free, Apache 2.0:
- 95% fewer CVEs than standard images
- Rootless by default
- Distroless runtime (no shell, no package manager)
- 7-day critical CVE fix guarantee

**Our position:** We continue using `alpine:3.22` as our base image. DHI is documented as an advanced option for users who want maximum hardening. Users can swap `FROM alpine:3.22` to a DHI variant in their own builds if needed. We do not ship a DHI-based image because:
- DHI is distroless — no shell for debugging (`docker exec -it ... sh` does not work)
- Alpine is well-understood by the self-hosting community
- DHI is newer and less proven for complex workloads (embedded PostgreSQL, FFmpeg subprocess)
- Our image already applies `read_only`, `no-new-privileges`, and `cap_drop: ALL`

---

## OS Update Detection

### Detection Strategy

The platform performs **read-only detection** at startup and every 24 hours. It never installs updates or modifies the host system.

### What We Detect

| Check | Linux | Docker | Windows |
|---|---|---|---|
| **OS name + version** | `/etc/os-release` | N/A | `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion` |
| **Kernel version** | `uname -r` | N/A | `ver` command or `[Environment]::OSVersion` |
| **Pending security updates** | Check if `apt list --upgradable` (Debian/Ubuntu) or `dnf check-update --security` (AlmaLinux/Rocky) has output | N/A | Not checked (requires elevated privileges) |
| **Docker Engine version** | `docker version` (if socket available) | From inside container: `/proc/1/cgroup` confirms Docker, version from environment | Same as Linux |
| **Alpine version (inside container)** | `cat /etc/alpine-release` | Always available | N/A |

### Detection Implementation

```rust
pub struct HostEnvironment {
    pub os_name: String,
    pub os_version: String,
    pub os_id: String,
    pub kernel_version: String,
    pub is_docker: bool,
    pub docker_engine_version: Option<String>,
    pub alpine_version: Option<String>,
}

impl HostEnvironment {
    pub fn detect() -> Self {
        Self {
            os_name: Self::read_os_release("NAME").unwrap_or_else(|| "Unknown".into()),
            os_version: Self::read_os_release("VERSION_ID").unwrap_or_else(|| "Unknown".into()),
            os_id: Self::read_os_release("ID").unwrap_or_else(|| "unknown".into()),
            kernel_version: Self::read_kernel_version(),
            is_docker: Self::detect_docker(),
            docker_engine_version: Self::detect_docker_version(),
            alpine_version: Self::read_alpine_release(),
        }
    }
}
```

### Minimum Version Validation

```rust
pub struct VersionRequirement {
    pub os_id: &'static str,
    pub min_version: &'static str,
    pub recommendation: &'static str,
    pub severity: Severity,
}

const VERSION_REQUIREMENTS: &[VersionRequirement] = &[
    VersionRequirement { os_id: "debian",      min_version: "12",   recommendation: "Debian 13 (trixie)",       severity: Severity::Warning },
    VersionRequirement { os_id: "ubuntu",      min_version: "22.04", recommendation: "Ubuntu 24.04 LTS (noble)", severity: Severity::Warning },
    VersionRequirement { os_id: "almalinux",   min_version: "9",    recommendation: "AlmaLinux 9.x latest",     severity: Severity::Warning },
    VersionRequirement { os_id: "rocky",       min_version: "9",    recommendation: "Rocky Linux 9.x latest",   severity: Severity::Warning },
];

const DOCKER_MIN_VERSION: &str = "28.0.0";
const DOCKER_RECOMMENDED_VERSION: &str = "29.4.3";
```

### Admin Dashboard Display

When requirements are not met, the admin dashboard shows a **System Health** panel:

| Condition | Icon | Message |
|---|---|---|
| OS version below minimum | Warning | "Debian 11 (bullseye) is below the minimum requirement of Debian 12. Security updates may no longer be available. Consider upgrading." |
| Docker Engine below minimum | Error | "Docker Engine 27.3.1 is below the minimum requirement of v28.0.0. Critical security vulnerabilities are unpatched. Update Docker Engine." |
| Docker Engine below recommended | Warning | "Docker Engine 28.1.0 does not include mitigation for CVE-2026-31431 (Copy Fail). Update to Docker Engine v29.4.3+ for the best protection." |
| Pending security updates | Info | "12 security updates are available for your OS. Consider installing them." |

### When Detection Runs

1. **At server startup** — always runs; results cached in memory
2. **Every 24 hours** — scheduled task `system_requirement_check`; results update cached state
3. **On admin demand** — admin clicks "Refresh" on the System Health panel

Detection results are **not stored in the database** — they are transient, like health checks. If the server restarts, detection re-runs fresh.

---

## No New Tables

No database schema changes are needed. The OS detection is a runtime-only concern, similar to health checks. The scheduled task `system_requirement_check` is added to the existing `scheduled_tasks` CHECK constraint.

---

## No New Error Codes

OS version warnings are not errors — they are informational warnings shown in the admin dashboard and logged at `warn!` level. They do not block server startup or operation.

---

## Research Sources

### Docker Security
- Docker Security Announcements: https://docs.docker.com/security/security-announcements/
- Docker Blog — Mitigating CVE-2026-31431 (Copy Fail) in Docker Engine: https://www.docker.com/blog/mitigating-cve-2026-31431-copy-fail-in-docker-engine/
- NVD — CVE-2026-28400 Detail: https://nvd.nist.gov/vuln/detail/CVE-2026-28400

### Container Hardening
- Sysdig — 17 Comprehensive Container Security Best Practices for 2026 (March 2026): https://www.sysdig.com/learn-cloud-native/container-security-best-practices
- Ping Identity — Evaluation of Docker Base Image Security: https://developer.pingidentity.com/devops/docker-images/dockerImageSecurity.html
- Kubesimplify — Day 2: Your Images Are a Supply Chain (April 2026): https://blog.kubesimplify.com/day-2-your-images-are-a-supply-chain-and-it-s-probably-broken
- Reddit r/devsecops — What are the options for hardened container images in 2026? (March 2026): https://www.reddit.com/r/devsecops/comments/1s35onx/

### Linux Distributions
- Knightli — Debian, Rocky Linux, AlmaLinux, and Ubuntu Server Compared (May 2026): https://knightli.com/en/2026/05/07/linux-server-distro-comparison-2026/
- Contabo — Best Linux Distros in 2026 (March 2026): https://contabo.com/blog/best-linux-distros/

### Alpine Linux
- Alpine Linux — Stable Releases 3.20.10, 3.21.7, 3.22.4, 3.23.4 (April 2026): https://www.alpinelinux.org/posts/Alpine-3.20.10-3.21.7-3.22.4-3.23.4-released.html
- Alpine Linux — Stable Releases 3.20.9, 3.21.6, 3.22.3, 3.23.3 (January 2026): https://www.alpinelinux.org/posts/Alpine-3.20.9-3.21.6-3.22.3-3.23.3-released.html

### Windows Lifecycle
- Microsoft Lifecycle Policy: https://learn.microsoft.com/en-us/lifecycle/
- Microsoft — Products Ending Support in 2026: https://learn.microsoft.com/en-us/lifecycle/end-of-support/end-of-support-2026
