// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;

struct OwnedStart {
    session: TranscodeSession,
    cancellation: super::lifecycle::WorkerCancellation,
}

impl TranscodeManager {
    pub async fn start_session(
        &self,
        params: StartSessionParams,
        data_dir: &Path,
    ) -> Result<TranscodeSession, PlaybackError> {
        let manager = self.clone();
        let data_dir = data_dir.to_path_buf();
        let first_id = Uuid::now_v7();
        let result = self
            .output
            .start_command(first_id, move |stop| async move {
                let result = manager
                    .start_owned(params, &data_dir, &stop, first_id)
                    .await;
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
