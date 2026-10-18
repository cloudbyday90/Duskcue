// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::{Path, PathBuf};

use axum::Router;
use axum::http::StatusCode;
use duskcue::services::transcoding::plan::StreamAction;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Evidence, ProgressiveFixture, media, process};
pub(super) async fn clean_source(
    tools: &process::FixtureProcesses,
    original: &Path,
    root: &Path,
) -> anyhow::Result<PathBuf> {
    let path = root.join("clean.mp4");
    process::original(
        tools,
        "ffmpeg",
        &[
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-nostdin".into(),
            "-i".into(),
            original.to_string_lossy().into_owned(),
            "-map".into(),
            "0:0".into(),
            "-map".into(),
            "0:1".into(),
            "-map".into(),
            "0:2".into(),
            "-c:a".into(),
            "copy".into(),
            "-disposition:a:0".into(),
            "0".into(),
            "-disposition:a:1".into(),
            "default+visual_impaired".into(),
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            "veryfast".into(),
            "-profile:v".into(),
            "high".into(),
            "-level:v".into(),
            "4.0".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            "-vf".into(),
            "setparams=color_primaries=bt709:color_trc=bt709:colorspace=bt709".into(),
            "-color_primaries".into(),
            "bt709".into(),
            "-color_trc".into(),
            "bt709".into(),
            "-colorspace".into(),
            "bt709".into(),
            "-b:v".into(),
            "400000".into(),
            "-minrate".into(),
            "400000".into(),
            "-maxrate".into(),
            "400000".into(),
            "-bufsize".into(),
            "800000".into(),
            "-x264-params".into(),
            "nal-hrd=cbr:force-cfr=1".into(),
            "-g".into(),
            "48".into(),
            "-keyint_min".into(),
            "48".into(),
            "-sc_threshold".into(),
            "0".into(),
            "-threads".into(),
            "1".into(),
            "-filter_threads".into(),
            "1".into(),
            "-t".into(),
            "8".into(),
            "-fs".into(),
            "16777216".into(),
            "-movflags".into(),
            "+faststart".into(),
            "-n".into(),
            path.to_string_lossy().into_owned(),
        ],
    )
    .await?;
    Ok(path)
}

async fn probe(tools: &process::FixtureProcesses, path: &Path) -> anyhow::Result<Value> {
    Ok(serde_json::from_slice(
        &process::original(
            tools,
            "ffprobe",
            &[
                "-v".into(),
                "error".into(),
                "-show_streams".into(),
                "-show_format".into(),
                "-of".into(),
                "json".into(),
                path.to_string_lossy().into_owned(),
            ],
        )
        .await?,
    )?)
}

pub(super) async fn seed(
    fixture: &ProgressiveFixture<'_>,
    evidence: &Evidence,
    source: &Path,
) -> anyhow::Result<()> {
    seed_source(
        fixture,
        &evidence.tools,
        source,
        evidence.item,
        evidence.file,
        8,
    )
    .await
}

