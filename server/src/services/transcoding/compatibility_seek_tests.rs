// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use anyhow::{Context, ensure};
use serde_json::{Value, json};

use crate::services::transcoding::HwAccelMethod;
use crate::services::transcoding::arguments::{
    build_ffmpeg_input_args, build_ffmpeg_output_seek_args, build_hls_output_args,
    build_stream_mapping_args_for_video,
};
use crate::services::transcoding::encoding::{
    build_audio_encode_args, build_threading_args, build_video_encode_args,
};
use crate::services::transcoding::plan::{StreamAction, StreamPlan};
use crate::state::CpuConfig;

use super::fixture::{Corpus, arguments};

fn prelude() -> Vec<String> {
    arguments(&["-hide_banner", "-loglevel", "error", "-nostdin"])
}

async fn generate_source(corpus: &Corpus) -> anyhow::Result<()> {
    for (name, text) in [
        ("first", "UNSELECTED CAPTION"),
        ("second", "DUSKCUE SELECTED CAPTION"),
    ] {
        tokio::fs::write(
            corpus.owned.join(format!("{name}.srt")),
            format!("1\n00:00:01,000 --> 00:00:07,000\n{text}\n\n"),
        )
        .await?;
    }
    let mut command = prelude();
    for input in [
        "sine=frequency=220:sample_rate=48000",
        "color=c=red:size=320x180:rate=24",
        "sine=frequency=880:sample_rate=48000",
    ] {
        command.extend(arguments(&["-f", "lavfi", "-i", input]));
    }
    command.extend(arguments(&[
        "-i",
        &corpus.input("first.srt"),
        "-i",
        &corpus.input("second.srt"),
    ]));
    for stream in ["0:a:0", "1:v:0", "2:a:0", "3:s:0", "4:s:0"] {
        command.extend(arguments(&["-map", stream]));
    }
    command.extend(arguments(&[
        "-t",
        "8",
        "-fs",
        "16777216",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
    ]));
    command.extend(arguments(&[
        "-g",
        "48",
        "-keyint_min",
        "48",
        "-sc_threshold",
        "0",
        "-threads",
        "1",
    ]));
    command.extend(arguments(&[
        "-c:a",
        "aac",
        "-c:s",
        "srt",
        "-disposition:a:0",
        "0",
        "-disposition:a:1",
        "default+visual_impaired",
    ]));
    command.extend(arguments(&[
        "-metadata:s:a:0",
        "language=eng",
        "-metadata:s:a:1",
        "language=fra",
    ]));
    command.extend(arguments(&[
        "-disposition:s:0",
        "0",
        "-disposition:s:1",
        "hearing_impaired",
        "-metadata:s:s:1",
        "language=fra",
    ]));
    command.extend(arguments(&["-n", &corpus.input("eight-seconds.mkv")]));
    corpus.media("ffmpeg", command).await?;
    let mut command = prelude();
    command.extend(arguments(&[
        "-stream_loop",
        "-1",
        "-i",
        &corpus.input("eight-seconds.mkv"),
    ]));
    command.extend(arguments(&[
        "-map",
        "0",
        "-c",
        "copy",
        "-threads",
        "1",
        "-t",
        "600",
        "-fs",
        "33554432",
        "-n",
        &corpus.input("source.mkv"),
    ]));
    corpus.media("ffmpeg", command).await?;
    let source = corpus.owned.join("source.mkv");
    let metadata = tokio::fs::symlink_metadata(&source).await?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() > 0
            && metadata.len() <= 33_554_432,
        "seek source escaped its regular-file size bounds"
    );
    let inventory = probe(corpus, "source.mkv", &["-show_streams", "-show_format"]).await?;
    let seconds = inventory["format"]["duration"]
        .as_str()
        .context("seek source duration absent")?
        .parse::<f64>()?;
    ensure!(
        seconds.is_finite() && (seconds - 600.0).abs() <= 1.0,
        "seek source duration is {seconds}, expected 600 seconds"
    );
    let streams = inventory["streams"]
        .as_array()
        .context("seek source streams absent")?;
    ensure!(
        streams.len() == 5
            && streams[0]["codec_type"] == "audio"
            && streams[1]["codec_name"] == "h264"
            && streams[1]["width"] == 320
            && streams[1]["height"] == 180
            && streams[2]["codec_name"] == "aac"
            && streams[2]["disposition"]["default"] == 1
            && streams[2]["disposition"]["visual_impaired"] == 1,
        "seek fixture no longer has the qualified audio-first video1/audio2 layout"
    );
    Ok(())
}

