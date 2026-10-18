// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::{Path, PathBuf};

use duskcue::services::transcoding::plan::StreamAction;
use serde_json::{Value, json};
use uuid::Uuid;

use super::super::native::Candidate;
use super::{AppState, ProgressiveFixture, process};

#[path = "playback_recovery_process.rs"]
mod injection;

pub(super) struct ArmedStall {
    play: Uuid,
    generation: Uuid,
    cache: PathBuf,
    encoder: process::Encoder,
    checkpoint: Option<Uuid>,
    _frozen: injection::FrozenEncoder,
}

impl ArmedStall {
    pub(super) async fn arm(
        fixture: &ProgressiveFixture<'_>,
        state: &AppState,
        root: &Path,
        candidate: &Candidate,
        request: &Value,
    ) -> anyhow::Result<(Self, Value)> {
        let resource = std::env::var("DUSKCUE_TEST_RESOURCE_ID")?;
        anyhow::ensure!(
            request["resourceId"] == resource
                && request["positionMs"] == 4000
                && request["paused"] == true
        );
        let play = uuid(request, "sessionId")?;
        let generation = uuid(request, "transcodeId")?;
        let row: (Uuid, Uuid, Uuid, Value) = sqlx::query_as("SELECT user_id,profile_id,media_item_id,metadata FROM play_sessions WHERE id=$1 AND stopped_at IS NULL")
            .bind(play).fetch_one(fixture.pool).await?;
        let metadata = row.3;
        anyhow::ensure!(
            row.0 == fixture.user
                && row.1 == fixture.original_profile
                && row.2 == candidate.item
                && metadata["media_file_id"] == candidate.file.to_string()
                && metadata["transcode_session_id"] == generation.to_string()
                && metadata["recovery_budget"]["attempts"]
                    .as_u64()
                    .unwrap_or(0)
                    == 0
        );
        anyhow::ensure!(
            metadata["stream_offset_ms"] == 4000 && metadata["current_position_ms"] == 4000,
            "native arm checkpoint differs from 4000ms: offset={:?}, position={:?}, paused={}",
            metadata["stream_offset_ms"].as_i64(),
            metadata["current_position_ms"].as_i64(),
            metadata["current_state"] == "paused"
        );
        let session = state
            .transcode_manager
            .get_session(&generation)
            .ok_or_else(|| anyhow::anyhow!("native stall generation unavailable"))?;
        anyhow::ensure!(
            session.user_id == fixture.user
                && session.media_file_id == candidate.file
                && session.source_path == candidate.path
                && session.source_path.canonicalize()?.starts_with(root)
                && session.segment_dir.canonicalize()?.starts_with(root)
                && session.source_start_ms == 4000
                && session.source_video_stream_index == Some(1)
                && session.source_audio_stream_index == Some(2)
                && session.stream_plan.video == StreamAction::Copy
                && session.stream_plan.audio == StreamAction::Copy
                && session.output_profile.is_none()
                && session.recovery_attempts == 0
                && state
                    .transcode_manager
                    .session_failure(generation)
                    .is_none()
        );
        let encoder =
            process::Encoder::observe(&candidate.path, &session.manifest_path, Some(4000))?;
        let frozen = injection::FrozenEncoder::stop(&encoder)?;
        let response = json!({"resourceId":resource,"sessionId":play,"transcodeId":generation,"positionMs":4000,"paused":true,"armed":true,"encoderIdentity":encoder.recorded_identity()});
        Ok((
            Self {
                play,
                generation,
                cache: session.segment_dir,
                encoder,
                checkpoint: None,
                _frozen: frozen,
            },
            response,
        ))
    }

    pub(super) async fn verify(
        &mut self,
        fixture: &ProgressiveFixture<'_>,
        state: &AppState,
        observed: &Value,
        metadata: &Value,
    ) -> anyhow::Result<Value> {
        let generation = uuid(observed, "transcodeId")?;
        let previous = uuid(observed, "previousTranscodeId")?;
        let phase = observed["phase"].as_str().unwrap_or("");
        anyhow::ensure!(
            uuid(observed, "sessionId")? == self.play
                && generation != self.generation
                && metadata["recovery_budget"]["attempts"] == 1
                && metadata["recovery_budget"]["exhausted"] != true
                && metadata["recovery"]["status"] == "recovered"
                && metadata["recovery"]["reason"] == "stalled"
                && metadata["recovery"]["attempts"] == 1
                && metadata["recovery"]["skipped_ms"] == 0
                && metadata["recovery"]["quality_reduced"] == false
        );
        self.encoder.gone()?;
        anyhow::ensure!(
            !self.cache.exists()
                && state
                    .transcode_manager
                    .get_session(&self.generation)
                    .is_none()
        );
        let recoveries: i64 = sqlx::query_scalar("SELECT count(*) FROM play_events WHERE play_session_id=$1 AND event_type='transcode_change' AND details @> '{\"cause\":\"recovery\",\"status\":\"recovered\",\"reason\":\"stalled\",\"attempts\":1}'::jsonb")
            .bind(self.play).fetch_one(fixture.pool).await?;
        anyhow::ensure!(recoveries == 1);
        if phase == "native-recovery" {
            anyhow::ensure!(
                self.checkpoint.is_none()
                    && previous == self.generation
                    && observed["positionMs"] == 4000
                    && metadata["current_state"] == "paused"
            );
            self.checkpoint = Some(generation);
        } else {
            anyhow::ensure!(
                phase == "native"
                    && self.checkpoint == Some(previous)
                    && observed["positionMs"] == 6000
            );
        }
        Ok(
            json!({"reason":"stalled","attempts":1,"skipped_ms":0,"quality_reduced":false,"paused":true,
            "frozen_encoder_gone":true,"stalled_cache_retired":true,"same_session":true,
            "encoder_identity":self.encoder.recorded_identity(),"stalled_generation":self.generation,
            "recovered_generation":self.checkpoint,"source_offset_ms":4000}),
        )
    }
}

fn uuid(value: &Value, field: &str) -> anyhow::Result<Uuid> {
    value[field]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("native stall identity missing"))?
        .parse()
        .map_err(Into::into)
}
