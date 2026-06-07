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

use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum LockfileError {
    #[error("Another Duskcue instance is already running (PID {pid})")]
    AlreadyRunning { pid: u32 },
    #[error("Failed to read lockfile: {0}")]
    Read(#[from] std::io::Error),
    #[error("Lockfile contains invalid content")]
    InvalidContent,
}

pub struct Lockfile {
    path: PathBuf,
    released: bool,
}

impl Lockfile {
    pub fn acquire(data_dir: &Path) -> Result<Self, LockfileError> {
        let path = data_dir.join(".duskcue.lock");

        if path.exists() {
            match Self::read_pid(&path) {
                Ok(pid) => {
                    if is_pid_alive(pid) {
                        return Err(LockfileError::AlreadyRunning { pid });
                    }
                    tracing::warn!(pid, "Removing stale lockfile from previous crash");
                    std::fs::remove_file(&path)?;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Lockfile exists but is unreadable, removing");
                    std::fs::remove_file(&path)?;
                }
            }
        }

        let pid = std::process::id();
        std::fs::write(&path, pid.to_string())?;
        tracing::info!(path = %path.display(), pid, "Startup lockfile acquired");

        Ok(Self {
            path,
            released: false,
        })
    }

    pub fn release(&mut self) {
        if self.released {
            return;
        }
        if let Err(e) = std::fs::remove_file(&self.path) {
            tracing::warn!(error = %e, "Failed to remove startup lockfile");
        } else {
            tracing::info!("Startup lockfile removed");
        }
        self.released = true;
    }

    fn read_pid(path: &Path) -> Result<u32, LockfileError> {
        let content = std::fs::read_to_string(path)?;
        content
            .trim()
            .parse::<u32>()
            .map_err(|_| LockfileError::InvalidContent)
    }
}

impl Drop for Lockfile {
    fn drop(&mut self) {
        if !self.released
            && let Err(e) = std::fs::remove_file(&self.path)
        {
            tracing::warn!(error = %e, "Failed to remove startup lockfile on drop");
        }
    }
}

fn is_pid_alive(pid: u32) -> bool {
    use sysinfo::{Pid, ProcessesToUpdate, System};

    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
    system.process(Pid::from_u32(pid)).is_some()
}
