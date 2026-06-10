# Configuration Strategy

## Overview

Two-tier configuration: a minimal **bootstrap** layer (file + environment + CLI) to reach the database, and a **runtime** layer loaded from `server_config` for everything else. This avoids two sources of truth — the database is the single source of truth for server behavior.

## Architecture

```
┌─────────────────────────────────────────────────┐
│ Tier 1: Bootstrap Config (pre-database)         │
│                                                  │
│  Priority (highest wins):                        │
│    CLI args                                      │
│    → Environment variables (DUSKCUE_*)      │
│    → config.toml file                            │
│    → Built-in defaults                           │
│                                                  │
│  Contains: database URL, directories, log level, │
│            environment name                      │
├─────────────────────────────────────────────────┤
│ Tier 2: Runtime Config (post-database)           │
│                                                  │
│  Source: server_config table (single row)        │
│  Cached in memory, refreshed on change           │
│                                                  │
│  Contains: ports, SSL, transcode, backup,        │
│            notifications, integrations, etc.     │
│  See DATABASE.md for full schema                 │
└─────────────────────────────────────────────────┘
```

## Libraries

| Crate | Version | Role |
|---|---|---|
| **config-rs** | 0.15.23 | Layered config builder — TOML/ENV merge, serde deserialization, file watching |
| **clap** | 4.6.1 | CLI argument parsing — derive macros, help generation, shell completions |

**config-rs** is maintained by the `rust-cli` working group (epage, mehcode). It supports layered sources where later sources override earlier: `Config::builder().add_source(defaults).add_source(file).add_source(env).build()`. Supports TOML, JSON, YAML, INI, RON, JSON5, Corn. Has file watching via the `notify` crate.

**clap** is the de facto standard CLI parser for Rust. Derive macros map CLI args to a struct with `#[arg(env = "PREFIX_KEY")]` for env var fallback. Provides help generation, shell completions, and validation.

## Config File Format: TOML

TOML chosen over YAML, JSON, INI:

| Criteria | TOML | YAML | JSON | INI |
|---|---|---|---|---|
| Human-readable | Yes | Yes | No (no comments) | Yes |
| Comments | Yes | Yes | No | Yes (`;`) |
| Nested structures | Yes | Yes | Yes | Limited |
| Type coercion surprises | No | Yes (Norway problem, implicit typing) | No | No |
| Rust ecosystem fit | Native (Cargo.toml) | Secondary | Native | Secondary |
| Schema complexity | Medium | High | Medium | Low |

## Bootstrap Config

### Config File

Default location: `{data_dir}/config/config.toml`

```toml
[server]
database_url = "postgresql://duskcue:password@localhost:5432/duskcue"
data_dir = "/var/lib/duskcue"
cache_dir = "/var/cache/duskcue"
log_level = "info"
environment = "production"
```

Only five fields. Everything else is in `server_config` after the database is reachable.

### Field Reference

| Field | Type | Default | ENV Override | CLI Override | Required |
|---|---|---|---|---|---|
| `database_url` | String | — | `DUSKCUE_DATABASE_URL` | `--database-url` | Conditional |

`database_url` is **conditional**: it is required in external database mode, but **not required** in embedded database mode. When `database_url` is absent from all sources (CLI, ENV, TOML):

- **Docker:** The entrypoint script detects the missing URL, starts embedded PostgreSQL, creates the database, and exports `DUSKCUE_DATABASE_URL` pointing to the Unix socket before executing the server binary
- **Non-Docker (native):** The `postgresql_embedded` crate (theseus-rs) downloads and manages PostgreSQL binaries at runtime, then provides the connection URL
- **External mode:** Set `database_url` explicitly to connect to any existing PostgreSQL instance (Docker sidecar, managed cloud DB, shared NAS database)

See [DOCKER_DEPLOYMENT.md](DOCKER_DEPLOYMENT.md) for the full embedded/external database strategy.
| `data_dir` | Path | Platform default | `DUSKCUE_DATA_DIR` | `--data-dir` | No |
| `cache_dir` | Path | `{data_dir}/cache` | `DUSKCUE_CACHE_DIR` | `--cache-dir` | No |
| `log_level` | String | `info` | `DUSKCUE_LOG_LEVEL` | `--log-level` | No |
| `environment` | String | `production` | `DUSKCUE_ENVIRONMENT` | `--environment` | No |

`environment` must be one of: `development`, `staging`, `production`. This controls error response verbosity as documented in ERROR_HANDLING.md.

### Platform Defaults

When `data_dir` is not specified, the server uses platform-appropriate defaults:

| Platform | `data_dir` | `config_dir` |
|---|---|---|
| Linux | `/var/lib/duskcue` | `{data_dir}/config` |
| macOS | `~/Library/Application Support/Duskcue` | `{data_dir}/config` |
| Windows | `%PROGRAMDATA%\Duskcue` | `{data_dir}/config` |
| Docker | `/data` (container volume) | `/data/config` |

### Docker Internal Directory Structure

Inside the Docker container, the directory structure is:

