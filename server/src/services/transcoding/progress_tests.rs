// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::ProgressParser;

#[test]
fn publishes_complete_records_with_microseconds_normalized_to_milliseconds() {
    let mut parser = ProgressParser::default();
    for line in [
        "frame=48",
        "fps=23.98",
        "bitrate=600.0kbits/s",
        "total_size=12000",
        "out_time_us=2000123",
        "out_time_ms=2000123",
        "speed=1.5x",
    ] {
        assert!(parser.push(line).is_none());
    }
    let record = parser.push("progress=continue").unwrap();
    assert_eq!(record.frame, 48);
    assert_eq!(record.out_time_ms, 2000);
    assert_eq!(record.total_size, 12000);
    assert_eq!(record.fps, 23.98);
    assert_eq!(record.speed, 1.5);
    assert_eq!(record.bitrate_kbps, 600.0);
    assert!(!record.is_complete);
}

#[test]
fn malformed_fields_and_missing_values_do_not_erase_a_confirmed_snapshot() {
    let mut parser = ProgressParser::default();
    parser.push("out_time_us=3000000");
    parser.push("frame=72");
    parser.push("progress=continue");
    for line in [
        "out_time_us=-1",
        "out_time_us=N/A",
        "fps=NaN",
        "speed=inf",
        "total_size=oops",
        "progress=garbage",
        "decoder message",
    ] {
        assert!(parser.push(line).is_none());
    }
    let record = parser.push("progress=continue").unwrap();
    assert_eq!(record.out_time_ms, 3000);
    assert_eq!(record.frame, 72);
    assert!(record.fps.is_finite());
    assert!(record.speed.is_finite());
    assert!(
        parser
            .push(&format!("out_time_us={}1", "0".repeat(4096)))
            .is_none()
    );
}

#[test]
fn end_is_an_atomic_record_and_trailing_lines_cannot_reopen_it() {
    let mut parser = ProgressParser::default();
    parser.push("out_time_us=4000000");
    let record = parser.push("progress=end").unwrap();
    assert!(record.is_complete);
    assert_eq!(record.out_time_ms, 4000);
    assert!(parser.push("out_time_us=5000000").is_none());
    assert!(parser.push("progress=continue").is_none());
}
