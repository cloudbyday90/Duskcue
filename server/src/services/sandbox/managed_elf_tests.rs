// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;

fn fixture() -> Vec<u8> {
    let mut bytes = vec![0; 2048];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    bytes[16..18].copy_from_slice(&3_u16.to_le_bytes());
    bytes[18..20].copy_from_slice(
        &(if cfg!(target_arch = "x86_64") {
            62_u16
        } else {
            183
        })
        .to_le_bytes(),
    );
    bytes[20..24].copy_from_slice(&1_u32.to_le_bytes());
    bytes[32..40].copy_from_slice(&64_u64.to_le_bytes());
    bytes[54..56].copy_from_slice(&56_u16.to_le_bytes());
    bytes[56..58].copy_from_slice(&3_u16.to_le_bytes());
    for (index, (kind, offset, size)) in [(1_u32, 0_u64, 2048_u64), (2, 512, 80), (3, 1024, 24)]
        .into_iter()
        .enumerate()
    {
        let start = 64 + index * 56;
        bytes[start..start + 4].copy_from_slice(&kind.to_le_bytes());
        bytes[start + 8..start + 16].copy_from_slice(&offset.to_le_bytes());
        bytes[start + 16..start + 24].copy_from_slice(&(0x400000 + offset).to_le_bytes());
        bytes[start + 32..start + 40].copy_from_slice(&size.to_le_bytes());
    }
    for (index, (tag, value)) in [(1_u64, 0_u64), (5, 0x400300), (10, 200), (25, 0x400400)]
        .into_iter()
        .enumerate()
    {
        let start = 512 + index * 16;
        bytes[start..start + 8].copy_from_slice(&tag.to_le_bytes());
        bytes[start + 8..start + 16].copy_from_slice(&value.to_le_bytes());
    }
    bytes[768..768 + BOOTSTRAP.len()].copy_from_slice(BOOTSTRAP.as_bytes());
    let interpreter = if cfg!(target_arch = "x86_64") {
        b"/lib/ld-musl-x86_64.so.1\0".as_slice()
    } else {
        b"/lib/ld-musl-aarch64.so.1\0".as_slice()
    };
    bytes[1024..1024 + interpreter.len()].copy_from_slice(interpreter);
    bytes[64 + 2 * 56 + 32..64 + 2 * 56 + 40]
        .copy_from_slice(&(interpreter.len() as u64).to_le_bytes());
    bytes
}

fn with_fixture(bytes: &[u8], inspect: impl FnOnce(&File)) {
    let path = std::env::temp_dir().join(format!("duskcue-managed-elf-{}", uuid::Uuid::now_v7()));
    fs::write(&path, bytes).unwrap();
    let file = File::open(&path).unwrap();
    inspect(&file);
    drop(file);
    fs::remove_file(path).unwrap();
}

#[test]
fn refuses_missing_mandatory_dependency_architecture_and_truncated_tables() {
    let original = fixture();
    for offset in [0, 4, 5, 18, 54, 56, 768] {
        let mut invalid = original.clone();
        invalid[offset] = 255;
        with_fixture(&invalid, |file| {
            assert!(validate_elf(file, true).is_err(), "offset {offset}")
        });
    }
    with_fixture(&original[..80], |file| {
        assert!(validate_elf(file, true).is_err())
    });
    let mut invalid = original.clone();
    invalid[512..520].copy_from_slice(&0_u64.to_le_bytes());
    with_fixture(&invalid, |file| assert!(validate_elf(file, true).is_err()));
}

#[test]
fn bootstrap_requires_a_constructor_and_forbids_search_path_overrides() {
    let mut bytes = fixture();
    with_fixture(&bytes, |file| validate_elf(file, false).unwrap());
    bytes[512 + 3 * 16..512 + 3 * 16 + 8].copy_from_slice(&0_u64.to_le_bytes());
    with_fixture(&bytes, |file| assert!(validate_elf(file, false).is_err()));
    for tag in [15_u64, 29] {
        let mut invalid = fixture();
        invalid[512..520].copy_from_slice(&tag.to_le_bytes());
        with_fixture(&invalid, |file| assert!(validate_elf(file, false).is_err()));
    }
}

#[test]
fn string_mapping_and_reads_reject_overflow_and_oversized_sections() {
    let segments = [Segment {
        kind: 1,
        offset: 0,
        address: 0x400000,
        size: 100,
    }];
    assert_eq!(virtual_offset(&segments, 0x400010, 10).unwrap(), 16);
    assert!(virtual_offset(&segments, 0x400060, 10).is_err());
    assert!(virtual_offset(&segments, u64::MAX, 10).is_err());
    with_fixture(&fixture(), |file| {
        assert!(read(file, u64::MAX, 1).is_err());
        assert!(read(file, 0, MAX_SECTION + 1).is_err());
        assert!(read(file, 2000, 49).is_err());
    });
}
