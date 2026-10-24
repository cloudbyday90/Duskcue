// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::{
    Extension, Router,
    body::{Body, to_bytes},
    extract::Path,
    http::{Request, Response, StatusCode},
    routing::get as route_get,
};
use duskcue::{
    domains::live_tv::transport::Output,
    extractors::{CanManageUsers, Require},
    state::AppState,
};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

pub struct Actor {
    pub user: Uuid,
    pub profile: Uuid,
    pub token: String,
}

pub async fn actor(pool: &PgPool, role: &str, kind: &str) -> anyhow::Result<Actor> {
    let user = Uuid::now_v7();
    let profile = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, username, display_name, role) VALUES ($1,$2,'Transport fixture',$3)")
        .bind(user).bind(format!("transport-{user}")).bind(role).execute(pool).await?;
    sqlx::query("INSERT INTO user_profiles (id,owner_user_id,name,profile_type,is_default) VALUES ($1,$2,'Fixture',$3,true)")
        .bind(profile).bind(user).bind(kind).execute(pool).await?;
    let token = duskcue::domains::auth::service::generate_session_token();
    let hash = ring::digest::digest(&ring::digest::SHA256, token.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    sqlx::query("INSERT INTO user_sessions (user_id,active_profile_id,token_hash,expires_at) VALUES ($1,$2,$3,now()+interval '1 hour')")
        .bind(user).bind(profile).bind(hash).execute(pool).await?;
    Ok(Actor {
        user,
        profile,
        token,
    })
}

#[derive(Clone)]
struct Boundary {
    user: Uuid,
    profile: Uuid,
    output: Output,
}

pub fn router(state: AppState, actor: &Actor, output: Output) -> Router {
    Router::new()
        .route("/qualification/live/{name}", route_get(asset))
        .layer(Extension(Boundary {
            user: actor.user,
            profile: actor.profile,
            output,
        }))
        .with_state(state)
}

async fn asset(
    grant: Require<CanManageUsers>,
    Extension(boundary): Extension<Boundary>,
    Path(name): Path<String>,
) -> Response<Body> {
    if grant.user.user_id != boundary.user || grant.user.profile_id != boundary.profile {
        return Response::builder().status(403).body(Body::empty()).unwrap();
    }
    match boundary.output.read(&name).await {
        Ok(bytes) => Response::builder()
            .header("cache-control", "no-store")
            .header(
                "content-type",
                if name == "manifest.m3u8" {
                    "application/vnd.apple.mpegurl"
                } else {
                    "video/mp4"
                },
            )
            .body(Body::from(bytes))
            .unwrap(),
        Err(_) => Response::builder()
            .status(404)
            .header("cache-control", "no-store")
            .body(Body::empty())
            .unwrap(),
    }
}

pub async fn get(
    app: &Router,
    token: Option<(&str, bool)>,
    name: &str,
) -> anyhow::Result<(StatusCode, Vec<u8>)> {
    let mut request = Request::builder().uri(format!("/qualification/live/{name}"));
    if let Some((token, cookie)) = token {
        request = if cookie {
            request.header("cookie", format!("session={token}"))
        } else {
            request.header("authorization", format!("Bearer {token}"))
        };
    }
    let response = app.clone().oneshot(request.body(Body::empty())?).await?;
    let status = response.status();
    if status.is_success() {
        anyhow::ensure!(
            response.headers()["cache-control"] == "no-store",
            "output cache policy differs"
        );
    }
    Ok((
        status,
        to_bytes(response.into_body(), 4 * 1024 * 1024 + 1)
            .await?
            .to_vec(),
    ))
}
