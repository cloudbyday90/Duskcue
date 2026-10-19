// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use duskcue::services::transcoding::{
    HwAccelMethod, OutputProfile, StartSessionParams,
    plan::{StreamAction, StreamPlan},
};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use super::{AppState, ProgressiveFixture, inventory, process, source_media};

struct Atom<'a> {
    kind: [u8; 4],
    payload: &'a [u8],
}

fn atoms(mut bytes: &[u8]) -> anyhow::Result<Vec<Atom<'_>>> {
    let mut result = Vec::new();
    while !bytes.is_empty() {
        anyhow::ensure!(bytes.len() >= 8 && result.len() < 256);
        let small = number(bytes, 0)?;
        let (size, header) = if small == 1 {
            (usize::try_from(long(bytes, 8)?)?, 16)
        } else {
            (small as usize, 8)
        };
        anyhow::ensure!(size >= header && size <= bytes.len());
        result.push(Atom {
            kind: bytes[4..8].try_into()?,
            payload: &bytes[header..size],
        });
        bytes = &bytes[size..];
    }
    Ok(result)
}

fn one<'a>(values: &[Atom<'a>], kind: &[u8; 4]) -> anyhow::Result<&'a [u8]> {
    let mut matching = values.iter().filter(|value| &value.kind == kind);
    let value = matching
        .next()
        .ok_or_else(|| anyhow::anyhow!("copy boundary box unavailable"))?;
    anyhow::ensure!(matching.next().is_none());
    Ok(value.payload)
}

fn number(bytes: &[u8], offset: usize) -> anyhow::Result<u32> {
    Ok(u32::from_be_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| anyhow::anyhow!("copy boundary integer truncated"))?
            .try_into()?,
    ))
}

fn long(bytes: &[u8], offset: usize) -> anyhow::Result<u64> {
    Ok(u64::from_be_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or_else(|| anyhow::anyhow!("copy boundary integer truncated"))?
            .try_into()?,
    ))
}

fn versioned(bytes: &[u8]) -> anyhow::Result<u32> {
    match bytes.first() {
        Some(0) => number(bytes, 12),
        Some(1) => number(bytes, 20),
        _ => anyhow::bail!("copy boundary clock version unsupported"),
    }
}

fn origin(initialization: &[u8], fragment: &[u8]) -> anyhow::Result<(i64, Value)> {
    let root = atoms(initialization)?;
    let movie = atoms(one(&root, b"moov")?)?;
    let mut clocks = BTreeMap::new();
    for track in movie.iter().filter(|value| &value.kind == b"trak") {
        let track = atoms(track.payload)?;
        anyhow::ensure!(track.iter().all(|value| &value.kind != b"edts"));
        let id = versioned(one(&track, b"tkhd")?)?;
        let media = atoms(one(&track, b"mdia")?)?;
        let scale = versioned(one(&media, b"mdhd")?)?;
        let handler = one(&media, b"hdlr")?
            .get(8..12)
            .ok_or_else(|| anyhow::anyhow!("copy boundary handler missing"))?;
        anyhow::ensure!(
            id > 0
                && (1..=1_000_000_000).contains(&scale)
                && (handler == b"vide" || handler == b"soun")
                && clocks.insert(id, (scale, handler.to_vec())).is_none()
        );
    }
    anyhow::ensure!(clocks.len() == 2);
    let root = atoms(fragment)?;
    let moof = atoms(one(&root, b"moof")?)?;
    let mut decoded = BTreeMap::new();
    let mut proof = Vec::new();
    for track in moof.iter().filter(|value| &value.kind == b"traf") {
        let track = atoms(track.payload)?;
        let id = number(one(&track, b"tfhd")?, 4)?;
        let (scale, handler) = clocks
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("copy boundary fragment track unknown"))?;
        let data = one(&track, b"tfdt")?;
        anyhow::ensure!(number(data, 0)? & 0x00ff_ffff == 0);
        let timestamp = match data.first() {
            Some(0) if data.len() == 8 => u64::from(number(data, 4)?),
            Some(1) if data.len() == 12 => long(data, 4)?,
            _ => anyhow::bail!("copy boundary decode time invalid"),
        };
        let milliseconds = i64::try_from(u128::from(timestamp) * 1000 / u128::from(*scale))?;
        anyhow::ensure!(
            (3500..=5000).contains(&milliseconds) && decoded.insert(id, milliseconds).is_none()
        );
        proof.push(json!({"track_id":id,"handler":std::str::from_utf8(handler)?,"timescale":scale,"tfdt":timestamp,"floor_ms":milliseconds}));
    }
    anyhow::ensure!(decoded.len() == 2 && decoded.keys().eq(clocks.keys()));
    let minimum = *decoded
        .values()
        .min()
        .ok_or_else(|| anyhow::anyhow!("copy boundary origin missing"))?;
    Ok((minimum, json!(proof)))
}

