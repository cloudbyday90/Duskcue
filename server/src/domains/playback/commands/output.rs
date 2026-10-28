// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::preparation::{self, PreparedCommand};
use crate::{
    domains::playback,
    services::{
        playback_admission::{AdmissionError, AdmissionOwner, vod::revisions},
        transcoding::{StartSessionParams, TranscodeSession},
        vod_lifecycle::Generation,
    },
    state::AppState,
};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

pub(super) struct Output {
    pub id: Uuid,
    pub operation: Option<Uuid>,
    pub prepared: Arc<PreparedCommand>,
    pub group: Arc<Generation>,
    pub generation: Option<Uuid>,
    pub parameters: Option<StartSessionParams>,
    pub position: i32,
    pub published: AtomicBool,
    pub cleaned: AtomicBool,
    pub session: Mutex<Option<TranscodeSession>>,
    pub failure: Mutex<Option<crate::services::transcoding::FailureClass>>,
    pub recovery: Option<Value>,
}

impl Output {
    pub(super) fn new(
        prepared: Arc<PreparedCommand>,
        operation: Option<Uuid>,
        position: i32,
        parameters: Option<StartSessionParams>,
        recovery: Option<Value>,
    ) -> Result<Arc<Self>, AdmissionError> {
        let parameters = match parameters {
            Some(params) => Some(params),
            None if prepared.plan.stream_plan.is_some() => Some(preparation::parameters(
                &prepared.plan,
                prepared.request.actor.user,
            )?),
            None => None,
        }
        .map(|mut params| {
            params.seek_position_ms = (position > 0).then_some(i64::from(position));
            params
        });
        Ok(Arc::new(Self {
            id: Uuid::now_v7(),
            operation,
            generation: parameters.as_ref().map(|_| Uuid::now_v7()),
            parameters,
            position,
            prepared,
            recovery,
            group: Generation::viewer(),
            published: AtomicBool::new(false),
            cleaned: AtomicBool::new(false),
            session: Mutex::new(None),
            failure: Mutex::new(None),
        }))
    }
    pub(super) fn configuration(&self) -> Value {
        let p = &self.prepared;
        json!({"media_item_id":p.plan.source.media_item_id,"media_file_id":p.plan.source.details.id,
            "audio_stream_index":p.plan.source.tracks.audio_stream_index,"subtitle_stream_index":p.plan.source.tracks.subtitle_stream_index,
            "quality_mode":p.selection.quality_mode.as_deref().unwrap_or("auto"),"max_streaming_bitrate":p.selection.max_streaming_bitrate,
            "force_hls":p.selection.force_hls.unwrap_or(false),"force_transcode":p.selection.force_transcode.unwrap_or(false),
            "delivery_mode":p.binding.mode.as_str(),"encodes":p.binding.encodes,"playback_mode":p.plan.source.playback_mode,
            "ambient_channel_id":p.binding.ambient.map(|v|v.0),"ambient_channel_updated_at":p.binding.ambient.map(|v|v.1)})
    }
    pub(super) async fn insert(&self, conn: &mut sqlx::PgConnection) -> Result<(), AdmissionError> {
        revisions::insert_output(
            conn,
            revisions::NewOutput {
                viewer: self.prepared.request.id,
                id: self.id,
                operation: self.operation,
                generation: self.generation,
                binding: self.prepared.binding,
                configuration: &self.configuration(),
            },
        )
        .await
    }
    pub(super) fn snapshot(&self, state: &AppState) -> Result<(), AdmissionError> {
        if let Some(id) = self.generation {
            if let Some(session) = state.transcode_manager.get_session(&id) {
                *self.session.lock().map_err(|_| AdmissionError::Cleanup)? = Some(session);
            }
            if let Some(failure) = state.transcode_manager.session_failure(id) {
                *self.failure.lock().map_err(|_| AdmissionError::Cleanup)? = Some(failure);
            }
        }
        Ok(())
    }
    pub(super) async fn clean(
        self: &Arc<Self>,
        state: &AppState,
        owner: &Arc<AdmissionOwner>,
    ) -> Result<(), AdmissionError> {
        self.published.store(false, Ordering::Release);
        self.group.retire();
        let observation = self.snapshot(state);
        let (readers, process) = tokio::join!(self.group.drain(), async {
            if !self.cleaned.load(Ordering::Acquire) {
                if let Some(id) = self.generation {
                    state.transcode_manager.stop_session(id).await?;
                }
                self.cleaned.store(true, Ordering::Release);
            }
            Ok::<_, playback::PlaybackError>(())
        });
        if observation.is_err() || readers.is_err() || process.is_err() {
            let output = self.clone();
            let _=owner.sql(true,move|conn|Box::pin(async move {
                crate::services::playback_admission::vod::cleanup_lock(conn,output.prepared.request.actor.user).await?;
                super::database::read(conn,output.prepared.request.id).await?.ok_or(AdmissionError::Storage)?;
                sqlx::query("UPDATE playback_vod_outputs SET cleanup_uncertain=true WHERE id=$1 AND reservation_id=$2").bind(output.id).bind(output.prepared.request.id).execute(conn).await?;
                Ok(())
            })).await;
            return Err(AdmissionError::Cleanup);
        }
        let output = self.clone();
        let failure=self.failure.lock().map_err(|_|AdmissionError::Cleanup)?.map(|class| {
            use crate::services::transcoding::FailureClass;
            json!({"kind":match class {FailureClass::Hardware=>"hardware",FailureClass::Decode=>"decode",FailureClass::Container=>"container",FailureClass::Stalled=>"stalled",FailureClass::Unknown=>"unknown"}})
        });
        owner.sql(true,move|conn|Box::pin(async move {
            crate::services::playback_admission::vod::cleanup_lock(conn,output.prepared.request.actor.user).await?;
            super::database::read(conn,output.prepared.request.id).await?.ok_or(AdmissionError::Storage)?;
            sqlx::query("UPDATE playback_vod_outputs SET state='complete',resources_completed=true,failure=COALESCE(failure,$3) WHERE id=$1 AND reservation_id=$2").bind(output.id).bind(output.prepared.request.id).bind(failure).execute(&mut *conn).await?;
            if let Some(operation)=output.operation {
                sqlx::query("UPDATE playback_vod_operations SET cleanup_at=COALESCE(cleanup_at,clock_timestamp()) WHERE id=$1 AND reservation_id=$2").bind(operation).bind(output.prepared.request.id).execute(conn).await?;
            }
            Ok(())
        })).await
    }
    pub(super) async fn identity(self: &Arc<Self>, state: &AppState) -> Result<(), AdmissionError> {
        let identity = playback::direct::FileIdentity::read_scoped(
            &state.transcode_manager.output,
            &self.group,
            &self.prepared.plan.source.details,
        )
        .await;
        if identity.as_ref() != Some(&self.prepared.identity) {
            return Err(AdmissionError::Authority);
        }
        Ok(())
    }
}
