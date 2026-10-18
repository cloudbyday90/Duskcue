// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::net::SocketAddr;
use std::time::Duration;

use serde_json::json;
use uuid::Uuid;

use super::{AppState, Path, ProgressiveFixture, Router, Value, process};

pub(super) async fn run(
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    app: &Router,
    root: &Path,
    source: &Path,
    tools: &process::FixtureProcesses,
) -> anyhow::Result<Value> {
    let resource = Uuid::parse_str(&std::env::var("DUSKCUE_TEST_RESOURCE_ID")?)?;
    anyhow::ensure!(std::env::var("DUSKCUE_TEST_DIRECT_BROWSER").as_deref() == Ok("live"));
    let original = tokio::fs::read(source).await?;
    anyhow::ensure!(!original.is_empty() && original.len() <= 8 * 1024 * 1024);
    let original_hash = blake3::hash(&original);
    let probe: Value = serde_json::from_slice(
        &process::original(
            tools,
            "ffprobe",
            &[
                "-v".into(),
                "error".into(),
                "-show_packets".into(),
                "-show_streams".into(),
                "-of".into(),
                "json".into(),
                source.to_string_lossy().into_owned(),
            ],
        )
        .await?,
    )?;
    let streams = probe["streams"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("direct streams missing"))?;
    let video = streams
        .iter()
        .find(|stream| stream["codec_type"] == "video")
        .ok_or_else(|| anyhow::anyhow!("direct video missing"))?;
    anyhow::ensure!(video["index"] == 1 && video["codec_name"] == "h264");
    let mut normalized_video = video.clone();
    normalized_video["bit_depth"] = json!(8);
    normalized_video["level"] = json!(4.0);
    normalized_video["frame_rate"] = json!(24.0);
    let video_bitrate = video["bit_rate"].as_str().unwrap_or("").parse::<i32>()?;
    let mut audio = streams
        .iter()
        .filter(|stream| stream["codec_type"] == "audio")
        .cloned()
        .collect::<Vec<_>>();
    anyhow::ensure!(audio.len() == 2 && audio[1]["index"] == 2);
    for stream in &mut audio {
        stream["bitrate"] = json!(stream["bit_rate"].as_str().unwrap_or("").parse::<u32>()?);
    }
    let audio_bitrate = audio[1]["bitrate"].as_u64().unwrap_or(0);
    anyhow::ensure!(
        (100_000..=500_000).contains(&video_bitrate) && (32_000..=192_000).contains(&audio_bitrate)
    );
    let packets = probe["packets"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("direct packets missing"))?;
    let mut candidates = Vec::new();
    let mut items = Vec::new();
    let mut files = Vec::new();
    let mut server = None;
    let mut shutdown = None;
    let proof = async {
    for (variant, kind) in [
        ("video-length", 0),
        ("video-slice", 1),
        ("audio-payload", 2),
        ("video-empty", 3),
    ] {
        let mut bytes = original.clone();
        let mut altered = 0;
        for packet in packets {
            let time = packet["pts_time"]
                .as_str()
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(-1.0);
            if !(2.0..4.0).contains(&time)
                || packet["stream_index"] != if kind == 2 { 2 } else { 1 }
            {
                continue;
            }
            let start = packet["pos"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("packet position missing"))?
                .parse::<usize>()?;
            let length = packet["size"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("packet size missing"))?
                .parse::<usize>()?;
            let end = start
                .checked_add(length)
                .filter(|end| *end <= bytes.len())
                .ok_or_else(|| anyhow::anyhow!("packet outside fixture"))?;
            anyhow::ensure!(length >= 8);
            match kind {
                0 => bytes[start..start + 4].copy_from_slice(&0x7fff_ffffu32.to_be_bytes()),
                1 => bytes[start + 4..end].fill(0xff),
                2 => bytes[start..end].fill(0xff),
                _ => bytes[start..start + 4].copy_from_slice(&0u32.to_be_bytes()),
            }
            altered += 1;
        }
        anyhow::ensure!(altered > 1);
        let path = root.join(format!("direct-browser-{variant}.mp4"));
        tokio::fs::write(&path, &bytes).await?;
        let item = Uuid::now_v7();
        let file = Uuid::now_v7();
        items.push(item);
        sqlx::query("INSERT INTO media_items(id,library_id,type,title,sort_title,content_rating,runtime_seconds) VALUES ($1,$2,'movie','Direct browser fixture','Direct browser fixture','R',8)")
            .bind(item).bind(fixture.library).execute(fixture.pool).await?;
            sqlx::query("INSERT INTO media_files(id,media_item_id,file_path,file_size,container_format,video_codec,video_resolution,video_bitrate,video_dynamic_range,audio_codec,audio_channels,audio_bitrate,runtime_seconds,additional_streams) VALUES ($1,$2,$3,$4,'mp4','h264','320x180',$5,'sdr','aac',1,$6,8,$7)")
            .bind(file).bind(item).bind(path.to_string_lossy().as_ref()).bind(i64::try_from(bytes.len())?)
                .bind(video_bitrate).bind(i32::try_from(audio_bitrate)?)
            .bind(json!({"video":normalized_video,"audio":audio,"subtitles":[]})).execute(fixture.pool).await?;
        let sha256 = ring::digest::digest(&ring::digest::SHA256, &bytes)
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        candidates.push(json!({"item":item,"file":file,"variant":variant,"sha256":sha256}));
        files.push((path, blake3::hash(&bytes)));
    }
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 38081))).await?;
    let (signal, receiver) = tokio::sync::oneshot::channel::<()>();
    shutdown = Some(signal);
    let router = app.clone();
    server = Some(tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            let _ = receiver.await;
        })
        .await
    }));
        let ready = json!({"resourceId":resource,"token":fixture.token,"userId":fixture.user,"profileId":fixture.original_profile,"candidates":candidates});
        let path = Path::new("/tmp/direct-ready.pending");
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        use tokio::io::AsyncWriteExt;
        let mut file = options.open(path).await?;
        file.write_all(serde_json::to_string(&ready)?.as_bytes()).await?;
        file.flush().await?;
        drop(file);
        tokio::fs::rename(path, "/tmp/direct-ready.json").await?;
        let complete = tokio::time::timeout(Duration::from_secs(100), async {
            loop {
                match tokio::fs::read("/tmp/direct-complete.json").await {
                    Ok(bytes) => {
                        anyhow::ensure!(bytes.len() <= 65536);
                        break Ok::<Value, anyhow::Error>(serde_json::from_slice(&bytes)?);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => tokio::time::sleep(Duration::from_millis(100)).await,
                    Err(error) => break Err(error.into()),
                }
            }
        }).await??;
        anyhow::ensure!(complete["resourceId"] == resource.to_string());
        anyhow::ensure!(complete["passed"] == true, "real browser qualification failed: {}", complete.get("error").unwrap_or(&Value::Null));
        anyhow::ensure!(complete["nativeError"]["code"] == 3 && complete["nativeError"]["trusted"] == true);
        anyhow::ensure!(complete["directFrames"].as_u64().unwrap_or(0) > 0 && complete["hlsFrames"].as_u64().unwrap_or(0) >= 5);
        anyhow::ensure!(complete["selectedAudioHz"].as_f64().is_some_and(|frequency| (frequency - 880.0).abs() < 20.0));
        anyhow::ensure!(complete["selectedAudioDb"].as_f64().is_some_and(|level| level > -60.0) && complete["hlsElapsedMs"].as_f64().is_some_and(|elapsed| elapsed >= 500.0));
        anyhow::ensure!(complete["recovery"]["attempts"] == 1 && complete["recovery"]["quality_reduced"] == true);
        anyhow::ensure!(blake3::hash(&tokio::fs::read(source).await?) == original_hash);
        for (path, expected) in &files { anyhow::ensure!(blake3::hash(&tokio::fs::read(path).await?) == *expected); }
        Ok::<_, anyhow::Error>(complete)
    }.await;
    if let Some(signal) = shutdown {
        let _ = signal.send(());
    }
    let mut cleanup_errors = Vec::new();
    if let Some(mut server) = server {
        match tokio::time::timeout(Duration::from_secs(5), &mut server).await {
            Ok(Ok(Ok(()))) => {}
            stopped => {
                cleanup_errors.push(format!("owned HTTP server shutdown failed: {stopped:?}"));
                server.abort();
                let _ = server.await;
            }
        }
    }
    let sessions: Vec<(Uuid, Value)> = sqlx::query_as(
        "SELECT id,metadata FROM play_sessions WHERE media_item_id=ANY($1) AND stopped_at IS NULL",
    )
    .bind(&items)
    .fetch_all(fixture.pool)
    .await?;
    for (id, metadata) in sessions {
        if let Err(error) = duskcue::domains::playback::service::stop_playback(
            fixture.pool,
            &state.transcode_manager,
            fixture.user,
            id,
            None,
            false,
        )
        .await
        {
            cleanup_errors.push(error.to_string());
        }
        if let Some(id) = metadata["transcode_session_id"]
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok())
        {
            if state.transcode_manager.get_session(&id).is_some() {
                if let Err(error) = state.transcode_manager.stop_session(id).await {
                    cleanup_errors.push(error.to_string());
                }
            }
        }
    }
    if let Err(error) = sqlx::query("DELETE FROM media_items WHERE id=ANY($1)")
        .bind(&items)
        .execute(fixture.pool)
        .await
    {
        cleanup_errors.push(error.to_string());
    }
    anyhow::ensure!(
        cleanup_errors.is_empty(),
        "browser cleanup failed: {}",
        cleanup_errors.join("; ")
    );
    proof
}
