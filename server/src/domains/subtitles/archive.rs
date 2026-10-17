// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::io::{Cursor, Read};

use super::SubtitleError;

const MAX_ARCHIVE_BYTES: usize = 32 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 1024;
const MAX_SUBTITLE_BYTES: u64 = 8 * 1024 * 1024;
const SUBTITLE_EXTENSIONS: &[&str] = &[".srt", ".vtt", ".ass", ".ssa", ".ttml"];

pub(super) fn extract_subtitle_from_zip(data: &[u8]) -> Result<Vec<u8>, SubtitleError> {
    if data.len() > MAX_ARCHIVE_BYTES {
        return Err(failure(
            "ZIP archive exceeds the 32 MiB compressed input limit",
        ));
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(data))
        .map_err(|error| failure(format!("ZIP parse error: {error}")))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(failure("ZIP archive exceeds the 1024 entry limit"));
    }
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| failure(format!("ZIP entry read error: {error}")))?;
        if !entry.is_file() {
            continue;
        }
        let name = entry
            .name()
            .map_err(|error| failure(format!("ZIP entry name error: {error}")))?
            .to_ascii_lowercase();
        if unsafe_archive_path(&name) || entry.enclosed_name().is_none() {
            continue;
        }
        if SUBTITLE_EXTENSIONS
            .iter()
            .any(|extension| name.ends_with(extension))
        {
            let advertised_size = entry.size();
            return read_subtitle(&mut entry, advertised_size);
        }
    }
    Err(failure("ZIP archive contains no subtitle files"))
}

fn unsafe_archive_path(name: &str) -> bool {
    let bytes = name.as_bytes();
    name.starts_with(['/', '\\'])
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
        || name.split(['/', '\\']).any(|component| component == "..")
}

fn read_subtitle(reader: impl Read, advertised_size: u64) -> Result<Vec<u8>, SubtitleError> {
    if advertised_size > MAX_SUBTITLE_BYTES {
        return Err(failure(
            "ZIP subtitle exceeds the 8 MiB uncompressed size limit",
        ));
    }
    let mut bytes = Vec::new();
    reader
        .take(MAX_SUBTITLE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| failure(format!("ZIP content read error: {error}")))?;
    if bytes.len() as u64 > MAX_SUBTITLE_BYTES {
        return Err(failure(
            "ZIP subtitle exceeds the 8 MiB uncompressed size limit",
        ));
    }
    Ok(bytes)
}

fn failure(reason: impl Into<String>) -> SubtitleError {
    SubtitleError::FetchFailed {
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
