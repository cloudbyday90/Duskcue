// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::collections::BTreeMap;
use std::path::Path;

use tokio::io::{AsyncReadExt, AsyncSeekExt};

use super::{OwnedCache, playlist, stable};
use crate::domains::playback::PlaybackError;

const INIT_LIMIT: u64 = 512 * 1024;
const MOOF_LIMIT: u64 = 512 * 1024 - 4096;
const FRAGMENT_LIMIT: u64 = 512 * 1024 * 1024;

pub(super) async fn inspect(
    directory: &Path,
    manifest: &Path,
    requested_ms: i64,
) -> Result<i64, PlaybackError> {
    let cache = OwnedCache::new(directory, manifest).await?;
    let content = read(&cache, "manifest.m3u8", super::MAX_PLAYLIST_BYTES).await?;
    let content = std::str::from_utf8(&content).map_err(|_| failure())?;
    let references = playlist::parse(content)?.ok_or_else(failure)?;
    let initialization = read(&cache, &references.initialization, INIT_LIMIT).await?;
    let moof = read_moof(&cache, &references.segment).await?;
    origin(&initialization, &moof, requested_ms)
}

async fn read(cache: &OwnedCache, name: &str, limit: u64) -> Result<Vec<u8>, PlaybackError> {
    let mut file = cache.open(name).await?.ok_or_else(failure)?;
    let before = file.metadata().await.map_err(|_| failure())?;
    if before.len() == 0 || before.len() > limit {
        return Err(failure());
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| failure())?;
    let after = file.metadata().await.map_err(|_| failure())?;
    if bytes.len() as u64 != before.len() || !stable(&before, &after) {
        return Err(failure());
    }
    Ok(bytes)
}

async fn read_moof(cache: &OwnedCache, name: &str) -> Result<Vec<u8>, PlaybackError> {
    let mut file = cache.open(name).await?.ok_or_else(failure)?;
    let before = file.metadata().await.map_err(|_| failure())?;
    if before.len() == 0 || before.len() > FRAGMENT_LIMIT {
        return Err(failure());
    }
    let mut offset = 0_u64;
    for _ in 0..256 {
        if before.len().saturating_sub(offset) < 8 {
            return Err(failure());
        }
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|_| failure())?;
        let mut header = [0_u8; 16];
        file.read_exact(&mut header[..8])
            .await
            .map_err(|_| failure())?;
        let small = u32::from_be_bytes(header[..4].try_into().map_err(|_| failure())?);
        let (size, head) = if small == 1 {
            file.read_exact(&mut header[8..])
                .await
                .map_err(|_| failure())?;
            (
                u64::from_be_bytes(header[8..].try_into().map_err(|_| failure())?),
                16_u64,
            )
        } else {
            (u64::from(small), 8)
        };
        if size < head
            || offset
                .checked_add(size)
                .is_none_or(|end| end > before.len())
        {
            return Err(failure());
        }
        if &header[4..8] == b"moof" {
            if size - head > MOOF_LIMIT {
                return Err(failure());
            }
            let mut bytes = vec![0; usize::try_from(size - head).map_err(|_| failure())?];
            file.read_exact(&mut bytes).await.map_err(|_| failure())?;
            let after = file.metadata().await.map_err(|_| failure())?;
            if !stable(&before, &after) {
                return Err(failure());
            }
            return Ok(bytes);
        }
        offset += size;
    }
    Err(failure())
}

struct Atom<'a> {
    kind: [u8; 4],
    payload: &'a [u8],
}

fn atoms(mut bytes: &[u8]) -> Result<Vec<Atom<'_>>, PlaybackError> {
    let mut result = Vec::new();
    while !bytes.is_empty() {
        if result.len() >= 256 || bytes.len() < 8 {
            return Err(failure());
        }
        let small = number(bytes, 0)?;
        let (size, header) = if small == 1 {
            (usize::try_from(long(bytes, 8)?).map_err(|_| failure())?, 16)
        } else {
            (small as usize, 8)
        };
        if size < header || size > bytes.len() {
            return Err(failure());
        }
        result.push(Atom {
            kind: bytes[4..8].try_into().map_err(|_| failure())?,
            payload: &bytes[header..size],
        });
        bytes = &bytes[size..];
    }
    Ok(result)
}

fn unique<'a>(atoms: &[Atom<'a>], kind: &[u8; 4]) -> Result<&'a [u8], PlaybackError> {
    let mut matching = atoms.iter().filter(|atom| &atom.kind == kind);
    let first = matching.next().ok_or_else(failure)?;
    if matching.next().is_some() {
        return Err(failure());
    }
    Ok(first.payload)
}

fn number(bytes: &[u8], offset: usize) -> Result<u32, PlaybackError> {
    Ok(u32::from_be_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(failure)?
            .try_into()
            .map_err(|_| failure())?,
    ))
}

fn long(bytes: &[u8], offset: usize) -> Result<u64, PlaybackError> {
    Ok(u64::from_be_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or_else(failure)?
            .try_into()
            .map_err(|_| failure())?,
    ))
}

fn versioned_number(bytes: &[u8]) -> Result<u32, PlaybackError> {
    match bytes.first() {
        Some(0) => number(bytes, 12),
        Some(1) => number(bytes, 20),
        _ => Err(failure()),
    }
}

