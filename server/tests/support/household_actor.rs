// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use duskcue::domains::{
    auth::service::generate_session_token, household_security::operator::fingerprint,
};
use sqlx::PgPool;
use uuid::Uuid;

pub const PASSWORD: &str = "private-fixture-secret";

pub struct Actor {
    pub user: Uuid,
    pub profile: Uuid,
    pub token: String,
    pub cookie: bool,
}

pub async fn actor(pool: &PgPool, profile_type: &str, password: bool) -> anyhow::Result<Actor> {
    let user = Uuid::now_v7();
    let profile = Uuid::now_v7();
    let hash = if password {
        Some(duskcue::domains::auth::service::hash_password(PASSWORD)?)
    } else {
        None
    };
    sqlx::query("INSERT INTO users(id,username,display_name,role,password_hash) VALUES ($1,$2,'Owner security fixture','member',$3)")
        .bind(user).bind(format!("owner-fixture-{user}")).bind(hash).execute(pool).await?;
    sqlx::query("INSERT INTO user_profiles(id,owner_user_id,name,profile_type,is_default) VALUES ($1,$2,'Fixture',$3,true)")
        .bind(profile).bind(user).bind(profile_type).execute(pool).await?;
    let token = session(pool, user, profile).await?;
    Ok(Actor {
        user,
        profile,
        token,
        cookie: false,
    })
}

pub async fn session(pool: &PgPool, user: Uuid, profile: Uuid) -> anyhow::Result<String> {
    let token = generate_session_token();
    sqlx::query("INSERT INTO user_sessions(user_id,active_profile_id,token_hash,expires_at) VALUES ($1,$2,$3,clock_timestamp()+interval '1 hour')")
        .bind(user).bind(profile).bind(fingerprint(token.as_bytes())).execute(pool).await?;
    Ok(token)
}
