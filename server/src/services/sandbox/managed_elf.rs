// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::Path;

use ring::digest::{Context, SHA256};

pub(super) const EXECUTABLE: &str = "/usr/local/libexec/duskcue-ffmpeg";
pub(super) const BOOTSTRAP: &str = "/usr/local/lib/duskcue-ffmpeg-bootstrap.so";
const MANIFEST: &str = "/usr/local/libexec/duskcue-ffmpeg.sha256";
const MAX_SECTION: usize = 1_048_576;

struct Segment {
    kind: u32,
    offset: u64,
    address: u64,
    size: u64,
}

pub(super) fn validate() -> io::Result<()> {
    validate_runtime(EXECUTABLE, MANIFEST)
}

#[cfg(test)]
pub(super) fn validate_probe() -> io::Result<()> {
    validate_runtime(
        "/usr/local/libexec/duskcue-ffmpeg-probe",
        "/usr/local/libexec/duskcue-ffmpeg-probe.sha256",
    )
}

fn validate_runtime(executable: &str, manifest_path: &str) -> io::Result<()> {
    let manifest = trusted_file(Path::new(manifest_path), false)?;
    if manifest.metadata()?.len() > 4096 {
        return Err(invalid("oversized managed FFmpeg manifest"));
    }
    let mut text = String::new();
    manifest.take(4097).read_to_string(&mut text)?;
    if text.len() > 4096 {
        return Err(invalid("oversized managed FFmpeg manifest"));
    }
    let lines: Vec<_> = text.lines().collect();
    if lines.len() != 2 {
        return Err(invalid("invalid managed FFmpeg manifest"));
    }
    for (line, path) in lines.iter().zip([executable, BOOTSTRAP]) {
        let (hash, identity) = line
            .split_once("  ")
            .ok_or_else(|| invalid("invalid managed FFmpeg manifest entry"))?;
        if identity != path
            || hash.len() != 64
            || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(invalid("invalid managed FFmpeg file identity"));
        }
        let mut file = trusted_file(Path::new(path), path == executable)?;
        let expected_length = file.metadata()?.len();
        if expected_length > 256 * 1024 * 1024 {
            return Err(invalid("oversized managed FFmpeg file"));
        }
        let mut digest = Context::new(&SHA256);
        let mut buffer = [0; 32768];
        let mut remaining = expected_length;
        while remaining > 0 {
            let capacity = remaining.min(buffer.len() as u64) as usize;
            let count = file.read(&mut buffer[..capacity])?;
            if count == 0 {
                return Err(invalid("managed FFmpeg file changed during validation"));
            }
            digest.update(&buffer[..count]);
            remaining -= count as u64;
        }
        if file.read(&mut buffer[..1])? != 0 {
            return Err(invalid("managed FFmpeg file changed during validation"));
        }
        let actual = digest.finish();
        let expected: Vec<u8> = hash
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap_or(""), 16))
            .collect::<Result<_, _>>()
            .map_err(|_| invalid("invalid managed FFmpeg digest"))?;
        if actual.as_ref() != expected {
            return Err(invalid("managed FFmpeg file digest mismatch"));
        }
        validate_elf(&file, path == executable)?;
    }
    Ok(())
}

fn trusted_file(path: &Path, executable: bool) -> io::Result<File> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        if metadata.file_type().is_symlink() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0
        {
            return Err(invalid("untrusted managed FFmpeg path"));
        }
        if ancestor != path && !metadata.is_dir() {
            return Err(invalid("invalid managed FFmpeg parent"));
        }
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != 0
        || metadata.mode() & 0o022 != 0
        || (executable && metadata.mode() & 0o111 == 0)
    {
        return Err(invalid("untrusted managed FFmpeg file"));
    }
    Ok(file)
}

