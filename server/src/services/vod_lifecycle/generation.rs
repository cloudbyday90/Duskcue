// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::queue::Queue;
use crate::domains::playback::PlaybackError;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::OwnedSemaphorePermit;
use tokio_util::{
    sync::CancellationToken,
    task::{TaskTracker, task_tracker::TaskTrackerToken},
};
use uuid::Uuid;

struct Gate {
    retired: bool,
    queues: BTreeMap<Uuid, Weak<Queue>>,
}

pub(crate) struct Generation {
    gate: Mutex<Gate>,
    pub stop: CancellationToken,
    tracker: TaskTracker,
    failed: AtomicBool,
    permit: Mutex<Option<OwnedSemaphorePermit>>,
}
impl Generation {
    pub(super) fn new(permit: OwnedSemaphorePermit) -> Self {
        Self::create(Some(permit))
    }
    pub(crate) fn viewer() -> Arc<Self> {
        Arc::new(Self::create(None))
    }
    fn create(permit: Option<OwnedSemaphorePermit>) -> Self {
        Self {
            gate: Mutex::new(Gate {
                retired: false,
                queues: BTreeMap::new(),
            }),
            stop: CancellationToken::new(),
            tracker: TaskTracker::new(),
            failed: AtomicBool::new(false),
            permit: Mutex::new(permit),
        }
    }
    pub(super) fn register(
        &self,
        id: Uuid,
        queue: &Arc<Queue>,
    ) -> Result<TaskTrackerToken, PlaybackError> {
        let mut gate = self
            .gate
            .lock()
            .map_err(|_| PlaybackError::LifecycleCleanup)?;
        if gate.retired {
            return Err(PlaybackError::SessionNotFound);
        }
        gate.queues.insert(id, Arc::downgrade(queue));
        Ok(self.tracker.token())
    }
    pub(super) fn remove(&self, id: Uuid) {
        if let Ok(mut gate) = self.gate.lock() {
            gate.queues.remove(&id);
        } else {
            self.fail();
        }
    }
    pub fn retire(&self) {
        let mut gate = self.gate.lock().unwrap_or_else(|p| {
            self.failed.store(true, Ordering::Release);
            p.into_inner()
        });
        gate.retired = true;
        for queue in gate.queues.values().filter_map(Weak::upgrade) {
            queue.cancel();
        }
        self.tracker.close();
        self.stop.cancel();
    }
    pub fn active(&self) -> bool {
        self.gate.lock().is_ok_and(|g| !g.retired)
    }
    pub(super) fn release(&self) {
        if let Ok(mut permit) = self.permit.lock() {
            permit.take();
        } else {
            self.fail();
        }
    }
    pub async fn inspect<T, F>(self: &Arc<Self>, action: F) -> Result<T, PlaybackError>
    where
        T: Send + 'static,
        F: std::future::Future<Output = Result<T, PlaybackError>> + Send + 'static,
    {
        let receive = {
            let gate = self
                .gate
                .lock()
                .map_err(|_| PlaybackError::LifecycleCleanup)?;
            if gate.retired {
                return Err(PlaybackError::LifecycleUnavailable);
            }
            let generation = self.clone();
            let (send, receive) = tokio::sync::oneshot::channel();
            self.tracker.spawn(async move {
                let mut completion = Inspection {
                    generation,
                    done: false,
                };
                let result = action.await;
                let _ = send.send(result);
                completion.done = true;
            });
            drop(gate);
            receive
        };
        receive.await.map_err(|_| PlaybackError::LifecycleCleanup)?
    }
    pub fn fail(&self) {
        self.failed.store(true, Ordering::Release);
        self.retire();
    }
    pub async fn drain(&self) -> Result<(), PlaybackError> {
        self.retire();
        self.tracker.wait().await;
        if self.failed.load(Ordering::Acquire) {
            Err(PlaybackError::LifecycleCleanup)
        } else {
            Ok(())
        }
    }
}

struct Inspection {
    generation: Arc<Generation>,
    done: bool,
}
impl Drop for Inspection {
    fn drop(&mut self) {
        if !self.done {
            self.generation.fail();
        }
    }
}
