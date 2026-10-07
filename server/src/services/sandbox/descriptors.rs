// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::time::Duration;

use tokio::io::unix::AsyncFd;

use super::wire::{self, ACK_BYTES, FilesystemEnforcement};

pub(super) struct PreparedDescriptors {
    policy: File,
    acknowledgment_write: OwnedFd,
    acknowledgment_read: AsyncFd<OwnedFd>,
    nonce: u64,
}

pub(crate) struct BootstrapReadiness {
    acknowledgment: AsyncFd<OwnedFd>,
    nonce: u64,
}

impl PreparedDescriptors {
    pub(super) fn new(filter: &seccompiler::BpfProgram) -> io::Result<Self> {
        let nonce = rand::random::<u64>();
        let bytes = wire::encode(filter, nonce)?;
        Self::from_bytes(&bytes, nonce)
    }

    #[cfg(test)]
    pub(super) fn corrupt(filter: &seccompiler::BpfProgram, offset: usize) -> io::Result<Self> {
        let nonce = rand::random::<u64>();
        let mut bytes = wire::encode(filter, nonce)?;
        bytes[offset] ^= 1;
        Self::from_bytes(&bytes, nonce)
    }

    fn from_bytes(bytes: &[u8], nonce: u64) -> io::Result<Self> {
        let raw_policy = unsafe {
            libc::memfd_create(
                c"duskcue-ffmpeg-policy".as_ptr(),
                libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
            )
        };
        if raw_policy < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut policy = unsafe { File::from_raw_fd(raw_policy) };
        policy.write_all(bytes)?;
        let seals =
            libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;
        if unsafe { libc::fcntl(policy.as_raw_fd(), libc::F_ADD_SEALS, seals) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut pipe = [-1; 2];
        if unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let acknowledgment_read = unsafe { OwnedFd::from_raw_fd(pipe[0]) };
        let acknowledgment_write = unsafe { OwnedFd::from_raw_fd(pipe[1]) };
        Ok(Self {
            policy,
            acknowledgment_write,
            acknowledgment_read: AsyncFd::new(acknowledgment_read)?,
            nonce,
        })
    }

    pub(super) fn policy_fd(&self) -> RawFd {
        self.policy.as_raw_fd()
    }

    pub(super) fn acknowledgment_fd(&self) -> RawFd {
        self.acknowledgment_write.as_raw_fd()
    }

    pub(super) fn readiness(self) -> BootstrapReadiness {
        BootstrapReadiness {
            acknowledgment: self.acknowledgment_read,
            nonce: self.nonce,
        }
    }
}

impl BootstrapReadiness {
    pub(crate) async fn wait(self) -> io::Result<FilesystemEnforcement> {
        tokio::time::timeout(Duration::from_secs(5), self.read())
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "managed FFmpeg bootstrap timed out",
                )
            })?
    }

    async fn read(self) -> io::Result<FilesystemEnforcement> {
        let mut bytes = [0; ACK_BYTES];
        let mut offset = 0;
        while offset < bytes.len() {
            let mut ready = self.acknowledgment.readable().await?;
            let result = ready.try_io(|descriptor| {
                let count = unsafe {
                    libc::read(
                        descriptor.get_ref().as_raw_fd(),
                        bytes[offset..].as_mut_ptr().cast(),
                        bytes.len() - offset,
                    )
                };
                if count < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(count as usize)
                }
            });
            match result {
                Ok(Ok(0)) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "managed FFmpeg exited before bootstrap acknowledgment",
                    ));
                }
                Ok(Ok(count)) => offset += count,
                Ok(Err(error)) if error.kind() == io::ErrorKind::Interrupted => {}
                Ok(Err(error)) => return Err(error),
                Err(_) => {}
            }
        }
        wire::decode_ack(&bytes, self.nonce)
    }
}

pub(super) fn inherit(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn write_filesystem_status(fd: RawFd, status: u8) -> io::Result<()> {
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(b"DUSKFS01");
    bytes[8] = status;
    bytes[9] = 1;
    for _ in 0..8 {
        let count = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        if count == bytes.len() as isize {
            return Ok(());
        }
        if count < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
            continue;
        }
        return Err(io::Error::from_raw_os_error(libc::EIO));
    }
    Err(io::Error::from_raw_os_error(libc::EINTR))
}

#[cfg(test)]
#[path = "descriptor_tests.rs"]
mod tests;
