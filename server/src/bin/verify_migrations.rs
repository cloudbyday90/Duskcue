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

use std::time::Duration;

use clap::Parser;
use sqlx::postgres::{PgPool, PgPoolOptions};

#[derive(Parser, Debug)]
#[command(
    name = "verify_migrations",
    about = "Run Duskcue embedded SQLx migrations against a disposable database"
)]
struct Args {
    #[arg(long, env = "DUSKCUE_DATABASE_URL")]
    database_url: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&args.database_url)
        .await?;

    sqlx::migrate!().run(&pool).await?;
    validate_media_query_contract(&pool).await?;
    validate_storyboard_lock_contract(&pool).await?;
    validate_kids_parent_unlock_schema(&pool).await?;
    validate_ambient_playback_schema(&pool).await?;
    pool.close().await;

    println!("Duskcue migrations verified successfully");
    Ok(())
}

async fn validate_media_query_contract(pool: &PgPool) -> anyhow::Result<()> {
    let queries = [
        "SELECT mi.library_id, mi.content_rating FROM media_items mi JOIN libraries l ON l.id = mi.library_id WHERE mi.id = NULL::uuid AND l.deleted_at IS NULL",
        "SELECT EXISTS(SELECT 1 FROM media_items WHERE id = NULL::uuid)",
        "SELECT id, library_id FROM media_items WHERE id = NULL::uuid",
        "SELECT id, tmdb_id FROM media_items WHERE type = 'movie' AND match_state = 'confirmed' AND tmdb_id IS NOT NULL AND tmdb_id IN (NULL::bigint)",
        "SELECT mi.id FROM media_items mi WHERE mi.type IN ('movie', 'episode') AND EXISTS (SELECT 1 FROM media_files mf WHERE mf.media_item_id = mi.id AND mf.is_healthy = true) LIMIT 0",
        "SELECT artifact_id FROM storyboards LIMIT 0",
        "SELECT config_fingerprint FROM storyboards LIMIT 0",
    ];

    for query in queries {
        sqlx::query(query).execute(pool).await?;
    }

    Ok(())
}

async fn validate_storyboard_lock_contract(pool: &PgPool) -> anyhow::Result<()> {
    let key = 5_284_919_i64;
    let mut first = pool.begin().await?;
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
        .bind(key)
        .fetch_one(&mut *first)
        .await?;
    anyhow::ensure!(acquired, "could not acquire storyboard advisory lock");

    let mut second = pool.begin().await?;
    let acquired_while_held: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
        .bind(key)
        .fetch_one(&mut *second)
        .await?;
    anyhow::ensure!(
        !acquired_while_held,
        "storyboard advisory lock was not exclusive"
    );

    first.rollback().await?;
    let acquired_after_release: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
        .bind(key)
        .fetch_one(&mut *second)
        .await?;
    anyhow::ensure!(
        acquired_after_release,
        "storyboard advisory lock was not released with its transaction"
    );
    second.rollback().await?;
    Ok(())
}

async fn validate_kids_parent_unlock_schema(pool: &PgPool) -> anyhow::Result<()> {
    let queries = [
        "SELECT parent_pin_hash, parent_pin_failed_attempts, parent_pin_locked_until FROM user_profiles LIMIT 0",
        "SELECT parent_unlock_profile_id, parent_unlock_expires_at FROM user_sessions LIMIT 0",
    ];
    for query in queries {
        sqlx::query(query).execute(pool).await?;
    }

    let profile_pin_constraint: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid = 'user_profiles'::regclass \
         AND pg_get_constraintdef(oid) LIKE '%parent_pin_failed_attempts >= 0%')",
    )
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(
        profile_pin_constraint,
        "user_profiles is missing the non-negative parent PIN failure constraint"
    );

    let session_unlock_index: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_indexes WHERE schemaname = current_schema() \
         AND indexname = 'idx_user_sessions_parent_unlock_profile')",
    )
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(
        session_unlock_index,
        "user_sessions is missing the parent-unlock lookup index"
    );
    Ok(())
}

async fn validate_ambient_playback_schema(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query("SELECT ambient_channel_id FROM play_sessions LIMIT 0")
        .execute(pool)
        .await?;

    let ambient_session_index: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_indexes WHERE schemaname = current_schema() \
         AND indexname = 'idx_play_sessions_ambient_channel_id')",
    )
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(
        ambient_session_index,
        "play_sessions is missing the ambient-channel diagnostics index"
    );

    let ambient_channel_foreign_key: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_constraint \
         WHERE conrelid = 'play_sessions'::regclass AND contype = 'f' \
           AND confrelid = 'ambient_channels'::regclass)",
    )
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(
        ambient_channel_foreign_key,
        "play_sessions is missing the ambient-channel foreign key"
    );
    Ok(())
}