async fn read(path: &Path, limit: u64) -> anyhow::Result<Vec<u8>> {
    let metadata = tokio::fs::symlink_metadata(path).await?;
    anyhow::ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && (1..=limit).contains(&metadata.len())
    );
    let mut bytes = Vec::new();
    tokio::fs::File::open(path)
        .await?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .await?;
    anyhow::ensure!(bytes.len() as u64 == metadata.len());
    Ok(bytes)
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

async fn pixels(
    tools: &process::FixtureProcesses,
    path: &Path,
    absolute: bool,
) -> anyhow::Result<Vec<u8>> {
    let mut args = vec!["-v".into(), "error".into(), "-nostdin".into()];
    if absolute {
        args.push("-copyts".into());
    }
    args.extend([
        "-threads".into(),
        "1".into(),
        "-i".into(),
        path.to_string_lossy().into_owned(),
        "-ss".into(),
        "5".into(),
        "-map".into(),
        "0:v:0".into(),
        "-frames:v".into(),
        "1".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-threads".into(),
        "1".into(),
        "-f".into(),
        "rawvideo".into(),
        "pipe:1".into(),
    ]);
    let bytes = process::original(tools, "ffmpeg", &args).await?;
    anyhow::ensure!(bytes.len() == 320 * 180 * 3 / 2);
    Ok(bytes)
}

fn frame_id(bytes: &[u8]) -> anyhow::Result<u8> {
    let mut value = 0;
    for bit in 0..8 {
        let mut sum = 0_u32;
        for y in 164..172 {
            for x in 24 + 32 * bit..40 + 32 * bit {
                sum += u32::from(bytes[y * 320 + x]);
            }
        }
        let mean = f64::from(sum) / 128.0;
        anyhow::ensure!(mean <= 60.0 || mean >= 190.0);
        if mean > 125.0 {
            value |= 1 << bit;
        }
    }
    Ok(value)
}

async fn audio(tools: &process::FixtureProcesses, path: &Path) -> anyhow::Result<f64> {
    let bytes = process::original(
        tools,
        "ffmpeg",
        &[
            "-v".into(),
            "error".into(),
            "-nostdin".into(),
            "-copyts".into(),
            "-threads".into(),
            "1".into(),
            "-i".into(),
            path.to_string_lossy().into_owned(),
            "-ss".into(),
            "5".into(),
            "-map".into(),
            "0:a:0".into(),
            "-vn".into(),
            "-t".into(),
            "0.5".into(),
            "-c:a".into(),
            "pcm_s16le".into(),
            "-ac".into(),
            "1".into(),
            "-ar".into(),
            "48000".into(),
            "-threads".into(),
            "1".into(),
            "-f".into(),
            "s16le".into(),
            "pipe:1".into(),
        ],
    )
    .await?;
    let samples = bytes
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        samples.len() == 24_000 && samples.iter().any(|sample| sample.unsigned_abs() > 500)
    );
    let crossings = samples
        .windows(2)
        .filter(|pair| pair[0] <= 0 && pair[1] > 0)
        .count();
    let hz = crossings as f64 * 48_000.0 / samples.len() as f64;
    anyhow::ensure!((hz - 880.0).abs() <= 20.0);
    Ok(hz)
}

