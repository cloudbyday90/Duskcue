// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#![cfg(test)]

use std::io::{Read, Write};

#[test]
fn isolated_miniz_backend_preserves_payloads_and_refuses_incomplete_streams() {
    let input = (0..=255).cycle().take(128 * 1024).collect::<Vec<u8>>();
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&input).unwrap();
    let gzip = gzip.finish().unwrap();
    let mut plain = Vec::new();
    flate2::read::GzDecoder::new(gzip.as_slice())
        .read_to_end(&mut plain)
        .unwrap();
    assert_eq!(plain, input);
    for n in 1..=8 {
        assert!(
            flate2::read::GzDecoder::new(&gzip[..gzip.len() - n])
                .read_to_end(&mut Vec::new())
                .is_err()
        );
    }
    let mut corrupt = gzip.clone();
    let crc = corrupt.len() - 8;
    corrupt[crc] ^= 1;
    assert!(
        flate2::read::GzDecoder::new(corrupt.as_slice())
            .read_to_end(&mut Vec::new())
            .is_err()
    );
    let mut deflate =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    deflate.write_all(&input).unwrap();
    let deflate = deflate.finish().unwrap();
    plain.clear();
    flate2::read::DeflateDecoder::new(deflate.as_slice())
        .read_to_end(&mut plain)
        .unwrap();
    assert_eq!(plain, input);
    assert!(
        flate2::read::DeflateDecoder::new(&deflate[..deflate.len() - 1])
            .read_to_end(&mut Vec::new())
            .is_err()
    );
    let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    zlib.write_all(&input).unwrap();
    let zlib = zlib.finish().unwrap();
    plain.clear();
    flate2::read::ZlibDecoder::new(zlib.as_slice())
        .read_to_end(&mut plain)
        .unwrap();
    assert_eq!(plain, input);
    assert!(
        flate2::read::ZlibDecoder::new(&zlib[..zlib.len() - 1])
            .read_to_end(&mut Vec::new())
            .is_err()
    );
}
