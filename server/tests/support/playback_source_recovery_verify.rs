// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;

#[path = "playback_source_seek_negatives.rs"]
mod seek_negatives;

pub(super) async fn arm(
    inventory: &Inventory,
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    client_kind: &str,
    observed: &Value,
) -> anyhow::Result<(Armed, Value)> {
    let item = http::uuid(observed, "itemId")?;
    let file = http::uuid(observed, "fileId")?;
    let expected_variant = if client_kind == "browser" {
        "video-slice"
    } else {
        "video-slice-native"
    };
    anyhow::ensure!(observed["caseId"] == expected_variant);
    let candidate = inventory
        .candidates
        .iter()
        .find(|candidate| {
            candidate.item == item
                && candidate.file == file
                && candidate.variant == expected_variant
        })
        .ok_or_else(|| anyhow::anyhow!("source-copy arm candidate unavailable"))?;
    let play = http::uuid(observed, "sessionId")?;
    let generation = http::uuid(observed, "transcodeId")?;
    let row:(Uuid,Uuid,Uuid,Value)=sqlx::query_as("SELECT user_id,profile_id,media_item_id,metadata FROM play_sessions WHERE id=$1 AND library_id=$2 AND stopped_at IS NULL").bind(play).bind(inventory.library).fetch_one(fixture.pool).await?;
    anyhow::ensure!(
        row.0 == fixture.user
            && row.1 == fixture.original_profile
            && row.2 == item
            && row.3["transcode_session_id"] == generation.to_string()
            && row.3["recovery_budget"]["attempts"].as_u64().unwrap_or(0) == 0
            && row.3["recovery"].is_null()
            && observed["paused"] == true
            && observed["positionMs"]
                .as_i64()
                .is_some_and(|value| (0..1500).contains(&value))
    );
    let route = super::super::source_verify::accepted(observed, &row.3)?;
    anyhow::ensure!(matches!(
        route.as_deref(),
        Some("hls_video_copy_audio_copy" | "hls_video_copy_aac")
    ));
    let session = state
        .transcode_manager
        .get_session(&generation)
        .ok_or_else(|| {
            anyhow::anyhow!("healthy copy generation unavailable before playback failure")
        })?;
    anyhow::ensure!(
        session.user_id == fixture.user
            && session.media_file_id == file
            && session.source_path == candidate.path
            && session.source_video_stream_index == Some(1)
            && session.source_audio_stream_index == Some(2)
            && session.output_profile == Some(OutputProfile::SourceCopySdr)
            && session.stream_plan.video == StreamAction::Copy
            && session.target_video_codec == "h264"
            && session.target_audio_codec == "aac"
            && session.recovery_attempts == 0
            && state
                .transcode_manager
                .session_failure(generation)
                .is_none()
            && session.segment_dir.is_dir()
    );
    anyhow::ensure!(
        session.stream_plan.audio
            == if route.as_deref() == Some("hls_video_copy_audio_copy") {
                StreamAction::Copy
            } else {
                StreamAction::Encode
            }
    );
    anyhow::ensure!(inventory::hash(&candidate.path).await? == candidate.sha256);
    Ok((
        Armed {
            play,
            generation,
            candidate: file,
            directory: session.segment_dir.clone(),
            route: observed["route"].clone(),
        },
        json!({"armed":true,"armed_generation":generation,"healthy_copy":true,"budget":0,"same_session":true,"actual_healthy_copy":true,"worker_failure":false,"attempts":0,"source_video_index":1,"source_audio_index":2,"output_profile":"source_copy_sdr","transport":if client_kind=="browser"{"cookie"}else{"bearer"}}),
    ))
}

