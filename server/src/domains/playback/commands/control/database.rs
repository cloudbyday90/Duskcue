// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::{
    database as root,
    error::CommandError,
    output::Output,
    ownership::{Viewer, same_actor},
    preparation, representation,
};
use super::{
    projection,
    types::{ConfigurationResponse, OperationResponse},
};
use crate::{
    domains::household_security::types::Scope,
    services::playback_admission::{
        AdmissionError, AdmissionOwner, Request,
        vod::{self, revisions},
    },
    state::AppState,
};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

pub(super) async fn selected(
    state: &AppState,
    scope: Scope,
    id: Uuid,
) -> Result<(Arc<AdmissionOwner>, Request, Arc<Viewer>), CommandError> {
    let owner = state
        .playback_admission
        .get()
        .cloned()
        .ok_or(CommandError::Unavailable)?;
    let (row, request) = representation::load(state, &owner, scope, id, false)
        .await?
        .ok_or(CommandError::Forbidden)?;
    if row.protocol != 2
        || row.instance_id != owner.instance()
        || row.reservation_state != "held"
        || row.deadline <= row.observed_at
    {
        return Err(CommandError::Forbidden);
    }
    let viewer = state
        .vod_commands
        .get(id)
        .ok_or(CommandError::Unavailable)?;
    if !same_actor(viewer.prepared.request, request) || !viewer.group.active() {
        return Err(CommandError::Forbidden);
    }
    Ok((owner, request, viewer))
}

pub(super) async fn configuration(
    state: &AppState,
    owner: &Arc<AdmissionOwner>,
    request: Request,
    viewer: &Arc<Viewer>,
) -> Result<(ConfigurationResponse, String), CommandError> {
    let local = viewer.current()?;
    let state = state.clone();
    owner
        .active_sql(request, move |conn| {
            Box::pin(async move {
                let control = revisions::control(conn, request.id).await?;
                let output = revisions::output(
                    conn,
                    request.id,
                    control
                        .current_output
                        .ok_or(AdmissionError::Indeterminate)?,
                )
                .await?;
                projection::configuration(&state, request.id, &control, &output, &local)
                    .map_err(|_| AdmissionError::Indeterminate)
            })
        })
        .await
        .map_err(CommandError::from)
}

pub(super) async fn status(
    state: &AppState,
    scope: Scope,
    viewer: Uuid,
    id: Uuid,
    cleanup: bool,
) -> Result<OperationResponse, CommandError> {
    let owner = state
        .playback_admission
        .get()
        .ok_or(CommandError::Unavailable)?;
    let (parent, request) = representation::load(state, owner, scope, viewer, cleanup)
        .await?
        .ok_or(CommandError::Forbidden)?;
    let private = cleanup || parent.reservation_state != "held";
    let idle = state.runtime_config.load().auth.session_idle_timeout_hours;
    if private {
        owner
            .sql(cleanup, move |conn| {
                Box::pin(read_status(conn, scope, viewer, id, private, cleanup, idle))
            })
            .await
            .map_err(CommandError::from)
    } else {
        owner
            .active_sql(request, move |conn| {
                Box::pin(read_status(conn, scope, viewer, id, private, cleanup, idle))
            })
            .await
            .map_err(CommandError::from)
    }
}

async fn read_status(
    conn: &mut sqlx::PgConnection,
    scope: Scope,
    viewer: Uuid,
    id: Uuid,
    private: bool,
    cleanup: bool,
    idle: Option<i32>,
) -> Result<OperationResponse, AdmissionError> {
    let revision = if cleanup {
        root::cleanup_context(conn, scope, idle).await?;
        0
    } else {
        root::context(conn, scope, idle).await?
    };
    let parent = root::read_scoped(conn, viewer, Some(scope.user))
        .await?
        .ok_or(AdmissionError::Authority)?;
    root::actor(viewer, &parent, scope, revision, cleanup)?;
    match revisions::operation(conn, id).await? {
        Some(row) if row.reservation_id == viewer => {
            Ok(projection::operation(viewer, &row, private))
        }
        Some(_) => Err(AdmissionError::Conflict),
        None => {
            revisions::missing(conn, id).await?;
            Ok(OperationResponse {
                protocol: super::super::types::PROTOCOL,
                request_id: viewer,
                operation_id: id,
                status: "unknown".into(),
                status_url: format!("/api/v1/playback-commands/{viewer}/operations/{id}"),
                output_revision: None,
                result: None,
                cleanup_complete: false,
            })
        }
    }
}

pub(super) struct Acceptance {
    pub operation: Uuid,
    pub expected: Uuid,
    pub hash: [u8; 32],
    pub condition: String,
    pub action: &'static str,
    pub checkpoint: Option<(i64, i32)>,
    pub budget: Option<Value>,
    pub evidence: Value,
}

