// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use serde_json::{Value, json};

use super::process;

fn damage_sample(bytes: &mut [u8]) -> anyhow::Result<usize> {
    let mut offset = 0;
    let mut payloads = Vec::new();
    let mut count = 0;
    while offset < bytes.len() {
        anyhow::ensure!(bytes.len() - offset >= 4 && count < 64);
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
        let header = offset + 4;
        let end = header
            .checked_add(length)
            .filter(|end| length > 0 && *end <= bytes.len())
            .ok_or_else(|| anyhow::anyhow!("source recovery AVCC NAL length is invalid"))?;
        anyhow::ensure!(bytes[header] & 0x80 == 0 && (1..=23).contains(&(bytes[header] & 31)));
        if matches!(bytes[header] & 31, 1 | 5) {
            anyhow::ensure!(length > 1);
            payloads.push(header + 1..end);
        }
        offset = end;
        count += 1;
    }
    anyhow::ensure!(count > 0);
    for range in &payloads {
        bytes[range.clone()].fill(0xff);
    }
    Ok(payloads.len())
}

pub(super) async fn author(
    tools: &process::FixtureProcesses,
    source: &Path,
    destination: &Path,
) -> anyhow::Result<Value> {
    let metadata = tokio::fs::symlink_metadata(source).await?;
    anyhow::ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && (1..=8 * 1024 * 1024).contains(&metadata.len())
    );
    let original = tokio::fs::read(source).await?;
    anyhow::ensure!(original.len() as u64 == metadata.len());
    let probe: Value = serde_json::from_slice(&process::original(tools,"ffprobe",&[
        "-v".into(),"error".into(),"-select_streams".into(),"V".into(),"-show_packets".into(),"-show_streams".into(),
        "-show_data".into(),"-data_dump_format".into(),"base64".into(),"-show_entries".into(),
        "stream=index,codec_name,extradata,extradata_size:packet=stream_index,pts_time,pos,size,flags".into(),
        "-of".into(),"json".into(),source.to_string_lossy().into_owned(),
    ]).await?)?;
    let streams = probe["streams"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("source recovery configuration missing"))?;
    anyhow::ensure!(
        streams.len() == 1 && streams[0]["index"] == 1 && streams[0]["codec_name"] == "h264"
    );
    use base64::Engine;
    let encoded = streams[0]["extradata"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("source recovery avcC missing"))?;
    anyhow::ensure!(encoded.len() <= 90_000);
    let encoded = encoded
        .bytes()
        .filter(|value| !value.is_ascii_whitespace())
        .collect::<Vec<_>>();
    let configuration = base64::engine::general_purpose::STANDARD.decode(encoded)?;
    anyhow::ensure!(
        configuration.len() >= 7
            && configuration.len() <= 65536
            && configuration[0] == 1
            && configuration[4] & 3 == 3
            && streams[0]["extradata_size"].as_u64() == Some(configuration.len() as u64)
    );
    let packets = probe["packets"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("source recovery packets missing"))?;
    anyhow::ensure!(packets.len() == 192);
    let mut bytes = original.clone();
    let mut samples = 0;
    let mut vcl_nals = 0;
    let mut idr_samples = 0;
    for packet in packets {
        anyhow::ensure!(packet["stream_index"] == 1);
        let pts = packet["pts_time"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("source recovery PTS missing"))?
            .parse::<f64>()?;
        anyhow::ensure!(pts.is_finite());
        if !(2.0..4.0).contains(&pts) {
            continue;
        }
        let start = packet["pos"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("source recovery sample position missing"))?
            .parse::<usize>()?;
        let length = packet["size"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("source recovery sample length missing"))?
            .parse::<usize>()?;
        let end = start
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| anyhow::anyhow!("source recovery sample outside file"))?;
        let changed = damage_sample(&mut bytes[start..end])?;
        anyhow::ensure!(changed > 0);
        samples += 1;
        vcl_nals += changed;
        if packet["flags"]
            .as_str()
            .is_some_and(|flags| flags.contains('K'))
        {
            idr_samples += 1;
        }
    }
    anyhow::ensure!(samples == 48 && vcl_nals >= 48 && idr_samples == 1 && bytes != original);
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    use tokio::io::AsyncWriteExt;
    let mut file = options.open(destination).await?;
    file.write_all(&bytes).await?;
    file.flush().await?;
    anyhow::ensure!(tokio::fs::read(source).await? == original);
    Ok(
        json!({"damaged_samples":samples,"damaged_vcl_nals":vcl_nals,"idr_headers_preserved":idr_samples,
        "damage_start_ms":2000,"damage_end_ms":4000,"avcc_lengths_headers_and_non_vcl_preserved":true,
        "source_unchanged":true}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(nals: &[&[u8]]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for nal in nals {
            bytes.extend((nal.len() as u32).to_be_bytes());
            bytes.extend(*nal);
        }
        bytes
    }

    #[test]
    fn avcc_vcl_damage_preserves_every_length_header_and_non_vcl_nal() {
        let mut bytes = sample(&[
            &[0x06, 0x80, 0x01],
            &[0x65, 0x88, 0x84, 0x21],
            &[0x0c, 0xff, 0x80],
            &[0x41, 0x01, 0x02],
        ]);
        let expected = sample(&[
            &[0x06, 0x80, 0x01],
            &[0x65, 0xff, 0xff, 0xff],
            &[0x0c, 0xff, 0x80],
            &[0x41, 0xff, 0xff],
        ]);
        assert_eq!(damage_sample(&mut bytes).unwrap(), 2);
        assert_eq!(bytes, expected);
        let mut non_vcl = sample(&[&[0x67, 1, 2], &[0x68, 3], &[0x06, 4], &[0x0c, 5]]);
        let original = non_vcl.clone();
        assert_eq!(damage_sample(&mut non_vcl).unwrap(), 0);
        assert_eq!(non_vcl, original);
    }

    #[test]
    fn malformed_avcc_samples_fail_without_partial_corruption() {
        for mut bytes in [
            vec![],
            vec![0, 0, 0],
            vec![0, 0, 0, 0],
            vec![0, 0, 0, 4, 0x65, 1],
            sample(&[&[0xe5, 1]]),
            [sample(&[&[0x65, 1, 2]]), vec![0xff, 0xff, 0xff, 0xff]].concat(),
        ] {
            let original = bytes.clone();
            assert!(damage_sample(&mut bytes).is_err());
            assert_eq!(bytes, original);
        }
    }
}
