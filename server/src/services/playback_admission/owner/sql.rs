// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::AdmissionOwner;
use crate::services::playback_admission::{AdmissionError, database};
use sqlx::PgConnection;
use std::{future::Future, pin::Pin, sync::Arc};

struct Completion {
    owner: Arc<AdmissionOwner>,
    done: bool,
}
impl Drop for Completion {
    fn drop(&mut self) {
        if !self.done {
            self.owner
                .failed
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

pub(crate) type Work<'a, T> = Pin<Box<dyn Future<Output = Result<T, AdmissionError>> + Send + 'a>>;

impl AdmissionOwner {
    pub(crate) async fn active_sql<T, F>(
        self: &Arc<Self>,
        request: super::super::Request,
        action: F,
    ) -> Result<T, AdmissionError>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a mut PgConnection) -> Work<'a, T> + Send + 'static,
    {
        let binding = {
            let gate = self.gate.lock().map_err(|_| AdmissionError::Closed)?;
            let entry = gate
                .entries
                .get(&request.id)
                .ok_or(AdmissionError::Retired)?;
            if entry.request.encode()? != request.encode()? || entry.cancel.is_cancelled() {
                return Err(AdmissionError::Retired);
            }
            entry.binding
        };
        let owner = self.clone();
        self.sql(false, move |conn| {
            Box::pin(async move {
                owner.ledger.check_on(conn, request, false, binding).await?;
                action(conn).await
            })
        })
        .await
    }
    pub(crate) async fn sql<T, F>(
        self: &Arc<Self>,
        cleanup: bool,
        action: F,
    ) -> Result<T, AdmissionError>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a mut PgConnection) -> Work<'a, T> + Send + 'static,
    {
        let permit = self
            .commands
            .clone()
            .try_acquire_owned()
            .map_err(|_| AdmissionError::Capacity)?;
        let receive = {
            let gate = self.gate.lock().map_err(|_| AdmissionError::Closed)?;
            if gate.closed && !cleanup {
                return Err(AdmissionError::Closed);
            }
            let (send, receive) = tokio::sync::oneshot::channel();
            let owner = self.clone();
            self.tracker.spawn(async move {
                let _permit = permit;
                let mut completion = Completion {
                    owner: owner.clone(),
                    done: false,
                };
                let result =
                    database::transaction(&owner.ledger.pool, async |conn| action(conn).await)
                        .await;
                let _ = send.send(result);
                completion.done = true;
            });
            drop(gate);
            receive
        };
        receive.await.map_err(|_| AdmissionError::Storage)?
    }
}
