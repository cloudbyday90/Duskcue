// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{TransportError, playlist};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::io::AsyncReadExt;

pub const CACHE_BYTES: u64 = 32 * 1024 * 1024;
pub const CACHE_FILES: u32 = 24;

#[derive(Clone)]
pub struct Output {
    pub(super) directory: PathBuf,
    pub(super) active: Arc<AtomicBool>,
    pub(super) writing: Arc<AtomicBool>,
}

#[derive(Debug)]
pub struct Snapshot {
    pub sequence: u64,
    pub segments: Vec<String>,
    pub bytes: u64,
    pub files: u32,
}

impl Output {
    pub(super) fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            active: Arc::new(AtomicBool::new(false)),
            writing: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn read(&self, name: &str) -> Result<Vec<u8>, TransportError> {
        if !self.active.load(Ordering::Acquire) {
            return Err(TransportError::Unavailable);
        }
        let bytes = self.read_file(name).await?;
        if !self.active.load(Ordering::Acquire) {
            return Err(TransportError::Unavailable);
        }
        Ok(bytes)
    }

    pub fn input_pending(&self) -> bool {
        self.writing.load(Ordering::Acquire)
    }

    pub async fn snapshot(&self) -> Result<Snapshot, TransportError> {
        if !self.active.load(Ordering::Acquire) {
            return Err(TransportError::Unavailable);
        }
        self.inspect().await?.ok_or(TransportError::Unavailable)
    }

    pub(super) async fn inspect(&self) -> Result<Option<Snapshot>, TransportError> {
        let (bytes, files) = self.account().await?;
        let manifest = match self.read_file("manifest.m3u8").await {
            Ok(bytes) => bytes,
            Err(TransportError::Unavailable) => return Ok(None),
            Err(error) => return Err(error),
        };
        let text = std::str::from_utf8(&manifest).map_err(|_| TransportError::Cache)?;
        let Some(parsed) = playlist::parse(text)? else {
            return Ok(None);
        };
        for (name, initialization) in [
            ("init.mp4", true),
            (parsed.segments[parsed.segments.len() - 2].as_str(), false),
            (parsed.segments[parsed.segments.len() - 1].as_str(), false),
        ] {
            if !crate::services::transcoding::complete_hls_asset(
                &self.directory,
                name,
                initialization,
            )
            .await
            .map_err(|_| TransportError::Cache)?
            {
                return Ok(None);
            }
        }
        Ok(Some(Snapshot {
            sequence: parsed.sequence,
            segments: parsed.segments,
            bytes,
            files,
        }))
    }

    pub(super) async fn account(&self) -> Result<(u64, u32), TransportError> {
        let mut entries = tokio::fs::read_dir(&self.directory)
            .await
            .map_err(|_| TransportError::Cache)?;
        let (mut bytes, mut files) = (0_u64, 0_u32);
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|_| TransportError::Cache)?
        {
            let name = entry.file_name();
            let name = name.to_str().ok_or(TransportError::Cache)?;
            let plain = name.strip_suffix(".tmp").unwrap_or(name);
            if !matches!(plain, "init.mp4" | "manifest.m3u8") && !playlist::segment(plain) {
                return Err(TransportError::Cache);
            }
            let metadata = match tokio::fs::symlink_metadata(entry.path()).await {
                Ok(metadata) => metadata,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => return Err(TransportError::Cache),
            };
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > 4 * 1024 * 1024
            {
                return Err(TransportError::Cache);
            }
            files += 1;
            bytes = bytes
                .checked_add(metadata.len())
                .ok_or(TransportError::Cache)?;
            if files > CACHE_FILES || bytes > CACHE_BYTES {
                return Err(TransportError::Cache);
            }
        }
        Ok((bytes, files))
    }

    async fn read_file(&self, name: &str) -> Result<Vec<u8>, TransportError> {
        let limit = if name == "manifest.m3u8" {
            32768
        } else if name == "init.mp4" || playlist::segment(name) {
            4 * 1024 * 1024
        } else {
            return Err(TransportError::Unavailable);
        };
        let mut options = tokio::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        let file = options
            .open(self.directory.join(name))
            .await
            .map_err(|_| TransportError::Unavailable)?;
        if !file
            .metadata()
            .await
            .map_err(|_| TransportError::Cache)?
            .is_file()
        {
            return Err(TransportError::Cache);
        }
        let mut bytes = Vec::new();
        file.take(limit + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| TransportError::Cache)?;
        if bytes.len() as u64 > limit {
            return Err(TransportError::Cache);
        }
        Ok(bytes)
    }

    pub(super) async fn create(&self) -> Result<(), TransportError> {
        let builder = tokio::fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        builder
            .create(&self.directory)
            .await
            .map_err(|_| TransportError::Cache)
    }

    pub(super) async fn remove(&self, root: &Path) -> Result<(), TransportError> {
        self.active.store(false, Ordering::Release);
        if self.directory.parent() != Some(root)
            || tokio::fs::symlink_metadata(&self.directory)
                .await
                .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(TransportError::Cleanup);
        }
        match tokio::fs::remove_dir_all(&self.directory).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(TransportError::Cleanup),
        }
    }
}
