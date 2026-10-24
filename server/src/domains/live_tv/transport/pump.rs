// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::io::AsyncWriteExt;
use tokio::process::ChildStdin;
use tokio_util::sync::CancellationToken;

use super::TransportError;

pub(super) async fn run(
    mut response: reqwest::Response,
    mut input: ChildStdin,
    cancel: CancellationToken,
    writing: Arc<AtomicBool>,
) -> Result<(), TransportError> {
    let mut prefix = Vec::with_capacity(564);
    let mut validated = false;
    loop {
        let chunk = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(TransportError::Cancelled),
            chunk = response.chunk() => chunk.map_err(|_| TransportError::Connection)?.ok_or(TransportError::Ended)?,
        };
        if chunk.len() > 1024 * 1024 {
            return Err(TransportError::Input);
        }
        let mut bytes = chunk.as_ref();
        if !validated {
            let take = (564 - prefix.len()).min(bytes.len());
            prefix.extend_from_slice(&bytes[..take]);
            bytes = &bytes[take..];
            if prefix.len() != 564 {
                continue;
            }
            if [0, 188, 376].into_iter().any(|i| prefix[i] != 0x47) {
                return Err(TransportError::Input);
            }
            write(&mut input, &prefix, &cancel, &writing).await?;
            validated = true;
        }
        write(&mut input, bytes, &cancel, &writing).await?;
    }
}

async fn write(
    input: &mut ChildStdin,
    bytes: &[u8],
    cancel: &CancellationToken,
    writing: &AtomicBool,
) -> Result<(), TransportError> {
    writing.store(true, Ordering::Release);
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(TransportError::Cancelled),
        result = tokio::time::timeout(std::time::Duration::from_secs(5), input.write_all(bytes)) => result.map_err(|_| TransportError::Deadline)?.map_err(|_| TransportError::Worker),
    };
    writing.store(false, Ordering::Release);
    result
}
