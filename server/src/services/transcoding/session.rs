// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{StartSessionParams, TranscodeSession};

impl TranscodeSession {
    pub(crate) fn restart_params(self, position_ms: i64) -> StartSessionParams {
        StartSessionParams {
            media_file_id: self.media_file_id,
            user_id: self.user_id,
            source_path: self.source_path,
            source_video_codec: self.source_video_codec,
            source_video_resolution: self.source_video_resolution,
            source_audio_codec: self.source_audio_codec,
            source_video_stream_index: self.source_video_stream_index,
            source_audio_stream_index: self.source_audio_stream_index,
            stream_plan: self.stream_plan,
            output_profile: self.output_profile,
            target_audio_channels: self.target_audio_channels,
            target_audio_bitrate: self.target_audio_bitrate,
            subtitle_stream_index: self.subtitle_stream_index,
            subtitle_stream_ordinal: self.subtitle_stream_ordinal,
            subtitle_burn_in: self.subtitle_burn_in,
            target_video_codec: Some(self.target_video_codec),
            target_audio_codec: Some(self.target_audio_codec),
            target_resolution: Some(self.target_video_resolution),
            target_bitrate: (self.target_bitrate > 0).then_some(self.target_bitrate),
            seek_position_ms: Some(position_ms),
            hw_accel_override: Some(self.hw_accel),
            recovery_attempts: self.recovery_attempts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::transcoding::{
        HwAccelMethod,
        plan::{StreamAction, StreamPlan},
    };
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn copied_video_seek_retains_finalized_surround_audio_and_source_identity() {
        let session = TranscodeSession {
            id: Uuid::now_v7(),
            media_file_id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            started_at: Utc::now(),
            source_path: "movie.mkv".into(),
            source_video_codec: "h264".into(),
            source_video_resolution: (1920, 1080),
            source_audio_codec: "dts".into(),
            source_video_stream_index: Some(4),
            source_audio_stream_index: Some(0),
            stream_plan: StreamPlan {
                video: StreamAction::Copy,
                audio: StreamAction::Encode,
            },
            output_profile: None,
            target_audio_channels: Some(6),
            target_audio_bitrate: Some(320_000),
            subtitle_stream_index: None,
            subtitle_stream_ordinal: None,
            subtitle_burn_in: false,
            target_video_codec: "h264".into(),
            target_video_resolution: (1920, 1080),
            target_audio_codec: "aac".into(),
            target_bitrate: 0,
            hw_accel: HwAccelMethod::Software,
            source_start_ms: 0,
            recovery_attempts: 1,
            rendition_name: "source".into(),
            segment_dir: "cache".into(),
            manifest_path: "cache/manifest.m3u8".into(),
            segments_written: 0,
            client_position_segment: 0,
            progress: None,
            is_complete: false,
            is_seeking: false,
        };
        let restart = session.restart_params(125_000);
        assert_eq!(restart.target_audio_channels, Some(6));
        assert_eq!(restart.target_audio_bitrate, Some(320_000));
        assert_eq!(restart.source_video_stream_index, Some(4));
        assert_eq!(restart.source_audio_stream_index, Some(0));
        assert_eq!(restart.stream_plan.video, StreamAction::Copy);
        assert_eq!(restart.stream_plan.audio, StreamAction::Encode);
        assert_eq!(restart.seek_position_ms, Some(125_000));
        assert_eq!(restart.hw_accel_override, Some(HwAccelMethod::Software));
        assert_eq!(restart.recovery_attempts, 1);
        assert!(restart.target_bitrate.is_none());
    }
}
