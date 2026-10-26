// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{catalog_http, fixture::Fixture};
use crate::domains::live_tv::{
    catalog::{cursor::Position, limits::Budget, types::Window},
    feed::{
        imports::{
            codec,
            reader::{self, Snapshot},
        },
        source::Fence,
    },
};
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

#[derive(Clone, Copy)]
pub enum Lane {
    Historical,
    Dense,
    Nulls,
}
impl Lane {
    pub fn name(self) -> &'static str {
        match self {
            Self::Historical => "historical",
            Self::Dense => "dense_ties_overlap",
            Self::Nulls => "null_heavy",
        }
    }
}
pub struct Seeded {
    pub snapshot: Snapshot,
    pub guide: Uuid,
    pub channel: Uuid,
    pub lane: Lane,
    pub window: Option<Window>,
    pub after: Position,
    pub last: Vec<Uuid>,
    pub milliseconds: u128,
}
impl Seeded {
    pub fn path(&self) -> String {
        let root = format!("{}/{}/programmes", catalog_http::ROOT, self.channel);
        match self.window {
            Some(window) => format!(
                "{root}?snapshot_id={}&from={}&to={}&limit=100",
                self.snapshot.id(),
                window
                    .from
                    .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
                window
                    .to
                    .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
            ),
            None => format!(
                "{root}/unpositionable?snapshot_id={}&limit=100",
                self.snapshot.id()
            ),
        }
    }
}

pub async fn populate(fixture: &Fixture, lane: Lane) -> anyhow::Result<Seeded> {
    let began = std::time::Instant::now();
    let mut tx = fixture.case.pool.begin().await?;
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR NO KEY UPDATE")
        .bind(fixture.viewer.user)
        .execute(&mut *tx)
        .await?;
    let (assignment, generation): (Uuid, i64) = sqlx::query_as(
        "SELECT id,source_generation FROM live_tv_assignments WHERE household_user_id=$1",
    )
    .bind(fixture.viewer.user)
    .fetch_one(&mut *tx)
    .await?;
    let snapshot = reader::snapshot(
        &mut tx,
        &fixture.case.state.encryption_key,
        Fence {
            user: fixture.viewer.user,
            assignment,
            generation,
        },
        &mut Budget::new(),
    )
    .await?;
    let (guide,channel):(Uuid,Uuid)=sqlx::query_as("SELECT guide_id,min(id::text)::uuid FROM live_tv_catalog_channels WHERE snapshot_id=$1 AND guide_id IS NOT NULL GROUP BY guide_id HAVING count(*)=2").bind(snapshot.id()).fetch_one(&mut *tx).await?;
    let (channels, guides): (i32, i32) = sqlx::query_as(
        "SELECT channel_count,guide_count FROM live_tv_catalog_snapshots WHERE id=$1",
    )
    .bind(snapshot.id())
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM live_tv_catalog_programmes WHERE snapshot_id=$1")
        .bind(snapshot.id())
        .execute(&mut *tx)
        .await?;
    let origin: DateTime<Utc> = "2026-01-01T00:00:00Z".parse()?;
    let dense: DateTime<Utc> = "2026-10-25T12:00:00Z".parse()?;
    let window = match lane {
        Lane::Historical => {
            let to = origin + Duration::minutes(250000);
            Some(Window {
                from: to - Duration::hours(24),
                to,
            })
        }
        Lane::Dense => Some(Window {
            from: dense,
            to: dense + Duration::hours(1),
        }),
        Lane::Nulls => None,
    };
    let mut after = None;
    let mut last = Vec::with_capacity(100);
    let mut plain_bytes = 0usize;
    let mut cipher_bytes = 0usize;
    for batch in 0..1000 {
        let mut rows = Vec::with_capacity(250);
        for offset in 0..250 {
            let index = batch * 250 + offset;
            let mut row = codec::empty(Uuid::now_v7());
            row.guide = Some(guide);
            match lane {
                Lane::Historical => {
                    row.start = Some(origin + Duration::minutes(index));
                    row.stop = row.start.map(|v| v + Duration::minutes(1));
                }
                Lane::Dense => {
                    row.start = Some(dense);
                    row.stop = Some(dense + Duration::hours(2));
                    row.flags = 6;
                }
                Lane::Nulls => {
                    if index % 3 == 1 {
                        row.start = Some(dense);
                    } else if index % 3 == 2 {
                        row.stop = Some(dense);
                    }
                    row.flags = 1;
                }
            }
            let raw=json!({"start":row.start.map(|v|v.format("%Y%m%d%H%M%S +0000").to_string()).or_else(||matches!(lane,Lane::Nulls).then(||"2026".into())),"stop":row.stop.map(|v|v.format("%Y%m%d%H%M%S +0000").to_string()),"title":[{"lang":"en","value":format!("Plan-private-{index}")}]}).to_string();
            plain_bytes += raw.len();
            row.metadata = codec::seal(
                &fixture.case.state.encryption_key,
                snapshot.receipt,
                "programme",
                &row,
                "metadata",
                &raw,
            )?;
            cipher_bytes += row.metadata.len();
            if index == 249899 {
                after = Some(match window {
                    Some(_) => Position::Timed {
                        start: row
                            .start
                            .ok_or_else(|| anyhow::anyhow!("fixture start missing"))?,
                        id: row.id,
                    },
                    None => Position::Unpositionable { id: row.id },
                });
            }
            if index >= 249900 {
                last.push(row.id);
            }
            rows.push(row);
        }
        let mut query = QueryBuilder::<Postgres>::new(
            "INSERT INTO live_tv_catalog_programmes(id,household_user_id,assignment_id,snapshot_id,guide_id,starts_at,stops_at,quality_flags,metadata) ",
        );
        query.push_values(&rows, |mut bind, row| {
            bind.push_bind(row.id)
                .push_bind(fixture.viewer.user)
                .push_bind(assignment)
                .push_bind(snapshot.id())
                .push_bind(guide)
                .push_bind(row.start)
                .push_bind(row.stop)
                .push_bind(row.flags)
                .push_bind(&row.metadata);
        });
        query.build().persistent(false).execute(&mut *tx).await?;
    }
    anyhow::ensure!(
        plain_bytes <= 96 * 1024 * 1024 && cipher_bytes <= 144 * 1024 * 1024,
        "fixture exceeds qualified snapshot bytes"
    );
    let manifest = codec::seal(
        &fixture.case.state.encryption_key,
        snapshot.receipt,
        "snapshot",
        &codec::empty(snapshot.id()),
        "manifest",
        &codec::manifest(
            snapshot.receipt,
            (channels as usize, guides as usize, 250000),
        )
        .to_string(),
    )?;
    sqlx::query(
        "UPDATE live_tv_catalog_snapshots SET programme_count=250000,manifest=$2 WHERE id=$1",
    )
    .bind(snapshot.id())
    .bind(manifest)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    sqlx::query("ANALYZE live_tv_catalog_programmes")
        .execute(&fixture.case.pool)
        .await?;
    Ok(Seeded {
        snapshot,
        guide,
        channel,
        lane,
        window,
        after: after.ok_or_else(|| anyhow::anyhow!("last-page anchor missing"))?,
        last,
        milliseconds: began.elapsed().as_millis(),
    })
}
