// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    http::Case,
    import_fixture as fixture,
    peer::{self, Behavior, Peer, Security},
    refresh_cases,
};
use axum::http::StatusCode;
use sqlx::{Connection, PgConnection};
use std::{sync::Arc, time::Duration};
use tokio::sync::Notify;

pub async fn run(case: &mut Case, url: &str) -> anyhow::Result<()> {
    case.admin_proof(false).await?;
    refresh_cases::enable(case, true).await?;
    let prior = fixture::status(case).await?.snapshot_id;
    let gate = Arc::new(Notify::new());
    let a = Peer::new(
        "127.0.0.1".parse()?,
        Security::Plain,
        Behavior::Reply(peer::response(&fixture::playlist())),
    )
    .await?;
    let b = Peer::new(
        "127.0.0.1".parse()?,
        Security::Tls,
        Behavior::GatedReply(peer::response(&fixture::guide()), gate.clone()),
    )
    .await?;
    fixture::prepare(case, &a, &b).await?;
    refresh_cases::start(case)
        .await?
        .expect(StatusCode::ACCEPTED)?;
    b.wait_phase(3).await?;
    let mut connection = PgConnection::connect(url).await?;
    let mut tx = connection.begin().await?;
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(case.admin.user)
        .fetch_one(&mut *tx)
        .await?;
    gate.notify_one();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE 'SELECT id FROM users WHERE id=ANY%')").fetch_one(&case.pool).await?;
            if waiting {return Ok::<_,anyhow::Error>(());}
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await??;
    sqlx::query("INSERT INTO user_capabilities(user_id,capability,is_granted) VALUES ($1,'can_manage_users',false) ON CONFLICT (user_id,capability) DO UPDATE SET is_granted=false").bind(case.admin.user).execute(&mut *tx).await?;
    tx.commit().await?;
    refresh_cases::wait_idle(case.state.live_tv_refresh.get().unwrap()).await?;
    let current = fixture::status(case).await?;
    anyhow::ensure!(
        current.snapshot_id == prior
            && current.state == "failed"
            && current.category.as_deref() == Some("authority"),
        "revoked initiating authority published or lost last-good data"
    );
    sqlx::query("DELETE FROM user_capabilities WHERE user_id=$1 AND capability='can_manage_users'")
        .bind(case.admin.user)
        .execute(&case.pool)
        .await?;
    a.finish(true).await?;
    b.finish(true).await?;
    println!("IPTV_REFRESH_STAGE observed_publication_authority_race_passed");
    Ok(())
}
