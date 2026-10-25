// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::{
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub(super) struct Gate(Mutex<(bool, Option<Waker>)>);

pub(super) struct Limited<T> {
    inner: T,
    remaining: usize,
    gate: Arc<Gate>,
}

pub(super) fn bounded<T>(inner: T) -> (Limited<T>, Arc<Gate>) {
    let gate = Arc::new(Gate(Mutex::new((false, None))));
    (
        Limited {
            inner,
            remaining: 32768,
            gate: gate.clone(),
        },
        gate,
    )
}

impl Gate {
    pub fn release(&self) {
        let wake = {
            let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
            state.0 = true;
            state.1.take()
        };
        if let Some(waker) = wake {
            waker.wake();
        }
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for Limited<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        destination: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let open = {
            let mut gate = self.gate.0.lock().unwrap_or_else(|e| e.into_inner());
            if !gate.0 && self.remaining == 0 && destination.remaining() > 0 {
                gate.1 = Some(cx.waker().clone());
                return Poll::Pending;
            }
            gate.0
        };
        if open || destination.remaining() == 0 {
            return Pin::new(&mut self.inner).poll_read(cx, destination);
        }
        let mut scratch = [0; 8192];
        let count = self
            .remaining
            .min(destination.remaining())
            .min(scratch.len());
        let mut limited = ReadBuf::new(&mut scratch[..count]);
        match Pin::new(&mut self.inner).poll_read(cx, &mut limited) {
            Poll::Ready(Ok(())) => {
                self.remaining -= limited.filled().len();
                destination.put_slice(limited.filled());
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for Limited<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, bytes)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
