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

use std::fs::Metadata;
use std::path::{Path, PathBuf};

use super::playlist;
use crate::domains::playback::PlaybackError;

pub(super) struct OwnedCache {
    directory: PathBuf,
}

impl OwnedCache {
    pub(super) async fn new(directory: &Path, manifest: &Path) -> Result<Self, PlaybackError> {
        if manifest
            .file_name()
            .is_none_or(|name| name != "manifest.m3u8")
            || manifest.parent() != Some(directory)
        {
            return Err(failure("unowned HLS startup manifest"));
        }
        let metadata = tokio::fs::symlink_metadata(directory)
            .await
            .map_err(io_error)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(failure("invalid owned HLS startup cache"));
        }
        Ok(Self {
            directory: tokio::fs::canonicalize(directory).await.map_err(io_error)?,
        })
    }

    pub(super) async fn open(
        &self,
        filename: &str,
    ) -> Result<Option<tokio::fs::File>, PlaybackError> {
        if !matches!(filename, "manifest.m3u8" | "init.mp4") && !playlist::segment_name(filename) {
            return Err(failure("unowned HLS startup resource name"));
        }
        let path = self.directory.join(filename);
        let metadata = match tokio::fs::symlink_metadata(&path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(error)),
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(failure("unowned HLS startup resource"));
        }
        let canonical = tokio::fs::canonicalize(&path).await.map_err(io_error)?;
        if canonical != path || canonical.parent() != Some(self.directory.as_path()) {
            return Err(failure("HLS startup resource escaped its cache"));
        }
        let mut options = tokio::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW);
        match options.open(canonical).await {
            Ok(file) => Ok(Some(file)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_error(error)),
        }
    }
}

pub(super) fn stable(before: &Metadata, after: &Metadata) -> bool {
    let unchanged = before.len() == after.len()
        && before
            .modified()
            .ok()
            .zip(after.modified().ok())
            .is_some_and(|(before, after)| before == after);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        unchanged && before.dev() == after.dev() && before.ino() == after.ino()
    }
    #[cfg(not(unix))]
    unchanged
}

fn failure(message: &'static str) -> PlaybackError {
    PlaybackError::FfmpegFailed(message.into())
}

fn io_error(error: std::io::Error) -> PlaybackError {
    PlaybackError::FfmpegFailed(format!("HLS startup cache inspection failed: {error}"))
}
