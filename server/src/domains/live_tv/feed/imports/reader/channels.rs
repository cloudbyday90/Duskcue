// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::{codec, model::SealedRow};
use super::{Snapshot, open, projection};
use crate::{
    domains::live_tv::catalog::{
        error::CatalogError,
        limits::Budget,
        types::{ChannelResponse, GuideResponse},
    },
    services::encryption::EncryptionKey,
};
use sqlx::PgConnection;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub(crate) struct ChannelEntry {
    pub value: ChannelResponse,
    pub key: Vec<u8>,
    pub guide: Option<Uuid>,
}

#[derive(sqlx::FromRow)]
struct ChannelRow {
    id: Uuid,
    channel_key: Vec<u8>,
    guide_id: Option<Uuid>,
    metadata: String,
}
#[derive(sqlx::FromRow)]
struct GuideRow {
    id: Uuid,
    metadata: String,
}

async fn guides(
    conn: &mut PgConnection,
    key: &EncryptionKey,
    snapshot: &Snapshot,
    ids: &[Uuid],
    budget: &mut Budget,
) -> Result<BTreeMap<Uuid, GuideResponse>, CatalogError> {
    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    if ids.len() > 100 {
        return Err(CatalogError::Unavailable);
    }
    let fence = snapshot.receipt.fence;
    let rows:Vec<GuideRow>=sqlx::query_as("SELECT id,metadata FROM live_tv_catalog_guides WHERE household_user_id=$1 AND assignment_id=$2 AND snapshot_id=$3 AND id=ANY($4)")
        .bind(fence.user).bind(fence.assignment).bind(snapshot.id()).bind(ids).fetch_all(conn).await?;
    if rows.len() != ids.len() {
        return Err(CatalogError::Unavailable);
    }
    let mut result = BTreeMap::new();
    for row in rows {
        budget.cipher(row.metadata.len())?;
        let mut sealed = codec::empty(row.id);
        sealed.metadata = row.metadata;
        let plain = open(key, snapshot, "guide", &sealed, budget)?;
        result.insert(row.id, projection::guide(&plain)?);
    }
    Ok(result)
}

async fn project(
    conn: &mut PgConnection,
    key: &EncryptionKey,
    snapshot: &Snapshot,
    rows: Vec<ChannelRow>,
    budget: &mut Budget,
) -> Result<Vec<ChannelEntry>, CatalogError> {
    let ids = rows
        .iter()
        .filter_map(|row| row.guide_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let guides = guides(conn, key, snapshot, &ids, budget).await?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        if row.id.get_version_num() != 7 || row.channel_key.len() != 32 {
            return Err(CatalogError::Unavailable);
        }
        let sealed = SealedRow {
            id: row.id,
            guide: row.guide_id,
            channel_key: Some(row.channel_key.clone()),
            metadata: row.metadata,
            ..codec::empty(row.id)
        };
        let plain = open(key, snapshot, "channel", &sealed, budget)?;
        let guide = row
            .guide_id
            .map(|id| guides.get(&id).cloned().ok_or(CatalogError::Unavailable))
            .transpose()?;
        result.push(ChannelEntry {
            value: projection::channel(row.id, &plain, guide)?,
            key: row.channel_key,
            guide: row.guide_id,
        });
    }
    Ok(result)
}

pub(crate) async fn channels(
    conn: &mut PgConnection,
    key: &EncryptionKey,
    snapshot: &Snapshot,
    after: &[u8],
    limit: u32,
    budget: &mut Budget,
) -> Result<(Vec<ChannelEntry>, bool), CatalogError> {
    if !(1..=100).contains(&limit) || (!after.is_empty() && after.len() != 32) {
        return Err(CatalogError::Unavailable);
    }
    let fence = snapshot.receipt.fence;
    let mut rows:Vec<ChannelRow>=sqlx::query_as("SELECT id,channel_key,guide_id,metadata FROM live_tv_catalog_channels WHERE household_user_id=$1 AND assignment_id=$2 AND snapshot_id=$3 AND channel_key>$4 ORDER BY channel_key LIMIT $5")
        .bind(fence.user).bind(fence.assignment).bind(snapshot.id()).bind(after).bind(i64::from(limit)+1).fetch_all(&mut *conn).await?;
    for row in &rows {
        budget.cipher(row.metadata.len())?;
    }
    let more = rows.len() > limit as usize;
    if more {
        let row = rows.last().ok_or(CatalogError::Unavailable)?;
        let sealed = SealedRow {
            guide: row.guide_id,
            channel_key: Some(row.channel_key.clone()),
            metadata: row.metadata.clone(),
            ..codec::empty(row.id)
        };
        if row.id.get_version_num() != 7 || row.channel_key.len() != 32 {
            return Err(CatalogError::Unavailable);
        }
        let plain = open(key, snapshot, "channel", &sealed, budget)?;
        projection::channel(row.id, &plain, None)?;
    }
    rows.truncate(limit as usize);
    Ok((project(conn, key, snapshot, rows, budget).await?, more))
}

pub(crate) async fn channel(
    conn: &mut PgConnection,
    key: &EncryptionKey,
    snapshot: &Snapshot,
    id: Uuid,
    budget: &mut Budget,
) -> Result<ChannelEntry, CatalogError> {
    let fence = snapshot.receipt.fence;
    let row:ChannelRow=sqlx::query_as("SELECT id,channel_key,guide_id,metadata FROM live_tv_catalog_channels WHERE household_user_id=$1 AND assignment_id=$2 AND snapshot_id=$3 AND id=$4")
        .bind(fence.user).bind(fence.assignment).bind(snapshot.id()).bind(id).fetch_optional(&mut *conn).await?.ok_or(CatalogError::NotFound)?;
    budget.cipher(row.metadata.len())?;
    project(conn, key, snapshot, vec![row], budget)
        .await?
        .pop()
        .ok_or(CatalogError::Unavailable)
}
