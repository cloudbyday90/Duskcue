// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use duskcue::services::transcoding::{
    HwAccelMethod, OutputProfile,
    plan::{StreamAction, StreamPlan},
};
use serde_json::{Value, json};
use uuid::Uuid;

use super::super::policy;
use super::{
    AppState, Inventory, ProgressiveFixture, inventory, process, source_http as http, source_media,
};

pub(super) async fn scanner_metadata(
    metadata: &Value,
    path: &Path,
    source: &Value,
) -> anyhow::Result<Value> {
    let descriptor =
        duskcue::services::source_capabilities::PlaybackDescriptor::from_metadata(metadata)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "golden source lacks an exact actual scanner descriptor: {}",
                    source["id"]
                )
            })?;
    let file = tokio::fs::File::open(path).await?;
    anyhow::ensure!(
        descriptor.scan.matches_metadata(&file.metadata().await?)
            && descriptor.video.index == 0
            && descriptor.video.codec == source["codec"].as_str().unwrap_or("")
            && descriptor.video.bit_depth == source["bitDepth"].as_u64().unwrap_or(0) as u8
            && descriptor.video.width == 320
            && descriptor.video.height == 180
            && descriptor.video.frame_rate == 24.0
            && descriptor.video.dynamic_range == source["dynamicRange"].as_str().unwrap_or("")
    );
    let hex = source["configuration"]["configurationHex"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("golden source decoder configuration missing"))?;
    anyhow::ensure!(
        hex.len() <= 131072
            && hex.len() % 2 == 0
            && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
    );
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16))
        .collect::<Result<Vec<_>, _>>()?;
    let expected_hash = source_media::sha256(&bytes);
    anyhow::ensure!(
        source["configuration"]["configurationSource"] == "ffprobe-demuxed-extradata"
            && source["configuration"]["streamIndex"] == 0
            && source["configuration"]["configurationSha256"] == expected_hash
            && descriptor.video.config_sha256 == expected_hash,
        "actual scanned configuration differs from authored per-file configuration: id={},expected_sha256={},scanned_sha256={}",
        source["id"],
        expected_hash,
        descriptor.video.config_sha256
    );
    let audio = descriptor
        .audio
        .iter()
        .find(|audio| audio.index == 2)
        .ok_or_else(|| anyhow::anyhow!("golden default audio descriptor missing"))?;
    anyhow::ensure!(
        audio.codec == "opus"
            && audio.file_codec == "opus"
            && audio.copy_codec.is_none()
            && audio.sample_rate == 48000
            && audio.channels == 1
    );
    Ok(
        json!({"id":source["id"],"file_codec":descriptor.video.file_codec,"copy_codec":descriptor.video.copy_codec,"configuration_sha256":descriptor.video.config_sha256,"scan_identity_matches":true,"bitrate_basis":descriptor.video.bitrate_basis,"selected_audio_index":2,"opus_hls_copy_admitted":false}),
    )
}

