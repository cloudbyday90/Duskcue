// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::TranscodeManager;
#[cfg(test)]
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use std::{
    io,
    path::{Path, PathBuf},
};
#[cfg(test)]
use uuid::Uuid;
#[cfg(test)]
#[path = "qualification_windows/api.rs"]
mod api;
#[path = "qualification_windows/files.rs"]
mod files;
#[cfg(test)]
#[path = "qualification_windows/process.rs"]
mod process;
#[cfg(test)]
pub use process::{FrozenEncoder, OwnedEncoder};

const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_EXECUTABLE_BYTES: u64 = 256 * 1024 * 1024;
pub(super) const INPUT_ARGS: [&str; 6] = [
    "-readrate",
    "1",
    "-readrate_initial_burst",
    "2",
    "-readrate_catchup",
    "1",
];

#[derive(Debug)]
pub struct OwnedPacedInput {
    root: PathBuf,
    source: files::PinnedFile,
    executable: files::PinnedFile,
    #[cfg(test)]
    spawns: Mutex<BTreeMap<Uuid, Arc<OwnedEncoder>>>,
}
impl OwnedPacedInput {
    pub async fn new(root: &Path, source: &Path) -> io::Result<Self> {
        let root = tokio::fs::canonicalize(root).await?;
        if !tokio::fs::metadata(&root).await?.is_dir() || !source.starts_with(&root) {
            return Err(io::Error::other(
                "paced source must stay under its owned root",
            ));
        }
        let executable = PathBuf::from(
            std::env::var_os("DUSKCUE_TEST_FFMPEG_EXE")
                .ok_or_else(|| io::Error::other("explicit qualification executable required"))?,
        )
        .canonicalize()?;
        Ok(Self {
            root,
            source: files::PinnedFile::open(source, MAX_INPUT_BYTES).await?,
            executable: files::PinnedFile::open(&executable, MAX_EXECUTABLE_BYTES).await?,
            #[cfg(test)]
            spawns: Mutex::new(BTreeMap::new()),
        })
    }
    pub(super) async fn applies(&self, source: &Path) -> io::Result<bool> {
        if source != self.source.path {
            return Ok(false);
        }
        if !source.starts_with(&self.root) {
            return Err(io::Error::other("paced source escaped root"));
        }
        self.source.verify(MAX_INPUT_BYTES).await?;
        self.executable.verify(MAX_EXECUTABLE_BYTES).await?;
        Ok(true)
    }
    #[cfg(test)]
    pub(super) fn capture(
        &self,
        id: Uuid,
        pid: Option<u32>,
        source: &Path,
        manifest: &Path,
        args: &[String],
    ) -> io::Result<()> {
        if source != self.source.path {
            return Ok(());
        }
        let mut spawns = self
            .spawns
            .lock()
            .map_err(|_| io::Error::other("spawn observation poisoned"))?;
        if spawns.len() >= 4 || spawns.contains_key(&id) {
            return Err(io::Error::other("spawn observation full or repeated"));
        }
        let process = OwnedEncoder::capture(
            pid.ok_or_else(|| io::Error::other("spawned encoder has no PID"))?,
            &self.executable.path,
            source,
            manifest,
            args,
        )?;
        spawns.insert(id, Arc::new(process));
        Ok(())
    }
    #[cfg(test)]
    pub fn encoder(&self, id: Uuid) -> io::Result<Arc<OwnedEncoder>> {
        self.spawns
            .lock()
            .map_err(|_| io::Error::other("spawn observation poisoned"))?
            .get(&id)
            .cloned()
            .ok_or_else(|| io::Error::other("exact encoder observation missing"))
    }
    #[cfg(test)]
    pub fn confirm_stopped(&self) -> io::Result<()> {
        for encoder in self
            .spawns
            .lock()
            .map_err(|_| io::Error::other("spawn observation poisoned"))?
            .values()
        {
            if !encoder.clean()? {
                return Err(io::Error::other(
                    "qualification encoder cleanup is unconfirmed",
                ));
            }
        }
        Ok(())
    }
}
impl TranscodeManager {
    pub fn with_qualification_input(mut self, input: OwnedPacedInput) -> Self {
        self.qualification_input = Some(std::sync::Arc::new(input));
        self
    }
    #[cfg(test)]
    pub fn qualification_encoder(&self, id: Uuid) -> io::Result<Arc<OwnedEncoder>> {
        self.qualification_input
            .as_ref()
            .ok_or_else(|| io::Error::other("qualification input is not configured"))?
            .encoder(id)
    }
    #[cfg(test)]
    pub fn confirm_qualification_stopped(&self) -> io::Result<()> {
        if let Some(input) = &self.qualification_input {
            input.confirm_stopped()?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "qualification_windows/tests.rs"]
mod tests;
