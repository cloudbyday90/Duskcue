// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{catalog_http as api, fixture::Fixture, http, routes::Inventory};
use axum::http::StatusCode;
use duskcue::domains::household_security::operator::fingerprint;
use serde_json::json;
use uuid::Uuid;

pub async fn session(fixture: &Fixture) -> anyhow::Result<Uuid> {
    Ok(
        sqlx::query_scalar("SELECT id FROM user_sessions WHERE user_id=$1 AND token_hash=$2")
            .bind(fixture.viewer.user)
            .bind(fingerprint(fixture.viewer.token.as_bytes()))
            .fetch_one(&fixture.case.pool)
            .await?,
    )
}

async fn revision(fixture: &Fixture) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT profile_context_revision FROM user_sessions WHERE id=$1")
            .bind(session(fixture).await?)
            .fetch_one(&fixture.case.pool)
            .await?,
    )
}

pub async fn run(fixture: &mut Fixture, inventory: &Inventory) -> anyhow::Result<()> {
    let first = revision(fixture).await?;
    sqlx::query("UPDATE user_sessions SET profile_context_revision=999,last_active_at=clock_timestamp(),metadata=jsonb_build_object('ordinary_activity',true) WHERE id=$1").bind(session(fixture).await?).execute(&fixture.case.pool).await?;
    anyhow::ensure!(
        revision(fixture).await? == first,
        "ordinary activity changed or caller overwrote profile revision"
    );
    for target in [fixture.second, fixture.case.standard] {
        http::call(
            &fixture.case.app,
            Some(&fixture.viewer),
            "POST",
            &format!("/api/v1/profiles/{target}/switch"),
            Some(json!({"remember_on_device":false})),
            &[],
        )
        .await?
        .expect(StatusCode::OK)?;
        api::call(
            &fixture.case.app,
            Some(&fixture.viewer),
            &format!("{}?limit=1&cursor={}", api::ROOT, inventory.cursor),
        )
        .await?
        .expect(StatusCode::CONFLICT)?;
    }
    anyhow::ensure!(
        revision(fixture).await? == first + 2,
        "A-to-B-to-A did not advance context twice"
    );
    let token = api::call(
        &fixture.case.app,
        Some(&fixture.viewer),
        &format!("{}?limit=1", api::ROOT),
    )
    .await?
    .expect(StatusCode::OK)?
    .value["cursor"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("fresh cursor missing"))?
        .to_owned();
    sqlx::query("UPDATE user_sessions SET profile_selection_required=true WHERE id=$1")
        .bind(session(fixture).await?)
        .execute(&fixture.case.pool)
        .await?;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    sqlx::query("UPDATE user_sessions SET profile_selection_required=false WHERE id=$1")
        .bind(session(fixture).await?)
        .execute(&fixture.case.pool)
        .await?;
    api::call(
        &fixture.case.app,
        Some(&fixture.viewer),
        &format!("{}?limit=1&cursor={token}", api::ROOT),
    )
    .await?
    .expect(StatusCode::CONFLICT)?;
    anyhow::ensure!(
        revision(fixture).await? == first + 4,
        "selection round trip revived paging context"
    );
    let id = session(fixture).await?;
    let mut tx = fixture.case.pool.begin().await?;
    sqlx::query("UPDATE user_sessions SET active_profile_id=$2 WHERE id=$1")
        .bind(id)
        .bind(fixture.second)
        .execute(&mut *tx)
        .await?;
    tx.rollback().await?;
    anyhow::ensure!(
        revision(fixture).await? == first + 4,
        "rolled-back context change escaped its transaction"
    );
    let token = api::call(
        &fixture.case.app,
        Some(&fixture.viewer),
        &format!("{}?limit=1", api::ROOT),
    )
    .await?
    .expect(StatusCode::OK)?
    .value["cursor"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("fresh cursor missing"))?
        .to_owned();
    let previous = fixture.viewer.token.clone();
    http::unlock(
        &fixture.case.app,
        &fixture.case.state,
        &mut fixture.viewer,
        false,
        &mut fixture.case.secrets,
    )
    .await?;
    let old = http::Actor {
        user: fixture.viewer.user,
        profile: fixture.case.standard,
        token: previous,
        cookie: false,
    };
    api::call(&fixture.case.app, Some(&old), api::ROOT)
        .await?
        .expect(StatusCode::UNAUTHORIZED)?;
    api::call(
        &fixture.case.app,
        Some(&fixture.viewer),
        &format!("{}?limit=1&cursor={token}", api::ROOT),
    )
    .await?
    .expect(StatusCode::CONFLICT)?;
    anyhow::ensure!(
        revision(fixture).await? == first + 4,
        "credential rotation changed the profile-only revision"
    );
    overflow(fixture).await?;
    capabilities(fixture).await?;
    expiration(fixture).await?;
    println!("IPTV_CATALOG_STAGE profile_epoch_and_authority_passed");
    Ok(())
}

