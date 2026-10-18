// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use ring::digest::{Context, SHA256};
use tokio::io::AsyncReadExt;

use super::TranscodeManager;

const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;
pub(super) const INPUT_ARGS: [&str; 6] = [
    "-readrate",
    "1",
    "-readrate_initial_burst",
    "2",
    "-readrate_catchup",
    "1",
];

#[derive(Debug, PartialEq, Eq)]
struct InputIdentity {
    device: u64,
    inode: u64,
    bytes: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl InputIdentity {
    fn read(metadata: &std::fs::Metadata) -> io::Result<Self> {
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_INPUT_BYTES {
            return Err(invalid(
                "paced input must be a bounded nonempty regular file",
            ));
        }
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            bytes: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }
}

#[derive(Debug)]
pub struct OwnedPacedInput {
    root: PathBuf,
    source: PathBuf,
    identity: InputIdentity,
    sha256: [u8; 32],
}

impl OwnedPacedInput {
    pub async fn new(root: &Path, source: &Path) -> io::Result<Self> {
        let root = tokio::fs::canonicalize(root).await?;
        if !tokio::fs::metadata(&root).await?.is_dir() {
            return Err(invalid("paced input root must be a directory"));
        }
        let canonical = tokio::fs::canonicalize(source).await?;
        if source != canonical || !canonical.starts_with(&root) {
            return Err(invalid(
                "paced input must use its canonical path beneath the owned root",
            ));
        }
        let (identity, sha256) = inspect(&root, &canonical).await?;
        Ok(Self {
            root,
            source: canonical,
            identity,
            sha256,
        })
    }

    pub(super) async fn applies(&self, source: &Path) -> io::Result<bool> {
        if source != self.source {
            return Ok(false);
        }
        let (identity, sha256) = inspect(&self.root, source).await?;
        if identity != self.identity || sha256 != self.sha256 {
            return Err(invalid("owned paced input identity changed"));
        }
        Ok(true)
    }
}

async fn inspect(root: &Path, source: &Path) -> io::Result<(InputIdentity, [u8; 32])> {
    let canonical = tokio::fs::canonicalize(source).await?;
    if canonical != source || !canonical.starts_with(root) {
        return Err(invalid("owned paced input path changed"));
    }
    let path_metadata = tokio::fs::symlink_metadata(source).await?;
    let identity = InputIdentity::read(&path_metadata)?;
    let mut file = tokio::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(source)
        .await?;
    if InputIdentity::read(&file.metadata().await?)? != identity {
        return Err(invalid("owned paced input changed during validation"));
    }
    let mut digest = Context::new(&SHA256);
    let mut buffer = [0u8; 32768];
    let mut remaining = identity.bytes;
    while remaining > 0 {
        let capacity = remaining.min(buffer.len() as u64) as usize;
        let count = file.read(&mut buffer[..capacity]).await?;
        if count == 0 {
            return Err(invalid("owned paced input was truncated during validation"));
        }
        digest.update(&buffer[..count]);
        remaining -= count as u64;
    }
    if file.read(&mut buffer[..1]).await? != 0
        || InputIdentity::read(&file.metadata().await?)? != identity
        || InputIdentity::read(&tokio::fs::symlink_metadata(source).await?)? != identity
        || tokio::fs::canonicalize(source).await? != canonical
    {
        return Err(invalid(
            "owned paced input changed during content validation",
        ));
    }
    let sha256 = digest
        .finish()
        .as_ref()
        .try_into()
        .map_err(|_| invalid("invalid paced input SHA-256 identity"))?;
    Ok((identity, sha256))
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

impl TranscodeManager {
    pub fn with_qualification_input(mut self, input: OwnedPacedInput) -> Self {
        self.qualification_input = Some(input);
        self
    }
}

#[cfg(test)]
#[path = "qualification_tests.rs"]
mod tests;