```
/data/                        # DUSKCUE_DATA_DIR
├── config/config.toml        # Bootstrap config (optional)
├── metadata/                 # Artwork, thumbnails (persistent)
├── logs/                     # Rolling JSON logs (persistent)
├── transcode/                # Temporary transcode files (tmpfs, purged on restart)
└── backups/                  # pg_dump logical backups (if local target)

/cache/                       # DUSKCUE_CACHE_DIR
├── hls/                      # HLS segment cache
├── images/                   # Processed image cache
├── storyboards/              # Seek preview thumbnail sprite sheets
└── search/                   # Search index artifacts

/media/                       # Bind mount point for libraries (read-only)
├── tv/                       # → host path
├── movies/                   # → host path
└── music/                    # → host path
```

Full Docker deployment documentation is in [DOCKER_DEPLOYMENT.md](DOCKER_DEPLOYMENT.md).

The config file is always at `{data_dir}/config/config.toml`. The `config_dir` is derived from `data_dir` unless explicitly overridden.

### Environment Variables

All bootstrap fields are overridable via environment variables with the `DUSKCUE_` prefix. This is the primary mechanism for Docker and Synology NAS deployments where editing config files is inconvenient.

```bash
DUSKCUE_DATABASE_URL="postgresql://user:pass@db:5432/duskcue"
DUSKCUE_DATA_DIR="/data"
DUSKCUE_LOG_LEVEL="debug"
DUSKCUE_ENVIRONMENT="production"
```

Environment variables override the TOML file but are overridden by CLI arguments.

### CLI Arguments

```
duskcue [OPTIONS]

Options:
      --database-url <URL>       PostgreSQL connection string
      --data-dir <PATH>          Server data directory
      --cache-dir <PATH>         Transcode/cache directory
      --log-level <LEVEL>        Log level (trace|debug|info|warn|error)
      --environment <ENV>        Runtime environment (development|staging|production)
      --config <PATH>            Path to config.toml (overrides default discovery)
  -h, --help                     Print help
  -V, --version                  Print version
```

The `--config` flag allows specifying a non-default config file location. This is itself only discoverable via CLI or `DUSKCUE_CONFIG` env var — it cannot be inside the config file.

### Layered Merge (config-rs)

```rust
fn build_bootstrap_config(cli: CliArgs) -> BootstrapConfig {
    let config_path = cli.config
        .or_else(|| env::var("DUSKCUE_CONFIG").ok().map(PathBuf::from))
        .unwrap_or_else(|| default_config_path());

    let config = Config::builder()
        .add_source(Config::try_from(&BootstrapDefaults::default()).unwrap())
        .add_source(config::File::from(config_path).required(false))
        .add_source(
            config::Environment::with_prefix("DUSKCUE")
                .prefix_separator("_")
                .separator("_")
        )
        .add_source(Config::try_from(&cli).unwrap())
        .build()
        .unwrap();

    config.try_deserialize().unwrap()
}
```

Sources are applied in order — each subsequent source overrides overlapping keys from the previous. The config file is `required(false)` so the server can start with only env vars (Docker scenario).

### BootstrapConfig Rust Struct

```rust
use clap::Parser;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "duskcue", version, about = "Self-hosted media streaming server")]
pub struct CliArgs {
    #[arg(long, env = "DUSKCUE_DATABASE_URL")]
    pub database_url: Option<String>,

    #[arg(long, env = "DUSKCUE_DATA_DIR")]
    pub data_dir: Option<PathBuf>,

    #[arg(long, env = "DUSKCUE_CACHE_DIR")]
    pub cache_dir: Option<PathBuf>,

    #[arg(long, env = "DUSKCUE_LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    #[arg(long, env = "DUSKCUE_ENVIRONMENT", default_value = "production")]
    pub environment: String,

    #[arg(long, env = "DUSKCUE_CONFIG")]
    pub config: Option<PathBuf>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BootstrapConfig {
    pub database_url: Option<String>,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub log_level: String,
    pub environment: String,
}
```

`database_url` is `Option<String>` in both `CliArgs` and `BootstrapConfig`. When `None` after layering, the server attempts to start embedded PostgreSQL (Docker entrypoint provides it, or `postgresql_embedded` crate handles it for native deployments).

## Runtime Config

After connecting to PostgreSQL, the server loads the single row from `server_config` and caches it in memory. This is the source of truth for all server behavior: HTTP/HTTPS ports, SSL, transcoding, backup, notifications, integrations, etc.

Full schema is in DATABASE.md (`server_config` table DDL).

### RuntimeConfig Rust Struct

Each JSONB column maps to a typed Rust struct. Typed columns are direct fields.

