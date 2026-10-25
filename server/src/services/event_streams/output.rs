// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::response::sse::Event;
use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_util::sync::CancellationToken;

pub struct Output {
    rx: Option<mpsc::Receiver<Result<Event, Infallible>>>,
    stop: CancellationToken,
    cancelled: Pin<Box<dyn Future<Output = ()> + Send>>,
}
impl Output {
    pub(super) fn new(
        rx: mpsc::Receiver<Result<Event, Infallible>>,
        stop: CancellationToken,
    ) -> Self {
        Self {
            rx: Some(rx),
            cancelled: Box::pin(stop.clone().cancelled_owned()),
            stop,
        }
    }
}
impl Stream for Output {
    type Item = Result<Event, Infallible>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.rx.is_none() {
            return Poll::Ready(None);
        }
        if this.cancelled.as_mut().poll(cx).is_ready() {
            this.rx.take();
            return Poll::Ready(None);
        }
        let next = this.rx.as_mut().expect("active receiver").poll_recv(cx);
        if this.stop.is_cancelled() || matches!(&next, Poll::Ready(None)) {
            this.rx.take();
            return Poll::Ready(None);
        }
        next
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
