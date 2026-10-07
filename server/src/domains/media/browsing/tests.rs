// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::*;

fn fixture_scope() -> ProfileScope {
    ProfileScope {
        owner_user_id: Uuid::now_v7(),
        profile_id: Uuid::now_v7(),
        profile_type: "standard".into(),
        max_content_rating: "NC-17".into(),
        allow_search: true,
        allow_downloads: true,
        allow_external_links: true,
        allow_ambient_channels: true,
        library_ids: vec![],
        user_library_ids: vec![],
        has_all_library_access: false,
    }
}

#[test]
fn browse_rejects_invalid_query_and_cross_context_cursors() {
    for query in [
        MediaBrowseQuery {
            limit: Some(0),
            ..Default::default()
        },
        MediaBrowseQuery {
            limit: Some(101),
            ..Default::default()
        },
        MediaBrowseQuery {
            sort: Some("runtime".into()),
            ..Default::default()
        },
        MediaBrowseQuery {
            order: Some("sideways".into()),
            ..Default::default()
        },
        MediaBrowseQuery {
            watch: Some("anything".into()),
            ..Default::default()
        },
        MediaBrowseQuery {
            year: Some(-1),
            ..Default::default()
        },
    ] {
        assert!(browse_options(&query).is_err());
    }
    let scope = fixture_scope();
    let query = MediaBrowseQuery::default();
    let options = browse_options(&query).unwrap();
    let context = browse_context(&scope, BrowseMode::Catalog, &query, &options);
    let cursor = base64::engine::general_purpose::STANDARD.encode(
        serde_json::to_vec(&BrowseCursor {
            version: 1,
            context: context.clone(),
            id: Uuid::now_v7(),
            key: BrowseCursorKey::Added,
        })
        .unwrap(),
    );
    assert!(decode_browse_cursor(Some(&cursor), &context, false).is_ok());
    for other_context in [
        browse_context(&fixture_scope(), BrowseMode::Catalog, &query, &options),
        browse_context(&scope, BrowseMode::Continue, &query, &options),
        browse_context(
            &scope,
            BrowseMode::Catalog,
            &MediaBrowseQuery {
                favorite: Some(true),
                ..Default::default()
            },
            &options,
        ),
    ] {
        assert!(decode_browse_cursor(Some(&cursor), &other_context, false).is_err());
    }
    for invalid in ["", "bad-base64", "e30=", "eyJpZCI6ImJhZCJ9"] {
        assert!(decode_browse_cursor(Some(invalid), &context, true).is_err());
    }
    let legacy = base64::engine::general_purpose::STANDARD
        .encode(serde_json::json!({"id": Uuid::now_v7()}).to_string());
    assert!(decode_browse_cursor(Some(&legacy), &context, true).is_ok());
    assert!(decode_browse_cursor(Some(&legacy), &context, false).is_err());
    let wrong_key = BrowseCursor {
        version: 1,
        context,
        id: Uuid::now_v7(),
        key: BrowseCursorKey::Title("title".into()),
    };
    assert!(
        push_browse_cursor(
            &mut QueryBuilder::new("SELECT 1"),
            &wrong_key,
            &options,
            "year"
        )
        .is_err()
    );
}

async fn seed_item(
    pool: &sqlx::PgPool,
    library: Uuid,
    item_type: &str,
    title: &str,
    rating: Option<&str>,
) -> anyhow::Result<Uuid> {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO media_items (id, library_id, type, title, sort_title, content_rating, runtime_seconds) VALUES ($1, $2, $3, $4, $4, $5, 900)")
        .bind(id).bind(library).bind(item_type).bind(title).bind(rating).execute(pool).await?;
    Ok(id)
}

