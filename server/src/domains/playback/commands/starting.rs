// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    preparation::{self, PreparedCommand},
    settlement,
    types::Intent,
};
use crate::{
    domains::{household_security::types::Scope, playback::inspection},
    services::playback_admission::{AdmissionError, AdmissionOwner},
    state::AppState,
};
use std::{sync::Arc, time::Duration};
use tokio::time::Instant;

pub(super) async fn prepare(
    state: &AppState,
    owner: &Arc<AdmissionOwner>,
    scope: Scope,
    intent: Intent,
    deadline: Instant,
) -> Result<PreparedCommand, AdmissionError> {
    let id = intent.id;
    let stop = match state.vod_commands.prepare(id) {
        Ok(stop) => stop,
        Err(error) => {
            settlement::complete(owner, scope, id, true).await?;
            return Err(error);
        }
    };
    let output = state.transcode_manager.output.clone();
    let shutdown = output.shutdown_token();
    let pending = settlement::pending(state, owner, scope, id, None).await;
    if !matches!(&pending, Ok(Some(saved)) if saved.status == "preparing") {
        if let Err(error) = settlement::complete(owner, scope, id, true).await {
            output.fail();
            return Err(error);
        }
        state.vod_commands.finish_preparation(id)?;
        return Err(pending.err().unwrap_or(AdmissionError::Retired));
    }
    let work = inspection::scope(
        output.clone(),
        stop.clone(),
        preparation::prepare(state, owner, scope, intent),
    );
    tokio::pin!(work);
    let mut fenced = false;
    let result = tokio::select! {biased;
        _=shutdown.cancelled()=>{fenced=true;None},
        _=stop.cancelled()=>{fenced=true;None},
        _=tokio::time::sleep_until(deadline)=>{fenced=true;None},
        result=&mut work=>Some(result),
    };
    if fenced {
        stop.cancel();
        if settlement::withdraw(state, scope, id).await.is_err() {
            output.fail();
        }
    }
    let result = match result {
        Some(result) => result,
        None => work.await,
    };
    fenced |= Instant::now() >= deadline
        || stop.is_cancelled()
        || shutdown.is_cancelled()
        || result.is_err();
    if let Err(error) = settlement::complete(owner, scope, id, fenced).await {
        output.fail();
        return Err(error);
    }
    state.vod_commands.finish_preparation(id)?;
    if fenced {
        return result.and(Err(AdmissionError::Retired));
    }
    result
}

pub(super) fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(60)
}
