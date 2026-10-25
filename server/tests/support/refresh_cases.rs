// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    http::{self, Case},
    import_fixture as fixture,
    peer::{self, Behavior, Peer, Security},
};
use axum::http::StatusCode;
use duskcue::domains::live_tv::refresh::RefreshManager;
use serde_json::{Value, json};
use std::time::Duration;
use uuid::Uuid;

pub const CONTROL: &str = "/api/v1/live-tv/manual-refresh-control";
pub fn path(case: &Case) -> String {
    format!("/api/v1/live-tv/households/{}/refresh", case.owner.user)
}
pub async fn status(case: &Case) -> anyhow::Result<Value> {
    Ok(http::call(
        &case.app,
        Some(&case.admin),
        "GET",
        &format!("{}/status", path(case)),
        None,
        &[],
    )
    .await?
    .expect(StatusCode::OK)?
    .value)
}
pub async fn enable(case: &Case, enabled: bool) -> anyhow::Result<()> {
    let tag = http::call(&case.app, Some(&case.admin), "GET", CONTROL, None, &[])
        .await?
        .expect(StatusCode::OK)?
        .etag()?;
    http::call(
        &case.app,
        Some(&case.admin),
        "PUT",
        CONTROL,
        Some(json!({"enabled":enabled})),
        &[("if-match", &tag)],
    )
    .await?
    .expect(StatusCode::OK)?;
    Ok(())
}
pub async fn start(case: &Case) -> anyhow::Result<http::Reply> {
    let tag = http::call(&case.app, Some(&case.admin), "GET", &path(case), None, &[])
        .await?
        .expect(StatusCode::OK)?
        .etag()?;
    http::call(
        &case.app,
        Some(&case.admin),
        "POST",
        &path(case),
        Some(json!({"playlist_pin":0,"guide_pin":0})),
        &[("if-match", &tag)],
    )
    .await
}
pub async fn wait_idle(owner: &RefreshManager) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(8), async {
        while !owner.idle() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    Ok(())
}

