// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::{codec, model::SealedRow};
use super::{Snapshot, open, projection};
use crate::{
    domains::live_tv::catalog::{
        cursor::Position,
        error::CatalogError,
        limits::Budget,
        types::{ProgrammeResponse, Window},
    },
    services::encryption::EncryptionKey,
};
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, Postgres, QueryBuilder};
use uuid::Uuid;

pub(crate) struct ProgrammeEntry {
    pub value: ProgrammeResponse,
    pub position: Position,
}
#[derive(sqlx::FromRow)]
struct ProgrammeRow {
    id: Uuid,
    guide_id: Uuid,
    starts_at: Option<DateTime<Utc>>,
    stops_at: Option<DateTime<Utc>>,
    quality_flags: i32,
    metadata: String,
}

pub(crate) struct ProgrammeQuery<'a> {
    pub guide: Option<Uuid>,
    pub window: Option<Window>,
    pub after: Option<&'a Position>,
    pub limit: u32,
}

pub(crate) async fn programmes(
    conn: &mut PgConnection,
    key: &EncryptionKey,
    snapshot: &Snapshot,
    query: ProgrammeQuery<'_>,
    budget: &mut Budget,
) -> Result<(Vec<ProgrammeEntry>, bool), CatalogError> {
    let ProgrammeQuery {
        guide,
        window,
        after,
        limit,
    } = query;
    if !(1..=100).contains(&limit) {
        return Err(CatalogError::Unavailable);
    }
    let Some(guide) = guide else {
        return Ok((Vec::new(), false));
    };
    let mut query = QueryBuilder::<Postgres>::new("");
    selection(&mut query, snapshot, guide, window, after, limit)?;
    let rows = query
        .build_query_as::<ProgrammeRow>()
        .persistent(false)
        .fetch_all(conn)
        .await?;
    for row in &rows {
        budget.cipher(row.metadata.len())?;
    }
    let more = rows.len() > limit as usize;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        if row.id.get_version_num() != 7 {
            return Err(CatalogError::Unavailable);
        }
        let position = if window.is_some() {
            Position::Timed {
                start: row.starts_at.ok_or(CatalogError::Unavailable)?,
                id: row.id,
            }
        } else {
            Position::Unpositionable { id: row.id }
        };
        let sealed = SealedRow {
            guide: Some(row.guide_id),
            start: row.starts_at,
            stop: row.stops_at,
            flags: row.quality_flags,
            metadata: row.metadata,
            ..codec::empty(row.id)
        };
        let plain = open(key, snapshot, "programme", &sealed, budget)?;
        result.push(ProgrammeEntry {
            value: projection::programme(&sealed, &plain)?,
            position,
        });
    }
    result.truncate(limit as usize);
    Ok((result, more))
}

pub(super) fn selection(
    query: &mut QueryBuilder<Postgres>,
    snapshot: &Snapshot,
    guide: Uuid,
    window: Option<Window>,
    after: Option<&Position>,
    limit: u32,
) -> Result<(), CatalogError> {
    let fence = snapshot.receipt.fence;
    query.push("WITH page AS MATERIALIZED (SELECT id,starts_at FROM live_tv_catalog_programmes WHERE household_user_id=");
    query
        .push_bind(fence.user)
        .push(" AND assignment_id=")
        .push_bind(fence.assignment)
        .push(" AND snapshot_id=")
        .push_bind(snapshot.id())
        .push(" AND guide_id=")
        .push_bind(guide);
    match window {
        Some(window) => {
            query
                .push(" AND starts_at IS NOT NULL AND stops_at IS NOT NULL AND starts_at<")
                .push_bind(window.to)
                .push(" AND stops_at>")
                .push_bind(window.from);
            if let Some(position) = after {
                let Position::Timed { start, id } = position else {
                    return Err(CatalogError::Changed);
                };
                query
                    .push(" AND (starts_at,id)>(")
                    .push_bind(*start)
                    .push(",")
                    .push_bind(*id)
                    .push(")");
            }
            query.push(" ORDER BY starts_at,id");
        }
        None => {
            query.push(" AND (starts_at IS NULL OR stops_at IS NULL)");
            if let Some(position) = after {
                let Position::Unpositionable { id } = position else {
                    return Err(CatalogError::Changed);
                };
                query.push(" AND id>").push_bind(*id);
            }
            query.push(" ORDER BY id");
        }
    }
    query.push(" LIMIT ").push_bind(i64::from(limit)+1).push(") SELECT p.id,p.guide_id,p.starts_at,p.stops_at,p.quality_flags,p.metadata FROM page JOIN live_tv_catalog_programmes p ON p.id=page.id WHERE p.household_user_id=")
        .push_bind(fence.user).push(" AND p.assignment_id=").push_bind(fence.assignment).push(" AND p.snapshot_id=").push_bind(snapshot.id()).push(" AND p.guide_id=").push_bind(guide);
    query.push(if window.is_some() {
        " ORDER BY page.starts_at,page.id"
    } else {
        " ORDER BY page.id"
    });
    Ok(())
}
