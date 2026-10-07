use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use duskcue::config::BootstrapConfig;
use duskcue::domains::profiles::types::{
    CreateProfileRequest, SubtitleMode, UpdateViewingPreferencesRequest, ViewingPreferencesRequest,
};
use duskcue::domains::profiles::{self, ProfilesError, service};
use duskcue::services::encryption::EncryptionKey;
use duskcue::state::AppState;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use tower::ServiceExt;
use uuid::Uuid;

const PREFERENCE_PATH: &str = "/api/v1/profiles/current/viewing-preferences";

fn payload(profile_id: Uuid, autoplay: bool, language: Option<&str>) -> Value {
    json!({
        "expected_profile_id": profile_id,
        "viewing_preferences": {
            "autoplay_next_episode": autoplay,
            "audio_language": language,
            "prefer_audio_description": false,
            "subtitle_mode": "none",
            "subtitle_language": null,
            "prefer_sdh": false
        }
    })
}

async fn http(
    app: &Router,
    token: Option<&str>,
    method: Method,
    path: &str,
    payload: Option<Value>,
) -> anyhow::Result<(StatusCode, Value)> {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }
    let body = match payload {
        Some(value) => {
            builder = builder.header("Content-Type", "application/json");
            Body::from(serde_json::to_vec(&value)?)
        }
        None => Body::empty(),
    };
    let response = app.clone().oneshot(builder.body(body)?).await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65536).await?;
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    Ok((status, value))
}

async fn switch(
    app: &Router,
    token: &str,
    profile_id: Uuid,
    remember: Option<bool>,
) -> anyhow::Result<(StatusCode, Value)> {
    let body = match remember {
        Some(value) => json!({"remember_on_device": value}),
        None => json!({}),
    };
    http(
        app,
        Some(token),
        Method::POST,
        &format!("/api/v1/profiles/{profile_id}/switch"),
        Some(body),
    )
    .await
}

async fn wait_for_profile_lock(pool: &PgPool, count: i64) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() \
                 AND application_name = 'duskcue_profile_preferences_fixture' \
                 AND wait_event_type = 'Lock' AND query LIKE '%FOR NO KEY UPDATE%'",
            )
            .fetch_one(pool)
            .await?;
            if waiting >= count {
                return Ok::<_, sqlx::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    Ok(())
}

