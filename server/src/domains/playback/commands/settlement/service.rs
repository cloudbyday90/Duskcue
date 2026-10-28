// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database, types::SettlementResponse};
use crate::{
    domains::{
        household_security::types::Scope,
        playback::commands::{error::CommandError, types::PROTOCOL},
    },
    services::playback_admission::AdmissionError,
    state::AppState,
};
use uuid::Uuid;

pub(in crate::domains::playback::commands) async fn read(
    state: &AppState,
    scope: Scope,
    id: Uuid,
) -> Result<SettlementResponse, CommandError> {
    inspect(state, scope, id, false).await
}

pub(in crate::domains::playback::commands) async fn withdraw(
    state: &AppState,
    scope: Scope,
    id: Uuid,
) -> Result<SettlementResponse, CommandError> {
    let response = inspect(state, scope, id, true).await?;
    state.vod_commands.cancel_preparation(id)?;
    if response.parent_released == Some(false) {
        super::super::service::stop(state, scope, id).await?;
    }
    read(state, scope, id).await
}

async fn inspect(
    state: &AppState,
    scope: Scope,
    id: Uuid,
    withdraw: bool,
) -> Result<SettlementResponse, CommandError> {
    if id.get_version_num() != 7 {
        return Err(CommandError::Invalid);
    }
    let owner = state
        .playback_admission
        .get()
        .ok_or(CommandError::Unavailable)?;
    let instance = owner.instance();
    let idle = state.runtime_config.load().auth.session_idle_timeout_hours;
    Ok(owner.sql(true,move |conn|Box::pin(async move {
        sqlx::query("SELECT id FROM playback_instances WHERE id=$1 FOR SHARE").bind(instance).execute(&mut *conn).await?;
        super::super::database::cleanup_context(conn,scope,idle).await?;
        database::compatible(conn).await?;
        let saved=database::row(conn,id).await?;
        if saved.is_none() && sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM playback_reservations WHERE id=$1)").bind(id).fetch_one(&mut *conn).await? {
            return Err(AdmissionError::Conflict);
        }
        if saved.is_none() && withdraw {
            let expired: bool=sqlx::query_scalar("UPDATE playback_retention SET cutoff=greatest(cutoff,date_trunc('milliseconds',clock_timestamp())-interval '5 minutes') WHERE id RETURNING uuid_extract_timestamp($1)<cutoff")
                .bind(id).fetch_one(&mut *conn).await?;
            if !expired {
                sqlx::query("INSERT INTO playback_start_intents(id,household_id,session_id,credential_version,transport,instance_id,admission_fenced,preparation_complete) VALUES ($1,$2,$3,$4,$5,$6,true,true)")
                    .bind(id).bind(scope.user).bind(scope.session).bind(scope.version).bind(database::transport(scope)?).bind(instance).execute(&mut *conn).await?;
            }
        }
        if let Some(saved)=saved.as_ref() {database::actor(saved,scope)?;}
        if withdraw {
            sqlx::query("UPDATE playback_start_intents SET admission_fenced=true WHERE id=$1 AND household_id=$2").bind(id).bind(scope.user).execute(&mut *conn).await?;
            sqlx::query("UPDATE playback_reservations SET state='retiring' WHERE id=$1 AND household_id=$2 AND state='held'").bind(id).bind(scope.user).execute(&mut *conn).await?;
        }
        let saved=database::row(conn,id).await?;
        let now=sqlx::query_scalar("SELECT clock_timestamp()").fetch_one(&mut *conn).await?;
        let mut response=SettlementResponse {
            protocol:PROTOCOL,request_id:id,status:"pending",status_url:format!("/api/v1/playback-commands/{id}/settlement"),observed_at:now,
            admission_fenced:false,preparation_complete:false,parent_released:None,
        };
        if let Some(saved)=saved {
            database::actor(&saved,scope)?;
            response.observed_at=saved.observed_at;
            response.admission_fenced=saved.admission_fenced;
            response.preparation_complete=saved.preparation_complete;
            response.parent_released=sqlx::query_scalar("SELECT state='released' FROM playback_reservations WHERE id=$1 AND household_id=$2")
                .bind(id).bind(scope.user).fetch_optional(&mut *conn).await?;
            response.status=if saved.settled_at.is_some() && response.parent_released!=Some(false) {"settled"} else if saved.instance_id!=instance && (!saved.preparation_complete || saved.promoted) {"uncertain"} else {"pending"};
        } else {
            let permanent:bool=sqlx::query_scalar("SELECT uuid_extract_timestamp($1)<cutoff FROM playback_retention WHERE id").bind(id).fetch_one(&mut *conn).await?;
            if permanent {
                response.admission_fenced=true;
                response.preparation_complete=true;
                response.status="settled";
            }
        }
        Ok::<_,AdmissionError>(response)
    })).await?)
}
