// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::TransportError;

pub(super) struct Playlist {
    pub sequence: u64,
    pub segments: Vec<String>,
}

pub(super) fn segment(name: &str) -> bool {
    name.strip_prefix("seg_")
        .and_then(|s| s.strip_suffix(".m4s"))
        .is_some_and(|s| s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()))
}

pub(super) fn parse(text: &str) -> Result<Option<Playlist>, TransportError> {
    if !text.ends_with('\n') {
        return Ok(None);
    }
    let mut lines = text.lines();
    if lines.next() != Some("#EXTM3U") {
        return Err(TransportError::Cache);
    }
    let (mut sequence, mut target, mut map, mut independent, mut pending) =
        (None, false, false, false, false);
    let mut segments = Vec::new();
    for line in lines {
        if line.len() > 256 {
            return Err(TransportError::Cache);
        }
        if let Some(value) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            if sequence.is_some() {
                return Err(TransportError::Cache);
            }
            sequence = Some(value.parse().map_err(|_| TransportError::Cache)?);
        } else if let Some(value) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            if target || !value.parse::<u32>().is_ok_and(|n| (1..=4).contains(&n)) {
                return Err(TransportError::Cache);
            }
            target = true;
        } else if line == "#EXT-X-MAP:URI=\"init.mp4\"" {
            if map {
                return Err(TransportError::Cache);
            }
            map = true;
        } else if line == "#EXT-X-INDEPENDENT-SEGMENTS" {
            independent = true;
        } else if let Some(value) = line.strip_prefix("#EXTINF:") {
            let duration = value.strip_suffix(',').and_then(|s| s.parse::<f64>().ok());
            if pending || !duration.is_some_and(|d| d.is_finite() && d > 0.0 && d <= 4.0) {
                return Err(TransportError::Cache);
            }
            pending = true;
        } else if line.starts_with("#EXT-X-VERSION:") || line.is_empty() {
        } else if segment(line) && pending && !segments.iter().any(|s| s == line) {
            segments.push(line.to_string());
            pending = false;
        } else {
            return Err(TransportError::Cache);
        }
    }
    if segments.len() > 15 {
        return Err(TransportError::Cache);
    }
    if pending || !target || !map || !independent || segments.len() < 2 {
        return Ok(None);
    }
    Ok(Some(Playlist {
        sequence: sequence.ok_or(TransportError::Cache)?,
        segments,
    }))
}