#[tokio::test]
#[ignore = "requires migrated disposable PostgreSQL 18 and DUSKCUE_PROFILE_PREFERENCES_TESTS=disposable"]
async fn profile_viewing_preferences_database_contract() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("DUSKCUE_PROFILE_PREFERENCES_TESTS").as_deref() == Ok("disposable"),
        "use an explicitly disposable PostgreSQL fixture"
    );
    let pool = PgPoolOptions::new()
        .max_connections(12)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET application_name = 'duskcue_profile_preferences_fixture'")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&std::env::var("DUSKCUE_DATABASE_URL")?)
        .await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(
        database.starts_with("duskcue_migration"),
        "preference verification requires a disposable migration database"
    );
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS audit_log_profile_preferences_test PARTITION OF audit_log DEFAULT",
    )
    .execute(&pool)
    .await?;

    let owner = Uuid::now_v7();
    let other_owner = Uuid::now_v7();
    let first = Uuid::now_v7();
    let second = Uuid::now_v7();
    let other = Uuid::now_v7();
    let session = Uuid::now_v7();
    let token = duskcue::domains::auth::service::generate_session_token();
    let token_hash = ring::digest::digest(&ring::digest::SHA256, token.as_bytes())
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    for user_id in [owner, other_owner] {
        sqlx::query("INSERT INTO users (id, username, display_name) VALUES ($1, $2, 'Fixture')")
            .bind(user_id)
            .bind(format!("preference-fixture-{user_id}"))
            .execute(&pool)
            .await?;
    }
    for (profile_id, user_id, name, is_default) in [
        (first, owner, "First", true),
        (second, owner, "Second", false),
        (other, other_owner, "Other", true),
    ] {
        sqlx::query(
            "INSERT INTO user_profiles (id, owner_user_id, name, is_default, metadata) \
             VALUES ($1, $2, $3, $4, '{\"fixture_sibling\":{\"retained\":true}}')",
        )
        .bind(profile_id)
        .bind(user_id)
        .bind(name)
        .bind(is_default)
        .execute(&pool)
        .await?;
    }
    let kids = service::create_profile(
        &pool,
        owner,
        CreateProfileRequest {
            name: "Kids fixture".into(),
            avatar: None,
            profile_type: Some("kids".into()),
            max_content_rating: None,
            library_ids: None,
            allow_search: None,
            allow_downloads: None,
            allow_external_links: None,
            allow_ambient_channels: None,
            parent_pin: Some("482916".into()),
        },
    )
    .await?
    .id;
    sqlx::query(
        "INSERT INTO user_sessions (id, user_id, active_profile_id, token_hash, device_id, expires_at) \
         VALUES ($1, $2, $3, $4, 'profile-preferences-fixture', now() + interval '1 hour')",
    )
    .bind(session)
    .bind(owner)
    .bind(first)
    .bind(token_hash)
    .execute(&pool)
    .await?;

    let bootstrap = BootstrapConfig {
        database_url: None,
        bind_address: "127.0.0.1".into(),
        port: 0,
        data_dir: std::env::temp_dir().join(format!("duskcue-preference-fixture-{owner}")),
        cache_dir: std::env::temp_dir().join(format!("duskcue-preference-cache-{owner}")),
        log_level: "error".into(),
        environment: "test".into(),
        encryption_key: None,
        geoip_license_key: None,
    };
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let (encryption_key, _) = EncryptionKey::generate();
    let state = AppState::new(pool.clone(), bootstrap, recorder.handle(), encryption_key);
    let app = profiles::router(state.clone()).with_state(state);

    assert_eq!(
        http(&app, None, Method::GET, PREFERENCE_PATH, None)
            .await?
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, unsaved) = http(&app, Some(&token), Method::GET, PREFERENCE_PATH, None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unsaved["profile_id"], json!(first));
    assert_eq!(unsaved["has_saved_preferences"], false);
    assert_eq!(
        unsaved["viewing_preferences"]["autoplay_next_episode"],
        true
    );

    let mut complete = payload(first, false, Some("ENG"));
    complete["viewing_preferences"]["subtitle_mode"] = json!("always");
    complete["viewing_preferences"]["subtitle_language"] = json!("SPA");
    complete["viewing_preferences"]["prefer_audio_description"] = json!(true);
    complete["viewing_preferences"]["prefer_sdh"] = json!(true);
    let (status, saved) = http(
        &app,
        Some(&token),
        Method::PATCH,
        PREFERENCE_PATH,
        Some(complete),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["has_saved_preferences"], true);
    assert_eq!(saved["viewing_preferences"]["audio_language"], "en");
    assert_eq!(saved["viewing_preferences"]["subtitle_language"], "es");
    assert_eq!(
        http(&app, Some(&token), Method::GET, PREFERENCE_PATH, None)
            .await?
            .1,
        saved
    );
    let metadata: Value = sqlx::query_scalar("SELECT metadata FROM user_profiles WHERE id = $1")
        .bind(first)
        .fetch_one(&pool)
        .await?;
    assert_eq!(metadata["fixture_sibling"]["retained"], true);
    assert_eq!(
        metadata["viewing_preferences"],
        saved["viewing_preferences"]
    );

    assert_eq!(switch(&app, &token, second, None).await?.0, StatusCode::OK);
    let fresh_second = http(&app, Some(&token), Method::GET, PREFERENCE_PATH, None)
        .await?
        .1;
    assert_eq!(fresh_second["has_saved_preferences"], false);
    for stale_profile in [first, other] {
        assert_eq!(
            http(
                &app,
                Some(&token),
                Method::PATCH,
                PREFERENCE_PATH,
                Some(payload(stale_profile, true, None)),
            )
            .await?
            .0,
            StatusCode::CONFLICT
        );
    }
    assert!(matches!(
        service::get_viewing_preferences(&pool, other_owner, session, other).await,
        Err(ProfilesError::AccessDenied)
    ));
    let other_metadata: Value =
        sqlx::query_scalar("SELECT metadata FROM user_profiles WHERE id = $1")
            .bind(other)
            .fetch_one(&pool)
            .await?;
    assert!(other_metadata.get("viewing_preferences").is_none());

    sqlx::query("UPDATE user_sessions SET profile_selection_required = true WHERE id = $1")
        .bind(session)
        .execute(&pool)
        .await?;
    for (method, body) in [
        (Method::GET, None),
        (Method::PATCH, Some(payload(second, true, None))),
    ] {
        assert_eq!(
            http(&app, Some(&token), method, PREFERENCE_PATH, body)
                .await?
                .0,
            StatusCode::CONFLICT
        );
    }
    sqlx::query("UPDATE user_sessions SET profile_selection_required = false WHERE id = $1")
        .bind(session)
        .execute(&pool)
        .await?;
    for invalid_field in [
        "max_content_rating",
        "audio_stream_index",
        "allow_downloads",
    ] {
        let mut invalid = payload(second, true, None);
        invalid["viewing_preferences"][invalid_field] = json!(1);
        assert_eq!(
            http(
                &app,
                Some(&token),
                Method::PATCH,
                PREFERENCE_PATH,
                Some(invalid)
            )
            .await?
            .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    for invalid_language in [
        "English",
        "en-US",
        "und",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ] {
        let (status, problem) = http(
            &app,
            Some(&token),
            Method::PATCH,
            PREFERENCE_PATH,
            Some(payload(second, true, Some(invalid_language))),
        )
        .await?;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(problem["errors"][0]["field"], "viewing_preferences");
        assert_eq!(problem["errors"][0]["code"], "invalid");
        assert!(problem["errors"][0]["message"].as_str().is_some());
    }
    let mut missing_subtitle_language = payload(second, true, None);
    missing_subtitle_language["viewing_preferences"]["subtitle_mode"] = json!("always");
    assert_eq!(
        http(
            &app,
            Some(&token),
            Method::PATCH,
            PREFERENCE_PATH,
            Some(missing_subtitle_language),
        )
        .await?
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    assert_eq!(
        switch(&app, &token, first, Some(true)).await?.0,
        StatusCode::OK
    );
    assert_eq!(switch(&app, &token, kids, None).await?.0, StatusCode::OK);
    assert_eq!(
        http(
            &app,
            Some(&token),
            Method::PATCH,
            PREFERENCE_PATH,
            Some(payload(kids, false, None))
        )
        .await?
        .0,
        StatusCode::OK
    );
    assert_eq!(
        http(
            &app,
            Some(&token),
            Method::PATCH,
            &format!("/api/v1/profiles/{kids}"),
            Some(json!({"name":"Changed"}))
        )
        .await?
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        switch(&app, &token, second, None).await?.0,
        StatusCode::FORBIDDEN
    );

    let mut blocker = pool.begin().await?;
    sqlx::query("SELECT id FROM user_profiles WHERE id = $1 FOR NO KEY UPDATE")
        .bind(kids)
        .fetch_one(&mut *blocker)
        .await?;
    let unlock_app = app.clone();
    let unlock_token = token.clone();
    let unlock = tokio::spawn(async move {
        http(
            &unlock_app,
            Some(&unlock_token),
            Method::POST,
            "/api/v1/profiles/parent-unlock",
            Some(json!({"pin":"482916"})),
        )
        .await
    });
    wait_for_profile_lock(&pool, 1).await?;
    let preference_app = app.clone();
    let preference_token = token.clone();
    let preference_save = tokio::spawn(async move {
        http(
            &preference_app,
            Some(&preference_token),
            Method::PATCH,
            PREFERENCE_PATH,
            Some(payload(kids, true, None)),
        )
        .await
    });
    wait_for_profile_lock(&pool, 2).await?;
    let switch_app = app.clone();
    let switch_token = token.clone();
    let switching =
        tokio::spawn(async move { switch(&switch_app, &switch_token, second, None).await });
    wait_for_profile_lock(&pool, 3).await?;
    blocker.commit().await?;
    let (unlock, preference_save, switching) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(unlock, preference_save, switching)
        })
        .await?;
    assert_eq!(unlock??.0, StatusCode::OK);
    assert_eq!(preference_save??.0, StatusCode::OK);
    assert_eq!(switching??.0, StatusCode::OK);
    let session_state = sqlx::query(
        "SELECT active_profile_id, parent_unlock_profile_id, parent_unlock_expires_at FROM user_sessions WHERE id = $1",
    )
    .bind(session)
    .fetch_one(&pool)
    .await?;
    assert_eq!(session_state.get::<Uuid, _>("active_profile_id"), second);
    assert_eq!(
        session_state.get::<Option<Uuid>, _>("parent_unlock_profile_id"),
        None
    );
    assert!(
        session_state
            .get::<Option<chrono::DateTime<chrono::Utc>>, _>("parent_unlock_expires_at")
            .is_none()
    );
    assert_eq!(
        service::remembered_profile_id(&pool, owner, Some("profile-preferences-fixture")).await?,
        Some(first)
    );

    assert_eq!(switch(&app, &token, first, None).await?.0, StatusCode::OK);
    let mut blocker = pool.begin().await?;
    sqlx::query("SELECT id FROM user_profiles WHERE id = $1 FOR NO KEY UPDATE")
        .bind(first)
        .fetch_one(&mut *blocker)
        .await?;
    let switch_app = app.clone();
    let switch_token = token.clone();
    let switching =
        tokio::spawn(async move { switch(&switch_app, &switch_token, second, None).await });
    wait_for_profile_lock(&pool, 1).await?;
    let preference_app = app.clone();
    let preference_token = token.clone();
    let stale_save = tokio::spawn(async move {
        http(
            &preference_app,
            Some(&preference_token),
            Method::PATCH,
            PREFERENCE_PATH,
            Some(payload(first, true, Some("fr"))),
        )
        .await
    });
    wait_for_profile_lock(&pool, 2).await?;
    blocker.commit().await?;
    let (switching, stale_save) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(switching, stale_save)
    })
    .await?;
    assert_eq!(switching??.0, StatusCode::OK);
    assert_eq!(stale_save??.0, StatusCode::CONFLICT);
    let first_preferences: Value = sqlx::query_scalar(
        "SELECT metadata->'viewing_preferences' FROM user_profiles WHERE id = $1",
    )
    .bind(first)
    .fetch_one(&pool)
    .await?;
    assert_eq!(first_preferences, saved["viewing_preferences"]);
    assert_eq!(
        http(&app, Some(&token), Method::GET, PREFERENCE_PATH, None)
            .await?
            .1["has_saved_preferences"],
        false
    );

    let mut first_save: UpdateViewingPreferencesRequest =
        serde_json::from_value(payload(second, false, Some("de")))?;
    first_save.viewing_preferences.prefer_audio_description = true;
    let second_save = UpdateViewingPreferencesRequest {
        expected_profile_id: second,
        viewing_preferences: ViewingPreferencesRequest {
            autoplay_next_episode: true,
            audio_language: Some("ja".into()),
            prefer_audio_description: false,
            subtitle_mode: SubtitleMode::Always,
            subtitle_language: Some("eng".into()),
            prefer_sdh: true,
        },
    };
    let (first_save, second_save) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(
            service::update_viewing_preferences(&pool, owner, session, second, first_save),
            service::update_viewing_preferences(&pool, owner, session, second, second_save),
        )
    })
    .await?;
    let first_save = first_save?;
    let second_save = second_save?;
    let final_preferences = service::get_viewing_preferences(&pool, owner, session, second).await?;
    assert!(
        final_preferences.viewing_preferences == first_save.viewing_preferences
            || final_preferences.viewing_preferences == second_save.viewing_preferences
    );

    sqlx::query("UPDATE user_profiles SET metadata = jsonb_set(metadata, '{viewing_preferences}', 'null', true) WHERE id = $1")
        .bind(second).execute(&pool).await?;
    assert_eq!(
        http(&app, Some(&token), Method::GET, PREFERENCE_PATH, None)
            .await?
            .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        http(
            &app,
            Some(&token),
            Method::PATCH,
            PREFERENCE_PATH,
            Some(payload(second, true, None))
        )
        .await?
        .0,
        StatusCode::OK
    );
    sqlx::query("UPDATE user_profiles SET metadata = '[]' WHERE id = $1")
        .bind(second)
        .execute(&pool)
        .await?;
    assert_eq!(
        http(
            &app,
            Some(&token),
            Method::PATCH,
            PREFERENCE_PATH,
            Some(payload(second, true, None))
        )
        .await?
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );

    sqlx::query("DELETE FROM users WHERE id = ANY($1)")
        .bind(vec![owner, other_owner])
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}
