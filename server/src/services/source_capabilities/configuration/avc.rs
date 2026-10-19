// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::CodecConfiguration;

fn nals(bytes: &[u8], offset: &mut usize, count: usize, kind: u8) -> Option<()> {
    if count == 0 || count > 64 {
        return None;
    }
    for _ in 0..count {
        let length = u16::from_be_bytes(bytes.get(*offset..*offset + 2)?.try_into().ok()?) as usize;
        *offset += 2;
        let nal = bytes.get(*offset..*offset + length)?;
        if nal.len() < 2 || nal[0] & 0x80 != 0 || nal[0] & 31 != kind {
            return None;
        }
        if kind == 7 && (nal.len() < 4 || nal[1..4] != bytes[1..4]) {
            return None;
        }
        *offset += length;
    }
    Some(())
}

pub(super) fn parse(bytes: &[u8]) -> Option<CodecConfiguration> {
    if bytes.len() < 7
        || bytes[0] != 1
        || bytes[4] & 0xfc != 0xfc
        || bytes[5] & 0xe0 != 0xe0
        || bytes[4] & 3 == 2
        || bytes[3] == 0
        || bytes[2] & 3 != 0
    {
        return None;
    }
    let profile = bytes[1];
    if ![66, 77, 88, 100, 110].contains(&profile) {
        return None;
    }
    let mut offset = 6;
    nals(bytes, &mut offset, (bytes[5] & 31) as usize, 7)?;
    let count = *bytes.get(offset)? as usize;
    offset += 1;
    nals(bytes, &mut offset, count, 8)?;
    let mut depth = 8;
    if offset < bytes.len() {
        if ![100, 110].contains(&profile)
            || bytes.len() - offset < 4
            || bytes[offset] != 0xfd
            || bytes[offset + 1] & 0xf8 != 0xf8
            || bytes[offset + 2] & 0xf8 != 0xf8
        {
            return None;
        }
        depth = 8 + (bytes[offset + 1] & 7);
        if depth != 8 + (bytes[offset + 2] & 7) || ![8, 10].contains(&depth) {
            return None;
        }
        let count = bytes[offset + 3] as usize;
        offset += 4;
        if count > 0 {
            nals(bytes, &mut offset, count, 13)?;
        }
    } else if profile == 110 {
        return None;
    }
    if offset != bytes.len() {
        return None;
    }
    Some(CodecConfiguration {
        copy_codec: format!("avc1.{:02X}{:02X}{:02X}", bytes[1], bytes[2], bytes[3]),
        bit_depth: depth,
        level: i64::from(bytes[3]),
    })
}
