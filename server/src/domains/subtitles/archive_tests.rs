// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::io::{Cursor, Read, Write};

use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use super::{
    MAX_ARCHIVE_BYTES, MAX_ARCHIVE_ENTRIES, MAX_SUBTITLE_BYTES, extract_subtitle_from_zip,
    read_subtitle,
};

const CONTENT: &[u8] = b"1\n00:00:00,000 --> 00:00:02,000\nBounded subtitle\n";

fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in files {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(contents).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn central_header(bytes: &[u8]) -> usize {
    bytes
        .windows(4)
        .position(|signature| signature == b"PK\x01\x02")
        .unwrap()
}

#[test]
fn returns_compressed_subtitle_contents_and_preserves_supported_extensions() {
    for extension in ["srt", "vtt", "ass", "ssa", "ttml"] {
        let name = format!("nested/subtitle.{}", extension.to_uppercase());
        assert_eq!(
            extract_subtitle_from_zip(&archive(&[("readme.txt", b"ignored"), (&name, CONTENT)]))
                .unwrap(),
            CONTENT
        );
    }
}

#[test]
fn invalid_empty_and_non_subtitle_archives_fail_without_filesystem_extraction() {
    assert!(extract_subtitle_from_zip(b"not a ZIP archive").is_err());
    assert!(
        extract_subtitle_from_zip(&archive(&[]))
            .unwrap_err()
            .to_string()
            .contains("no subtitle files")
    );
    assert!(
        extract_subtitle_from_zip(&archive(&[("cover.jpg", b"not subtitles")]))
            .unwrap_err()
            .to_string()
            .contains("no subtitle files")
    );
    let mut truncated = archive(&[("movie.srt", CONTENT)]);
    truncated.truncate(10);
    assert!(extract_subtitle_from_zip(&truncated).is_err());
}

#[test]
fn archive_input_and_entry_count_limits_are_enforced() {
    assert!(
        extract_subtitle_from_zip(&vec![0; MAX_ARCHIVE_BYTES + 1])
            .unwrap_err()
            .to_string()
            .contains("compressed input limit")
    );
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..=MAX_ARCHIVE_ENTRIES {
        writer
            .start_file(
                format!("{index}.txt"),
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
    }
    let too_many = writer.finish().unwrap().into_inner();
    assert!(
        extract_subtitle_from_zip(&too_many)
            .unwrap_err()
            .to_string()
            .contains("entry limit")
    );
}

#[test]
fn advertised_uncompressed_size_cannot_authorize_oversized_subtitles() {
    let mut bytes = archive(&[("movie.srt", CONTENT)]);
    let header = central_header(&bytes);
    bytes[header + 24..header + 28]
        .copy_from_slice(&((MAX_SUBTITLE_BYTES + 1) as u32).to_le_bytes());
    assert!(
        extract_subtitle_from_zip(&bytes)
            .unwrap_err()
            .to_string()
            .contains("uncompressed size limit")
    );
}

#[test]
fn actual_read_limit_applies_even_when_advertised_size_is_small() {
    let exact = vec![b'x'; MAX_SUBTITLE_BYTES as usize];
    assert_eq!(
        read_subtitle(Cursor::new(&exact), 1).unwrap().len(),
        exact.len()
    );
    let oversized = std::io::repeat(b'x').take(MAX_SUBTITLE_BYTES + 1);
    assert!(
        read_subtitle(oversized, 1)
            .unwrap_err()
            .to_string()
            .contains("uncompressed size limit")
    );
}

#[test]
fn underreported_compressed_zip_payload_cannot_exceed_the_read_bound() {
    let contents = vec![b'x'; MAX_SUBTITLE_BYTES as usize + 1];
    let mut bytes = archive(&[("movie.srt", &contents)]);
    let header = central_header(&bytes);
    bytes[header + 24..header + 28].copy_from_slice(&(MAX_SUBTITLE_BYTES as u32).to_le_bytes());
    assert!(extract_subtitle_from_zip(&bytes).is_err());
}

#[test]
fn traversal_absolute_directory_and_symlink_entries_are_skipped() {
    for name in [
        "../../outside.srt",
        "..\\..\\outside.srt",
        "/outside.srt",
        "\\outside.srt",
        "C:/outside.srt",
        "C:outside.srt",
        "directory.srt/",
    ] {
        assert_eq!(
            extract_subtitle_from_zip(&archive(&[(name, b"ignored"), ("safe/movie.srt", CONTENT)]))
                .unwrap(),
            CONTENT
        );
        assert!(extract_subtitle_from_zip(&archive(&[(name, b"ignored")])).is_err());
    }
    let mut bytes = archive(&[("link.srt", b"../../outside"), ("movie.srt", CONTENT)]);
    let header = central_header(&bytes);
    bytes[header + 5] = 3;
    bytes[header + 38..header + 42].copy_from_slice(&(0o120777_u32 << 16).to_le_bytes());
    assert_eq!(extract_subtitle_from_zip(&bytes).unwrap(), CONTENT);
}
