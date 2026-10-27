// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    database,
    preparation::{self, PreparedCommand},
    types::ReadyResponse,
};
use crate::{
    domains::playback,
    services::{
        playback_admission::{AdmissionError, AdmissionOwner},
        vod_lifecycle::Generation,
    },
    state::AppState,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub(crate) struct Commands {
    viewers: Mutex<BTreeMap<Uuid, Arc<Viewer>>>,
}
pub(super) struct Viewer {
    pub prepared: PreparedCommand,
    pub group: Arc<Generation>,
    pub generation: Option<Uuid>,
    pub published: AtomicBool,
    pub cleanup_uncertain: AtomicBool,
    pub retry: Notify,
}

impl Commands {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            viewers: Mutex::new(BTreeMap::new()),
        })
    }
    pub(super) fn register(
        &self,
        prepared: PreparedCommand,
    ) -> Result<Arc<Viewer>, AdmissionError> {
        let mut viewers = self.viewers.lock().map_err(|_| AdmissionError::Closed)?;
        if let Some(saved) = viewers.get(&prepared.request.id) {
            return Err(
                if saved.prepared.binding.intent == prepared.binding.intent
                    && same_actor(saved.prepared.request, prepared.request)
                {
                    AdmissionError::Pending
                } else {
                    AdmissionError::Conflict
                },
            );
        }
        if viewers.len() >= 128 {
            return Err(AdmissionError::Capacity);
        }
        let generation = prepared.plan.stream_plan.map(|_| Uuid::now_v7());
        let viewer = Arc::new(Viewer {
            prepared,
            group: Generation::viewer(),
            generation,
            published: AtomicBool::new(false),
            cleanup_uncertain: AtomicBool::new(false),
            retry: Notify::new(),
        });
        viewers.insert(viewer.prepared.request.id, viewer.clone());
        Ok(viewer)
    }
    pub(super) fn get(&self, id: Uuid) -> Option<Arc<Viewer>> {
        self.viewers.lock().ok()?.get(&id).cloned()
    }
    pub(super) fn remove(&self, viewer: &Arc<Viewer>) -> Result<(), AdmissionError> {
        let mut viewers = self.viewers.lock().map_err(|_| AdmissionError::Closed)?;
        if viewers
            .get(&viewer.prepared.request.id)
            .is_some_and(|current| Arc::ptr_eq(current, viewer))
        {
            viewers.remove(&viewer.prepared.request.id);
        }
        Ok(())
    }
}

fn same_actor(
    a: crate::services::playback_admission::Request,
    b: crate::services::playback_admission::Request,
) -> bool {
    a.actor.user == b.actor.user
        && a.actor.session == b.actor.session
        && a.actor.profile == b.actor.profile
        && a.actor.version == b.actor.version
        && a.actor.transport == b.actor.transport
        && a.profile_revision == b.profile_revision
        && a.subject == b.subject
}