async fn seed_watch(
    pool: &sqlx::PgPool,
    owner: Uuid,
    profile: Uuid,
    item: Uuid,
    watched: bool,
    favorite: bool,
    activity: &str,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO user_item_data (user_id, profile_id, media_item_id, is_watched, is_favorite, resume_position_ms, last_played_at, play_count) VALUES ($1, $2, $3, $4, $5, 30000, $6::text::timestamptz, 1)")
        .bind(owner).bind(profile).bind(item).bind(watched).bind(favorite).bind(activity).execute(pool).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL 18, migrated schema, DUSKCUE_MEDIA_BROWSING_TESTS=disposable"]
async fn media_browsing_database_contract() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("DUSKCUE_MEDIA_BROWSING_TESTS").as_deref() == Ok("disposable"),
        "requires disposable media test opt-in"
    );
    let pool = sqlx::PgPool::connect(&std::env::var("DUSKCUE_DATABASE_URL")?).await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(
        database.starts_with("duskcue_browse_"),
        "requires disposable duskcue_browse_ database"
    );
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS audit_log_media_browsing_test PARTITION OF audit_log DEFAULT",
    )
    .execute(&pool)
    .await?;
    let owner = Uuid::now_v7();
    let profiles = [Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7()];
    let libraries = [Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7()];
    sqlx::query("INSERT INTO users (id, username, display_name) VALUES ($1, $2, 'Browse fixture')")
        .bind(owner)
        .bind(format!("browse-{owner}"))
        .execute(&pool)
        .await?;
    for (index, profile) in profiles.iter().enumerate() {
        sqlx::query("INSERT INTO user_profiles (id, owner_user_id, name, profile_type, max_content_rating) VALUES ($1, $2, $3, $4, 'TV-PG')")
            .bind(profile).bind(owner).bind(format!("Profile {index}"))
            .bind(if index == 2 { "kids" } else { "standard" }).execute(&pool).await?;
    }
    for (index, library) in libraries.iter().enumerate() {
        sqlx::query("INSERT INTO libraries (id, name, slug, media_type, root_path, deleted_at) VALUES ($1, 'Fixture', $2, 'tvshows', 'fixture', CASE WHEN $3 = 2 THEN now() ELSE NULL END)")
            .bind(library).bind(format!("browse-{library}")).bind(index as i32).execute(&pool).await?;
    }
    sqlx::query("INSERT INTO user_library_access (user_id, library_id) VALUES ($1, $2), ($1, $3)")
        .bind(owner)
        .bind(libraries[0])
        .bind(libraries[2])
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO profile_library_access (profile_id, library_id) VALUES ($1, $2)")
        .bind(profiles[2])
        .bind(libraries[0])
        .execute(&pool)
        .await?;
    let scope =
        crate::domains::profiles::service::load_profile_scope(&pool, owner, profiles[0], false)
            .await?;
    let other_scope =
        crate::domains::profiles::service::load_profile_scope(&pool, owner, profiles[1], false)
            .await?;
    let kids_scope =
        crate::domains::profiles::service::load_profile_scope(&pool, owner, profiles[2], false)
            .await?;
    assert!(
        crate::domains::profiles::service::load_profile_scope(
            &pool,
            Uuid::now_v7(),
            profiles[0],
            true
        )
        .await
        .is_err()
    );
    let movie = seed_item(&pool, libraries[0], "movie", "Movie", Some("G")).await?;
    let unavailable = seed_item(&pool, libraries[0], "movie", "Unavailable", Some("G")).await?;
    let completed = seed_item(&pool, libraries[0], "movie", "Completed", Some("G")).await?;
    let other_movie = seed_item(&pool, libraries[0], "movie", "Other profile", Some("G")).await?;
    let forbidden = seed_item(&pool, libraries[1], "movie", "Forbidden", Some("G")).await?;
    let deleted = seed_item(&pool, libraries[2], "movie", "Deleted library", Some("G")).await?;
    for (item, watched) in [
        (movie, false),
        (unavailable, false),
        (completed, true),
        (forbidden, false),
        (deleted, false),
    ] {
        seed_watch(
            &pool,
            owner,
            profiles[0],
            item,
            watched,
            item == movie,
            "2026-10-03T12:00:00Z",
        )
        .await?;
    }
    seed_watch(
        &pool,
        owner,
        profiles[1],
        other_movie,
        false,
        false,
        "2026-10-03T12:00:00Z",
    )
    .await?;
    sqlx::query("INSERT INTO media_files (media_item_id, file_path, file_size, container_format, runtime_seconds) VALUES ($1, 'movie.mp4', 1, 'mp4', 1200)")
        .bind(movie).execute(&pool).await?;
    let series = seed_item(&pool, libraries[0], "series", "Series", Some("G")).await?;
    let season = seed_item(&pool, libraries[0], "season", "Season 1", Some("G")).await?;
    sqlx::query("INSERT INTO series (id) VALUES ($1)")
        .bind(series)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO seasons (id, series_id, season_number) VALUES ($1, $2, 1)")
        .bind(season)
        .bind(series)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO media_items (library_id, type, title, sort_title, runtime_seconds, content_rating, premiere_date, metadata) SELECT $1, 'episode', 'Episode ' || number, 'Same title', 900, CASE WHEN number = 106 THEN 'R' ELSE ' tv-y7-fv: annotation' END, CASE WHEN number > 103 THEN NULL ELSE make_date(2020 + number % 2, 1, 1) END, jsonb_build_object('fixture_episode', number) FROM generate_series(1, 106) number")
        .bind(libraries[0]).execute(&pool).await?;
    sqlx::query("INSERT INTO episodes (id, series_id, season_id, episode_number) SELECT id, $1, $2, (metadata->>'fixture_episode')::int FROM media_items WHERE library_id = $3 AND metadata ? 'fixture_episode'")
        .bind(series).bind(season).bind(libraries[0]).execute(&pool).await?;
    let unknown = seed_item(&pool, libraries[0], "episode", "Unknown number", None).await?;
    sqlx::query("INSERT INTO episodes (id, series_id, season_id) VALUES ($1, $2, $3)")
        .bind(unknown)
        .bind(series)
        .bind(season)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO media_files (media_item_id, file_path, file_size, container_format, runtime_seconds, is_healthy) SELECT ep.id, ep.id::text || '.mp4', 1, 'mp4', 1234, ep.episode_number != 2 FROM episodes ep WHERE ep.season_id = $1 AND ep.episode_number IS NOT NULL")
        .bind(season).execute(&pool).await?;
    let episode_one: Uuid =
        sqlx::query_scalar("SELECT id FROM episodes WHERE season_id = $1 AND episode_number = 1")
            .bind(season)
            .fetch_one(&pool)
            .await?;
    let episode_metadata = super::super::service::get_media_item(&pool, episode_one).await?;
    assert_eq!(episode_metadata.series_id, Some(series));
    assert_eq!(episode_metadata.season_id, Some(season));
    assert_eq!(episode_metadata.season_number, Some(1));
    seed_watch(
        &pool,
        owner,
        profiles[0],
        episode_one,
        false,
        false,
        "2026-10-02T12:00:00Z",
    )
    .await?;
    seed_watch(
        &pool,
        owner,
        profiles[2],
        episode_one,
        true,
        false,
        "2026-10-02T12:00:00Z",
    )
    .await?;
    let mut page_query = MediaBrowsePageQuery {
        limit: Some(1),
        cursor: None,
    };
    let mut continued = vec![];
    loop {
        let page = browse_continue_watching(&pool, &scope, &page_query).await?;
        continued.extend(page.items.iter().map(|item| item.media.id));
        if let Some(item) = page.items.iter().find(|item| item.media.id == movie) {
            assert_eq!(item.duration_ms, Some(1_200_000));
            assert!(item.availability.can_play);
            assert_eq!(item.watch_state.resume_position_ms, 30_000);
            assert!(item.watch_state.is_favorite);
        }
        if let Some(item) = page.items.iter().find(|item| item.media.id == unavailable) {
            assert!(!item.availability.can_play);
        }
        if !page.has_more {
            assert!(page.cursor.is_none());
            break;
        }
        assert!(
            browse_continue_watching(
                &pool,
                &other_scope,
                &MediaBrowsePageQuery {
                    cursor: page.cursor.clone(),
                    limit: Some(1)
                }
            )
            .await
            .is_err()
        );
        page_query.cursor = page.cursor;
    }
    let mut first_movies = [movie, unavailable];
    first_movies.sort_by(|a, b| b.cmp(a));
    assert_eq!(
        continued,
        vec![first_movies[0], first_movies[1], episode_one]
    );
    let other_page =
        browse_continue_watching(&pool, &other_scope, &MediaBrowsePageQuery::default()).await?;
    assert_eq!(other_page.items.len(), 1);
    assert_eq!(other_page.items[0].media.id, other_movie);
    let seasons =
        browse_series_seasons(&pool, &scope, series, &MediaBrowsePageQuery::default()).await?;
    assert_eq!(seasons.items[0].episode_count, Some(107));
    assert_eq!(seasons.items[0].available_episode_count, Some(105));
    let kids_seasons =
        browse_series_seasons(&pool, &kids_scope, series, &MediaBrowsePageQuery::default()).await?;
    assert_eq!(kids_seasons.items[0].episode_count, Some(105));
    assert_eq!(kids_seasons.items[0].available_episode_count, Some(104));
    assert_eq!(kids_seasons.items[0].watched_episode_count, Some(1));
    for (number, rating) in [(0, "G"), (2, "R")] {
        let extra_season =
            seed_item(&pool, libraries[0], "season", "Other season", Some(rating)).await?;
        sqlx::query("INSERT INTO seasons (id, series_id, season_number) VALUES ($1, $2, $3)")
            .bind(extra_season)
            .bind(series)
            .bind(number)
            .execute(&pool)
            .await?;
    }
    for (selected_scope, expected) in [(&scope, vec![0, 1, 2]), (&kids_scope, vec![0, 1])] {
        let mut query = MediaBrowsePageQuery {
            limit: Some(1),
            cursor: None,
        };
        let mut numbers = vec![];
        loop {
            let page = browse_series_seasons(&pool, selected_scope, series, &query).await?;
            numbers.extend(
                page.items
                    .iter()
                    .map(|item| item.media.season_number.unwrap()),
            );
            if !page.has_more {
                break;
            }
            assert!(
                browse_season_episodes(
                    &pool,
                    selected_scope,
                    season,
                    &MediaBrowsePageQuery {
                        cursor: page.cursor.clone(),
                        limit: Some(1)
                    }
                )
                .await
                .is_err()
            );
            query.cursor = page.cursor;
        }
        assert_eq!(numbers, expected);
    }
    assert!(
        browse_series_seasons(&pool, &scope, forbidden, &MediaBrowsePageQuery::default())
            .await
            .is_err()
    );
    assert!(
        crate::domains::profiles::service::assert_media_access(&pool, &scope, forbidden)
            .await
            .is_err()
    );
    assert!(
        browse_season_episodes(&pool, &scope, series, &MediaBrowsePageQuery::default())
            .await
            .is_err()
    );
    for (selected_scope, expected) in [(&scope, 107), (&kids_scope, 105)] {
        let mut query = MediaBrowsePageQuery {
            limit: Some(7),
            cursor: None,
        };
        let mut ids = std::collections::HashSet::new();
        let mut numbers = vec![];
        loop {
            let page = browse_season_episodes(&pool, selected_scope, season, &query).await?;
            for item in page.items {
                assert!(ids.insert(item.media.id));
                assert_eq!(item.media.series_id, Some(series));
                assert_eq!(item.media.season_id, Some(season));
                assert_eq!(item.media.season_number, Some(1));
                numbers.push(item.media.episode_number);
                if item.media.episode_number == Some(2) {
                    assert!(!item.availability.can_play);
                }
            }
            if !page.has_more {
                break;
            }
            query.cursor = page.cursor;
        }
        assert_eq!(ids.len(), expected);
        assert_eq!(
            numbers
                .iter()
                .filter_map(|number| *number)
                .collect::<Vec<_>>(),
            (1..=if expected == 107 { 106 } else { 105 }).collect::<Vec<_>>()
        );
        if expected == 107 {
            assert_eq!(numbers.last(), Some(&None));
        }
    }
    for (sort, order) in [
        ("title", "asc"),
        ("title", "desc"),
        ("year", "asc"),
        ("year", "desc"),
    ] {
        let mut query = MediaBrowseQuery {
            r#type: Some("episode".into()),
            sort: Some(sort.into()),
            order: Some(order.into()),
            limit: Some(9),
            ..Default::default()
        };
        let mut ids = std::collections::HashSet::new();
        let mut found_null = false;
        loop {
            let page = browse_media_items(&pool, &scope, &query).await?;
            for item in page.items {
                assert!(ids.insert(item.media.id));
                if sort == "year" {
                    if item.media.premiere_date.is_none() {
                        found_null = true;
                    } else {
                        assert!(!found_null);
                    }
                }
            }
            if !page.has_more {
                break;
            }
            query.cursor = page.cursor;
        }
        assert_eq!(ids.len(), 107);
    }
    let favorites = browse_media_items(
        &pool,
        &scope,
        &MediaBrowseQuery {
            favorite: Some(true),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(favorites.items.len(), 1);
    assert_eq!(favorites.items[0].media.id, movie);
    let watched = browse_media_items(
        &pool,
        &scope,
        &MediaBrowseQuery {
            watch: Some("watched".into()),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(watched.items.len(), 1);
    assert_eq!(watched.items[0].media.id, completed);
    let in_progress = browse_media_items(
        &pool,
        &scope,
        &MediaBrowseQuery {
            watch: Some("in_progress".into()),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(in_progress.items.len(), 3);
    let unwatched = browse_media_items(
        &pool,
        &scope,
        &MediaBrowseQuery {
            r#type: Some("movie".into()),
            watch: Some("unwatched".into()),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(unwatched.items.len(), 3);
    assert!(
        unwatched
            .items
            .iter()
            .find(|item| item.media.id == other_movie)
            .is_some_and(|item| item.watch_state.resume_position_ms == 0
                && item.watch_state.last_played_at.is_none())
    );
    let year_filtered = browse_media_items(
        &pool,
        &scope,
        &MediaBrowseQuery {
            year: Some(2021),
            limit: Some(100),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(year_filtered.items.len(), 52);
    let genre = Uuid::now_v7();
    sqlx::query("INSERT INTO genres (id, name, slug) VALUES ($1, $2, $2)")
        .bind(genre)
        .bind(format!("browse-{genre}"))
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO media_genres (media_item_id, genre_id) VALUES ($1, $2)")
        .bind(movie)
        .bind(genre)
        .execute(&pool)
        .await?;
    let genre_filtered = browse_media_items(
        &pool,
        &scope,
        &MediaBrowseQuery {
            genre_id: Some(genre),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(genre_filtered.items.len(), 1);
    assert_eq!(genre_filtered.items[0].media.id, movie);
    let kids_all = browse_media_items(
        &pool,
        &kids_scope,
        &MediaBrowseQuery {
            limit: Some(100),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(kids_all.items.len(), 100);
    assert!(kids_all.has_more);
    let mut denied_scope = kids_scope.clone();
    denied_scope.library_ids.clear();
    assert!(
        browse_media_items(&pool, &denied_scope, &MediaBrowseQuery::default())
            .await?
            .items
            .is_empty()
    );
    sqlx::query("DELETE FROM libraries WHERE id = ANY($1::uuid[])")
        .bind(libraries.to_vec())
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(owner)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM genres WHERE id = $1")
        .bind(genre)
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}
