// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{Output, Source, TransportError, worker};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Semaphore, watch};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[derive(Clone, Copy)]
pub(super) enum Status {
    Starting,
    Ready(u32),
    Finished(Result<(), TransportError>),
}

pub struct TransportOwner {
    root: PathBuf,
    tracker: TaskTracker,
    cancel: CancellationToken,
    closed: Mutex<bool>,
    capacity: Arc<Semaphore>,
}

pub struct Session {
    cancel: CancellationToken,
    status: watch::Receiver<Status>,
    output: Output,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl TransportOwner {
    pub async fn new(root: &Path) -> Result<Self, TransportError> {
        runtime()?;
        let metadata = tokio::fs::symlink_metadata(root)
            .await
            .map_err(|_| TransportError::Cache)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(TransportError::Cache);
        }
        let root = tokio::fs::canonicalize(root)
            .await
            .map_err(|_| TransportError::Cache)?;
        if root.to_str().is_none() {
            return Err(TransportError::Cache);
        }
        Ok(Self {
            root,
            tracker: TaskTracker::new(),
            cancel: CancellationToken::new(),
            closed: Mutex::new(false),
            capacity: Arc::new(Semaphore::new(1)),
        })
    }

    pub fn start(&self, source: Source) -> Result<Session, TransportError> {
        runtime()?;
        let closed = self.closed.lock().unwrap_or_else(|e| e.into_inner());
        if *closed {
            return Err(TransportError::Cancelled);
        }
        let permit = self
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| TransportError::Capacity)?;
        let output = Output::new(self.root.join(uuid::Uuid::now_v7().to_string()));
        let cancel = self.cancel.child_token();
        let (sender, status) = watch::channel(Status::Starting);
        let (task_output, root, task_cancel) = (output.clone(), self.root.clone(), cancel.clone());
        let task = self.tracker.spawn(async move {
            let outcome = match task_output.create().await {
                Ok(()) => worker::run(source, &task_output, &sender, task_cancel).await,
                Err(error) => {
                    drop(permit);
                    sender.send_replace(Status::Finished(Err(error)));
                    return;
                }
            };
            if task_output.remove(&root).await.is_err() {
                sender.send_replace(Status::Finished(Err(TransportError::Cleanup)));
                loop {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    if task_output.remove(&root).await.is_ok() {
                        break;
                    }
                }
            }
            let outcome = match outcome {
                Err(TransportError::Cancelled) => Ok(()),
                other => other,
            };
            drop(permit);
            sender.send_replace(Status::Finished(outcome));
        });
        Ok(Session {
            cancel,
            status,
            output,
            task: Some(task),
        })
    }

    pub async fn shutdown(&self) -> Result<(), TransportError> {
        *self.closed.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.cancel.cancel();
        self.tracker.close();
        tokio::time::timeout(Duration::from_secs(10), self.tracker.wait())
            .await
            .map_err(|_| TransportError::Cleanup)
    }

    pub fn idle(&self) -> bool {
        self.tracker.is_empty() && self.capacity.available_permits() == 1
    }
}

impl Drop for TransportOwner {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.tracker.close();
    }
}

impl Session {
    pub fn output(&self) -> Output {
        self.output.clone()
    }

    pub async fn ready(&mut self) -> Result<u32, TransportError> {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                match *self.status.borrow_and_update() {
                    Status::Ready(pid) => return Ok(pid),
                    Status::Finished(result) => {
                        return result.and(Err(TransportError::Unavailable));
                    }
                    Status::Starting => {}
                }
                self.status
                    .changed()
                    .await
                    .map_err(|_| TransportError::Cleanup)?;
            }
        })
        .await
        .map_err(|_| TransportError::Deadline)?
    }

    pub async fn finished(&mut self) -> Result<(), TransportError> {
        loop {
            let finished = match *self.status.borrow_and_update() {
                Status::Finished(result) => Some(result),
                _ => None,
            };
            if let Some(result) = finished {
                if result != Err(TransportError::Cleanup) {
                    if let Some(task) = self.task.as_mut() {
                        task.await.map_err(|_| TransportError::Cleanup)?;
                    }
                    self.task = None;
                }
                return result;
            }
            self.status
                .changed()
                .await
                .map_err(|_| TransportError::Cleanup)?;
        }
    }

    pub async fn stop(&mut self) -> Result<(), TransportError> {
        self.cancel.cancel();
        tokio::time::timeout(Duration::from_secs(10), self.finished())
            .await
            .map_err(|_| TransportError::Cleanup)?
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

fn runtime() -> Result<(), TransportError> {
    if matches!(
        tokio::runtime::Handle::try_current().map(|handle| handle.runtime_flavor()),
        Ok(tokio::runtime::RuntimeFlavor::MultiThread)
    ) {
        Ok(())
    } else {
        Err(TransportError::Worker)
    }
}
