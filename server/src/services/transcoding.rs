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
use tokio_process_tools::{GracefulShutdown, Next};
use uuid::Uuid;

use crate::domains::playback::error::PlaybackError;
use crate::services::hw_accel::{self, HwAccelDetectionResult};
use crate::state::{CpuConfig, RuntimeConfig};

pub mod arguments;
mod launcher;
mod lifecycle;
use arguments::{
    build_ffmpeg_input_args, build_hls_output_args, build_stream_mapping_args,
    build_subtitle_burn_in_filter, compose_video_filters,
};
use launcher::{LaunchReadiness, spawn_session_ffmpeg};
use lifecycle::{TranscodeWorkers, WorkerCancellation, WorkerLaunch};

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
            rungs.push(Self {
                name: "480p".to_string(),
                width: 854,
                height: 480,
                video_bitrate: 1_500_000,
                audio_bitrate: 128_000,
                audio_channels: 2,
            });
        }

        rungs
    }
}

pub struct StartSessionParams {
    pub media_file_id: Uuid,
    pub user_id: Uuid,
    pub source_path: PathBuf,
    pub source_video_codec: String,
    pub source_video_resolution: (u32, u32),
    pub source_audio_codec: String,
    pub source_audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub subtitle_stream_ordinal: Option<usize>,
    pub subtitle_burn_in: bool,
    pub target_video_codec: Option<String>,
    pub target_audio_codec: Option<String>,
    pub target_resolution: Option<(u32, u32)>,
    pub target_bitrate: Option<u32>,
    pub seek_position_ms: Option<i64>,
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
    pub source_audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub subtitle_stream_ordinal: Option<usize>,
    pub subtitle_burn_in: bool,

    pub target_video_codec: String,
    pub target_video_resolution: (u32, u32),
    pub target_audio_codec: String,
    pub target_bitrate: u32,

