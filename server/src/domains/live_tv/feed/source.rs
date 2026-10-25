// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::FeedError;
use crate::{
    domains::live_tv::{
        assignment::secrets::{self, FeedField},
        destination::{self, Approval},
    },
    services::encryption::EncryptionKey,
};
use sqlx::{PgPool, Row};
use std::{net::SocketAddr, time::Duration};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fence {
    pub user: Uuid,
    pub assignment: Uuid,
    pub generation: i64,
}

pub(crate) struct Source {
    pub url: Url,
    pub address: SocketAddr,
    pub fence: Fence,
}

impl Fence {
    pub async fn current(self, pool: &PgPool) -> Result<(), FeedError> {
        let check = sqlx::query_scalar::<_, bool>("SELECT a.source_generation=$3 AND a.enabled AND u.status='active' AND u.deleted_at IS NULL FROM live_tv_assignments a JOIN users u ON u.id=a.household_user_id WHERE a.id=$1 AND a.household_user_id=$2")
            .bind(self.assignment).bind(self.user).bind(self.generation).fetch_optional(pool);
        match tokio::time::timeout(Duration::from_secs(1), check).await {
            Ok(Ok(Some(true))) => Ok(()),
            _ => Err(FeedError::Authority),
        }
    }
}

#[cfg(feature = "playback-qualification")]
pub(crate) async fn load(
    pool: &PgPool,
    key: &EncryptionKey,
    user: Uuid,
    feed: FeedField,
    index: usize,
) -> Result<Source, FeedError> {
    let query = match feed {
        FeedField::Playlist => {
            "SELECT a.id,a.source_generation,a.playlist_secret AS secret,a.playlist_destination AS destination,a.playlist_address_count AS count FROM live_tv_assignments a JOIN users u ON u.id=a.household_user_id WHERE u.id=$1 AND u.status='active' AND u.deleted_at IS NULL AND a.enabled"
        }
        FeedField::Guide => {
            "SELECT a.id,a.source_generation,a.guide_secret AS secret,a.guide_destination AS destination,a.guide_address_count AS count FROM live_tv_assignments a JOIN users u ON u.id=a.household_user_id WHERE u.id=$1 AND u.status='active' AND u.deleted_at IS NULL AND a.enabled"
        }
    };
    let row = tokio::time::timeout(
        Duration::from_secs(1),
        sqlx::query(query).bind(user).fetch_optional(pool),
    )
    .await
    .map_err(|_| FeedError::Authority)?
    .map_err(|_| FeedError::Authority)?
    .ok_or(FeedError::Authority)?;
    decode(row, key, user, feed, index)
}

pub(crate) fn decode(
    row: sqlx::postgres::PgRow,
    key: &EncryptionKey,
    user: Uuid,
    feed: FeedField,
    index: usize,
) -> Result<Source, FeedError> {
    let assignment = row.get("id");
    let secret: String = row.get("secret");
    let envelope: Option<String> = row.get("destination");
    let count: i16 = row.get("count");
    let raw =
        secrets::open(key, user, assignment, feed, &secret).map_err(|_| FeedError::Authority)?;
    let url = destination::source(&raw).map_err(|_| FeedError::Authority)?;
    let approval = Approval::open(
        key,
        user,
        assignment,
        feed,
        envelope.as_deref().ok_or(FeedError::Authority)?,
        &url,
    )
    .map_err(|_| FeedError::Authority)?;
    if approval.addresses.len() != count as usize {
        return Err(FeedError::Authority);
    }
    let ip = *approval.addresses.get(index).ok_or(FeedError::Authority)?;
    Ok(Source {
        address: SocketAddr::new(ip, url.port_or_known_default().ok_or(FeedError::Authority)?),
        url,
        fence: Fence {
            user,
            assignment,
            generation: row.get("source_generation"),
        },
    })
}
