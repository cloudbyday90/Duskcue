// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, ensure};
use serde_json::{Value, json};
use uuid::Uuid;

use super::super::media_command;

pub(super) struct Corpus {
    directory: PathBuf,
    pub(super) owned: PathBuf,
    relative: String,
}

impl Corpus {
    pub(super) async fn new() -> anyhow::Result<Self> {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("workspace parent absent")?
            .join(".cache/playback-compatibility");
        #[cfg(target_os = "linux")]
        let directory = if super::super::local::enabled() {
            PathBuf::from("/fixtures")
        } else {
            directory
        };
        let relative = format!("compatibility-{}", Uuid::now_v7());
        let owned = directory.join(&relative);
        tokio::fs::create_dir_all(&owned).await?;
        Ok(Self {
            directory,
            owned,
            relative,
        })
    }

    pub(super) fn input(&self, name: &str) -> String {
        format!("/fixtures/{}/{name}", self.relative)
    }

    pub(super) async fn cleanup(&self) -> anyhow::Result<()> {
        let directory = tokio::fs::canonicalize(&self.directory).await?;
        let owned = tokio::fs::canonicalize(&self.owned).await?;
        ensure!(
            owned.parent() == Some(directory.as_path())
                && owned.file_name().and_then(|name| name.to_str()) == Some(self.relative.as_str()),
            "compatibility cleanup target escaped its owned directory"
        );
        tokio::fs::remove_dir_all(owned).await?;
        Ok(())
    }

    pub(super) async fn media(
        &self,
        program: &str,
        arguments: Vec<String>,
    ) -> anyhow::Result<Vec<u8>> {
        media_command(&self.directory, program, arguments).await
    }

    async fn bytes(&self, name: &str) -> anyhow::Result<Vec<u8>> {
        let path = self.owned.join(name);
        ensure!(
            tokio::fs::metadata(&path).await?.len() <= 2_000_000,
            "generated corpus input exceeded its bound"
        );
        Ok(tokio::fs::read(path).await?)
    }

    pub(super) async fn hashes(&self, names: &[&str]) -> anyhow::Result<BTreeMap<String, String>> {
        let mut hashes = BTreeMap::new();
        for name in names {
            hashes.insert(
                (*name).to_owned(),
                blake3::hash(&self.bytes(name).await?).to_hex().to_string(),
            );
        }
        Ok(hashes)
    }
}

pub(super) fn arguments(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

pub(super) fn prelude() -> Vec<String> {
    arguments(&[
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-threads",
        "2",
        "-filter_threads",
        "2",
        "-filter_complex_threads",
        "2",
    ])
}

pub(super) async fn generate(corpus: &Corpus) -> anyhow::Result<()> {
    let mut command = prelude();
    command.extend(arguments(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=160x90:rate=24",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000",
        "-t",
        "8",
        "-map",
        "0:v:0",
        "-map",
        "1:a:0",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-pix_fmt",
        "yuv420p",
        "-bf",
        "0",
        "-g",
        "48",
        "-keyint_min",
        "48",
        "-sc_threshold",
        "0",
        "-c:a",
        "aac",
        "-ac",
        "1",
        "-b:a",
        "64000",
        "-threads",
        "2",
        "-y",
    ]));
    command.push(corpus.input("clean.mp4"));
    corpus.media("ffmpeg", command).await?;

    let mut command = prelude();
    command.extend(arguments(&["-i", &corpus.input("clean.mp4")]));
    command.extend(arguments(&[
        "-map", "0:v:0", "-map", "0:a:0", "-c:v", "copy", "-c:a", "libopus", "-b:a", "64000",
        "-threads", "2", "-y",
    ]));
    command.push(corpus.input("opus.mkv"));
    corpus.media("ffmpeg", command).await?;

    let mut command = prelude();
    command.extend(arguments(&["-i", &corpus.input("clean.mp4")]));
    command.extend(arguments(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=880:sample_rate=48000",
        "-map",
        "1:a:0",
        "-map",
        "0:v:0",
        "-map",
        "0:a:0",
        "-t",
        "8",
        "-c:v",
        "copy",
        "-c:a",
        "libopus",
        "-b:a",
        "64000",
        "-threads",
        "2",
        "-y",
    ]));
    command.push(corpus.input("audio-first.mkv"));
    corpus.media("ffmpeg", command).await?;

    let mut command = prelude();
    command.extend(arguments(&["-i", &corpus.input("clean.mp4")]));
    command.extend(arguments(&[
        "-map",
        "0:v:0",
        "-map",
        "0:a:0",
        "-c",
        "copy",
        "-muxrate",
        "1000k",
        "-mpegts_flags",
        "+resend_headers",
        "-f",
        "mpegts",
        "-y",
    ]));
    command.push(corpus.input("clean.ts"));
    corpus.media("ffmpeg", command).await?;
    Ok(())
}