pub(super) async fn run(
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    root: &Path,
    tools: &process::FixtureProcesses,
    reference: &Path,
) -> anyhow::Result<Value> {
    anyhow::ensure!(super::source_enabled() && state.transcode_manager.active_session_count() == 0);
    let reference_hash = inventory::hash(reference).await?;
    let source = root.join("source-copy-boundary.mp4");
    process::original(
        tools,
        "ffmpeg",
        &[
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-nostdin".into(),
            "-threads".into(),
            "1".into(),
            "-filter_threads".into(),
            "1".into(),
            "-i".into(),
            reference.to_string_lossy().into_owned(),
            "-map".into(),
            "0:1".into(),
            "-map".into(),
            "0:0".into(),
            "-map".into(),
            "0:2".into(),
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
            "-bf".into(),
            "3".into(),
            "-x264-params".into(),
            "b-adapt=0:scenecut=0:open-gop=0".into(),
            "-g".into(),
            "48".into(),
            "-keyint_min".into(),
            "48".into(),
            "-b:v".into(),
            "400000".into(),
            "-c:a".into(),
            "aac".into(),
            "-profile:a".into(),
            "aac_low".into(),
            "-ar".into(),
            "48000".into(),
            "-ac".into(),
            "1".into(),
            "-b:a".into(),
            "96000".into(),
            "-disposition:a:0".into(),
            "0".into(),
            "-disposition:a:1".into(),
            "default".into(),
            "-threads".into(),
            "1".into(),
            "-t".into(),
            "8".into(),
            "-fs".into(),
            "4194304".into(),
            "-movflags".into(),
            "+faststart".into(),
            "-n".into(),
            source.to_string_lossy().into_owned(),
        ],
    )
    .await?;
    let source_hash = source_media::sha256(&read(&source, 4 * 1024 * 1024).await?);
    tokio::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o444)).await?;
    let source_probe = probe(tools, &source).await?;
    let streams = source_probe["streams"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("copy boundary source streams missing"))?;
    anyhow::ensure!(
        streams.len() == 3
            && streams[0]["codec_name"] == "aac"
            && streams[1]["codec_name"] == "h264"
            && streams[1]["profile"] == "High"
            && streams[1]["level"] == 40
            && streams[1]["has_b_frames"]
                .as_u64()
                .is_some_and(|count| count > 0)
            && streams[2]["codec_name"] == "aac"
            && streams[2]["disposition"]["default"] == 1
    );
    let expected = pixels(tools, &source, false).await?;
    anyhow::ensure!(frame_id(&expected)? == 120);
    let mut cases = Vec::new();
    for audio_action in [StreamAction::Copy, StreamAction::Encode] {
        let encode = audio_action == StreamAction::Encode;
        let session = state
            .transcode_manager
            .start_session(
                StartSessionParams {
                    media_file_id: Uuid::now_v7(),
                    user_id: fixture.user,
                    source_path: source.clone(),
                    source_video_codec: "h264".into(),
                    source_video_resolution: (320, 180),
                    source_audio_codec: "aac".into(),
                    source_video_stream_index: Some(1),
                    source_audio_stream_index: Some(2),
                    stream_plan: StreamPlan {
                        video: StreamAction::Copy,
                        audio: audio_action,
                    },
                    output_profile: Some(OutputProfile::SourceCopySdr),
                    target_audio_channels: encode.then_some(1),
                    target_audio_bitrate: encode.then_some(96_000),
                    subtitle_stream_index: None,
                    subtitle_stream_ordinal: None,
                    subtitle_burn_in: false,
                    target_video_codec: Some("h264".into()),
                    target_audio_codec: Some("aac".into()),
                    target_resolution: None,
                    target_bitrate: None,
                    seek_position_ms: Some(5000),
                    hw_accel_override: Some(HwAccelMethod::Software),
                    recovery_attempts: 0,
                },
                &state.bootstrap.data_dir,
            )
            .await?;
        let checked = async {
            anyhow::ensure!(session.output_profile == Some(OutputProfile::SourceCopySdr) && session.stream_plan.video == StreamAction::Copy
                && session.stream_plan.audio == audio_action && session.source_video_stream_index == Some(1) && session.source_audio_stream_index == Some(2)
                && session.segment_dir.canonicalize()?.starts_with(root) && state.transcode_manager.session_failure(session.id).is_none());
            let initialization = read(&session.segment_dir.join("init.mp4"),512*1024).await?;
            let fragment = read(&session.segment_dir.join("seg_0000.m4s"),8*1024*1024).await?;
            let (origin_ms, track_dts) = origin(&initialization,&fragment)?;
            anyhow::ensure!(origin_ms == session.source_start_ms && origin_ms == if encode {3916} else {3904},
                "actual H264 copied source origin differs from its raw track DTS or pinned reference: {origin_ms}");
            let path = root.join(if encode {"source-copy-boundary-encode.mp4"} else {"source-copy-boundary-copy.mp4"});
            let mut media = initialization;
            media.extend_from_slice(&fragment);
            tokio::fs::write(&path,&media).await?;
            let actual = pixels(tools,&path,true).await?;
            anyhow::ensure!(actual == expected && frame_id(&actual)? == 120,"H264 copied seek changed source frame120 native pixels");
            let hz = audio(tools,&path).await?;
            let output = probe(tools,&path).await?;
            let video = &output["streams"][0];
            let audio = &output["streams"][1];
            anyhow::ensure!(output["streams"].as_array().is_some_and(|streams|streams.len()==2) && video["codec_name"]=="h264"
                && video["profile"]=="High" && video["level"]==40 && video["codec_tag_string"]=="avc1" && video["r_frame_rate"]=="24/1"
                && audio["codec_name"]=="aac" && audio["profile"]=="LC" && audio["sample_rate"]=="48000" && audio["channels"]==1);
            tokio::fs::remove_file(&path).await?;
            Ok::<_,anyhow::Error>(json!({"audio_action":if encode{"encode"}else{"copy"},"source_offset_ms":origin_ms,"track_dts":track_dts,
                "no_edit_lists":true,"source_frame_id":120,"exact_source_pixels":true,"native_pixels_sha256":source_media::sha256(&actual),
                "selected_audio_hz":hz,"source_video_index":1,"source_audio_index":2,"output_profile":"source_copy_sdr"}))
        }.await;
        tokio::time::timeout(
            Duration::from_secs(10),
            state.transcode_manager.stop_session(session.id),
        )
        .await??;
        anyhow::ensure!(
            state.transcode_manager.get_session(&session.id).is_none()
                && !session.segment_dir.exists()
                && state.transcode_manager.active_session_count() == 0,
            "H264 copied boundary did not release its generation/cache/capacity"
        );
        let mut proof = checked?;
        proof["cleanup_passed"] = json!(true);
        cases.push(proof);
    }
    anyhow::ensure!(
        inventory::hash(reference).await? == reference_hash
            && inventory::hash(&source).await? == source_hash
    );
    tokio::fs::remove_file(&source).await?;
    Ok(
        json!({"passed":true,"actual_manager":true,"cases":cases,"source_sha256":source_hash,"reference_sha256":reference_hash,
        "source_preserved":true,"reference_preserved":true,"cleanup_passed":true,"source_b_frames":streams[1]["has_b_frames"]}),
    )
}
