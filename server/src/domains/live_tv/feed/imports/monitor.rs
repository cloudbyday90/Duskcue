// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{ImportError, receipt::Receipt};
use sqlx::PgPool;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub(super) struct Monitor {
    pool: PgPool,
    receipt: Receipt,
    cancel: CancellationToken,
    task: Option<JoinHandle<ImportError>>,
}

impl Monitor {
    pub fn start(pool: PgPool, receipt: Receipt) -> Self {
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task_pool = pool.clone();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _=token.cancelled()=>return ImportError::Cancelled,
                    _=tokio::time::sleep(Duration::from_millis(200))=>{},
                }
                if let Err(error) = receipt.current(&task_pool).await {
                    return error;
                }
            }
        });
        Self {
            pool,
            receipt,
            cancel,
            task: Some(task),
        }
    }
    pub async fn current(&self) -> Result<(), ImportError> {
        self.receipt.current(&self.pool).await
    }
    pub async fn error(&mut self) -> ImportError {
        let Some(task) = self.task.as_mut() else {
            return ImportError::Cleanup;
        };
        let result = task.await.unwrap_or(ImportError::Cleanup);
        self.task = None;
        result
    }
    pub async fn stop(mut self) -> Result<(), ImportError> {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            match task.await.map_err(|_| ImportError::Cleanup)? {
                ImportError::Cancelled => Ok(()),
                error => Err(error),
            }
        } else {
            Ok(())
        }
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