    pub hw_accel: HwAccelMethod,
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
    pub fn progress_percent(&self) -> Option<f32> {
        self.progress.as_ref().map(|p| {
            if p.is_complete {
                100.0
            } else {
                (p.out_time_ms as f64 / 3600000.0 * 100.0).min(100.0) as f32
            }
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

    pub async fn start_session(
        &self,
        params: StartSessionParams,
        data_dir: &Path,
    ) -> Result<TranscodeSession, PlaybackError> {
        let StartSessionParams {
            media_file_id,
            user_id,
            source_path,
            source_video_codec,
            source_video_resolution,
            source_audio_codec,
            source_audio_stream_index,
            subtitle_stream_index,
            subtitle_stream_ordinal,
            subtitle_burn_in,
            target_video_codec,
            target_audio_codec,
            target_resolution,
            target_bitrate,
            seek_position_ms,
        } = params;

        let permit = Arc::clone(&self.semaphore)
            .try_acquire_owned()
            .map_err(|_| PlaybackError::TranscodeCapacityReached)?;

        let config = self.config.load_full();
        let transcoding = &config.transcoding;
        let cpu = &config.cpu;

        let hw_accel = self.get_hw_accel();
        let effective_video_codec =
            target_video_codec.unwrap_or_else(|| transcoding.default_video_codec.clone());
        let effective_audio_codec =
            target_audio_codec.unwrap_or_else(|| transcoding.default_audio_codec.clone());

        let encoder = hw_accel.ffmpeg_encoder(&effective_video_codec);

        let (target_w, target_h) = target_resolution.unwrap_or(source_video_resolution);

        let rendition = TranscodeRendition::smart_ladder(
            source_video_resolution.0,
            source_video_resolution.1,
            target_bitrate.map(|b| b as u64),
            Some((target_w, target_h)),
        )
        .into_iter()
        .last()
        .unwrap_or(TranscodeRendition {
            name: "auto".to_string(),
            width: target_w,
            height: target_h,
            video_bitrate: target_bitrate.unwrap_or(6_000_000),
            audio_bitrate: 192_000,
            audio_channels: 2,
        });

        let session_id = Uuid::now_v7();
        let segment_dir = data_dir
            .join(transcoding.transcode_path.trim_start_matches('/'))
            .join(session_id.to_string());
        let manifest_path = segment_dir.join("manifest.m3u8");

        tokio::fs::create_dir_all(&segment_dir).await.map_err(|e| {
            PlaybackError::FfmpegFailed(format!("failed to create segment dir: {e}"))
        })?;

        let segment_filename = segment_dir.join("seg_%04d.m4s");

        let fps = 24u32;
        let gop_size = fps * transcoding.segment_duration_seconds;

        let mut args = build_ffmpeg_input_args(seek_position_ms, &source_path);

        let subtitle_filter = if subtitle_burn_in {
            let ordinal = subtitle_stream_ordinal.ok_or_else(|| {
                PlaybackError::FfmpegFailed(
                    "subtitle burn-in requires a selected subtitle stream".into(),
                )
            })?;
            Some(build_subtitle_burn_in_filter(&source_path, ordinal))
        } else {
            None
        };
        let video_args = build_video_encode_args(
            &encoder,
            &effective_video_codec,
            rendition.width,
            rendition.height,
            rendition.video_bitrate,
            gop_size,
            hw_accel,
        );
        let (video_args, subtitle_filter) = compose_video_filters(video_args, subtitle_filter)
            .map_err(|message| PlaybackError::FfmpegFailed(message.into()))?;
        args.extend(build_stream_mapping_args(
            source_audio_stream_index,
            subtitle_filter,
        ));
        args.extend(video_args);

        args.extend(build_audio_encode_args(
            &effective_audio_codec,
            rendition.audio_channels,
            rendition.audio_bitrate,
        ));

        args.extend(build_threading_args(cpu));

        args.extend(["-progress".to_string(), "pipe:1".to_string()]);

        args.extend(build_hls_output_args(
            transcoding.segment_duration_seconds,
            &segment_filename.to_string_lossy(),
            &manifest_path.to_string_lossy(),
        ));

        let spawned = spawn_session_ffmpeg(&args, session_id, &source_path, &segment_dir).await?;

        let session = TranscodeSession {
            id: session_id,
            media_file_id,
            user_id,
            started_at: Utc::now(),
            source_path,
            source_video_codec,
            source_video_resolution,
            source_audio_codec,
            source_audio_stream_index,
            subtitle_stream_index,
            subtitle_stream_ordinal,
            subtitle_burn_in,
            target_video_codec: effective_video_codec,
            target_video_resolution: (rendition.width, rendition.height),
            target_audio_codec: effective_audio_codec,
            target_bitrate: rendition.video_bitrate,
            hw_accel,
            rendition_name: rendition.name,
            segment_dir,
            manifest_path,
            segments_written: 0,
            client_position_segment: 0,
            progress: None,
            is_complete: false,
            is_seeking: false,
        };

        let sessions = Arc::clone(&self.sessions);
        let cleanup_sessions = Arc::clone(&self.sessions);
        self.workers.start(
            WorkerLaunch {
                session_id,
                handle: spawned.handle,
                permit,
                directory: session.segment_dir.clone(),
                shutdown: build_graceful_shutdown(
                    config.resource_limits.ffmpeg_shutdown_grace_secs,
                ),
            },
            move |line| {
                if let Some(update) = parse_progress_line(&line)
                    && let Some(mut session) = sessions.get_mut(&session_id)
                {
                    if update.is_complete {
                        session.is_complete = true;
                    }
                    session.progress = Some(update);
                }
                Next::Continue
            },
            move || {
                cleanup_sessions.remove(&session_id);
            },
        );

        self.sessions.insert(session_id, session.clone());
        self.confirm_bootstrap(session_id, spawned.readiness)
            .await?;

        Ok(session)
    }

    async fn confirm_bootstrap(
        &self,
        session_id: Uuid,
        readiness: LaunchReadiness,
    ) -> Result<(), PlaybackError> {
        let pending = self.pending_session(session_id);
        if let Err(error) = readiness.wait().await {
            self.stop_session(session_id).await?;
            return Err(error);
        }
        pending.acknowledge();
        Ok(())
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

        self.start_session(
            StartSessionParams {
                media_file_id: old_session.media_file_id,
                user_id: old_session.user_id,
                source_path: old_session.source_path,
                source_video_codec: old_session.source_video_codec,
                source_video_resolution: old_session.source_video_resolution,
                source_audio_codec: old_session.source_audio_codec,
                source_audio_stream_index: old_session.source_audio_stream_index,
                subtitle_stream_index: old_session.subtitle_stream_index,
                subtitle_stream_ordinal: old_session.subtitle_stream_ordinal,
                subtitle_burn_in: old_session.subtitle_burn_in,
                target_video_codec: Some(old_session.target_video_codec),
                target_audio_codec: Some(old_session.target_audio_codec),
                target_resolution: Some(old_session.target_video_resolution),
                target_bitrate: Some(old_session.target_bitrate),
                seek_position_ms: Some(position_ms),
            },
            data_dir,
        )
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
        let permit = Arc::clone(&self.semaphore)
            .try_acquire_owned()
            .map_err(|_| PlaybackError::TranscodeCapacityReached)?;

        let config = self.config.load_full();
        let transcoding = &config.transcoding;

        let session_id = Uuid::now_v7();
        let segment_dir = data_dir
            .join(transcoding.transcode_path.trim_start_matches('/'))
            .join(session_id.to_string());
        let manifest_path = segment_dir.join("manifest.m3u8");

        tokio::fs::create_dir_all(&segment_dir).await.map_err(|e| {
            PlaybackError::FfmpegFailed(format!("failed to create segment dir: {e}"))
        })?;

        let segment_filename = segment_dir.join("seg_%04d.m4s");

        let mut args = build_ffmpeg_input_args(None, &source_path);

        args.extend(build_stream_mapping_args(source_audio_stream_index, None));
        args.extend([
            "-c:v".to_string(),
            "copy".to_string(),
            "-c:a".to_string(),
            "copy".to_string(),
        ]);

        args.extend(["-progress".to_string(), "pipe:1".to_string()]);

        args.extend(build_hls_output_args(
            transcoding.segment_duration_seconds,
            &segment_filename.to_string_lossy(),
            &manifest_path.to_string_lossy(),
        ));

        let spawned = spawn_session_ffmpeg(&args, session_id, &source_path, &segment_dir).await?;

        let hw_accel = self.get_hw_accel();

        let session = TranscodeSession {
            id: session_id,
            media_file_id,
            user_id,
            started_at: Utc::now(),
            source_path,
            source_video_codec,
            source_video_resolution,
            source_audio_codec,
            source_audio_stream_index,
            subtitle_stream_index: None,
            subtitle_stream_ordinal: None,
            subtitle_burn_in: false,
            target_video_codec: "copy".to_string(),
            target_video_resolution: source_video_resolution,
            target_audio_codec: "copy".to_string(),
            target_bitrate: 0,
            hw_accel,
            rendition_name: "remux".to_string(),
            segment_dir,
            manifest_path,
            segments_written: 0,
            client_position_segment: 0,
            progress: None,
            is_complete: false,
            is_seeking: false,
        };

        let sessions = Arc::clone(&self.sessions);
        let cleanup_sessions = Arc::clone(&self.sessions);
        self.workers.start(
            WorkerLaunch {
                session_id,
                handle: spawned.handle,
                permit,
                directory: session.segment_dir.clone(),
                shutdown: build_graceful_shutdown(
                    config.resource_limits.ffmpeg_shutdown_grace_secs,
                ),
            },
            move |line| {
                if let Some(update) = parse_progress_line(&line)
                    && let Some(mut session) = sessions.get_mut(&session_id)
                {
                    if update.is_complete {
                        session.is_complete = true;
                    }
                    session.progress = Some(update);
                }
                Next::Continue
            },
            move || {
                cleanup_sessions.remove(&session_id);
            },
        );

        self.sessions.insert(session_id, session.clone());
        self.confirm_bootstrap(session_id, spawned.readiness)
            .await?;

        Ok(session)
    }

    pub fn get_session(&self, session_id: &Uuid) -> Option<TranscodeSession> {
        self.sessions.get(session_id).map(|r| r.value().clone())
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

fn build_video_encode_args(
    encoder: &str,
    _codec: &str,
    width: u32,
    height: u32,
    maxrate: u32,
    gop_size: u32,
    _hw_accel: HwAccelMethod,
) -> Vec<String> {
    let is_hw = !encoder.contains("libx2");

    let mut args = vec!["-c:v:0".to_string(), encoder.to_string()];

    if !is_hw {
        args.extend([
            "-preset".to_string(),
            "veryfast".to_string(),
            "-crf".to_string(),
            "23".to_string(),
        ]);
    }

    args.extend([
        "-maxrate".to_string(),
        maxrate.to_string(),
        "-bufsize".to_string(),
        (maxrate * 2).to_string(),
        "-vf".to_string(),
        format!(
            "scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2"
        ),
    ]);

    args.extend([
        "-g".to_string(),
        gop_size.to_string(),
        "-keyint_min".to_string(),
        gop_size.to_string(),
        "-sc_threshold".to_string(),
        "0".to_string(),
    ]);

    if !is_hw {
        args.extend([
            "-profile:v".to_string(),
            "high".to_string(),
            "-level".to_string(),
            "4.1".to_string(),
            "-pix_fmt".to_string(),
            "yuv420p".to_string(),
        ]);
    }

    args
}

fn build_audio_encode_args(audio_codec: &str, channels: u32, bitrate: u32) -> Vec<String> {
    let encoder = match audio_codec {
        "eac3" | "e-ac-3" => "eac3",
        "opus" => "libopus",
        _ => "aac",
    };

    vec![
        "-c:a:0".to_string(),
        encoder.to_string(),
        "-ac".to_string(),
        channels.to_string(),
        "-b:a".to_string(),
        bitrate.to_string(),
    ]
}

fn build_threading_args(cpu: &CpuConfig) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(threads) = cpu.ffmpeg_threads {
        args.extend(["-threads".to_string(), threads.to_string()]);
    }

    args.extend(["-thread_type".to_string(), cpu.ffmpeg_thread_type.clone()]);

    args
}

type OutputStream = tokio_process_tools::SingleSubscriberOutputStream<
    tokio_process_tools::LossyWithoutBackpressure,
    tokio_process_tools::ReplayEnabled,
>;
type FfmpegHandle = tokio_process_tools::ProcessHandle<OutputStream>;

fn parse_progress_line(line: &str) -> Option<ProgressUpdate> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let (key, value) = line.split_once('=')?;

    match key {
        "frame" => Some(ProgressUpdate {
            frame: value.parse().ok()?,
            fps: 0.0,
            bitrate_kbps: 0.0,
            total_size: 0,
            out_time_ms: 0,
            speed: 0.0,
            is_complete: false,
        }),
        "fps" => value.parse::<f64>().ok().map(|f| ProgressUpdate {
            frame: 0,
            fps: f,
            bitrate_kbps: 0.0,
            total_size: 0,
            out_time_ms: 0,
            speed: 0.0,
            is_complete: false,
        }),
        "bitrate" => {
            let kbps = value
                .trim_end_matches("kbits/s")
                .trim()
                .parse::<f64>()
                .unwrap_or(0.0);
            Some(ProgressUpdate {
                frame: 0,
                fps: 0.0,
                bitrate_kbps: kbps,
                total_size: 0,
                out_time_ms: 0,
                speed: 0.0,
                is_complete: false,
            })
        }
        "total_size" => Some(ProgressUpdate {
            frame: 0,
            fps: 0.0,
            bitrate_kbps: 0.0,
            total_size: value.parse().ok()?,
            out_time_ms: 0,
            speed: 0.0,
            is_complete: false,
        }),
        "out_time_ms" => Some(ProgressUpdate {
            frame: 0,
            fps: 0.0,
            bitrate_kbps: 0.0,
            total_size: 0,
            out_time_ms: value.parse().ok()?,
            speed: 0.0,
            is_complete: false,
        }),
        "speed" => {
            let spd = value
                .trim_end_matches('x')
                .trim()
                .parse::<f64>()
                .unwrap_or(0.0);
            Some(ProgressUpdate {
                frame: 0,
                fps: 0.0,
                bitrate_kbps: 0.0,
                total_size: 0,
                out_time_ms: 0,
                speed: spd,
                is_complete: false,
            })
        }
        "progress" if value.trim() == "end" => Some(ProgressUpdate {
            frame: 0,
            fps: 0.0,
            bitrate_kbps: 0.0,
            total_size: 0,
            out_time_ms: 0,
            speed: 0.0,
            is_complete: true,
        }),
        _ => None,
    }
}
