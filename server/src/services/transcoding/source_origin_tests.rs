// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;

fn atom(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(((payload.len() + 8) as u32).to_be_bytes());
    bytes.extend(kind);
    bytes.extend(payload);
    bytes
}

fn initialization(ids: [u32; 2], scales: [u32; 2]) -> Vec<u8> {
    let mut movie = Vec::new();
    for (index, handler) in [b"vide", b"soun"].into_iter().enumerate() {
        let mut tkhd = vec![0; 16];
        tkhd[12..16].copy_from_slice(&ids[index].to_be_bytes());
        let mut mdhd = vec![0; 16];
        mdhd[12..16].copy_from_slice(&scales[index].to_be_bytes());
        let mut hdlr = vec![0; 12];
        hdlr[8..12].copy_from_slice(handler);
        let mut media = atom(b"mdhd", &mdhd);
        media.extend(atom(b"hdlr", &hdlr));
        let mut track = atom(b"tkhd", &tkhd);
        track.extend(atom(b"mdia", &media));
        movie.extend(atom(b"trak", &track));
    }
    atom(b"moov", &movie)
}

fn fragment(ids: [u32; 2], times: [u64; 2], wide: bool) -> Vec<u8> {
    let mut moof = Vec::new();
    for (id, time) in ids.into_iter().zip(times) {
        let mut tfhd = vec![0, 2, 0, 0];
        tfhd.extend(id.to_be_bytes());
        let mut tfdt = vec![u8::from(wide), 0, 0, 0];
        if wide {
            tfdt.extend(time.to_be_bytes());
        } else {
            tfdt.extend((time as u32).to_be_bytes());
        }
        let mut track = atom(b"tfhd", &tfhd);
        track.extend(atom(b"tfdt", &tfdt));
        track.extend(atom(b"trun", &[0, 0, 0, 0, 0, 0, 0, 1]));
        moof.extend(atom(b"traf", &track));
    }
    moof
}

fn delayed_initialization(delays_ms: [u32; 2]) -> Vec<u8> {
    let initialization = initialization([1, 2], [16_000, 48_000]);
    let root = atoms(&initialization).unwrap();
    let movie = atoms(unique(&root, b"moov").unwrap()).unwrap();
    let mut tracks = Vec::new();
    for (track, delay) in movie.iter().zip(delays_ms) {
        let mut entries = vec![0; 4];
        entries.extend(2_u32.to_be_bytes());
        for (duration, media_time) in [(delay, -1_i32), (0, 0)] {
            entries.extend(duration.to_be_bytes());
            entries.extend(media_time.to_be_bytes());
            entries.extend([0, 1, 0, 0]);
        }
        let mut payload = track.payload.to_vec();
        payload.extend(atom(b"edts", &atom(b"elst", &entries)));
        tracks.extend(atom(b"trak", &payload));
    }
    atom(b"moov", &tracks)
}

#[test]
fn zero_fragment_dts_with_delay_edits_cannot_claim_an_absolute_source_origin() {
    let fragment = fragment([1, 2], [0, 0], true);
    assert!(origin(&delayed_initialization([4000, 4978]), &fragment, 5000).is_err());
    assert!(origin(&delayed_initialization([0, 0]), &fragment, 0).is_err());
}

#[test]
fn raw_fragment_dts_preserves_positive_seek_and_common_initial_shift_without_edits() {
    let initialization = initialization([1, 2], [16_000, 48_000]);
    assert_eq!(
        origin(
            &initialization,
            &fragment([1, 2], [64_000, 238_976], true),
            5000
        )
        .unwrap(),
        4000
    );
    assert_eq!(
        origin(&initialization, &fragment([1, 2], [302, 0], true), 0).unwrap(),
        0
    );
}

