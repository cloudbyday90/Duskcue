// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use super::process::Encoder;

pub(super) struct FrozenEncoder {
    descriptor: OwnedFd,
}

impl FrozenEncoder {
    pub(super) fn stop(encoder: &Encoder) -> anyhow::Result<Self> {
        encoder.alive()?;
        let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, encoder.pid, 0) };
        anyhow::ensure!(
            descriptor >= 0,
            "owned fixture pidfd unavailable: {}",
            std::io::Error::last_os_error()
        );
        let descriptor = unsafe { OwnedFd::from_raw_fd(i32::try_from(descriptor)?) };
        encoder.alive()?;
        let guard = Self { descriptor };
        guard.signal(libc::SIGSTOP)?;
        Ok(guard)
    }

    fn signal(&self, signal: i32) -> anyhow::Result<()> {
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.descriptor.as_raw_fd(),
                signal,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        if result < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
}

impl Drop for FrozenEncoder {
    fn drop(&mut self) {
        let _ = self.signal(libc::SIGCONT);
    }
}