pub(super) async fn accept(
    state: &AppState,
    owner: &Arc<AdmissionOwner>,
    request: Request,
    viewer: &Arc<Viewer>,
    target: &Arc<Output>,
    input: Acceptance,
) -> Result<(OperationResponse, bool), CommandError> {
    let local = viewer.current()?;
    let target = target.clone();
    let viewer = viewer.clone();
    let state = state.clone();
    owner.active_sql(request,move|conn|Box::pin(async move {
        let control=revisions::control(conn,request.id).await?;
        if let Some(row)=revisions::operation(conn,input.operation).await? {
            if row.reservation_id!=request.id || row.intent_hash!=input.hash || row.condition!=input.condition {return Err(AdmissionError::Conflict);}
            return Ok((projection::operation(request.id,&row,false),false));
        }
        revisions::missing(conn,input.operation).await?;
        if control.pending_operation.is_some() {return Err(AdmissionError::Pending);}
        let current=revisions::output(conn,request.id,control.current_output.ok_or(AdmissionError::Indeterminate)?).await?;
        let (_,etag)=projection::configuration(&state,request.id,&control,&current,&local).map_err(|_|AdmissionError::Indeterminate)?;
        if input.condition!=etag {return Err(AdmissionError::Precondition);}
        if control.current_output!=Some(input.expected) || !viewer.group.active() || !matches!(control.state.as_str(),"ready"|"unavailable") {return Err(AdmissionError::Conflict);}
        if let Some((sequence,position))=input.checkpoint && (control.checkpoint_sequence!=sequence || control.checkpoint_output!=Some(input.expected) || control.checkpoint_position!=Some(position)) {return Err(AdmissionError::Conflict);}
        let policy=vod::policy(conn,request.actor.user).await?;
        if input.action=="reconfigure" {preparation::validate(&policy,&target.prepared.plan,target.prepared.binding)?;}else if !policy.permits(target.prepared.binding) {return Err(AdmissionError::Policy);}
        let details=crate::domains::playback::media_source::fetch_media_file_details_on_connection(conn,target.prepared.plan.source.details.id,target.prepared.plan.source.media_item_id).await.map_err(|_|AdmissionError::Authority)?;
        if !target.prepared.identity.matches_details(&details) {return Err(AdmissionError::Authority);}
        sqlx::query("INSERT INTO playback_vod_operations(id,reservation_id,household_id,intent_hash,condition,action,expected_output,candidate_output,evidence) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(input.operation).bind(request.id).bind(request.actor.user).bind(input.hash.as_slice()).bind(&input.condition).bind(input.action).bind(input.expected).bind(target.id).bind(input.evidence).execute(&mut *conn).await?;
        target.insert(conn).await?;
        sqlx::query("UPDATE playback_vod_controls SET state='changing',pending_operation=$2,claim_encodes=claim_encodes OR $3,recovery_budget=COALESCE($4,recovery_budget) WHERE reservation_id=$1")
            .bind(request.id).bind(input.operation).bind(target.prepared.binding.encodes).bind(input.budget).execute(&mut *conn).await?;
        let row=revisions::operation(conn,input.operation).await?.ok_or(AdmissionError::Storage)?;
        Ok((projection::operation(request.id,&row,false),true))
    })).await.map_err(|error|match error {AdmissionError::Pending=>CommandError::Busy,_=>CommandError::from(error)})
}

pub(super) async fn replay(
    state: &AppState,
    scope: Scope,
    viewer: Uuid,
    intent: &super::intent::Intent,
) -> Result<Option<OperationResponse>, CommandError> {
    let owner = state
        .playback_admission
        .get()
        .ok_or(CommandError::Unavailable)?;
    representation::load(state, owner, scope, viewer, false)
        .await?
        .ok_or(CommandError::Forbidden)?;
    let id = intent.body.operation_id;
    let hash = intent.hash;
    let condition = intent.condition.clone();
    let idle = state.runtime_config.load().auth.session_idle_timeout_hours;
    owner
        .sql(false, move |conn| {
            Box::pin(async move {
                root::context(conn, scope, idle).await?;
                if let Some(row) = revisions::operation(conn, id).await? {
                    if row.reservation_id != viewer
                        || row.intent_hash != hash
                        || row.condition != condition
                    {
                        return Err(AdmissionError::Conflict);
                    }
                    return Ok(Some(projection::operation(viewer, &row, true)));
                }
                revisions::missing(conn, id).await?;
                Ok(None)
            })
        })
        .await
        .map_err(CommandError::from)
}
