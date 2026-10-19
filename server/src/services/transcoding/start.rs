// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::arguments::{
    build_ffmpeg_input_args, build_ffmpeg_output_seek_args, build_hls_output_args,
    build_stream_mapping_args_for_video, build_subtitle_burn_in_filter_for_video,
    compose_video_filters,
};
use super::encoding::{build_audio_encode_args, build_threading_args, build_video_encode_args};
use super::launcher::{LaunchReadiness, spawn_session_ffmpeg};
use super::lifecycle::{ExecutionState, WorkerLaunch, WorkerObservation};
use super::progress::ProgressParser;
use super::*;
use tokio_process_tools::Next;

impl TranscodeManager {
    pub async fn start_session(
        &self,
        mut params: StartSessionParams,
        data_dir: &Path,
    ) -> Result<TranscodeSession, PlaybackError> {
        if let Some(profile) = params.output_profile {
            profile
                .validate(&params)
                .map_err(|message| PlaybackError::FfmpegFailed(message.into()))?;
        }
        let selected_hardware = params
            .hw_accel_override
            .unwrap_or_else(|| self.get_hw_accel());
        let allow_startup_fallback = params.hw_accel_override.is_none()
            && params.stream_plan.video == StreamAction::Encode
            && selected_hardware != HwAccelMethod::Software;
        params.hw_accel_override = Some(selected_hardware);
        let mut observation = None;
        let result = self
            .start_attempt(params.clone(), data_dir, &mut observation)
            .await;
        let hardware_failed = observation.as_ref().is_some_and(|(id, observation)| {
            let snapshot = observation.snapshot();
            self.get_session(id).is_none()
                && snapshot.state == ExecutionState::Failed
                && snapshot.failure == Some(FailureClass::Hardware)
        });
        if result.is_err()
            && hardware_failed
            && params.recovery_attempts < recovery::MAX_RECOVERY_ATTEMPTS
            && allow_startup_fallback
            && params.stream_plan.video == StreamAction::Encode
        {
            params.hw_accel_override = Some(HwAccelMethod::Software);
            params.recovery_attempts += 1;
            return self.start_attempt(params, data_dir, &mut None).await;
        }
        result
    }

    async fn start_attempt(
        &self,
        params: StartSessionParams,
        data_dir: &Path,
        observation: &mut Option<(Uuid, WorkerObservation)>,
    ) -> Result<TranscodeSession, PlaybackError> {
        let profiled_rendition = params
            .output_profile
            .map(|profile| profile.rendition(&params))
            .transpose()
            .map_err(|message| PlaybackError::FfmpegFailed(message.into()))?;
        let profiled_args = params
            .output_profile
            .map(|profile| profile.output_args(&params));
        let StartSessionParams {
            media_file_id,
            user_id,
            source_path,
            source_video_codec,
            source_video_resolution,
            source_audio_codec,
            source_video_stream_index,
            source_audio_stream_index,
            stream_plan,
            output_profile,
            target_audio_channels,
            target_audio_bitrate,
            subtitle_stream_index,
            subtitle_stream_ordinal,
            subtitle_burn_in,
            target_video_codec,
            target_audio_codec,
            target_resolution,
            target_bitrate,
            seek_position_ms,
            hw_accel_override,
            recovery_attempts,
        } = params;

        let permit = Arc::clone(&self.semaphore)
            .try_acquire_owned()
            .map_err(|_| PlaybackError::TranscodeCapacityReached)?;

        let config = self.config.load_full();
        let transcoding = &config.transcoding;
        let cpu = &config.cpu;

        let hw_accel = if stream_plan.video == StreamAction::Encode {
            hw_accel_override.unwrap_or_else(|| self.get_hw_accel())
        } else {
            HwAccelMethod::Software
        };
        let effective_video_codec = if stream_plan.video == StreamAction::Copy {
            source_video_codec.clone()
        } else {
            target_video_codec.unwrap_or_else(|| transcoding.default_video_codec.clone())
        };
        let effective_audio_codec = if stream_plan.audio == StreamAction::Copy {
            source_audio_codec.clone()
        } else {
            target_audio_codec.unwrap_or_else(|| transcoding.default_audio_codec.clone())
        };

        let encoder = hw_accel.ffmpeg_encoder(&effective_video_codec);

        let (target_w, target_h) = target_resolution.unwrap_or(source_video_resolution);

        let mut rendition = profiled_rendition.unwrap_or_else(|| {
            TranscodeRendition::smart_ladder(
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
            })
        });

