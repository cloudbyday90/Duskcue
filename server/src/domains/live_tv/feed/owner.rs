// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{FeedError, operation, tls};
use crate::{
    domains::live_tv::assignment::secrets::FeedField, services::encryption::EncryptionKey,
};
use sqlx::PgPool;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::Semaphore, task::JoinHandle};
use tokio_rustls::TlsConnector;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use uuid::Uuid;

struct Admission {
    closed: bool,
    users: HashSet<Uuid>,
}

struct Active {
    state: Arc<Mutex<Admission>>,
    user: Uuid,
}

impl Drop for Active {
    fn drop(&mut self) {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .users
            .remove(&self.user);
    }
}

pub struct FeedOwner {
    pool: PgPool,
    key: Arc<EncryptionKey>,
    tls: TlsConnector,
    state: Arc<Mutex<Admission>>,
    capacity: Arc<Semaphore>,
    cancel: CancellationToken,
    tracker: TaskTracker,
}

pub struct Fetch {
    cancel: CancellationToken,
    task: Option<JoinHandle<Result<Vec<u8>, FeedError>>>,
}

impl FeedOwner {
    pub fn new(pool: PgPool, key: Arc<EncryptionKey>) -> Result<Self, FeedError> {
        Self::build(pool, key, false)
    }

    pub fn qualification_fixture(pool: PgPool, key: Arc<EncryptionKey>) -> Result<Self, FeedError> {
        Self::build(pool, key, true)
    }

    fn build(pool: PgPool, key: Arc<EncryptionKey>, fixture: bool) -> Result<Self, FeedError> {
        Ok(Self {
            pool,
            key,
            tls: tls::connector(fixture)?,
            state: Arc::new(Mutex::new(Admission {
                closed: false,
                users: HashSet::new(),
            })),
            capacity: Arc::new(Semaphore::new(2)),
            cancel: CancellationToken::new(),
            tracker: TaskTracker::new(),
        })
    }

    pub fn start(&self, user: Uuid, feed: FeedField, index: usize) -> Result<Fetch, FeedError> {
        let mut state = self.state.lock().map_err(|_| FeedError::Cleanup)?;
        if state.closed {
            return Err(FeedError::Cancelled);
        }
        let permit = self
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| FeedError::Capacity)?;
        if !state.users.insert(user) {
            return Err(FeedError::Capacity);
        }
        let active = Active {
            state: self.state.clone(),
            user,
        };
        let (pool, key, tls) = (self.pool.clone(), self.key.clone(), self.tls.clone());
        let cancel = self.cancel.child_token();
        let task_cancel = cancel.clone();
        let task = self.tracker.spawn(async move {
            let result = operation::run(&pool, &key, &tls, user, feed, index, &task_cancel).await;
            drop(active);
            drop(permit);
            tracing::debug!(
                event = "saved_feed_operation_finished",
                succeeded = result.is_ok()
            );
            result
        });
        Ok(Fetch {
            cancel,
            task: Some(task),
        })
    }

    pub fn idle(&self) -> bool {
        self.tracker.is_empty() && self.capacity.available_permits() == 2
    }

    pub async fn shutdown(&self) -> Result<(), FeedError> {
        self.state.lock().map_err(|_| FeedError::Cleanup)?.closed = true;
        self.cancel.cancel();
        self.tracker.close();
        tokio::time::timeout(Duration::from_secs(10), self.tracker.wait())
            .await
            .map_err(|_| FeedError::Cleanup)
    }
}

impl Drop for FeedOwner {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.tracker.close();
    }
}

impl Fetch {
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub async fn finish(mut self) -> Result<Vec<u8>, FeedError> {
        let result = self
            .task
            .as_mut()
            .ok_or(FeedError::Cleanup)?
            .await
            .map_err(|_| FeedError::Cleanup)?;
        self.task = None;
        result
    }

    pub async fn stop(self) -> Result<(), FeedError> {
        self.cancel.cancel();
        match tokio::time::timeout(Duration::from_secs(10), self.finish())
            .await
            .map_err(|_| FeedError::Cleanup)?
        {
            Ok(_) | Err(FeedError::Cancelled) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

impl Drop for Fetch {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
