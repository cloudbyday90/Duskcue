// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::http::{self, Case};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use uuid::Uuid;

pub async fn run(case: &mut Case) -> anyhow::Result<()> {
    let path = case.admin_path();
    let create = json!({"playlist_url":http::PLAYLIST,"guide_url":http::GUIDE});
    http::call(
        &case.app,
        None,
        "PUT",
        &path,
        Some(create.clone()),
        &[("if-none-match", "*")],
    )
    .await?
    .expect(StatusCode::UNAUTHORIZED)?;
    http::call(
        &case.app,
        Some(&case.other),
        "PUT",
        &path,
        Some(create.clone()),
        &[("if-none-match", "*")],
    )
    .await?
    .expect(StatusCode::FORBIDDEN)?;
    http::call(
        &case.app,
        Some(&case.admin),
        "PUT",
        &path,
        Some(create.clone()),
        &[("if-none-match", "*")],
    )
    .await?
    .expect(StatusCode::FORBIDDEN)?;
    sqlx::query("UPDATE users SET role='owner' WHERE id=$1")
        .bind(case.admin.user)
        .execute(&case.pool)
        .await?;
    http::call(
        &case.app,
        Some(&case.admin),
        "PUT",
        &path,
        Some(create.clone()),
        &[("if-none-match", "*")],
    )
    .await?
    .expect(StatusCode::FORBIDDEN)?;
    sqlx::query("UPDATE users SET role='admin' WHERE id=$1")
        .bind(case.admin.user)
        .execute(&case.pool)
        .await?;
    case.admin_proof(true).await?;
    http::call(
        &case.app,
        Some(&case.admin),
        "PUT",
        &path,
        Some(create.clone()),
        &[],
    )
    .await?
    .expect(StatusCode::PRECONDITION_REQUIRED)?;
    let created = http::call(
        &case.app,
        Some(&case.admin),
        "PUT",
        &path,
        Some(create.clone()),
        &[("if-none-match", "*")],
    )
    .await?
    .expect(StatusCode::CREATED)?;
    anyhow::ensure!(
        created.value["enabled"] == false
            && created.value["allow_owner_profile_access"] == false
            && created.value["allow_owner_live_limit"] == false
            && created.value["max_live_streams"].is_null()
            && created.value["owner_max_live_streams"].is_null(),
        "assignment defaults were permissive"
    );
    anyhow::ensure!(
        created.value["network_checked"] == false && created.value["playback_available"] == false,
        "saved assignment claimed runtime qualification"
    );
    let etag = created.etag()?;
    http::call(
        &case.app,
        Some(&case.admin),
        "PUT",
        &path,
        Some(create),
        &[("if-none-match", "*")],
    )
    .await?
    .expect(StatusCode::PRECONDITION_FAILED)?;
    http::call(
        &case.app,
        Some(&case.admin),
        "GET",
        &path,
        None,
        &[("if-none-match", &etag)],
    )
    .await?
    .expect(StatusCode::NOT_MODIFIED)?;
    let configure = json!({"operation":"configure","enabled":true,"max_live_streams":3,"allow_owner_profile_access":true,"allow_owner_live_limit":true});
    for condition in ["W/\"weak\"", "\"stale\""] {
        http::call(
            &case.app,
            Some(&case.admin),
            "PATCH",
            &path,
            Some(configure.clone()),
            &[("if-match", condition)],
        )
        .await?
        .expect(StatusCode::PRECONDITION_FAILED)?;
    }
    http::call(
        &case.app,
        Some(&case.admin),
        "PATCH",
        &path,
        Some(configure.clone()),
        &[("if-match", "*")],
    )
    .await?
    .expect(StatusCode::PRECONDITION_REQUIRED)?;
    for condition in ["unquoted", "*,\"version\""] {
        http::call(
            &case.app,
            Some(&case.admin),
            "PATCH",
            &path,
            Some(configure.clone()),
            &[("if-match", condition)],
        )
        .await?
        .expect(StatusCode::BAD_REQUEST)?;
    }
    let missing = json!({"operation":"configure","enabled":true,"allow_owner_profile_access":true,"allow_owner_live_limit":true});
    http::call(
        &case.app,
        Some(&case.admin),
        "PATCH",
        &path,
        Some(missing),
        &[("if-match", &etag)],
    )
    .await?
    .expect(StatusCode::BAD_REQUEST)?;
    let mut invalid = configure.clone();
    invalid["extra"] = json!(http::PLAYLIST);
    http::call(
        &case.app,
        Some(&case.admin),
        "PATCH",
        &path,
        Some(invalid),
        &[("if-match", &etag)],
    )
    .await?
    .expect(StatusCode::BAD_REQUEST)?;
    for cap in [0, -1] {
        let mut invalid = configure.clone();
        invalid["max_live_streams"] = json!(cap);
        http::call(
            &case.app,
            Some(&case.admin),
            "PATCH",
            &path,
            Some(invalid),
            &[("if-match", &etag)],
        )
        .await?
        .expect(StatusCode::UNPROCESSABLE_ENTITY)?;
    }
    let bad_url = json!({"operation":"rotate_credentials","playlist_url":"file:///private","guide_url":http::GUIDE});
    let invalid = http::call(
        &case.app,
        Some(&case.admin),
        "PATCH",
        &path,
        Some(bad_url),
        &[("if-match", &etag)],
    )
    .await?
    .expect(StatusCode::UNPROCESSABLE_ENTITY)?;
    anyhow::ensure!(
        invalid.value["type"]
            .as_str()
            .is_some_and(|s| s == "/errors/valid_001"),
        "URL validation contract changed"
    );
    anyhow::ensure!(
        invalid.value.get("instance").is_none(),
        "assignment validation named an unrelated endpoint"
    );
    http::call(
        &case.app,
        Some(&case.admin),
        "PATCH",
        &path,
        Some(configure.clone()),
        &[("if-match", &etag), ("content-type", "text/plain")],
    )
    .await?
    .expect(StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
    let oversized = json!({"operation":"rotate_credentials","playlist_url":"x".repeat(17000),"guide_url":http::GUIDE});
    http::call(
        &case.app,
        Some(&case.admin),
        "PATCH",
        &path,
        Some(oversized),
        &[("if-match", &etag)],
    )
    .await?
    .expect(StatusCode::PAYLOAD_TOO_LARGE)?;
    let request = Request::builder()
        .method("PATCH")
        .uri(&path)
        .header("authorization", format!("Bearer {}", case.admin.token))
        .header("content-type", "application/json")
        .header("if-match", &etag)
        .body(Body::from("{"))?;
    http::raw(&case.app, request)
        .await?
        .expect(StatusCode::BAD_REQUEST)?;
    let list = format!("W/{etag}, \"unknown,tag\", {etag}");
    let changed = http::call(
        &case.app,
        Some(&case.admin),
        "PATCH",
        &path,
        Some(configure.clone()),
        &[("if-match", &list)],
    )
    .await?
    .expect(StatusCode::OK)?;
    anyhow::ensure!(changed.etag()? != etag, "policy change retained stale ETag");
    http::call(
        &case.app,
        Some(&case.admin),
        "PATCH",
        &path,
        Some(configure),
        &[("if-match", &etag)],
    )
    .await?
    .expect(StatusCode::PRECONDITION_FAILED)?;
    http::call(
        &case.app,
        Some(&case.admin),
        "GET",
        &format!("/api/v1/live-tv/households/{}/assignment", Uuid::now_v7()),
        None,
        &[],
    )
    .await?
    .expect(StatusCode::NOT_FOUND)?;
    let malformed = "/api/v1/live-tv/households/private-fixture-path/assignment";
    let bad = http::call(&case.app, Some(&case.admin), "GET", malformed, None, &[])
        .await?
        .expect(StatusCode::BAD_REQUEST)?;
    anyhow::ensure!(
        !bad.value.to_string().contains("private-fixture-path"),
        "path rejection reflected input"
    );
    let cors = http::call(
        &case.app,
        Some(&case.admin),
        "GET",
        &path,
        None,
        &[("origin", "http://localhost:48027")],
    )
    .await?
    .expect(StatusCode::OK)?;
    anyhow::ensure!(
        cors.headers
            .get("access-control-expose-headers")
            .is_some_and(|v| v
                .to_str()
                .is_ok_and(|v| v.split(',').any(|v| v.trim().eq_ignore_ascii_case("etag")))),
        "ETag was not exposed through CORS"
    );
    http::call(&case.app, Some(&case.other), "GET", http::OWNER, None, &[])
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    let current = case.admin_read().await?.etag()?;
    let oversized_condition = format!("\"{}\"", "x".repeat(1023));
    http::call(
        &case.app,
        Some(&case.admin),
        "DELETE",
        &path,
        None,
        &[("if-match", &oversized_condition)],
    )
    .await?
    .expect(StatusCode::BAD_REQUEST)?;
    let native = http::call(&case.app, Some(&case.admin), "POST", &path, None, &[])
        .await?
        .expect(StatusCode::METHOD_NOT_ALLOWED)?;
    anyhow::ensure!(
        native
            .headers
            .get("content-type")
            .is_none_or(|v| v != "application/problem+json"),
        "native method error was mislabeled JSON"
    );
    let expired = http::Actor {
        user: case.admin.user,
        profile: case.admin.profile,
        token: http::session(&case.pool, case.admin.user, case.admin.profile).await?,
        cookie: false,
    };
    case.secrets.push(expired.token.clone());
    sqlx::query("UPDATE user_sessions SET expires_at=clock_timestamp()-interval '1 second' WHERE token_hash=$1").bind(duskcue::domains::household_security::operator::fingerprint(expired.token.as_bytes())).execute(&case.pool).await?;
    http::call(&case.app, Some(&expired), "GET", &path, None, &[])
        .await?
        .expect(StatusCode::UNAUTHORIZED)?;
    anyhow::ensure!(
        case.admin_read().await?.etag()? == current,
        "request refusals mutated configuration"
    );
    Ok(())
}