        if stream_plan.video == StreamAction::Copy {
            rendition.name = "source".into();
            rendition.width = source_video_resolution.0;
            rendition.height = source_video_resolution.1;
            rendition.video_bitrate = 0;
        }
        if let Some(channels) = target_audio_channels {
            rendition.audio_channels = channels;
        }
        if let Some(bitrate) = target_audio_bitrate {
            rendition.audio_bitrate = bitrate;
        }

        let session_id = Uuid::now_v7();
        let segment_dir = data_dir
            .join(transcoding.transcode_path.trim_start_matches('/'))
            .join(session_id.to_string());
        let manifest_path = segment_dir.join("manifest.m3u8");

        let segment_filename = segment_dir.join("seg_%04d.m4s");

        let fps = if output_profile == Some(OutputProfile::CompatibleSdr) {
            output_profile::FRAME_RATE
        } else {
            24
        };
        let gop_size = fps * transcoding.segment_duration_seconds;

        let timestamp_preserving = output_profile == Some(OutputProfile::SourceCopySdr);
        let mut args = if timestamp_preserving {
            vec!["-copyts".into(), "-start_at_zero".into()]
        } else {
            Vec::new()
        };
        args.extend(
            self.build_session_input_args(seek_position_ms, &source_path)
                .await?,
        );
        args.extend(build_ffmpeg_output_seek_args(seek_position_ms, stream_plan));

        let subtitle_filter = if subtitle_burn_in {
            let ordinal = subtitle_stream_ordinal.ok_or_else(|| {
                PlaybackError::FfmpegFailed(
                    "subtitle burn-in requires a selected subtitle stream".into(),
                )
            })?;
            Some(build_subtitle_burn_in_filter_for_video(
                &source_path,
                ordinal,
                source_video_stream_index,
            ))
        } else {
            None
        };
        let video_args = if stream_plan.video == StreamAction::Encode {
            build_video_encode_args(
                &encoder,
                &effective_video_codec,
                rendition.width,
                rendition.height,
                rendition.video_bitrate,
                gop_size,
                hw_accel,
            )
        } else {
            Vec::new()
        };
        let (video_args, subtitle_filter) = compose_video_filters(video_args, subtitle_filter)
            .map_err(|message| PlaybackError::FfmpegFailed(message.into()))?;
        args.extend(build_stream_mapping_args_for_video(
            source_video_stream_index,
            source_audio_stream_index,
            subtitle_filter,
        ));
        args.extend(stream_plan.codec_args(
            video_args,
            build_audio_encode_args(
                &effective_audio_codec,
                rendition.audio_channels,
                rendition.audio_bitrate,
            ),
        ));
        if let Some(profile_args) = profiled_args {
            args.extend(profile_args);
        }
        if timestamp_preserving {
            args.extend(["-avoid_negative_ts".into(), "disabled".into()]);
        }

        args.extend(build_threading_args(cpu));

        args.extend(["-progress".to_string(), "pipe:1".to_string()]);

        args.extend(build_hls_output_args(
            transcoding.segment_duration_seconds,
            &segment_filename.to_string_lossy(),
            &manifest_path.to_string_lossy(),
        ));

        tokio::fs::create_dir_all(&segment_dir).await.map_err(|e| {
            PlaybackError::FfmpegFailed(format!("failed to create segment dir: {e}"))
        })?;

        let spawned = spawn_session_ffmpeg(&args, session_id, &source_path, &segment_dir).await?;

