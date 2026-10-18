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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use tokio::sync::Semaphore;
use tokio_process_tools::GracefulShutdown;
use uuid::Uuid;

use crate::domains::playback::error::PlaybackError;
use crate::services::hw_accel::{self, HwAccelDetectionResult};
use crate::state::RuntimeConfig;

pub mod arguments;
mod encoding;
pub mod failure;
mod launcher;
mod lifecycle;
pub mod output_profile;
pub mod plan;
pub mod progress;
mod readiness;
pub mod recovery;
mod session;
mod start;
mod watchdog;
pub use failure::FailureClass;
use lifecycle::{TranscodeWorkers, WorkerCancellation};
pub use output_profile::OutputProfile;
use plan::{StreamAction, StreamPlan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HwAccelMethod {
    Nvenc,
    Qsv,
    Vaapi,
    VideoToolbox,
    Amf,
    Software,
}

impl HwAccelMethod {
    pub fn ffmpeg_encoder(&self, codec: &str) -> String {
        match (self, codec) {
            (HwAccelMethod::Nvenc, "h264") => "h264_nvenc".to_string(),
            (HwAccelMethod::Nvenc, "hevc") | (HwAccelMethod::Nvenc, "h265") => {
                "hevc_nvenc".to_string()
            }
            (HwAccelMethod::Qsv, "h264") => "h264_qsv".to_string(),
            (HwAccelMethod::Qsv, "hevc") | (HwAccelMethod::Qsv, "h265") => "hevc_qsv".to_string(),
            (HwAccelMethod::Vaapi, "h264") => "h264_vaapi".to_string(),
            (HwAccelMethod::Vaapi, "hevc") | (HwAccelMethod::Vaapi, "h265") => {
                "hevc_vaapi".to_string()
            }
            (HwAccelMethod::VideoToolbox, "h264") => "h264_videotoolbox".to_string(),
            (HwAccelMethod::VideoToolbox, "hevc") | (HwAccelMethod::VideoToolbox, "h265") => {
                "hevc_videotoolbox".to_string()
            }
            (HwAccelMethod::Amf, "h264") => "h264_amf".to_string(),
            (HwAccelMethod::Amf, "hevc") | (HwAccelMethod::Amf, "h265") => "hevc_amf".to_string(),
            (_, "hevc") | (_, "h265") => "libx265".to_string(),
            _ => "libx264".to_string(),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            HwAccelMethod::Nvenc => "nvenc",
            HwAccelMethod::Qsv => "qsv",
            HwAccelMethod::Vaapi => "vaapi",
            HwAccelMethod::VideoToolbox => "videotoolbox",
            HwAccelMethod::Amf => "amf",
            HwAccelMethod::Software => "software",
        }
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub frame: u64,
    pub fps: f64,
    pub bitrate_kbps: f64,
    pub total_size: u64,
    pub out_time_ms: u64,
    pub speed: f64,
    pub is_complete: bool,
}

#[derive(Debug, Clone)]
pub struct TranscodeRendition {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub video_bitrate: u32,
    pub audio_bitrate: u32,
    pub audio_channels: u32,
}

impl TranscodeRendition {
    pub fn default_ladder() -> Vec<Self> {
        vec![
            Self {
                name: "480p".to_string(),
                width: 854,
                height: 480,
                video_bitrate: 1_500_000,
                audio_bitrate: 128_000,
                audio_channels: 2,
            },
            Self {
                name: "720p".to_string(),
                width: 1280,
                height: 720,
                video_bitrate: 3_000_000,
                audio_bitrate: 160_000,
                audio_channels: 2,
            },
            Self {
                name: "1080p".to_string(),
                width: 1920,
                height: 1080,
                video_bitrate: 6_000_000,
                audio_bitrate: 256_000,
                audio_channels: 6,
            },
            Self {
                name: "1080p-hq".to_string(),
                width: 1920,
                height: 1080,
                video_bitrate: 10_000_000,
                audio_bitrate: 320_000,
                audio_channels: 6,
            },
        ]
    }

    pub fn smart_ladder(
        source_width: u32,
        source_height: u32,
        max_bitrate: Option<u64>,
        max_resolution: Option<(u32, u32)>,
    ) -> Vec<Self> {
        let mut rungs: Vec<Self> = Self::default_ladder()
            .into_iter()
            .filter(|r| {
                r.width <= source_width
                    && r.height <= source_height
                    && max_bitrate.is_none_or(|mb| r.video_bitrate as u64 <= mb)
                    && max_resolution.is_none_or(|(mw, mh)| r.width <= mw && r.height <= mh)
            })
            .collect();

        if rungs.is_empty() {
            let (max_width, max_height) = max_resolution.unwrap_or((source_width, source_height));
            rungs.push(Self {
                name: "source".to_string(),
                width: source_width.min(max_width).max(2) / 2 * 2,
                height: source_height.min(max_height).max(2) / 2 * 2,
                video_bitrate: max_bitrate.unwrap_or(1_500_000).min(1_500_000) as u32,
                audio_bitrate: 128_000,
                audio_channels: 2,
            });
        }

        rungs
    }
}

#[derive(Clone)]
pub struct StartSessionParams {
    pub media_file_id: Uuid,
    pub user_id: Uuid,
    pub source_path: PathBuf,
    pub source_video_codec: String,
    pub source_video_resolution: (u32, u32),
    pub source_audio_codec: String,
    pub source_video_stream_index: Option<i32>,
    pub source_audio_stream_index: Option<i32>,
    pub stream_plan: StreamPlan,
    pub output_profile: Option<OutputProfile>,
    pub target_audio_channels: Option<u32>,
    pub target_audio_bitrate: Option<u32>,
    pub subtitle_stream_index: Option<i32>,
    pub subtitle_stream_ordinal: Option<usize>,
    pub subtitle_burn_in: bool,
    pub target_video_codec: Option<String>,
    pub target_audio_codec: Option<String>,
    pub target_resolution: Option<(u32, u32)>,
    pub target_bitrate: Option<u32>,
    pub seek_position_ms: Option<i64>,
    pub hw_accel_override: Option<HwAccelMethod>,
    pub recovery_attempts: u8,
}

#[derive(Debug, Clone)]
pub struct TranscodeSession {
    pub id: Uuid,
    pub media_file_id: Uuid,
    pub user_id: Uuid,
    pub started_at: DateTime<Utc>,

    pub source_path: PathBuf,
    pub source_video_codec: String,
    pub source_video_resolution: (u32, u32),
    pub source_audio_codec: String,
    pub source_video_stream_index: Option<i32>,
    pub source_audio_stream_index: Option<i32>,
    pub stream_plan: StreamPlan,
    pub output_profile: Option<OutputProfile>,
    pub target_audio_channels: Option<u32>,
    pub target_audio_bitrate: Option<u32>,
    pub subtitle_stream_index: Option<i32>,
    pub subtitle_stream_ordinal: Option<usize>,
    pub subtitle_burn_in: bool,

    pub target_video_codec: String,
    pub target_video_resolution: (u32, u32),
    pub target_audio_codec: String,
    pub target_bitrate: u32,

    pub hw_accel: HwAccelMethod,
    pub source_start_ms: i64,
    pub recovery_attempts: u8,
    pub rendition_name: String,

    pub segment_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub segments_written: u32,
    pub client_position_segment: u32,

    pub progress: Option<ProgressUpdate>,
    pub is_complete: bool,
    pub is_seeking: bool,
}

impl TranscodeSession {
    pub fn progress_percent(&self, duration_ms: Option<i64>) -> Option<f32> {
        if self.is_complete {
            return Some(100.0);
        }
        let remaining = duration_ms?
            .checked_sub(self.source_start_ms)
            .filter(|duration| *duration > 0)?;
        self.progress.as_ref().map(|progress| {
            (progress.out_time_ms as f64 / remaining as f64 * 100.0).min(100.0) as f32
        })
    }

    pub fn manifest_url(&self) -> String {
        format!("/api/v1/transcode/{}/manifest.m3u8", self.id)
    }

    pub fn segment_url_pattern(&self) -> String {
        format!(
            "/api/v1/transcode/{}/{}/seg_%04d.m4s",
            self.id, self.rendition_name
        )
    }
}

pub(crate) struct PendingTranscodeSession {
    cancellation: Option<WorkerCancellation>,
}

impl PendingTranscodeSession {
    pub(crate) fn acknowledge(mut self) {
        if let Some(cancellation) = self.cancellation.as_mut() {
            cancellation.acknowledge();
        }
    }
}

pub struct TranscodeManager {
    sessions: Arc<DashMap<Uuid, TranscodeSession>>,
    semaphore: Arc<Semaphore>,
    workers: TranscodeWorkers,
    config: Arc<ArcSwap<RuntimeConfig>>,
    hw_detection: Arc<std::sync::RwLock<HwAccelDetectionResult>>,
}

impl TranscodeManager {
    pub fn new(config: Arc<ArcSwap<RuntimeConfig>>) -> Self {
        let max_concurrent = config.load().resource_limits.max_concurrent_transcodes as usize;

        let runtime = config.load();
        let detection = hw_accel::detect_hw_accel_runtime(&runtime.transcoding, &runtime.cpu);

        Self {
            sessions: Arc::new(DashMap::new()),
            semaphore: Arc::new(Semaphore::new(max_concurrent.max(1))),
            workers: TranscodeWorkers::default(),
            config,
            hw_detection: Arc::new(std::sync::RwLock::new(detection)),
        }
    }

    pub async fn stop_session(&self, session_id: Uuid) -> Result<(), PlaybackError> {
        if self.workers.stop(session_id).await? {
            return Ok(());
        }
        if let Some((_, session)) = self.sessions.remove(&session_id) {
            match tokio::fs::remove_dir_all(&session.segment_dir).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {
                    self.sessions.insert(session_id, session);
                    return Err(PlaybackError::FfmpegFailed(
                        "transcode cache cleanup failed".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn pending_session(&self, session_id: Uuid) -> PendingTranscodeSession {
        PendingTranscodeSession {
            cancellation: self.workers.pending(session_id),
        }
    }

    pub async fn seek_session(
        &self,
        session_id: Uuid,
        position_ms: i64,
        data_dir: &Path,
    ) -> Result<TranscodeSession, PlaybackError> {
        let old_session = self
            .get_session(&session_id)
            .ok_or(PlaybackError::SessionNotFound)?;
        self.stop_session(session_id).await?;

        self.start_session(old_session.restart_params(position_ms), data_dir)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn start_remux_session(
        &self,
        media_file_id: Uuid,
        user_id: Uuid,
        source_path: PathBuf,
        source_video_codec: String,
        source_video_resolution: (u32, u32),
        source_audio_codec: String,
        source_audio_stream_index: Option<i32>,
        data_dir: &Path,
    ) -> Result<TranscodeSession, PlaybackError> {
        self.start_session(
            StartSessionParams {
                media_file_id,
                user_id,
                source_path,
                source_video_codec,
                source_video_resolution,
                source_audio_codec,
                source_video_stream_index: None,
                source_audio_stream_index,
                stream_plan: StreamPlan {
                    video: StreamAction::Copy,
                    audio: StreamAction::Copy,
                },
                output_profile: None,
                target_audio_channels: None,
                target_audio_bitrate: None,
                subtitle_stream_index: None,
                subtitle_stream_ordinal: None,
                subtitle_burn_in: false,
                target_video_codec: None,
                target_audio_codec: None,
                target_resolution: None,
                target_bitrate: None,
                seek_position_ms: None,
                hw_accel_override: None,
                recovery_attempts: 0,
            },
            data_dir,
        )
        .await
    }

    pub fn get_session(&self, session_id: &Uuid) -> Option<TranscodeSession> {
        self.sessions.get(session_id).map(|r| {
            let mut session = r.value().clone();
            session.is_complete =
                self.workers
                    .observation(*session_id)
                    .is_some_and(|observation| {
                        observation.snapshot().state == lifecycle::ExecutionState::Succeeded
                    });
            session
        })
    }

    #[cfg(test)]
    pub(crate) fn insert_test_session(&self, session: TranscodeSession) {
        self.sessions.insert(session.id, session);
    }

    pub fn active_session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn get_hw_accel(&self) -> HwAccelMethod {
        self.hw_detection
            .read()
            .map(|g| g.method)
            .unwrap_or(HwAccelMethod::Software)
    }

    pub fn get_hw_detection(&self) -> HwAccelDetectionResult {
        self.hw_detection
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| HwAccelDetectionResult {
                method: HwAccelMethod::Software,
                nvidia_detected: false,
                vaapi_available: false,
                qsv_available: false,
                amf_available: false,
                videotoolbox_available: false,
                verified_encoders: vec![],
                source: "error".to_string(),
            })
    }

    pub fn redetect_hw_accel(&self) {
        let config = self.config.load();
        let result = hw_accel::detect_hw_accel_runtime(&config.transcoding, &config.cpu);
        if let Ok(mut guard) = self.hw_detection.write() {
            *guard = result;
        }
    }

    pub fn list_active_sessions(&self) -> Vec<TranscodeSession> {
        self.sessions.iter().map(|r| r.value().clone()).collect()
    }

    pub async fn cleanup_orphaned_sessions(&self, data_dir: &Path) {
        let config = self.config.load();
        let transcode_base =
            data_dir.join(config.transcoding.transcode_path.trim_start_matches('/'));

        if !transcode_base.exists() {
            return;
        }

        let mut entries = match tokio::fs::read_dir(&transcode_base).await {
            Ok(e) => e,
            Err(_) => return,
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Ok(dir_uuid) = Uuid::parse_str(&name_str)
                && !self.sessions.contains_key(&dir_uuid)
            {
                let _ = tokio::fs::remove_dir_all(entry.path()).await;
            }
        }
    }
}

fn build_graceful_shutdown(grace_secs: u64) -> GracefulShutdown {
    let timeout = Duration::from_secs(grace_secs);
    GracefulShutdown::builder()
        .unix_sigterm(timeout)
        .windows_ctrl_break(timeout)
        .build()
}

type OutputStream = tokio_process_tools::SingleSubscriberOutputStream<
    tokio_process_tools::LossyWithoutBackpressure,
    tokio_process_tools::ReplayEnabled,
>;
type FfmpegHandle = tokio_process_tools::ProcessHandle<OutputStream>;