```rust
pub struct RuntimeConfig {
    pub server_name: String,
    pub base_url: Option<String>,
    pub http_port: u16,
    pub https_port: Option<u16>,
    pub ssl_certificate_path: Option<PathBuf>,
    pub ssl_private_key_path: Option<PathBuf>,

    pub network: NetworkConfig,
    pub transcoding: TranscodingConfig,
    pub metadata: MetadataConfig,
    pub auth: AuthConfig,
    pub security: SecurityConfig,
    pub notifications: NotificationConfig,
    pub backup: BackupConfig,
    pub integrations: IntegrationsConfig,
    pub logging: LoggingConfig,
    pub storage: StorageConfig,
    pub maintenance: MaintenanceConfig,
    pub resource_limits: ResourceLimitsConfig,
    pub cpu: CpuConfig,
    pub quality: QualityConfig,
    pub subtitles: SubtitleConfig,
}
```

`BackupConfig` is defined in BACKUP_RECOVERY.md. `StorageConfig` is defined in CACHE_STORAGE.md. `MaintenanceConfig` is defined in DATABASE_MAINTENANCE.md. `ResourceLimitsConfig` is defined in MEMORY.md. `CpuConfig` is defined in CPU.md. `QualityConfig` is defined below. `SubtitleConfig` is defined below. `AuthConfig` is defined below. `MetadataConfig` is defined in POSTER_MANAGEMENT.md and METADATA_PROVIDERS.md (expanded in Phase 6 with 22 fields including `ProviderConfig` for TMDB/TVDB/Fanart/OMDb). Other structs follow the same serde-deserialized pattern from JSONB. The full audio format catalog (codecs, channels, spatial audio, transcode targets) is documented in [AUDIO_FORMATS.md](../design/AUDIO_FORMATS.md).

### QualityConfig Rust Struct

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityConfig {
    pub capability_wizard_enabled: bool,
    pub network_probe_interval_minutes: i32,
    pub network_probe_browsing_interval_minutes: i32,
    pub network_probe_paused_interval_minutes: i32,
    pub network_probe_bytes: i64,
    pub throughput_estimate_window: i32,
    pub throughput_safety_factor: f64,
    pub default_transcode_codec: String,
    pub fallback_max_resolution: String,
    pub fallback_max_bitrate_bps: i64,
    pub qoe_report_interval_seconds: i32,
    pub allow_client_side_dv_fallback: bool,
    pub tone_mapping_algorithm: String,
    pub tone_mapping_peak_nits: i32,
    pub audio_passthrough_enabled: bool,
    pub subtitle_burn_in_policy: String,
    pub default_quality_mode: String,
}

impl Default for QualityConfig {
    fn default() -> Self {
        Self {
            capability_wizard_enabled: true,
            network_probe_interval_minutes: 5,
            network_probe_browsing_interval_minutes: 15,
            network_probe_paused_interval_minutes: 10,
            network_probe_bytes: 102400,
            throughput_estimate_window: 5,
            throughput_safety_factor: 0.8,
            default_transcode_codec: "h264".to_string(),
            fallback_max_resolution: "1080p".to_string(),
            fallback_max_bitrate_bps: 6_000_000,
            qoe_report_interval_seconds: 30,
            allow_client_side_dv_fallback: true,
            tone_mapping_algorithm: "bt2390".to_string(),
            tone_mapping_peak_nits: 100,
            audio_passthrough_enabled: true,
            subtitle_burn_in_policy: "last_resort".to_string(),
            default_quality_mode: "auto".to_string(),
        }
    }
}
```

**Field semantics:**
- `capability_wizard_enabled` — whether the capability wizard is offered to new devices. When disabled, only client self-reports and the known device database are used.
- `network_probe_interval_minutes` — how often the client downloads a probe payload during active playback (default: 5 minutes).
- `network_probe_browsing_interval_minutes` — probe interval during library browsing, no active stream (default: 15 minutes).
- `network_probe_paused_interval_minutes` — probe interval while playback is paused (default: 10 minutes).
- `network_probe_bytes` — size of the probe payload (default: 100 KB). Large enough for accurate measurement, small enough to be negligible on metered connections.
- `throughput_estimate_window` — number of recent measurements used for the harmonic mean estimate (default: 5 segments). Resistant to outlier segments.
- `throughput_safety_factor` — multiplied against estimated throughput before comparing to ABR rung bitrates (default: 0.8). Prevents selecting a rung that's too close to the measured throughput, which would cause rebuffers.
- `default_transcode_codec` — the codec used when transcoding is needed (default: `h264` for universal compatibility). Options: `h264`, `hevc`, `av1`.
- `fallback_max_resolution` — maximum resolution for the conservative fallback device profile used when no device profile exists (default: `1080p`).
- `fallback_max_bitrate_bps` — maximum bitrate for the fallback profile (default: 6 Mbps, matching the 1080p ABR rung).
- `qoe_report_interval_seconds` — how often the client sends QoE metrics during playback (default: 30 seconds).
- `allow_client_side_dv_fallback` — when true, allows direct play of Dolby Vision Profile 7 content to devices that support HDR10 (even if they don't support DV). The client's video decoder handles DV→HDR10 fallback automatically. Default: true.
- `tone_mapping_algorithm` — algorithm for HDR→SDR conversion. Options: `bt2390` (default, correct standard), `libplacebo` (best quality, requires Vulkan). Never Hable/Mobius/Reinhard.
- `tone_mapping_peak_nits` — target peak luminance for tone-mapped output in nits. Default: 100 (standard SDR display). Range: 100–300.
- `audio_passthrough_enabled` — when true, audio codecs (TrueHD, DTS-HD MA, etc.) are passed through unmodified when the device reports support. Never deprioritizes audio codecs for HLS streaming. Default: true.
- `subtitle_burn_in_policy` — controls when subtitles are burned into video. Options: `last_resort` (default, only PGS/VobSub when client can't overlay), `never` (reject playback instead of burn-in), `always` (burn in all subtitles). Text-based subtitles are never burned in regardless of this setting.
- `default_quality_mode` — quality mode for new users/devices. Options: `auto` (default, Netflix-like adaptation), `maximum` (always highest quality), `manual` (user picks resolution).

### SubtitleConfig Rust Struct

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubtitleConfig {
    pub ocr_enabled: bool,
    pub ocr_engine: String,
    pub ocr_confidence_threshold: f64,
    pub voice_activity_analysis: bool,
    pub voice_activity_schedule: String,
    pub default_subtitle_mode: String,
    pub default_subtitle_language: String,
    pub auto_fetch_enabled: bool,
    pub auto_fetch_languages: Vec<String>,
}

impl Default for SubtitleConfig {
    fn default() -> Self {
        Self {
            ocr_enabled: true,
            ocr_engine: "paddleocr".to_string(),
            ocr_confidence_threshold: 0.80,
            voice_activity_analysis: false,
            voice_activity_schedule: "0 5 * * *".to_string(),
            default_subtitle_mode: "default".to_string(),
            default_subtitle_language: "en".to_string(),
            auto_fetch_enabled: false,
            auto_fetch_languages: vec![],
        }
    }
}
```

