// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::body::Bytes;
use std::{collections::VecDeque, sync::Mutex, task::Waker};
use tokio::sync::Notify;

pub(super) struct State {
    pub chunks: VecDeque<Bytes>,
    pub done: bool,
    pub cancelled: bool,
    pub waker: Option<Waker>,
}

pub(super) struct Queue {
    pub state: Mutex<State>,
    pub changed: Notify,
    pub stop: tokio_util::sync::CancellationToken,
}

impl Queue {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                chunks: VecDeque::new(),
                done: false,
                cancelled: false,
                waker: None,
            }),
            changed: Notify::new(),
            stop: tokio_util::sync::CancellationToken::new(),
        }
    }
    pub fn cancel(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.cancelled = true;
        state.chunks.clear();
        self.stop.cancel();
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
        self.changed.notify_waiters();
    }
    pub fn cancelled(&self) -> bool {
        self.state.lock().map_or(true, |s| s.cancelled)
    }
    pub fn finish(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.done = true;
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
        self.changed.notify_waiters();
    }
    pub async fn push(&self, bytes: Bytes) -> bool {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                if state.cancelled {
                    return false;
                }
                if state.chunks.len() < 2 {
                    state.chunks.push_back(bytes);
                    if let Some(waker) = state.waker.take() {
                        waker.wake();
                    }
                    return true;
                }
            }
            notified.await;
        }
    }
    pub async fn drained(&self) {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                if state.cancelled || (state.done && state.chunks.is_empty()) {
                    return;
                }
            }
            notified.await;
        }
    }
}