async fn probe(corpus: &Corpus, name: &str, options: &[&str]) -> anyhow::Result<Value> {
    let mut command = arguments(&["-v", "error"]);
    command.extend(arguments(options));
    command.extend(arguments(&["-of", "json", &corpus.input(name)]));
    let output = corpus.media("ffprobe", command).await?;
    ensure!(
        output.len() <= 262_144,
        "seek probe exceeded its JSON evidence bound"
    );
    Ok(serde_json::from_slice(&output)?)
}

async fn bounded_file(corpus: &Corpus, name: &str) -> anyhow::Result<Vec<u8>> {
    let path = corpus.owned.join(name);
    let metadata = tokio::fs::symlink_metadata(&path).await?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() > 0
            && metadata.len() <= 16_777_216,
        "seek fragment is not a bounded regular file: {name}"
    );
    Ok(tokio::fs::read(path).await?)
}

async fn reference_first_video_hash(
    corpus: &Corpus,
    video_args: &[String],
) -> anyhow::Result<String> {
    let mut command = prelude();
    command.extend(build_ffmpeg_input_args(
        Some(30_000),
        Path::new(&corpus.input("source.mkv")),
    ));
    command.extend(arguments(&["-map", "0:1", "-an", "-sn"]));
    command.extend_from_slice(video_args);
    command.extend(build_threading_args(&CpuConfig {
        ffmpeg_threads: Some(1),
        ..CpuConfig::default()
    }));
    command.extend(arguments(&[
        "-t",
        "8",
        "-y",
        &corpus.input("video-reference.mp4"),
    ]));
    corpus.media("ffmpeg", command).await?;
    super::decode::decoded_video_hashes(corpus, "video-reference.mp4")
        .await?
        .into_iter()
        .next()
        .context("video-only reference did not decode a first frame")
}