fn validate_elf(file: &File, executable: bool) -> io::Result<()> {
    let header = read(file, 0, 64)?;
    let machine = if cfg!(target_arch = "x86_64") {
        62
    } else {
        183
    };
    let kind = u16_at(&header, 16);
    if &header[..4] != b"\x7fELF"
        || header[4] != 2
        || header[5] != 1
        || header[6] != 1
        || u16_at(&header, 18) != machine
        || u32_at(&header, 20) != 1
        || !matches!(kind, 2 | 3)
        || (!executable && kind != 3)
    {
        return Err(invalid("managed FFmpeg ELF architecture mismatch"));
    }
    let count = usize::from(u16_at(&header, 56));
    if u16_at(&header, 54) != 56 || count == 0 || count > 512 {
        return Err(invalid("invalid managed FFmpeg program headers"));
    }
    let bytes = read(file, u64_at(&header, 32), count * 56)?;
    let segments: Vec<_> = bytes
        .chunks_exact(56)
        .map(|entry| Segment {
            kind: u32_at(entry, 0),
            offset: u64_at(entry, 8),
            address: u64_at(entry, 16),
            size: u64_at(entry, 32),
        })
        .collect();
    let dynamic = segments
        .iter()
        .find(|segment| segment.kind == 2)
        .ok_or_else(|| invalid("missing managed FFmpeg dynamic table"))?;
    let dynamic_size = usize::try_from(dynamic.size)
        .map_err(|_| invalid("invalid managed FFmpeg dynamic size"))?;
    if dynamic_size == 0 || !dynamic_size.is_multiple_of(16) {
        return Err(invalid("invalid managed FFmpeg dynamic table"));
    }
    let table = read(file, dynamic.offset, dynamic_size)?;
    let mut needed = Vec::new();
    let mut strings = None;
    let mut strings_size = None;
    let mut constructor = false;
    let mut terminated = false;
    for entry in table.chunks_exact(16) {
        match u64_at(entry, 0) {
            0 => {
                terminated = true;
                break;
            }
            1 => needed.push(u64_at(entry, 8)),
            5 => strings = Some(u64_at(entry, 8)),
            10 => strings_size = Some(u64_at(entry, 8)),
            12 | 25 => constructor = u64_at(entry, 8) != 0 || constructor,
            15 | 29 => {
                return Err(invalid(
                    "managed FFmpeg loader search override is forbidden",
                ));
            }
            _ => {}
        }
    }
    if !terminated {
        return Err(invalid("unterminated managed FFmpeg dynamic table"));
    }
    if !executable {
        return if constructor {
            Ok(())
        } else {
            Err(invalid("missing mandatory bootstrap constructor"))
        };
    }
    let size = usize::try_from(
        strings_size.ok_or_else(|| invalid("missing managed FFmpeg string table"))?,
    )
    .map_err(|_| invalid("invalid managed FFmpeg string table"))?;
    let offset = virtual_offset(
        &segments,
        strings.ok_or_else(|| invalid("missing managed FFmpeg string address"))?,
        size,
    )?;
    let strings = read(file, offset, size)?;
    let mut mandatory = 0;
    for index in needed {
        let start =
            usize::try_from(index).map_err(|_| invalid("invalid managed FFmpeg dependency"))?;
        let tail = strings
            .get(start..)
            .ok_or_else(|| invalid("invalid managed FFmpeg dependency"))?;
        let end = tail
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| invalid("unterminated managed FFmpeg dependency"))?;
        if &tail[..end] == BOOTSTRAP.as_bytes() {
            mandatory += 1;
        }
    }
    if mandatory != 1 {
        return Err(invalid(
            "missing unique mandatory FFmpeg bootstrap dependency",
        ));
    }
    let interpreter = segments
        .iter()
        .find(|segment| segment.kind == 3)
        .ok_or_else(|| invalid("missing managed FFmpeg interpreter"))?;
    if interpreter.size == 0 || interpreter.size > 4096 {
        return Err(invalid("invalid managed FFmpeg interpreter"));
    }
    let interpreter = read(file, interpreter.offset, interpreter.size as usize)?;
    let expected = if cfg!(target_arch = "x86_64") {
        b"/lib/ld-musl-x86_64.so.1\0".as_slice()
    } else {
        b"/lib/ld-musl-aarch64.so.1\0".as_slice()
    };
    if interpreter != expected {
        return Err(invalid("unsupported managed FFmpeg interpreter"));
    }
    let interpreter_path = std::str::from_utf8(&interpreter[..interpreter.len() - 1])
        .map_err(|_| invalid("invalid managed FFmpeg interpreter path"))?;
    trusted_file(&fs::canonicalize(interpreter_path)?, true)?;
    Ok(())
}

fn virtual_offset(segments: &[Segment], address: u64, size: usize) -> io::Result<u64> {
    for segment in segments.iter().filter(|segment| segment.kind == 1) {
        if let Some(relative) = address.checked_sub(segment.address)
            && relative
                .checked_add(size as u64)
                .is_some_and(|end| end <= segment.size)
        {
            return segment
                .offset
                .checked_add(relative)
                .ok_or_else(|| invalid("invalid managed FFmpeg section offset"));
        }
    }
    Err(invalid("unmapped managed FFmpeg string table"))
}

fn read(file: &File, offset: u64, size: usize) -> io::Result<Vec<u8>> {
    if size == 0
        || size > MAX_SECTION
        || offset
            .checked_add(size as u64)
            .is_none_or(|end| end > file.metadata().map_or(0, |metadata| metadata.len()))
    {
        return Err(invalid("invalid managed FFmpeg section bounds"));
    }
    let mut bytes = vec![0; size];
    file.read_exact_at(&mut bytes, offset)?;
    Ok(bytes)
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::other(message)
}
fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

#[cfg(test)]
#[path = "managed_elf_tests.rs"]
mod tests;