        let mut session = TranscodeSession {
            id: session_id,
            media_file_id,
            user_id,
            started_at: Utc::now(),
            source_path,
            source_video_codec,
            source_video_resolution,
            source_audio_codec,
            source_video_stream_index,
            source_audio_stream_index,
            stream_plan,
            output_profile,
            target_audio_channels: (stream_plan.audio == StreamAction::Encode)
                .then_some(rendition.audio_channels),
            target_audio_bitrate: (stream_plan.audio == StreamAction::Encode)
                .then_some(rendition.audio_bitrate),
            subtitle_stream_index,
            subtitle_stream_ordinal,
            subtitle_burn_in,
            target_video_codec: effective_video_codec,
            target_video_resolution: (rendition.width, rendition.height),
            target_audio_codec: effective_audio_codec,
            target_bitrate: rendition.video_bitrate,
            hw_accel,
            source_start_ms: if timestamp_preserving {
                0
            } else {
                seek_position_ms.unwrap_or(0)
            },
            recovery_attempts,
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
        let mut parser = ProgressParser::default();
        self.sessions.insert(session_id, session.clone());
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
                if let Some(mut update) = parser.push(&line)
                    && let Some(mut session) = sessions.get_mut(&session_id)
                {
                    if timestamp_preserving {
                        update.out_time_ms = update
                            .out_time_ms
                            .saturating_sub(session.source_start_ms.max(0) as u64);
                    }
                    session.progress = Some(update);
                }
                Next::Continue
            },
            move || {
                cleanup_sessions.remove(&session_id);
            },
        );

        *observation = self
            .workers
            .observation(session_id)
            .map(|observation| (session_id, observation));
        let mut publication = self
            .workers
            .pending(session_id)
            .ok_or_else(|| PlaybackError::FfmpegFailed("HLS startup ownership was lost".into()))?;
        let deadline = tokio::time::Instant::now() + readiness::STARTUP_TIMEOUT;
        self.confirm_startup(&session, spawned.readiness, deadline)
            .await?;
        if timestamp_preserving {
            let origin = tokio::time::timeout_at(
                deadline,
                readiness::copied_source_origin(
                    &session.segment_dir,
                    &session.manifest_path,
                    seek_position_ms.unwrap_or(0),
                ),
            )
            .await
            .unwrap_or_else(|_| {
                Err(PlaybackError::FfmpegFailed(
                    "source-copy timestamp startup deadline exceeded".into(),
                ))
            });
            let origin = match origin {
                Ok(origin) => origin,
                Err(error) => {
                    self.stop_session(session_id).await?;
                    return Err(error);
                }
            };
            session.source_start_ms = origin;
            let mut updated = false;
            if let Some(mut stored) = self.sessions.get_mut(&session_id) {
                stored.source_start_ms = origin;
                if let Some(progress) = stored.progress.as_mut() {
                    progress.out_time_ms = progress.out_time_ms.saturating_sub(origin as u64);
                }
                session.progress = stored.progress.clone();
                updated = true;
            }
            if !updated {
                self.stop_session(session_id).await?;
                return Err(PlaybackError::FfmpegFailed(
                    "HLS startup ownership was lost".into(),
                ));
            }
        }
        publication.acknowledge();

        Ok(session)
    }

    pub(super) async fn build_session_input_args(
        &self,
        seek_position_ms: Option<i64>,
        source_path: &Path,
    ) -> Result<Vec<String>, PlaybackError> {
        let ordinary = build_ffmpeg_input_args(seek_position_ms, source_path);
        #[cfg(all(target_os = "linux", any(test, feature = "playback-qualification")))]
        if let Some(input) = &self.qualification_input
            && input.applies(source_path).await.map_err(|error| {
                PlaybackError::FfmpegFailed(format!(
                    "qualification input validation failed: {error}"
                ))
            })?
        {
            let mut args = qualification::INPUT_ARGS
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>();
            args.extend(ordinary);
            return Ok(args);
        }
        Ok(ordinary)
    }

    async fn confirm_startup(
        &self,
        session: &TranscodeSession,
        launch: LaunchReadiness,
        deadline: tokio::time::Instant,
    ) -> Result<(), PlaybackError> {
        readiness::confirm(
            &self.workers,
            session.id,
            &session.segment_dir,
            &session.manifest_path,
            launch.wait(),
            deadline,
        )
        .await
    }
}
