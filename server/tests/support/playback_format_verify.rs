// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use duskcue::services::transcoding::{HwAccelMethod, plan::StreamAction};
use serde_json::{Value, json};
use uuid::Uuid;

use super::super::policy::{self, ORIGIN};
use super::{AppState, Inventory, ProgressiveFixture, inventory, media, process};

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
    );
    anyhow::ensure!(inventory::hash(&case.path).await? == case.source["sha256"]);
    let client = policy::client()?;
    let headers = headers(fixture.token, client_kind)?;
    if case.hdr() {
        let (status, code) = super::hdr_refusal();
        anyhow::ensure!(observed["status"] == status && observed["code"] == code);
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM play_sessions WHERE media_item_id=$1")
                .bind(case.item)
                .fetch_one(fixture.pool)
                .await?;
        anyhow::ensure!(count == 0 && state.transcode_manager.active_session_count() == 0);
        let response = client.post(format!("{ORIGIN}/api/v1/playback/start")).headers(headers).json(&json!({"media_item_id":case.item,"media_file_id":case.file,"force_hls":client_kind=="native","direct_recovery_protocol":1})).send().await?;
        anyhow::ensure!(response.status().as_u16() == status);
        let body: Value = response.json().await?;
        anyhow::ensure!(body["title"] == code);
        let after: i64 =
            sqlx::query_scalar("SELECT count(*) FROM play_sessions WHERE media_item_id=$1")
                .bind(case.item)
                .fetch_one(fixture.pool)
                .await?;
        anyhow::ensure!(after == 0 && state.transcode_manager.active_session_count() == 0);
        let denied = negatives(
            &client,
            &format!("/api/v1/stream/{}", case.file),
            &inventory.peer_token,
            &inventory.kids_token,
        )
        .await?;
        return Ok(
            json!({"case_id":case.source["id"],"index":index,"outcome":"hdr_refused","status":status,"code":code,"no_session_allocated":true,"no_worker_allocated":true,"sources_unchanged":true,"authorization":denied}),
        );
    }
    let play = uuid(observed, "sessionId")?;
    let generation = uuid(observed, "transcodeId")?;
    let previous = uuid(observed, "previousTranscodeId")?;
    anyhow::ensure!(
        previous != generation
            && observed["positionMs"] == 5000
            && observed["paused"] == true
            && observed["attempts"] == 0
    );
    let row: (Uuid,Uuid,Uuid,String,Value) = sqlx::query_as("SELECT user_id,profile_id,media_item_id,stream_decision,metadata FROM play_sessions WHERE id=$1 AND library_id=$2 AND stopped_at IS NULL")
        .bind(play).bind(inventory.library).fetch_one(fixture.pool).await?;
    anyhow::ensure!(
        row.0 == fixture.user
            && row.1 == fixture.original_profile
            && row.2 == case.item
            && row.3 == "transcode"
            && row.4["media_file_id"] == case.file.to_string()
            && row.4["transcode_session_id"] == generation.to_string()
            && row.4["stream_offset_ms"] == 5000
            && row.4["current_position_ms"]
                .as_i64()
                .is_some_and(|position| position >= 5000 && position <= 8000)
            && row.4["recovery_budget"]["attempts"].as_u64().unwrap_or(0) == 0
    );
    let session = state
        .transcode_manager
        .get_session(&generation)
        .ok_or_else(|| anyhow::anyhow!("format post-seek generation unavailable"))?;
    anyhow::ensure!(
        session.user_id == fixture.user
            && session.media_file_id == case.file
            && session.source_path == case.path
            && case.source["codec"].as_str() == Some(session.source_video_codec.as_str())
            && session.source_audio_codec == "opus"
            && session.source_video_stream_index == Some(0)
            && session.source_audio_stream_index == Some(2)
            && session.stream_plan.video == StreamAction::Encode
            && session.stream_plan.audio == StreamAction::Encode
            && session.output_profile.is_none()
            && session.target_video_codec == "h264"
            && session.target_audio_codec == "aac"
            && session.target_video_resolution == (320, 180)
            && session.source_start_ms == 5000
            && session.recovery_attempts == 0
            && session.hw_accel == HwAccelMethod::Software
            && session.segment_dir.canonicalize()?.starts_with(root)
    );
    let previous_cache = session
        .segment_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("format generation parent missing"))?
        .join(previous.to_string());
    anyhow::ensure!(
        state.transcode_manager.get_session(&previous).is_none() && !previous_cache.exists()
    );
    let seek: i64 = sqlx::query_scalar("SELECT (details->>'target_position_ms')::bigint FROM play_events WHERE play_session_id=$1 AND event_type='seek' ORDER BY created_at DESC,id DESC LIMIT 1").bind(play).fetch_one(fixture.pool).await?;
    anyhow::ensure!(seek == 5000);
    let playlist = String::from_utf8(
        fetch(
            &client,
            &format!("/api/v1/transcode/{generation}/manifest.m3u8"),
            &headers,
        )
        .await?,
    )?;
    let segments = super::super::super::media::segments(&playlist)?;
    let init = playlist
        .lines()
        .find_map(|line| {
            line.strip_prefix("#EXT-X-MAP:URI=\"")
                .and_then(|value| value.strip_suffix('"'))
        })
        .ok_or_else(|| anyhow::anyhow!("format initialization missing"))?;
    let prefix = format!("/api/v1/transcode/{generation}/{}/", session.rendition_name);
    anyhow::ensure!(
        init == format!("{prefix}init.mp4")
            && segments.iter().all(|path| path.starts_with(&prefix))
    );
    let initialization = fetch(&client, init, &headers).await?;
    let fragment = fetch(&client, &segments[0], &headers).await?;
    let decoded = media::decode(
        tools,
        root,
        &format!("format-{client_kind}-{index:02}"),
        &session,
        &initialization,
        &fragment,
    )
    .await?;
    let denied = negatives(
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
        json!({"case_id":case.source["id"],"index":index,"outcome":"sdr_fallback","same_session":true,"old_generation_retired":true,"source_offset_ms":5000,"selected_video_index":0,"selected_audio_index":2,"attempts":0,"output_profile":"ordinary_sdr","sources_unchanged":true,"authorization":denied,"actual_media":decoded["actual_media"],"capability":decoded["capability"],"bitrate_source":decoded["bitrate_source"]}),
    )
}

fn uuid(value: &Value, key: &str) -> anyhow::Result<Uuid> {
    Ok(value[key]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("format request identity missing"))?
        .parse()?)
}

fn headers(token: &str, client: &str) -> anyhow::Result<reqwest::header::HeaderMap> {
    let mut headers = reqwest::header::HeaderMap::new();
    if client == "browser" {
        headers.insert(reqwest::header::COOKIE, format!("session={token}").parse()?);
    } else {
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {token}").parse()?,
        );
    }
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

async fn negatives(
    client: &reqwest::Client,
    path: &str,
    peer: &str,
    kids: &str,
) -> anyhow::Result<Value> {
    let url = format!("{ORIGIN}{path}");
    let anonymous = client.get(&url).send().await?;
    let peer = client.get(&url).bearer_auth(peer).send().await?;
    let kids = client.get(&url).bearer_auth(kids).send().await?;
    anyhow::ensure!(
        anonymous.status().as_u16() == 401
            && peer.status().as_u16() == 404
            && kids.status().as_u16() == 404
    );
    Ok(json!({"unauthenticated_status":401,"peer_status":404,"kids_status":404}))
}
