// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use anyhow::{Context, ensure};
use serde_json::json;

use super::super::super::HwAccelMethod;
use super::super::super::encoding::{build_audio_encode_args, build_video_encode_args};
use super::super::super::plan::{StreamAction, StreamPlan};
use super::super::{
    build_ffmpeg_input_args, build_hls_output_args, build_stream_mapping_args_for_video,
};

#[path = "compatibility_decode.rs"]
mod decode;
#[path = "compatibility_fixture.rs"]
mod fixture;
#[path = "compatibility_seek_tests.rs"]
mod seek;

use decode::{audio_frequency, decoded_rgb_frame, decoded_video_hashes, probe, require_output};
use fixture::{
    Corpus, arguments, damage_transport_stream, generate, prelude, remove_final_metadata,
};

const WIDTH: usize = 160;
const HEIGHT: usize = 90;

async fn package(
    corpus: &Corpus,
    source: &str,
    output: &str,
    video: i32,
    audio: i32,
    plan: StreamPlan,
) -> anyhow::Result<Vec<u8>> {
    tokio::fs::create_dir(corpus.owned.join(output)).await?;
    let mut command = prelude();
    command.extend(build_ffmpeg_input_args(
        None,
        Path::new(&corpus.input(source)),
    ));
    command.extend(build_stream_mapping_args_for_video(
        Some(video),
        Some(audio),
        None,
    ));
    command.extend(plan.codec_args(
        build_video_encode_args(
            "libx264",
            "h264",
            WIDTH as u32,
            HEIGHT as u32,
            300000,
            48,
            HwAccelMethod::Software,
        ),
        build_audio_encode_args("aac", 1, 64000),
    ));
    command.extend(arguments(&[
        "-threads",
        "2",
        "-progress",
        "pipe:1",
        "-nostats",
    ]));
    command.extend(build_hls_output_args(
        2,
        &corpus.input(&format!("{output}/seg_%04d.m4s")),
        &corpus.input(&format!("{output}/manifest.m3u8")),
    ));
    corpus.media("ffmpeg", command).await
}

async fn exercise(corpus: &Corpus) -> anyhow::Result<()> {
    let identity = String::from_utf8(corpus.media("ffmpeg", arguments(&["-version"])).await?)?;
    ensure!(
        identity.contains("ffmpeg version") && identity.contains("configuration:"),
        "FFmpeg runtime identity absent"
    );
    println!(
        "DUSKCUE_COMPATIBILITY_RUNTIME={}",
        json!({"ffmpeg_version_and_configuration":identity,"image":std::env::var("DUSKCUE_TEST_FFMPEG_IMAGE").ok()})
    );
    generate(corpus).await?;
    let damage = damage_transport_stream(corpus).await?;
    remove_final_metadata(corpus).await?;
    let names = [
        "clean.mp4",
        "opus.mkv",
        "audio-first.mkv",
        "clean.ts",
        "damaged-truncated.ts",
        "missing-moov.mp4",
    ];
    let original_hashes = corpus.hashes(&names).await?;
    let reference = decoded_video_hashes(corpus, "clean.mp4").await?;
    ensure!(
        reference.len() == 192,
        "clean eight-second video frame inventory changed"
    );
    let audio_first = probe(corpus, "audio-first.mkv").await?;
    ensure!(
        audio_first["streams"][0]["codec_type"] == "audio"
            && audio_first["streams"][1]["codec_type"] == "video"
            && audio_first["streams"][2]["codec_type"] == "audio",
        "audio-first fixture global stream layout changed"
    );
    for (source, output, video, audio, audio_action, expected_frequency) in [
        ("clean.mp4", "clean", 0, 1, StreamAction::Copy, 440.0),
        (
            "opus.mkv",
            "audio-converted",
            0,
            1,
            StreamAction::Encode,
            440.0,
        ),
        (
            "audio-first.mkv",
            "audio-zero",
            1,
            0,
            StreamAction::Encode,
            880.0,
        ),
    ] {
        package(
            corpus,
            source,
            output,
            video,
            audio,
            StreamPlan {
                video: StreamAction::Copy,
                audio: audio_action,
            },
        )
        .await?;
        require_output(corpus, output).await?;
        let playlist = format!("{output}/manifest.m3u8");
        ensure!(
            decoded_video_hashes(corpus, &playlist).await? == reference,
            "copied video changed decoded frame hashes for {source}"
        );
        let frequency = audio_frequency(corpus, &playlist, "0.5").await?;
        ensure!(
            (frequency - expected_frequency).abs() < 20.0,
            "wrong selected audio for {source}: {frequency}"
        );
    }
    let progress = String::from_utf8(
        package(
            corpus,
            "damaged-truncated.ts",
            "recovered",
            0,
            1,
            StreamPlan {
                video: StreamAction::Encode,
                audio: StreamAction::Encode,
            },
        )
        .await?,
    )?;
    let reached = progress
        .lines()
        .filter_map(|line| line.strip_prefix("out_time_us="))
        .filter_map(|value| value.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    ensure!(
        reached >= 5_500_000 && progress.lines().any(|line| line == "progress=end"),
        "damaged source did not progress beyond its known surviving keyframe"
    );
    require_output(corpus, "recovered").await?;
    let earlier = decoded_rgb_frame(corpus, "recovered/manifest.m3u8", "5.0").await?;
    let later = decoded_rgb_frame(corpus, "recovered/manifest.m3u8", "5.5").await?;
    ensure!(
        earlier != later,
        "video stopped advancing after the known damaged region"
    );
    let frequency = audio_frequency(corpus, "recovered/manifest.m3u8", "5.0").await?;
    ensure!(
        (frequency - 440.0).abs() < 20.0,
        "audio did not survive beyond damage"
    );
    let unavailable = package(
        corpus,
        "missing-moov.mp4",
        "unrecoverable",
        0,
        1,
        StreamPlan {
            video: StreamAction::Encode,
            audio: StreamAction::Encode,
        },
    )
    .await;
    ensure!(
        unavailable.is_err(),
        "MP4 without essential sample metadata unexpectedly packaged"
    );
    let phantom = corpus.owned.join("unrecoverable/manifest.m3u8");
    if tokio::fs::try_exists(&phantom).await? {
        let playlist = tokio::fs::read_to_string(phantom).await?;
        ensure!(
            !playlist.contains("#EXTINF:"),
            "unrecoverable source published media segments"
        );
    }
    ensure!(
        corpus.hashes(&names).await? == original_hashes,
        "playback modified a corpus source"
    );
    println!(
        "DUSKCUE_COMPATIBILITY_CORPUS={}",
        json!({"version":1,"source_blake3":original_hashes,"copy_route_frame_hash_count":reference.len(),"ts_damage":damage,"recovered_out_time_us":reached,"missing_moov":"expected_failure_without_media_playlist","result":"passed"})
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted resource wrapper and existing FFmpeg runtime; generated compatibility corpus"]
async fn real_generated_compatibility_corpus_uses_production_plans_and_recovers_surviving_media()
-> anyhow::Result<()> {
    let corpus = Corpus::new().await?;
    let result = exercise(&corpus).await;
    let cleanup = corpus.cleanup().await;
    cleanup.context("owned compatibility fixture cleanup failed")?;
    result
}
