use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use duskcue::config::BootstrapConfig;
use duskcue::domains::playback::{self, PlaybackError, service};
use duskcue::domains::profiles::ProfilesError;
use duskcue::error::AppError;
use duskcue::services::encryption::EncryptionKey;
use duskcue::state::AppState;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

async fn get(app: &Router, token: Option<&str>, path: &str) -> anyhow::Result<(StatusCode, Value)> {
    let mut request = Request::builder().uri(path);
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    let response = app.clone().oneshot(request.body(Body::empty())?).await?;
    let status = response.status();
    let body = to_bytes(response.into_body(), 65536).await?;
    let problem: Value = serde_json::from_slice(&body)?;
    assert_eq!(problem["status"], json!(status.as_u16()));
    Ok((status, problem))
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL 18 and DUSKCUE_TRANSCODE_ACCESS_TESTS=disposable"]
async fn transcode_binding_is_distinct_active_and_profile_scoped() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("DUSKCUE_TRANSCODE_ACCESS_TESTS").as_deref() == Ok("disposable"),
        "requires disposable fixture opt-in"
    );
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&std::env::var("DUSKCUE_DATABASE_URL")?)
        .await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(
        database.starts_with("duskcue_transcode_test"),
        "requires a duskcue_transcode_test disposable database"
    );
    sqlx::migrate!().run(&pool).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS play_sessions_transcode_test PARTITION OF play_sessions DEFAULT")
        .execute(&pool).await?;

    let owner = Uuid::now_v7();
    let other_owner = Uuid::now_v7();
    let profile = Uuid::now_v7();
    let other_profile = Uuid::now_v7();
    let peer_profile = Uuid::now_v7();
    for user_id in [owner, other_owner] {
        sqlx::query("INSERT INTO users (id,username,display_name,has_all_library_access) VALUES ($1,$2,'Transcode fixture',true)")
            .bind(user_id).bind(format!("transcode-{user_id}")).execute(&pool).await?;
    }
    for (profile_id, user_id) in [
        (profile, owner),
        (other_profile, owner),
        (peer_profile, other_owner),
    ] {
        sqlx::query("INSERT INTO user_profiles (id,owner_user_id,name,is_default) VALUES ($1,$2,'Fixture',$3)")
            .bind(profile_id).bind(user_id).bind(profile_id != other_profile).execute(&pool).await?;
    }
    let library = Uuid::now_v7();
    let item = Uuid::now_v7();
    sqlx::query("INSERT INTO libraries (id,name,slug,media_type,root_path) VALUES ($1,'Transcode fixture',$2,'movies','fixture')")
        .bind(library).bind(format!("transcode-{library}")).execute(&pool).await?;
    sqlx::query("INSERT INTO media_items (id,library_id,type,title,sort_title,content_rating) VALUES ($1,$2,'movie','Fixture','Fixture','PG')")
        .bind(item).bind(library).execute(&pool).await?;
    let play_session = Uuid::now_v7();
    let transcode_session = Uuid::now_v7();
    assert_ne!(play_session, transcode_session);
    sqlx::query("INSERT INTO play_sessions (id,user_id,profile_id,media_item_id,library_id,started_at,client_name,stream_decision,metadata) VALUES ($1,$2,$3,$4,$5,now(),'fixture','transcode',$6)")
        .bind(play_session).bind(owner).bind(profile).bind(item).bind(library)
        .bind(json!({"transcode_session_id":transcode_session,"sibling":"preserved"})).execute(&pool).await?;

    service::assert_transcode_profile_access(&pool, owner, profile, true, transcode_session)
        .await?;
    assert!(matches!(
        service::get_session_media_item_id(&pool, owner, profile, transcode_session).await,
        Err(PlaybackError::SessionNotFound)
    ));
    assert_eq!(
        service::get_session_media_item_id(&pool, owner, profile, play_session).await?,
        item
    );
    for (user_id, profile_id, stream_id) in [
        (owner, profile, play_session),
        (owner, other_profile, transcode_session),
        (other_owner, peer_profile, transcode_session),
        (owner, profile, Uuid::now_v7()),
    ] {
        assert!(matches!(
            service::assert_transcode_profile_access(&pool, user_id, profile_id, true, stream_id)
                .await,
            Err(AppError::Playback(PlaybackError::SessionNotFound))
        ));
    }

    let mut tokens = Vec::new();
    for (user_id, profile_id) in [
        (owner, profile),
        (owner, other_profile),
        (other_owner, peer_profile),
    ] {
        let token = duskcue::domains::auth::service::generate_session_token();
        let hash = ring::digest::digest(&ring::digest::SHA256, token.as_bytes())
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let session = Uuid::now_v7();
        sqlx::query("INSERT INTO user_sessions (id,user_id,active_profile_id,token_hash,expires_at) VALUES ($1,$2,$3,$4,now()+interval '1 hour')")
            .bind(session).bind(user_id).bind(profile_id).bind(hash).execute(&pool).await?;
        tokens.push(token);
    }
    let bootstrap = BootstrapConfig {
        database_url: None,
        bind_address: "127.0.0.1".into(),
        port: 0,
        data_dir: std::env::temp_dir().join(format!("duskcue-transcode-fixture-{owner}")),
        cache_dir: std::env::temp_dir().join(format!("duskcue-transcode-cache-{owner}")),
        log_level: "error".into(),
        environment: "test".into(),
        encryption_key: None,
        geoip_license_key: None,
    };
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let (encryption_key, _) = EncryptionKey::generate();
    let state = AppState::new(pool.clone(), bootstrap, recorder.handle(), encryption_key);
    let app = playback::router(state.clone()).with_state(state);
    let paths = [
        format!("/api/v1/transcode/{transcode_session}/manifest.m3u8"),
        format!("/api/v1/transcode/{transcode_session}/720p/index.m3u8"),
        format!("/api/v1/transcode/{transcode_session}/720p/seg_0000.m4s"),
    ];
    for path in &paths {
        assert_eq!(get(&app, None, path).await?.0, StatusCode::UNAUTHORIZED);
        assert_eq!(
            get(&app, Some(&tokens[1]), path).await?.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get(&app, Some(&tokens[2]), path).await?.0,
            StatusCode::NOT_FOUND
        );
    }

    assert!(matches!(
        service::assert_transcode_profile_access(&pool, owner, profile, false, transcode_session)
            .await,
        Err(AppError::Profiles(ProfilesError::ContentNotAllowed))
    ));
    sqlx::query(
        "UPDATE user_profiles SET profile_type='kids',max_content_rating='TV-PG' WHERE id=$1",
    )
    .bind(profile)
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO profile_library_access (profile_id,library_id) VALUES ($1,$2)")
        .bind(profile)
        .bind(library)
        .execute(&pool)
        .await?;
    service::assert_transcode_profile_access(&pool, owner, profile, true, transcode_session)
        .await?;
    sqlx::query("UPDATE media_items SET content_rating='R' WHERE id=$1")
        .bind(item)
        .execute(&pool)
        .await?;
    for path in &paths {
        let (status, problem) = get(&app, Some(&tokens[0]), path).await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(problem["title"], "PROFILE_003");
    }
    sqlx::query("UPDATE media_items SET content_rating=NULL WHERE id=$1")
        .bind(item)
        .execute(&pool)
        .await?;
    assert!(matches!(
        service::assert_transcode_profile_access(&pool, owner, profile, true, transcode_session)
            .await,
        Err(AppError::Profiles(ProfilesError::ContentNotAllowed))
    ));
    sqlx::query("UPDATE media_items SET content_rating='PG' WHERE id=$1")
        .bind(item)
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE libraries SET deleted_at=now() WHERE id=$1")
        .bind(library)
        .execute(&pool)
        .await?;
    assert!(matches!(
        service::assert_transcode_profile_access(&pool, owner, profile, true, transcode_session)
            .await,
        Err(AppError::Profiles(ProfilesError::ContentNotAllowed))
    ));
    sqlx::query("UPDATE libraries SET deleted_at=NULL WHERE id=$1")
        .bind(library)
        .execute(&pool)
        .await?;

    let replacement = Uuid::now_v7();
    sqlx::query("UPDATE play_sessions SET metadata=jsonb_set(metadata,'{transcode_session_id}',$2) WHERE id=$1")
        .bind(play_session).bind(json!(replacement)).execute(&pool).await?;
    assert!(matches!(
        service::assert_transcode_profile_access(&pool, owner, profile, true, transcode_session)
            .await,
        Err(AppError::Playback(PlaybackError::SessionNotFound))
    ));
    service::assert_transcode_profile_access(&pool, owner, profile, true, replacement).await?;
    sqlx::query("UPDATE play_sessions SET stopped_at=now() WHERE id=$1")
        .bind(play_session)
        .execute(&pool)
        .await?;
    assert!(matches!(
        service::assert_transcode_profile_access(&pool, owner, profile, true, replacement).await,
        Err(AppError::Playback(PlaybackError::SessionNotFound))
    ));
    for malformed in [
        json!({}),
        json!({"transcode_session_id":"invalid"}),
        json!({"transcode_session_id":42}),
    ] {
        sqlx::query("UPDATE play_sessions SET stopped_at=NULL,metadata=$2 WHERE id=$1")
            .bind(play_session)
            .bind(malformed)
            .execute(&pool)
            .await?;
        assert!(matches!(
            service::assert_transcode_profile_access(&pool, owner, profile, true, replacement)
                .await,
            Err(AppError::Playback(PlaybackError::SessionNotFound))
        ));
    }
    sqlx::query("DELETE FROM users WHERE id=ANY($1)")
        .bind(vec![owner, other_owner])
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM libraries WHERE id=$1")
        .bind(library)
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}
