// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::CodecConfiguration;

pub(super) fn parse(bytes: &[u8], colors: [u8; 4]) -> Option<CodecConfiguration> {
    if bytes.len() < 5
        || bytes[0] != 0x81
        || bytes[3] & 0xe0 != 0
        || (bytes[3] & 0x10 == 0 && bytes[3] & 15 != 0)
    {
        return None;
    }
    let profile = bytes[1] >> 5;
    let level = bytes[1] & 31;
    let tier = if bytes[2] & 128 == 0 { 'M' } else { 'H' };
    let depth = if bytes[2] & 64 == 0 { 8 } else { 10 };
    if profile != 0
        || level > 23
        || bytes[2] & 0x30 != 0
        || bytes[2] & 0x0c != 0x0c
        || (level < 8 && tier == 'H')
        || ![0, 1, 2].contains(&(bytes[2] & 3))
        || colors[..3].iter().any(|value| *value == 0 || *value == 2)
        || colors[3] > 1
    {
        return None;
    }
    let mut offset = 4;
    let mut sequences = 0;
    while offset < bytes.len() {
        let header = *bytes.get(offset)?;
        offset += 1;
        let kind = (header >> 3) & 15;
        if header & 0x81 != 0
            || header & 2 == 0
            || ![1, 5].contains(&kind)
            || (sequences == 0 && kind != 1)
        {
            return None;
        }
        if header & 4 != 0 {
            if bytes.get(offset)? & 7 != 0 {
                return None;
            }
            offset += 1;
        }
        let mut length = 0usize;
        let mut shift = 0;
        loop {
            let value = *bytes.get(offset)?;
            offset += 1;
            if shift >= 28 {
                return None;
            }
            length = length.checked_add(((value & 127) as usize).checked_shl(shift)?)?;
            shift += 7;
            if value & 128 == 0 {
                break;
            }
        }
        if length == 0 || offset.checked_add(length)? > bytes.len() {
            return None;
        }
        if kind == 1 {
            sequences += 1;
        }
        offset += length;
    }
    if sequences != 1 {
        return None;
    }
    Some(CodecConfiguration {
        copy_codec: format!(
            "av01.{profile}.{level:02}{tier}.{depth:02}.0.11{}.{:02}.{:02}.{:02}.{}",
            bytes[2] & 3,
            colors[0],
            colors[1],
            colors[2],
            colors[3]
        ),
        bit_depth: depth,
        level: i64::from(level),
    })
}