**Field semantics:**
- `ocr_enabled` — enable PGS/VobSub OCR during library scan. Requires PaddleOCR (Python) or Tesseract. Default: true.
- `ocr_engine` — primary OCR engine. Options: `paddleocr` (default, best accuracy), `tesseract` (fallback). If the primary engine is unavailable, the other is tried automatically.
- `ocr_confidence_threshold` — below this score (0.0–1.0), admin is warned to review the OCR result. Default: 0.80.
- `voice_activity_analysis` — enable voice activity detection for automatic subtitle sync (Plex-style). CPU-intensive background task. Default: false.
- `voice_activity_schedule` — cron schedule for voice activity analysis. Default: `0 5 * * *` (daily at 05:00, after library scan).
- `default_subtitle_mode` — default subtitle mode for new users. Options: `default` (auto-select if audio ≠ user language), `always`, `none`, `forced_only`. Default: `default`.
- `default_subtitle_language` — default subtitle language (ISO 639-1) for new users. Default: `en`.
- `auto_fetch_enabled` — enable auto-download of subtitles from external providers during scan. Provider credentials configured in `server_config.integrations.subtitle_providers`. Default: false.
- `auto_fetch_languages` — languages to auto-fetch. Empty array = disabled regardless of `auto_fetch_enabled`. Default: `[]`.

Full subtitle domain design documented in [SUBTITLES.md](../design/SUBTITLES.md).

