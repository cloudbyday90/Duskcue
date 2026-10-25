// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{Output, Unavailable, forwarding, wire};
use crate::{
    error::AppError,
    services::event_bus::{ConnectionGuard, EventBus},
};
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use uuid::Uuid;

pub struct EventStreams {
    registration: Mutex<()>,
    closed: AtomicBool,
    failed: AtomicBool,
    stop: CancellationToken,
    tracker: TaskTracker,
}

struct Completion {
    owner: Arc<EventStreams>,
    connection: Option<ConnectionGuard>,
    finished: bool,
}
impl Completion {
    fn finish(mut self) {
        drop(self.connection.take());
        self.finished = true;
    }
}
impl Drop for Completion {
    fn drop(&mut self) {
        if !self.finished {
            self.owner.fail();
        }
        drop(self.connection.take());
    }
}

impl Default for EventStreams {
    fn default() -> Self {
        Self {
            registration: Mutex::new(()),
            closed: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            stop: CancellationToken::new(),
            tracker: TaskTracker::new(),
        }
    }
}
impl EventStreams {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn open(
        self: &Arc<Self>,
        bus: Arc<EventBus>,
        user: Uuid,
        filter: Option<HashSet<String>>,
        last: Option<Uuid>,
    ) -> Result<Output, AppError> {
        tokio::runtime::Handle::try_current().map_err(|_| Unavailable)?;
        let (output, ready) = {
            let _registration = match self.registration.lock() {
                Ok(registration) => registration,
                Err(poisoned) => {
                    drop(poisoned);
                    self.fail();
                    return Err(Unavailable.into());
                }
            };
            if self.closed.load(Ordering::Acquire) || self.failed.load(Ordering::Acquire) {
                return Err(Unavailable.into());
            }
            let guard = bus
                .register_connection(user)
                .map_err(|_| AppError::RateLimited {
                    code: "SSE_LIMIT_REACHED".into(),
                })?;
            let (tx, rx) = mpsc::channel(wire::CHANNEL_DEPTH);
            let (prepared, ready) = oneshot::channel();
            let stop = self.stop.child_token();
            let token = stop.clone();
            let completion = Completion {
                owner: self.clone(),
                connection: Some(guard),
                finished: false,
            };
            self.tracker.spawn(async move {
                forwarding::run(bus, user, last, filter, tx, &token, Some(prepared)).await;
                completion.finish();
            });
            (Output::new(rx, stop), ready)
        };
        ready.await.map_err(|_| Unavailable)?;
        Ok(output)
    }

    pub fn close(&self) {
        let registration = self.registration.lock();
        self.closed.store(true, Ordering::Release);
        if registration.is_err() {
            self.failed.store(true, Ordering::Release);
        }
        self.tracker.close();
        drop(registration);
        self.stop.cancel();
    }

    pub async fn drain(&self) -> Result<(), Unavailable> {
        self.tracker.wait().await;
        if !self.closed.load(Ordering::Acquire)
            || self.failed.load(Ordering::Acquire)
            || self.registration.is_poisoned()
        {
            return Err(Unavailable);
        }
        Ok(())
    }

    pub fn idle(&self) -> bool {
        self.tracker.is_empty()
    }

    fn fail(&self) {
        self.failed.store(true, Ordering::Release);
        self.tracker.close();
        self.stop.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn poisoned_admission_cancels_live_output_and_prevents_successful_drain() {
        let owner = Arc::new(EventStreams::new());
        let bus = Arc::new(EventBus::new(1));
        let mut output = owner
            .open(bus.clone(), Uuid::now_v7(), None, None)
            .await
            .unwrap();
        let poisoned = owner.clone();
        assert!(
            std::thread::spawn(move || {
                let _registration = poisoned.registration.lock().unwrap();
                panic!("qualification: poisoned admission");
            })
            .join()
            .is_err()
        );
        assert!(
            owner
                .open(bus.clone(), Uuid::now_v7(), None, None)
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), output.next())
                .await
                .unwrap()
                .is_none()
        );
        owner.close();
        assert!(owner.drain().await.is_err());
        assert_eq!(bus.total_connection_count(), 0);
    }
}
