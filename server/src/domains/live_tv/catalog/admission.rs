// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::error::CatalogError;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub struct CatalogReads {
    slots: Arc<Semaphore>,
}

impl Default for CatalogReads {
    fn default() -> Self {
        Self {
            slots: Arc::new(Semaphore::new(4)),
        }
    }
}

impl CatalogReads {
    pub fn close(&self) {
        self.slots.close();
    }

    pub(crate) fn admit(&self) -> Result<OwnedSemaphorePermit, CatalogError> {
        self.slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| CatalogError::Unavailable)
    }
}
