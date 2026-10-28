// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::{
    error::CommandError,
    output::Output,
    ownership::{Viewer, same_actor},
    preparation,
};
use super::{
    Work, database, intent, recovery,
    types::{Action, ChangeRequest, OperationResponse},
};
use crate::{
    domains::{household_security::types::Scope, playback},
    services::playback_admission::{AdmissionError, AdmissionOwner, vod::revisions},
    state::AppState,
};
use serde_json::json;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub(super) async fn change(
    state: &AppState,
    scope: Scope,
    id: Uuid,
    body: ChangeRequest,
    condition: String,
) -> Result<OperationResponse, CommandError> {
    if scope.binding.is_none() {
        return Err(CommandError::Conflict);
    }
    let intent = intent::normalize(id, body, condition)?;
    let state = state.clone();
    let output = state.transcode_manager.output.clone();
    output
        .command(false, move |_| async move {
            Ok(change_owned(&state, scope, id, intent).await)
        })
        .await
        .map_err(|_| CommandError::Unavailable)?
}

async fn change_owned(
    state: &AppState,
    scope: Scope,
    id: Uuid,
    mut intent: intent::Intent,
) -> Result<OperationResponse, CommandError> {
    if let Some(replay) = database::replay(state, scope, id, &intent).await? {
        return Ok(replay);
    }
    let (owner, request, viewer) = database::selected(state, scope, id).await?;
    let guard = viewer.begin().map_err(|error| {
        if error == AdmissionError::Pending {
            CommandError::Busy
        } else {
            CommandError::from(error)
        }
    })?;
    let state = state.clone();
    let group = viewer.group.clone();
    group.inspect(async move {
        let result=async {
            let (configuration,etag)=database::configuration(&state,&owner,request,&viewer).await?;
            if intent.condition!=etag {return Err(CommandError::ConditionFailed);}
            if !matches!(configuration.state.as_str(),"ready"|"unavailable") {return Err(CommandError::Busy);}
            let previous=viewer.current()?;
            if previous.id!=intent.body.expected_output {return Err(CommandError::Conflict);}
            let identity=playback::direct::FileIdentity::read_scoped(&state.transcode_manager.output,&viewer.group,&previous.prepared.plan.source.details).await;
            if identity.as_ref()!=Some(&previous.prepared.identity) {return Err(CommandError::Forbidden);}
            previous.snapshot(&state)?;
            let control=owner.active_sql(request,move|conn|Box::pin(async move {revisions::control(conn,id).await})).await?;
            let mut changed_budget=None;
            let mut checkpoint=None;
            let mut note=None;
            let mut evidence=json!({});
            let (prepared,position,parameters)=match &intent.body.action {
                Action::Seek {position_ms}=>{
                    let params=previous.session.lock().map_err(|_|CommandError::Unavailable)?.clone().map(|session|session.restart_params(i64::from(*position_ms)));
                    (previous.prepared.clone(),*position_ms,params)
                },
                Action::Reconfigure {position_ms,..}=>{
                    let selection=intent.selection.take().ok_or(CommandError::Invalid)?;
                    let original=&viewer.prepared;
                    if selection.selection.media_item_id!=Some(original.plan.source.media_item_id) || selection.selection.media_file_id!=Some(original.plan.source.details.id) {return Err(CommandError::Conflict);}
                    let prepared=preparation::prepare(&state,&owner,scope,selection).await?;
                    if !same_actor(prepared.request,request) || prepared.identity!=original.identity || prepared.binding.ambient!=original.binding.ambient || prepared.plan.source.playback_mode!=original.plan.source.playback_mode {return Err(CommandError::Conflict);}
                    (Arc::new(prepared),*position_ms,None)
                },
                Action::Recover {checkpoint_sequence,evidence:hint}=>{
                    let sequence=i64::try_from(*checkpoint_sequence).map_err(|_|CommandError::Invalid)?;
                    if control.checkpoint_sequence!=sequence || control.checkpoint_output!=Some(previous.id) {return Err(CommandError::Conflict);}
                    let confirmed=control.checkpoint_position.ok_or(CommandError::Conflict)?;
                    let recovery=recovery::prepare(&state,&previous,&control.recovery_budget,hint,confirmed)?;
                    checkpoint=Some((sequence,confirmed));
                    changed_budget=Some(recovery.budget);
                    evidence=json!({"checkpoint_sequence":sequence,"position_ms":confirmed,"action":recovery.result});
                    note=Some(recovery.result);
                    (recovery.prepared,recovery.position,Some(recovery.parameters))
                },
            };
            let duration=playback::source_candidates::duration_ms(&prepared.plan.source).ok_or(CommandError::Conflict)?;
            if position<0 || i64::from(position)>duration {return Err(CommandError::Invalid);}
            let target=Output::new(prepared,Some(intent.body.operation_id),position,parameters,note)?;
            if position>0 && target.parameters.as_ref().is_some_and(|p| p.stream_plan.video==crate::services::transcoding::plan::StreamAction::Copy && p.output_profile!=Some(crate::services::transcoding::OutputProfile::SourceCopySdr)) {return Err(CommandError::Conflict);}
            let operation=intent.body.operation_id;
            let result=database::accept(&state,&owner,request,&viewer,&target,database::Acceptance {operation,expected:previous.id,hash:intent.hash,condition:intent.condition,action:intent::name(&intent.body.action),checkpoint,budget:changed_budget,evidence}).await;
            match result {
                Ok((response,false))=>Ok(response),
                Ok((response,true))=>{
                    if let Err(error)=viewer.queue(Work {operation,previous,target:target.clone()},target,guard) {
                        let _=owner.stop(id);
                        return Err(CommandError::from(error));
                    }
                    Ok(response)
                },
                Err(error)=>{if matches!(error,CommandError::Unavailable) {let _=owner.stop(id);}Err(error)},
            }
        }.await;
        Ok(result)
    }).await.map_err(|_|CommandError::Unavailable)?
}