impl Viewer {
    pub(super) async fn run(
        self: Arc<Self>,
        state: AppState,
        owner: Arc<AdmissionOwner>,
        cancel: CancellationToken,
    ) -> Result<(), AdmissionError> {
        if self.prepare_output(&state, &owner, &cancel).await.is_err() {
            let id = self.prepared.request.id;
            let _ = owner
                .sql(true, move |conn| {
                    Box::pin(async move { database::failed(conn, id).await })
                })
                .await;
            cancel.cancel();
        } else {
            self.published.store(true, Ordering::Release);
        }
        let physical = async {
            if let Some(id) = self.generation {
                state.transcode_manager.output_retired(id).await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        tokio::select! {
            _ = cancel.cancelled() => {},
            _ = self.group.stop.cancelled() => {},
            _ = physical => {},
        }
        cancel.cancel();
        self.published.store(false, Ordering::Release);
        self.group.retire();
        loop {
            let notified = self.retry.notified();
            if self.cleanup(&state, &owner).await.is_ok() {
                state.vod_commands.remove(&self)?;
                return Ok(());
            }
            self.cleanup_uncertain.store(true, Ordering::Release);
            let id = self.prepared.request.id;
            let _=owner.sql(true,move|conn|Box::pin(async move {
                sqlx::query("UPDATE playback_vod_commands SET cleanup_uncertain=true WHERE reservation_id=$1 AND NOT resources_completed").bind(id).execute(conn).await?;
                Ok(())
            })).await;
            notified.await;
        }
    }

    async fn prepare_output(
        self: &Arc<Self>,
        state: &AppState,
        owner: &Arc<AdmissionOwner>,
        cancel: &CancellationToken,
    ) -> Result<(), AdmissionError> {
        if cancel.is_cancelled() {
            return Err(AdmissionError::Retired);
        }
        let (pending, stream_offset_ms) = if let Some(id) = self.generation {
            let params =
                preparation::parameters(&self.prepared.plan, self.prepared.request.actor.user)?;
            let session = state
                .transcode_manager
                .start_session_bound(params, &state.bootstrap.data_dir, id, Some(cancel.clone()))
                .await
                .map_err(|_| AdmissionError::Authority)?;
            (
                Some(state.transcode_manager.pending_session(id)),
                Some(session.source_start_ms),
            )
        } else {
            (None, None)
        };
        let identity = playback::direct::FileIdentity::read_scoped(
            &state.transcode_manager.output,
            &self.group,
            &self.prepared.plan.source.details,
        )
        .await
        .ok_or(AdmissionError::Authority)?;
        if identity != self.prepared.identity || cancel.is_cancelled() {
            return Err(AdmissionError::Authority);
        }
        let viewer = self.clone();
        owner.active_sql(self.prepared.request,move|conn|Box::pin(async move {
            let source=&viewer.prepared.plan.source;
            let details=playback::media_source::fetch_media_file_details_on_connection(conn,source.details.id,source.media_item_id).await.map_err(|_|AdmissionError::Authority)?;
            if !viewer.prepared.identity.matches_details(&details){return Err(AdmissionError::Authority);}
            let waiting:bool=sqlx::query_scalar("SELECT state='preparing' FROM playback_vod_commands WHERE reservation_id=$1 FOR UPDATE").bind(viewer.prepared.request.id).fetch_one(&mut *conn).await?;
            if !waiting {return Err(AdmissionError::Retired);}
            let session=playback::service::create_play_session(&mut *conn,viewer.prepared.request.actor.user,viewer.prepared.request.actor.profile,source.media_item_id,source.library_id,viewer.prepared.binding.mode.as_str(),viewer.generation,Some(source.details.id),None,None,&source.playback_mode,source.ambient_channel.as_ref(),None,None).await.map_err(|_|AdmissionError::Storage)?;
            let started:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("SELECT transaction_timestamp()").fetch_one(&mut *conn).await?;
            if sqlx::query("UPDATE play_sessions SET metadata=metadata||'{\"vod_command_protocol\":1}'::jsonb WHERE id=$1 AND started_at=$2").bind(session).bind(started).execute(&mut *conn).await?.rows_affected()!=1 {return Err(AdmissionError::Storage);}
            let result=ReadyResponse{session_id:session,media_item_id:source.media_item_id,media_file_id:source.details.id,delivery_mode:viewer.prepared.binding.mode.as_str().into(),audio_stream_index:source.tracks.audio_stream_index,subtitle_stream_index:source.tracks.subtitle_stream_index,playback_mode:source.playback_mode.clone(),stream_offset_ms};
            sqlx::query("UPDATE playback_vod_commands SET state='ready',history_id=$2,history_started_at=$3,generation_id=$4,result=$5 WHERE reservation_id=$1 AND state='preparing'")
                .bind(viewer.prepared.request.id).bind(session).bind(started).bind(viewer.generation).bind(serde_json::to_value(result).map_err(|_|AdmissionError::Storage)?).execute(conn).await?;
            Ok(())
        })).await?;
        if let Some(pending) = pending {
            pending.acknowledge();
        }
        Ok(())
    }

    async fn cleanup(
        self: &Arc<Self>,
        state: &AppState,
        owner: &Arc<AdmissionOwner>,
    ) -> Result<(), AdmissionError> {
        self.group
            .drain()
            .await
            .map_err(|_| AdmissionError::Cleanup)?;
        if let Some(generation) = self.generation {
            state
                .transcode_manager
                .stop_session(generation)
                .await
                .map_err(|_| AdmissionError::Cleanup)?;
        }
        let request = self.prepared.request;
        owner.sql(true,move|conn|Box::pin(async move {
            crate::services::playback_admission::vod::cleanup_lock(conn,request.actor.user).await?;
            let row=database::read(conn,request.id).await?.ok_or(AdmissionError::Storage)?;
            if let (Some(id),Some(started))=(row.history_id,row.history_started_at) {
                let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM play_sessions WHERE id=$1 AND started_at=$2 AND user_id=$3)").bind(id).bind(started).bind(request.actor.user).fetch_one(&mut *conn).await?;
                if exists {playback::stop::freeze_on_connection(conn,request.actor.user,id,Some(started),None).await.map_err(|_|AdmissionError::Storage)?;}
            }
            sqlx::query("UPDATE playback_vod_commands SET resources_completed=true,state=CASE WHEN state IN ('preparing','ready') THEN 'stopping' ELSE state END WHERE reservation_id=$1").bind(request.id).execute(conn).await?;
            Ok(())
        })).await
    }
}
