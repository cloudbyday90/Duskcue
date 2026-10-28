// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{control, database, ownership::Viewer};
use crate::{
    domains::playback,
    services::playback_admission::{AdmissionError, AdmissionOwner, vod::revisions},
    state::AppState,
};
use std::sync::{Arc, atomic::Ordering};
use tokio_util::sync::CancellationToken;

impl Viewer {
    pub(super) async fn run(
        self: Arc<Self>,
        state: AppState,
        owner: Arc<AdmissionOwner>,
        cancel: CancellationToken,
    ) -> Result<(), AdmissionError> {
        let initial = self.current()?;
        let output = initial.clone();
        let request = self.prepared.request;
        let setup=owner.active_sql(request,move|conn|Box::pin(async move {
            output.insert(conn).await?;
            sqlx::query("INSERT INTO playback_vod_controls(reservation_id,current_output,claim_encodes) VALUES ($1,$2,$3)").bind(request.id).bind(output.id).bind(output.prepared.binding.encodes).execute(conn).await?;
            Ok(())
        })).await;
        if setup.is_err()
            || initial
                .prepare_output(&state, &owner, &self, &cancel)
                .await
                .is_err()
        {
            let _ = owner
                .sql(true, move |conn| {
                    Box::pin(async move { database::failed(conn, request.id).await })
                })
                .await;
            cancel.cancel();
        }
        loop {
            let notified = self.changed.notified();
            if cancel.is_cancelled() || !self.group.active() {
                break;
            }
            if let Some(work) = self.take()? {
                let result = control::execute(&state, &owner, &self, work, &cancel).await;
                if matches!(
                    result,
                    Err(AdmissionError::Cleanup | AdmissionError::Indeterminate)
                ) {
                    self.cleanup_uncertain.store(true, Ordering::Release);
                }
                if result.is_err() {
                    cancel.cancel();
                }
                self.finish();
                continue;
            }
            let current = self.current()?;
            let physical = async {
                if current.published.load(Ordering::Acquire)
                    && let Some(id) = current.generation
                {
                    state.transcode_manager.output_retired(id).await;
                } else {
                    std::future::pending::<()>().await;
                }
            };
            let failed = tokio::select! {biased;
                _=cancel.cancelled()=>false,
                _=self.group.stop.cancelled()=>false,
                _=notified=>{continue;},
                _=physical=>true,
            };
            if !failed {
                break;
            }
            if current.snapshot(&state).is_err() {
                cancel.cancel();
                continue;
            }
            let id = current.id;
            let changed=owner.active_sql(request,move|conn|Box::pin(async move {
                let row=revisions::control(conn,request.id).await?;
                if row.pending_operation.is_some() {return Ok(false);}
                if row.current_output!=Some(id) {return Err(AdmissionError::Conflict);}
                sqlx::query("UPDATE playback_vod_controls SET state='unavailable' WHERE reservation_id=$1").bind(request.id).execute(conn).await?;
                Ok(true)
            })).await;
            match changed {
                Ok(false) => continue,
                Ok(true) => {
                    if current.clean(&state, &owner).await.is_err() {
                        self.cleanup_uncertain.store(true, Ordering::Release);
                        cancel.cancel();
                    }
                }
                Err(_) => cancel.cancel(),
            }
        }
        cancel.cancel();
        self.group.retire();
        for output in self.outputs()? {
            output.published.store(false, Ordering::Release);
            output.group.retire();
        }
        self.group
            .drain()
            .await
            .map_err(|_| AdmissionError::Cleanup)?;
        loop {
            let retry = self.retry.notified();
            if self.cleanup(&state, &owner).await.is_ok() {
                state.vod_commands.remove(&self)?;
                return Ok(());
            }
            self.cleanup_uncertain.store(true, Ordering::Release);
            let _=owner.sql(true,move|conn|Box::pin(async move {
                sqlx::query("UPDATE playback_vod_commands SET cleanup_uncertain=true WHERE reservation_id=$1 AND NOT resources_completed").bind(request.id).execute(conn).await?;
                Ok(())
            })).await;
            retry.await;
        }
    }

    async fn cleanup(
        self: &Arc<Self>,
        state: &AppState,
        owner: &Arc<AdmissionOwner>,
    ) -> Result<(), AdmissionError> {
        for output in self.outputs()? {
            output.clean(state, owner).await?;
        }
        let request = self.prepared.request;
        owner.sql(true,move|conn|Box::pin(async move {
            crate::services::playback_admission::vod::cleanup_lock(conn,request.actor.user).await?;
            let row=database::read(conn,request.id).await?.ok_or(AdmissionError::Storage)?;
            revisions::complete(conn,request.id).await?;
            if let (Some(id),Some(started))=(row.history_id,row.history_started_at) {
                let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM play_sessions WHERE id=$1 AND started_at=$2 AND user_id=$3)").bind(id).bind(started).bind(request.actor.user).fetch_one(&mut *conn).await?;
                if exists {playback::stop::freeze_on_connection(conn,request.actor.user,id,Some(started),None).await.map_err(|_|AdmissionError::Storage)?;}
            }
            sqlx::query("UPDATE playback_vod_commands SET resources_completed=true,state=CASE WHEN state IN ('preparing','ready') THEN 'stopping' ELSE state END WHERE reservation_id=$1").bind(request.id).execute(conn).await?;
            Ok(())
        })).await
    }
}
