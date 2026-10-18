// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use duskcue::services::transcoding::plan::StreamAction;
use duskcue::services::transcoding::{HwAccelMethod, OutputProfile};
use serde_json::{Value, json};
use uuid::Uuid;

use super::policy::{self, ORIGIN};
use super::{AppState, ProgressiveFixture, process};

pub(super) async fn run(
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    root: &Path,
    tools: &process::FixtureProcesses,
    observed: &Value,
) -> anyhow::Result<Value> {
    let resource = std::env::var("DUSKCUE_TEST_RESOURCE_ID")?;
    anyhow::ensure!(observed["resourceId"] == resource);
    let phase = observed["phase"]
        .as_str()
        .filter(|phase| matches!(*phase, "browser" | "native"))
        .ok_or_else(|| anyhow::anyhow!("invalid live verification phase"))?;
    let play = uuid(observed, "sessionId")?;
    let generation = uuid(observed, "transcodeId")?;
    let previous = uuid(observed, "previousTranscodeId")?;
    let position = observed["positionMs"]
        .as_i64()
        .filter(|position| (4000..=6000).contains(position))
        .ok_or_else(|| {
            anyhow::anyhow!("live verification seek is outside the healthy source tail")
        })?;
    anyhow::ensure!(generation != previous && observed["paused"] == true);
    let row: (Uuid, Uuid, Uuid, String, Value) = sqlx::query_as(
        "SELECT p.user_id,p.profile_id,p.media_item_id,p.stream_decision,p.metadata \
         FROM play_sessions p JOIN media_items i ON i.id=p.media_item_id \
         WHERE p.id=$1 AND p.stopped_at IS NULL AND i.library_id=$2",
    )
    .bind(play)
    .bind(fixture.library)
    .fetch_one(fixture.pool)
    .await?;
    anyhow::ensure!(row.0 == fixture.user && row.1 == fixture.original_profile);
    let metadata = &row.4;
    anyhow::ensure!(
        metadata["transcode_session_id"] == generation.to_string()
            && metadata["stream_offset_ms"] == position
            && metadata["current_position_ms"]
                .as_i64()
                .is_some_and(|checkpoint| checkpoint >= position)
    );
    let session = state
        .transcode_manager
        .get_session(&generation)
        .ok_or_else(|| anyhow::anyhow!("live post-seek generation unavailable"))?;
    let source_file = uuid(metadata, "media_file_id")?;
    let source_path: String = sqlx::query_scalar(
        "SELECT file_path FROM media_files WHERE id=$1 AND media_item_id=$2 AND is_healthy=true",
    )
    .bind(source_file)
    .bind(row.2)
    .fetch_one(fixture.pool)
    .await?;
    anyhow::ensure!(
        session.user_id == fixture.user
            && session.media_file_id == source_file
            && session.source_path == Path::new(&source_path)
            && session.source_start_ms == position
            && session.source_video_stream_index == Some(1)
            && session.source_audio_stream_index == Some(2)
            && session.source_path.canonicalize()?.starts_with(root)
            && session.segment_dir.canonicalize()?.starts_with(root)
            && session.target_video_codec == "h264"
            && session.target_audio_codec == "aac"
            && session.target_video_resolution == (320, 180)
            && session.hw_accel == HwAccelMethod::Software
    );
    let old_directory = session
        .segment_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("live generation parent unavailable"))?
        .join(previous.to_string());
    anyhow::ensure!(
        state.transcode_manager.get_session(&previous).is_none() && !old_directory.exists(),
        "seek did not retire its previous physical generation/cache"
    );
    let attempts = metadata["recovery_budget"]["attempts"]
        .as_u64()
        .unwrap_or(0);
    let frame_rate;
    if phase == "browser" {
        let descriptor = &metadata["direct_source_context"]["candidate"]["descriptor"];
        anyhow::ensure!(
            row.3 == "transcode"
                && attempts == 1
                && session.recovery_attempts == 1
                && session.output_profile == Some(OutputProfile::CompatibleSdr)
                && session.stream_plan.video == StreamAction::Encode
                && session.stream_plan.audio == StreamAction::Encode
                && session.target_bitrate
                    == descriptor["video"]["bitrate"].as_u64().unwrap_or(0) as u32
                && session.target_audio_bitrate
                    == descriptor["audio"]["bitrate"]
                        .as_u64()
                        .and_then(|bitrate| u32::try_from(bitrate).ok())
                && session.target_audio_channels
                    == descriptor["audio"]["channels"]
                        .as_str()
                        .and_then(|channels| channels.parse::<u32>().ok())
                && metadata["recovery"]["reason"] == "native_decode"
                && metadata["recovery"]["attempts"] == 1
        );
        frame_rate = 30;
    } else {
        anyhow::ensure!(
            attempts == 0
                && session.recovery_attempts == 0
                && session.output_profile.is_none()
                && session.stream_plan.video == StreamAction::Copy
                && session.stream_plan.audio == StreamAction::Copy,
            "ordinary native HLS seek changed its admitted plan/profile or spent recovery budget"
        );
        frame_rate = 24;
    }
    let target: String = sqlx::query_scalar(
        "SELECT details->>'target_position_ms' FROM play_events WHERE play_session_id=$1 \
         AND event_type='seek' ORDER BY created_at DESC,id DESC LIMIT 1",
    )
    .bind(play)
    .fetch_one(fixture.pool)
    .await?;
    anyhow::ensure!(target.parse::<i64>()? == position);
    let client = policy::client()?;
    let headers = headers(fixture.token, phase)?;
    let playlist = String::from_utf8(
        fetch(
            &client,
            &format!("/api/v1/transcode/{generation}/manifest.m3u8"),
            &headers,
        )
        .await?,
    )?;
    let segments = super::super::media::segments(&playlist)?;
    let initialization_uri = playlist
        .lines()
        .find_map(|line| {
            line.strip_prefix("#EXT-X-MAP:URI=\"")
                .and_then(|uri| uri.strip_suffix('"'))
        })
        .ok_or_else(|| anyhow::anyhow!("post-seek initialization URI unavailable"))?;
    let prefix = format!("/api/v1/transcode/{generation}/{}/", session.rendition_name);
    anyhow::ensure!(
        initialization_uri == format!("{prefix}init.mp4")
            && segments.iter().all(|uri| uri.starts_with(&prefix))
    );
    let initialization = fetch(&client, initialization_uri, &headers).await?;
    let fragment = fetch(&client, &segments[0], &headers).await?;
    let media = super::super::direct_media::decode_fragment(
        tools,
        root,
        &super::super::media::FragmentContext {
            name: format!("direct-{phase}-post-seek"),
            source_seek_ms: Some(position),
            source_start_ms: position,
            stream_plan: session.stream_plan,
        },
        &initialization,
        &fragment,
        frame_rate,
    )
    .await?;
    let denied = negative_generation_access(fixture, &client, generation).await?;
    Ok(json!({
        "same_session":true,"old_generation_retired":true,"source_offset_ms":position,
        "selected_video_index":1,"selected_audio_index":2,"attempts":attempts,
        "output_profile":if phase == "browser" { "compatible_sdr" } else { "ordinary_copy" },
        "transport":if phase == "browser" { "cookie" } else { "bearer" },
        "actual_media":media,"authorization":denied
    }))
}

