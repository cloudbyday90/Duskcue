// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{CHUNK, PLAYLIST, ReadContext};
use crate::domains::playback::PlaybackError;
use std::{
    io::SeekFrom,
    path::{Component, Path},
};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt},
    sync::OwnedSemaphorePermit,
};

impl ReadContext {
    pub(super) async fn permit(&self) -> Result<OwnedSemaphorePermit, PlaybackError> {
        self.check()?;
        let permit = tokio::select! {biased;
            _=self.entry.queue.stop.cancelled()=>return Err(PlaybackError::LifecycleUnavailable),
            result=self.owner.io.clone().acquire_owned()=>result.map_err(|_|PlaybackError::LifecycleCleanup)?,
        };
        self.check()?;
        Ok(permit)
    }
    pub async fn open(
        &mut self,
        path: &Path,
        cache: Option<&Path>,
    ) -> Result<usize, PlaybackError> {
        let _permit = self.permit().await?;
        if let Some(root) = cache {
            check_cache(root, path).await?;
        }
        let mut options = tokio::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NONBLOCK | if cache.is_some() { libc::O_NOFOLLOW } else { 0 });
        #[cfg(windows)]
        if cache.is_some() {
            options.custom_flags(0x00200000);
        }
        let mut file = options
            .open(path)
            .await
            .map_err(|_| PlaybackError::FileNotFound)?;
        file.set_max_buf_size(CHUNK);
        let metadata = file.metadata().await;
        if !metadata.is_ok_and(|m| m.is_file() && !m.file_type().is_symlink()) {
            drop(file.into_std().await);
            return Err(PlaybackError::FileNotFound);
        }
        let index = self.files.len();
        self.files.push(Some(file));
        self.check()?;
        Ok(index)
    }
    pub async fn metadata(&self, id: usize) -> Result<std::fs::Metadata, PlaybackError> {
        let _permit = self.permit().await?;
        self.files
            .get(id)
            .and_then(Option::as_ref)
            .ok_or(PlaybackError::LifecycleCleanup)?
            .metadata()
            .await
            .map_err(|_| PlaybackError::FileNotFound)
    }
    pub async fn seek(&mut self, id: usize, offset: u64) -> Result<(), PlaybackError> {
        let _permit = self.permit().await?;
        self.files
            .get_mut(id)
            .and_then(Option::as_mut)
            .ok_or(PlaybackError::LifecycleCleanup)?
            .seek(SeekFrom::Start(offset))
            .await
            .map_err(|_| PlaybackError::FileNotFound)?;
        self.check()
    }
    pub async fn text(&mut self, path: &Path, root: &Path) -> Result<String, PlaybackError> {
        let id = self.open(path, Some(root)).await?;
        if self.metadata(id).await?.len() > PLAYLIST as u64 {
            return Err(PlaybackError::SessionNotFound);
        }
        let mut bytes = Vec::new();
        loop {
            let part = self
                .chunk(id, CHUNK.min(PLAYLIST + 1 - bytes.len()))
                .await?;
            if part.is_empty() {
                break;
            }
            bytes.extend_from_slice(&part);
            if bytes.len() > PLAYLIST {
                return Err(PlaybackError::SessionNotFound);
            }
        }
        self.close_file(id).await;
        String::from_utf8(bytes).map_err(|_| PlaybackError::SessionNotFound)
    }
    pub(super) async fn chunk(
        &mut self,
        id: usize,
        limit: usize,
    ) -> Result<Vec<u8>, PlaybackError> {
        let _permit = self.permit().await?;
        let mut bytes = vec![0; limit.min(CHUNK)];
        let length = self
            .files
            .get_mut(id)
            .and_then(Option::as_mut)
            .ok_or(PlaybackError::LifecycleCleanup)?
            .read(&mut bytes)
            .await
            .map_err(|_| PlaybackError::FileNotFound)?;
        self.check()?;
        bytes.truncate(length);
        Ok(bytes)
    }
    pub async fn close_file(&mut self, id: usize) {
        if let Some(file) = self.files.get_mut(id).and_then(Option::take) {
            drop(file.into_std().await);
        }
    }
    pub(super) async fn close_files(&mut self) {
        for file in &mut self.files {
            if let Some(file) = file.take() {
                drop(file.into_std().await);
            }
        }
    }
}

async fn check_cache(root: &Path, path: &Path) -> Result<(), PlaybackError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| PlaybackError::SessionNotFound)?;
    let root_meta = tokio::fs::symlink_metadata(root)
        .await
        .map_err(|_| PlaybackError::SessionNotFound)?;
    if !root_meta.is_dir() || root_meta.file_type().is_symlink() {
        return Err(PlaybackError::SessionNotFound);
    }
    let canonical = tokio::fs::canonicalize(root)
        .await
        .map_err(|_| PlaybackError::SessionNotFound)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(PlaybackError::SessionNotFound);
        }
        current.push(component);
        let metadata = tokio::fs::symlink_metadata(&current)
            .await
            .map_err(|_| PlaybackError::SessionNotFound)?;
        if metadata.file_type().is_symlink() {
            return Err(PlaybackError::SessionNotFound);
        }
    }
    if tokio::fs::canonicalize(path)
        .await
        .map_err(|_| PlaybackError::SessionNotFound)?
        != canonical.join(relative)
    {
        return Err(PlaybackError::SessionNotFound);
    }
    Ok(())
}

pub(crate) async fn remove_cache(path: &Path) -> Result<(), PlaybackError> {
    match tokio::fs::symlink_metadata(path).await {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        _ => return Err(PlaybackError::LifecycleCleanup),
    }
    tokio::fs::remove_dir_all(path)
        .await
        .map_err(|_| PlaybackError::LifecycleCleanup)
}
