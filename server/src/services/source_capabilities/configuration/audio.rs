// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::Bits;

pub(super) fn aac_lc(bytes: &[u8], sample_rate: u32, channels: u16) -> bool {
    let Some(()) = (|| {
        let mut bits = Bits::new(bytes);
        if bits.read(5)? != 2 {
            return None;
        }
        let index = bits.read(4)?;
        let rate = if index == 15 {
            bits.read(24)?
        } else {
            *[
                96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000,
                7350,
            ]
            .get(index as usize)?
        };
        let configuration = bits.read(4)?;
        let count = *[0, 1, 2, 3, 4, 5, 6, 8].get(configuration as usize)?;
        if rate != sample_rate
            || count == 0
            || count != u32::from(channels)
            || bits.read(1)? != 0
            || bits.read(1)? != 0
            || bits.read(1)? != 0
        {
            return None;
        }
        if bits.remaining() >= 16 {
            let extension = bits.read(11)?;
            if extension != 0 && (extension != 0x2b7 || bits.read(5)? != 5 || bits.read(1)? != 0) {
                return None;
            }
        }
        while let Some(value) = bits.read(1) {
            if value != 0 {
                return None;
            }
        }
        Some(())
    })() else {
        return false;
    };
    true
}

pub(super) fn opus(bytes: &[u8], channels: u16) -> bool {
    bytes.len() == 19
        && bytes.starts_with(b"OpusHead")
        && bytes[8] == 1
        && u16::from(bytes[9]) == channels
        && [1, 2].contains(&channels)
        && bytes[18] == 0
}