async fn exercise(corpus: &Corpus) -> anyhow::Result<()> {
    let identity = String::from_utf8(corpus.media("ffmpeg", arguments(&["-version"])).await?)?;
    generate_source(corpus).await?;
    let original = blake3::hash(&tokio::fs::read(corpus.owned.join("source.mkv")).await?)
        .to_hex()
        .to_string();
    tokio::fs::create_dir(corpus.owned.join("seek")).await?;
    let plan = StreamPlan {
        video: StreamAction::Encode,
        audio: StreamAction::Copy,
    };
    let video_args = build_video_encode_args(
        "libx264",
        "h264",
        320,
        180,
        1_500_000,
        48,
        HwAccelMethod::Software,
    );
    let mut command = prelude();
    command.extend(build_ffmpeg_input_args(
        Some(30_000),
        Path::new(&corpus.input("source.mkv")),
    ));
    command.extend(build_ffmpeg_output_seek_args(Some(30_000), plan));
    command.extend(build_stream_mapping_args_for_video(Some(1), Some(2), None));
    command.extend(plan.codec_args(
        video_args.clone(),
        build_audio_encode_args("aac", 1, 128_000),
    ));
    command.extend(build_threading_args(&CpuConfig {
        ffmpeg_threads: Some(1),
        ..CpuConfig::default()
    }));
    command.extend(arguments(&["-t", "8", "-progress", "pipe:1"]));
    command.extend(build_hls_output_args(
        2,
        &corpus.input("seek/seg_%04d.m4s"),
        &corpus.input("seek/manifest.m3u8"),
    ));
    let production_arguments = command.clone();
    corpus.media("ffmpeg", command).await?;
    let playlist = String::from_utf8(bounded_file(corpus, "seek/manifest.m3u8").await?)?;
    ensure!(
        playlist.len() <= 65_536
            && playlist
                .lines()
                .any(|line| line == "#EXT-X-MAP:URI=\"init.mp4\"")
            && playlist
                .lines()
                .find(|line| !line.is_empty() && !line.starts_with('#'))
                == Some("seg_0000.m4s"),
        "seek HLS did not publish its actual first fragment and initialization"
    );
    let mut first = bounded_file(corpus, "seek/init.mp4").await?;
    first.extend(bounded_file(corpus, "seek/seg_0000.m4s").await?);
    ensure!(
        first.len() <= 16_777_216,
        "combined seek first fragment exceeded its bound"
    );
    tokio::fs::write(corpus.owned.join("first-fragment.mp4"), first).await?;
    let decoded = probe(
        corpus,
        "first-fragment.mp4",
        &[
            "-select_streams",
            "v:0",
            "-show_frames",
            "-show_entries",
            "frame=best_effort_timestamp_time,pkt_dts_time",
        ],
    )
    .await?;
    let packets = probe(
        corpus,
        "first-fragment.mp4",
        &[
            "-show_packets",
            "-show_entries",
            "packet=codec_type,stream_index,pts_time,dts_time,duration_time,flags",
        ],
    )
    .await?;
    let packets = packets["packets"]
        .as_array()
        .context("seek fragment packet diagnostics absent")?;
    let timestamp = decoded["frames"]
        .as_array()
        .and_then(|frames| frames.first())
        .and_then(|frame| frame["best_effort_timestamp_time"].as_str())
        .map(str::parse::<f64>)
        .transpose()?;
    let packet_pts = |kind: &str| -> anyhow::Result<f64> {
        let packet = packets
            .iter()
            .find(|packet| packet["codec_type"] == kind)
            .with_context(|| format!("first {kind} packet absent"))?;
        Ok(packet["pts_time"]
            .as_str()
            .with_context(|| format!("first {kind} packet PTS absent"))?
            .parse()?)
    };
    let audio_pts = packet_pts("audio")?;
    let video_pts = packet_pts("video")?;
    let packet_delta = video_pts - audio_pts;
    println!(
        "DUSKCUE_SEEK_TIMELINE={}",
        json!({"version":1,"seek_source_ms":30_000,"source_seconds":600,"output_limit_seconds":8,"source_audio_index":2,"source_video_index":1,"stream_plan":{"video":"encode","audio":"copy"},"first_decoded_video_pts_seconds":timestamp,"first_video_minus_audio_packet_pts_seconds":packet_delta,"first_video_packet":packets.iter().find(|packet| packet["codec_type"] == "video"),"first_audio_packet":packets.iter().find(|packet| packet["codec_type"] == "audio"),"ffmpeg_version_and_configuration":identity,"image":std::env::var("DUSKCUE_TEST_FFMPEG_IMAGE").ok(),"arguments":production_arguments,"source_blake3":original})
    );
    let frequency = super::decode::audio_frequency(corpus, "first-fragment.mp4", "0.0").await?;
    println!(
        "DUSKCUE_SEEK_SELECTED_AUDIO={}",
        json!({"measured_frequency_hz":frequency,"expected_frequency_hz":880})
    );
    let mixed_hash = super::decode::decoded_video_hashes(corpus, "first-fragment.mp4")
        .await?
        .into_iter()
        .next()
        .context("mixed seek fragment did not decode a first frame")?;
    let reference_hash = reference_first_video_hash(corpus, &video_args).await?;
    println!(
        "DUSKCUE_SEEK_FIRST_FRAME={}",
        json!({"mixed_audio_frame_hash":mixed_hash,"video_only_reference_frame_hash":reference_hash,"source_pattern":"constant_red","proof_boundary":"first_decoded_pixel_preservation; source_position_requires_timestamp_evidence"})
    );
    ensure!(
        blake3::hash(&tokio::fs::read(corpus.owned.join("source.mkv")).await?)
            .to_hex()
            .as_str()
            == original,
        "seek playback modified its source"
    );
    let timestamp = timestamp.context(
        "first init+fragment did not decode a video frame; packet diagnostics are above",
    )?;
    ensure!(
        timestamp.is_finite() && (0.0..=0.25).contains(&timestamp),
        "first decoded video PTS after a 30-second video-encode/audio-copy seek is {timestamp:.9}s; expected stream-relative 0..=0.25s"
    );
    ensure!(
        packet_delta.is_finite() && packet_delta.abs() <= 0.25,
        "first video/audio packet PTS delta is {packet_delta:.9}s, expected absolute delta <=0.25s"
    );
    ensure!(
        (frequency - 880.0).abs() < 20.0,
        "copy-audio seek selected {frequency:.3}Hz content instead of 880Hz"
    );
    ensure!(
        mixed_hash == reference_hash,
        "copy-audio clipping changed the first decoded video frame compared with the identical video-only seek reference"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted resource wrapper and existing Docker FFmpeg image; real 30-second copy-audio seek regression"]
async fn real_audio_first_copy_audio_seek_keeps_first_fragment_stream_relative()
-> anyhow::Result<()> {
    let corpus = Corpus::new().await?;
    let result = exercise(&corpus).await;
    let cleanup = corpus.cleanup().await;
    cleanup.context("owned seek regression cleanup failed")?;
    result
}