pub(super) fn accepted(observed: &Value, metadata: &Value) -> anyhow::Result<Option<String>> {
    let preparation = &observed["preparation"];
    let observations = &observed["observations"];
    if preparation.is_null() || observations.is_null() {
        anyhow::ensure!(metadata["source_route_context"].is_null() && observed["route"].is_null());
        return Ok(None);
    }
    anyhow::ensure!(
        preparation["protocol"] == 1
            && observations["version"] == 1
            && preparation["source_revision"] == observations["source_revision"]
    );
    let candidates = preparation["candidates"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("source prepared candidates missing"))?;
    let reports = observations["observations"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("source capability reports missing"))?;
    anyhow::ensure!(candidates.len() <= 3 && reports.len() <= 3);
    let revision = preparation["source_revision"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("source capability revision missing"))?;
    anyhow::ensure!(
        revision.len() == 64
            && revision
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    let selected = candidates.iter().find(|candidate| {
        reports.iter().any(|report| {
            report["candidate_id"] == candidate["id"]
                && report["status"] == "reported"
                && report["supported"] == true
                && report["smooth"] == true
                && report["delivery_supported"] == true
        })
    });
    let Some(selected) = selected else {
        anyhow::ensure!(metadata["source_route_context"].is_null() && observed["route"].is_null());
        return Ok(None);
    };
    let id = selected["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("source candidate identity missing"))?;
    anyhow::ensure!(
        matches!(
            id,
            "native_file" | "hls_video_copy_audio_copy" | "hls_video_copy_aac"
        ) && metadata["source_route_context"]["version"] == 1
            && metadata["source_route_context"]["candidate_id"] == id
            && metadata["source_route_context"]["source_revision"]
                == preparation["source_revision"]
            && observed["route"]["candidate_id"] == id
            && observed["route"]["source_revision"] == preparation["source_revision"]
    );
    Ok(Some(id.into()))
}

fn copy_decision_matches(decision: &str, plan: StreamPlan) -> bool {
    plan.video == StreamAction::Copy
        && decision
            == match plan.audio {
                StreamAction::Copy => "direct_stream",
                StreamAction::Encode => "transcode",
            }
}

pub(super) async fn run(
    inventory: &mut Inventory,
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    root: &Path,
    tools: &process::FixtureProcesses,
    index: usize,
    client_kind: &str,
    observed: &Value,
) -> anyhow::Result<Value> {
    let case = &inventory.cases[index];
    anyhow::ensure!(
        observed["caseId"] == case.source["id"]
            && observed["itemId"] == case.item.to_string()
            && observed["fileId"] == case.file.to_string()
            && inventory::hash(&case.path).await? == case.source["sha256"]
    );
    if case.hdr() {
        return super::verify::run(
            inventory,
            fixture,
            state,
            root,
            tools,
            index,
            client_kind,
            observed,
        )
        .await;
    }
    let play = http::uuid(observed, "sessionId")?;
    anyhow::ensure!(
        observed["positionMs"] == 5000 && observed["paused"] == true && observed["attempts"] == 0
    );
    let row:(Uuid,Uuid,Uuid,String,Value)=sqlx::query_as("SELECT user_id,profile_id,media_item_id,stream_decision,metadata FROM play_sessions WHERE id=$1 AND library_id=$2 AND stopped_at IS NULL").bind(play).bind(inventory.library).fetch_one(fixture.pool).await?;
    anyhow::ensure!(
        row.0 == fixture.user
            && row.1 == fixture.original_profile
            && row.2 == case.item
            && row.4["media_file_id"] == case.file.to_string()
            && row.4["current_position_ms"]
                .as_i64()
                .is_some_and(|value| (5000..=8000).contains(&value))
            && row.4["quality_mode"] == "auto"
            && row.4["recovery_budget"]["attempts"].as_u64().unwrap_or(0) == 0
    );
    let route = accepted(observed, &row.4)?;
    let client = policy::client()?;
    let headers = http::headers(fixture.token, client_kind)?;
    let event:i64=sqlx::query_scalar("SELECT (details->>'target_position_ms')::bigint FROM play_events WHERE play_session_id=$1 AND event_type='seek' ORDER BY created_at DESC,id DESC LIMIT 1").bind(play).fetch_one(fixture.pool).await?;
    anyhow::ensure!(event == 5000);
    if route.as_deref() == Some("native_file") {
        let revision = http::uuid(observed, "directSourceRevision")?;
        let previous = http::uuid(observed, "previousDirectSourceRevision")?;
        anyhow::ensure!(
            client_kind == "browser"
                && row.3 == "direct_play"
                && row.4["transcode_session_id"].is_null()
                && observed["transcodeId"].is_null()
                && revision != previous
                && row.4["direct_source_revision"] == revision.to_string()
                && state.transcode_manager.active_session_count() == 0
        );
        let bytes =
            http::fetch(&client, &format!("/api/v1/stream/{}", case.file), &headers).await?;
        anyhow::ensure!(source_media::sha256(&bytes) == case.source["sha256"]);
        let actual = source_media::native(tools, &case.path).await?;
        let denied = http::negatives(
            &client,
            &format!("/api/v1/stream/{}", case.file),
            &inventory.peer_token,
            &inventory.kids_token,
        )
        .await?;
        inventory.source_sessions.push(play);
        return Ok(
            json!({"case_id":case.source["id"],"index":index,"outcome":"native_file","same_session":true,"direct_revision_rotated":true,"source_offset_ms":5000,"selected_video_index":0,"selected_audio_index":2,"attempts":0,"transport":"cookie","sources_unchanged":true,"authorization":denied,"actual_media":actual,"source_route":observed["route"]}),
        );
    }
    let generation = http::uuid(observed, "transcodeId")?;
    let previous = http::uuid(observed, "previousTranscodeId")?;
    anyhow::ensure!(
        generation != previous && row.4["transcode_session_id"] == generation.to_string()
    );
    let session = state
        .transcode_manager
        .get_session(&generation)
        .ok_or_else(|| anyhow::anyhow!("source HLS generation missing"))?;
    anyhow::ensure!(
        session.user_id == fixture.user
            && session.media_file_id == case.file
            && session.source_path == case.path
            && session.source_video_stream_index == Some(0)
            && session.source_audio_stream_index == Some(2)
            && row.4["stream_offset_ms"] == session.source_start_ms
            && session.recovery_attempts == 0
            && session.hw_accel == HwAccelMethod::Software
            && session.segment_dir.canonicalize()?.starts_with(root)
    );
    let copied = route.is_some();
    if copied {
        anyhow::ensure!(
            copy_decision_matches(&row.3, session.stream_plan)
                && session.output_profile == Some(OutputProfile::SourceCopySdr)
                && session.stream_plan.video == StreamAction::Copy
                && session.target_video_codec == case.source["codec"].as_str().unwrap_or("")
                && session.target_audio_codec == "aac"
                && session.stream_plan.audio == StreamAction::Encode
                && route.as_deref() == Some("hls_video_copy_aac")
        );
    } else {
        anyhow::ensure!(
            row.3 == "transcode"
                && session.source_start_ms == 5000
                && session.output_profile.is_none()
                && session.stream_plan.video == StreamAction::Encode
                && session.stream_plan.audio == StreamAction::Encode
                && session.target_video_codec == "h264"
                && session.target_audio_codec == "aac"
        );
    }
    let old = session
        .segment_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("source generation parent missing"))?
        .join(previous.to_string());
    anyhow::ensure!(state.transcode_manager.get_session(&previous).is_none() && !old.exists());
    let (init, fragment) =
        http::fragments(&client, generation, &session.rendition_name, &headers).await?;
    let actual = source_media::hls(
        tools,
        root,
        &format!("source-{client_kind}-{index:02}"),
        &session,
        &init,
        &fragment,
        copied,
        &case.source,
    )
    .await?;
    let denied = http::negatives(
        &client,
        &format!("/api/v1/transcode/{generation}/manifest.m3u8"),
        &inventory.peer_token,
        &inventory.kids_token,
    )
    .await?;
    inventory
        .sessions
        .push((play, generation, session.segment_dir.clone()));
    Ok(
        json!({"case_id":case.source["id"],"index":index,"outcome":if copied{"video_copy_aac"}else{"sdr_fallback"},"same_session":true,"old_generation_retired":true,"source_offset_ms":5000,"selected_video_index":0,"selected_audio_index":2,"attempts":0,"output_profile":if copied{"source_copy_sdr"}else{"ordinary_sdr"},"transport":if client_kind=="browser"{"cookie"}else{"bearer"},"sources_unchanged":true,"authorization":denied,"actual_media":actual,"source_route":observed["route"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(smooth: bool) -> Value {
        json!({"preparation":{"protocol":1,"source_revision":"a".repeat(64),"candidates":[{"id":"native_file"}]},"observations":{"version":1,"source_revision":"a".repeat(64),"observations":[{"candidate_id":"native_file","status":"reported","supported":true,"smooth":smooth,"delivery_supported":true}]},"route":null})
    }

    #[test]
    fn source_proof_requires_both_smooth_support_and_matching_server_context() {
        assert_eq!(accepted(&report(false), &json!({})).unwrap(), None);
        let mut reported = report(true);
        reported["route"] = json!({"candidate_id":"native_file","source_revision":"a".repeat(64)});
        let metadata = json!({"source_route_context":{"version":1,"candidate_id":"native_file","source_revision":"a".repeat(64)}});
        assert_eq!(
            accepted(&reported, &metadata).unwrap().as_deref(),
            Some("native_file")
        );
        let mut wrong = metadata;
        wrong["source_route_context"]["source_revision"] = json!("b".repeat(64));
        assert!(accepted(&reported, &wrong).is_err());
    }

    #[test]
    fn unavailable_preflight_can_prove_only_conservative_fallback() {
        assert_eq!(accepted(&json!({}), &json!({})).unwrap(), None);
        assert!(accepted(&json!({}), &json!({"source_route_context":{"version":1}})).is_err());
    }

    #[test]
    fn copied_source_decision_reports_audio_encoding_without_claiming_video_encoding() {
        for video in [StreamAction::Copy, StreamAction::Encode] {
            for audio in [StreamAction::Copy, StreamAction::Encode] {
                for decision in ["direct_play", "direct_stream", "transcode"] {
                    let expected = video == StreamAction::Copy
                        && matches!(
                            (audio, decision),
                            (StreamAction::Copy, "direct_stream")
                                | (StreamAction::Encode, "transcode")
                        );
                    assert_eq!(
                        copy_decision_matches(decision, StreamPlan { video, audio }),
                        expected,
                        "decision={decision},video={video:?},audio={audio:?}"
                    );
                }
            }
        }
    }
}
