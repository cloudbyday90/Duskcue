// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;
use serde_json::{Value, json};
use uuid::Uuid;

#[path = "../../../tests/support/guarded_docker.rs"]
mod guarded_docker;

#[cfg(target_os = "linux")]
#[path = "arguments_local_tests.rs"]
mod local;

#[path = "arguments_source_probe.rs"]
mod source_probe;

#[test]
fn hls_output_defaults_to_append_only_event_publication() {
    let arguments = build_hls_output_args(2, "/cache/seg_%04d.m4s", "/cache/manifest.m3u8");
    for (option, expected) in [
        ("-hls_playlist_type", "event"),
        ("-hls_list_size", "0"),
        ("-hls_segment_type", "fmp4"),
        ("-hls_fmp4_init_filename", "init.mp4"),
    ] {
        let position = arguments.iter().position(|value| value == option).unwrap();
        assert_eq!(arguments[position + 1], expected);
    }
}

#[test]
fn type_mapping_keeps_audio_zero_and_never_assumes_video_global_zero() {
    assert_eq!(
        build_stream_mapping_args(Some(0), None),
        ["-map", "0:v:0", "-map", "0:0"]
    );
    assert_eq!(
        build_stream_mapping_args(None, None),
        ["-map", "0:v:0", "-map", "0:a:0?"]
    );
}

#[test]
fn selected_subtitle_ordinal_and_path_escaping_survive_type_based_video_selection() {
    let filter = build_subtitle_burn_in_filter(Path::new("D:\\Media\\movie's.mkv"), 1);
    assert!(filter.starts_with("[0:v:0]subtitles="));
    assert!(filter.contains(":si=1[video]"));
    assert!(filter.contains("D\\:"));
    assert!(filter.contains("movie\\'s.mkv"));
    let args = build_stream_mapping_args(Some(2), Some(filter));
    assert_eq!(&args[2..], ["-map", "[video]", "-map", "0:2"]);
}

#[test]
fn subtitle_scaling_uses_one_complex_graph_without_conflicting_output_filter() {
    let encoder = super::super::build_video_encode_args(
        "libx264",
        "h264",
        320,
        180,
        400000,
        48,
        super::super::HwAccelMethod::Software,
    );
    let (plain, filter) = compose_video_filters(encoder.clone(), None).unwrap();
    assert_eq!(plain, encoder);
    assert!(filter.is_none());
    let caption = build_subtitle_burn_in_filter(Path::new("source.mkv"), 1);
    let (encoded, filter) = compose_video_filters(encoder, Some(caption)).unwrap();
    assert!(!encoded.iter().any(|argument| argument == "-vf"));
    let graph = filter.unwrap();
    assert!(graph.starts_with("[0:v:0]subtitles=filename='source.mkv':si=1,scale="));
    assert!(graph.ends_with("[video]"));
}

async fn media_command(
    directory: &Path,
    program: &str,
    args: Vec<String>,
) -> anyhow::Result<Vec<u8>> {
    #[cfg(target_os = "linux")]
    if local::enabled() {
        anyhow::ensure!(
            directory == Path::new("/fixtures"),
            "local fixture directory must be the owned overlay"
        );
        return local::media(program, args).await;
    }
    let container = guarded_docker::GuardedContainer::registered()?;
    let image =
        std::env::var("DUSKCUE_TEST_FFMPEG_IMAGE").unwrap_or_else(|_| "duskcue:local".into());
    let mut command = container.run_args();
    command.extend([
        "--mount".into(),
        format!("type=bind,source={},target=/fixtures", directory.display()),
        "--entrypoint".into(),
        program.into(),
        image,
    ]);
    command.extend(args);
    let output = guarded_docker::command(&command).await?;
    container.remove().await?;
    anyhow::ensure!(
        output.status.success(),
        "real FFmpeg fixture command failed"
    );
    Ok(output.stdout)
}

async fn encode(
    directory: &Path,
    name: &str,
    audio: i32,
    subtitle: Option<usize>,
) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(directory.join(name)).await?;
    let mut args = vec!["-hide_banner".into(), "-loglevel".into(), "error".into()];
    args.extend(build_ffmpeg_input_args(
        None,
        Path::new("/fixtures/source.mkv"),
    ));
    let subtitle_filter = subtitle
        .map(|ordinal| build_subtitle_burn_in_filter(Path::new("/fixtures/source.mkv"), ordinal));
    let video_args = super::super::build_video_encode_args(
        "libx264",
        "h264",
        320,
        180,
        400000,
        48,
        super::super::HwAccelMethod::Software,
    );
    let (video_args, subtitle_filter) =
        compose_video_filters(video_args, subtitle_filter).map_err(anyhow::Error::msg)?;
    args.extend(build_stream_mapping_args(Some(audio), subtitle_filter));
    args.extend(video_args);
    args.extend(super::super::build_audio_encode_args("aac", 1, 64000));
    args.extend([
        "-threads".into(),
        "2".into(),
        "-filter_complex_threads".into(),
        "2".into(),
    ]);
    args.extend(build_hls_output_args(
        2,
        &format!("/fixtures/{name}/seg_%04d.m4s"),
        &format!("/fixtures/{name}/manifest.m3u8"),
    ));
    #[cfg(target_os = "linux")]
    if local::enabled() {
        local::encode(
            &args,
            Path::new("/fixtures/source.mkv"),
            &directory.join(name),
        )
        .await?;
        return Ok(());
    }
    media_command(directory, "ffmpeg", args).await?;
    Ok(())
}