#[test]
fn copied_aac_can_precede_reordered_video_in_the_normalized_origin() {
    let initialization = initialization([1, 2], [12_288, 48_000]);
    assert_eq!(
        origin(
            &initialization,
            &fragment([1, 2], [48_128, 187_392], true),
            5000
        )
        .unwrap(),
        3904
    );
    assert_eq!(
        origin(
            &initialization,
            &fragment([1, 2], [48_128, 238_976], true),
            5000
        )
        .unwrap(),
        3916
    );
    assert_eq!(
        origin(&initialization, &fragment([1, 2], [0, 2976], true), 0).unwrap(),
        0
    );
}

#[test]
fn earliest_decode_timestamp_matches_hls_normalization_across_timescales() {
    let init = initialization([7, 99], [24_000, 48_000]);
    assert_eq!(
        origin(&init, &fragment([7, 99], [96_000, 238_944], true), 5000).unwrap(),
        4000
    );
    assert_eq!(
        origin(&init, &fragment([7, 99], [94_000, 192_000], true), 5000).unwrap(),
        3916
    );
    assert_eq!(
        origin(&init, &fragment([7, 99], [96_000, 191_999], false), 5000).unwrap(),
        3999
    );
    assert_eq!(
        origin(&init, &fragment([7, 99], [0, 0], false), 0).unwrap(),
        0
    );
}

#[test]
fn unknown_duplicate_missing_and_zero_timescale_tracks_fail_closed() {
    let moof = fragment([7, 99], [0, 0], false);
    assert!(origin(&initialization([7, 99], [0, 48_000]), &moof, 0).is_err());
    assert!(origin(&initialization([7, 7], [24_000, 48_000]), &moof, 0).is_err());
    let init = initialization([7, 99], [24_000, 48_000]);
    assert!(origin(&init, &fragment([7, 100], [0, 0], false), 0).is_err());
    assert!(origin(&init, &fragment([7, 7], [0, 0], false), 0).is_err());
    let first_traf = atoms(&moof).unwrap()[0].payload.to_vec();
    assert!(origin(&init, &atom(b"traf", &first_traf), 0).is_err());
}

#[test]
fn negative_wrapped_and_later_than_requested_origins_are_never_relabeled_zero() {
    let init = initialization([1, 2], [24_000, 48_000]);
    assert!(origin(&init, &fragment([1, 2], [u64::MAX, u64::MAX], true), 5000).is_err());
    assert!(origin(&init, &fragment([1, 2], [u64::MAX, 0], true), 5000).is_err());
    assert!(origin(&init, &fragment([1, 2], [144_000, 288_000], true), 5000).is_err());
    assert!(origin(&init, &fragment([1, 2], [1, 1], false), -1).is_err());
}

#[test]
fn truncated_oversized_and_unbounded_atom_records_are_rejected() {
    let init = initialization([1, 2], [24_000, 48_000]);
    let mut moof = fragment([1, 2], [0, 0], true);
    moof.pop();
    assert!(origin(&init, &moof, 0).is_err());
    let malformed = [
        0, 0, 0, 1, b'm', b'o', b'o', b'f', 255, 255, 255, 255, 255, 255, 255, 255,
    ];
    assert!(atoms(&malformed).is_err());
    let mut boxes = Vec::new();
    for _ in 0..257 {
        boxes.extend(atom(b"free", &[]));
    }
    assert!(atoms(&boxes).is_err());
    assert!(origin(&vec![0; INIT_LIMIT as usize + 1], &[], 0).is_err());
    assert!(origin(&init, &vec![0; MOOF_LIMIT as usize + 1], 0).is_err());
}

#[test]
fn fragment_run_integrity_is_required_before_timestamp_admission() {
    assert!(samples(&[0, 0, 0, 0, 0, 0, 0, 0]).is_err());
    assert!(samples(&[0, 0, 1, 0, 0, 0, 0, 2, 0, 0, 0, 1]).is_err());
    assert!(samples(&[0, 0, 16, 0, 0, 0, 0, 1]).is_err());
    assert!(decode_time(&[1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
    assert!(track_id(&[0, 2, 0, 1, 0, 0, 0, 1]).is_err());
}
