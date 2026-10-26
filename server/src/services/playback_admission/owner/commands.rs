// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::{AdmissionError, Receipt, Request, ReservationState};
use super::{AdmissionOwner, Entry};
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    sync::{oneshot, watch},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

impl AdmissionOwner {
    pub async fn start<F, Fut>(
        self: &Arc<Self>,
        request: Request,
        factory: F,
    ) -> Result<Receipt, AdmissionError>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), AdmissionError>> + Send + 'static,
    {
        let receive = self.enqueue(request, factory)?;
        receive.await.map_err(|_| AdmissionError::Storage)?
    }

    fn enqueue<F, Fut>(
        self: &Arc<Self>,
        request: Request,
        factory: F,
    ) -> Result<oneshot::Receiver<Result<Receipt, AdmissionError>>, AdmissionError>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), AdmissionError>> + Send + 'static,
    {
        request.encode()?;
        let permit = self
            .commands
            .clone()
            .try_acquire_owned()
            .map_err(|_| AdmissionError::Capacity)?;
        let (send, receive) = oneshot::channel();
        let mut gate = self.gate.lock().map_err(|_| AdmissionError::Closed)?;
        if gate.closed || self.failed.load(Ordering::SeqCst) {
            return Err(AdmissionError::Closed);
        }
        let (entry, inserted) = if let Some(entry) = gate.entries.get(&request.id) {
            if entry.request.encode()? != request.encode()? {
                return Err(AdmissionError::Conflict);
            }
            if entry.pending.load(Ordering::SeqCst) {
                return Err(AdmissionError::Pending);
            }
            (entry.clone(), false)
        } else {
            if gate.entries.len() >= 128 {
                return Err(AdmissionError::Capacity);
            }
            let entry = Arc::new(Entry {
                request,
                cancel: CancellationToken::new(),
                pending: AtomicBool::new(true),
                attached: AtomicBool::new(false),
                checking: AtomicBool::new(false),
                deadline: watch::channel(None).0,
            });
            gate.entries.insert(request.id, entry.clone());
            (entry, true)
        };
        let owner = self.clone();
        self.tracker.spawn(async move {
            let _permit = permit;
            let began = Instant::now();
            let result = owner.ledger.claim(request, inserted).await;
            if inserted {
                match &result {
                    Ok(receipt) if receipt.fresh && receipt.state == ReservationState::Held => {
                        entry.deadline.send_replace(Some(
                            began
                                + (receipt.deadline - receipt.observed_at)
                                    .to_std()
                                    .unwrap_or_default(),
                        ));
                        let mut launch = false;
                        if let Ok(gate) = owner.gate.lock()
                            && !gate.closed
                            && !entry.cancel.is_cancelled()
                        {
                            entry.attached.store(true, Ordering::SeqCst);
                            let resource_owner = owner.clone();
                            let resource_entry = entry.clone();
                            owner.tracker.spawn(async move {
                                resource_owner.resource(resource_entry, factory).await;
                            });
                            launch = true;
                        }
                        if !launch {
                            entry.cancel.cancel();
                        }
                    }
                    Ok(_) => {
                        owner.remove(request.id);
                    }
                    Err(AdmissionError::Indeterminate) => {
                        entry.cancel.cancel();
                    }
                    Err(_) => {
                        owner.remove(request.id);
                    }
                }
                entry.pending.store(false, Ordering::SeqCst);
            }
            let _ = send.send(result);
        });
        drop(gate);
        Ok(receive)
    }

    pub async fn renew(self: &Arc<Self>, request: Request) -> Result<Receipt, AdmissionError> {
        self.check(request, true).await
    }

    pub async fn authorize_use(
        self: &Arc<Self>,
        request: Request,
    ) -> Result<Receipt, AdmissionError> {
        self.check(request, false).await
    }

    async fn check(
        self: &Arc<Self>,
        request: Request,
        renew: bool,
    ) -> Result<Receipt, AdmissionError> {
        let receive = self.queue_check(request, renew)?;
        receive.await.map_err(|_| AdmissionError::Storage)?
    }

    fn queue_check(
        self: &Arc<Self>,
        request: Request,
        renew: bool,
    ) -> Result<oneshot::Receiver<Result<Receipt, AdmissionError>>, AdmissionError> {
        let permit = self
            .commands
            .clone()
            .try_acquire_owned()
            .map_err(|_| AdmissionError::Capacity)?;
        let (send, receive) = oneshot::channel();
        let gate = self.gate.lock().map_err(|_| AdmissionError::Closed)?;
        if gate.closed || self.failed.load(Ordering::SeqCst) {
            return Err(AdmissionError::Closed);
        }
        let entry = gate
            .entries
            .get(&request.id)
            .cloned()
            .ok_or(AdmissionError::Retired)?;
        if entry.request.encode()? != request.encode()? {
            return Err(AdmissionError::Conflict);
        }
        if entry.pending.load(Ordering::SeqCst) || entry.cancel.is_cancelled() {
            return Err(AdmissionError::Retired);
        }
        let owner = self.clone();
        self.tracker.spawn(async move {
            let _permit = permit;
            let began = Instant::now();
            let result = owner.ledger.check(request, renew).await;
            if renew && let Ok(receipt) = &result {
                entry.deadline.send_replace(Some(
                    began
                        + (receipt.deadline - receipt.observed_at)
                            .to_std()
                            .unwrap_or_default(),
                ));
            }
            if result.is_err() {
                entry.cancel.cancel();
            }
            let _ = send.send(result);
        });
        drop(gate);
        Ok(receive)
    }
}
