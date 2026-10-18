// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

pub(super) const NO_PROGRESS_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) struct ProgressWatchdog {
    timeout: Duration,
    advanced_at: Instant,
    media_time_ms: u64,
}

impl ProgressWatchdog {
    pub(super) fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            advanced_at: Instant::now(),
            media_time_ms: 0,
        }
    }

    pub(super) fn observe(&mut self, media_time_ms: u64) {
        self.observe_at(media_time_ms, Instant::now());
    }

    fn observe_at(&mut self, media_time_ms: u64, now: Instant) {
        if media_time_ms > self.media_time_ms {
            self.media_time_ms = media_time_ms;
            self.advanced_at = now;
        }
    }

    fn deadline(&self) -> Instant {
        self.advanced_at + self.timeout
    }

    pub(super) async fn expired(watchdog: Arc<Mutex<Self>>) {
        loop {
            let deadline = watchdog
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .deadline();
            tokio::time::sleep_until(deadline).await;
            if watchdog
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .deadline()
                <= Instant::now()
            {
                return;
            }
        }
    }
}

#[cfg(test)]
#[path = "watchdog_tests.rs"]
mod tests;