async fn frequency(directory: &Path, name: &str) -> anyhow::Result<f64> {
    let args = [
        "-v",
        "error",
        "-i",
        &format!("/fixtures/{name}/manifest.m3u8"),
        "-map",
        "0:a:0",
        "-t",
        "1",
        "-ac",
        "1",
        "-ar",
        "8000",
        "-c:a",
        "pcm_s16le",
        "-f",
        "s16le",
        "pipe:1",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let pcm = media_command(directory, "ffmpeg", args).await?;
    let values: Vec<i16> = pcm
        .chunks_exact(2)
        .map(|value| i16::from_le_bytes([value[0], value[1]]))
        .skip(500)
        .collect();
    anyhow::ensure!(
        values.len() > 4000,
        "real mapped audio did not decode enough samples"
    );
    let crossings = values
        .windows(2)
        .filter(|pair| (pair[0] < 0) != (pair[1] < 0))
        .count();
    Ok(crossings as f64 * 8000.0 / (2.0 * values.len() as f64))
}

async fn frame(directory: &Path, name: &str) -> anyhow::Result<Vec<u8>> {
    media_command(
        directory,
        "ffmpeg",
        [
            "-v",
            "error",
            "-ss",
            "3",
            "-i",
            &format!("/fixtures/{name}/manifest.m3u8"),
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted resource wrapper and existing Docker FFmpeg image"]
async fn real_audio_first_default_description_and_selected_srt_decode_through_production_arguments()
-> anyhow::Result<()> {
    let cache = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(".cache/tonight-server-ffmpeg");
    tokio::fs::create_dir_all(&cache).await?;
    let directory = cache.join(Uuid::now_v7().to_string());
    #[cfg(target_os = "linux")]
    let directory = if local::enabled() {
        Path::new("/fixtures").to_owned()
    } else {
        directory
    };
    tokio::fs::create_dir_all(&directory).await?;
    tokio::fs::write(
        directory.join("first.srt"),
        "1\n00:00:01,000 --> 00:00:07,000\nUNSELECTED CAPTION\n\n",
    )
    .await?;
    tokio::fs::write(
        directory.join("second.srt"),
        "1\n00:00:01,000 --> 00:00:07,000\nDUSKCUE SELECTED CAPTION\n\n",
    )
    .await?;
    let generate = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=220:sample_rate=48000",
        "-f",
        "lavfi",
        "-i",
        "color=c=red:size=320x180:rate=24",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=880:sample_rate=48000",
        "-i",
        "/fixtures/first.srt",
        "-i",
        "/fixtures/second.srt",
        "-map",
        "0:a:0",
        "-map",
        "1:v:0",
        "-map",
        "2:a:0",
        "-map",
        "3:s:0",
        "-map",
        "4:s:0",
        "-t",
        "8",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-g",
        "48",
        "-keyint_min",
        "48",
        "-sc_threshold",
        "0",
        "-threads",
        "2",
        "-c:a",
        "aac",
        "-c:s",
        "srt",
        "-disposition:a:0",
        "0",
        "-disposition:a:1",
        "default+visual_impaired",
        "-metadata:s:a:0",
        "language=eng",
        "-metadata:s:a:1",
        "language=fra",
        "-disposition:s:0",
        "0",
        "-disposition:s:1",
        "hearing_impaired",
        "-metadata:s:s:1",
        "language=fra",
        "-y",
        "/fixtures/source.mkv",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    media_command(&directory, "ffmpeg", generate).await?;
    source_probe::require_two_second_keyframes(&directory).await?;
    let probe: Value = serde_json::from_slice(
        &media_command(
            &directory,
            "ffprobe",
            [
                "-v",
                "error",
                "-show_streams",
                "-of",
                "json",
                "/fixtures/source.mkv",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        )
        .await?,
    )?;
    let streams = probe["streams"].as_array().unwrap();
    assert_eq!(streams[0]["codec_type"], json!("audio"));
    assert_eq!(streams[1]["codec_type"], json!("video"));
    let audio = streams
        .iter()
        .filter(|stream| stream["codec_type"] == "audio")
        .map(|stream| json!({"index":stream["index"],"disposition":stream["disposition"]}))
        .collect::<Vec<_>>();
    let metadata = json!({"audio":audio});
    let default = crate::domains::playback::service::default_audio_stream_index(&metadata).unwrap();
    assert_eq!(default, 2);
    assert_eq!(streams[2]["disposition"]["visual_impaired"], json!(1));
    assert_eq!(streams[4]["disposition"]["hearing_impaired"], json!(1));
    encode(&directory, "default", default, None).await?;
    encode(&directory, "zero", 0, None).await?;
    encode(&directory, "caption", default, Some(1)).await?;
    let default_frequency = frequency(&directory, "default").await?;
    let zero_frequency = frequency(&directory, "zero").await?;
    assert!((default_frequency - 880.0).abs() < 20.0);
    assert!((zero_frequency - 220.0).abs() < 20.0);
    let plain = frame(&directory, "default").await?;
    let caption = frame(&directory, "caption").await?;
    assert_eq!(plain.len(), 320 * 180 * 3);
    assert_eq!(caption.len(), plain.len());
    let lower_half = plain.len() / 2;
    assert!(
        plain[lower_half..]
            .iter()
            .zip(&caption[lower_half..])
            .filter(|(a, b)| a != b)
            .count()
            > 100,
        "selected SRT did not visibly alter the caption region"
    );
    println!(
        "\nDUSKCUE_FFMPEG_FIXTURE={}",
        json!({"directory":directory,"audio_default_index":default,"subtitle_ordinal":1,"default_frequency":default_frequency,"zero_frequency":zero_frequency})
    );
    Ok(())
}
