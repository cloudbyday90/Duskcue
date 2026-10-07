use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use duskcue::config::BootstrapConfig;
use duskcue::domains::{collections, libraries, media, search};
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
    if status == StatusCode::OK {
        assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    }
    let bytes = to_bytes(response.into_body(), 1_000_000).await?;
    Ok((status, serde_json::from_slice(&bytes)?))
}

fn page_url(path: &str, cursor: Option<&str>, extra: &str) -> String {
    format!(
        "{path}?limit=5{extra}{}",
        cursor
            .map(|value| format!(
                "&cursor={}",
                url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>()
            ))
            .unwrap_or_default()
    )
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL migrations and DUSKCUE_TONIGHT_BROWSE_TESTS=disposable"]
async fn browse_reads_are_complete_profile_scoped_and_separate_from_management()
-> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("DUSKCUE_TONIGHT_BROWSE_TESTS").as_deref() == Ok("disposable"),
        "requires disposable fixture opt-in"
    );
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("DUSKCUE_DATABASE_URL")?)
        .await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(
        database.starts_with("duskcue_tonight_"),
        "requires a duskcue_tonight_ disposable database"
    );
    sqlx::query("CREATE TABLE IF NOT EXISTS audit_log_tonight_test PARTITION OF audit_log DEFAULT")
        .execute(&pool)
        .await?;

    let owner = Uuid::now_v7();
    let profiles = [Uuid::now_v7(), Uuid::now_v7()];
    let libraries = [
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
    ];
    sqlx::query("INSERT INTO users (id,username,display_name,role,has_all_library_access) VALUES ($1,$2,'Browse fixture','member',false)")
        .bind(owner).bind(format!("tonight-{owner}")).execute(&pool).await?;
    for (index, profile) in profiles.iter().enumerate() {
        sqlx::query("INSERT INTO user_profiles (id,owner_user_id,name,profile_type,max_content_rating) VALUES ($1,$2,$3,$4,'TV-PG')")
            .bind(profile).bind(owner).bind(format!("Profile {index}")).bind(if index == 1 { "kids" } else { "standard" }).execute(&pool).await?;
    }
    for (index, library) in libraries.iter().enumerate() {
        sqlx::query("INSERT INTO libraries (id,name,slug,media_type,root_path,deleted_at) VALUES ($1,$2,$3,'movies','private-fixture-path',CASE WHEN $4=3 THEN now() ELSE NULL END)")
            .bind(library).bind(format!("Room {index}")).bind(format!("tonight-{library}")).bind(index as i32).execute(&pool).await?;
    }
    sqlx::query(
        "INSERT INTO user_library_access (user_id,library_id) VALUES ($1,$2),($1,$3),($1,$4)",
    )
    .bind(owner)
    .bind(libraries[0])
    .bind(libraries[1])
    .bind(libraries[3])
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO profile_library_access (profile_id,library_id) VALUES ($1,$2)")
        .bind(profiles[1])
        .bind(libraries[0])
        .execute(&pool)
        .await?;

    let collection = Uuid::now_v7();
    sqlx::query("INSERT INTO collections (id,name,slug,item_count,metadata) VALUES ($1,'Evening collection',$2,999,'{\"secret_admin_config\":true}')")
        .bind(collection).bind(format!("tonight-{collection}")).execute(&pool).await?;
    let genres = [Uuid::now_v7(), Uuid::now_v7()];
    let genre_slugs = [format!("family-{owner}"), format!("restricted-{owner}")];
    for (index, genre) in genres.iter().enumerate() {
        sqlx::query("INSERT INTO genres (id,name,slug) VALUES ($1,$2,$3)")
            .bind(genre)
            .bind(format!("Genre {genre}"))
            .bind(&genre_slugs[index])
            .execute(&pool)
            .await?;
    }
    let mut expected = Vec::new();
    for index in 0..69 {
        let id = Uuid::now_v7();
        let library = if index == 67 {
            libraries[2]
        } else if index == 68 {
            libraries[3]
        } else {
            libraries[0]
        };
        let rating = if index == 0 {
            Some("R")
        } else if index == 1 {
            None
        } else {
            Some("PG")
        };
        sqlx::query("INSERT INTO media_items (id,library_id,type,title,sort_title,content_rating,runtime_seconds,premiere_date,overview,rating_average) VALUES ($1,$2,'movie',$3,$3,$4,900,CASE WHEN $5%3=0 THEN NULL ELSE make_date(2000+$5%5,1,1) END,repeat('night ', $5%4),($5%5)::real + 5)")
            .bind(id).bind(library).bind(format!("Night {}", index % 9)).bind(rating).bind(index as i32).execute(&pool).await?;
        sqlx::query("INSERT INTO collection_items (collection_id,media_item_id,position,is_missing) VALUES ($1,$2,$3,$4)")
            .bind(collection).bind(id).bind((index / 2) as i32).bind(index == 2).execute(&pool).await?;
        sqlx::query("INSERT INTO media_genres (media_item_id,genre_id) VALUES ($1,$2)")
            .bind(id)
            .bind(genres[if index == 0 || index >= 67 { 1 } else { 0 }])
            .execute(&pool)
            .await?;
        if index < 67 && index != 2 {
            expected.push((index / 2, id));
        }
    }
    expected.sort();
    let favorite = expected[3].1;
    sqlx::query("INSERT INTO user_item_data (user_id,profile_id,media_item_id,is_favorite,is_watched) VALUES ($1,$2,$3,true,true)")
        .bind(owner).bind(profiles[0]).bind(favorite).execute(&pool).await?;

    let hidden = Uuid::now_v7();
    let disabled = Uuid::now_v7();
    let denied_library_collection = Uuid::now_v7();
    let empty = Uuid::now_v7();
    for (id, visibility, enabled, library_id) in [
        (hidden, "hidden", true, None),
        (disabled, "visible", false, None),
        (
            denied_library_collection,
            "visible",
            true,
            Some(libraries[2]),
        ),
        (empty, "visible", true, None),
    ] {
        sqlx::query("INSERT INTO collections (id,name,slug,visibility,is_enabled,library_id) VALUES ($1,'Restricted collection',$2,$3,$4,$5)")
            .bind(id).bind(format!("tonight-{id}")).bind(visibility).bind(enabled).bind(library_id).execute(&pool).await?;
        if id != empty {
            sqlx::query(
                "INSERT INTO collection_items (collection_id,media_item_id) VALUES ($1,$2)",
            )
            .bind(id)
            .bind(favorite)
            .execute(&pool)
            .await?;
        }
    }

    let mut expected_collections = vec![collection];
    for _ in 0..6 {
        let id = Uuid::now_v7();
        sqlx::query("INSERT INTO collections (id,name,slug) VALUES ($1,'Evening collection',$2)")
            .bind(id)
            .bind(format!("tonight-{id}"))
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO collection_items (collection_id,media_item_id) VALUES ($1,$2)")
            .bind(id)
            .bind(favorite)
            .execute(&pool)
            .await?;
        expected_collections.push(id);
    }
    expected_collections.sort();

    let session = Uuid::now_v7();
    let token = duskcue::domains::auth::service::generate_session_token();
    let token_hash = ring::digest::digest(&ring::digest::SHA256, token.as_bytes())
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    sqlx::query("INSERT INTO user_sessions (id,user_id,active_profile_id,token_hash,expires_at) VALUES ($1,$2,$3,$4,now()+interval '1 hour')")
        .bind(session).bind(owner).bind(profiles[0]).bind(token_hash).execute(&pool).await?;
    let bootstrap = BootstrapConfig {
        database_url: None,
        bind_address: "127.0.0.1".into(),
        port: 0,
        data_dir: std::env::temp_dir().join(format!("duskcue-tonight-fixture-{owner}")),
        cache_dir: std::env::temp_dir().join(format!("duskcue-tonight-cache-{owner}")),
        log_level: "error".into(),
        environment: "test".into(),
        encryption_key: None,
        geoip_license_key: None,
    };
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let (key, _) = EncryptionKey::generate();
    let state = AppState::new(pool.clone(), bootstrap, recorder.handle(), key);
    let app = libraries::router(state.clone())
        .merge(collections::router(state.clone()))
        .merge(media::router(state.clone()))
        .merge(search::router(state.clone()))
        .with_state(state);

    assert_eq!(
        get(&app, None, "/api/v1/browse/libraries").await?.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&app, Some(&token), "/api/v1/libraries").await?.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(&app, Some(&token), "/api/v1/collections").await?.0,
        StatusCode::FORBIDDEN
    );
    let (status, rooms) = get(&app, Some(&token), "/api/v1/browse/libraries?limit=1").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rooms["items"].as_array().unwrap().len(), 1);
    assert_eq!(rooms["has_more"], true);
    assert!(rooms["items"][0].get("root_path").is_none());
    let library_cursor = rooms["cursor"].as_str().unwrap();
    let next_rooms = get(
        &app,
        Some(&token),
        &page_url("/api/v1/browse/libraries", Some(library_cursor), ""),
    )
    .await?
    .1;
    assert_eq!(next_rooms["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        get(
            &app,
            Some(&token),
            &format!("/api/v1/browse/libraries/{}", libraries[2])
        )
        .await?
        .0,
        StatusCode::NOT_FOUND
    );
    let collections = get(&app, Some(&token), "/api/v1/browse/collections")
        .await?
        .1;
    assert_eq!(collections["items"].as_array().unwrap().len(), 7);
    let main_collection = collections["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == json!(collection))
        .unwrap();
    assert_eq!(main_collection["item_count"], 66);
    let mut collection_cursor = None;
    let mut collection_ids = Vec::new();
    loop {
        let (status, page) = get(
            &app,
            Some(&token),
            &page_url(
                "/api/v1/browse/collections",
                collection_cursor.as_deref(),
                "",
            ),
        )
        .await?;
        assert_eq!(status, StatusCode::OK);
        collection_ids.extend(
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| serde_json::from_value::<Uuid>(item["id"].clone()).unwrap()),
        );
        if page["has_more"] == false {
            break;
        }
        collection_cursor = Some(page["cursor"].as_str().unwrap().to_string());
        assert!(collection_ids.len() <= 7);
    }
    assert_eq!(collection_ids, expected_collections);
    assert!(collections["items"][0].get("metadata").is_none());
    assert!(collections["items"][0].get("dynamic_config").is_none());
    for id in [hidden, disabled, denied_library_collection, empty] {
        assert_eq!(
            get(
                &app,
                Some(&token),
                &format!("/api/v1/browse/collections/{id}")
            )
            .await?
            .0,
            StatusCode::NOT_FOUND
        );
    }
    let path = format!("/api/v1/browse/collections/{collection}/items");
    let mut cursor = None;
    let mut ids = Vec::new();
    let mut first_cursor = None;
    loop {
        let (status, page) =
            get(&app, Some(&token), &page_url(&path, cursor.as_deref(), "")).await?;
        assert_eq!(status, StatusCode::OK);
        for item in page["items"].as_array().unwrap() {
            ids.push(serde_json::from_value::<Uuid>(item["id"].clone())?);
            assert!(item.get("watch_state").is_some());
            assert_eq!(item["availability"]["can_play"], false);
        }
        if page["has_more"] == false {
            break;
        }
        cursor = Some(page["cursor"].as_str().unwrap().to_string());
        if first_cursor.is_none() {
            first_cursor = cursor.clone();
        }
        assert!(ids.len() <= 66);
    }
    assert_eq!(ids, expected.iter().map(|(_, id)| *id).collect::<Vec<_>>());
    let favorites = get(&app, Some(&token), &page_url(&path, None, "&favorite=true"))
        .await?
        .1;
    assert_eq!(favorites["items"].as_array().unwrap().len(), 1);
    assert_eq!(favorites["items"][0]["id"], json!(favorite));
    let sorted = get(
        &app,
        Some(&token),
        &page_url(&path, None, "&sort=year&order=asc"),
    )
    .await?
    .1;
    assert!(
        sorted["items"]
            .as_array()
            .unwrap()
            .windows(2)
            .all(|pair| pair[0]["premiere_date"].as_str() <= pair[1]["premiere_date"].as_str())
    );
    assert_eq!(
        get(
            &app,
            Some(&token),
            &page_url("/api/v1/browse/collections", Some(library_cursor), "")
        )
        .await?
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let invalid = get(&app, Some(&token), "/api/v1/browse/libraries?limit=0").await?;
    assert_eq!(invalid.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(invalid.1["errors"][0]["field"], "query");

    let mut search_cursor = None;
    for sort in ["relevance", "title", "year"] {
        for order in ["asc", "desc"] {
            let mut cursor = None;
            let mut items = Vec::new();
            loop {
                let (status, page) = get(
                    &app,
                    Some(&token),
                    &page_url(
                        "/api/v1/search",
                        cursor.as_deref(),
                        &format!("&q=night&sort={sort}&order={order}"),
                    ),
                )
                .await?;
                assert_eq!(status, StatusCode::OK, "{page}");
                assert_eq!(page["facets"]["types"][0]["count"], 67);
                for item in page["items"].as_array().unwrap() {
                    assert!(item.get("watch_state").is_some());
                    items.push(item.clone());
                }
                if page["has_more"] == false {
                    break;
                }
                cursor = Some(page["cursor"].as_str().unwrap().to_string());
                if sort == "relevance" && order == "desc" && search_cursor.is_none() {
                    search_cursor = cursor.clone();
                }
                assert!(items.len() <= 67);
            }
            assert_eq!(items.len(), 67);
            let distinct = items
                .iter()
                .map(|item| item["id"].as_str().unwrap())
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(distinct.len(), 67);
            if sort == "title" {
                assert!(items.windows(2).all(|pair| {
                    let left = (pair[0]["sort_title"].as_str(), pair[0]["id"].as_str());
                    let right = (pair[1]["sort_title"].as_str(), pair[1]["id"].as_str());
                    if order == "asc" {
                        left <= right
                    } else {
                        left >= right
                    }
                }));
            }
            if sort == "year" {
                let mut null_seen = false;
                let mut previous = None;
                for item in items {
                    if item["premiere_date"].is_null() {
                        null_seen = true;
                        continue;
                    }
                    assert!(!null_seen, "unknown year must be last in either direction");
                    let key = (
                        item["premiere_date"].as_str().unwrap()[..4].to_string(),
                        item["id"].as_str().unwrap().to_string(),
                    );
                    if let Some(previous) = previous {
                        assert!(if order == "asc" {
                            previous <= key
                        } else {
                            previous >= key
                        });
                    }
                    previous = Some(key);
                }
            }
        }
    }
    let genre_search = get(
        &app,
        Some(&token),
        &format!("/api/v1/search?q=night&genre={}&limit=100", genre_slugs[0]),
    )
    .await?
    .1;
    assert_eq!(genre_search["items"].as_array().unwrap().len(), 66);
    assert_eq!(
        genre_search["facets"]["genres"].as_array().unwrap().len(),
        1
    );
    assert_eq!(genre_search["facets"]["genres"][0]["count"], 66);
    let rated_search = get(
        &app,
        Some(&token),
        "/api/v1/search?q=night&year=2001&rating_min=6",
    )
    .await?
    .1;
    assert!(!rated_search["items"].as_array().unwrap().is_empty());
    assert!(
        rated_search["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(
                |item| item["premiere_date"].as_str().unwrap().starts_with("2001")
                    && item["rating_average"].as_f64().unwrap() >= 6.0
            )
    );

    let favorite_search = get(
        &app,
        Some(&token),
        "/api/v1/search?q=night&favorite=true&watch=watched",
    )
    .await?
    .1;
    assert_eq!(favorite_search["items"].as_array().unwrap().len(), 1);
    assert_eq!(favorite_search["items"][0]["id"], json!(favorite));
    assert_eq!(favorite_search["facets"]["types"][0]["count"], 1);
    for extra in ["&q=other", "&q=night&sort=title", "&q=night&favorite=true"] {
        assert_eq!(
            get(
                &app,
                Some(&token),
                &page_url("/api/v1/search", search_cursor.as_deref(), extra)
            )
            .await?
            .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let invalid_search = get(&app, Some(&token), "/api/v1/search?q=night&sort=unknown").await?;
    assert_eq!(invalid_search.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(invalid_search.1["errors"][0]["field"], "query");

    sqlx::query("UPDATE user_sessions SET active_profile_id=$2 WHERE id=$1")
        .bind(session)
        .bind(profiles[1])
        .execute(&pool)
        .await?;
    let kids_rooms = get(&app, Some(&token), "/api/v1/browse/libraries").await?.1;
    assert_eq!(kids_rooms["items"].as_array().unwrap().len(), 1);
    let kids_collection = get(
        &app,
        Some(&token),
        &format!("/api/v1/browse/collections/{collection}"),
    )
    .await?
    .1;
    assert_eq!(kids_collection["item_count"], 64);
    assert_eq!(
        get(
            &app,
            Some(&token),
            &page_url(&path, first_cursor.as_deref(), "")
        )
        .await?
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        get(&app, Some(&token), &page_url(&path, None, "&favorite=true"))
            .await?
            .1["items"],
        json!([])
    );
    let kids_search = get(&app, Some(&token), "/api/v1/search?q=night&limit=100")
        .await?
        .1;
    assert_eq!(kids_search["items"].as_array().unwrap().len(), 65);
    assert_eq!(kids_search["facets"]["types"][0]["count"], 65);
    assert_eq!(kids_search["facets"]["genres"].as_array().unwrap().len(), 1);
    assert_eq!(
        kids_search["facets"]["genres"][0]["value"],
        json!(genre_slugs[0])
    );
    assert_eq!(kids_search["facets"]["genres"][0]["count"], 65);
    assert!(
        kids_search["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["content_rating"] == "PG"
                && item["watch_state"]["is_favorite"] == false)
    );
    assert_eq!(
        get(
            &app,
            Some(&token),
            &page_url(
                "/api/v1/search",
                search_cursor.as_deref(),
                "&q=night&sort=relevance&order=desc"
            )
        )
        .await?
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    sqlx::query("UPDATE user_profiles SET allow_search=false WHERE id=$1")
        .bind(profiles[1])
        .execute(&pool)
        .await?;
    let denied_search = get(&app, Some(&token), "/api/v1/search?q=night").await?;
    assert_eq!(denied_search.0, StatusCode::FORBIDDEN);
    assert_eq!(denied_search.1["title"], "PROFILE_004");
    sqlx::query("UPDATE user_sessions SET profile_selection_required=true WHERE id=$1")
        .bind(session)
        .execute(&pool)
        .await?;
    for path in [
        "/api/v1/browse/libraries",
        "/api/v1/browse/collections",
        "/api/v1/media-items/continue-watching",
        "/api/v1/search?q=night",
    ] {
        let rejected = get(&app, Some(&token), path).await?;
        assert_eq!(rejected.0, StatusCode::CONFLICT);
        assert_eq!(rejected.1["title"], "PROFILE_014");
    }
    pool.close().await;
    Ok(())
}
