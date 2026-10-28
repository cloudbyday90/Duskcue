// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database, output::Output, ownership::Viewer, preparation, types::ReadyResponse};
use crate::{
    domains::playback,
    services::playback_admission::{
        AdmissionError, AdmissionOwner,
        vod::{self, revisions},
    },
    state::AppState,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

impl Output {
    pub(super) async fn prepare_output(
        self: &Arc<Self>,
        state: &AppState,
        owner: &Arc<AdmissionOwner>,
        viewer: &Arc<Viewer>,
        cancel: &CancellationToken,
    ) -> Result<(), AdmissionError> {
        if cancel.is_cancelled() || !viewer.group.active() {
            return Err(AdmissionError::Retired);
        }
        let (pending, offset) =
            if let (Some(id), Some(parameters)) = (self.generation, self.parameters.clone()) {
                let session = state
                    .transcode_manager
                    .start_session_bound(
                        parameters,
                        &state.bootstrap.data_dir,
                        id,
                        Some(cancel.clone()),
                    )
                    .await
                    .map_err(|_| AdmissionError::Authority)?;
                if session.id != id {
                    return Err(AdmissionError::Conflict);
                }
                let offset = session.source_start_ms;
                *self.session.lock().map_err(|_| AdmissionError::Cleanup)? = Some(session);
                (
                    Some(state.transcode_manager.pending_session(id)),
                    Some(offset),
                )
            } else {
                (None, None)
            };
        self.identity(state).await?;
        if cancel.is_cancelled() || !viewer.group.active() {
            return Err(AdmissionError::Retired);
        }
        let output = self.clone();
        let local = viewer.clone();
        owner.active_sql(viewer.prepared.request,move|conn|Box::pin(async move {
            let request=local.prepared.request;
            let control=revisions::control(conn,request.id).await?;
            let original=database::read(conn,request.id).await?.ok_or(AdmissionError::Storage)?;
            let source=&output.prepared.plan.source;
            let details=playback::media_source::fetch_media_file_details_on_connection(conn,source.details.id,source.media_item_id).await.map_err(|_|AdmissionError::Authority)?;
            if !output.prepared.identity.matches_details(&details) || !local.group.active() {return Err(AdmissionError::Retired);}
            if let Some(id)=output.operation {
                let operation=revisions::operation(conn,id).await?.ok_or(AdmissionError::Storage)?;
                if control.pending_operation!=Some(id) || operation.candidate_output!=output.id || operation.state!="preparing" {return Err(AdmissionError::Retired);}
                if operation.action=="reconfigure" {
                    preparation::validate(&vod::policy(conn,request.actor.user).await?,&output.prepared.plan,output.prepared.binding)?;
                }
            } else if control.state!="preparing" || original.history_id.is_some() {return Err(AdmissionError::Retired);}
            let (history,started)=match (original.history_id,original.history_started_at) {
                (Some(id),Some(started))=>(id,started),
                (None,None) if output.operation.is_none()=>{
                    let id=playback::service::create_play_session(&mut *conn,request.actor.user,request.actor.profile,source.media_item_id,source.library_id,output.prepared.binding.mode.as_str(),output.generation,Some(source.details.id),None,None,&source.playback_mode,source.ambient_channel.as_ref(),None,None).await.map_err(|_|AdmissionError::Storage)?;
                    let started=sqlx::query_scalar("SELECT transaction_timestamp()").fetch_one(&mut *conn).await?;
                    (id,started)
                },
                _=>return Err(AdmissionError::Storage),
            };
            let result=ReadyResponse {output_revision:Some(output.id),position_ms:Some(output.position),recovery:output.recovery.clone(),session_id:history,media_item_id:source.media_item_id,media_file_id:source.details.id,delivery_mode:output.prepared.binding.mode.as_str().into(),audio_stream_index:source.tracks.audio_stream_index,subtitle_stream_index:source.tracks.subtitle_stream_index,playback_mode:source.playback_mode.clone(),stream_offset_ms:offset};
            let result=serde_json::to_value(result).map_err(|_|AdmissionError::Storage)?;
            if sqlx::query("UPDATE play_sessions SET stream_decision=$4,metadata=metadata||jsonb_build_object('vod_command_protocol',2,'transcode_session_id',$5::uuid,'stream_offset_ms',$6::bigint,'output_revision',$7::uuid) WHERE id=$1 AND started_at=$2 AND user_id=$3 AND stopped_at IS NULL")
                .bind(history).bind(started).bind(request.actor.user).bind(output.prepared.binding.mode.as_str()).bind(output.generation).bind(offset).bind(output.id).execute(&mut *conn).await?.rows_affected()!=1 {return Err(AdmissionError::Storage);}
            if sqlx::query("UPDATE playback_vod_outputs SET state='ready',result=$3 WHERE id=$1 AND reservation_id=$2 AND state='preparing' AND NOT resources_completed")
                .bind(output.id).bind(request.id).bind(&result).execute(&mut *conn).await?.rows_affected()!=1 {return Err(AdmissionError::Retired);}
            if let Some(id)=output.operation {
                sqlx::query("UPDATE playback_vod_operations SET state='completed',result=$2 WHERE id=$1 AND state='preparing'").bind(id).bind(&result).execute(&mut *conn).await?;
            } else {
                sqlx::query("UPDATE playback_vod_commands SET state='ready',history_id=$2,history_started_at=$3,generation_id=$4,result=$5 WHERE reservation_id=$1 AND state='preparing'")
                    .bind(request.id).bind(history).bind(started).bind(output.generation).bind(&result).execute(&mut *conn).await?;
            }
            sqlx::query("UPDATE playback_vod_controls SET state='ready',current_output=$2,pending_operation=NULL,claim_encodes=$3,checkpoint_output=NULL,checkpoint_sequence=0,checkpoint_position=NULL WHERE reservation_id=$1")
                .bind(request.id).bind(output.id).bind(output.prepared.binding.encodes).execute(conn).await?;
            Ok(())
        })).await?;
        viewer.promote(self)?;
        if let Some(pending) = pending {
            pending.acknowledge();
        }
        Ok(())
    }
}
