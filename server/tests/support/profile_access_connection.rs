// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::time::Duration;

use duskcue::domains::profiles::ProfilesError;
use duskcue::domains::profiles::service::{
    assert_media_access_on_connection, load_profile_scope_on_connection,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub(super) async fn run(
    pool: &PgPool,
    owner: Uuid,
    profile: Uuid,
    item: Uuid,
) -> anyhow::Result<()> {
    let single = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(250))
        .connect_with(pool.connect_options().as_ref().clone())
        .await?;
    let result = tokio::time::timeout(Duration::from_secs(3), async {
        let mut transaction = single.begin().await?;
        anyhow::ensure!(single.try_acquire().is_none());
        let result = exercise(&mut transaction, owner, profile, item).await;
        transaction.rollback().await?;
        result
    })
    .await
    .map_err(anyhow::Error::from)
    .and_then(|result| result);
    single.close().await;
    result
}

async fn exercise(
    connection: &mut PgConnection,
    owner: Uuid,
    profile: Uuid,
    item: Uuid,
) -> anyhow::Result<()> {
    let scope = load_profile_scope_on_connection(connection, owner, profile, true).await?;
    assert_media_access_on_connection(connection, &scope, item).await?;
    anyhow::ensure!(matches!(
        load_profile_scope_on_connection(connection, Uuid::now_v7(), profile, true).await,
        Err(ProfilesError::NotFound)
    ));
    let library: Uuid = sqlx::query_scalar("SELECT library_id FROM media_items WHERE id = $1")
        .bind(item)
        .fetch_one(&mut *connection)
        .await?;
    sqlx::query("DELETE FROM user_library_access WHERE user_id = $1")
        .bind(owner)
        .execute(&mut *connection)
        .await?;
    let scope = load_profile_scope_on_connection(connection, owner, profile, false).await?;
    anyhow::ensure!(matches!(
        assert_media_access_on_connection(connection, &scope, item).await,
        Err(ProfilesError::ContentNotAllowed)
    ));
    sqlx::query("INSERT INTO user_library_access(user_id, library_id) VALUES ($1, $2)")
        .bind(owner)
        .bind(library)
        .execute(&mut *connection)
        .await?;
    let mut scope = load_profile_scope_on_connection(connection, owner, profile, false).await?;
    assert_media_access_on_connection(connection, &scope, item).await?;
    scope.profile_type = "kids".into();
    scope.max_content_rating = "PG".into();
    scope.library_ids = vec![library];
    anyhow::ensure!(matches!(
        assert_media_access_on_connection(connection, &scope, item).await,
        Err(ProfilesError::ContentNotAllowed)
    ));
    scope.max_content_rating = "NC-17".into();
    assert_media_access_on_connection(connection, &scope, item).await?;
    scope.library_ids.clear();
    anyhow::ensure!(matches!(
        assert_media_access_on_connection(connection, &scope, item).await,
        Err(ProfilesError::ContentNotAllowed)
    ));
    Ok(())
}
