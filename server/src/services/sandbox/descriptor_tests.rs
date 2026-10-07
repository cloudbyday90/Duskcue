// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;

#[tokio::test]
async fn private_policy_is_cloexec_sealed_and_cannot_be_changed() {
    let filter = super::super::filter::build_ffmpeg_filter().unwrap();
    let mut prepared = PreparedDescriptors::new(&filter).unwrap();
    let fd = prepared.policy_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    assert!(flags >= 0);
    assert_ne!(flags & libc::FD_CLOEXEC, 0);
    let required = libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;
    assert_eq!(
        unsafe { libc::fcntl(fd, libc::F_GET_SEALS) } & required,
        required
    );
    assert_eq!(
        prepared
            .policy
            .write_all(b"corrupt")
            .unwrap_err()
            .raw_os_error(),
        Some(libc::EPERM)
    );
    assert_eq!(unsafe { libc::ftruncate(fd, 0) }, -1);
    assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::EPERM));
}

#[tokio::test]
async fn owned_pipe_acknowledges_both_stages_and_rejects_partial_eof() {
    let filter = super::super::filter::build_ffmpeg_filter().unwrap();
    let prepared = PreparedDescriptors::new(&filter).unwrap();
    let fd = prepared.acknowledgment_fd();
    write_filesystem_status(fd, 1).unwrap();
    let mut acknowledgment = [0; 24];
    acknowledgment[..8].copy_from_slice(b"DUSKACK1");
    acknowledgment[8..16].copy_from_slice(&prepared.nonce.to_le_bytes());
    acknowledgment[16..20].copy_from_slice(&wire::audit_arch().to_le_bytes());
    acknowledgment[20..24].copy_from_slice(&1_u32.to_le_bytes());
    assert_eq!(
        unsafe { libc::write(fd, acknowledgment.as_ptr().cast(), acknowledgment.len()) },
        24
    );
    assert_eq!(
        prepared.readiness().wait().await.unwrap(),
        FilesystemEnforcement::Partial
    );
    let prepared = PreparedDescriptors::new(&filter).unwrap();
    write_filesystem_status(prepared.acknowledgment_fd(), 1).unwrap();
    assert_eq!(
        prepared.readiness().wait().await.unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
}

#[tokio::test]
async fn missing_acknowledgment_times_out_while_writer_stays_owned() {
    let filter = super::super::filter::build_ffmpeg_filter().unwrap();
    let prepared = PreparedDescriptors::new(&filter).unwrap();
    let writer = prepared.acknowledgment_write.try_clone().unwrap();
    let result = prepared.readiness().wait().await;
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
    drop(writer);
}