fn uuid(value: &Value, field: &str) -> anyhow::Result<Uuid> {
    Ok(value[field]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("live verification UUID unavailable: {field}"))?
        .parse()?)
}

fn headers(token: &str, phase: &str) -> anyhow::Result<reqwest::header::HeaderMap> {
    let mut headers = reqwest::header::HeaderMap::new();
    let (name, value) = if phase == "browser" {
        (reqwest::header::COOKIE, format!("session={token}"))
    } else {
        (reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
    };
    headers.insert(name, value.parse()?);
    Ok(headers)
}

async fn fetch(
    client: &reqwest::Client,
    path: &str,
    headers: &reqwest::header::HeaderMap,
) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        path.starts_with("/api/v1/transcode/")
            && !path
                .chars()
                .any(|character| matches!(character, '?' | '#' | '\\'))
    );
    let response = client
        .get(format!("{ORIGIN}{path}"))
        .headers(headers.clone())
        .send()
        .await?;
    anyhow::ensure!(response.status() == reqwest::StatusCode::OK);
    let bytes = response.bytes().await?;
    anyhow::ensure!(!bytes.is_empty() && bytes.len() <= 8 * 1024 * 1024);
    Ok(bytes.to_vec())
}

async fn negative_generation_access(
    fixture: &ProgressiveFixture<'_>,
    client: &reqwest::Client,
    generation: Uuid,
) -> anyhow::Result<Value> {
    let url = format!("{ORIGIN}/api/v1/transcode/{generation}/manifest.m3u8");
    let unauthenticated = client.get(&url).send().await?;
    let peer = client
        .get(&url)
        .header("authorization", format!("Bearer {}", fixture.peer_token))
        .send()
        .await?;
    anyhow::ensure!(
        unauthenticated.status() == reqwest::StatusCode::UNAUTHORIZED
            && peer.status() == reqwest::StatusCode::NOT_FOUND
    );
    let session = Uuid::now_v7();
    let token = duskcue::domains::auth::service::generate_session_token();
    let hash = ring::digest::digest(&ring::digest::SHA256, token.as_bytes())
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    sqlx::query("INSERT INTO user_sessions(id,user_id,active_profile_id,token_hash,expires_at) VALUES($1,$2,$3,$4,now()+interval '5 minutes')")
        .bind(session).bind(fixture.user).bind(fixture.kids_profile).bind(hash).execute(fixture.pool).await?;
    let kids = client
        .get(&url)
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await;
    let cleanup = sqlx::query("DELETE FROM user_sessions WHERE id=$1 AND user_id=$2")
        .bind(session)
        .bind(fixture.user)
        .execute(fixture.pool)
        .await;
    let kids = kids?;
    cleanup?;
    anyhow::ensure!(kids.status() == reqwest::StatusCode::NOT_FOUND);
    Ok(json!({"unauthenticated_status":401,"peer_status":404,"kids_status":404}))
}
