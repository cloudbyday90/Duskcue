use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use duskcue::config::BootstrapConfig;
use duskcue::domains::live_tv::{CONNECTION_BODY_LIMIT, VALIDATE_CONNECTION_PATH};
use duskcue::services::encryption::EncryptionKey;
use duskcue::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

async fn seed_actor(
    pool: &PgPool,
    role: &str,
    profile_type: &str,
) -> anyhow::Result<(Uuid, String)> {
    let user = Uuid::now_v7();
    let profile = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, username, display_name, role) VALUES ($1, $2, 'Live TV fixture', $3)")
        .bind(user)
        .bind(format!("live-tv-{user}"))
        .bind(role)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO user_profiles (id, owner_user_id, name, profile_type, is_default) VALUES ($1, $2, 'Fixture', $3, true)")
        .bind(profile)
        .bind(user)
        .bind(profile_type)
        .execute(pool)
        .await?;
    let token = duskcue::domains::auth::service::generate_session_token();
    let hash = ring::digest::digest(&ring::digest::SHA256, token.as_bytes())
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    sqlx::query("INSERT INTO user_sessions (user_id, active_profile_id, token_hash, expires_at) VALUES ($1, $2, $3, now() + interval '1 hour')")
        .bind(user)
        .bind(profile)
        .bind(hash)
        .execute(pool)
        .await?;
    Ok((user, token))
}

async fn post(
    app: &Router,
    token: Option<(&str, bool)>,
    body: &str,
) -> anyhow::Result<(StatusCode, Value)> {
    let mut request = Request::builder()
        .method("POST")
        .uri(VALIDATE_CONNECTION_PATH)
        .header("content-type", "application/json");
    if let Some((token, cookie)) = token {
        request = if cookie {
            request.header("cookie", format!("session={token}"))
        } else {
            request.header("authorization", format!("Bearer {token}"))
        };
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string()))?)
        .await?;
    assert_eq!(response.headers()["cache-control"], "no-store");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 32768).await?;
    Ok((status, serde_json::from_slice(&bytes)?))
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL 18 and DUSKCUE_LIVE_TV_TESTS=disposable"]
async fn live_tv_connection_validation_authorization_contract() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("DUSKCUE_LIVE_TV_TESTS").as_deref() == Ok("disposable"),
        "requires disposable fixture opt-in"
    );
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("DUSKCUE_DATABASE_URL")?)
        .await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(
        database.starts_with("duskcue_live_tv_test"),
        "requires an isolated Live TV test database"
    );
    sqlx::migrate!("./migrations").run(&pool).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS audit_log_live_tv_test PARTITION OF audit_log DEFAULT")
        .execute(&pool)
        .await?;

    let (owner, owner_token) = seed_actor(&pool, "owner", "standard").await?;
    let (_, admin_token) = seed_actor(&pool, "admin", "standard").await?;
    let (member, member_token) = seed_actor(&pool, "member", "standard").await?;
    let (_, kids_token) = seed_actor(&pool, "owner", "kids").await?;
    let bootstrap = BootstrapConfig {
        database_url: None,
        bind_address: "127.0.0.1".into(),
        port: 0,
        data_dir: std::env::temp_dir().join(format!("duskcue-live-tv-fixture-{owner}")),
        cache_dir: std::env::temp_dir().join(format!("duskcue-live-tv-cache-{owner}")),
        log_level: "error".into(),
        environment: "test".into(),
        encryption_key: None,
        geoip_license_key: None,
    };
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let (key, _) = EncryptionKey::generate();
    let state = AppState::new(pool.clone(), bootstrap, recorder.handle(), key);
    let app = duskcue::router::build_router(state.clone()).with_state(state);
    let valid = json!({
        "playlist_url": "http://dispatcharr.invalid/private-path?token=private-secret",
        "guide_url": "https://guide.invalid/private-guide?key=private-secret"
    })
    .to_string();
    assert_eq!(
        post(&app, None, "malformed-private-secret").await?.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post(
            &app,
            Some((&member_token, false)),
            "malformed-private-secret"
        )
        .await?
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(&app, Some((&kids_token, false)), &valid).await?.0,
        StatusCode::FORBIDDEN
    );

    for token in [&owner_token, &admin_token] {
        for cookie in [false, true] {
            let (status, response) = post(&app, Some((token, cookie)), &valid).await?;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(response["network_checked"], false);
            assert_eq!(response["saved"], false);
            assert!(!response.to_string().contains("private"));
        }
    }

    sqlx::query("INSERT INTO user_capabilities (user_id, capability, is_granted) VALUES ($1, 'can_manage_users', true)")
        .bind(member)
        .execute(&pool)
        .await?;
    assert_eq!(
        post(&app, Some((&member_token, false)), &valid).await?.0,
        StatusCode::OK
    );
    sqlx::query("UPDATE user_capabilities SET is_granted = false WHERE user_id = $1 AND capability = 'can_manage_users'")
        .bind(member)
        .execute(&pool)
        .await?;
    assert_eq!(
        post(&app, Some((&member_token, false)), &valid).await?.0,
        StatusCode::FORBIDDEN
    );

    for (body, expected) in [
        ("{\"private-secret", StatusCode::BAD_REQUEST),
        (
            "{\"private-secret\":true}",
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "{\"playlist_url\":\"file:///private-secret\",\"guide_url\":\"file:///private-secret\"}",
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        let (status, response) = post(&app, Some((&admin_token, false)), body).await?;
        assert_eq!(status, expected);
        assert!(!response.to_string().contains("private-secret"));
    }
    let large = format!(
        "{{\"private-secret\":\"{}\"}}",
        "x".repeat(CONNECTION_BODY_LIMIT)
    );
    assert_eq!(
        post(&app, Some((&admin_token, false)), &large).await?.0,
        StatusCode::PAYLOAD_TOO_LARGE
    );

    sqlx::query(
        "UPDATE user_sessions SET expires_at = now() - interval '1 second' WHERE user_id = $1",
    )
    .bind(owner)
    .execute(&pool)
    .await?;
    assert_eq!(
        post(&app, Some((&owner_token, false)), &valid).await?.0,
        StatusCode::UNAUTHORIZED
    );
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM play_sessions")
        .fetch_one(&pool)
        .await?;
    assert_eq!(sessions, 0);
    pool.close().await;
    Ok(())
}
