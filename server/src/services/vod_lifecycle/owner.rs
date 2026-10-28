// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{Generation, queue::Queue};
use crate::domains::playback::PlaybackError;
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use uuid::Uuid;

pub(super) struct ResponseEntry {
    pub queue: Arc<Queue>,
    pub generation: Option<Arc<Generation>>,
    pub viewer: Option<Arc<Generation>>,
    pub(super) _permit: OwnedSemaphorePermit,
}
pub(super) struct Registry {
    pub closed: bool,
    pub generations: BTreeMap<Uuid, Arc<Generation>>,
    pub responses: BTreeMap<Uuid, Arc<ResponseEntry>>,
}

pub(crate) struct VodLifecycle {
    pub(super) registry: Mutex<Registry>,
    generations: Arc<Semaphore>,
    pub(super) responses: Arc<Semaphore>,
    pub(super) io: Arc<Semaphore>,
    pub(super) response_tasks: TaskTracker,
    commands: TaskTracker,
    command_slots: Arc<Semaphore>,
    stop: CancellationToken,
    failed: AtomicBool,
}

struct CommandGuard {
    owner: Arc<VodLifecycle>,
    done: bool,
}
impl Drop for CommandGuard {
    fn drop(&mut self) {
        if !self.done {
            self.owner.fail();
        }
    }
}
struct Observer {
    stop: CancellationToken,
    done: bool,
}
impl Drop for Observer {
    fn drop(&mut self) {
        if !self.done {
            self.stop.cancel();
        }
    }
}

