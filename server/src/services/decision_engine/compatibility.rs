// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::HashSet;

static CODEC_ALIASES: &[(&str, &[&str])] = &[
    ("h264", &["h264", "avc", "avc1"]),
    ("hevc", &["hevc", "h265", "hvc1", "hev1"]),
    ("av1", &["av1", "av01"]),
    ("vp9", &["vp9", "vp09"]),
    ("mpeg2video", &["mpeg2video", "mpeg2"]),
    ("mpeg4", &["mpeg4", "mp4v"]),
    ("vc1", &["vc1", "vc-1"]),
    ("vp8", &["vp8"]),
    ("aac", &["aac"]),
    ("ac3", &["ac3", "ac-3"]),
    ("eac3", &["eac3", "e-ac-3", "dd+", "dolby_digital_plus"]),
    ("truehd", &["truehd", "thd", "thd+atmos"]),
    ("dts", &["dts"]),
    ("dts_hd_ma", &["dts-hd_ma", "dts_hd_ma"]),
    ("dts_hd_hra", &["dts-hd_hra", "dts_hd_hra"]),
    ("flac", &["flac"]),
    ("opus", &["opus"]),
    ("vorbis", &["vorbis"]),
    ("pcm", &["pcm", "lpcm"]),
    ("alac", &["alac"]),
    ("mp3", &["mp3"]),
];

pub fn codec_supported(codec: &str, supported: &HashSet<String>) -> bool {
    let codec = canonical_codec(codec);
    known_identifier(&codec)
        && supported
            .iter()
            .any(|supported_codec| codec == canonical_codec(supported_codec))
}

fn canonical_codec(codec: &str) -> String {
    let normalized = codec.trim().to_ascii_lowercase();
    CODEC_ALIASES
        .iter()
        .find_map(|(canonical, aliases)| {
            aliases
                .contains(&normalized.as_str())
                .then(|| (*canonical).to_string())
        })
        .unwrap_or(normalized)
}

fn known_identifier(identifier: &str) -> bool {
    !matches!(
        identifier,
        "" | "unknown" | "none" | "n/a" | "und" | "unspecified"
    ) && !identifier.contains(',')
}

pub(super) fn container_supported(container: &str, supported: &HashSet<String>) -> bool {
    container.split(',').all(|source| {
        let source = canonical_container(source);
        known_identifier(&source)
            && supported.iter().any(|formats| {
                formats
                    .split(',')
                    .any(|format| source == canonical_container(format))
            })
    })
}

fn canonical_container(container: &str) -> String {
    let normalized = container.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "mkv" => "matroska".to_string(),
        "quicktime" => "mov".to_string(),
        "ts" | "m2ts" | "mts" => "mpegts".to_string(),
        "mpg" | "mpegps" | "mpeg-ps" => "mpeg".to_string(),
        _ => normalized,
    }
}

pub(super) fn hdr_supported(dynamic_range: &str, supported: &HashSet<String>) -> bool {
    let canonical = |format: &str| match format.trim().to_ascii_lowercase().as_str() {
        "hdr10+" => "hdr10_plus".to_string(),
        value => value.to_string(),
    };
    let dynamic_range = canonical(dynamic_range);
    dynamic_range == "sdr"
        || supported
            .iter()
            .any(|format| canonical(format) == dynamic_range)
}