### AuthConfig Rust Struct

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthConfig {
    pub network_mode: NetworkMode,
    pub rp_id: Option<String>,
    pub rp_origin: Option<String>,
    pub setup_complete: bool,
    pub auth_required: bool,
    pub require_https: bool,
    pub max_login_attempts: i32,
    pub lockout_duration_minutes: i32,
    pub invite_code_length: usize,
    pub invite_code_default_expiry_days: i32,
    pub invite_code_max_attempts_per_ip: i32,
    pub invite_code_attempt_window_minutes: i32,
    pub device_linking_code_length: usize,
    pub device_linking_code_expiry_seconds: i32,
    pub device_linking_poll_interval_seconds: i32,
    pub reauth_code_length: usize,
    pub reauth_code_expiry_hours: i32,
    pub reauth_max_requests_per_user_per_day: i32,
    pub session_absolute_timeout_days: i32,
    pub session_idle_timeout_hours: Option<i32>,
    pub session_renewal_timeout_hours: i32,
    pub rate_limits: RateLimitConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RateLimitConfig {
    pub global_per_minute: u32,
    pub global_burst: u32,
    pub auth_per_minute: u32,
    pub auth_burst: u32,
    pub authenticated_per_minute: u32,
    pub authenticated_burst: u32,
    pub streaming_per_minute: u32,
    pub streaming_burst: u32,
    pub admin_per_minute: u32,
    pub admin_burst: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    Local,
    Exposed,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            network_mode: NetworkMode::Local,
            rp_id: None,
            rp_origin: None,
            setup_complete: false,
            auth_required: false,
            require_https: false,
            max_login_attempts: 5,
            lockout_duration_minutes: 30,
            invite_code_length: 24,
            invite_code_default_expiry_days: 30,
            invite_code_max_attempts_per_ip: 5,
            invite_code_attempt_window_minutes: 15,
            device_linking_code_length: 8,
            device_linking_code_expiry_seconds: 900,
            device_linking_poll_interval_seconds: 5,
            reauth_code_length: 16,
            reauth_code_expiry_hours: 24,
            reauth_max_requests_per_user_per_day: 3,
            session_absolute_timeout_days: 90,
            session_idle_timeout_hours: None,
            session_renewal_timeout_hours: 720,
            rate_limits: RateLimitConfig::default(),
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            global_per_minute: 100,
            global_burst: 50,
            auth_per_minute: 10,
            auth_burst: 5,
            authenticated_per_minute: 300,
            authenticated_burst: 100,
            streaming_per_minute: 600,
            streaming_burst: 50,
            admin_per_minute: 1000,
            admin_burst: 200,
        }
    }
}
```

### SecurityConfig

Full design in [SECURITY.md](../security/SECURITY.md).

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SecurityConfig {
    pub allowed_origins: Vec<String>,
    pub tls: TlsConfig,
    pub stream_signing: StreamSigningConfig,
    pub vpn_detection: VpnDetectionConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub port: u16,
    pub acme_directory: String,
    pub acme_email: String,
    pub challenge_type: AcmeChallengeType,
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    pub hsts_max_age_seconds: u32,
    pub min_tls_version: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcmeChallengeType {
    Http01,
    Dns01,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StreamSigningConfig {
    pub enabled: bool,
    pub manifest_ttl_seconds: u64,
    pub segment_ttl_seconds: u64,
    pub key_rotation_hours: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VpnDetectionConfig {
    pub auto_detect: bool,
    pub vpn_interfaces: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec![],
            tls: TlsConfig {
                enabled: false,
                port: 443,
                acme_directory: "https://acme-v02.api.letsencrypt.org/directory".to_string(),
                acme_email: String::new(),
                challenge_type: AcmeChallengeType::Http01,
                cert_path: None,
                key_path: None,
                hsts_max_age_seconds: 63072000,
                min_tls_version: "1.2".to_string(),
            },
            stream_signing: StreamSigningConfig {
                enabled: false,
                manifest_ttl_seconds: 60,
                segment_ttl_seconds: 300,
                key_rotation_hours: 24,
            },
            vpn_detection: VpnDetectionConfig {
                auto_detect: true,
                vpn_interfaces: vec![
                    "tun0".into(),
                    "wg0".into(),
                    "utun".into(),
                    "tailscale0".into(),
                ],
            },
        }
    }
}
```

**SecurityConfig field semantics:**
- `allowed_origins` — CORS allowed origins for exposed mode. Empty in local mode. Full CORS design in [API_CONVENTIONS.md](../design/API_CONVENTIONS.md).
- `tls.enabled` — `true` when `network_mode = "exposed"`. Auto-set during remote access setup.
- `tls.port` — HTTPS port. Default 443.
- `tls.acme_directory` — Let's Encrypt production or staging directory URL. Staging used during initial setup.
- `tls.acme_email` — Email for Let's Encrypt notifications (expiry warnings).
- `tls.challenge_type` — HTTP-01 (default, requires port 80) or DNS-01 (for behind-reverse-proxy setups).
- `tls.cert_path` / `tls.key_path` — Custom certificate paths. When `None`, ACME-managed certs stored in `data_dir/tls/`.
- `tls.hsts_max_age_seconds` — HSTS max-age. Default 63072000 (2 years). Per MDN, 2 years is recommended for production.
- `tls.min_tls_version` — Minimum TLS version. Default `"1.2"`. `"1.3"` can be set for strict environments.
- `stream_signing.enabled` — `true` when `network_mode = "exposed"`. HMAC-SHA256 signed streaming URLs.
- `stream_signing.manifest_ttl_seconds` — Signed manifest URL lifetime. Default 60 seconds.
- `stream_signing.segment_ttl_seconds` — Signed segment wildcard path lifetime. Default 300 seconds.
- `stream_signing.key_rotation_hours` — HMAC signing key rotation interval. Default 24 hours.
- `vpn_detection.auto_detect` — Auto-detect VPN interfaces at startup. Used for admin UI network status.
- `vpn_detection.vpn_interfaces` — Interface names to check for active VPN connections.

