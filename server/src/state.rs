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
use std::str::FromStr;
use std::sync::Arc;

use arc_swap::ArcSwap;
use dashmap::DashMap;
use ipnet::IpNet;
use metrics_exporter_prometheus::PrometheusHandle;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use webauthn_rs::prelude::{PasskeyAuthentication, PasskeyRegistration, Webauthn};

use crate::config::BootstrapConfig;
use crate::error::set_environment;
use crate::middleware::RateLimitState;
use crate::services::encryption::EncryptionKey;
use crate::services::event_bus::EventBus;
use crate::services::fs_watcher::LibraryWatcherManager;
use crate::services::geoip::GeoIpService;
use crate::services::metadata::EnrichmentOrchestrator;
use crate::services::scheduler::Scheduler;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub allowed_metrics_subnets: Vec<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            allowed_metrics_subnets: vec!["127.0.0.1/32".to_string(), "::1/128".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscodingConfig {
    pub hardware_accel: String,
    pub transcode_path: String,
    pub max_concurrent_transcodes: u32,
    pub segment_duration_seconds: u32,
    pub allow_hw_tone_mapping: bool,
    pub allow_hw_subtitle_burn_in: bool,
    pub default_video_codec: String,
    pub default_audio_codec: String,
    pub max_downscale_resolution: String,
    pub enable_thumb_extraction: bool,
    pub thread_count: Option<u32>,
    pub thread_type: String,
    pub prefer_hw_decode: bool,
    #[serde(default)]
    pub segment_detection_enabled: bool,
    #[serde(default)]
    pub segment_safety: SegmentSafetyConfig,
    #[serde(default)]
    pub segment_analysis: SegmentAnalysisConfig,
    #[serde(default)]
    pub storyboards_enabled: bool,
    #[serde(default = "default_storyboard_interval_mode")]
    pub storyboard_interval_mode: String,
    #[serde(default = "default_storyboard_fixed_interval_seconds")]
    pub storyboard_fixed_interval_seconds: u32,
    #[serde(default = "default_storyboard_width")]
    pub storyboard_width: u32,
    #[serde(default = "default_storyboard_quality")]
    pub storyboard_quality: u32,
    #[serde(default = "default_storyboard_keyframe_only")]
    pub storyboard_keyframe_only: bool,
    #[serde(default = "default_storyboard_sprite_columns")]
    pub storyboard_sprite_columns: u32,
    #[serde(default = "default_storyboard_sprite_rows")]
    pub storyboard_sprite_rows: u32,
}

fn default_storyboard_interval_mode() -> String {
    "adaptive".to_string()
}
fn default_storyboard_fixed_interval_seconds() -> u32 {
    10
}
fn default_storyboard_width() -> u32 {
    320
}
fn default_storyboard_quality() -> u32 {
    75
}
fn default_storyboard_keyframe_only() -> bool {
    true
}
fn default_storyboard_sprite_columns() -> u32 {
    10
}
fn default_storyboard_sprite_rows() -> u32 {
    20
}

impl Default for TranscodingConfig {
    fn default() -> Self {
        Self {
            hardware_accel: "auto".to_string(),
            transcode_path: "/cache/transcodes".to_string(),
            max_concurrent_transcodes: 2,
            segment_duration_seconds: 6,
            allow_hw_tone_mapping: true,
            allow_hw_subtitle_burn_in: true,
            default_video_codec: "h264".to_string(),
            default_audio_codec: "aac".to_string(),
            max_downscale_resolution: "3840x2160".to_string(),
            enable_thumb_extraction: true,
            thread_count: None,
            thread_type: "frame".to_string(),
            prefer_hw_decode: true,
            segment_detection_enabled: true,
            segment_safety: SegmentSafetyConfig::default(),
            segment_analysis: SegmentAnalysisConfig::default(),
            storyboards_enabled: true,
            storyboard_interval_mode: default_storyboard_interval_mode(),
            storyboard_fixed_interval_seconds: default_storyboard_fixed_interval_seconds(),
            storyboard_width: default_storyboard_width(),
            storyboard_quality: default_storyboard_quality(),
            storyboard_keyframe_only: default_storyboard_keyframe_only(),
            storyboard_sprite_columns: default_storyboard_sprite_columns(),
            storyboard_sprite_rows: default_storyboard_sprite_rows(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentSafetyConfig {
    pub intro_end_padding_ms: i32,
    pub credits_end_padding_ms: i32,
    pub min_confidence: f32,
}

impl Default for SegmentSafetyConfig {
    fn default() -> Self {
        Self {
            intro_end_padding_ms: 2_000,
            credits_end_padding_ms: 0,
            min_confidence: 0.7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentAnalysisConfig {
    pub max_concurrent_analyses: u32,
    pub chromaprint_sample_rate: u32,
    pub blackframe_amount: u8,
    pub blackframe_threshold: u8,
    pub silence_noise_db: i16,
    pub silence_min_duration_ms: i32,
}

impl Default for SegmentAnalysisConfig {
    fn default() -> Self {
        Self {
            max_concurrent_analyses: 1,
            chromaprint_sample_rate: 11_025,
            blackframe_amount: 75,
            blackframe_threshold: 2,
            silence_noise_db: -55,
            silence_min_duration_ms: 2_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataConfig {
    pub artwork_language_priority: Vec<String>,
    pub artwork_auto_download: bool,
    pub artwork_download_originals_only: bool,
    pub asset_directory: Option<String>,
    pub overlays_enabled: bool,
    pub overlay_apply_schedule: String,
    pub overlay_image_format: String,
    pub overlay_image_quality: i32,
    pub overlay_max_image_size_mb: i32,
    pub overlay_default_font: String,
    pub overlay_reapply_on_artwork_change: bool,
    pub collections_enabled: bool,
    pub collection_sync_schedule: String,
    pub collection_default_poster_source: String,
    pub collection_max_items_default: i32,
    pub collection_track_missing: bool,
    pub collection_external_rate_limit_per_minute: i32,
    pub providers: ProviderConfig,
    pub auto_refresh_hours: u32,
    pub max_concurrent_probes: u32,
    pub metadata_language: String,
    pub enrichment_timeout_seconds: u32,
    pub export_cache_days: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub tmdb: TmdbProviderConfig,
    pub tvdb: OptionalProviderConfig,
    pub fanart: OptionalProviderConfig,
    pub omdb: OptionalProviderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbProviderConfig {
    pub api_key: String,
    pub access_token: String,
    pub enabled: bool,
    pub include_adult: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OptionalProviderConfig {
    pub api_key: Option<String>,
    pub enabled: bool,
}

impl Default for MetadataConfig {
    fn default() -> Self {
        Self {
            artwork_language_priority: vec!["en".to_string()],
            artwork_auto_download: true,
            artwork_download_originals_only: true,
            asset_directory: None,
            overlays_enabled: true,
            overlay_apply_schedule: "0 5 * * *".to_string(),
            overlay_image_format: "webp".to_string(),
            overlay_image_quality: 90,
            overlay_max_image_size_mb: 10,
            overlay_default_font: "Inter".to_string(),
            overlay_reapply_on_artwork_change: true,
            collections_enabled: true,
            collection_sync_schedule: "0 6 * * *".to_string(),
            collection_default_poster_source: "auto".to_string(),
            collection_max_items_default: 100,
            collection_track_missing: true,
            collection_external_rate_limit_per_minute: 30,
            providers: ProviderConfig::default(),
            auto_refresh_hours: 6,
            max_concurrent_probes: 2,
            metadata_language: "en".to_string(),
            enrichment_timeout_seconds: 30,
            export_cache_days: 7,
        }
    }
}

impl Default for TmdbProviderConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            access_token: String::new(),
            enabled: true,
            include_adult: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationConfig {
    pub webhook: WebhookDispatchConfig,
    pub push: PushDispatchConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebhookDispatchConfig {
    pub url: Option<String>,
    pub secret: Option<String>,
    pub format: String,
}

impl Default for WebhookDispatchConfig {
    fn default() -> Self {
        Self {
            url: None,
            secret: None,
            format: "generic".to_string(),
        }
    }
}

impl WebhookDispatchConfig {
    pub fn is_configured(&self) -> bool {
        self.url.as_ref().is_some_and(|u| !u.trim().is_empty())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PushDispatchConfig {
    pub enabled: bool,
    pub provider: Option<String>,
    pub fcm: FcmPushConfig,
    pub apns: ApnsPushConfig,
    pub unifiedpush: UnifiedPushConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FcmPushConfig {
    pub project_id: Option<String>,
    pub client_email: Option<String>,
    pub private_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ApnsPushConfig {
    pub team_id: Option<String>,
    pub key_id: Option<String>,
    pub private_key: Option<String>,
    pub bundle_id: Option<String>,
    pub sandbox: bool,
}

impl Default for ApnsPushConfig {
    fn default() -> Self {
        Self {
            team_id: None,
            key_id: None,
            private_key: None,
            bundle_id: None,
            sandbox: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UnifiedPushConfig {
    pub enabled: bool,
}

impl PushDispatchConfig {
    pub fn is_configured(&self) -> bool {
        if !self.enabled {
            return false;
        }
        match self.provider.as_deref() {
            Some("fcm") => {
                non_empty(self.fcm.project_id.as_deref())
                    && non_empty(self.fcm.client_email.as_deref())
                    && non_empty(self.fcm.private_key.as_deref())
            }
            Some("apns") => {
                non_empty(self.apns.team_id.as_deref())
                    && non_empty(self.apns.key_id.as_deref())
                    && non_empty(self.apns.private_key.as_deref())
                    && non_empty(self.apns.bundle_id.as_deref())
            }
            Some("unifiedpush") => self.unifiedpush.enabled,
            _ => false,
        }
    }
}

fn non_empty(value: Option<&str>) -> bool {
    value.is_some_and(|v| !v.trim().is_empty())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WalGStorageType {
    #[default]
    Local,
    S3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupConfig {
    pub wal_g_enabled: bool,
    pub wal_g_storage_type: WalGStorageType,
    pub wal_g_storage_path: String,
    pub wal_g_s3_endpoint: String,
    pub wal_g_s3_bucket: String,
    pub wal_g_s3_prefix: String,
    pub wal_g_s3_region: String,
    pub wal_g_encryption_enabled: bool,
    pub wal_g_encryption_key_id: String,
    pub wal_g_encryption_auto_s3: bool,
    pub wal_g_retention_full: u32,
    pub wal_g_retention_weekly: u32,
    pub wal_g_retention_monthly: u32,
    pub pg_dump_enabled: bool,
    pub pg_dump_storage_path: String,
    pub pg_dump_retention_daily: u32,
    pub pg_dump_retention_monthly: u32,
    pub archive_timeout_seconds: u32,
    pub data_checksums: bool,
    pub verification_enabled: bool,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            wal_g_enabled: true,
            wal_g_storage_type: WalGStorageType::Local,
            wal_g_storage_path: "/data/backups/wal-g".to_string(),
            wal_g_s3_endpoint: String::new(),
            wal_g_s3_bucket: String::new(),
            wal_g_s3_prefix: "backups".to_string(),
            wal_g_s3_region: String::new(),
            wal_g_encryption_enabled: false,
            wal_g_encryption_key_id: String::new(),
            wal_g_encryption_auto_s3: true,
            wal_g_retention_full: 7,
            wal_g_retention_weekly: 4,
            wal_g_retention_monthly: 12,
            pg_dump_enabled: true,
            pg_dump_storage_path: "/data/backups/dump".to_string(),
            pg_dump_retention_daily: 30,
            pg_dump_retention_monthly: 12,
            archive_timeout_seconds: 60,
            data_checksums: true,
            verification_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntegrationsConfig {
    pub subtitle_providers: SubtitleProviderConfig,
    pub trakt: TraktConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraktConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl Default for TraktConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            redirect_uri: "http://localhost:48027/trakt/callback".to_string(),
        }
    }
}

impl TraktConfig {
    pub fn is_configured(&self) -> bool {
        !self.client_id.is_empty() && !self.client_secret.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubtitleProviderConfig {
    pub subdl: SubdlProviderConfig,
    pub opensubtitles: OpensubtitlesProviderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubdlProviderConfig {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub auto_fetch_enabled: bool,
    pub auto_fetch_languages: Vec<String>,
    pub prefer_hearing_impaired: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpensubtitlesProviderConfig {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub api_token: Option<String>,
    pub auto_fetch_enabled: bool,
    pub auto_fetch_languages: Vec<String>,
    pub prefer_hearing_impaired: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub max_file_size_mb: u32,
    pub max_files: u32,
    pub format: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            max_file_size_mb: 10,
            max_files: 5,
            format: "json".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub disk_space_warnings: DiskSpaceWarnings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiskSpaceWarnings {
    pub data_threshold_percent: u8,
    pub cache_threshold_percent: u8,
    pub transcode_threshold_percent: u8,
    pub check_interval_seconds: u32,
    pub notify_on_warning: bool,
}

impl Default for DiskSpaceWarnings {
    fn default() -> Self {
        Self {
            data_threshold_percent: 90,
            cache_threshold_percent: 90,
            transcode_threshold_percent: 80,
            check_interval_seconds: 1800,
            notify_on_warning: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MaintenanceConfig {
    pub autovacuum_tuning_enabled: bool,
    pub reindex_enabled: bool,
    pub reindex_schedule: String,
    pub reindex_bloat_threshold_percent: u8,
    pub reindex_min_index_size_mb: u32,
    pub partition_retention_months: PartitionRetention,
    pub analyze_parent_tables_enabled: bool,
    pub analyze_parent_schedule: String,
}

impl Default for MaintenanceConfig {
    fn default() -> Self {
        Self {
            autovacuum_tuning_enabled: true,
            reindex_enabled: true,
            reindex_schedule: "0 2 * * 0".to_string(),
            reindex_bloat_threshold_percent: 30,
            reindex_min_index_size_mb: 10,
            partition_retention_months: PartitionRetention::default(),
            analyze_parent_tables_enabled: true,
            analyze_parent_schedule: "0 3 * * *".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PartitionRetention {
    pub play_sessions: u32,
    pub play_events: u32,
    pub audit_log: u32,
}

impl Default for PartitionRetention {
    fn default() -> Self {
        Self {
            play_sessions: 24,
            play_events: 12,
            audit_log: 12,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsConfig {
    pub geoip_enabled: bool,
    pub impossible_travel_enabled: bool,
    pub velocity_threshold_kmh: u32,
    pub min_distance_km: u32,
    pub lookback_hours: u32,
    pub same_country_suppress: bool,
    pub trusted_ips: Vec<String>,
    pub trusted_cidrs: Vec<String>,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            geoip_enabled: true,
            impossible_travel_enabled: true,
            velocity_threshold_kmh: 1000,
            min_distance_km: 500,
            lookback_hours: 24,
            same_country_suppress: true,
            trusted_ips: Vec::new(),
            trusted_cidrs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub analytics: AnalyticsConfig,
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
            analytics: AnalyticsConfig::default(),
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

pub struct WebauthnChallenge {
    pub registration_state: Option<PasskeyRegistration>,
    pub authentication_state: Option<PasskeyAuthentication>,
    pub user_id: Option<uuid::Uuid>,
    pub created_at: std::time::Instant,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub runtime_config: Arc<ArcSwap<RuntimeConfig>>,
    pub bootstrap: BootstrapConfig,
    pub rate_limits: Arc<RateLimitState>,
    pub metrics_handle: PrometheusHandle,
    pub metrics_allowed_subnets: Arc<Vec<IpNet>>,
    pub webauthn: Arc<Webauthn>,
    pub webauthn_challenges: Arc<DashMap<String, WebauthnChallenge>>,
    pub trakt_sync_locks: Arc<DashMap<Uuid, std::time::Instant>>,
    pub migration_runs: Arc<DashMap<Uuid, CancellationToken>>,
    pub fs_watcher: Arc<LibraryWatcherManager>,
    pub enrichment: Arc<EnrichmentOrchestrator>,
    pub encryption_key: Arc<EncryptionKey>,
    pub transcode_manager: Arc<crate::services::transcoding::TranscodeManager>,
    pub event_bus: Arc<EventBus>,
    pub geoip: Arc<GeoIpService>,
    pub scheduler: Arc<std::sync::OnceLock<Arc<Scheduler>>>,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        bootstrap: BootstrapConfig,
        metrics_handle: PrometheusHandle,
        encryption_key: EncryptionKey,
    ) -> Self {
        set_environment(bootstrap.environment.clone());
        let subnets = parse_metrics_subnets(&NetworkConfig::default().allowed_metrics_subnets);
        let webauthn = build_webauthn("localhost", "http://localhost:48027");
        let enrichment = Arc::new(EnrichmentOrchestrator::new(
            crate::services::metadata::ProviderRegistry::new(),
            pool.clone(),
            MetadataConfig::default(),
            bootstrap.data_dir.clone(),
        ));
        let fs_watcher = Arc::new(LibraryWatcherManager::new(pool.clone(), enrichment.clone()));
        let transcode_manager = Arc::new(crate::services::transcoding::TranscodeManager::new(
            Arc::new(ArcSwap::from_pointee(RuntimeConfig::default())),
        ));
        Self {
            pool,
            runtime_config: Arc::new(ArcSwap::from_pointee(RuntimeConfig::default())),
            bootstrap,
            rate_limits: Arc::new(RateLimitState::from_defaults()),
            metrics_handle,
            metrics_allowed_subnets: Arc::new(subnets),
            webauthn: Arc::new(webauthn),
            webauthn_challenges: Arc::new(DashMap::new()),
            trakt_sync_locks: Arc::new(DashMap::new()),
            migration_runs: Arc::new(DashMap::new()),
            fs_watcher,
            enrichment,
            encryption_key: Arc::new(encryption_key),
            transcode_manager,
            event_bus: Arc::new(EventBus::with_default_limit()),
            geoip: Arc::new(GeoIpService::disabled()),
            scheduler: Arc::new(std::sync::OnceLock::new()),
        }
    }

    pub fn new_with_config(
        pool: PgPool,
        bootstrap: BootstrapConfig,
        runtime_config: RuntimeConfig,
        metrics_handle: PrometheusHandle,
        encryption_key: EncryptionKey,
    ) -> Self {
        set_environment(bootstrap.environment.clone());
        let rate_limits = RateLimitState::new(&runtime_config.auth.rate_limits);
        let subnets = parse_metrics_subnets(&runtime_config.network.allowed_metrics_subnets);

        let rp_id = runtime_config.auth.rp_id.as_deref().unwrap_or("localhost");
        let rp_origin = runtime_config
            .auth
            .rp_origin
            .as_deref()
            .unwrap_or("http://localhost:48027");
        let webauthn = build_webauthn(rp_id, rp_origin);

        let metadata_config = runtime_config.metadata.clone();
        let registry = crate::services::metadata::ProviderRegistry::from_config(&metadata_config);
        let enrichment = Arc::new(EnrichmentOrchestrator::new(
            registry,
            pool.clone(),
            metadata_config,
            bootstrap.data_dir.clone(),
        ));

        let fs_watcher = Arc::new(LibraryWatcherManager::new(pool.clone(), enrichment.clone()));

        let config_arc = Arc::new(ArcSwap::from_pointee(runtime_config));
        let transcode_manager = Arc::new(crate::services::transcoding::TranscodeManager::new(
            config_arc.clone(),
        ));

        let geoip = Arc::new(GeoIpService::new(&bootstrap.data_dir));

        Self {
            pool,
            runtime_config: config_arc,
            bootstrap,
            rate_limits: Arc::new(rate_limits),
            metrics_handle,
            metrics_allowed_subnets: Arc::new(subnets),
            webauthn: Arc::new(webauthn),
            webauthn_challenges: Arc::new(DashMap::new()),
            trakt_sync_locks: Arc::new(DashMap::new()),
            migration_runs: Arc::new(DashMap::new()),
            fs_watcher,
            enrichment,
            encryption_key: Arc::new(encryption_key),
            transcode_manager,
            event_bus: Arc::new(EventBus::with_default_limit()),
            geoip,
            scheduler: Arc::new(std::sync::OnceLock::new()),
        }
    }

    pub fn reload_runtime_config(&self, new_config: RuntimeConfig) {
        self.runtime_config.store(Arc::new(new_config));
        tracing::info!("Runtime configuration reloaded");
    }

    pub fn set_scheduler(&self, scheduler: Arc<Scheduler>) {
        if self.scheduler.set(scheduler).is_err() {
            tracing::warn!("Scheduled task runner already registered in AppState");
        }
    }

    pub fn scheduler(&self) -> Option<Arc<Scheduler>> {
        self.scheduler.get().cloned()
    }
}

fn build_webauthn(rp_id: &str, rp_origin: &str) -> Webauthn {
    let origin = url::Url::parse(rp_origin).unwrap_or_else(|_| {
        tracing::warn!(
            "Invalid WebAuthn RP origin '{}', falling back to http://localhost:48027",
            rp_origin
        );
        url::Url::parse("http://localhost:48027").unwrap()
    });

    webauthn_rs::WebauthnBuilder::new(rp_id, &origin)
        .and_then(|b| {
            b.timeout(std::time::Duration::from_secs(300)).build()
        })
        .unwrap_or_else(|e| {
            tracing::error!("Failed to build Webauthn with rp_id='{}', rp_origin='{}': {}. Falling back to localhost.", rp_id, rp_origin, e);
            webauthn_rs::WebauthnBuilder::new("localhost", &url::Url::parse("http://localhost:48027").unwrap())
                .unwrap()
                .build()
                .unwrap()
        })
}

fn parse_metrics_subnets(subnets: &[String]) -> Vec<IpNet> {
    subnets
        .iter()
        .filter_map(|s| {
            IpNet::from_str(s)
                .map_err(|e| {
                    tracing::warn!(subnet = %s, error = %e, "Invalid metrics subnet, skipping");
                    e
                })
                .ok()
        })
        .collect()
}

pub async fn load_runtime_config(
    pool: &PgPool,
    encryption_key: Option<&EncryptionKey>,
) -> Result<RuntimeConfig, sqlx::Error> {
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
            subtitles,
            analytics
        FROM server_config
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(RuntimeConfig::default());
    };

    let server_name: String = row
        .try_get("server_name")
        .unwrap_or_else(|_| "My Duskcue".to_string());
    let base_url: Option<String> = row.try_get("base_url").unwrap_or(None);
    let http_port: i32 = row.try_get("http_port").unwrap_or(48027);
    let https_port: Option<i32> = row.try_get("https_port").unwrap_or(None);
    let ssl_certificate_path: Option<String> = row.try_get("ssl_certificate_path").unwrap_or(None);
    let ssl_private_key_path: Option<String> = row.try_get("ssl_private_key_path").unwrap_or(None);

    let network: serde_json::Value = row
        .try_get("network")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let transcoding: serde_json::Value = row
        .try_get("transcoding")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let metadata: serde_json::Value = row
        .try_get("metadata")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let auth: serde_json::Value = row
        .try_get("auth")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let security: serde_json::Value = row
        .try_get("security")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let notifications: serde_json::Value = row
        .try_get("notifications")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let backup: serde_json::Value = row
        .try_get("backup")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let integrations: serde_json::Value = row
        .try_get("integrations")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let logging: serde_json::Value = row
        .try_get("logging")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let storage: serde_json::Value = row
        .try_get("storage")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let maintenance: serde_json::Value = row
        .try_get("maintenance")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let resource_limits: serde_json::Value = row
        .try_get("resource_limits")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let cpu: serde_json::Value = row
        .try_get("cpu")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let quality: serde_json::Value = row
        .try_get("quality")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let subtitles: serde_json::Value = row
        .try_get("subtitles")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let analytics: serde_json::Value = row
        .try_get("analytics")
        .unwrap_or(serde_json::Value::Object(Default::default()));

    Ok(RuntimeConfig {
        server_name,
        base_url,
        http_port: http_port as u16,
        https_port: https_port.map(|p| p as u16),
        ssl_certificate_path: ssl_certificate_path.map(PathBuf::from),
        ssl_private_key_path: ssl_private_key_path.map(PathBuf::from),
        network: serde_json::from_value(network).unwrap_or_default(),
        transcoding: serde_json::from_value(transcoding).unwrap_or_default(),
        metadata: {
            let mut mc: MetadataConfig = serde_json::from_value(metadata).unwrap_or_default();
            if let Some(key) = encryption_key {
                crate::services::encryption::decrypt_provider_config(&mut mc.providers, key);
            }
            mc
        },
        auth: serde_json::from_value(auth).unwrap_or_default(),
        security: serde_json::from_value(security).unwrap_or_default(),
        notifications: {
            let mut nc: NotificationConfig =
                serde_json::from_value(notifications).unwrap_or_default();
            if let Some(key) = encryption_key {
                crate::services::encryption::decrypt_notification_config(&mut nc, key);
            }
            nc
        },
        backup: serde_json::from_value(backup).unwrap_or_default(),
        integrations: {
            let mut ic: IntegrationsConfig =
                serde_json::from_value(integrations).unwrap_or_default();
            if let Some(key) = encryption_key {
                crate::services::encryption::decrypt_trakt_config(&mut ic.trakt, key);
                crate::services::encryption::decrypt_subtitle_provider_config(
                    &mut ic.subtitle_providers,
                    key,
                );
            }
            ic
        },
        logging: serde_json::from_value(logging).unwrap_or_default(),
        storage: serde_json::from_value(storage).unwrap_or_default(),
        maintenance: serde_json::from_value(maintenance).unwrap_or_default(),
        resource_limits: serde_json::from_value(resource_limits).unwrap_or_default(),
        cpu: serde_json::from_value(cpu).unwrap_or_default(),
        quality: serde_json::from_value(quality).unwrap_or_default(),
        subtitles: serde_json::from_value(subtitles).unwrap_or_default(),
        analytics: serde_json::from_value(analytics).unwrap_or_default(),
    })
}
