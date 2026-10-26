// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    authority,
    cursor::{self, Context, Position},
    database::Lease,
    error::CatalogError,
    limits::Budget,
    query,
    types::*,
};
use crate::{
    domains::{household_security::types::Scope, live_tv::feed::imports::reader},
    state::AppState,
};
use sqlx::PgConnection;
use std::{future::Future, pin::Pin};
type ReadFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<u8>, CatalogError>> + Send + 'a>>;
use tokio::time::timeout_at;
use uuid::Uuid;

async fn read<F>(state: &AppState, actor: Scope, work: F) -> Result<Vec<u8>, CatalogError>
where
    F: for<'a> FnOnce(
            &'a mut PgConnection,
            &'a authority::Viewing,
            &'a mut Budget,
        ) -> ReadFuture<'a>
        + Send,
{
    let _permit = state.live_tv_catalog.admit()?;
    let mut budget = Budget::new();
    let mut lease = Lease::acquire(&state.pool, budget.deadline).await?;
    let idle = state.runtime_config.load().auth.session_idle_timeout_hours;
    let outcome = timeout_at(budget.deadline, async {
        lease.begin().await?;
        let view =
            authority::viewing(&mut lease, actor, &state.encryption_key, idle, &mut budget).await?;
        let bytes = work(&mut lease, &view, &mut budget).await?;
        authority::session(&mut lease, actor, Some(view.profile_revision), idle).await?;
        budget.check()?;
        Ok(bytes)
    })
    .await
    .unwrap_or(Err(CatalogError::Unavailable));
    lease.release().await?;
    outcome
}

pub(crate) async fn channels(
    state: &AppState,
    actor: Scope,
    request: ChannelsRequest,
) -> Result<Vec<u8>, CatalogError> {
    let limit = query::limit(request.limit)?;
    query::cursor(request.cursor.as_deref())?;
    let key = state.encryption_key.clone();
    read(state, actor, move |conn, view, budget| {
        Box::pin(async move {
            let snapshot = reader::snapshot(conn, &key, view.fence, budget).await?;
            let context = Context::new(view, snapshot.id(), limit, None, None);
            let position = cursor::decode(&key, view, &context, request.cursor.as_deref(), budget)?;
            let after = match position.as_ref() {
                None => &[][..],
                Some(Position::Channels { key }) => key.as_slice(),
                _ => return Err(CatalogError::Changed),
            };
            let (entries, has_more) =
                reader::channels(conn, &key, &snapshot, after, limit, budget).await?;
            let cursor = if has_more {
                let last = entries.last().ok_or(CatalogError::Unavailable)?;
                Some(cursor::encode(
                    &key,
                    view,
                    context,
                    Position::Channels {
                        key: last.key.clone(),
                    },
                    budget,
                )?)
            } else {
                None
            };
            let response = ChannelsResponse {
                context: snapshot.context(actor.profile, view.profile_revision),
                items: entries.into_iter().map(|entry| entry.value).collect(),
                cursor,
                has_more,
            };
            budget.serialize(&response)
        })
    })
    .await
}

pub(crate) struct ProgrammeRead {
    pub channel: Uuid,
    pub snapshot: Uuid,
    pub window: Option<Window>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

pub(crate) async fn programmes(
    state: &AppState,
    actor: Scope,
    request: ProgrammeRead,
) -> Result<Vec<u8>, CatalogError> {
    query::id(request.channel)?;
    query::id(request.snapshot)?;
    let limit = query::limit(request.limit)?;
    query::cursor(request.cursor.as_deref())?;
    let key = state.encryption_key.clone();
    read(state, actor, move |conn, view, budget| {
        Box::pin(async move {
            let snapshot = reader::snapshot(conn, &key, view.fence, budget).await?;
            if snapshot.id() != request.snapshot {
                return Err(CatalogError::Changed);
            }
            let context = Context::new(
                view,
                snapshot.id(),
                limit,
                Some(request.channel),
                request.window,
            );
            let position = cursor::decode(&key, view, &context, request.cursor.as_deref(), budget)?;
            let channel = reader::channel(conn, &key, &snapshot, request.channel, budget).await?;
            let request_query = reader::ProgrammeQuery {
                guide: channel.guide,
                window: request.window,
                after: position.as_ref(),
                limit,
            };
            let (entries, has_more) =
                reader::programmes(conn, &key, &snapshot, request_query, budget).await?;
            let cursor = if has_more {
                let last = entries.last().ok_or(CatalogError::Unavailable)?;
                Some(cursor::encode(
                    &key,
                    view,
                    context,
                    last.position.clone(),
                    budget,
                )?)
            } else {
                None
            };
            let response = ProgrammesResponse {
                context: snapshot.context(actor.profile, view.profile_revision),
                channel_id: request.channel,
                guide: channel.value.guide,
                view: if request.window.is_some() {
                    "window"
                } else {
                    "unpositionable"
                },
                window: request.window,
                items: entries.into_iter().map(|entry| entry.value).collect(),
                cursor,
                has_more,
            };
            budget.serialize(&response)
        })
    })
    .await
}