**AuthConfig field semantics:**
- `network_mode` — `"local"` (default) or `"exposed"`. Controls security enforcement level.
- `rp_id` / `rp_origin` — WebAuthn Relying Party ID and origin. Auto-detected during setup. Changing `rp_id` breaks existing passkeys.
- `setup_complete` — `false` until the first owner account is created. When `false`, only `POST /api/v1/setup` is accessible.
- `auth_required` — In local mode, can be `false` (all requests run as owner). Forced to `true` in exposed mode.
- `require_https` — Forced to `true` in exposed mode. In local mode, `false` by default.
- `max_login_attempts` / `lockout_duration_minutes` — Account lockout thresholds. After `max_login_attempts` consecutive failures, the account is locked for `lockout_duration_minutes`.
- `invite_code_length` — Number of base-20 characters in invite codes. Default 24 (~103 bits entropy). Per RFC 8628 Section 6.1 base-20 charset `BCDFGHJKLMNPQRSTVWXZ`.
- `invite_code_default_expiry_days` — Default expiry for new invite codes. Admin can override per-invite.
- `invite_code_max_attempts_per_ip` / `invite_code_attempt_window_minutes` — Rate limiting for invite code verification. Max failed attempts per IP within the window. Per RFC 8628 Section 5.1.
- `device_linking_code_length` — Number of base-20 characters in device linking user codes. Default 8 (~34.5 bits entropy). Per RFC 8628 Section 6.1.
- `device_linking_code_expiry_seconds` — Device linking code lifetime. Default 900 (15 minutes). Short lifetime limits phishing per RFC 8628 Section 5.4.
- `device_linking_poll_interval_seconds` — Device polling interval. Default 5 seconds. Per RFC 8628 Section 3.2.
- `reauth_code_length` — Number of base-20 characters in re-authentication codes. Default 16 (~69 bits entropy). Shorter than invite codes since they're short-lived and rate-limited.
- `reauth_code_expiry_hours` — Re-authentication code lifetime. Default 24 hours.
- `reauth_max_requests_per_user_per_day` — Rate limit for re-auth code requests. Default 3 per user per 24 hours.
- `session_absolute_timeout_days` — Maximum session lifetime regardless of activity. Default 90 days.
- `session_idle_timeout_hours` — Session expires after this many hours of inactivity. `None` means no idle timeout (default). Used for shared devices.
- `session_renewal_timeout_hours` — How often a session token is automatically renewed on activity. Default 720 hours (30 days).
- `rate_limits` — HTTP-layer rate limiting configuration (governor). Full design in [API_CONVENTIONS.md](../design/API_CONVENTIONS.md). All values are per-minute with burst capacity. Changes take effect on next request (no restart).
- `session_absolute_timeout_days` — Maximum session lifetime regardless of activity. Default 90 days (3 months). Per NIST SP 800-63B AAL1, 30 days is recommended for exposed mode.
- `session_idle_timeout_hours` — Session inactivity timeout. `None` means no idle timeout (default for local mode). For exposed mode, consider 168 hours (7 days). Per NIST SP 800-63B, idle timeout is not required at AAL1 but recommended at higher levels.
- `session_renewal_timeout_hours` — How often the session ID is regenerated during an active session. Default 720 hours (30 days). Per OWASP Session Management Cheat Sheet, renewal timeout complements absolute timeout for long-lived sessions.

### Cache and Reload

- `RuntimeConfig` is wrapped in `Arc<RwLock<RuntimeConfig>>` (or `arc-swap`) for concurrent access
- Loaded once at startup after DB connection is established
- Admin API changes (`PUT /api/v1/server/config`) write to `server_config` table, then trigger a cache reload
- File-based hot-reload is NOT used for runtime config — changes go through the API to maintain the audit trail (`audit_log` trigger on `server_config`)
- The audit trigger on `server_config` is already defined in DATABASE.md

## Startup Sequence

```
 1. Parse CLI arguments                        ← clap
 2. Discover config file path                  ← CLI > ENV > platform default
 3. Build BootstrapConfig                      ← config-rs layered merge
 4. Validate: database_url present?            ← fail-fast if missing
 5. Initialize logging at bootstrap level      ← tracing-subscriber
 6. Acquire startup lockfile                   ← /data/.duskcue.lock (prevent concurrent instances)
 7. Connect to PostgreSQL                      ← sqlx::PgPool
 8. Validate PostgreSQL settings               ← fsync, full_page_writes, data_checksums, wal_level
    └── Warn if mismatch (never block startup)
 9. Run pending migrations                     ← sqlx-cli embedded
10. Load server_config row                     ← RuntimeConfig
    ├── Row exists → deserialize into RuntimeConfig, cache in memory
    └── No row → seed with defaults, launch setup wizard
11. Check auth setup state
    ├── auth.setup_complete = false → enter setup mode (only POST /api/v1/setup accessible)
    └── auth.setup_complete = true  → normal auth enforcement
12. Start scheduled task runner
13. Bind HTTP/HTTPS listeners                 ← ports from RuntimeConfig
14. Ready
```

**Steps 6-8 are new** — startup lockfile prevents concurrent instances sharing a PG data directory; PostgreSQL settings validation warns if durability settings are misconfigured. Both are non-blocking (warn only). Full details in [MEMORY.md](../design/MEMORY.md).

### Fail-Fast Rules

- If `database_url` is missing after layering and embedded PG is not available: print error + example config, exit code 1
- If `database_url` is missing but embedded PG is available (Docker or `postgresql_embedded`): auto-start embedded PG and provide connection URL
- If config.toml is malformed: print parse error with line number, exit code 1
- If database is unreachable: retry with backoff (default 3 attempts, 5s interval), then fail with clear message
- If migrations fail: fail immediately (no partial schema state), exit code 1
- If `environment` is not one of `development`/`staging`/`production`: fail with message, exit code 1

