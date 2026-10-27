// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{AdmissionError, Request, Subject, types::Row};
use crate::{
    domains::{
        auth,
        live_tv::{
            catalog::{authority, limits::Budget},
            feed::imports::reader,
        },
        profiles,
    },
    services::encryption::EncryptionKey,
};
use sqlx::{PgConnection, Row as _};
use uuid::Uuid;

pub(super) struct Verified {
    pub assignment: Option<Uuid>,
    pub generation: Option<i64>,
    pub key: Option<Vec<u8>>,
    pub cap: Option<i32>,
}

pub(super) async fn verify(
    conn: &mut PgConnection,
    key: &EncryptionKey,
    idle: Option<i32>,
    request: Request,
    saved: Option<&Row>,
) -> Result<Verified, AdmissionError> {
    request.encode()?;
    let actor = request.actor;
    crate::domains::household_security::context::check(conn, actor)
        .await
        .map_err(|_| AdmissionError::Conflict)?;
    let user=sqlx::query("SELECT role,has_all_library_access FROM users WHERE id=$1 AND status='active' AND deleted_at IS NULL")
        .bind(actor.user).fetch_optional(&mut *conn).await?.ok_or(AdmissionError::Authority)?;
    authority::session(conn, actor, Some(request.profile_revision), idle).await?;
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_profiles WHERE id=$1 AND owner_user_id=$2)",
    )
    .bind(actor.profile)
    .bind(actor.user)
    .fetch_one(&mut *conn)
    .await?;
    if !owned {
        return Err(AdmissionError::Authority);
    }
    match request.subject {
        Subject::Vod { file, item } => {
            let capabilities = auth::service::resolve_capabilities_on_connection(
                conn,
                actor.user,
                &user.try_get::<String, _>("role")?,
            )
            .await
            .map_err(|_| AdmissionError::Authority)?;
            if !capabilities.iter().any(|v| v == "play_media") {
                return Err(AdmissionError::Authority);
            }
            let scope = profiles::service::load_profile_scope_on_connection(
                conn,
                actor.user,
                actor.profile,
                user.try_get("has_all_library_access")?,
            )
            .await
            .map_err(|_| AdmissionError::Authority)?;
            let matches: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM media_files WHERE id=$1 AND media_item_id=$2 FOR SHARE",
            )
            .bind(file)
            .bind(item)
            .fetch_optional(&mut *conn)
            .await?;
            if matches.is_none() {
                return Err(AdmissionError::Authority);
            }
            let locked:Option<Uuid>=sqlx::query_scalar("SELECT m.id FROM media_items m JOIN libraries l ON l.id=m.library_id WHERE m.id=$1 FOR SHARE OF m,l")
                .bind(item).fetch_optional(&mut *conn).await?;
            if locked.is_none() {
                return Err(AdmissionError::Authority);
            }
            profiles::service::assert_media_access_on_connection(conn, &scope, item)
                .await
                .map_err(|_| AdmissionError::Authority)?;
            Ok(Verified {
                assignment: None,
                generation: None,
                key: None,
                cap: None,
            })
        }
        Subject::Live {
            snapshot: requested,
            channel,
        } => {
            let mut budget = Budget::new();
            let viewing = authority::viewing(conn, actor, key, idle, &mut budget).await?;
            let snapshot = reader::snapshot(conn, key, viewing.fence, &mut budget).await?;
            let entry = if let Some(saved) = saved {
                if saved.assignment_id != Some(viewing.fence.assignment)
                    || saved.source_generation != Some(viewing.fence.generation)
                {
                    return Err(AdmissionError::Authority);
                }
                reader::channel_by_key(
                    conn,
                    key,
                    &snapshot,
                    saved
                        .channel_key
                        .as_deref()
                        .ok_or(AdmissionError::Authority)?,
                    &mut budget,
                )
                .await?
            } else {
                if requested != snapshot.id() {
                    return Err(AdmissionError::Authority);
                }
                reader::channel(conn, key, &snapshot, channel, &mut budget).await?
            };
            let (admin,owner):(Option<i32>,Option<i32>)=sqlx::query_as("SELECT max_live_streams,owner_max_live_streams FROM live_tv_assignments WHERE id=$1").bind(viewing.fence.assignment).fetch_one(conn).await?;
            if admin.is_some_and(|v| v < 0) || owner.is_some_and(|v| v < 0) {
                return Err(AdmissionError::Policy);
            }
            Ok(Verified {
                assignment: Some(viewing.fence.assignment),
                generation: Some(viewing.fence.generation),
                key: Some(entry.key),
                cap: match (admin, owner) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                },
            })
        }
    }
}
