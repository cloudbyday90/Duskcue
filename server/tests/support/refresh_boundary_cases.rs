// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    http::{self, Case},
    import_fixture as fixture, refresh_cases,
};
use axum::http::StatusCode;
use duskcue::{domains::live_tv::refresh::RefreshManager, state::NetworkMode};
use serde_json::json;
use std::sync::{Arc, OnceLock};

pub async fn run(case: &mut Case) -> anyhow::Result<()> {
    case.admin_proof(false).await?;
    refresh_cases::enable(case, true).await?;
    let (a, b) = fixture::peers(&fixture::playlist(), &fixture::guide()).await?;
    fixture::prepare(case, &a, &b).await?;
    let mut state = case.state.clone();
    state.live_tv_refresh = Arc::new(OnceLock::new());
    let owner = RefreshManager::attach(&state)?;
    let original = state.runtime_config.load_full();
    let mut config = (*original).clone();
    config.auth.network_mode = NetworkMode::Exposed;
    config.security.allowed_origins = vec!["https://admin.duskcue.invalid".into()];
    state.runtime_config.store(Arc::new(config));
    let app = duskcue::router::build_router(state.clone()).with_state(state.clone());
    state.runtime_config.store(original);
    let path = refresh_cases::path(case);
    let tag = http::call(&app, Some(&case.admin), "GET", &path, None, &[])
        .await?
        .etag()?;
    for (origin, allowed) in [
        ("https://admin.duskcue.invalid", true),
        ("https://unapproved.invalid", false),
    ] {
        let reply = http::call(
            &app,
            Some(&case.admin),
            "OPTIONS",
            &path,
            None,
            &[
                ("origin", origin),
                ("access-control-request-method", "POST"),
                ("access-control-request-headers", "content-type,if-match"),
            ],
        )
        .await?;
        anyhow::ensure!(
            reply
                .headers
                .get("access-control-allow-origin")
                .is_some_and(|v| v == origin)
                == allowed,
            "refresh CORS widened allowed origins"
        );
        if allowed {
            anyhow::ensure!(
                reply
                    .headers
                    .get("access-control-allow-credentials")
                    .is_some_and(|v| v == "true"),
                "explicit-origin credentials contract absent"
            );
        }
    }
    let accepted = http::call(
        &app,
        Some(&case.admin),
        "POST",
        &path,
        Some(json!({"playlist_pin":0,"guide_pin":0})),
        &[
            ("if-match", &tag),
            ("origin", "https://admin.duskcue.invalid"),
        ],
    )
    .await?
    .expect(StatusCode::ACCEPTED)?;
    let exposed = accepted
        .headers
        .get("access-control-expose-headers")
        .unwrap()
        .to_str()?;
    anyhow::ensure!(
        exposed.contains("location") && exposed.contains("etag") && exposed.contains("retry-after"),
        "refresh browser response metadata was not exposed"
    );
    refresh_cases::wait_idle(&owner).await?;
    let status = fixture::status(case).await?;
    anyhow::ensure!(
        status.state == "failed" && a.hits() == 1 && b.hits() == 1,
        "normal public-root owner accepted qualification-only TLS trust"
    );
    owner.shutdown().await?;
    a.finish(true).await?;
    b.finish(true).await?;
    println!("IPTV_REFRESH_STAGE cors_and_normal_tls_trust_boundary_passed");
    Ok(())
}
