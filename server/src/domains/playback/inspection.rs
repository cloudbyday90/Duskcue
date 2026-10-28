// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use crate::services::vod_lifecycle::VodLifecycle;
use std::{future::Future, sync::Arc};
use tokio_util::sync::CancellationToken;

tokio::task_local! {
    static INSPECTION: (Arc<VodLifecycle>, CancellationToken);
}

pub(super) fn active() -> bool {
    INSPECTION.try_with(|_| ()).is_ok()
}

pub(super) async fn scope<T>(
    owner: Arc<VodLifecycle>,
    stop: CancellationToken,
    work: impl Future<Output = T>,
) -> T {
    INSPECTION.scope((owner, stop), work).await
}

pub(super) async fn run<T: Send + 'static>(
    work: impl FnOnce() -> Option<T> + Send + 'static,
) -> Option<T> {
    let (owner, stop) = INSPECTION.try_with(Clone::clone).ok()?;
    let permit = owner.inspection_permit(&stop).await.ok()?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if stop.is_cancelled() {
            return None;
        }
        work()
    })
    .await
    .ok()
    .flatten()
}
