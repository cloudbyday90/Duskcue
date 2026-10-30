// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use ring::digest::{Context, SHA256};
use std::{
    fs::File,
    io,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use tokio::io::AsyncReadExt;

#[derive(Debug)]
pub(super) struct PinnedFile {
    _file: File,
    pub(super) path: PathBuf,
    bytes: u64,
    created: u64,
    modified: u64,
    pub(super) digest: [u8; 32],
}
impl PinnedFile {
    pub(super) async fn open(path: &Path, maximum: u64) -> io::Result<Self> {
        if tokio::fs::canonicalize(path).await? != path {
            return Err(io::Error::other("qualification file must be canonical"));
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.file_attributes() & 0x400 != 0
            || metadata.len() == 0
            || metadata.len() > maximum
        {
            return Err(io::Error::other(
                "qualification file is not a bounded regular input",
            ));
        }
        let mut file = tokio::fs::File::from_std(file);
        let mut context = Context::new(&SHA256);
        let mut buffer = [0u8; 32768];
        let mut left = metadata.len();
        while left > 0 {
            let capacity = left.min(buffer.len() as u64) as usize;
            let count = file.read(&mut buffer[..capacity]).await?;
            if count == 0 {
                return Err(io::Error::other("qualification file was truncated"));
            }
            context.update(&buffer[..count]);
            left -= count as u64;
        }
        if file.read(&mut buffer[..1]).await? != 0 || tokio::fs::canonicalize(path).await? != path {
            return Err(io::Error::other("qualification file changed"));
        }
        let after = file.metadata().await?;
        if metadata.creation_time() != after.creation_time()
            || metadata.last_write_time() != after.last_write_time()
            || metadata.len() != after.len()
        {
            return Err(io::Error::other("qualification file identity changed"));
        }
        Ok(Self {
            _file: file.into_std().await,
            path: path.to_owned(),
            bytes: metadata.len(),
            created: metadata.creation_time(),
            modified: metadata.last_write_time(),
            digest: context
                .finish()
                .as_ref()
                .try_into()
                .map_err(|_| io::Error::other("invalid SHA256 size"))?,
        })
    }
    pub(super) async fn verify(&self, maximum: u64) -> io::Result<()> {
        let current = Self::open(&self.path, maximum).await?;
        if self.bytes != current.bytes
            || self.created != current.created
            || self.modified != current.modified
            || self.digest != current.digest
        {
            return Err(io::Error::other("qualification file pin changed"));
        }
        Ok(())
    }
}
