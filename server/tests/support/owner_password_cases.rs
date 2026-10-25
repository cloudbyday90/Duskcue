// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use crate::http::{self, PASSWORD, ROOT};
use axum::{Router, http::StatusCode};
use duskcue::{
    domains::household_security::operator::{fingerprint, issue_handoff},
    state::AppState,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn run(
    app: &Router,
    state: &AppState,
    pool: &PgPool,
    secrets: &mut Vec<String>,
) -> anyhow::Result<()> {
    let mut actor = http::actor(pool, "standard", true).await?;
    secrets.push(actor.token.clone());
    let viewer = http::Actor {
        user: actor.user,
        profile: actor.profile,
        token: http::session(pool, actor.user, actor.profile).await?,
        cookie: false,
    };
    secrets.push(viewer.token.clone());
    let kids = http::actor(pool, "kids", true).await?;
    secrets.push(kids.token.clone());
    anyhow::ensure!(
        http::call(
            app,
            None,
            "POST",
            &format!("{ROOT}/password"),
            Some(json!({"password":PASSWORD}))
        )
        .await?
        .0 == StatusCode::UNAUTHORIZED,
        "anonymous owner proof accepted"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&kids),
            "POST",
            &format!("{ROOT}/password"),
            Some(json!({"password":PASSWORD}))
        )
        .await?
        .0 == StatusCode::FORBIDDEN,
        "Kids gained management"
    );
    let (code, legacy, _) = http::call(
        app,
        Some(&actor),
        "POST",
        "/api/v1/profiles",
        Some(json!({"name":"Legacy creation"})),
    )
    .await?;
    anyhow::ensure!(
        code == StatusCode::CREATED,
        "unprotected profile behavior changed"
    );
    let legacy_id = legacy["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("legacy profile identity missing"))?;
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "DELETE",
            &format!("/api/v1/profiles/{legacy_id}"),
            None
        )
        .await?
        .0 == StatusCode::OK,
        "unprotected deletion changed"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "POST",
            &format!("{ROOT}/password"),
            Some(json!({"password":PASSWORD,"private-fixture-secret":true}))
        )
        .await?
        .0 == StatusCode::BAD_REQUEST,
        "unknown proof fields accepted"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "POST",
            &format!("{ROOT}/password"),
            Some(json!({"password":"x".repeat(70000)}))
        )
        .await?
        .0 == StatusCode::PAYLOAD_TOO_LARGE,
        "proof body limit missing"
    );

    let (legacy_kids, _, _) = http::call(
        app,
        Some(&kids),
        "POST",
        "/api/v1/user/passkeys/register/start",
        Some(json!({"name":"Legacy Kids credential"})),
    )
    .await?;
    anyhow::ensure!(
        legacy_kids == StatusCode::OK,
        "unprotected credential behavior changed"
    );
    let (strict, _, _) = http::call(
        app,
        Some(&actor),
        "POST",
        &format!("{ROOT}/passkey/start"),
        Some(json!({"operation":"unlock","private-fixture-secret":true})),
    )
    .await?;
    anyhow::ensure!(
        strict == StatusCode::BAD_REQUEST,
        "unexpected ceremony fields accepted"
    );
    let handoff = issue_handoff(pool, actor.user).await?;
    secrets.push(handoff.clone());
    anyhow::ensure!(
        http::status(app, &actor).await?["protection_enabled"] == false,
        "unfinished handoff activated protection"
    );
    let (code, _, _) = http::call(
        app,
        Some(&actor),
        "POST",
        &format!("{ROOT}/password"),
        Some(json!({"password":"wrong-private-fixture-secret","handoff":handoff})),
    )
    .await?;
    anyhow::ensure!(
        code == StatusCode::UNAUTHORIZED,
        "wrong password activated protection"
    );
    sqlx::query("UPDATE household_owner_handoffs SET expires_at=clock_timestamp()-interval '1 second' WHERE user_id=$1").bind(actor.user).execute(pool).await?;
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "POST",
            &format!("{ROOT}/password"),
            Some(json!({"password":PASSWORD,"handoff":handoff}))
        )
        .await?
        .0 == StatusCode::UNAUTHORIZED,
        "expired handoff accepted"
    );
    let handoff = issue_handoff(pool, actor.user).await?;
    secrets.push(handoff.clone());
    let old = actor.token.clone();
    let expiry: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM user_sessions WHERE token_hash=$1")
            .bind(fingerprint(actor.token.as_bytes()))
            .fetch_one(pool)
            .await?;
    let internal_session: Uuid =
        sqlx::query_scalar("SELECT id FROM user_sessions WHERE token_hash=$1")
            .bind(fingerprint(actor.token.as_bytes()))
            .fetch_one(pool)
            .await?;
    let old_scope = duskcue::domains::household_security::types::Scope {
        user: actor.user,
        session: internal_session,
        profile: actor.profile,
        version: 1,
    };
    http::password(app, &mut actor, Some(&handoff), secrets).await?;
    let stable: Uuid = sqlx::query_scalar("SELECT id FROM user_sessions WHERE token_hash=$1")
        .bind(fingerprint(actor.token.as_bytes()))
        .fetch_one(pool)
        .await?;
    anyhow::ensure!(
        stable == internal_session,
        "token rotation severed session-bound resources"
    );
    let mut stale =
        duskcue::domains::household_security::transactions::begin(pool, actor.user).await?;
    anyhow::ensure!(
        duskcue::domains::household_security::transactions::authorize(
            &mut stale,
            actor.user,
            Some(old_scope)
        )
        .await
        .is_err(),
        "in-flight old credential gained elevated authority"
    );
    stale.rollback().await?;
    let status = http::status(app, &actor).await?;
    anyhow::ensure!(
        status["protection_enabled"] == true
            && status["password_available"] == true
            && status["expires_at"].is_string(),
        "password owner authority missing"
    );
    let replacement_expiry: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM user_sessions WHERE token_hash=$1")
            .bind(fingerprint(actor.token.as_bytes()))
            .fetch_one(pool)
            .await?;
    anyhow::ensure!(
        expiry == replacement_expiry,
        "proof extended viewing-session expiry"
    );
    let ttl:f64=sqlx::query_scalar("SELECT extract(epoch FROM expires_at-clock_timestamp())::float8 FROM household_owner_grants WHERE user_id=$1").bind(actor.user).fetch_one(pool).await?;
    anyhow::ensure!(
        ttl > 295.0 && ttl <= 300.0,
        "management grant lifetime changed"
    );
    let old_actor = http::Actor {
        user: actor.user,
        profile: actor.profile,
        token: old,
        cookie: false,
    };
    anyhow::ensure!(
        http::call(
            app,
            Some(&old_actor),
            "GET",
            &format!("{ROOT}/status"),
            None
        )
        .await?
        .0 == StatusCode::UNAUTHORIZED,
        "old session survived privilege rotation"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&viewer),
            "POST",
            "/api/v1/profiles",
            Some(json!({"name":"Forbidden"}))
        )
        .await?
        .0 == StatusCode::FORBIDDEN,
        "viewing session created profile"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&viewer),
            "PATCH",
            &format!("/api/v1/profiles/{}", actor.profile),
            Some(json!({"max_content_rating":"NC-17"}))
        )
        .await?
        .0 == StatusCode::FORBIDDEN,
        "viewing session changed restrictions"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&viewer),
            "POST",
            "/api/v1/user/passkeys/register/start",
            Some(json!({"name":"Forbidden"}))
        )
        .await?
        .0 == StatusCode::FORBIDDEN,
        "viewing session enrolled owner factor"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "POST",
            &format!("{ROOT}/password"),
            Some(json!({"password":PASSWORD,"handoff":handoff}))
        )
        .await?
        .0 == StatusCode::UNAUTHORIZED,
        "handoff replay succeeded"
    );
    let (code, profile, _) = http::call(
        app,
        Some(&actor),
        "POST",
        "/api/v1/profiles",
        Some(json!({"name":"Protected creation","profile_type":"kids","parent_pin":"5678"})),
    )
    .await?;
    anyhow::ensure!(
        code == StatusCode::CREATED,
        "proven owner could not create Kids profile"
    );
    let child = profile["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Kids profile identity missing"))?;
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "PATCH",
            &format!("/api/v1/profiles/{child}"),
            Some(json!({"parent_pin":"6789","max_content_rating":"TV-Y"}))
        )
        .await?
        .0 == StatusCode::OK,
        "proven owner could not update parental controls"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&viewer),
            "DELETE",
            &format!("/api/v1/profiles/{child}"),
            None
        )
        .await?
        .0 == StatusCode::FORBIDDEN,
        "viewer deleted protected profile"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "DELETE",
            &format!("/api/v1/profiles/{child}"),
            None
        )
        .await?
        .0 == StatusCode::OK,
        "proven owner could not delete profile"
    );
    sqlx::query("UPDATE household_owner_grants SET expires_at=clock_timestamp()-interval '1 second' WHERE user_id=$1").bind(actor.user).execute(pool).await?;
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "POST",
            "/api/v1/profiles",
            Some(json!({"name":"Expired grant"}))
        )
        .await?
        .0 == StatusCode::FORBIDDEN,
        "expired grant authorized mutation"
    );

    http::password(app, &mut actor, None, secrets).await?;
    let other_profile = Uuid::now_v7();
    sqlx::query("INSERT INTO user_profiles(id,owner_user_id,name,profile_type) VALUES ($1,$2,'Alternate','standard')").bind(other_profile).bind(actor.user).execute(pool).await?;
    for profile in [other_profile, actor.profile] {
        anyhow::ensure!(
            http::call(
                app,
                Some(&actor),
                "POST",
                &format!("/api/v1/profiles/{profile}/switch"),
                Some(json!({"remember_on_device":false}))
            )
            .await?
            .0 == StatusCode::OK,
            "profile switch failed"
        );
    }
    anyhow::ensure!(
        http::status(app, &actor).await?["expires_at"].is_null(),
        "switch-away-and-back restored management grant"
    );
    actor.cookie = true;
    http::password(app, &mut actor, None, secrets).await?;
    anyhow::ensure!(
        http::call(app, Some(&actor), "DELETE", &format!("{ROOT}/grant"), None)
            .await?
            .0
            == StatusCode::NO_CONTENT,
        "explicit management lock failed"
    );
    anyhow::ensure!(
        http::status(app, &actor).await?["expires_at"].is_null(),
        "explicit lock retained grant"
    );

    let permits = state
        .owner_proof_slots
        .clone()
        .acquire_many_owned(2)
        .await?;
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "POST",
            &format!("{ROOT}/password"),
            Some(json!({"password":PASSWORD}))
        )
        .await?
        .0 == StatusCode::TOO_MANY_REQUESTS,
        "proof execution capacity was unbounded"
    );
    drop(permits);
    for _ in 0..5 {
        anyhow::ensure!(
            http::call(
                app,
                Some(&actor),
                "POST",
                &format!("{ROOT}/password"),
                Some(json!({"password":"wrong-private-fixture-secret"}))
            )
            .await?
            .0 == StatusCode::UNAUTHORIZED,
            "failed proof outcome changed"
        );
    }
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "POST",
            &format!("{ROOT}/password"),
            Some(json!({"password":PASSWORD}))
        )
        .await?
        .0 == StatusCode::TOO_MANY_REQUESTS,
        "local-mode owner lockout missing"
    );
    let lock_ttl:f64=sqlx::query_scalar("SELECT extract(epoch FROM locked_until-clock_timestamp())::float8 FROM household_owner_state WHERE user_id=$1").bind(actor.user).fetch_one(pool).await?;
    anyhow::ensure!(
        lock_ttl > 890.0 && lock_ttl <= 900.0,
        "proof lockout lifetime changed"
    );
    sqlx::query("UPDATE household_owner_state SET locked_until=clock_timestamp()-interval '1 second' WHERE user_id=$1").bind(actor.user).execute(pool).await?;
    http::password(app, &mut actor, None, secrets).await?;

    let mut revocation = pool.begin().await?;
    sqlx::query("UPDATE users SET password_hash=NULL WHERE id=$1")
        .bind(actor.user)
        .execute(&mut *revocation)
        .await?;
    let app_clone = app.clone();
    let user = actor.user;
    let profile = actor.profile;
    let token = actor.token.clone();
    let mutation = tokio::spawn(async move {
        let actor = http::Actor {
            user,
            profile,
            token,
            cookie: true,
        };
        http::call(
            &app_clone,
            Some(&actor),
            "POST",
            "/api/v1/profiles",
            Some(json!({"name":"Revoked race"})),
        )
        .await
    });
    let blocker: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *revocation)
        .await?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
        )
        .bind(blocker)
        .fetch_one(pool)
        .await?;
        if waiting {
            break;
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "mutation did not wait on revocation transaction"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    revocation.commit().await?;
    anyhow::ensure!(
        mutation.await??.0 == StatusCode::FORBIDDEN,
        "revocation race authorized mutation"
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_profiles WHERE owner_user_id=$1 AND name='Revoked race'",
    )
    .bind(actor.user)
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(
        count == 0 && http::status(app, &actor).await?["password_available"] == false,
        "password revocation did not invalidate trust"
    );
    Ok(())
}