pub(super) async fn damage_transport_stream(corpus: &Corpus) -> anyhow::Result<Value> {
    let inventory: Value = serde_json::from_slice(
        &corpus
            .media(
                "ffprobe",
                arguments(&[
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-show_packets",
                    "-show_entries",
                    "packet=pos,pts_time,flags",
                    "-of",
                    "json",
                    &corpus.input("clean.ts"),
                ]),
            )
            .await?,
    )?;
    let packets = inventory["packets"]
        .as_array()
        .context("TS video packet inventory absent")?;
    let packet_time = |packet: &Value| -> anyhow::Result<f64> {
        let value: f64 = packet["pts_time"]
            .as_str()
            .context("TS packet timestamp absent")?
            .parse()?;
        ensure!(value.is_finite(), "TS timestamp is not finite");
        Ok(value)
    };
    let origin = packet_time(packets.first().context("TS video packets absent")?)?;
    let mut damage = None;
    let mut truncate = None;
    let mut recovery_keyframe = None;
    for packet in packets {
        let time = packet_time(packet)? - origin;
        let position: usize = packet["pos"]
            .as_str()
            .context("TS packet file position absent")?
            .parse()?;
        if damage.is_none() && time >= 2.25 {
            damage = Some((position, time));
        }
        if recovery_keyframe.is_none()
            && (3.9..=4.1).contains(&time)
            && packet["flags"]
                .as_str()
                .is_some_and(|flags| flags.contains('K'))
        {
            recovery_keyframe = Some(time);
        }
        if truncate.is_none() && time >= 6.5 {
            truncate = Some((position, time));
        }
    }
    let (damage_position, damage_time) = damage.context("damage landmark absent")?;
    let (truncate_position, truncate_time) = truncate.context("truncation landmark absent")?;
    let recovery_keyframe = recovery_keyframe.context("four-second recovery keyframe absent")?;
    let mut bytes = corpus.bytes("clean.ts").await?;
    ensure!(
        damage_position % 188 == 0
            && truncate_position % 188 == 0
            && damage_position + 188 < truncate_position
            && truncate_position < bytes.len()
            && bytes[damage_position] == 0x47,
        "TS byte landmarks do not describe complete transport packets"
    );
    let adaptation = (bytes[damage_position + 3] >> 4) & 3;
    let payload = match adaptation {
        1 => 4,
        3 => 5 + usize::from(bytes[damage_position + 4]),
        _ => anyhow::bail!("selected TS video packet has no payload"),
    };
    let damage_start = damage_position + (payload + 32).max(92);
    let damage_end = damage_position + 188;
    ensure!(
        damage_start < damage_end
            && payload + 4 <= 188
            && bytes[damage_position + 1] & 0x40 != 0
            && bytes[damage_position + payload..damage_position + payload + 3] == [0, 0, 1]
            && (0xe0..=0xef).contains(&bytes[damage_position + payload + 3]),
        "selected TS packet does not contain the start of video elementary-stream payload"
    );
    for value in &mut bytes[damage_start..damage_end] {
        *value ^= 0xa5;
    }
    let video_pid = ((u16::from(bytes[damage_position + 1]) & 0x1f) << 8)
        | u16::from(bytes[damage_position + 2]);
    bytes.truncate(truncate_position);
    tokio::fs::write(corpus.owned.join("damaged-truncated.ts"), bytes).await?;
    Ok(json!({
        "packet_size_bytes":188,
        "damaged_byte_range":[damage_start,damage_end],
        "damaged_packet_start":damage_position,
        "damaged_video_pid":video_pid,
        "damage_source_time_seconds":damage_time,
        "retained_byte_range":[0,truncate_position],
        "truncation_source_time_seconds":truncate_time,
        "next_verified_clean_keyframe_seconds":recovery_keyframe,
        "expected_surviving_source_ranges_seconds":[[0.0,2.0],[4.0,6.0]]
    }))
}

pub(super) async fn remove_final_metadata(corpus: &Corpus) -> anyhow::Result<()> {
    let bytes = corpus.bytes("clean.mp4").await?;
    let mut offset = 0_usize;
    let mut mdat_seen = false;
    while offset + 8 <= bytes.len() {
        let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?);
        let size = match size {
            0 => bytes.len() - offset,
            1 => {
                ensure!(
                    offset + 16 <= bytes.len(),
                    "extended MP4 atom is incomplete"
                );
                usize::try_from(u64::from_be_bytes(
                    bytes[offset + 8..offset + 16].try_into()?,
                ))?
            }
            value => value as usize,
        };
        ensure!(
            size >= 8 && size <= bytes.len() - offset,
            "generated MP4 atom bounds are invalid"
        );
        let kind = &bytes[offset + 4..offset + 8];
        if kind == b"mdat" {
            mdat_seen = true;
        }
        if kind == b"moov" {
            ensure!(
                mdat_seen,
                "fixture requires a final moov after media payload"
            );
            tokio::fs::write(corpus.owned.join("missing-moov.mp4"), &bytes[..offset]).await?;
            return Ok(());
        }
        offset += size;
    }
    anyhow::bail!("generated MP4 lacks final metadata")
}
