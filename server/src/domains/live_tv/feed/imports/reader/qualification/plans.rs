// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{fixture::Fixture, seed::Seeded};
use crate::domains::live_tv::{
    catalog::{cursor::Position, types::Window},
    feed::imports::reader::programmes,
};
use serde_json::{Value, json};
use sqlx::{Postgres, QueryBuilder};

pub async fn measure(fixture: &Fixture, seeded: &Seeded) -> anyhow::Result<Vec<Value>> {
    let mut evidence = Vec::new();
    for (name, window, after) in [
        ("first", seeded.window, None),
        ("last", seeded.window, Some(&seeded.after)),
    ] {
        evidence.push(plan(fixture, seeded, name, window, after).await?);
    }
    if seeded.window.is_some() {
        for name in ["empty_unpositionable", "empty_unpositionable_without_index"] {
            evidence.push(plan(fixture, seeded, name, None, None).await?);
        }
        evidence.push(
            plan(
                fixture,
                seeded,
                "empty",
                Some(Window {
                    from: "2027-10-25T00:00:00Z".parse()?,
                    to: "2027-10-26T00:00:00Z".parse()?,
                }),
                None,
            )
            .await?,
        );
    }
    Ok(evidence)
}
async fn plan(
    fixture: &Fixture,
    seeded: &Seeded,
    name: &str,
    window: Option<Window>,
    after: Option<&Position>,
) -> anyhow::Result<Value> {
    let mut tx = fixture.case.pool.begin().await?;
    sqlx::query("SET LOCAL statement_timeout='1500ms'")
        .execute(&mut *tx)
        .await?;
    if name == "empty_unpositionable_without_index" {
        sqlx::query("DROP INDEX live_tv_catalog_programmes_unpositionable")
            .execute(&mut *tx)
            .await?;
    }
    let mut query = QueryBuilder::<Postgres>::new("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) ");
    programmes::selection(
        &mut query,
        &seeded.snapshot,
        seeded.guide,
        window,
        after,
        100,
    )?;
    let (value,): (Value,) = query
        .build_query_as()
        .persistent(false)
        .fetch_one(&mut *tx)
        .await?;
    tx.rollback().await?;
    println!(
        "IPTV_CATALOG_PLAN {} {} execution_ms={}",
        seeded.lane.name(),
        name,
        value[0]["Execution Time"]
    );
    Ok(
        json!({"lane":seeded.lane.name(),"page":name,"seed_milliseconds":seeded.milliseconds,"explain":value}),
    )
}