## First-Run Setup

When `server_config` has no rows (fresh database):

1. Server detects empty `server_config` table
2. Seeds a single row with sensible defaults:
   - `server_name`: `"My Duskcue"`
   - `http_port`: `48027`
   - All JSONB groups: empty `{}` (domain defaults applied by Rust structs)
3. Starts HTTP listener on port 48027
4. All API endpoints return `503 Service Unavailable` except `/api/v1/setup/*`
5. Setup wizard guides the admin through:
   - Create admin account (passkey registration)
   - Set server name
   - Configure networking (ports, SSL)
   - Add first library
6. On completion, server reloads `RuntimeConfig` and begins normal operation

This means a fresh Docker container only needs:
```bash
docker run -e DUSKCUE_DATABASE_URL="postgresql://..." -p 48027:48027 duskcue
```

The setup wizard handles everything else through the browser.

## Docker / Synology Integration

Full Docker deployment documentation is in [DOCKER_DEPLOYMENT.md](DOCKER_DEPLOYMENT.md).

### Docker

The minimum Docker configuration requires only media paths. PostgreSQL is embedded inside the container — no separate database service or `DUSKCUE_DATABASE_URL` needed:

```bash
docker run -v /path/to/movies:/media/movies:ro -p 48027:48027 duskcue
```

The entrypoint automatically initializes PostgreSQL, creates the database, and provides the connection URL to the server. To use an external database instead, set `DUSKCUE_DATABASE_URL`:

```bash
docker run -e DUSKCUE_DATABASE_URL="postgresql://user:pass@db-host:5432/duskcue" -p 48027:48027 duskcue
```

The production `docker-compose.yml` includes PostgreSQL, healthchecks, named volumes, PUID/PGID user mapping, and hardware acceleration options. See [DOCKER_DEPLOYMENT.md](DOCKER_DEPLOYMENT.md) for the full compose file and `.env.example`.

### Synology NAS

Synology users can either:
1. Place `config.toml` in the shared folder mapped to `data_dir`
2. Set env vars in the Docker UI (Synology Container Manager)
3. Use CLI arguments in a startup script
4. Use Synology Container Manager's "Project" feature with `docker-compose.yml`

All methods work identically — same layered merge.

## File Discovery

The `--config` flag and `DUSKCUE_CONFIG` env var allow specifying a non-default config file path:

```
Priority:
  1. --config /path/to/config.toml          (CLI flag)
  2. DUSKCUE_CONFIG=/path/to/config.toml  (env var)
  3. {data_dir}/config/config.toml           (platform default)
```

When using the platform default, the server creates the `config/` directory if it doesn't exist and writes a commented example `config.toml` on first run.

## Example Config File

```toml
# Duskcue Configuration
# Only bootstrap settings go here.
# All other settings are managed through the web UI and stored in the database.

[server]
# PostgreSQL connection string (required)
database_url = "postgresql://duskcue:changeme@localhost:5432/duskcue"

# Server data directory — media metadata, database state, transcodes
# Default: platform-specific (see docs)
# data_dir = "/var/lib/duskcue"

# Cache directory — transcodes, temporary files
# Default: {data_dir}/cache
# cache_dir = "/var/cache/duskcue"

# Log level: trace, debug, info, warn, error
# Default: info
# log_level = "info"

# Environment: development, staging, production
# Affects error response verbosity (see ERROR_HANDLING.md)
# Default: production
# environment = "production"
```

## Hot-Reload

| Source | Hot-Reload? | Mechanism |
|---|---|---|
| `config.toml` | Not watched | Restart required. Bootstrap config is read once at startup. |
| `server_config` (DB) | Yes | Admin API writes trigger cache reload via `ArcSwap<RuntimeConfig>` |
| ENV vars | No | Restart required (standard behavior) |
| CLI args | No | Restart required (standard behavior) |

Hot-reload is only supported for runtime config via the admin API. This maintains the audit trail — every config change is recorded in `audit_log`. File-based hot-reload would bypass the audit system.

## Integration with Existing Systems

### Error Handling (ERROR_HANDLING.md)

The `environment` bootstrap field maps directly to error response behavior:
- `development` → full stack traces, internal details, all context
- `staging` → error codes with limited context
- `production` → error codes only, no internal details

### Backup & Recovery (BACKUP_RECOVERY.md)

`server_config.backup` JSONB is part of runtime config. `BackupConfig` Rust struct is loaded from the database, not from the TOML file. This means backup configuration is managed through the admin UI, not by editing config files.

### Migration Strategy (MIGRATION_STRATEGY.md)

The `database_url` from bootstrap config is passed to sqlx for both the connection pool and embedded migrations. Migrations run automatically at startup (step 7 in the startup sequence) before loading runtime config.

### Logging

Initial log level comes from bootstrap config. After runtime config loads, `server_config.logging` may specify a different level — the server updates the tracing subscriber at that point. This ensures migration output is always visible at the bootstrap level.

