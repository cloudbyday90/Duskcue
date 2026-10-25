// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    admission::{self, Request},
    error::RefreshError,
    types::AcceptedResponse,
};
use crate::{
    domains::live_tv::feed::{
        imports::{ImportError, operation},
        tls,
    },
    error::AppError,
    services::encryption::EncryptionKey,
    state::AppState,
};
use sqlx::PgPool;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};
use tokio_rustls::TlsConnector;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use uuid::Uuid;

struct State {
    closed: bool,
    quarantined: bool,
    attempt: Option<Uuid>,
    active: Option<CancellationToken>,
}
pub struct RefreshManager {
    pool: PgPool,
    key: Arc<EncryptionKey>,
    tls: TlsConnector,
    state: Mutex<State>,
    capacity: Arc<Semaphore>,
    stop: CancellationToken,
    tracker: TaskTracker,
}
struct Caller(CancellationToken);
impl Drop for Caller {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

struct Lease {
    manager: Arc<RefreshManager>,
    permit: Option<OwnedSemaphorePermit>,
}
impl Lease {
    fn finish(mut self, cleanup_failed: bool) {
        if let Ok(mut state) = self.manager.state.lock() {
            state.attempt = None;
            state.active = None;
            state.quarantined |= cleanup_failed;
        }
        if let Some(permit) = self.permit.take()
            && cleanup_failed
        {
            permit.forget();
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(permit) = self.permit.take() {
            if let Ok(mut state) = self.manager.state.lock() {
                state.quarantined = true;
            }
            self.manager.stop.cancel();
            permit.forget();
        }
    }
}

impl RefreshManager {
    pub fn attach(state: &AppState) -> Result<Arc<Self>, AppError> {
        Self::attach_with(state, false)
    }
    #[cfg(feature = "playback-qualification")]
    pub fn qualification_fixture(state: &AppState) -> Result<Arc<Self>, AppError> {
        Self::attach_with(state, true)
    }
    fn attach_with(state: &AppState, fixture: bool) -> Result<Arc<Self>, AppError> {
        let owner = Arc::new(Self {
            pool: state.pool.clone(),
            key: state.encryption_key.clone(),
            tls: tls::connector(fixture).map_err(|_| RefreshError::Unavailable)?,
            state: Mutex::new(State {
                closed: false,
                quarantined: false,
                attempt: None,
                active: None,
            }),
            capacity: Arc::new(Semaphore::new(1)),
            stop: CancellationToken::new(),
            tracker: TaskTracker::new(),
        });
        state
            .live_tv_refresh
            .set(owner.clone())
            .map_err(|_| RefreshError::Unavailable)?;
        Ok(owner)
    }
    pub(super) async fn start(
        self: &Arc<Self>,
        mut request: Request,
    ) -> Result<AcceptedResponse, AppError> {
        let cancel = CancellationToken::new();
        request.cancelled = cancel.clone();
        let _caller = Caller(cancel);
        let permit = {
            let state = self.state.lock().map_err(|_| RefreshError::Unavailable)?;
            if state.closed || state.quarantined {
                return Err(RefreshError::Unavailable.into());
            }
            self.capacity
                .clone()
                .try_acquire_owned()
                .map_err(|_| RefreshError::Unavailable)?
        };
        let (sender, receiver) = oneshot::channel();
        let manager = self.clone();
        let token = self.stop.child_token();
        {
            let mut state = self.state.lock().map_err(|_| RefreshError::Unavailable)?;
            if state.closed {
                return Err(RefreshError::Unavailable.into());
            }
            state.active = Some(token.clone());
            let lease = Lease {
                manager: manager.clone(),
                permit: Some(permit),
            };
            self.tracker.spawn(async move {
                let deadline = Instant::now() + Duration::from_secs(180);
                let target = request.target;
                let result = match admission::claim(&manager.pool, &manager.key, request, &token)
                    .await
                {
                    Ok(admitted) => {
                        if let Ok(mut state) = manager.state.lock() {
                            state.attempt = Some(admitted.receipt.attempt);
                        }
                        let accepted = AcceptedResponse {
                            attempt_id: admitted.receipt.attempt,
                            attempt_sequence: admitted.receipt.sequence,
                            status_url: format!(
                                "/api/v1/live-tv/households/{target}/refresh/status?attempt_id={}",
                                admitted.receipt.attempt
                            ),
                        };
                        let _ = sender.send(Ok(accepted));
                        operation::execute(
                            &manager.pool,
                            manager.key.clone(),
                            &manager.tls,
                            admitted.receipt,
                            admitted.sources,
                            &token,
                            deadline,
                        )
                        .await
                    }
                    Err(error) => {
                        let cleanup =
                            matches!(&error, AppError::IptvRefresh(RefreshError::Cleanup));
                        let _ = sender.send(Err(error));
                        if cleanup {
                            Err(ImportError::Cleanup)
                        } else {
                            Err(ImportError::Authority)
                        }
                    }
                };
                lease.finish(result == Err(ImportError::Cleanup));
                tracing::debug!(
                    event = "manual_refresh_finished",
                    succeeded = result.is_ok()
                );
            });
        }
        receiver.await.map_err(|_| RefreshError::Unavailable)?
    }
    pub fn owns(&self, attempt: Uuid) -> bool {
        self.state.lock().is_ok_and(|s| s.attempt == Some(attempt))
    }
    pub fn available(&self) -> bool {
        self.state.lock().is_ok_and(|s| !s.closed && !s.quarantined)
    }
    pub fn idle(&self) -> bool {
        self.tracker.is_empty() && self.capacity.available_permits() == 1
    }
    pub fn cancel_active(&self) {
        if let Ok(state) = self.state.lock()
            && let Some(token) = &state.active
        {
            token.cancel();
        }
    }
    pub fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
        }
        self.stop.cancel();
        self.tracker.close();
    }
    pub async fn drain(&self) -> Result<(), ImportError> {
        self.tracker.wait().await;
        if self.idle()
            && self
                .state
                .lock()
                .is_ok_and(|s| !s.quarantined && s.active.is_none() && s.attempt.is_none())
        {
            Ok(())
        } else {
            Err(ImportError::Cleanup)
        }
    }
    pub async fn shutdown(&self) -> Result<(), ImportError> {
        self.close();
        tokio::time::timeout(Duration::from_secs(30), self.drain())
            .await
            .map_err(|_| ImportError::Cleanup)?
    }
}
impl Drop for RefreshManager {
    fn drop(&mut self) {
        self.stop.cancel();
        self.tracker.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn abandoned_task_lease_quarantines_root_cleanup() {
        let pool = PgPool::connect_lazy("postgres://127.0.0.1/duskcue").unwrap();
        let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
        let state = AppState::new(
            pool,
            crate::config::BootstrapConfig {
                database_url: None,
                bind_address: "127.0.0.1".into(),
                port: 0,
                data_dir: std::env::temp_dir(),
                cache_dir: std::env::temp_dir(),
                log_level: "error".into(),
                environment: "test".into(),
                encryption_key: None,
                geoip_license_key: None,
            },
            recorder.handle(),
            EncryptionKey::generate().0,
        );
        let manager = RefreshManager::attach(&state).unwrap();
        let lease = Lease {
            manager: manager.clone(),
            permit: Some(manager.capacity.clone().try_acquire_owned().unwrap()),
        };
        let task = manager.tracker.spawn(async move {
            let _lease = lease;
            panic!("qualification: unexpected refresh task exit");
        });
        assert!(task.await.unwrap_err().is_panic());
        assert!(!manager.available());
        assert!(!manager.idle());
        assert_eq!(manager.shutdown().await, Err(ImportError::Cleanup));
    }
}
