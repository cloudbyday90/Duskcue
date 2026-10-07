// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::io;

pub(super) const HEADER_BYTES: usize = 40;
pub(super) const MAX_INSTRUCTIONS: usize = 4096;
pub(super) const ACK_BYTES: usize = 40;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FilesystemEnforcement {
    None,
    Partial,
    Full,
}

pub(super) fn audit_arch() -> u32 {
    #[cfg(target_arch = "x86_64")]
    {
        0xc000_003e
    }
    #[cfg(target_arch = "aarch64")]
    {
        0xc000_00b7
    }
}

pub(super) fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    })
}

pub(super) fn encode(filter: &seccompiler::BpfProgram, nonce: u64) -> io::Result<Vec<u8>> {
    if filter.is_empty() || filter.len() > MAX_INSTRUCTIONS {
        return Err(io::Error::other("invalid bootstrap instruction count"));
    }
    let mut bytes = vec![0; HEADER_BYTES];
    for instruction in filter {
        bytes.extend_from_slice(&instruction.code.to_le_bytes());
        bytes.extend([instruction.jt, instruction.jf]);
        bytes.extend_from_slice(&instruction.k.to_le_bytes());
    }
    bytes[..8].copy_from_slice(b"DUSKSC01");
    bytes[8..12].copy_from_slice(&1_u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&audit_arch().to_le_bytes());
    bytes[16..20].copy_from_slice(&(filter.len() as u32).to_le_bytes());
    bytes[24..32].copy_from_slice(&nonce.to_le_bytes());
    let hash = checksum(&bytes);
    bytes[32..40].copy_from_slice(&hash.to_le_bytes());
    Ok(bytes)
}

pub(super) fn decode_ack(bytes: &[u8; ACK_BYTES], nonce: u64) -> io::Result<FilesystemEnforcement> {
    if &bytes[..8] != b"DUSKFS01"
        || bytes[9] != 1
        || bytes[10..16] != [0; 6]
        || &bytes[16..24] != b"DUSKACK1"
        || bytes[24..32] != nonce.to_le_bytes()
        || bytes[32..36] != audit_arch().to_le_bytes()
        || bytes[36..40] != 1_u32.to_le_bytes()
    {
        return Err(io::Error::other("invalid owned bootstrap acknowledgment"));
    }
    match bytes[8] {
        0 => Ok(FilesystemEnforcement::None),
        1 => Ok(FilesystemEnforcement::Partial),
        2 => Ok(FilesystemEnforcement::Full),
        _ => Err(io::Error::other("invalid filesystem enforcement status")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acknowledgment(nonce: u64) -> [u8; ACK_BYTES] {
        let mut bytes = [0; ACK_BYTES];
        bytes[..8].copy_from_slice(b"DUSKFS01");
        bytes[8] = 1;
        bytes[9] = 1;
        bytes[16..24].copy_from_slice(b"DUSKACK1");
        bytes[24..32].copy_from_slice(&nonce.to_le_bytes());
        bytes[32..36].copy_from_slice(&audit_arch().to_le_bytes());
        bytes[36..40].copy_from_slice(&1_u32.to_le_bytes());
        bytes
    }

    #[test]
    fn acknowledgment_requires_owned_nonce_arch_and_both_restrictions() {
        let valid = acknowledgment(123);
        assert_eq!(
            decode_ack(&valid, 123).unwrap(),
            FilesystemEnforcement::Partial
        );
        assert!(decode_ack(&valid, 124).is_err());
        for offset in [0, 8, 9, 10, 16, 24, 32, 36] {
            let mut invalid = valid;
            invalid[offset] = 255;
            assert!(decode_ack(&invalid, 123).is_err(), "offset {offset}");
        }
    }

    #[test]
    fn policy_wire_is_bounded_and_uses_actual_instruction_bytes() {
        assert!(encode(&Vec::new(), 1).is_err());
        let filter = super::super::filter::build_ffmpeg_filter().unwrap();
        let maximum = vec![filter[0].clone(); MAX_INSTRUCTIONS];
        assert_eq!(
            encode(&maximum, 1).unwrap().len(),
            HEADER_BYTES + MAX_INSTRUCTIONS * 8
        );
        assert!(encode(&vec![filter[0].clone(); MAX_INSTRUCTIONS + 1], 1).is_err());
        let bytes = encode(&filter, 123).unwrap();
        assert_eq!(bytes.len(), HEADER_BYTES + filter.len() * 8);
        assert_eq!(&bytes[24..32], &123_u64.to_le_bytes());
        let mut checked = bytes.clone();
        checked[32..40].fill(0);
        assert_eq!(&bytes[32..40], &checksum(&checked).to_le_bytes());
        checked[24] ^= 1;
        assert_ne!(&bytes[32..40], &checksum(&checked).to_le_bytes());
    }
}
