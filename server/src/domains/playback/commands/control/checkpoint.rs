// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::{error::CommandError, types::PROTOCOL};
use super::{
    database,
    types::{CheckpointRequest, CheckpointResponse},
};
use crate::{
    domains::{household_security::types::Scope, playback},
    services::playback_admission::{
        AdmissionError,
        vod::revisions::{self, ControlRow},
    },
    state::AppState,
};
use uuid::Uuid;

fn response(id: Uuid, row: ControlRow) -> CheckpointResponse {
    CheckpointResponse {
        protocol: PROTOCOL,
        request_id: id,
        output_revision: row.checkpoint_output,
        sequence: row.checkpoint_sequence,
        position_ms: row.checkpoint_position,
        playback_state: row.playback_state,
    }
}

pub(super) async fn read(
    state: &AppState,
    scope: Scope,
    id: Uuid,
) -> Result<CheckpointResponse, CommandError> {
    let (owner, request, _) = database::selected(state, scope, id).await?;
    owner
        .active_sql(request, move |conn| {
            Box::pin(async move { Ok(response(id, revisions::control(conn, id).await?)) })
        })
        .await
        .map_err(CommandError::from)
}

pub(super) async fn report(
    state: &AppState,
    scope: Scope,
    id: Uuid,
    body: CheckpointRequest,
) -> Result<CheckpointResponse, CommandError> {
    if scope.binding.is_none() {
        return Err(CommandError::Conflict);
    }
    if body.protocol != PROTOCOL
        || body.output_revision.get_version_num() != 7
        || !(1..=9007199254740991).contains(&body.sequence)
        || body.position_ms < 0
        || !matches!(
            body.playback_state.as_str(),
            "playing" | "paused" | "buffering"
        )
    {
        return Err(CommandError::Invalid);
    }
    let (owner, request, viewer) = database::selected(state, scope, id).await?;
    let current = viewer.current()?;
    let duration = playback::source_candidates::duration_ms(&current.prepared.plan.source)
        .ok_or(CommandError::Conflict)?;
    if i64::from(body.position_ms) > duration {
        return Err(CommandError::Invalid);
    }
    let source = current.prepared.plan.source.details.clone();
    let state = state.clone();
    let group = viewer.group.clone();
    group.inspect(async move {
        let result=async {
            let identity=playback::direct::FileIdentity::read_scoped(&state.transcode_manager.output,&viewer.group,&source).await;
            if identity.as_ref()!=Some(&current.prepared.identity) {return Err(CommandError::Forbidden);}
            owner.active_sql(request,move|conn|Box::pin(async move {
                let row=revisions::control(conn,id).await?;
                if row.current_output!=Some(body.output_revision) || row.pending_operation.is_some() || !matches!(row.state.as_str(),"ready"|"unavailable") || !viewer.group.active() {return Err(AdmissionError::Conflict);}
                let seq=i64::try_from(body.sequence).map_err(|_|AdmissionError::Conflict)?;
                if seq==row.checkpoint_sequence && row.checkpoint_output==Some(body.output_revision) && row.checkpoint_position==Some(body.position_ms) && row.playback_state==body.playback_state {return Ok(response(id,row));}
                if seq<=row.checkpoint_sequence {return Err(AdmissionError::Conflict);}
                let parent=super::super::database::read(conn,id).await?.ok_or(AdmissionError::Storage)?;
                let (history,started)=parent.history_id.zip(parent.history_started_at).ok_or(AdmissionError::Retired)?;
                let published:bool=sqlx::query_scalar("SELECT published_at IS NOT NULL FROM playback_vod_outputs WHERE id=$1 AND reservation_id=$2").bind(body.output_revision).bind(id).fetch_one(&mut *conn).await?;
                if !published {return Err(AdmissionError::Conflict);}
                if sqlx::query("UPDATE play_sessions SET metadata=metadata||jsonb_build_object('current_position_ms',$4::integer,'current_state',$5::text,'last_heartbeat_at',clock_timestamp(),'checkpoint_output',$6::uuid,'checkpoint_sequence',$7::bigint) WHERE id=$1 AND started_at=$2 AND user_id=$3 AND stopped_at IS NULL")
                    .bind(history).bind(started).bind(request.actor.user).bind(body.position_ms).bind(&body.playback_state).bind(body.output_revision).bind(seq).execute(&mut *conn).await?.rows_affected()!=1 {return Err(AdmissionError::Retired);}
                if current.prepared.plan.source.playback_mode=="interactive" {
                    playback::history::upsert_user_item_data_heartbeat(&mut *conn,request.actor.user,request.actor.profile,current.prepared.plan.source.media_item_id,body.position_ms,Some(current.prepared.plan.source.details.id)).await.map_err(|_|AdmissionError::Storage)?;
                }
                if row.playback_state!=body.playback_state {
                    let event=match (row.playback_state.as_str(),body.playback_state.as_str()) {(_,"paused")=>"pause",("paused","playing")=>"resume",(_,"buffering")=>"buffer_start",("buffering","playing")=>"buffer_end",_=>"heartbeat"};
                    playback::history::emit_play_event(&mut *conn,history,request.actor.user,event,Some(body.position_ms/1000),serde_json::json!({"state":body.playback_state,"output_revision":body.output_revision})).await.map_err(|_|AdmissionError::Storage)?;
                }
                playback::history::emit_play_event(&mut *conn,history,request.actor.user,"heartbeat",Some(body.position_ms/1000),serde_json::json!({"state":body.playback_state,"output_revision":body.output_revision})).await.map_err(|_|AdmissionError::Storage)?;
                sqlx::query("UPDATE playback_vod_controls SET checkpoint_output=$2,checkpoint_sequence=$3,checkpoint_position=$4,playback_state=$5 WHERE reservation_id=$1")
                    .bind(id).bind(body.output_revision).bind(seq).bind(body.position_ms).bind(body.playback_state).execute(&mut *conn).await?;
                Ok(response(id,revisions::control(conn,id).await?))
            })).await.map_err(CommandError::from)
        }.await;
        Ok(result)
    }).await.map_err(|_|CommandError::Unavailable)?
}
