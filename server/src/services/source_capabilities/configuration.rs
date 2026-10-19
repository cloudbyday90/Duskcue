// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use ring::digest::{SHA256, digest};

mod audio;
mod av1;
mod avc;
mod hevc;

pub(crate) struct CodecConfiguration {
    pub copy_codec: String,
    pub bit_depth: u8,
    pub level: i64,
}

pub(crate) fn video(
    codec: &str,
    bytes: &[u8],
    bit_depth: u8,
    colors: [u8; 4],
) -> Option<CodecConfiguration> {
    if bytes.is_empty() || bytes.len() > 65536 {
        return None;
    }
    let parsed = match codec {
        "h264" => avc::parse(bytes)?,
        "hevc" => hevc::parse(bytes)?,
        "av1" => av1::parse(bytes, colors)?,
        _ => return None,
    };
    (parsed.bit_depth == bit_depth).then_some(parsed)
}

pub(crate) fn audio(
    codec: &str,
    bytes: &[u8],
    profile: Option<&str>,
    sample_rate: u32,
    channels: u16,
) -> Option<&'static str> {
    if bytes.is_empty() || bytes.len() > 65536 {
        return None;
    }
    match codec {
        "aac" if profile == Some("LC") && audio::aac_lc(bytes, sample_rate, channels) => {
            Some("mp4a.40.2")
        }
        "opus" if audio::opus(bytes, channels) && sample_rate == 48000 => Some("opus"),
        _ => None,
    }
}

pub(crate) fn sha256(bytes: &[u8]) -> String {
    digest(&SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) struct Bits<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Bits<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn read(&mut self, count: usize) -> Option<u32> {
        if count > 32 || self.offset.checked_add(count)? > self.bytes.len().checked_mul(8)? {
            return None;
        }
        let mut result = 0;
        for _ in 0..count {
            result =
                result << 1 | u32::from((self.bytes[self.offset / 8] >> (7 - self.offset % 8)) & 1);
            self.offset += 1;
        }
        Some(result)
    }

    pub(super) fn remaining(&self) -> usize {
        self.bytes.len() * 8 - self.offset
    }
}
