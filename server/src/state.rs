// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use crate::config::BootstrapConfig;
use crate::error::set_environment;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub allowed_origins: Vec<String>,
    pub tls: TlsConfig,
    pub stream_signing: StreamSigningConfig,
    pub vpn_detection: VpnDetectionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcmeChallengeType {
    Http01,
    Dns01,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamSigningConfig {
    pub enabled: bool,
    pub manifest_ttl_seconds: u64,
    pub segment_ttl_seconds: u64,
    pub key_rotation_hours: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkConfig {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranscodingConfig {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetadataConfig {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationConfig {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackupConfig {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntegrationsConfig {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoggingConfig {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageConfig {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MaintenanceConfig {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CpuConfig {}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            server_name: "My Duskcue".to_string(),
            base_url: None,
            http_port: 48027,
            https_port: None,
            ssl_certificate_path: None,
            ssl_private_key_path: None,
            network: NetworkConfig::default(),
            transcoding: TranscodingConfig::default(),
            metadata: MetadataConfig::default(),
            auth: AuthConfig::default(),
            security: SecurityConfig::default(),
            notifications: NotificationConfig::default(),
            backup: BackupConfig::default(),
            integrations: IntegrationsConfig::default(),
            logging: LoggingConfig::default(),
            storage: StorageConfig::default(),
            maintenance: MaintenanceConfig::default(),
            resource_limits: ResourceLimitsConfig::default(),
            cpu: CpuConfig::default(),
            quality: QualityConfig::default(),
            subtitles: SubtitleConfig::default(),
        }
    }
}

impl RuntimeConfig {
    pub fn is_development(&self) -> bool {
        false
    }

    pub fn is_local_mode(&self) -> bool {
        matches!(self.auth.network_mode, NetworkMode::Local)
    }

    pub fn is_setup_mode(&self) -> bool {
        !self.auth.setup_complete
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub runtime_config: Arc<ArcSwap<RuntimeConfig>>,
    pub bootstrap: BootstrapConfig,
}

impl AppState {
    pub fn new(pool: PgPool, bootstrap: BootstrapConfig) -> Self {
        set_environment(bootstrap.environment.clone());
        Self {
            pool,
            runtime_config: Arc::new(ArcSwap::from_pointee(RuntimeConfig::default())),
            bootstrap,
        }
    }

    pub fn new_with_config(
        pool: PgPool,
        bootstrap: BootstrapConfig,
        runtime_config: RuntimeConfig,
    ) -> Self {
        set_environment(bootstrap.environment.clone());
        Self {
            pool,
            runtime_config: Arc::new(ArcSwap::from_pointee(runtime_config)),
            bootstrap,
        }
    }

    pub fn reload_runtime_config(&self, new_config: RuntimeConfig) {
        self.runtime_config.store(Arc::new(new_config));
        tracing::info!("Runtime configuration reloaded");
    }
}

pub async fn load_runtime_config(pool: &PgPool) -> Result<RuntimeConfig, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT
            server_name,
            base_url,
            http_port,
            https_port,
            ssl_certificate_path,
            ssl_private_key_path,
            network,
            transcoding,
            metadata,
            auth,
            security,
            notifications,
            backup,
            integrations,
            logging,
            storage,
            maintenance,
            resource_limits,
            cpu,
            quality,
            subtitles
        FROM server_config
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(RuntimeConfig::default());
    };

    let server_name: String = row.try_get("server_name").unwrap_or_else(|_| "My Duskcue".to_string());
    let base_url: Option<String> = row.try_get("base_url").unwrap_or(None);
    let http_port: i32 = row.try_get("http_port").unwrap_or(48027);
    let https_port: Option<i32> = row.try_get("https_port").unwrap_or(None);
    let ssl_certificate_path: Option<String> = row.try_get("ssl_certificate_path").unwrap_or(None);
    let ssl_private_key_path: Option<String> = row.try_get("ssl_private_key_path").unwrap_or(None);

    let network: serde_json::Value = row.try_get("network").unwrap_or(serde_json::Value::Object(Default::default()));
    let transcoding: serde_json::Value = row.try_get("transcoding").unwrap_or(serde_json::Value::Object(Default::default()));
    let metadata: serde_json::Value = row.try_get("metadata").unwrap_or(serde_json::Value::Object(Default::default()));
    let auth: serde_json::Value = row.try_get("auth").unwrap_or(serde_json::Value::Object(Default::default()));
    let security: serde_json::Value = row.try_get("security").unwrap_or(serde_json::Value::Object(Default::default()));
    let notifications: serde_json::Value = row.try_get("notifications").unwrap_or(serde_json::Value::Object(Default::default()));
    let backup: serde_json::Value = row.try_get("backup").unwrap_or(serde_json::Value::Object(Default::default()));
    let integrations: serde_json::Value = row.try_get("integrations").unwrap_or(serde_json::Value::Object(Default::default()));
    let logging: serde_json::Value = row.try_get("logging").unwrap_or(serde_json::Value::Object(Default::default()));
    let storage: serde_json::Value = row.try_get("storage").unwrap_or(serde_json::Value::Object(Default::default()));
    let maintenance: serde_json::Value = row.try_get("maintenance").unwrap_or(serde_json::Value::Object(Default::default()));
    let resource_limits: serde_json::Value = row.try_get("resource_limits").unwrap_or(serde_json::Value::Object(Default::default()));
    let cpu: serde_json::Value = row.try_get("cpu").unwrap_or(serde_json::Value::Object(Default::default()));
    let quality: serde_json::Value = row.try_get("quality").unwrap_or(serde_json::Value::Object(Default::default()));
    let subtitles: serde_json::Value = row.try_get("subtitles").unwrap_or(serde_json::Value::Object(Default::default()));

    Ok(RuntimeConfig {
        server_name,
        base_url,
        http_port: http_port as u16,
        https_port: https_port.map(|p| p as u16),
        ssl_certificate_path: ssl_certificate_path.map(PathBuf::from),
        ssl_private_key_path: ssl_private_key_path.map(PathBuf::from),
        network: serde_json::from_value(network).unwrap_or_default(),
        transcoding: serde_json::from_value(transcoding).unwrap_or_default(),
        metadata: serde_json::from_value(metadata).unwrap_or_default(),
        auth: serde_json::from_value(auth).unwrap_or_default(),
        security: serde_json::from_value(security).unwrap_or_default(),
        notifications: serde_json::from_value(notifications).unwrap_or_default(),
        backup: serde_json::from_value(backup).unwrap_or_default(),
        integrations: serde_json::from_value(integrations).unwrap_or_default(),
        logging: serde_json::from_value(logging).unwrap_or_default(),
        storage: serde_json::from_value(storage).unwrap_or_default(),
        maintenance: serde_json::from_value(maintenance).unwrap_or_default(),
        resource_limits: serde_json::from_value(resource_limits).unwrap_or_default(),
        cpu: serde_json::from_value(cpu).unwrap_or_default(),
        quality: serde_json::from_value(quality).unwrap_or_default(),
        subtitles: serde_json::from_value(subtitles).unwrap_or_default(),
    })
}
