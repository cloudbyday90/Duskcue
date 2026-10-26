// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[path = "../../../../../../tests/support/feed_assignment.rs"]
#[allow(dead_code)]
mod assignment;
#[path = "../../../../../../tests/support/catalog_http.rs"]
#[allow(dead_code)]
mod catalog_http;
use crate::qualification::database;
#[path = "../../../../../../tests/support/catalog_feeds.rs"]
#[allow(dead_code)]
mod feeds;
#[path = "../../../../../../tests/support/catalog_fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "../../../../../../tests/support/assignment_http.rs"]
#[allow(dead_code, unused_imports)]
mod http;
#[path = "../../../../../../tests/support/feed_peer.rs"]
#[allow(dead_code)]
mod peer;

#[path = "qualification/plans.rs"]
mod plans;
#[path = "qualification/seed.rs"]
mod seed;

use crate::services::encryption::context::RecordContext;
use axum::http::StatusCode;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted owned PostgreSQL18 and 250000-entry encrypted query-plan fixtures"]
async fn representative_maximum_guide_plans() -> anyhow::Result<()> {
    database::run(run).await
}

async fn run(db: &database::Database) -> anyhow::Result<()> {
    let fixture = fixture::setup(db).await?;
    let result = exercise(db, &fixture).await;
    fixture.case.state.live_tv_catalog.close();
    fixture.source.finish().await?;
    result
}

async fn exercise(db: &database::Database, fixture: &fixture::Fixture) -> anyhow::Result<()> {
    let mut evidence = Vec::new();
    for lane in [seed::Lane::Historical, seed::Lane::Dense, seed::Lane::Nulls] {
        let seeded = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            seed::populate(fixture, lane),
        )
        .await??;
        let request_count = fixture.source.count();
        let path = seeded.path();
        let first = catalog_http::call(&fixture.case.app, Some(&fixture.viewer), &path)
            .await?
            .expect(StatusCode::OK)?;
        anyhow::ensure!(
            first.value["items"]
                .as_array()
                .is_some_and(|v| v.len() == 100),
            "maximum-guide first page incorrect"
        );
        let token = first.value["cursor"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("maximum-guide cursor missing"))?;
        let session: uuid::Uuid =
            sqlx::query_scalar("SELECT id FROM user_sessions WHERE token_hash=$1")
                .bind(crate::domains::household_security::operator::fingerprint(
                    fixture.viewer.token.as_bytes(),
                ))
                .fetch_one(&db.pool)
                .await?;
        let binding = RecordContext {
            namespace: "live-tv-catalog-cursor",
            owner: fixture.viewer.user,
            record: session,
            field: "v1",
        };
        let envelope = String::from_utf8(URL_SAFE_NO_PAD.decode(token)?)?;
        let mut plain: Value = serde_json::from_str(
            &fixture
                .case
                .state
                .encryption_key
                .open_record(&envelope, binding)?,
        )?;
        plain["position"] = serde_json::to_value(&seeded.after)?;
        let cursor = URL_SAFE_NO_PAD.encode(
            fixture
                .case
                .state
                .encryption_key
                .seal_record(&plain.to_string(), binding)?,
        );
        let last = catalog_http::call(
            &fixture.case.app,
            Some(&fixture.viewer),
            &format!("{path}&cursor={cursor}"),
        )
        .await?
        .expect(StatusCode::OK)?;
        let ids = last.value["items"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("last page missing"))?
            .iter()
            .map(|v| v["id"].as_str().and_then(|v| uuid::Uuid::parse_str(v).ok()))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| anyhow::anyhow!("last IDs malformed"))?;
        anyhow::ensure!(
            ids == seeded.last
                && last.value["has_more"] == false
                && last.value.get("cursor").is_none(),
            "maximum-guide last page mixed or incomplete"
        );
        evidence.extend(plans::measure(fixture, &seeded).await?);
        anyhow::ensure!(
            fixture.source.count() == request_count,
            "maximum-guide reads fetched a feed"
        );
        println!("IPTV_CATALOG_PLAN_STAGE {}", lane.name());
    }
    let sizes: Vec<(String,i64)>=sqlx::query_as("SELECT indexname,pg_relation_size(indexname::regclass) FROM pg_indexes WHERE tablename='live_tv_catalog_programmes' ORDER BY indexname").fetch_all(&db.pool).await?;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../.cache/testing-memory")
        .join(format!("{}.catalog-plans.json", db.resource));
    tokio::fs::write(path,serde_json::to_vec_pretty(&json!({"version":1,"resource_id":db.resource,"image":db.image,"entries_per_lane":250000,"plans":evidence,"index_bytes":sizes}))?).await?;
    println!(
        "IPTV_CATALOG_PLAN_RESULT {}",
        json!({"resource_id":db.resource,"lanes":3,"entries_per_lane":250000,"plans":evidence.len()})
    );
    Ok::<_, anyhow::Error>(())
}
