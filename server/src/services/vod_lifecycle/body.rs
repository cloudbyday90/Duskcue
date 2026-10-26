// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::queue::Queue;
use axum::body::Bytes;
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio_stream::Stream;

pub(super) struct Output {
    pub queue: Arc<Queue>,
    ended: bool,
}
impl Output {
    pub fn new(queue: Arc<Queue>) -> Self {
        Self {
            queue,
            ended: false,
        }
    }
}
impl Stream for Output {
    type Item = Result<Bytes, std::io::Error>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.ended {
            return Poll::Ready(None);
        }
        let queue = self.queue.clone();
        let mut state = match queue.state.lock() {
            Ok(state) => state,
            Err(_) => {
                self.ended = true;
                return Poll::Ready(Some(Err(std::io::Error::other(
                    "VOD output ownership lost",
                ))));
            }
        };
        if state.cancelled {
            self.ended = true;
            return Poll::Ready(Some(Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "VOD output interrupted",
            ))));
        }
        if let Some(bytes) = state.chunks.pop_front() {
            queue.changed.notify_waiters();
            return Poll::Ready(Some(Ok(bytes)));
        }
        if state.done {
            self.ended = true;
            queue.changed.notify_waiters();
            return Poll::Ready(None);
        }
        state.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        self.queue.cancel();
    }
}