pub(super) async fn run(
    inventory: &mut Inventory,
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    root: &Path,
    tools: &process::FixtureProcesses,
    client_kind: &str,
    observed: &Value,
    peer: &str,
    kids: &str,
) -> anyhow::Result<Value> {
    let item = http::uuid(observed, "itemId")?;
    let file = http::uuid(observed, "fileId")?;
    let expected_variant = if client_kind == "browser" {
        "video-slice"
    } else {
        "video-slice-native"
    };
    anyhow::ensure!(observed["caseId"] == expected_variant);
    let candidate = inventory
        .candidates
        .iter()
        .find(|candidate| {
            candidate.item == item
                && candidate.file == file
                && candidate.variant == expected_variant
        })
        .ok_or_else(|| {
            anyhow::anyhow!("source-recovery candidate is outside the damaged owned inventory")
        })?;
    anyhow::ensure!(
        inventory::hash(&candidate.path).await? == candidate.sha256
            && observed["positionMs"] == 5000
            && observed["paused"] == true
            && observed["attempts"] == 1
    );
    let play = http::uuid(observed, "sessionId")?;
    let generation = http::uuid(observed, "transcodeId")?;
    let previous = http::uuid(observed, "previousTranscodeId")?;
    let failed = http::uuid(observed, "failedTranscodeId")?;
    let armed = inventory
        .armed
        .get(client_kind)
        .ok_or_else(|| anyhow::anyhow!("source recovery was not independently armed"))?;
    anyhow::ensure!(
        armed.play == play
            && armed.generation == failed
            && armed.candidate == file
            && armed.route == observed["route"]
            && !armed.directory.exists()
            && state
                .transcode_manager
                .get_session(&armed.generation)
                .is_none()
    );
    anyhow::ensure!(generation != previous && generation != failed);
    let row:(Uuid,Uuid,Uuid,String,Value)=sqlx::query_as("SELECT user_id,profile_id,media_item_id,stream_decision,metadata FROM play_sessions WHERE id=$1 AND library_id=$2 AND stopped_at IS NULL").bind(play).bind(inventory.library).fetch_one(fixture.pool).await?;
    let metadata = &row.4;
    anyhow::ensure!(
        row.0 == fixture.user
            && row.1 == fixture.original_profile
            && row.2 == item
            && row.3 == "transcode"
            && metadata["media_file_id"] == file.to_string()
            && metadata["transcode_session_id"] == generation.to_string()
            && metadata["stream_offset_ms"] == 5000
            && metadata["recovery_budget"]["attempts"] == 1
            && metadata["recovery"]["status"] == "recovered"
            && metadata["recovery"]["reason"] == "client_decode"
            && metadata["recovery"]["skipped_ms"] == 0
            && metadata["recovery"]["quality_reduced"] == true
    );
    let route = super::super::source_verify::accepted(observed, metadata)?;
    anyhow::ensure!(matches!(
        route.as_deref(),
        Some("hls_video_copy_audio_copy" | "hls_video_copy_aac")
    ));
    let error = &observed["decoderError"];
    anyhow::ensure!(
        (error["code"] == 3 && error["trusted"] == true)
            || (error["fatal"] == true && error["type"] == "mediaError")
    );
    let recovery = &observed["recovery"];
    let confirmed = observed["recoveryPositionMs"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("source decoder failure position missing"))?;
    let decoder_request = &observed["decoderRequest"];
    anyhow::ensure!(
        decoder_request["session_id"] == play.to_string()
            && decoder_request["transcode_session_id"] == failed.to_string()
            && decoder_request["recovery_requested"] == true
            && decoder_request["copy_fallback"]["generation"] == failed.to_string()
            && decoder_request["copy_fallback"]["supported"] == true
            && decoder_request["copy_fallback"]["smooth"] == true
            && decoder_request["position_ms"] == confirmed
    );
    anyhow::ensure!(
        observed["confirmedPositionMs"]
            .as_f64()
            .is_some_and(|position| (5000.0..=8000.0).contains(&position))
    );
    anyhow::ensure!(
        (0..=4500).contains(&confirmed)
            && recovery["status"] == "recovered"
            && recovery["reason"] == "client_decode"
            && recovery["attempts"] == 1
            && recovery["skipped_ms"] == 0
            && recovery["quality_reduced"] == true
    );
    let events:Vec<Value>=sqlx::query_scalar("SELECT details FROM play_events WHERE play_session_id=$1 AND event_type='transcode_change' AND details->>'cause'='recovery' ORDER BY created_at,id LIMIT 3").bind(play).fetch_all(fixture.pool).await?;
    anyhow::ensure!(
        events.len() == 1
            && events[0]["attempts"] == 1
            && events[0]["reason"] == "client_decode"
            && events[0]["skipped_ms"] == 0
    );
    let session = state
        .transcode_manager
        .get_session(&generation)
        .ok_or_else(|| anyhow::anyhow!("source recovered generation unavailable"))?;
    anyhow::ensure!(
        session.user_id == fixture.user
            && session.media_file_id == file
            && session.source_path == candidate.path
            && session.source_video_stream_index == Some(1)
            && session.source_audio_stream_index == Some(2)
            && session.stream_plan.video == StreamAction::Encode
            && session.stream_plan.audio == StreamAction::Encode
            && session.output_profile == Some(OutputProfile::CompatibleSdr)
            && session.target_video_codec == "h264"
            && session.target_audio_codec == "aac"
            && session.hw_accel == HwAccelMethod::Software
            && session.source_start_ms == 5000
            && session.recovery_attempts == 1
            && session.segment_dir.canonicalize()?.starts_with(root)
    );
    let parent = session
        .segment_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("source recovered directory missing"))?;
    for old in [failed, previous] {
        anyhow::ensure!(
            state.transcode_manager.get_session(&old).is_none()
                && !parent.join(old.to_string()).exists(),
            "source recovery/seek did not retire a physical generation"
        );
    }
    let source_start = observed["replacementOffsetMs"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("source replacement offset proof missing"))?;
    anyhow::ensure!((source_start - confirmed).abs() <= 100);
    let client = policy::client()?;
    let headers = http::headers(fixture.token, client_kind)?;
    let replay=client.post(format!("{}/api/v1/playback/heartbeat",policy::ORIGIN)).headers(headers.clone()).json(&json!({"session_id":play,"position_ms":0,"transcode_session_id":failed,"recovery_requested":true,"copy_fallback":{"generation":failed,"supported":true,"smooth":true},"is_paused":false})).send().await?;
    anyhow::ensure!(replay.status() == reqwest::StatusCode::OK);
    let replay: Value = replay.json().await?;
    let after: Value = sqlx::query_scalar("SELECT metadata FROM play_sessions WHERE id=$1")
        .bind(play)
        .fetch_one(fixture.pool)
        .await?;
    anyhow::ensure!(
        replay["transcode_session_id"] == generation.to_string()
            && replay["position_ms"]
                .as_i64()
                .is_some_and(|position| position >= 5000)
            && after["transcode_session_id"] == metadata["transcode_session_id"]
            && after["source_route_context"] == metadata["source_route_context"]
            && after["recovery_budget"] == metadata["recovery_budget"]
            && after["current_position_ms"]
                .as_i64()
                .is_some_and(|position| position >= 5000)
            && state.transcode_manager.active_session_count() <= 1,
        "stale decoder hint changed its canonical generation/checkpoint/budget"
    );
    let actual = source_media::recovered(
        tools,
        root,
        &session,
        &format!("source-recovery-{client_kind}"),
        &client,
        &headers,
    )
    .await?;
    let negative_seek = seek_negatives::run(
        fixture,
        state,
        &client,
        &headers,
        play,
        file,
        generation,
        &session.segment_dir,
        kids,
    )
    .await?;
    let denied = http::negatives(
        &client,
        &format!("/api/v1/transcode/{generation}/manifest.m3u8"),
        peer,
        kids,
    )
    .await?;
    inventory
        .sessions
        .push((play, generation, session.segment_dir.clone()));
    Ok(
        json!({"item":item,"file":file,"variant":candidate.variant,"same_session":true,"healthy_copy_promoted":true,"genuine_decoder_failure":true,"recovery_reason":"client_decode","confirmed_position_ms":confirmed,"replacement_offset_ms":source_start,"old_generation_retired":true,"post_recovery_seek_preserved_encode":true,"stale_hint_readonly":true,"negative_seek":negative_seek,"source_offset_ms":5000,"selected_video_index":1,"selected_audio_index":2,"attempts":1,"skipped_ms":0,"quality_reduced":true,"output_profile":"compatible_sdr","transport":if client_kind=="browser"{"cookie"}else{"bearer"},"sources_unchanged":true,"authorization":denied,"actual_media":actual}),
    )
}
