// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use crate::{
    authenticator::Authenticator,
    http::{self, Actor, ROOT},
};
use axum::{Router, http::StatusCode};
use duskcue::{
    domains::household_security::operator::{fingerprint, issue_handoff},
    state::AppState,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn legacy_start(app: &Router, actor: &Actor) -> anyhow::Result<Value> {
    let (code, value, _) = http::call(
        app,
        Some(actor),
        "POST",
        "/api/v1/user/passkeys/register/start",
        Some(json!({"name":"Owner fixture"})),
    )
    .await?;
    anyhow::ensure!(
        code == StatusCode::OK,
        "existing credential registration start failed"
    );
    Ok(value)
}

async fn legacy_finish(
    app: &Router,
    actor: &Actor,
    start: &Value,
    key: &Authenticator,
) -> anyhow::Result<(StatusCode, Value)> {
    let challenge = start["challenge_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("registration challenge missing"))?;
    let credential = key.registration(&start["creation_options"])?;
    let (code, value, _) = http::call_with_challenge(
        app,
        Some(actor),
        "POST",
        "/api/v1/user/passkeys/register/finish",
        Some(json!({"credential":credential,"name":"Owner fixture"})),
        Some(challenge),
    )
    .await?;
    Ok((code, value))
}

async fn unlock(
    app: &Router,
    actor: &mut Actor,
    key: &mut Authenticator,
    secrets: &mut Vec<String>,
) -> anyhow::Result<()> {
    let start = http::start(app, actor, json!({"operation":"unlock"})).await?;
    let credential = key.assertion(&start["options"], actor.user, "")?;
    http::finish(app, actor, &start, credential, secrets).await
}

pub async fn run(
    app: &Router,
    _state: &AppState,
    pool: &PgPool,
    secrets: &mut Vec<String>,
) -> anyhow::Result<()> {
    let mut actor = http::actor(pool, "standard", false).await?;
    secrets.push(actor.token.clone());
    let viewer = Actor {
        user: actor.user,
        profile: actor.profile,
        token: http::session(pool, actor.user, actor.profile).await?,
        cookie: false,
    };
    secrets.push(viewer.token.clone());
    let other = http::actor(pool, "standard", false).await?;
    secrets.push(other.token.clone());
    let mut key = Authenticator::new()?;
    let initial = legacy_start(app, &actor).await?;
    let (code, value) = legacy_finish(app, &actor, &initial, &key).await?;
    anyhow::ensure!(
        code == StatusCode::OK,
        "genuine existing passkey registration failed"
    );
    let id: Uuid = value["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("passkey id missing"))?
        .parse()?;
    anyhow::ensure!(
        http::status(app, &actor).await?["trusted_passkeys"] == 0,
        "viewing passkey automatically became trusted"
    );
    let stale_start = legacy_start(app, &viewer).await?;
    let stale_key = Authenticator::new()?;
    let handoff = issue_handoff(pool, actor.user).await?;
    secrets.push(handoff.clone());
    let start = http::start(
        app,
        &actor,
        json!({"operation":"enroll","handoff":handoff,"passkey_id":id}),
    )
    .await?;
    let credential = key.assertion(&start["options"], actor.user, "")?;
    for wrong in [&viewer, &other] {
        anyhow::ensure!(
            http::call(
                app,
                Some(wrong),
                "POST",
                &format!("{ROOT}/passkey/finish"),
                Some(json!({"challenge_id":start["challenge_id"],"credential":credential}))
            )
            .await?
            .0 == StatusCode::UNAUTHORIZED,
            "cross-session/account ceremony accepted"
        );
    }
    let pending: i64 =
        sqlx::query_scalar("SELECT count(*) FROM household_owner_challenges WHERE user_id=$1")
            .bind(actor.user)
            .fetch_one(pool)
            .await?;
    anyhow::ensure!(pending == 1, "foreign session consumed owner ceremony");
    http::finish(app, &mut actor, &start, credential.clone(), secrets).await?;
    anyhow::ensure!(
        http::status(app, &actor).await?["trusted_passkeys"] == 1,
        "operator-authorized key not designated"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "POST",
            &format!("{ROOT}/passkey/finish"),
            Some(json!({"challenge_id":start["challenge_id"],"credential":credential}))
        )
        .await?
        .0 == StatusCode::UNAUTHORIZED,
        "successful ceremony replayed"
    );
    anyhow::ensure!(
        legacy_finish(app, &viewer, &stale_start, &stale_key)
            .await?
            .0
            == StatusCode::FORBIDDEN,
        "pre-activation registration bypassed owner guard"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "DELETE",
            &format!("/api/v1/user/passkeys/{id}"),
            None
        )
        .await?
        .0 == StatusCode::CONFLICT,
        "last trusted factor removed"
    );

    for fault in [
        "uv",
        "signature",
        "origin",
        "rp",
        "challenge",
        "user_handle",
        "cross_origin",
        "top_origin",
    ] {
        sqlx::query(
            "UPDATE household_owner_state SET failed_attempts=0,locked_until=NULL WHERE user_id=$1",
        )
        .bind(actor.user)
        .execute(pool)
        .await?;
        let challenge = http::start(app, &actor, json!({"operation":"unlock"})).await?;
        let bad = key.assertion(&challenge["options"], actor.user, fault)?;
        anyhow::ensure!(
            http::call(
                app,
                Some(&actor),
                "POST",
                &format!("{ROOT}/passkey/finish"),
                Some(json!({"challenge_id":challenge["challenge_id"],"credential":bad}))
            )
            .await?
            .0 == StatusCode::UNAUTHORIZED,
            "invalid signed assertion accepted: {fault}"
        );
        let valid = key.assertion(&challenge["options"], actor.user, "")?;
        anyhow::ensure!(
            http::call(
                app,
                Some(&actor),
                "POST",
                &format!("{ROOT}/passkey/finish"),
                Some(json!({"challenge_id":challenge["challenge_id"],"credential":valid}))
            )
            .await?
            .0 == StatusCode::UNAUTHORIZED,
            "failed assertion challenge replayed"
        );
    }
    sqlx::query(
        "UPDATE household_owner_state SET failed_attempts=0,locked_until=NULL WHERE user_id=$1",
    )
    .bind(actor.user)
    .execute(pool)
    .await?;
    let expired = http::start(app, &actor, json!({"operation":"unlock"})).await?;
    sqlx::query("UPDATE household_owner_challenges SET expires_at=clock_timestamp()-interval '1 second' WHERE user_id=$1").bind(actor.user).execute(pool).await?;
    let expired_assertion = key.assertion(&expired["options"], actor.user, "")?;
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "POST",
            &format!("{ROOT}/passkey/finish"),
            Some(json!({"challenge_id":expired["challenge_id"],"credential":expired_assertion}))
        )
        .await?
        .0 == StatusCode::UNAUTHORIZED,
        "expired assertion accepted"
    );
    let wrong_purpose = http::start(app, &actor, json!({"operation":"unlock"})).await?;
    anyhow::ensure!(http::call(app,Some(&actor),"POST",&format!("{ROOT}/passkey/finish"),Some(json!({"challenge_id":wrong_purpose["challenge_id"],"credential":key.registration(&wrong_purpose["options"])?}))).await?.0==StatusCode::UNAUTHORIZED,"registration substituted for proof");
    let optional_handle = http::start(app, &actor, json!({"operation":"unlock"})).await?;
    let assertion = key.assertion(&optional_handle["options"], actor.user, "no_handle")?;
    http::finish(app, &mut actor, &optional_handle, assertion, secrets).await?;
    actor.cookie = true;
    unlock(app, &mut actor, &mut key, secrets).await?;
    let count: i64 = sqlx::query_scalar("SELECT sign_count FROM user_passkeys WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await?;
    let serialized: Vec<u8> =
        sqlx::query_scalar("SELECT public_key FROM user_passkeys WHERE id=$1")
            .bind(id)
            .fetch_one(pool)
            .await?;
    let stored: webauthn_rs::prelude::Passkey = serde_json::from_slice(&serialized)?;
    let stored: webauthn_rs::prelude::Credential = stored.into();
    anyhow::ensure!(
        count == i64::from(key.counter) && stored.counter == key.counter,
        "verified passkey state was not persisted"
    );

    let mut second = Authenticator::new()?;
    let registration = legacy_start(app, &actor).await?;
    let (code, value) = legacy_finish(app, &actor, &registration, &second).await?;
    anyhow::ensure!(
        code == StatusCode::OK,
        "protected passkey enrollment failed"
    );
    let second_id: Uuid = value["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("second factor id missing"))?
        .parse()?;
    anyhow::ensure!(
        http::status(app, &actor).await?["trusted_passkeys"] == 2
            && http::status(app, &actor).await?["expires_at"].is_null(),
        "credential enrollment retained grant or lost trust"
    );
    unlock(app, &mut actor, &mut second, secrets).await?;
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "DELETE",
            &format!("/api/v1/user/passkeys/{id}"),
            None
        )
        .await?
        .0 == StatusCode::OK,
        "owner could not remove redundant factor"
    );
    anyhow::ensure!(
        http::status(app, &actor).await?["expires_at"].is_null(),
        "factor deletion retained grant"
    );
    unlock(app, &mut actor, &mut second, secrets).await?;
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "DELETE",
            &format!("/api/v1/user/passkeys/{second_id}"),
            None
        )
        .await?
        .0 == StatusCode::CONFLICT,
        "final factor deletion bypassed safeguard"
    );

    let capacity = crate::capacity::seed(pool, actor.user, second_id, 63, true).await?;
    unlock(app, &mut actor, &mut second, secrets).await?;
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "POST",
            "/api/v1/user/passkeys/register/start",
            Some(json!({"name":"Overflow"}))
        )
        .await?
        .0 == StatusCode::TOO_MANY_REQUESTS,
        "normal enrollment exceeded owner-factor limit"
    );
    sqlx::query("DELETE FROM user_passkeys WHERE id=ANY($1)")
        .bind(&capacity)
        .execute(pool)
        .await?;

    for _ in 0..8 {
        http::start(app, &actor, json!({"operation":"unlock"})).await?;
    }
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "POST",
            &format!("{ROOT}/passkey/start"),
            Some(json!({"operation":"unlock"}))
        )
        .await?
        .0 == StatusCode::TOO_MANY_REQUESTS,
        "per-account challenge ceiling missing"
    );
    let ttl:f64=sqlx::query_scalar("SELECT max(extract(epoch FROM expires_at-clock_timestamp()))::float8 FROM household_owner_challenges WHERE user_id=$1").bind(actor.user).fetch_one(pool).await?;
    anyhow::ensure!(ttl > 110.0 && ttl <= 120.0, "challenge lifetime changed");
    sqlx::query("UPDATE household_owner_challenges SET expires_at=clock_timestamp()-interval '1 second' WHERE user_id=$1").bind(actor.user).execute(pool).await?;
    http::start(app, &actor, json!({"operation":"unlock"})).await?;
    sqlx::query("DELETE FROM household_owner_challenges")
        .execute(pool)
        .await?;
    http::status(app, &other).await?;
    sqlx::query("INSERT INTO household_owner_challenges(user_id,session_id,credential_version,profile_id,revision,purpose,state,expires_at) SELECT $1,s.id,s.credential_version,$2,o.revision,'unlock','{}',clock_timestamp()+interval '2 minutes' FROM user_sessions s JOIN household_owner_state o ON o.user_id=s.user_id CROSS JOIN generate_series(1,4096) WHERE s.token_hash=$3")
        .bind(other.user).bind(other.profile).bind(fingerprint(other.token.as_bytes())).execute(pool).await?;
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "POST",
            &format!("{ROOT}/passkey/start"),
            Some(json!({"operation":"unlock"}))
        )
        .await?
        .0 == StatusCode::TOO_MANY_REQUESTS,
        "global challenge ceiling missing"
    );
    sqlx::query("DELETE FROM household_owner_challenges")
        .execute(pool)
        .await?;

    let handoff = issue_handoff(pool, actor.user).await?;
    secrets.push(handoff.clone());
    let recovery = http::status(app, &actor).await?;
    anyhow::ensure!(
        recovery["protection_enabled"] == true
            && recovery["expires_at"].is_null()
            && recovery["trusted_passkeys"] == 0,
        "recovery did not revoke old authority"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "POST",
            "/api/v1/user/passkeys/register/start",
            Some(json!({"name":"Forbidden recovery shortcut"}))
        )
        .await?
        .0 == StatusCode::FORBIDDEN,
        "recovery unlocked ordinary enrollment"
    );
    crate::capacity::seed(pool, actor.user, second_id, 65, false).await?;
    let replacement = Authenticator::new()?;
    let expired = http::start(
        app,
        &actor,
        json!({"operation":"register","handoff":handoff,"name":"Replacement"}),
    )
    .await?;
    sqlx::query("UPDATE household_owner_handoffs SET expires_at=clock_timestamp()-interval '1 second' WHERE user_id=$1").bind(actor.user).execute(pool).await?;
    anyhow::ensure!(http::call(app,Some(&actor),"POST",&format!("{ROOT}/passkey/finish"),Some(json!({"challenge_id":expired["challenge_id"],"credential":replacement.registration(&expired["options"])?}))).await?.0==StatusCode::UNAUTHORIZED,"expired recovery handoff completed");
    let handoff = issue_handoff(pool, actor.user).await?;
    secrets.push(handoff.clone());
    let recovery = http::start(
        app,
        &actor,
        json!({"operation":"register","handoff":handoff,"name":"Replacement"}),
    )
    .await?;
    let credential = replacement.registration(&recovery["options"])?;
    http::finish(app, &mut actor, &recovery, credential, secrets).await?;
    anyhow::ensure!(
        http::status(app, &actor).await?["trusted_passkeys"] == 1,
        "replacement owner factor not established"
    );
    let revoked: bool =
        sqlx::query_scalar("SELECT NOT owner_trusted FROM user_passkeys WHERE id=$1")
            .bind(second_id)
            .fetch_one(pool)
            .await?;
    anyhow::ensure!(revoked, "old viewing credential regained owner trust");
    let blocked = http::start(app, &actor, json!({"operation":"unlock"})).await?;
    let old_assertion = second.assertion(&blocked["options"], actor.user, "")?;
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "POST",
            &format!("{ROOT}/passkey/finish"),
            Some(json!({"challenge_id":blocked["challenge_id"],"credential":old_assertion}))
        )
        .await?
        .0 == StatusCode::UNAUTHORIZED,
        "revoked owner key regained management"
    );
    let session: Uuid = sqlx::query_scalar("SELECT id FROM user_sessions WHERE token_hash=$1")
        .bind(fingerprint(actor.token.as_bytes()))
        .fetch_one(pool)
        .await?;
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "DELETE",
            &format!("/api/v1/user/sessions/{session}"),
            None
        )
        .await?
        .0 == StatusCode::OK,
        "session revocation failed"
    );
    let grants: i64 =
        sqlx::query_scalar("SELECT count(*) FROM household_owner_grants WHERE user_id=$1")
            .bind(actor.user)
            .fetch_one(pool)
            .await?;
    anyhow::ensure!(grants == 0, "session deletion retained management grant");
    Ok(())
}