pub async fn run(case: &mut Case, url: &str) -> anyhow::Result<()> {
    let owner = RefreshManager::qualification_fixture(&case.state)?;
    let control = http::call(&case.app, Some(&case.admin), "GET", CONTROL, None, &[])
        .await?
        .expect(StatusCode::OK)?;
    anyhow::ensure!(
        control.value["enabled"] == false,
        "manual refresh was not default-off"
    );
    http::call(
        &case.app,
        Some(&case.admin),
        "PUT",
        "/api/v1/server/config/integrations",
        Some(json!({"value":{"live_tv":{"manual_refresh_enabled":true},"manual_refresh":{"enabled":true}}})),
        &[],
    )
    .await?
    .expect(StatusCode::OK)?;
    anyhow::ensure!(
        http::call(&case.app, Some(&case.admin), "GET", CONTROL, None, &[])
            .await?
            .value["enabled"]
            == false,
        "generic settings activated manual refresh"
    );
    let initial = status(case).await?;
    anyhow::ensure!(
        initial["latest"].is_null() && initial["observation"] == "never_imported",
        "initial status was not explicit"
    );
    let a = Peer::new(
        "127.0.0.1".parse()?,
        Security::Plain,
        Behavior::HeadersStall,
    )
    .await?;
    let b = Peer::new(
        "127.0.0.1".parse()?,
        Security::Tls,
        Behavior::Reply(peer::response(&fixture::guide())),
    )
    .await?;
    fixture::prepare(case, &a, &b).await?;
    start(case).await?.expect(StatusCode::SERVICE_UNAVAILABLE)?;
    anyhow::ensure!(
        a.hits() == 0 && b.hits() == 0,
        "disabled operation made network request"
    );
    enable(case, true).await?;
    let tag = http::call(&case.app, Some(&case.admin), "GET", &path(case), None, &[])
        .await?
        .etag()?;
    http::call(
        &case.app,
        Some(&case.admin),
        "POST",
        &path(case),
        Some(json!({"playlist_pin":0,"guide_pin":0})),
        &[],
    )
    .await?
    .expect(StatusCode::PRECONDITION_REQUIRED)?;
    let viewing = http::Actor {
        user: case.admin.user,
        profile: case.admin.profile,
        token: http::session(&case.pool, case.admin.user, case.admin.profile).await?,
        cookie: true,
    };
    case.secrets.push(viewing.token.clone());
    let control_tag = http::call(&case.app, Some(&viewing), "GET", CONTROL, None, &[])
        .await?
        .etag()?;
    http::call(
        &case.app,
        Some(&viewing),
        "PUT",
        CONTROL,
        Some(json!({"enabled":false})),
        &[("if-match", &control_tag)],
    )
    .await?
    .expect(StatusCode::FORBIDDEN)?;
    http::call(
        &case.app,
        Some(&viewing),
        "POST",
        &path(case),
        Some(json!({"playlist_pin":0,"guide_pin":0})),
        &[("if-match", &tag)],
    )
    .await?
    .expect(StatusCode::FORBIDDEN)?;
    http::call(&case.app, Some(&case.other), "GET", &path(case), None, &[])
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    let accepted = http::call(
        &case.app,
        Some(&case.admin),
        "POST",
        &path(case),
        Some(json!({"playlist_pin":0,"guide_pin":0})),
        &[("if-match", &tag)],
    )
    .await?
    .expect(StatusCode::ACCEPTED)?;
    let id = Uuid::parse_str(
        accepted.value["attempt_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("accepted identity missing"))?,
    )?;
    anyhow::ensure!(
        id.get_version_num() == 7
            && accepted
                .headers
                .get("location")
                .is_some_and(|v| Some(v.to_str().unwrap_or_default())
                    == accepted.value["status_url"].as_str()),
        "accepted resource identity/location incorrect"
    );
    a.wait_phase(3).await?;
    anyhow::ensure!(owner.owns(id), "accepted work was not root-owned");
    start(case).await?.expect(StatusCode::SERVICE_UNAVAILABLE)?;
    sqlx::query("UPDATE household_owner_grants SET expires_at=clock_timestamp()-interval '1 second' WHERE user_id=$1").bind(case.admin.user).execute(&case.pool).await?;
    tokio::time::sleep(Duration::from_millis(350)).await;
    anyhow::ensure!(
        owner.owns(id),
        "ordinary management proof expiry cancelled accepted work"
    );
    sqlx::query("UPDATE user_sessions SET active_profile_id=NULL,profile_selection_required=true WHERE token_hash=$1").bind(duskcue::domains::household_security::operator::fingerprint(case.admin.token.as_bytes())).execute(&case.pool).await?;
    wait_idle(&owner).await?;
    a.finish(true).await?;
    b.finish(false).await?;
    sqlx::query("UPDATE user_sessions SET active_profile_id=$2,profile_selection_required=false WHERE token_hash=$1").bind(duskcue::domains::household_security::operator::fingerprint(case.admin.token.as_bytes())).bind(case.admin.profile).execute(&case.pool).await?;
    case.admin_proof(false).await?;
    http::call(
        &case.app,
        Some(&case.admin),
        "POST",
        &path(case),
        Some(json!({"playlist_pin":0,"guide_pin":0})),
        &[("if-match", &tag)],
    )
    .await?
    .expect(StatusCode::PRECONDITION_FAILED)?;
    for input in [
        json!({"playlist_pin":8,"guide_pin":0}),
        json!({"playlist_pin":0,"guide_pin":0,"url":"http://private"}),
    ] {
        let tag = http::call(&case.app, Some(&case.admin), "GET", &path(case), None, &[])
            .await?
            .etag()?;
        http::call(
            &case.app,
            Some(&case.admin),
            "POST",
            &path(case),
            Some(input),
            &[("if-match", &tag)],
        )
        .await?
        .expect(StatusCode::BAD_REQUEST)?;
    }
    let (a, b) = fixture::peers(&fixture::playlist(), &fixture::guide()).await?;
    fixture::prepare(case, &a, &b).await?;
    let completed = start(case).await?.expect(StatusCode::ACCEPTED)?;
    wait_idle(&owner).await?;
    let current = status(case).await?;
    anyhow::ensure!(
        current["latest"]["state"] == "succeeded"
            && current["latest"]["channel_count"] == 2
            && current["latest"]["current_source"] == true,
        "accepted refresh did not publish paired data"
    );
    let bytes = current.to_string();
    anyhow::ensure!(
        !bytes.contains("private")
            && !bytes.contains("duskcue-secret")
            && !bytes.contains("duskcue-feed.invalid"),
        "status leaked input"
    );
    a.finish(true).await?;
    b.finish(true).await?;
    let observed = http::call(
        &case.app,
        Some(&case.admin),
        "GET",
        completed.value["status_url"].as_str().unwrap(),
        None,
        &[],
    )
    .await?
    .expect(StatusCode::OK)?;
    anyhow::ensure!(
        observed.value["observation"] == "current" && observed.headers.get("etag").is_none(),
        "status reused a control validator"
    );
    let a = Peer::new("127.0.0.1".parse()?, Security::Plain, Behavior::BodyStall).await?;
    let (_, b) = fixture::peers(&fixture::playlist(), &fixture::guide()).await?;
    fixture::prepare(case, &a, &b).await?;
    start(case).await?.expect(StatusCode::ACCEPTED)?;
    a.wait_phase(3).await?;
    enable(case, false).await?;
    wait_idle(&owner).await?;
    let stopped = status(case).await?;
    anyhow::ensure!(
        stopped["latest"]["snapshot_id"] == current["latest"]["snapshot_id"]
            && stopped["latest"]["state"] == "failed",
        "disable lost last-good or completed revoked work"
    );
    a.finish(true).await?;
    b.finish(false).await?;
    super::admission_cases::run(case, url).await?;
    super::publication_case::run(case, url).await?;
    owner.shutdown().await?;
    println!("IPTV_REFRESH_STAGE protected_admission_and_revocation_passed");
    Ok(())
}