pub(in crate::domains::playback::commands) async fn execute(
    state: &AppState,
    owner: &Arc<AdmissionOwner>,
    viewer: &Arc<Viewer>,
    work: Work,
    cancel: &CancellationToken,
) -> Result<(), AdmissionError> {
    let request = viewer.prepared.request;
    let id = work.operation;
    let previous = work.previous.clone();
    let target = work.target.clone();
    previous
        .published
        .store(false, std::sync::atomic::Ordering::Release);
    previous.group.retire();
    let old_id = previous.id;
    owner.active_sql(request,move|conn|Box::pin(async move {
        let control=revisions::control(conn,request.id).await?;
        if control.pending_operation!=Some(id) {return Err(AdmissionError::Retired);}
        sqlx::query("UPDATE playback_vod_operations SET state='retiring' WHERE id=$1 AND state='accepted'").bind(id).execute(&mut *conn).await?;
        sqlx::query("UPDATE playback_vod_outputs SET state='retiring' WHERE id=$1 AND NOT resources_completed").bind(old_id).execute(conn).await?;
        Ok(())
    })).await?;
    previous.clean(state, owner).await?;
    if cancel.is_cancelled() || !viewer.group.active() {
        return Err(AdmissionError::Retired);
    }
    owner.active_sql(request,move|conn|Box::pin(async move {
        sqlx::query("UPDATE playback_vod_operations SET state='preparing' WHERE id=$1 AND state='retiring'").bind(id).execute(conn).await?;
        Ok(())
    })).await?;
    match target.prepare_output(state, owner, viewer, cancel).await {
        Ok(()) => Ok(()),
        Err(AdmissionError::Indeterminate) => Err(AdmissionError::Indeterminate),
        Err(_) => {
            target.clean(state, owner).await?;
            owner.active_sql(request,move|conn|Box::pin(async move {
                let control=revisions::control(conn,request.id).await?;
                if control.pending_operation!=Some(id) {return Err(AdmissionError::Retired);}
                sqlx::query("UPDATE playback_vod_operations SET state='failed',result=$2 WHERE id=$1 AND state='preparing'").bind(id).bind(json!({"reason":"replacement_failed","recovery":target.recovery})).execute(&mut *conn).await?;
                let prior=revisions::output(conn,request.id,previous.id).await?;
                sqlx::query("UPDATE playback_vod_controls SET state='unavailable',pending_operation=NULL,claim_encodes=$2 WHERE reservation_id=$1").bind(request.id).bind(prior.encodes).execute(conn).await?;
                Ok(())
            })).await
        }
    }
}
