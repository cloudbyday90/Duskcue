// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::AdmissionOwner;
use crate::services::playback_admission::{
    AdmissionError, MaintenanceReceipt, retention::MAINTENANCE_INTERVAL,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::oneshot;
#[cfg(test)]
mod tests;
struct Completion {
    lost: Arc<AtomicBool>,
    done: bool,
}
impl Drop for Completion {
    fn drop(&mut self) {
        if !self.done {
            self.lost.store(true, Ordering::SeqCst);
        }
    }
}
impl AdmissionOwner {
    pub(super) fn start_maintenance(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        let closed = self.closed.clone();
        let lost = self.maintenance_lost.clone();
        self.tracker.spawn(async move {
            let mut completion = Completion { lost, done: false };
            loop {
                let Some(owner) = weak.upgrade() else { break; };
                if owner.closed.is_cancelled() { break; }
                let result = owner.maintain().await;
                if let Err(error) = result { tracing::warn!(category=%error,"Playback receipt maintenance deferred"); }
                drop(owner);
                tokio::select! {biased;_=closed.cancelled()=>break,_=tokio::time::sleep(MAINTENANCE_INTERVAL)=>{}}
            }
            completion.done = true;
        });
    }
    pub async fn maintain(self: &Arc<Self>) -> Result<MaintenanceReceipt, AdmissionError> {
        self.enqueue_maintenance()?
            .await
            .map_err(|_| AdmissionError::Storage)?
    }
    fn enqueue_maintenance(
        self: &Arc<Self>,
    ) -> Result<oneshot::Receiver<Result<MaintenanceReceipt, AdmissionError>>, AdmissionError> {
        let permit = self
            .maintenance
            .clone()
            .try_acquire_owned()
            .map_err(|_| AdmissionError::Pending)?;
        let gate = self.gate.lock().map_err(|_| AdmissionError::Closed)?;
        if gate.closed || self.maintenance_lost.load(Ordering::SeqCst) {
            return Err(AdmissionError::Closed);
        }
        let (send, receive) = oneshot::channel();
        let owner = self.clone();
        let task = self.tracker.spawn(async move {
            let _permit = permit;
            let mut completion = Completion {
                lost: owner.maintenance_lost.clone(),
                done: false,
            };
            let result = owner.ledger.prune().await;
            let _ = send.send(result);
            completion.done = true;
        });
        #[cfg(test)]
        {
            *self
                .maintenance_task
                .lock()
                .expect("test maintenance registration") = Some(task.abort_handle());
        }
        #[cfg(not(test))]
        drop(task);
        drop(gate);
        Ok(receive)
    }
}
