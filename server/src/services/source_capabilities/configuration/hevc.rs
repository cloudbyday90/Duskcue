// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::CodecConfiguration;

pub(super) fn parse(bytes: &[u8]) -> Option<CodecConfiguration> {
    if bytes.len() < 23
        || bytes[0] != 1
        || bytes[13] & 0xf0 != 0xf0
        || bytes[15] & 0xfc != 0xfc
        || bytes[16] & 0xfc != 0xfc
        || bytes[17] & 0xf8 != 0xf8
        || bytes[18] & 0xf8 != 0xf8
        || bytes[21] & 3 == 2
    {
        return None;
    }
    let profile = bytes[1] & 31;
    let depth = 8 + (bytes[17] & 7);
    if bytes[1] >> 6 != 0
        || ![1, 2].contains(&profile)
        || ![8, 10].contains(&depth)
        || depth != 8 + (bytes[18] & 7)
        || bytes[16] & 3 != 1
        || bytes[12] == 0
        || (profile == 1 && depth != 8)
    {
        return None;
    }
    let compatibility = u32::from_be_bytes(bytes[2..6].try_into().ok()?).reverse_bits();
    let mut constraints = bytes[6..12].to_vec();
    while constraints.last() == Some(&0) {
        constraints.pop();
    }
    let suffix: String = constraints
        .iter()
        .map(|value| format!(".{value:02X}"))
        .collect();
    let mut offset = 23;
    let mut arrays = std::collections::HashSet::new();
    for _ in 0..bytes[22] {
        let header = *bytes.get(offset)?;
        if header & 0x40 != 0 {
            return None;
        }
        let kind = header & 63;
        offset += 1;
        let count = u16::from_be_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?) as usize;
        offset += 2;
        if count == 0 || count > 64 {
            return None;
        }
        for _ in 0..count {
            let length =
                u16::from_be_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?) as usize;
            offset += 2;
            let nal = bytes.get(offset..offset + length)?;
            if length < 2 || nal[0] & 0x80 != 0 || (nal[0] >> 1) & 63 != kind || nal[1] & 7 == 0 {
                return None;
            }
            offset += length;
        }
        if !arrays.insert(kind) {
            return None;
        }
    }
    if offset != bytes.len() || ![32, 33, 34].iter().all(|kind| arrays.contains(kind)) {
        return None;
    }
    let space = ["", "A", "B", "C"][(bytes[1] >> 6) as usize];
    let tier = if bytes[1] & 32 == 0 { 'L' } else { 'H' };
    Some(CodecConfiguration {
        copy_codec: format!(
            "hev1.{space}{profile}.{compatibility:X}.{tier}{}{suffix}",
            bytes[12]
        ),
        bit_depth: depth,
        level: i64::from(bytes[12]),
    })
}
