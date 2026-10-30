// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::api;
use std::{
    collections::BTreeMap,
    io,
    os::windows::io::OwnedHandle,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug)]
pub struct OwnedEncoder {
    process: OwnedHandle,
    pub pid: u32,
    pub created: u64,
    pub source: PathBuf,
    pub manifest: PathBuf,
    cleanup_failed: AtomicBool,
}
impl OwnedEncoder {
    pub(super) fn capture(
        pid: u32,
        executable: &Path,
        source: &Path,
        manifest: &Path,
        args: &[String],
    ) -> io::Result<Self> {
        let process = api::process(pid)?;
        let created = api::created(&process)?;
        let emitted = super::super::arguments::hls_output_path(&manifest.to_string_lossy());
        if !api::alive(&process)?
            || created == 0
            || api::parent(pid)? != std::process::id()
            || api::image(&process)?.canonicalize()? != executable
            || !args
                .windows(2)
                .any(|pair| pair[0] == "-i" && pair[1] == source.to_string_lossy())
            || args.last().map(String::as_str) != Some(emitted.as_str())
            || !args
                .windows(2)
                .any(|pair| pair[0] == "-readrate" && pair[1] == "1")
            || !api::alive(&process)?
        {
            return Err(io::Error::other("owned FFmpeg spawn identity refused"));
        }
        Ok(Self {
            process,
            pid,
            created,
            source: source.to_owned(),
            manifest: manifest.to_owned(),
            cleanup_failed: AtomicBool::new(false),
        })
    }
    pub fn gone(&self) -> io::Result<bool> {
        Ok(!api::alive(&self.process)?)
    }
    pub(super) fn clean(&self) -> io::Result<bool> {
        Ok(self.gone()? && !self.cleanup_failed.load(Ordering::Acquire))
    }
}

pub struct FrozenEncoder {
    process: Arc<OwnedEncoder>,
    threads: BTreeMap<u32, OwnedHandle>,
}
impl FrozenEncoder {
    pub fn stop(process: Arc<OwnedEncoder>) -> io::Result<Self> {
        if process.gone()? {
            return Err(io::Error::other("encoder exited before suspension"));
        }
        let mut frozen = Self {
            process,
            threads: BTreeMap::new(),
        };
        for _ in 0..4 {
            let exited = frozen
                .threads
                .iter()
                .filter_map(|(id, handle)| match api::alive(handle) {
                    Ok(false) => Some(Ok(*id)),
                    Ok(true) => None,
                    Err(error) => Some(Err(error)),
                })
                .collect::<io::Result<Vec<_>>>()?;
            for id in exited {
                frozen.threads.remove(&id);
            }
            let ids = api::threads(frozen.process.pid).map_err(|error| {
                io::Error::other(format!("owned thread snapshot failed: {error}"))
            })?;
            if ids.is_empty() {
                return Err(io::Error::other("owned encoder has no threads"));
            }
            if ids == frozen.threads.keys().copied().collect::<Vec<_>>()
                && !frozen.process.gone()?
            {
                return Ok(frozen);
            }
            for id in ids {
                if frozen.threads.contains_key(&id) {
                    continue;
                }
                let handle = match api::thread(id, frozen.process.pid) {
                    Ok(handle) => handle,
                    Err(error)
                        if error.raw_os_error() == Some(87)
                            && !api::threads(frozen.process.pid)?.contains(&id) =>
                    {
                        continue;
                    }
                    Err(error) => {
                        return Err(io::Error::other(format!(
                            "open owned thread {id} failed: {error}"
                        )));
                    }
                };
                let prior = match api::suspend(&handle) {
                    Ok(prior) => prior,
                    Err(_) if !api::alive(&handle)? => continue,
                    Err(error) => {
                        return Err(io::Error::other(format!(
                            "suspend owned thread {id} failed: {error}"
                        )));
                    }
                };
                frozen.threads.insert(id, handle);
                if prior != 0 {
                    return Err(io::Error::other("encoder thread was already suspended"));
                }
            }
        }
        Err(io::Error::other(
            "owned encoder thread set did not stabilize",
        ))
    }
    pub fn thread_count(&self) -> usize {
        self.threads.len()
    }
}
impl Drop for FrozenEncoder {
    fn drop(&mut self) {
        if self.process.gone().unwrap_or(false) {
            return;
        }
        for handle in self.threads.values() {
            if api::alive(handle).unwrap_or(true)
                && api::resume(handle).is_err()
                && api::alive(handle).unwrap_or(true)
            {
                self.process.cleanup_failed.store(true, Ordering::Release);
            }
        }
    }
}
