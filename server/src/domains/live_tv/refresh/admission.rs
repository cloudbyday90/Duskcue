// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    authority::{self, Initiator},
    database,
    error::RefreshError,
    representation,
    types::StartRequest,
};
use crate::{
    domains::{
        household_security::types::Scope,
        live_tv::{
            assignment::{preconditions::Condition, secrets::FeedField},
            feed::{
                imports::{
                    receipt::{self, Receipt},
                    storage,
                },
                source::{self, Fence, Source},
            },
        },
    },
    error::AppError,
    services::encryption::EncryptionKey,
};
use sqlx::{PgConnection, PgPool};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub(super) struct Admitted {
    pub receipt: Receipt,
    pub sources: (Source, Source),
}

pub(super) struct Request {
    pub actor: Scope,
    pub target: Uuid,
    pub condition: Condition,
    pub pins: StartRequest,
    pub cancelled: CancellationToken,
}

pub(super) async fn claim(
    pool: &PgPool,
    key: &EncryptionKey,
    request: Request,
    stop: &CancellationToken,
) -> Result<Admitted, AppError> {
    let attempt = Uuid::now_v7();
    let snapshot = Uuid::now_v7();
    let imported_at =
        chrono::DateTime::from_timestamp_micros(chrono::Utc::now().timestamp_micros())
            .ok_or(RefreshError::Unavailable)?;
    let mut prepared = None;
    let result = database::write(pool, async |conn| {
        authority::authorize(conn, request.actor, request.target, false, true).await?;
        let resource = representation::resource(conn, request.target).await?;
        request.condition.check(Some(&representation::etag(&resource)?))?;
        if !resource.control.enabled || !resource.assignment_enabled
            || stop.is_cancelled() || request.cancelled.is_cancelled()
        {
            return Err(RefreshError::Unavailable.into());
        }
        if i16::from(request.pins.playlist_pin) >= resource.playlist_address_count
            || i16::from(request.pins.guide_pin) >= resource.guide_address_count
        {
            return Err(RefreshError::Invalid.into());
        }
        let playlist = load(conn, key, request.target, FeedField::Playlist, request.pins.playlist_pin).await?;
        let guide = load(conn, key, request.target, FeedField::Guide, request.pins.guide_pin).await?;
        receipt::lock_source(conn, playlist.fence).await.map_err(|_| RefreshError::Unavailable)?;
        let owner_revision: i64 = sqlx::query_scalar("SELECT revision FROM household_owner_state WHERE user_id=$1")
            .bind(request.actor.user).fetch_one(&mut *conn).await.map_err(|_| RefreshError::Unavailable)?;
        let sequence: i64 = sqlx::query_scalar("INSERT INTO live_tv_import_state(household_user_id,assignment_id,attempt_id,attempt_sequence,state) VALUES ($1,$2,$3,1,'running') ON CONFLICT (assignment_id) DO UPDATE SET attempt_id=$3,attempt_sequence=live_tv_import_state.attempt_sequence+1,state='running',category=NULL,attempted_at=clock_timestamp(),finished_at=NULL RETURNING attempt_sequence")
            .bind(request.target).bind(resource.assignment_id).bind(attempt)
            .fetch_one(&mut *conn).await.map_err(|_| RefreshError::Unavailable)?;
        let receipt = Receipt {
            fence: Fence { user: request.target, assignment: resource.assignment_id, generation: resource.source_generation },
            attempt, sequence, snapshot, imported_at,
            initiator: Some(Initiator { actor: request.actor, owner_revision, policy_revision: resource.control.revision }),
        };
        if playlist.fence != receipt.fence || guide.fence != receipt.fence {
            return Err(RefreshError::Unavailable.into());
        }
        sqlx::query("INSERT INTO audit_log(table_name,row_id,operation,new_data,user_id,db_user,application_name) VALUES ('live_tv_import_state',$1,'UPDATE',jsonb_build_object('event','accepted','attempt_id',$2::uuid,'sequence',$3::bigint),$4,current_user,'duskcue-live-tv-refresh')")
            .bind(resource.assignment_id).bind(attempt).bind(sequence).bind(request.actor.user)
            .execute(&mut *conn).await.map_err(|_| RefreshError::Unavailable)?;
        if stop.is_cancelled() || request.cancelled.is_cancelled() {
            return Err(RefreshError::Unavailable.into());
        }
        prepared = Some(Admitted { receipt, sources: (playlist, guide) });
        Ok(())
    }).await;
    match result {
        Ok(()) => prepared.ok_or_else(|| RefreshError::Unavailable.into()),
        Err(AppError::IptvRefresh(RefreshError::Uncertain)) => {
            let admitted = prepared.ok_or(RefreshError::Uncertain)?;
            let check = storage::write(pool, async |conn| {
                receipt::lock(conn, admitted.receipt.fence).await?;
                let found: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM live_tv_import_state WHERE assignment_id=$1 AND attempt_id=$2 AND attempt_sequence=$3)")
                    .bind(admitted.receipt.fence.assignment).bind(attempt).bind(admitted.receipt.sequence)
                    .fetch_one(conn).await?;
                Ok(found)
            }).await;
            match check {
                Ok(true) => Ok(admitted),
                Ok(false) | Err(super::super::feed::imports::ImportError::Authority) => {
                    Err(RefreshError::Uncertain.into())
                }
                Err(_) => Err(RefreshError::Cleanup.into()),
            }
        }
        Err(error) => Err(error),
    }
}

async fn load(
    conn: &mut PgConnection,
    key: &EncryptionKey,
    user: Uuid,
    feed: FeedField,
    index: u8,
) -> Result<Source, AppError> {
    let sql = match feed {
        FeedField::Playlist => {
            "SELECT id,source_generation,playlist_secret AS secret,playlist_destination AS destination,playlist_address_count AS count FROM live_tv_assignments WHERE household_user_id=$1"
        }
        FeedField::Guide => {
            "SELECT id,source_generation,guide_secret AS secret,guide_destination AS destination,guide_address_count AS count FROM live_tv_assignments WHERE household_user_id=$1"
        }
    };
    let row = sqlx::query(sql)
        .bind(user)
        .fetch_one(conn)
        .await
        .map_err(|_| RefreshError::Unavailable)?;
    source::decode(row, key, user, feed, usize::from(index))
        .map_err(|_| RefreshError::Unavailable.into())
}
