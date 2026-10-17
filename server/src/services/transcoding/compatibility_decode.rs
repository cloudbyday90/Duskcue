// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use anyhow::{Context, ensure};
use serde_json::Value;

use super::fixture::{Corpus, arguments, prelude};
use super::{HEIGHT, WIDTH};

pub(super) async fn probe(corpus: &Corpus, name: &str) -> anyhow::Result<Value> {
    serde_json::from_slice(
        &corpus
            .media(
                "ffprobe",
                arguments(&[
                    "-v",
                    "error",
                    "-show_streams",
                    "-of",
                    "json",
                    &corpus.input(name),
                ]),
            )
            .await?,
    )
    .map_err(Into::into)
}

pub(super) async fn decoded_video_hashes(
    corpus: &Corpus,
    name: &str,
) -> anyhow::Result<Vec<String>> {
    let mut command = prelude();
    command.extend(arguments(&[
        "-i",
        &corpus.input(name),
        "-map",
        "0:v:0",
        "-an",
        "-sn",
        "-c:v",
        "rawvideo",
        "-pix_fmt",
        "yuv420p",
        "-threads",
        "2",
        "-fps_mode",
        "passthrough",
        "-f",
        "framemd5",
        "pipe:1",
    ]));
    let output = String::from_utf8(corpus.media("ffmpeg", command).await?)?;
    let hashes = output
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            line.rsplit(',')
                .next()
                .context("frame digest absent")
                .map(|value| value.trim().to_owned())
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    ensure!(!hashes.is_empty(), "no decoded video frames");
    Ok(hashes)
}

pub(super) async fn audio_frequency(
    corpus: &Corpus,
    name: &str,
    position: &str,
) -> anyhow::Result<f64> {
    let mut command = prelude();
    command.extend(arguments(&[
        "-i",
        &corpus.input(name),
        "-ss",
        position,
        "-t",
        "0.75",
        "-map",
        "0:a:0",
        "-vn",
        "-ac",
        "1",
        "-ar",
        "8000",
        "-c:a",
        "pcm_s16le",
        "-threads",
        "2",
        "-f",
        "s16le",
        "pipe:1",
    ]));
    let pcm = corpus.media("ffmpeg", command).await?;
    let samples: Vec<i16> = pcm
        .chunks_exact(2)
        .map(|value| i16::from_le_bytes([value[0], value[1]]))
        .skip(500)
        .collect();
    ensure!(
        samples.len() >= 4000,
        "audio did not decode sufficient samples"
    );
    ensure!(
        samples.iter().any(|value| value.unsigned_abs() > 500),
        "decoded audio is silent"
    );
    let crossings = samples
        .windows(2)
        .filter(|pair| (pair[0] < 0) != (pair[1] < 0))
        .count();
    Ok(crossings as f64 * 8000.0 / (2.0 * samples.len() as f64))
}

pub(super) async fn require_output(corpus: &Corpus, output: &str) -> anyhow::Result<()> {
    let playlist =
        tokio::fs::read_to_string(corpus.owned.join(output).join("manifest.m3u8")).await?;
    ensure!(
        playlist.contains("#EXTINF:") && playlist.contains("#EXT-X-ENDLIST"),
        "finite fixture did not publish playable completed HLS"
    );
    let inventory = probe(corpus, &format!("{output}/manifest.m3u8")).await?;
    let streams = inventory["streams"]
        .as_array()
        .context("output stream inventory absent")?;
    ensure!(
        streams.len() == 2
            && streams[0]["codec_name"] == "h264"
            && streams[0]["codec_type"] == "video"
            && streams[1]["codec_name"] == "aac"
            && streams[1]["codec_type"] == "audio",
        "output does not contain the selected H264 video and AAC audio"
    );
    Ok(())
}

pub(super) async fn decoded_rgb_frame(
    corpus: &Corpus,
    name: &str,
    position: &str,
) -> anyhow::Result<Vec<u8>> {
    let mut command = prelude();
    command.extend(arguments(&[
        "-i",
        &corpus.input(name),
        "-ss",
        position,
        "-map",
        "0:v:0",
        "-frames:v",
        "1",
        "-an",
        "-pix_fmt",
        "rgb24",
        "-threads",
        "2",
        "-f",
        "rawvideo",
        "pipe:1",
    ]));
    let frame = corpus.media("ffmpeg", command).await?;
    ensure!(
        frame.len() == WIDTH * HEIGHT * 3,
        "no usable video frame at {position} seconds"
    );
    Ok(frame)
}