## Why Not a Bigger Config File?

Common alternatives considered:

**"Put everything in the TOML file"** — Creates two sources of truth. If `http_port` is in both the file and the database, which wins? How do you sync them? This is the #1 complaint about Jellyfin's XML config files.

**"File is source of truth, sync to DB"** — Adds complexity for no gain. The DB is already our config store, has an audit trail, and is editable from the web UI. A file sync layer would be fragile.

**"ENV vars for everything (Twelve-Factor)"** — The Twelve-Factor App recommends env vars for all config. This works for cloud-native services, but a self-hosted Duskcue has dozens of settings (transcode paths, hardware accel, backup retention, notification SMTP). A TOML file is more practical for the initial connection, and the web UI is more practical for everything else. ENV vars are available as an override for Docker.

## Implementation Status

**Phase 1 (complete):** The bootstrap config layer is fully implemented in `server/src/config.rs`:

- `CliArgs` struct with clap derive, all six fields with `DUSKCUE_` env var support
- `BootstrapConfig` struct with serde Deserialize
- `build_bootstrap_config()` function with config-rs layered merge
- Platform-aware `data_dir` defaults (Windows/macOS/Linux)
- Environment validation (`development`/`staging`/`production`)
- `set_override_option` for optional `database_url` field

**Phase 3 (in progress):** The runtime config layer and `AppState` are partially implemented in `server/src/state.rs`:

- `AppState` struct with `Clone` — holds `PgPool`, `Arc<ArcSwap<RuntimeConfig>>`, `BootstrapConfig`
- `RuntimeConfig` struct with all 21 fields matching `server_config` table columns
- 5 fully-defined sub-configs: `AuthConfig` (with `RateLimitConfig`, `NetworkMode`), `SecurityConfig` (with `TlsConfig`, `StreamSigningConfig`, `VpnDetectionConfig`, `AcmeChallengeType`), `QualityConfig`, `SubtitleConfig`, `ResourceLimitsConfig`
- 1 expanded sub-config (Phase 6): `MetadataConfig` — 22 fields covering artwork, overlays, collections, and provider configuration (`ProviderConfig` with `TmdbProviderConfig` + `OptionalProviderConfig` for TVDB/Fanart/OMDb); defined in POSTER_MANAGEMENT.md and METADATA_PROVIDERS.md
- 9 placeholder sub-configs with `Default`: `NetworkConfig`, `TranscodingConfig`, `NotificationConfig`, `BackupConfig`, `IntegrationsConfig`, `LoggingConfig`, `StorageConfig`, `MaintenanceConfig`, `CpuConfig` — expanded in their respective domain phases
- `load_runtime_config(pool)` — queries `server_config` table, deserializes JSONB columns with `unwrap_or_default()` fallback, returns `RuntimeConfig::default()` for empty table (first-run)
- `AppState::reload_runtime_config()` — atomic swap via `ArcSwap` for admin API config changes
- `arc-swap` v1.9.1 added as workspace dependency for lock-free config reads
- Environment detection wired: `set_environment()` in `error.rs` uses `OnceLock<String>` set during `AppState` construction

**Phase 3 — 14-step startup sequence (Task 7, complete):** The startup sequence is implemented in `server/src/main.rs`:

- PgPoolOptions configured per MEMORY.md: `max_connections(20)`, `min_connections(2)`, `acquire_timeout(5s)`, `max_lifetime(30min)`, `idle_timeout(10min)`, `after_connect` sets `application_name = 'duskcue'`
- Database connection retry: 3 attempts, 5s interval between retries
- Automatic schema migration via `sqlx::migrate!()` (compile-time embedded from `server/migrations/`)
- PostgreSQL settings validation: queries `pg_settings` for `fsync`, `full_page_writes`, `synchronous_commit`, `data_checksums`, `wal_level`; detects PG version via `current_setting('server_version')` and warns if below target version 18; logs WARN for mismatches with warning count summary; non-blocking
- Runtime config loaded from `server_config` table via `load_runtime_config()`; `AppState::new_with_config()` initializes rate limits from DB config
- Auth setup state checked: if `setup_complete = false`, logs WARN about setup mode
- `sqlx` workspace features updated: added `migrate` and `sqlx-toml`

**Phase 5 Tasks 5-6 (complete):**

- `workers/library_scanner.rs` — 6-phase scanning pipeline (discover, diff, probe, identify, enrich stub, cleanup)
- `services/scheduler.rs` — Scheduled task runner with `croner` v3 cron evaluation, 30-second tick interval, builder-pattern executor registration
- Scheduler wired into `main.rs` startup: seeds 8 default tasks, registers `library_scan` executor, starts with `TaskTracker` + `CancellationToken` for graceful shutdown
- Crates added to workspace: `ignore` 0.4, `blake3` 1, `regex` 1, `croner` 3
- `services/mod.rs` wired with `pub mod scheduler;`

**Not yet implemented:**

- Admin API endpoint (`PUT /api/v1/server/config`) that triggers `reload_runtime_config()` — Phase 13
