// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    database,
    http::{self, Case},
    peer::Peer,
};
use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

pub async fn setup(db: &database::Database) -> anyhow::Result<Case> {
    setup_state(db, db.state()).await
}

pub async fn setup_state(
    db: &database::Database,
    state: duskcue::state::AppState,
) -> anyhow::Result<Case> {
    let app = duskcue::router::build_router(state.clone()).with_state(state.clone());
    let admin = http::actor(&db.pool, "standard", true).await?;
    sqlx::query("UPDATE users SET role='admin' WHERE id=$1")
        .bind(admin.user)
        .execute(&db.pool)
        .await?;
    let owner = http::actor(&db.pool, "standard", true).await?;
    let other = http::actor(&db.pool, "standard", false).await?;
    let standard = owner.profile;
    let kids = Uuid::now_v7();
    sqlx::query("INSERT INTO user_profiles(id,owner_user_id,name,profile_type) VALUES ($1,$2,'Child','kids')").bind(kids).bind(owner.user).execute(&db.pool).await?;
    let secrets = vec![
        db.password.clone(),
        http::PASSWORD.into(),
        admin.token.clone(),
        owner.token.clone(),
        other.token.clone(),
        "feed-query-private".into(),
        "feed-response-private".into(),
        "feed-body-private".into(),
        "duskcue-feed.invalid".into(),
    ];
    let mut case = Case {
        app,
        state,
        pool: db.pool.clone(),
        admin,
        owner,
        other,
        standard,
        kids,
        secrets,
    };
    case.admin_proof(true).await?;
    http::call(&case.app,Some(&case.admin),"PUT",&case.admin_path(),Some(json!({"playlist_url":http::PLAYLIST,"guide_url":http::GUIDE,"enabled":true,"allow_owner_profile_access":true,"allow_owner_live_limit":true})),&[("if-none-match","*")]).await?.expect(StatusCode::CREATED)?;
    Ok(case)
}

pub async fn patch(case: &Case, body: Value) -> anyhow::Result<http::Reply> {
    let etag = case.admin_read().await?.etag()?;
    http::call(
        &case.app,
        Some(&case.admin),
        "PATCH",
        &case.admin_path(),
        Some(body),
        &[("if-match", &etag)],
    )
    .await
}

pub async fn prepare(
    case: &mut Case,
    peer: &Peer,
    scheme: &str,
    host: &str,
) -> anyhow::Result<Value> {
    let origin = peer.origin(scheme, host);
    let url = format!("{origin}/private-feed?credential=feed-query-private");
    case.secrets.extend([origin.clone(), url.clone()]);
    patch(
        case,
        json!({"operation":"rotate_credentials","playlist_url":url,"guide_url":url}),
    )
    .await?
    .expect(StatusCode::OK)?;
    let reply = patch(case,json!({"operation":"approve_destination","feed":"playlist","expected_origin":origin,"addresses":[peer.address.ip().to_string()]})).await?.expect(StatusCode::OK)?;
    anyhow::ensure!(
        reply.value["playlist_destination_approved"] == true
            && reply.value["playlist_address_count"] == 1
            && reply.value["network_checked"] == false
            && reply.value["playback_available"] == false,
        "approval metadata was incorrect"
    );
    let encoded = reply.value.to_string();
    anyhow::ensure!(
        !encoded.contains(host)
            && !encoded.contains(&peer.address.ip().to_string())
            && !encoded.contains("feed-query-private")
            && !encoded.contains("duskcue-secret"),
        "approval metadata disclosed authority"
    );
    Ok(reply.value)
}