pub(super) async fn seed_source(
    fixture: &ProgressiveFixture<'_>,
    tools: &process::FixtureProcesses,
    source: &Path,
    item: Uuid,
    file: Uuid,
    runtime: i32,
) -> anyhow::Result<()> {
    anyhow::ensure!(matches!(runtime, 8 | 60));
    let inspected = probe(tools, source).await?;
    let streams = inspected["streams"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("clean fixture streams unavailable"))?;
    let seconds = inspected["format"]["duration"]
        .as_str()
        .unwrap_or("")
        .parse::<f64>()?;
    anyhow::ensure!(
        streams.len() == 3
            && streams[0]["codec_type"] == "audio"
            && streams[1]["codec_name"] == "h264"
            && streams[2]["codec_type"] == "audio"
            && streams[0]["disposition"]["default"] == 0
            && streams[2]["disposition"]["default"] == 1
            && streams[1]["width"] == 320
            && streams[1]["height"] == 180
            && streams[1]["profile"] == "High"
            && streams[1]["level"] == 40
            && streams[1]["pix_fmt"] == "yuv420p"
            && streams[1]["color_transfer"] == "bt709"
            && streams[1]["r_frame_rate"] == "24/1"
            && seconds.is_finite()
            && (seconds - f64::from(runtime)).abs() < 0.25,
        "clean direct fixture differs from its required source envelope: {inspected}"
    );
    let video_bitrate = streams[1]["bit_rate"]
        .as_str()
        .unwrap_or("")
        .parse::<i32>()?;
    let audio_bitrate = streams[2]["bit_rate"]
        .as_str()
        .unwrap_or("")
        .parse::<i32>()?;
    anyhow::ensure!(
        (100_000..=500_000).contains(&video_bitrate) && (32_000..=192_000).contains(&audio_bitrate)
    );
    let mut audio = Vec::new();
    for index in [0, 2] {
        let mut stream = streams[index].clone();
        stream["bitrate"] = json!(stream["bit_rate"].as_str().unwrap_or("").parse::<u32>()?);
        audio.push(stream);
    }
    let mut video = streams[1].clone();
    video["bit_depth"] = json!(8);
    video["level"] = json!(4.0);
    video["frame_rate"] = json!(24.0);
    let additional = json!({"video":video,"audio":audio});
    let bytes = tokio::fs::metadata(source).await?.len();
    anyhow::ensure!(bytes > 0 && bytes <= 16 * 1024 * 1024);
    sqlx::query("INSERT INTO media_items(id,library_id,type,title,sort_title,content_rating,runtime_seconds) VALUES ($1,$2,'movie','Direct fallback fixture','Direct fallback fixture','R',$3)")
        .bind(item).bind(fixture.library).bind(runtime).execute(fixture.pool).await?;
    sqlx::query("INSERT INTO media_files(id,media_item_id,file_path,file_size,container_format,runtime_seconds,video_codec,video_resolution,video_bitrate,video_dynamic_range,audio_codec,audio_channels,audio_bitrate,additional_streams) VALUES($1,$2,$3,$4,'mp4',$5,'h264','320x180',$6,'sdr','aac',1,$7,$8)")
        .bind(file).bind(item).bind(source.to_string_lossy().as_ref()).bind(i64::try_from(bytes)?)
        .bind(runtime).bind(video_bitrate).bind(audio_bitrate).bind(additional).execute(fixture.pool).await?;
    Ok(())
}

pub(super) async fn output_media(
    app: &Router,
    token: &str,
    generation: Uuid,
    evidence: &Evidence,
) -> anyhow::Result<Value> {
    let (status, playlist) = media::playlist(app, token, generation).await?;
    anyhow::ensure!(status == StatusCode::OK);
    let segments = media::segments(&playlist)?;
    let init = playlist
        .lines()
        .find_map(|line| {
            line.strip_prefix("#EXT-X-MAP:URI=\"")
                .and_then(|uri| uri.strip_suffix('"'))
        })
        .ok_or_else(|| anyhow::anyhow!("direct replacement initialization unavailable"))?;
    let (status, initialization) = media::fetch(app, token, init).await?;
    anyhow::ensure!(status == StatusCode::OK);
    let (status, fragment) = media::fetch(app, token, &segments[0]).await?;
    anyhow::ensure!(status == StatusCode::OK);
    decode_fragment(
        &evidence.tools,
        &evidence.root,
        &media::FragmentContext {
            name: "direct-output".into(),
            source_seek_ms: Some(2000),
            source_start_ms: 2000,
            stream_plan: duskcue::services::transcoding::plan::StreamPlan {
                video: StreamAction::Encode,
                audio: StreamAction::Encode,
            },
        },
        &initialization,
        &fragment,
        30,
    )
    .await
}

