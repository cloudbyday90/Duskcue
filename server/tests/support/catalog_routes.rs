// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{catalog_http as api, fixture::Fixture};
use axum::http::StatusCode;
use serde_json::Value;
use std::collections::BTreeSet;
use uuid::Uuid;

pub struct Inventory {
    pub channel: Uuid,
    pub other: Uuid,
    pub undeclared: Uuid,
    pub cursor: String,
    pub programmes: Vec<Uuid>,
}

pub fn programme_path(
    snapshot: Uuid,
    channel: Uuid,
    unpositionable: bool,
    limit: u32,
    cursor: Option<&str>,
) -> String {
    let suffix = if unpositionable {
        "/unpositionable"
    } else {
        ""
    };
    let mut path = format!(
        "{}/{channel}/programmes{suffix}?snapshot_id={snapshot}&limit={limit}",
        api::ROOT
    );
    if !unpositionable {
        path.push_str("&from=2026-10-25T12:00:00Z&to=2026-10-25T13:00:00Z");
    }
    if let Some(cursor) = cursor {
        path.push_str("&cursor=");
        path.push_str(cursor);
    }
    path
}

pub async fn run(fixture: &Fixture) -> anyhow::Result<Inventory> {
    let case = &fixture.case;
    api::call(&case.app, None, api::ROOT)
        .await?
        .expect(StatusCode::UNAUTHORIZED)?;
    api::call(&case.app, Some(&case.other), api::ROOT)
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    let proofs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM household_owner_grants WHERE user_id=$1")
            .bind(case.owner.user)
            .fetch_one(&case.pool)
            .await?;
    anyhow::ensure!(
        proofs == 0,
        "source writes did not revoke management grants before catalog read"
    );
    let mut channels = Vec::new();
    let mut cursor = None;
    let mut first_cursor = None;
    loop {
        let path = cursor.as_ref().map_or_else(
            || format!("{}?limit=1", api::ROOT),
            |cursor| format!("{}?limit=1&cursor={cursor}", api::ROOT),
        );
        let reply = api::call(&case.app, Some(&fixture.viewer), &path)
            .await?
            .expect(StatusCode::OK)?;
        anyhow::ensure!(
            reply.value["context"]["snapshot_id"] == fixture.snapshot.to_string()
                && reply.value["context"]["channel_count"] == 4,
            "catalog context changed during channel paging"
        );
        let items = reply.value["items"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("catalog items missing"))?;
        anyhow::ensure!(items.len() == 1, "channel page ignored limit");
        channels.push(items[0].clone());
        cursor = reply.value["cursor"].as_str().map(str::to_owned);
        if first_cursor.is_none() {
            first_cursor = cursor.clone();
        }
        if reply.value["has_more"] == false {
            anyhow::ensure!(cursor.is_none(), "final channel page retained cursor");
            break;
        }
        anyhow::ensure!(
            channels.len() < 5 && cursor.is_some(),
            "channel paging did not progress"
        );
    }
    let identities = channels
        .iter()
        .map(|v| v["id"].as_str().unwrap_or_default())
        .collect::<BTreeSet<_>>();
    anyhow::ensure!(
        identities.len() == 4
            && identities
                .iter()
                .all(|v| Uuid::parse_str(v).is_ok_and(|id| id.get_version_num() == 7)),
        "channel entries lost UUIDv7 uniqueness"
    );
    let channel = entry(&channels, "Channel-private")?;
    let other = entry(&channels, "Other-private")?;
    let empty = entry(&channels, "")?;
    let undeclared = entry(&channels, "Undeclared-private")?;
    let first = channels
        .iter()
        .find(|v| v["name"] == "Channel-private")
        .ok_or_else(|| anyhow::anyhow!("channel metadata missing"))?;
    anyhow::ensure!(
        first["number"] == "007"
            && first["group"] == "News<&>"
            && first["guide"]["ambiguous"] == true
            && first["guide"]["declarations"]
                .as_array()
                .is_some_and(|v| v.len() == 2),
        "guide/number/group evidence was rewritten"
    );
    let mut programmes = Vec::new();
    let mut next = None;
    loop {
        let path = programme_path(fixture.snapshot, channel, false, 1, next.as_deref());
        let reply = api::call(&case.app, Some(&case.owner), &path)
            .await?
            .expect(StatusCode::OK)?;
        let items = reply.value["items"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("programme page missing"))?;
        programmes.extend(items.iter().cloned());
        next = reply.value["cursor"].as_str().map(str::to_owned);
        if reply.value["has_more"] == false {
            break;
        }
        anyhow::ensure!(
            programmes.len() < 4 && next.is_some(),
            "programme paging did not progress"
        );
    }
    anyhow::ensure!(
        programmes.len() == 3,
        "guide window lost half-open bounds or duplicate coalescing"
    );
    let overlapping = programmes
        .iter()
        .filter(|v| v["quality"]["overlap"] == true)
        .count();
    let conflicts = programmes
        .iter()
        .filter(|v| v["quality"]["conflict"] == true)
        .count();
    anyhow::ensure!(
        overlapping >= 2
            && conflicts == 2
            && programmes.iter().any(|v| v["quality"]["clump"] == true),
        "ambiguous programme evidence was suppressed"
    );
    anyhow::ensure!(
        programmes
            .iter()
            .any(|v| v["titles"][0]["language"] == "unknown_tag")
            && programmes
                .iter()
                .any(|v| v["titles"].as_array().is_some_and(|v| v.len() == 2)),
        "language variants were normalized or discarded"
    );
    let ids = programmes
        .iter()
        .map(|v| Uuid::parse_str(v["id"].as_str().unwrap_or_default()))
        .collect::<Result<Vec<_>, _>>()?;
    anyhow::ensure!(
        ids.iter().all(|id| id.get_version_num() == 7)
            && ids.iter().collect::<BTreeSet<_>>().len() == 3,
        "programme identity changed"
    );
    let shared = api::call(
        &case.app,
        Some(&fixture.viewer),
        &programme_path(fixture.snapshot, other, false, 100, None),
    )
    .await?
    .expect(StatusCode::OK)?;
    anyhow::ensure!(
        shared.value["items"].as_array().is_some_and(|v| v
            .iter()
            .map(|v| v["id"].as_str().unwrap_or_default())
            .collect::<Vec<_>>()
            == ids.iter().map(Uuid::to_string).collect::<Vec<_>>()),
        "shared guide merged channel identity or changed programme relation"
    );
    let mut uncertain = Vec::new();
    let mut next = None;
    loop {
        let reply = api::call(
            &case.app,
            Some(&fixture.viewer),
            &programme_path(fixture.snapshot, channel, true, 2, next.as_deref()),
        )
        .await?
        .expect(StatusCode::OK)?;
        anyhow::ensure!(
            reply.value["view"] == "unpositionable" && reply.value["window"].is_null(),
            "uncertain evidence was placed on a timeline"
        );
        uncertain.extend(
            reply.value["items"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("uncertain items missing"))?
                .iter()
                .cloned(),
        );
        next = reply.value["cursor"].as_str().map(str::to_owned);
        if reply.value["has_more"] == false {
            break;
        }
        anyhow::ensure!(
            uncertain.len() < 5 && next.is_some(),
            "uncertain paging did not progress"
        );
    }
    anyhow::ensure!(
        uncertain.len() == 4
            && uncertain.iter().all(|v| v["quality"]["uncertain"] == true
                && (v["starts_at"].is_null() || v["stops_at"].is_null())),
        "uncertain timing evidence was invented"
    );
    for channel in [empty, undeclared] {
        let reply = api::call(
            &case.app,
            Some(&fixture.viewer),
            &programme_path(fixture.snapshot, channel, false, 20, None),
        )
        .await?
        .expect(StatusCode::OK)?;
        anyhow::ensure!(
            reply.value["items"] == serde_json::json!([]) && reply.value["has_more"] == false,
            "empty guide was not explicit"
        );
        anyhow::ensure!(
            if channel == empty {
                reply.value["guide"].is_null()
            } else {
                reply.value["guide"]["undeclared"] == true
            },
            "missing guide declaration was invented"
        );
    }
    api::call(
        &case.app,
        Some(&fixture.viewer),
        &programme_path(fixture.snapshot, case.other.profile, false, 20, None),
    )
    .await?
    .expect(StatusCode::NOT_FOUND)?;
    for suffix in [
        "?limit=0",
        "?limit=101",
        "?limit=1&limit=2",
        "?profile_id=x",
        "?household_id=x",
        "?cursor=x",
    ] {
        api::call(
            &case.app,
            Some(&fixture.viewer),
            &format!("{}{suffix}", api::ROOT),
        )
        .await?
        .expect(StatusCode::BAD_REQUEST)?;
    }
    api::call(
        &case.app,
        Some(&fixture.viewer),
        &format!(
            "{}/{channel}/programmes/unpositionable?snapshot_id={}&from=2026",
            api::ROOT,
            fixture.snapshot
        ),
    )
    .await?
    .expect(StatusCode::BAD_REQUEST)?;
    for (from, to) in [
        ("2026-10-25T12:00:00.0000000001Z", "2026-10-25T13:00:00Z"),
        ("2016-12-31T23:59:60Z", "2017-01-01T01:00:00Z"),
    ] {
        api::call(
            &case.app,
            Some(&fixture.viewer),
            &format!(
                "{}/{channel}/programmes?snapshot_id={}&from={from}&to={to}",
                api::ROOT,
                fixture.snapshot
            ),
        )
        .await?
        .expect(StatusCode::BAD_REQUEST)?;
    }
    println!("IPTV_CATALOG_STAGE typed_metadata_routes_passed");
    Ok(Inventory {
        channel,
        other,
        undeclared,
        cursor: first_cursor.ok_or_else(|| anyhow::anyhow!("first cursor absent"))?,
        programmes: ids,
    })
}

fn entry(channels: &[Value], name: &str) -> anyhow::Result<Uuid> {
    Ok(Uuid::parse_str(
        channels
            .iter()
            .find(|v| v["name"] == name)
            .and_then(|v| v["id"].as_str())
            .ok_or_else(|| anyhow::anyhow!("catalog channel absent"))?,
    )?)
}
