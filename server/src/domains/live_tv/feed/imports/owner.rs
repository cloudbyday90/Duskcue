// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::tls;
use super::{ImportError, operation};
use crate::services::encryption::EncryptionKey;
use sqlx::PgPool;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::Semaphore, task::JoinHandle};
use tokio_rustls::TlsConnector;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use uuid::Uuid;

pub struct ImportOwner {
    pool: PgPool,
    key: Arc<EncryptionKey>,
    tls: TlsConnector,
    closed: Mutex<bool>,
    capacity: Arc<Semaphore>,
    cancel: CancellationToken,
    tracker: TaskTracker,
}

pub struct Import {
    cancel: CancellationToken,
    task: Option<JoinHandle<Result<Uuid, ImportError>>>,
}

impl ImportOwner {
    pub fn new(pool: PgPool, key: Arc<EncryptionKey>) -> Result<Self, ImportError> {
        Self::build(pool, key, false)
    }
    pub fn qualification_fixture(
        pool: PgPool,
        key: Arc<EncryptionKey>,
    ) -> Result<Self, ImportError> {
        Self::build(pool, key, true)
    }
    fn build(pool: PgPool, key: Arc<EncryptionKey>, fixture: bool) -> Result<Self, ImportError> {
        Ok(Self {
            pool,
            key,
            tls: tls::connector(fixture)?,
            closed: Mutex::new(false),
            capacity: Arc::new(Semaphore::new(1)),
            cancel: CancellationToken::new(),
            tracker: TaskTracker::new(),
        })
    }
    pub fn start(
        &self,
        user: Uuid,
        playlist_pin: usize,
        guide_pin: usize,
    ) -> Result<Import, ImportError> {
        let closed = self.closed.lock().map_err(|_| ImportError::Cleanup)?;
        if *closed {
            return Err(ImportError::Cancelled);
        }
        let permit = self
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| ImportError::Capacity)?;
        let (pool, key, tls) = (self.pool.clone(), self.key.clone(), self.tls.clone());
        let cancel = self.cancel.child_token();
        let task_cancel = cancel.clone();
        let task = self.tracker.spawn(async move {
            let result = operation::run(
                &pool,
                key,
                &tls,
                user,
                (playlist_pin, guide_pin),
                &task_cancel,
            )
            .await;
            if result == Err(ImportError::Cleanup) {
                permit.forget();
            } else {
                drop(permit);
            }
            tracing::debug!(
                event = "live_tv_import_finished",
                succeeded = result.is_ok()
            );
            result
        });
        Ok(Import {
            cancel,
            task: Some(task),
        })
    }
    pub fn idle(&self) -> bool {
        self.tracker.is_empty() && self.capacity.available_permits() == 1
    }
    pub async fn shutdown(&self) -> Result<(), ImportError> {
        *self.closed.lock().map_err(|_| ImportError::Cleanup)? = true;
        self.cancel.cancel();
        self.tracker.close();
        tokio::time::timeout(Duration::from_secs(30), self.tracker.wait())
            .await
            .map_err(|_| ImportError::Cleanup)?;
        if self.idle() {
            Ok(())
        } else {
            Err(ImportError::Cleanup)
        }
    }
}

impl Drop for ImportOwner {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.tracker.close();
    }
}

impl Import {
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    pub async fn finish(mut self) -> Result<Uuid, ImportError> {
        let result = self
            .task
            .as_mut()
            .ok_or(ImportError::Cleanup)?
            .await
            .map_err(|_| ImportError::Cleanup)?;
        self.task = None;
        result
    }
    pub async fn stop(self) -> Result<(), ImportError> {
        self.cancel.cancel();
        match tokio::time::timeout(Duration::from_secs(30), self.finish())
            .await
            .map_err(|_| ImportError::Cleanup)?
        {
            Ok(_) | Err(ImportError::Cancelled) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

impl Drop for Import {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