impl VodLifecycle {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            registry: Mutex::new(Registry {
                closed: false,
                generations: BTreeMap::new(),
                responses: BTreeMap::new(),
            }),
            generations: Arc::new(Semaphore::new(128)),
            responses: Arc::new(Semaphore::new(128)),
            io: Arc::new(Semaphore::new(8)),
            response_tasks: TaskTracker::new(),
            commands: TaskTracker::new(),
            command_slots: Arc::new(Semaphore::new(128)),
            stop: CancellationToken::new(),
            failed: AtomicBool::new(false),
        })
    }
    pub fn generation(&self, id: Uuid) -> Option<Arc<Generation>> {
        self.registry.lock().ok()?.generations.get(&id).cloned()
    }
    pub fn begin_generation(
        &self,
        id: Uuid,
        cleanup: bool,
    ) -> Result<Arc<Generation>, PlaybackError> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| PlaybackError::LifecycleCleanup)?;
        if let Some(generation) = registry.generations.get(&id) {
            return Ok(generation.clone());
        }
        if registry.closed && !cleanup {
            return Err(PlaybackError::LifecycleUnavailable);
        }
        let permit = self
            .generations
            .clone()
            .try_acquire_owned()
            .map_err(|_| PlaybackError::LifecycleUnavailable)?;
        let generation = Arc::new(Generation::new(permit));
        if registry.closed {
            generation.retire();
        }
        registry.generations.insert(id, generation.clone());
        Ok(generation)
    }
    pub fn finish_generation(&self, id: Uuid) {
        if let Ok(mut registry) = self.registry.lock() {
            if let Some(generation) = registry.generations.remove(&id) {
                generation.release();
            }
        } else {
            self.fail();
        }
    }
    pub fn ensure_open(&self) -> Result<(), PlaybackError> {
        let registry = self
            .registry
            .lock()
            .map_err(|_| PlaybackError::LifecycleCleanup)?;
        if registry.closed {
            Err(PlaybackError::LifecycleUnavailable)
        } else {
            Ok(())
        }
    }
    pub(crate) fn shutdown_token(&self) -> CancellationToken {
        self.stop.clone()
    }
    pub(crate) async fn inspection_permit(
        &self,
        stop: &CancellationToken,
    ) -> Result<OwnedSemaphorePermit, PlaybackError> {
        let permit = tokio::select! {biased;
            _=self.stop.cancelled()=>return Err(PlaybackError::LifecycleUnavailable),
            _=stop.cancelled()=>return Err(PlaybackError::LifecycleUnavailable),
            permit=self.io.clone().acquire_owned()=>permit.map_err(|_|PlaybackError::LifecycleCleanup)?,
        };
        if self.stop.is_cancelled() || stop.is_cancelled() {
            return Err(PlaybackError::LifecycleUnavailable);
        }
        Ok(permit)
    }
    pub async fn command<T, F, Fut>(
        self: &Arc<Self>,
        cleanup: bool,
        action: F,
    ) -> Result<T, PlaybackError>
    where
        T: Send + 'static,
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, PlaybackError>> + Send + 'static,
    {
        let (receive, mut observer) = self.enqueue(cleanup, None, action)?;
        let result = receive.await.map_err(|_| PlaybackError::LifecycleCleanup)?;
        observer.done = true;
        result
    }
    pub async fn start_command<T, F, Fut>(
        self: &Arc<Self>,
        id: Uuid,
        action: F,
    ) -> Result<T, PlaybackError>
    where
        T: Send + 'static,
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, PlaybackError>> + Send + 'static,
    {
        let (receive, mut observer) = self.enqueue(false, Some(id), action)?;
        let result = receive.await.map_err(|_| PlaybackError::LifecycleCleanup)?;
        observer.done = true;
        result
    }
    fn enqueue<T, F, Fut>(
        self: &Arc<Self>,
        cleanup: bool,
        generation: Option<Uuid>,
        action: F,
    ) -> Result<(oneshot::Receiver<Result<T, PlaybackError>>, Observer), PlaybackError>
    where
        T: Send + 'static,
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, PlaybackError>> + Send + 'static,
    {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| PlaybackError::LifecycleCleanup)?;
        if registry.closed && !cleanup {
            return Err(PlaybackError::LifecycleUnavailable);
        }
        let command_slot = self
            .command_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| PlaybackError::LifecycleUnavailable)?;
        if let Some(id) = generation {
            if registry.generations.contains_key(&id) {
                return Err(PlaybackError::LifecycleCleanup);
            }
            let permit = self
                .generations
                .clone()
                .try_acquire_owned()
                .map_err(|_| PlaybackError::LifecycleUnavailable)?;
            registry
                .generations
                .insert(id, Arc::new(Generation::new(permit)));
        }
        let token = self.stop.child_token();
        let observer = Observer {
            stop: token.clone(),
            done: false,
        };
        let (send, receive) = oneshot::channel();
        let owner = self.clone();
        self.commands.spawn(async move {
            let _command_slot = command_slot;
            let mut guard = CommandGuard { owner, done: false };
            let result = action(token).await;
            let _ = send.send(result);
            guard.done = true;
        });
        drop(registry);
        Ok((receive, observer))
    }
    pub fn close(&self) {
        let mut registry = self.registry.lock().unwrap_or_else(|p| {
            self.failed.store(true, Ordering::Release);
            p.into_inner()
        });
        registry.closed = true;
        for generation in registry.generations.values() {
            generation.retire();
        }
        for entry in registry.responses.values() {
            entry.queue.cancel();
        }
        self.stop.cancel();
        self.commands.close();
        self.response_tasks.close();
    }
    pub(crate) fn fail(&self) {
        self.failed.store(true, Ordering::Release);
    }
    pub async fn drain_commands(&self) -> Result<(), PlaybackError> {
        self.commands.wait().await;
        if self.failed.load(Ordering::Acquire) {
            Err(PlaybackError::LifecycleCleanup)
        } else {
            Ok(())
        }
    }
    pub async fn drain_responses(&self) -> Result<(), PlaybackError> {
        self.response_tasks.wait().await;
        let registry = self
            .registry
            .lock()
            .map_err(|_| PlaybackError::LifecycleCleanup)?;
        if self.failed.load(Ordering::Acquire) || !registry.responses.is_empty() {
            Err(PlaybackError::LifecycleCleanup)
        } else {
            Ok(())
        }
    }
    pub fn complete(&self) -> bool {
        self.registry
            .lock()
            .is_ok_and(|r| r.closed && r.generations.is_empty() && r.responses.is_empty())
            && !self.failed.load(Ordering::Acquire)
    }
}
