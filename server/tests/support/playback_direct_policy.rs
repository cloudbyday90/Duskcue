// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{AppState, ProgressiveFixture};

pub(super) const ORIGIN: &str = "http://127.0.0.1:38081";

pub(super) fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

pub(super) async fn canonical_user(fixture: &ProgressiveFixture<'_>) -> anyhow::Result<Value> {
    let user =
        duskcue::domains::auth::service::validate_session(fixture.pool, fixture.token).await?;
    anyhow::ensure!(
        user.user_id == fixture.user
            && user.active_profile_id == fixture.original_profile
            && !user.session.profile_selection_required,
        "live fixture session lost its canonical user/profile scope"
    );
    let preferences =
        duskcue::domains::users::service::get_user_preferences(fixture.pool, user.user_id).await?;
    Ok(json!({
        "id":user.user_id,"username":user.username,"display_name":user.display_name,"role":user.role,
        "capabilities":user.capabilities,"has_all_library_access":user.has_all_library_access,
        "active_profile_id":user.active_profile_id,"profile_selection_required":user.session.profile_selection_required,
        "locale":preferences.locale
    }))
}

pub(super) async fn assert_policy(
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    candidate: &Value,
) -> anyhow::Result<Value> {
    let client = client()?;
    let health = client.get(format!("{ORIGIN}/health/ready")).send().await?;
    anyhow::ensure!(health.status() == StatusCode::OK);
    let headers = health.headers();
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| anyhow::anyhow!("production request ID unavailable"))?;
    Uuid::parse_str(request_id)?;
    anyhow::ensure!(
        headers
            .get("x-content-type-options")
            .is_some_and(|value| value == "nosniff")
    );
    let csp = headers
        .get("content-security-policy")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| anyhow::anyhow!("production CSP unavailable"))?
        .to_owned();
    anyhow::ensure!(csp.contains("default-src"));
    let file = candidate["file"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("live policy file unavailable"))?;
    let unauthenticated = client
        .get(format!("{ORIGIN}/api/v1/stream/{file}"))
        .send()
        .await?;
    anyhow::ensure!(unauthenticated.status() == StatusCode::UNAUTHORIZED);
    let start_url = format!("{ORIGIN}/api/v1/playback/start");
    let no_auth_start = client.post(&start_url).json(&json!({})).send().await?;
    anyhow::ensure!(no_auth_start.status() == StatusCode::UNAUTHORIZED);
    let wrong_type = client
        .post(&start_url)
        .header("cookie", format!("session={}", fixture.token))
        .header("content-type", "text/plain")
        .body("{}")
        .send()
        .await?;
    anyhow::ensure!(wrong_type.status() == StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let preflight = client
        .request(reqwest::Method::OPTIONS, &start_url)
        .header("origin", "https://unrelated.invalid")
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "content-type")
        .send()
        .await?;
    anyhow::ensure!(
        preflight.status().is_success()
            && preflight
                .headers()
                .get("access-control-allow-origin")
                .is_some_and(|value| value == "*")
            && !preflight
                .headers()
                .contains_key("access-control-allow-credentials"),
        "local production CORS unexpectedly admitted credentialed cross-origin access"
    );
    anyhow::ensure!(state.transcode_manager.active_session_count() == 0);
    Ok(json!({
        "production_router":true,"request_id":true,"nosniff":true,"content_security_policy":csp,
        "unauthenticated_media_status":unauthenticated.status().as_u16(),
        "unauthenticated_start_status":no_auth_start.status().as_u16(),
        "non_json_start_status":wrong_type.status().as_u16(),
        "credentialed_cross_origin_allowed":false,
        "cookie_policy":"HttpOnly; SameSite=Strict; JSON mutation payloads; no custom CSRF-token or Origin-rejection middleware"
    }))
}
