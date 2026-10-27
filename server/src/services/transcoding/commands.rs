// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;
use tokio_util::sync::CancellationToken;

struct OwnedStart {
    session: TranscodeSession,
    cancellation: super::lifecycle::WorkerCancellation,
}

impl TranscodeManager {
    pub(crate) fn output_usable(&self, id: Uuid) -> bool {
        self.output
            .generation(id)
            .is_some_and(|generation| generation.active())
            && self.workers.execution(id).is_some_and(|execution| {
                matches!(
                    execution.state(),
                    lifecycle::ExecutionState::Running | lifecycle::ExecutionState::Succeeded
                )
            })
    }

    pub(crate) async fn output_retired(&self, id: Uuid) {
        let Some(generation) = self.output.generation(id) else {
            return;
        };
        let Some(mut execution) = self.workers.execution(id) else {
            return;
        };
        loop {
            match execution.state() {
                lifecycle::ExecutionState::Succeeded => {
                    generation.stop.cancelled().await;
                    return;
                }
                lifecycle::ExecutionState::Running => {}
                _ => return,
            }
            tokio::select! {
                _ = generation.stop.cancelled() => return,
                _ = execution.changed() => {},
            }
        }
    }

    pub async fn start_session(
        &self,
        params: StartSessionParams,
        data_dir: &Path,
    ) -> Result<TranscodeSession, PlaybackError> {
        self.start_session_bound(params, data_dir, Uuid::now_v7(), None)
            .await
    }

    pub(crate) async fn start_session_bound(
        &self,
        params: StartSessionParams,
        data_dir: &Path,
        first_id: Uuid,
        external: Option<CancellationToken>,
    ) -> Result<TranscodeSession, PlaybackError> {
        let manager = self.clone();
        let data_dir = data_dir.to_path_buf();
        let result = self
            .output
            .start_command(first_id, move |stop| async move {
                let local = stop.child_token();
                let external = external.unwrap_or_default();
                let result = {
                    let started = manager.start_owned(params, &data_dir, &local, first_id);
                    tokio::pin!(started);
                    tokio::select! { biased;
                        _=external.cancelled()=>{local.cancel();started.await},
                        result=&mut started=>result,
                    }
                };
                if result.is_err()
                    && !manager.sessions.contains_key(&first_id)
                    && !manager.preparations.contains_key(&first_id)
                {
                    manager.output.finish_generation(first_id);
                }
                let session = result?;
                let cancellation = manager
                    .workers
                    .pending(session.id)
                    .ok_or(PlaybackError::LifecycleCleanup)?;
                Ok(OwnedStart {
                    session,
                    cancellation,
                })
            })
            .await;
        let mut started = match result {
            Ok(started) => started,
            Err(error) => {
                if !self.sessions.contains_key(&first_id)
                    && !self.preparations.contains_key(&first_id)
                {
                    self.output.finish_generation(first_id);
                }
                return Err(error);
            }
        };
        started.cancellation.acknowledge();
        Ok(started.session)
    }

    pub(super) async fn replace_session(
        &self,
        old_id: Uuid,
        params: StartSessionParams,
        data_dir: &Path,
    ) -> Result<TranscodeSession, PlaybackError> {
        let manager = self.clone();
        let data_dir = data_dir.to_path_buf();
        let mut started = self
            .output
            .command(false, move |stop| async move {
                manager.stop_owned(old_id).await?;
                if stop.is_cancelled() {
                    return Err(PlaybackError::LifecycleUnavailable);
                }
                let id = Uuid::now_v7();
                manager.output.begin_generation(id, false)?;
                let result = manager.start_owned(params, &data_dir, &stop, id).await;
                if result.is_err()
                    && !manager.sessions.contains_key(&id)
                    && !manager.preparations.contains_key(&id)
                {
                    manager.output.finish_generation(id);
                }
                let session = result?;
                let cancellation = manager
                    .workers
                    .pending(session.id)
                    .ok_or(PlaybackError::LifecycleCleanup)?;
                Ok(OwnedStart {
                    session,
                    cancellation,
                })
            })
            .await?;
        started.cancellation.acknowledge();
        Ok(started.session)
    }
}