fn tracks(initialization: &[u8]) -> Result<BTreeMap<u32, u32>, PlaybackError> {
    let root = atoms(initialization)?;
    let movie = atoms(unique(&root, b"moov")?)?;
    let mut result = BTreeMap::new();
    let mut handlers = Vec::new();
    for track in movie.iter().filter(|atom| &atom.kind == b"trak") {
        if result.len() >= 2 {
            return Err(failure());
        }
        let boxes = atoms(track.payload)?;
        for edits in boxes.iter().filter(|atom| &atom.kind == b"edts") {
            if atoms(edits.payload)?
                .iter()
                .any(|atom| &atom.kind == b"elst")
            {
                return Err(failure());
            }
        }
        let id = versioned_number(unique(&boxes, b"tkhd")?)?;
        let media = atoms(unique(&boxes, b"mdia")?)?;
        let scale = versioned_number(unique(&media, b"mdhd")?)?;
        let handler: [u8; 4] = unique(&media, b"hdlr")?
            .get(8..12)
            .ok_or_else(failure)?
            .try_into()
            .map_err(|_| failure())?;
        if id == 0
            || !(1..=1_000_000_000).contains(&scale)
            || (handler != *b"vide" && handler != *b"soun")
            || result.insert(id, scale).is_some()
            || handlers.contains(&handler)
        {
            return Err(failure());
        }
        handlers.push(handler);
    }
    if result.len() != 2 {
        return Err(failure());
    }
    Ok(result)
}

fn track_id(bytes: &[u8]) -> Result<u32, PlaybackError> {
    let flags = number(bytes, 0)?;
    if flags >> 24 != 0 || flags & !0x0003_003b != 0 {
        return Err(failure());
    }
    let mut length = 8_usize;
    for (flag, bytes) in [(1, 8), (2, 4), (8, 4), (16, 4), (32, 4)] {
        if flags & flag != 0 {
            length += bytes;
        }
    }
    if length != bytes.len() {
        return Err(failure());
    }
    number(bytes, 4)
}

fn decode_time(bytes: &[u8]) -> Result<u64, PlaybackError> {
    if number(bytes, 0)? & 0x00ff_ffff != 0 {
        return Err(failure());
    }
    match bytes.first() {
        Some(0) if bytes.len() == 8 => Ok(u64::from(number(bytes, 4)?)),
        Some(1) if bytes.len() == 12 => long(bytes, 4),
        _ => Err(failure()),
    }
}

fn samples(bytes: &[u8]) -> Result<(), PlaybackError> {
    let flags = number(bytes, 0)?;
    let count = number(bytes, 4)?;
    if flags >> 24 > 1 || flags & 0x00ff_ffff & !0x0000_0f05 != 0 || !(1..=65_536).contains(&count)
    {
        return Err(failure());
    }
    let per_sample = [0x100, 0x200, 0x400, 0x800]
        .into_iter()
        .filter(|flag| flags & flag != 0)
        .count()
        * 4;
    let expected = 8
        + usize::from(flags & 1 != 0) * 4
        + usize::from(flags & 4 != 0) * 4
        + usize::try_from(count)
            .map_err(|_| failure())?
            .checked_mul(per_sample)
            .ok_or_else(failure)?;
    if expected != bytes.len() {
        return Err(failure());
    }
    Ok(())
}

fn origin(initialization: &[u8], moof: &[u8], requested_ms: i64) -> Result<i64, PlaybackError> {
    if initialization.len() as u64 > INIT_LIMIT
        || moof.len() as u64 > MOOF_LIMIT
        || requested_ms < 0
    {
        return Err(failure());
    }
    let tracks = tracks(initialization)?;
    let boxes = atoms(moof)?;
    let mut found = BTreeMap::new();
    for fragment in boxes.iter().filter(|atom| &atom.kind == b"traf") {
        if found.len() >= 2 {
            return Err(failure());
        }
        let records = atoms(fragment.payload)?;
        let id = track_id(unique(&records, b"tfhd")?)?;
        let scale = *tracks.get(&id).ok_or_else(failure)?;
        let time = decode_time(unique(&records, b"tfdt")?)?;
        if u128::from(time) * 1000 / u128::from(scale) > i32::MAX as u128 {
            return Err(failure());
        }
        let runs = records
            .iter()
            .filter(|atom| &atom.kind == b"trun")
            .collect::<Vec<_>>();
        if runs.is_empty() || found.insert(id, (time, scale)).is_some() {
            return Err(failure());
        }
        for run in runs {
            samples(run.payload)?;
        }
    }
    if found.len() != tracks.len() {
        return Err(failure());
    }
    let milliseconds = found
        .values()
        .map(|(time, scale)| u128::from(*time) * 1000 / u128::from(*scale))
        .min()
        .ok_or_else(failure)?;
    let milliseconds = i64::try_from(milliseconds).map_err(|_| failure())?;
    if milliseconds > requested_ms || milliseconds > i64::from(i32::MAX) {
        return Err(failure());
    }
    Ok(milliseconds)
}

fn failure() -> PlaybackError {
    PlaybackError::FfmpegFailed("source-copy HLS timestamp origin is unavailable".into())
}

#[cfg(test)]
#[path = "source_origin_tests.rs"]
mod tests;