async fn overflow(fixture: &mut Fixture) -> anyhow::Result<()> {
    let token = duskcue::domains::auth::service::generate_session_token();
    fixture.case.secrets.push(token.clone());
    let id:Uuid=sqlx::query_scalar("INSERT INTO user_sessions(user_id,active_profile_id,token_hash,expires_at,profile_context_revision) VALUES ($1,$2,$3,clock_timestamp()+interval '1 hour',$4) RETURNING id")
        .bind(fixture.viewer.user).bind(fixture.case.standard).bind(fingerprint(token.as_bytes())).bind(i64::MAX).fetch_one(&fixture.case.pool).await?;
    let actor = http::Actor {
        user: fixture.viewer.user,
        profile: fixture.case.standard,
        token,
        cookie: false,
    };
    fixture.case.secrets.push(actor.token.clone());
    let response = api::call(&fixture.case.app, Some(&actor), api::ROOT)
        .await?
        .expect(StatusCode::OK)?;
    anyhow::ensure!(
        response.value["context"]["profile_context_revision"].as_str()
            == Some("9223372036854775807"),
        "large profile revision lost integer precision"
    );
    let refused = sqlx::query("UPDATE user_sessions SET active_profile_id=$2 WHERE id=$1")
        .bind(id)
        .bind(fixture.second)
        .execute(&fixture.case.pool)
        .await;
    anyhow::ensure!(
        refused.is_err(),
        "exhausted context revision wrapped or reset"
    );
    let unchanged: (Uuid, i64) = sqlx::query_as(
        "SELECT active_profile_id,profile_context_revision FROM user_sessions WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&fixture.case.pool)
    .await?;
    anyhow::ensure!(
        unchanged == (fixture.case.standard, i64::MAX),
        "failed context change partially applied"
    );
    sqlx::query("DELETE FROM user_sessions WHERE id=$1")
        .bind(id)
        .execute(&fixture.case.pool)
        .await?;
    Ok(())
}

async fn capabilities(fixture: &mut Fixture) -> anyhow::Result<()> {
    let pool = &fixture.case.pool;
    let user = fixture.viewer.user;
    sqlx::query("UPDATE users SET role='admin' WHERE id=$1")
        .bind(user)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM user_capabilities WHERE user_id=$1 AND capability='can_use_live_tv'")
        .bind(user)
        .execute(pool)
        .await?;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    sqlx::query("INSERT INTO user_capabilities(user_id,capability,is_granted) VALUES ($1,'can_use_live_tv',true)").bind(user).execute(pool).await?;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::OK)?;
    sqlx::query("UPDATE user_capabilities SET is_granted=false WHERE user_id=$1 AND capability='can_use_live_tv'").bind(user).execute(pool).await?;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    sqlx::query("UPDATE users SET role='owner' WHERE id=$1")
        .bind(user)
        .execute(pool)
        .await?;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::OK)?;
    sqlx::query("UPDATE live_tv_profile_access SET enabled=false WHERE household_user_id=$1 AND profile_id=$2").bind(user).bind(fixture.case.standard).execute(pool).await?;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    sqlx::query("UPDATE live_tv_profile_access SET enabled=true WHERE household_user_id=$1 AND profile_id=$2").bind(user).bind(fixture.case.standard).execute(pool).await?;
    let kids = http::Actor {
        user,
        profile: fixture.case.kids,
        token: http::session(pool, user, fixture.case.kids).await?,
        cookie: false,
    };
    fixture.case.secrets.push(kids.token.clone());
    sqlx::query("INSERT INTO live_tv_profile_access(household_user_id,assignment_id,profile_id,enabled) SELECT household_user_id,id,$2,true FROM live_tv_assignments WHERE household_user_id=$1 ON CONFLICT (assignment_id,profile_id) DO UPDATE SET enabled=true").bind(user).bind(fixture.case.kids).execute(pool).await?;
    api::call(&fixture.case.app, Some(&kids), api::ROOT)
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    sqlx::query("UPDATE users SET role='member' WHERE id=$1")
        .bind(user)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE user_capabilities SET is_granted=true WHERE user_id=$1 AND capability='can_use_live_tv'").bind(user).execute(pool).await?;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::OK)?;
    Ok(())
}

async fn expiration(fixture: &mut Fixture) -> anyhow::Result<()> {
    let actor = http::Actor {
        user: fixture.viewer.user,
        profile: fixture.case.standard,
        token: http::session(
            &fixture.case.pool,
            fixture.viewer.user,
            fixture.case.standard,
        )
        .await?,
        cookie: false,
    };
    fixture.case.secrets.push(actor.token.clone());
    http::call(
        &fixture.case.app,
        Some(&actor),
        "POST",
        "/api/v1/auth/logout",
        None,
        &[],
    )
    .await?
    .expect(StatusCode::OK)?;
    api::call(&fixture.case.app, Some(&actor), api::ROOT)
        .await?
        .expect(StatusCode::UNAUTHORIZED)?;
    let actor = http::Actor {
        user: fixture.viewer.user,
        profile: fixture.case.standard,
        token: http::session(
            &fixture.case.pool,
            fixture.viewer.user,
            fixture.case.standard,
        )
        .await?,
        cookie: false,
    };
    fixture.case.secrets.push(actor.token.clone());
    sqlx::query("UPDATE user_sessions SET expires_at=clock_timestamp()-interval '1 second' WHERE token_hash=$1").bind(fingerprint(actor.token.as_bytes())).execute(&fixture.case.pool).await?;
    api::call(&fixture.case.app, Some(&actor), api::ROOT)
        .await?
        .expect(StatusCode::UNAUTHORIZED)?;
    Ok(())
}
