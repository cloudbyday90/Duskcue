// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::AdmissionError;
use super::{AdmissionOwner, Entry};
use std::{
    future::Future,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

impl AdmissionOwner {
    pub(super) async fn monitor(self: &Arc<Self>) {
        let Ok(mut entries) = self.entries() else {
            self.failed.store(true, Ordering::SeqCst);
            return;
        };
        if !entries.is_empty() {
            let offset = self.scan.fetch_add(8, Ordering::SeqCst) % entries.len();
            entries.rotate_left(offset);
        }
        for entry in entries {
            if entry.pending.load(Ordering::SeqCst) {
                continue;
            }
            if entry.checking.swap(true, Ordering::SeqCst) {
                continue;
            }
            let Ok(permit) = self.commands.clone().try_acquire_owned() else {
                entry.checking.store(false, Ordering::SeqCst);
                continue;
            };
            let Ok(gate) = self.gate.lock() else {
                self.failed.store(true, Ordering::SeqCst);
                return;
            };
            if gate.closed {
                return;
            }
            let owner = self.clone();
            self.tracker.spawn(async move {
                let _permit = permit;
                if !entry.cancel.is_cancelled()
                    && owner.ledger.check(entry.request, false).await.is_err()
                {
                    entry.cancel.cancel();
                }
                if entry.cancel.is_cancelled() {
                    if owner.ledger.retire(entry.request, false).await.is_err() {
                        owner.failed.store(true, Ordering::SeqCst);
                    }
                    if !entry.attached.load(Ordering::SeqCst)
                        && owner.ledger.retire(entry.request, true).await.is_ok()
                    {
                        owner.remove(entry.request.id);
                    }
                }
                entry.checking.store(false, Ordering::SeqCst);
            });
            drop(gate);
        }
    }

    pub(super) async fn resource<F, Fut>(self: Arc<Self>, entry: Arc<Entry>, factory: F)
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), AdmissionError>> + Send + 'static,
    {
        let token = entry.cancel.clone();
        let mut deadline = entry.deadline.subscribe();
        let mut task = tokio::spawn(async move { factory(token).await });
        let result = loop {
            let expires = (*deadline.borrow()).unwrap_or_else(Instant::now);
            break tokio::select! {
                biased;
                result=&mut task=>result,
                _=entry.cancel.cancelled()=> {
                    {let _permit=self.commands.acquire().await;let _=self.ledger.retire(entry.request,false).await;}
                    match tokio::time::timeout(Duration::from_secs(10),&mut task).await {
                        Ok(result)=>result,
                        Err(_)=>{self.failed.store(true,Ordering::SeqCst);task.await}
                    }
                }
                _=tokio::time::sleep_until(expires)=>{entry.cancel.cancel();continue;},
                changed=deadline.changed()=>{if changed.is_err(){entry.cancel.cancel();}continue;},
            };
        };
        entry.cancel.cancel();
        if !matches!(result, Ok(Ok(()))) {
            {
                let _permit = self.commands.acquire().await;
                let _ = self.ledger.retire(entry.request, false).await;
            }
            self.failed.store(true, Ordering::SeqCst);
            return;
        }
        let _permit = self.commands.acquire().await;
        if self.ledger.retire(entry.request, true).await.is_ok() {
            self.remove(entry.request.id);
        } else {
            self.failed.store(true, Ordering::SeqCst);
        }
    }
}
