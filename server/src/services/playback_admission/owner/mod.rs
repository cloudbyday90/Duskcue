// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

mod commands;
mod lifecycle;
mod maintenance;

use super::{
    AdmissionError, Request,
    ledger::{IdlePolicy, Ledger},
    retention::REGULAR_OPERATIONS,
};
use crate::{services::encryption::EncryptionKey, state::AppState};
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Semaphore, watch};
use tokio::time::Instant;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use uuid::Uuid;

struct Entry {
    request: Request,
    cancel: CancellationToken,
    pending: AtomicBool,
    attached: AtomicBool,
    checking: AtomicBool,
    deadline: watch::Sender<Option<Instant>>,
}
struct Gate {
    closed: bool,
    entries: BTreeMap<Uuid, Arc<Entry>>,
}

pub struct AdmissionOwner {
    ledger: Ledger,
    gate: Mutex<Gate>,
    commands: Arc<Semaphore>,
    maintenance: Arc<Semaphore>,
    maintenance_lost: Arc<AtomicBool>,
    #[cfg(test)]
    maintenance_task: Mutex<Option<tokio::task::AbortHandle>>,
    tracker: TaskTracker,
    closed: CancellationToken,
    failed: AtomicBool,
    scan: AtomicUsize,
}

impl AdmissionOwner {
    pub async fn new(
        pool: PgPool,
        key: Arc<EncryptionKey>,
        idle: Option<i32>,
    ) -> Result<Arc<Self>, AdmissionError> {
        Self::create(pool, key, IdlePolicy::Fixed(idle)).await
    }

    async fn create(
        pool: PgPool,
        key: Arc<EncryptionKey>,
        idle: IdlePolicy,
    ) -> Result<Arc<Self>, AdmissionError> {
        let ledger = Ledger::register(pool, key, idle).await?;
        let owner = Arc::new(Self {
            ledger,
            gate: Mutex::new(Gate {
                closed: false,
                entries: BTreeMap::new(),
            }),
            commands: Arc::new(Semaphore::new(REGULAR_OPERATIONS)),
            maintenance: Arc::new(Semaphore::new(1)),
            maintenance_lost: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            maintenance_task: Mutex::new(None),
            tracker: TaskTracker::new(),
            closed: CancellationToken::new(),
            failed: AtomicBool::new(false),
            scan: AtomicUsize::new(0),
        });
        let weak = Arc::downgrade(&owner);
        let closed = owner.closed.clone();
        owner.tracker.spawn(async move {
            loop {
                tokio::select! {biased;_=closed.cancelled()=>break,_=tokio::time::sleep(Duration::from_secs(1))=>{}}
                let Some(owner)=weak.upgrade() else {break;};
                owner.monitor().await;
            }
        });
        owner.start_maintenance();
        Ok(owner)
    }

    pub async fn attach(state: &AppState) -> Result<Arc<Self>, AdmissionError> {
        let owner = Self::create(
            state.pool.clone(),
            state.encryption_key.clone(),
            IdlePolicy::Runtime(state.runtime_config.clone()),
        )
        .await?;
        if state.playback_admission.set(owner.clone()).is_err() {
            owner.close();
            owner.drain().await?;
            return Err(AdmissionError::Conflict);
        }
        Ok(owner)
    }

    pub fn instance(&self) -> Uuid {
        self.ledger.instance
    }

    pub fn stop(&self, id: Uuid) -> Result<(), AdmissionError> {
        let gate = self.gate.lock().map_err(|_| AdmissionError::Closed)?;
        if let Some(entry) = gate.entries.get(&id) {
            entry.cancel.cancel();
        }
        Ok(())
    }

    pub fn close(&self) {
        if let Ok(mut gate) = self.gate.lock() {
            gate.closed = true;
            for entry in gate.entries.values() {
                entry.cancel.cancel();
            }
            self.closed.cancel();
            self.tracker.close();
        } else {
            self.failed.store(true, Ordering::SeqCst);
        }
    }

    pub async fn drain(&self) -> Result<(), AdmissionError> {
        self.tracker.wait().await;
        let entries = self.entries()?;
        for entry in entries {
            if !entry.attached.load(Ordering::SeqCst) {
                if self.ledger.retire(entry.request, true).await.is_ok() {
                    self.remove(entry.request.id);
                } else {
                    self.failed.store(true, Ordering::SeqCst);
                }
            }
        }
        if self.failed.load(Ordering::SeqCst)
            || self.maintenance_lost.load(Ordering::SeqCst)
            || !self.entries()?.is_empty()
        {
            return Err(AdmissionError::Cleanup);
        }
        Ok(())
    }

    fn entries(&self) -> Result<Vec<Arc<Entry>>, AdmissionError> {
        Ok(self
            .gate
            .lock()
            .map_err(|_| AdmissionError::Closed)?
            .entries
            .values()
            .cloned()
            .collect())
    }
    fn remove(&self, id: Uuid) {
        if let Ok(mut gate) = self.gate.lock() {
            gate.entries.remove(&id);
        } else {
            self.failed.store(true, Ordering::SeqCst);
        }
    }
}