pub(super) async fn decode_fragment(
    tools: &process::FixtureProcesses,
    root: &Path,
    context: &media::FragmentContext,
    initialization: &[u8],
    fragment: &[u8],
    expected_frame_rate: u32,
) -> anyhow::Result<Value> {
    let timestamp = media::decode_start(tools, root, context, initialization, fragment).await?;
    let path = root.join(format!("{}.mp4", context.name));
    let inspected = probe(tools, &path).await?;
    let streams = inspected["streams"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("encoded streams unavailable"))?;
    let video = streams
        .iter()
        .find(|stream| stream["codec_type"] == "video")
        .ok_or_else(|| anyhow::anyhow!("encoded video unavailable"))?;
    let audio = streams
        .iter()
        .find(|stream| stream["codec_type"] == "audio")
        .ok_or_else(|| anyhow::anyhow!("encoded audio unavailable"))?;
    anyhow::ensure!(
        streams.len() == 2
            && video["codec_name"] == "h264"
            && video["profile"] == "High"
            && video["level"] == 40
            && video["pix_fmt"] == "yuv420p"
            && video["r_frame_rate"] == format!("{expected_frame_rate}/1")
            && video["width"] == 320
            && video["height"] == 180
            && audio["codec_name"] == "aac"
            && audio["profile"] == "LC"
            && audio["sample_rate"] == "48000"
            && audio["channels"] == 1,
        "actual encoded fragment differs from the capability descriptor: {inspected}"
    );
    let start_time = |stream: &Value, name: &str| -> anyhow::Result<f64> {
        let value = stream["start_time"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{name} stream start timestamp unavailable"))?
            .parse::<f64>()?;
        anyhow::ensure!(
            value.is_finite() && (0.0..=0.25).contains(&value),
            "{name} stream start timestamp {value:?} is outside [0,0.25] seconds for {} at source offset {}ms",
            context.name,
            context.source_start_ms
        );
        Ok(value)
    };
    let video_start_seconds = start_time(video, "video")?;
    let audio_start_seconds = start_time(audio, "audio")?;
    let av_start_delta_seconds = video_start_seconds - audio_start_seconds;
    anyhow::ensure!(
        av_start_delta_seconds.abs() <= 0.1,
        "muxed stream start timestamp alignment exceeds 0.1 seconds: video={video_start_seconds:.6}, audio={audio_start_seconds:.6}, delta={av_start_delta_seconds:.6} for {} at source offset {}ms",
        context.name,
        context.source_start_ms
    );
    let pcm = process::original(
        tools,
        "ffmpeg",
        &[
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-nostdin".into(),
            "-i".into(),
            path.to_string_lossy().into_owned(),
            "-t".into(),
            "0.75".into(),
            "-map".into(),
            "0:a:0".into(),
            "-vn".into(),
            "-ac".into(),
            "1".into(),
            "-ar".into(),
            "8000".into(),
            "-c:a".into(),
            "pcm_s16le".into(),
            "-threads".into(),
            "1".into(),
            "-f".into(),
            "s16le".into(),
            "pipe:1".into(),
        ],
    )
    .await?;
    let samples = pcm
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
        .skip(500)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        samples.len() >= 4000 && samples.iter().any(|value| value.unsigned_abs() > 500)
    );
    let crossings = samples
        .windows(2)
        .filter(|pair| (pair[0] < 0) != (pair[1] < 0))
        .count();
    let frequency = crossings as f64 * 8000.0 / (2.0 * samples.len() as f64);
    anyhow::ensure!(
        (frequency - 880.0).abs() < 20.0,
        "promoted output selected {frequency:.3}Hz audio instead of 880Hz"
    );
    Ok(
        json!({"decoded_timestamp":timestamp,"selected_audio_hz":frequency,"video_profile":video["profile"],"video_level":video["level"],"video_frame_rate":video["r_frame_rate"],"audio_sample_rate":audio["sample_rate"],"audio_channels":audio["channels"],"video_start_seconds":video_start_seconds,"audio_start_seconds":audio_start_seconds,"av_start_delta_seconds":av_start_delta_seconds,"timestamp_alignment":true}),
    )
}

pub(super) async fn file_hash(path: &Path) -> anyhow::Result<String> {
    let metadata = tokio::fs::symlink_metadata(path).await?;
    anyhow::ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() > 0
            && metadata.len() <= 16 * 1024 * 1024
    );
    Ok(
        ring::digest::digest(&ring::digest::SHA256, &tokio::fs::read(path).await?)
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}
