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

use std::collections::HashMap;
use std::time::Instant;

use chrono::{DateTime, Duration, Utc};
use dashmap::mapref::entry::Entry;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::services::trakt_client::{
    TraktClient, TraktIds, TraktRating, TraktTokenResponse, TraktUserSettings,
};
use crate::state::AppState;

use crate::domains::trakt::error::TraktError;
use crate::domains::trakt::types::*;

const TOKEN_REFRESH_BUFFER: Duration = Duration::minutes(5);
const SYNC_LOCK_TTL: std::time::Duration = std::time::Duration::from_secs(15 * 60);

pub async fn get_account(
    state: &AppState,
    user_id: Uuid,
) -> Result<TraktAccountResponse, TraktError> {
    let account = load_account(&state.pool, user_id).await?;
    Ok(account_to_response(account))
}

pub async fn start_device_link(
    state: &AppState,
    _user_id: Uuid,
) -> Result<DeviceCodeResponse, TraktError> {
    let client = trakt_client(state)?;
    client.request_device_code().await
}

pub async fn poll_device_code(
    state: &AppState,
    user_id: Uuid,
    device_code: &str,
) -> Result<TraktAccountResponse, TraktError> {
    let client = trakt_client(state)?;

    let token = client.exchange_device_code(device_code).await?;

    let settings = client.get_user_settings(&token.access_token).await?;

    let account = upsert_account(state, user_id, &token, &settings).await?;

    Ok(account_to_response(Some(account)))
}

pub async fn unlink_account(state: &AppState, user_id: Uuid) -> Result<(), TraktError> {
    sqlx::query("DELETE FROM trakt_accounts WHERE user_id = $1")
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(TraktError::Database)?;
    Ok(())
}

pub async fn ensure_valid_token(
    state: &AppState,
    user_id: Uuid,
) -> Result<(String, TraktAccountRow), TraktError> {
    let mut account = load_account(&state.pool, user_id)
        .await?
        .ok_or(TraktError::AccountNotLinked)?;

    let (mut access_token, mut refresh_token, needs_storage_upgrade) =
        decrypt_account_tokens(state, &account)?;
    if needs_storage_upgrade {
        upgrade_legacy_token_storage(state, &account, &access_token, &refresh_token).await?;
        account = load_account(&state.pool, user_id)
            .await?
            .ok_or(TraktError::AccountNotLinked)?;
        (access_token, refresh_token, _) = decrypt_account_tokens(state, &account)?;
    }

    if !needs_token_refresh(account.token_expires_at, Utc::now()) {
        return Ok((access_token, account));
    }

    let client = trakt_client(state)?;
    let refreshed = client.refresh_token_pair(&refresh_token).await?;

    let new_expires_at = token_expires_at(&refreshed);
    update_tokens(state, user_id, &refreshed, new_expires_at).await?;

    let updated = load_account(&state.pool, user_id)
        .await?
        .ok_or(TraktError::AccountNotLinked)?;
    let (access_token, _, _) = decrypt_account_tokens(state, &updated)?;
    Ok((access_token, updated))
}

pub async fn get_sync_settings(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<SyncSettingsResponse, TraktError> {
    let row = sqlx::query(
        "SELECT sync_enabled, sync_watched, sync_collection, sync_ratings \
         FROM trakt_accounts WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(TraktError::Database)?;

    Ok(match row {
        Some(r) => SyncSettingsResponse {
            sync_enabled: r.get("sync_enabled"),
            sync_watched: r.get("sync_watched"),
            sync_collection: r.get("sync_collection"),
            sync_ratings: r.get("sync_ratings"),
        },
        None => SyncSettingsResponse {
            sync_enabled: false,
            sync_watched: false,
            sync_collection: false,
            sync_ratings: false,
        },
    })
}

pub async fn update_sync_settings(
    pool: &PgPool,
    user_id: Uuid,
    settings: &UpdateSyncSettingsRequest,
) -> Result<SyncSettingsResponse, TraktError> {
    let row = sqlx::query(
        "UPDATE trakt_accounts SET \
            sync_enabled = COALESCE($2, sync_enabled), \
            sync_watched = COALESCE($3, sync_watched), \
            sync_collection = COALESCE($4, sync_collection), \
            sync_ratings = COALESCE($5, sync_ratings), \
            updated_at = now() \
         WHERE user_id = $1 \
         RETURNING sync_enabled, sync_watched, sync_collection, sync_ratings",
    )
    .bind(user_id)
    .bind(settings.sync_enabled)
    .bind(settings.sync_watched)
    .bind(settings.sync_collection)
    .bind(settings.sync_ratings)
    .fetch_optional(pool)
    .await
    .map_err(TraktError::Database)?;

    Ok(match row {
        Some(r) => SyncSettingsResponse {
            sync_enabled: r.get("sync_enabled"),
            sync_watched: r.get("sync_watched"),
            sync_collection: r.get("sync_collection"),
            sync_ratings: r.get("sync_ratings"),
        },
        None => return Err(TraktError::AccountNotLinked),
    })
}

pub async fn trigger_sync(
    state: &AppState,
    user_id: Uuid,
) -> Result<SyncTriggerResponse, TraktError> {
    let summary = run_sync(state, user_id).await?;
    Ok(SyncTriggerResponse {
        completed: summary.completed,
        message: format!(
            "Sync complete — pulled {} watched, {} ratings, {} collection; pushed {} watched; {} unmatched",
            summary.pulled_watched,
            summary.pulled_ratings,
            summary.pulled_collection,
            summary.pushed_watched,
            summary.unmatched,
        ),
        summary,
    })
}

pub async fn run_sync(state: &AppState, user_id: Uuid) -> Result<SyncSummary, TraktError> {
    let started_at = Instant::now();
    let result = run_sync_once(state, user_id).await;
    record_sync_metrics(&result, started_at.elapsed());
    result
}

async fn run_sync_once(state: &AppState, user_id: Uuid) -> Result<SyncSummary, TraktError> {
    let account = load_account(&state.pool, user_id)
        .await?
        .ok_or(TraktError::AccountNotLinked)?;
    if !account.sync_enabled {
        return Err(TraktError::AccountNotLinked);
    }

    if !try_acquire_sync_lock(&state.trakt_sync_locks, user_id) {
        return Err(TraktError::SyncInProgress);
    }

    let result = run_sync_inner(state, user_id, &account).await;
    state.trakt_sync_locks.remove(&user_id);

    if let Err(error) = &result {
        record_sync_failure(&state.pool, user_id, error).await?;
    }

    result
}

fn record_sync_metrics(result: &Result<SyncSummary, TraktError>, duration: std::time::Duration) {
    let outcome = sync_metric_outcome(result);
    metrics::counter!("trakt_sync_operations_total", "outcome" => outcome).increment(1);
    metrics::histogram!("trakt_sync_duration_seconds", "outcome" => outcome)
        .record(duration.as_secs_f64());
    if let Err(error) = result {
        metrics::counter!("trakt_sync_errors_total", "code" => sync_error_code(error)).increment(1);
    }
}

fn sync_metric_outcome(result: &Result<SyncSummary, TraktError>) -> &'static str {
    match result {
        Ok(_) => "success",
        Err(TraktError::SyncInProgress | TraktError::AccountNotLinked) => "skipped",
        Err(_) => "failure",
    }
}

async fn run_sync_inner(
    state: &AppState,
    user_id: Uuid,
    account: &TraktAccountRow,
) -> Result<SyncSummary, TraktError> {
    let (access_token, _) = ensure_valid_token(state, user_id).await?;
    let client = trakt_client(state)?;
    let matcher = build_media_matcher(&state.pool).await?;

    let mut summary = SyncSummary::default();

    if account.sync_watched {
        match pull_watched(
            state,
            user_id,
            &client,
            &access_token,
            &matcher,
            &mut summary,
        )
        .await
        {
            Ok(()) => {}
            Err(error @ TraktError::RateLimited { .. }) => {
                tracing::warn!(user_id = %user_id, "Trakt rate limited during watched pull; aborting sync");
                return Err(error);
            }
            Err(e) => return Err(e),
        }
    }

    if account.sync_ratings {
        pull_ratings(
            state,
            user_id,
            &client,
            &access_token,
            &matcher,
            &mut summary,
        )
        .await?;
    }

    if account.sync_collection {
        pull_collection(
            state,
            user_id,
            &client,
            &access_token,
            &matcher,
            &mut summary,
        )
        .await?;
    }

    if account.sync_watched {
        match push_local_watched(&state.pool, &client, &access_token, user_id).await {
            Ok(pushed) => summary.pushed_watched = pushed,
            Err(error @ TraktError::RateLimited { .. }) => {
                tracing::warn!(user_id = %user_id, "Trakt rate limited during watched push; aborting sync");
                return Err(error);
            }
            Err(e) => return Err(e),
        }
    }

    sqlx::query(
        "UPDATE trakt_accounts SET \
            last_full_sync_at = now(), \
            last_sync_attempt_at = now(), \
            last_sync_error = NULL, \
            updated_at = now() \
         WHERE user_id = $1",
    )
    .bind(user_id)
    .execute(&state.pool)
    .await
    .map_err(TraktError::Database)?;

    summary.completed = true;
    summary.last_full_sync_at = Some(Utc::now());

    tracing::info!(
        user_id = %user_id,
        pulled_watched = summary.pulled_watched,
        pulled_ratings = summary.pulled_ratings,
        pulled_collection = summary.pulled_collection,
        pushed_watched = summary.pushed_watched,
        unmatched = summary.unmatched,
        "Trakt sync complete"
    );

    Ok(summary)
}

async fn pull_watched(
    state: &AppState,
    user_id: Uuid,
    client: &TraktClient,
    access_token: &str,
    matcher: &MediaMatcher,
    summary: &mut SyncSummary,
) -> Result<(), TraktError> {
    let mut tx = state.pool.begin().await.map_err(TraktError::Database)?;
    let mut matched = 0u64;
    let mut unmatched = 0u64;

    let movies = client.get_watched_movies(access_token).await?;
    for item in &movies {
        if let Some(media_item_id) = matcher.find("movie", &item.movie.ids) {
            upsert_sync_watched(
                &mut tx,
                user_id,
                media_item_id,
                &item.movie.ids,
                item.plays,
                &item.last_watched_at,
            )
            .await?;
            apply_uid_watched(
                &mut tx,
                user_id,
                media_item_id,
                item.plays,
                &item.last_watched_at,
            )
            .await?;
            matched += 1;
        } else {
            unmatched += 1;
        }
    }

    let episodes = client.get_watched_episodes(access_token).await?;
    for item in &episodes {
        if let Some(media_item_id) = matcher.find("episode", &item.episode.ids) {
            upsert_sync_watched(
                &mut tx,
                user_id,
                media_item_id,
                &item.episode.ids,
                item.plays,
                &item.last_watched_at,
            )
            .await?;
            apply_uid_watched(
                &mut tx,
                user_id,
                media_item_id,
                item.plays,
                &item.last_watched_at,
            )
            .await?;
            matched += 1;
        } else {
            unmatched += 1;
        }
    }

    tx.commit().await.map_err(TraktError::Database)?;
    summary.pulled_watched = matched as i64;
    summary.unmatched += unmatched as i64;
    Ok(())
}

async fn pull_ratings(
    state: &AppState,
    user_id: Uuid,
    client: &TraktClient,
    access_token: &str,
    matcher: &MediaMatcher,
    summary: &mut SyncSummary,
) -> Result<(), TraktError> {
    let mut tx = state.pool.begin().await.map_err(TraktError::Database)?;
    let mut matched = 0u64;
    let mut unmatched = 0u64;

    for media_type in ["movies", "shows", "seasons", "episodes"] {
        let ratings = client.get_ratings(access_token, media_type).await?;
        for r in &ratings {
            let Some(rating) = r.rating else { continue };
            let Some((item_type, ids)) = rating_target(r) else {
                continue;
            };
            let Some(media_item_id) = matcher.find(item_type, ids) else {
                unmatched += 1;
                continue;
            };
            upsert_sync_rating(&mut tx, user_id, media_item_id, ids, rating, &r.rated_at).await?;
            apply_uid_rating(&mut tx, user_id, media_item_id, rating).await?;
            matched += 1;
        }
    }

    tx.commit().await.map_err(TraktError::Database)?;
    summary.pulled_ratings = matched as i64;
    summary.unmatched += unmatched as i64;
    Ok(())
}

async fn pull_collection(
    state: &AppState,
    user_id: Uuid,
    client: &TraktClient,
    access_token: &str,
    matcher: &MediaMatcher,
    summary: &mut SyncSummary,
) -> Result<(), TraktError> {
    let mut tx = state.pool.begin().await.map_err(TraktError::Database)?;
    let mut matched = 0u64;
    let mut unmatched = 0u64;

    let items = client.get_collection_movies(access_token).await?;
    for item in &items {
        if let Some(media_item_id) = matcher.find("movie", &item.movie.ids) {
            upsert_sync_collection(
                &mut tx,
                user_id,
                media_item_id,
                &item.movie.ids,
                &item.collected_at,
            )
            .await?;
            matched += 1;
        } else {
            unmatched += 1;
        }
    }

    tx.commit().await.map_err(TraktError::Database)?;
    summary.pulled_collection = matched as i64;
    summary.unmatched += unmatched as i64;
    Ok(())
}

async fn push_local_watched(
    pool: &PgPool,
    client: &TraktClient,
    access_token: &str,
    user_id: Uuid,
) -> Result<i64, TraktError> {
    let rows = sqlx::query(
        "SELECT uid.media_item_id, mi.type, mi.trakt_id, mi.tmdb_id, mi.imdb_id, mi.tvdb_id, uid.last_played_at \
         FROM user_item_data uid \
         JOIN media_items mi ON mi.id = uid.media_item_id \
         LEFT JOIN trakt_sync_state tss ON tss.user_id = uid.user_id AND tss.media_item_id = uid.media_item_id \
         WHERE uid.user_id = $1 AND uid.is_watched = true \
         AND (tss.is_watched IS DISTINCT FROM true) \
         AND (mi.trakt_id IS NOT NULL OR mi.tmdb_id IS NOT NULL OR mi.imdb_id IS NOT NULL OR mi.tvdb_id IS NOT NULL)",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(TraktError::Database)?;

    let mut movies: Vec<serde_json::Value> = Vec::new();
    let mut episodes: Vec<serde_json::Value> = Vec::new();
    let mut pushed_ids: Vec<Uuid> = Vec::new();

    for row in &rows {
        let media_item_id: Uuid = row.get("media_item_id");
        let typ: String = row.get("type");
        let ids = row_ids_to_value(row);
        let watched_at: Option<DateTime<Utc>> = row
            .try_get::<Option<DateTime<Utc>>, _>("last_played_at")
            .unwrap_or(None);
        let entry = serde_json::json!({
            "ids": ids,
            "watched_at": watched_at.map(|d| d.to_rfc3339())
        });
        match typ.as_str() {
            "movie" => {
                movies.push(entry);
                pushed_ids.push(media_item_id);
            }
            "episode" => {
                episodes.push(entry);
                pushed_ids.push(media_item_id);
            }
            _ => {}
        }
    }

    if movies.is_empty() && episodes.is_empty() {
        return Ok(0);
    }

    let body = serde_json::json!({ "movies": movies, "episodes": episodes });
    let resp = client.add_to_history(access_token, &body).await?;
    let added = resp.added.total();
    tracing::info!(
        user_id = %user_id,
        added = added,
        had_not_found = resp.not_found.is_some(),
        "Pushed local watched state to Trakt"
    );
    if let Some(nf) = resp.not_found
        && has_not_found_items(&nf)
    {
        tracing::warn!(user_id = %user_id, not_found = %nf, "Trakt add_to_history reported not_found entries");
        return Err(TraktError::SyncIncomplete);
    }

    mark_pushed_as_synced(pool, user_id, &pushed_ids).await?;
    Ok(added)
}

pub async fn get_sync_status(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<SyncStatusResponse, TraktError> {
    let account = sqlx::query(
        "SELECT last_full_sync_at, last_sync_attempt_at, last_sync_error \
         FROM trakt_accounts WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(TraktError::Database)?;

    let (last_full_sync_at, last_sync_attempt_at, last_error) = match account {
        Some(r) => (
            r.try_get::<Option<DateTime<Utc>>, _>("last_full_sync_at")
                .unwrap_or(None),
            r.try_get::<Option<DateTime<Utc>>, _>("last_sync_attempt_at")
                .unwrap_or(None),
            r.try_get::<Option<String>, _>("last_sync_error")
                .unwrap_or(None),
        ),
        None => return Err(TraktError::AccountNotLinked),
    };

    let row = sqlx::query(
        "SELECT \
            COUNT(*) AS total, \
            COUNT(*) FILTER (WHERE is_watched) AS watched, \
            COUNT(*) FILTER (WHERE is_in_watchlist) AS watchlist, \
            COUNT(*) FILTER (WHERE is_in_collection) AS collection, \
            COUNT(*) FILTER (WHERE rating IS NOT NULL) AS rated \
         FROM trakt_sync_state WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(TraktError::Database)?;

    Ok(SyncStatusResponse {
        last_full_sync_at,
        total_items: row.get("total"),
        watched_count: row.get("watched"),
        watchlist_count: row.get("watchlist"),
        collection_count: row.get("collection"),
        rated_count: row.get("rated"),
        last_error,
        last_sync_attempt_at,
    })
}

pub async fn list_history(
    pool: &PgPool,
    user_id: Uuid,
    query: &HistoryQuery,
) -> Result<TraktHistoryResponse, TraktError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(25).clamp(1, 100);
    let offset = ((page - 1) * page_size) as i64;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trakt_sync_state WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .map_err(TraktError::Database)?;

    let rows = sqlx::query(
        "SELECT media_item_id, trakt_id, is_watched, watched_at, plays, \
                is_in_watchlist, is_in_collection, rating, rated_at, synced_at \
         FROM trakt_sync_state WHERE user_id = $1 \
         ORDER BY synced_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(user_id)
    .bind(page_size as i64)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(TraktError::Database)?;

    let items: Vec<TraktHistoryItem> = rows.iter().map(row_to_history_item).collect();
    let total_pages = (total as u64).div_ceil(page_size as u64) as u32;

    Ok(TraktHistoryResponse {
        items,
        total,
        page,
        page_size,
        total_pages: total_pages.max(1),
    })
}

pub async fn list_ratings(
    pool: &PgPool,
    user_id: Uuid,
    query: &HistoryQuery,
) -> Result<TraktHistoryResponse, TraktError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(25).clamp(1, 100);
    let offset = ((page - 1) * page_size) as i64;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM trakt_sync_state WHERE user_id = $1 AND rating IS NOT NULL",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(TraktError::Database)?;

    let rows = sqlx::query(
        "SELECT media_item_id, trakt_id, is_watched, watched_at, plays, \
                is_in_watchlist, is_in_collection, rating, rated_at, synced_at \
         FROM trakt_sync_state WHERE user_id = $1 AND rating IS NOT NULL \
         ORDER BY rated_at DESC NULLS LAST, synced_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(user_id)
    .bind(page_size as i64)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(TraktError::Database)?;

    let items: Vec<TraktHistoryItem> = rows.iter().map(row_to_history_item).collect();
    let total_pages = (total as u64).div_ceil(page_size as u64) as u32;

    Ok(TraktHistoryResponse {
        items,
        total,
        page,
        page_size,
        total_pages: total_pages.max(1),
    })
}

pub async fn get_settings(state: &AppState) -> Result<TraktSettingsResponse, TraktError> {
    let config = state.runtime_config.load();
    let trakt = &config.integrations.trakt;
    Ok(TraktSettingsResponse {
        client_id: trakt.client_id.clone(),
        client_secret_masked: crate::services::encryption::mask_secret(&trakt.client_secret),
        has_client_secret: !trakt.client_secret.is_empty(),
        redirect_uri: trakt.redirect_uri.clone(),
        is_configured: trakt.is_configured(),
    })
}

pub async fn update_settings(
    state: &AppState,
    req: &UpdateTraktSettingsRequest,
) -> Result<TraktSettingsResponse, TraktError> {
    let config = state.runtime_config.load();
    let mut trakt = config.integrations.trakt.clone();
    drop(config);

    if let Some(ref id) = req.client_id {
        trakt.client_id = id.clone();
    }
    if let Some(ref secret) = req.client_secret {
        trakt.client_secret = secret.clone();
    }
    if let Some(ref uri) = req.redirect_uri {
        if uri.is_empty() {
            return Err(TraktError::NotConfigured);
        }
        trakt.redirect_uri = uri.clone();
    }

    crate::services::encryption::encrypt_trakt_config(&mut trakt, &state.encryption_key);

    let json = serde_json::to_value(&trakt)
        .map_err(|e| TraktError::Database(sqlx::Error::Configuration(e.into())))?;

    sqlx::query(
        "UPDATE server_config SET integrations = jsonb_set(integrations, '{trakt}', $1::jsonb)",
    )
    .bind(json)
    .execute(&state.pool)
    .await
    .map_err(TraktError::Database)?;

    reload_runtime_config(state).await?;
    get_settings(state).await
}

async fn reload_runtime_config(state: &AppState) -> Result<(), TraktError> {
    let reloaded =
        crate::state::load_runtime_config(&state.pool, Some(&state.encryption_key)).await?;
    state.runtime_config.store(std::sync::Arc::new(reloaded));
    Ok(())
}

fn trakt_client(state: &AppState) -> Result<TraktClient, TraktError> {
    let config = state.runtime_config.load();
    if !config.integrations.trakt.is_configured() {
        return Err(TraktError::NotConfigured);
    }
    Ok(TraktClient::new(
        config.integrations.trakt.client_id.clone(),
        config.integrations.trakt.client_secret.clone(),
        config.integrations.trakt.redirect_uri.clone(),
    ))
}

async fn load_account(pool: &PgPool, user_id: Uuid) -> Result<Option<TraktAccountRow>, TraktError> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, trakt_username, trakt_user_id,
               access_token, refresh_token, token_expires_at, token_scope,
               last_full_sync_at, last_sync_attempt_at, last_sync_error,
               sync_enabled, sync_watched,
               sync_collection, sync_ratings, created_at, updated_at
        FROM trakt_accounts
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(TraktError::Database)?;

    Ok(row.map(|r| row_to_account(&r)))
}

async fn upsert_account(
    state: &AppState,
    user_id: Uuid,
    token: &TraktTokenResponse,
    settings: &TraktUserSettings,
) -> Result<TraktAccountRow, TraktError> {
    let expires_at = token_expires_at(token);
    let (access_token, refresh_token) = encrypt_token_pair(
        &state.encryption_key,
        &token.access_token,
        &token.refresh_token,
    )?;

    let row = sqlx::query(
        r#"
        INSERT INTO trakt_accounts (
            user_id, trakt_username, trakt_user_id,
            access_token, refresh_token, token_expires_at, token_scope,
            sync_enabled, sync_watched, sync_collection, sync_ratings
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, true, true, true, true)
        ON CONFLICT (user_id) DO UPDATE SET
            trakt_username = EXCLUDED.trakt_username,
            trakt_user_id = EXCLUDED.trakt_user_id,
            access_token = EXCLUDED.access_token,
            refresh_token = EXCLUDED.refresh_token,
            token_expires_at = EXCLUDED.token_expires_at,
            token_scope = EXCLUDED.token_scope,
            updated_at = now()
        RETURNING id, user_id, trakt_username, trakt_user_id,
                  access_token, refresh_token, token_expires_at, token_scope,
                  last_full_sync_at, last_sync_attempt_at, last_sync_error,
                  sync_enabled, sync_watched,
                  sync_collection, sync_ratings, created_at, updated_at
        "#,
    )
    .bind(user_id)
    .bind(&settings.user.username)
    .bind(settings.account.id)
    .bind(access_token)
    .bind(refresh_token)
    .bind(expires_at)
    .bind(token.scope.as_deref().filter(|s| !s.is_empty()))
    .fetch_one(&state.pool)
    .await
    .map_err(TraktError::Database)?;

    Ok(row_to_account(&row))
}

async fn update_tokens(
    state: &AppState,
    user_id: Uuid,
    token: &TraktTokenResponse,
    expires_at: DateTime<Utc>,
) -> Result<(), TraktError> {
    let (access_token, refresh_token) = encrypt_token_pair(
        &state.encryption_key,
        &token.access_token,
        &token.refresh_token,
    )?;

    sqlx::query(
        r#"
        UPDATE trakt_accounts SET
            access_token = $2,
            refresh_token = $3,
            token_expires_at = $4,
            token_scope = COALESCE($5, token_scope),
            updated_at = now()
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .bind(access_token)
    .bind(refresh_token)
    .bind(expires_at)
    .bind(token.scope.as_deref().filter(|s| !s.is_empty()))
    .execute(&state.pool)
    .await
    .map_err(TraktError::Database)?;
    Ok(())
}

fn token_expires_at(token: &TraktTokenResponse) -> DateTime<Utc> {
    Utc::now() + Duration::seconds(token.expires_in.max(0))
}

fn needs_token_refresh(expires_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    expires_at <= now + TOKEN_REFRESH_BUFFER
}

fn encrypt_token_pair(
    key: &crate::services::encryption::EncryptionKey,
    access_token: &str,
    refresh_token: &str,
) -> Result<(String, String), TraktError> {
    let access_token = key
        .encrypt(access_token)
        .map_err(|_| TraktError::TokenStorage)?;
    let refresh_token = key
        .encrypt(refresh_token)
        .map_err(|_| TraktError::TokenStorage)?;
    Ok((access_token, refresh_token))
}

fn decrypt_account_tokens(
    state: &AppState,
    account: &TraktAccountRow,
) -> Result<(String, String, bool), TraktError> {
    let access_is_encrypted = account
        .access_token
        .starts_with(crate::services::encryption::ENCRYPTED_PREFIX);
    let refresh_is_encrypted = account
        .refresh_token
        .starts_with(crate::services::encryption::ENCRYPTED_PREFIX);
    let access_token = if access_is_encrypted {
        state
            .encryption_key
            .decrypt(&account.access_token)
            .map_err(|_| TraktError::TokenStorage)?
    } else {
        account.access_token.clone()
    };
    let refresh_token = if refresh_is_encrypted {
        state
            .encryption_key
            .decrypt(&account.refresh_token)
            .map_err(|_| TraktError::TokenStorage)?
    } else {
        account.refresh_token.clone()
    };
    Ok((
        access_token,
        refresh_token,
        !access_is_encrypted || !refresh_is_encrypted,
    ))
}

async fn upgrade_legacy_token_storage(
    state: &AppState,
    account: &TraktAccountRow,
    access_token: &str,
    refresh_token: &str,
) -> Result<(), TraktError> {
    let (encrypted_access, encrypted_refresh) =
        encrypt_token_pair(&state.encryption_key, access_token, refresh_token)?;
    sqlx::query(
        "UPDATE trakt_accounts SET access_token = $2, refresh_token = $3, updated_at = now() \
         WHERE id = $1 AND access_token = $4 AND refresh_token = $5",
    )
    .bind(account.id)
    .bind(encrypted_access)
    .bind(encrypted_refresh)
    .bind(&account.access_token)
    .bind(&account.refresh_token)
    .execute(&state.pool)
    .await
    .map_err(TraktError::Database)?;
    Ok(())
}

async fn record_sync_failure(
    pool: &PgPool,
    user_id: Uuid,
    error: &TraktError,
) -> Result<(), TraktError> {
    sqlx::query(
        "UPDATE trakt_accounts SET last_sync_attempt_at = now(), last_sync_error = $2, updated_at = now() \
         WHERE user_id = $1",
    )
    .bind(user_id)
    .bind(sync_error_code(error))
    .execute(pool)
    .await
    .map_err(TraktError::Database)?;
    Ok(())
}

fn sync_error_code(error: &TraktError) -> &'static str {
    match error {
        TraktError::AccountNotLinked => "TRAKT_001",
        TraktError::RateLimited { .. } => "TRAKT_002",
        TraktError::TokenExpired => "TRAKT_003",
        TraktError::ServiceUnavailable => "TRAKT_004",
        TraktError::Timeout => "TRAKT_005",
        TraktError::SyncIncomplete => "TRAKT_006",
        TraktError::TokenStorage => "TRAKT_007",
        TraktError::DeviceCodeExpired => "DEVICE_CODE_EXPIRED",
        TraktError::DeviceCodePending => "DEVICE_CODE_PENDING",
        TraktError::DeviceCodeDenied => "DEVICE_CODE_DENIED",
        TraktError::SyncInProgress => "SYNC_IN_PROGRESS",
        TraktError::NotConfigured => "NOT_CONFIGURED",
        TraktError::Database(_) => "DATABASE_ERROR",
    }
}

fn row_to_account(row: &sqlx::postgres::PgRow) -> TraktAccountRow {
    TraktAccountRow {
        id: row.get("id"),
        user_id: row.get("user_id"),
        trakt_username: row.get("trakt_username"),
        trakt_user_id: row.get("trakt_user_id"),
        access_token: row.get("access_token"),
        refresh_token: row.get("refresh_token"),
        token_expires_at: row.get("token_expires_at"),
        token_scope: row.get("token_scope"),
        last_full_sync_at: row.get("last_full_sync_at"),
        last_sync_attempt_at: row.get("last_sync_attempt_at"),
        last_sync_error: row.get("last_sync_error"),
        sync_enabled: row.get("sync_enabled"),
        sync_watched: row.get("sync_watched"),
        sync_collection: row.get("sync_collection"),
        sync_ratings: row.get("sync_ratings"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn account_to_response(account: Option<TraktAccountRow>) -> TraktAccountResponse {
    match account {
        Some(a) => TraktAccountResponse {
            linked: true,
            trakt_username: Some(a.trakt_username),
            trakt_user_id: Some(a.trakt_user_id),
            token_expires_at: Some(a.token_expires_at),
            sync_enabled: a.sync_enabled,
            sync_watched: a.sync_watched,
            sync_collection: a.sync_collection,
            sync_ratings: a.sync_ratings,
            last_full_sync_at: a.last_full_sync_at,
            last_sync_attempt_at: a.last_sync_attempt_at,
            last_sync_error: a.last_sync_error,
        },
        None => TraktAccountResponse {
            linked: false,
            trakt_username: None,
            trakt_user_id: None,
            token_expires_at: None,
            sync_enabled: false,
            sync_watched: false,
            sync_collection: false,
            sync_ratings: false,
            last_full_sync_at: None,
            last_sync_attempt_at: None,
            last_sync_error: None,
        },
    }
}

#[derive(Default)]
struct MediaMatcher {
    by_trakt: HashMap<(String, i64), Uuid>,
    by_tmdb: HashMap<(String, i64), Uuid>,
    by_imdb: HashMap<(String, String), Uuid>,
    by_tvdb: HashMap<(String, i64), Uuid>,
}

impl MediaMatcher {
    fn find(&self, item_type: &str, ids: &TraktIds) -> Option<Uuid> {
        if let Some(t) = ids.trakt
            && let Some(id) = self.by_trakt.get(&(item_type.to_string(), t))
        {
            return Some(*id);
        }
        if let Some(t) = ids.tmdb
            && let Some(id) = self.by_tmdb.get(&(item_type.to_string(), t))
        {
            return Some(*id);
        }
        if let Some(ref t) = ids.imdb
            && !t.is_empty()
            && let Some(id) = self.by_imdb.get(&(item_type.to_string(), t.clone()))
        {
            return Some(*id);
        }
        if let Some(t) = ids.tvdb
            && let Some(id) = self.by_tvdb.get(&(item_type.to_string(), t))
        {
            return Some(*id);
        }
        None
    }
}

async fn build_media_matcher(pool: &PgPool) -> Result<MediaMatcher, TraktError> {
    let rows = sqlx::query("SELECT id, type, trakt_id, tmdb_id, imdb_id, tvdb_id FROM media_items")
        .fetch_all(pool)
        .await
        .map_err(TraktError::Database)?;

    let mut matcher = MediaMatcher::default();
    for row in &rows {
        let id: Uuid = row.get("id");
        let typ: String = row.get("type");
        if let Ok(Some(v)) = row.try_get::<Option<i64>, _>("trakt_id") {
            matcher.by_trakt.insert((typ.clone(), v), id);
        }
        if let Ok(Some(v)) = row.try_get::<Option<i64>, _>("tmdb_id") {
            matcher.by_tmdb.insert((typ.clone(), v), id);
        }
        if let Ok(Some(v)) = row.try_get::<Option<String>, _>("imdb_id")
            && !v.is_empty()
        {
            matcher.by_imdb.insert((typ.clone(), v), id);
        }
        if let Ok(Some(v)) = row.try_get::<Option<i64>, _>("tvdb_id") {
            matcher.by_tvdb.insert((typ, v), id);
        }
    }
    Ok(matcher)
}

fn rating_target(rating: &TraktRating) -> Option<(&'static str, &TraktIds)> {
    match rating.rating_type.as_deref() {
        Some("movie") => rating.movie.as_ref().map(|m| ("movie", &m.ids)),
        Some("show") => rating.show.as_ref().map(|s| ("series", &s.ids)),
        Some("episode") => rating.episode.as_ref().map(|e| ("episode", &e.ids)),
        Some("season") => rating.season.as_ref().map(|s| ("season", &s.ids)),
        _ => None,
    }
}

async fn upsert_sync_watched(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    media_item_id: Uuid,
    ids: &TraktIds,
    plays: Option<i32>,
    watched_at: &Option<String>,
) -> Result<(), TraktError> {
    let plays_i = plays.unwrap_or(0).max(0);
    let watched_at_ts = watched_at
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc));
    sqlx::query(
        "INSERT INTO trakt_sync_state \
            (id, user_id, media_item_id, trakt_id, is_watched, watched_at, plays, synced_at) \
         VALUES (uuidv7(), $1, $2, $3, true, $4, $5, now()) \
         ON CONFLICT (user_id, media_item_id) DO UPDATE SET \
            trakt_id = COALESCE(EXCLUDED.trakt_id, trakt_sync_state.trakt_id), \
            is_watched = true, \
            watched_at = COALESCE(EXCLUDED.watched_at, trakt_sync_state.watched_at), \
            plays = GREATEST(trakt_sync_state.plays, EXCLUDED.plays), \
            synced_at = now()",
    )
    .bind(user_id)
    .bind(media_item_id)
    .bind(ids.trakt)
    .bind(watched_at_ts)
    .bind(plays_i)
    .execute(&mut **tx)
    .await
    .map_err(TraktError::Database)?;
    Ok(())
}

async fn apply_uid_watched(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    media_item_id: Uuid,
    plays: Option<i32>,
    watched_at: &Option<String>,
) -> Result<(), TraktError> {
    let plays_i = plays.unwrap_or(1).max(1);
    let watched_at_ts = watched_at
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc));
    sqlx::query(
        "INSERT INTO user_item_data \
            (id, user_id, media_item_id, is_watched, play_count, last_played_at, resume_position_ms) \
         VALUES (uuidv7(), $1, $2, true, $3, $4, 0) \
         ON CONFLICT (user_id, media_item_id) DO UPDATE SET \
            is_watched = true, \
            play_count = GREATEST(user_item_data.play_count, EXCLUDED.play_count), \
            last_played_at = GREATEST(user_item_data.last_played_at, EXCLUDED.last_played_at), \
            resume_position_ms = 0, \
            updated_at = now()",
    )
    .bind(user_id)
    .bind(media_item_id)
    .bind(plays_i)
    .bind(watched_at_ts)
    .execute(&mut **tx)
    .await
    .map_err(TraktError::Database)?;
    Ok(())
}

async fn upsert_sync_rating(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    media_item_id: Uuid,
    ids: &TraktIds,
    rating: i32,
    rated_at: &Option<String>,
) -> Result<(), TraktError> {
    let rated_at_ts = rated_at
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc));
    sqlx::query(
        "INSERT INTO trakt_sync_state \
            (id, user_id, media_item_id, trakt_id, rating, rated_at, synced_at) \
         VALUES (uuidv7(), $1, $2, $3, $4, $5, now()) \
         ON CONFLICT (user_id, media_item_id) DO UPDATE SET \
            trakt_id = COALESCE(EXCLUDED.trakt_id, trakt_sync_state.trakt_id), \
            rating = EXCLUDED.rating, \
            rated_at = EXCLUDED.rated_at, \
            synced_at = now()",
    )
    .bind(user_id)
    .bind(media_item_id)
    .bind(ids.trakt)
    .bind(rating)
    .bind(rated_at_ts)
    .execute(&mut **tx)
    .await
    .map_err(TraktError::Database)?;
    Ok(())
}

async fn apply_uid_rating(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    media_item_id: Uuid,
    rating: i32,
) -> Result<(), TraktError> {
    sqlx::query(
        "INSERT INTO user_item_data (id, user_id, media_item_id, user_rating) \
         VALUES (uuidv7(), $1, $2, $3) \
         ON CONFLICT (user_id, media_item_id) DO UPDATE SET \
            user_rating = COALESCE(user_item_data.user_rating, EXCLUDED.user_rating), \
            updated_at = now()",
    )
    .bind(user_id)
    .bind(media_item_id)
    .bind(rating)
    .execute(&mut **tx)
    .await
    .map_err(TraktError::Database)?;
    Ok(())
}

async fn upsert_sync_collection(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    media_item_id: Uuid,
    ids: &TraktIds,
    collected_at: &Option<String>,
) -> Result<(), TraktError> {
    let collected_at_ts = collected_at
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc));
    sqlx::query(
        "INSERT INTO trakt_sync_state \
            (id, user_id, media_item_id, trakt_id, is_in_collection, collected_at, synced_at) \
         VALUES (uuidv7(), $1, $2, $3, true, $4, now()) \
         ON CONFLICT (user_id, media_item_id) DO UPDATE SET \
            trakt_id = COALESCE(EXCLUDED.trakt_id, trakt_sync_state.trakt_id), \
            is_in_collection = true, \
            collected_at = EXCLUDED.collected_at, \
            synced_at = now()",
    )
    .bind(user_id)
    .bind(media_item_id)
    .bind(ids.trakt)
    .bind(collected_at_ts)
    .execute(&mut **tx)
    .await
    .map_err(TraktError::Database)?;
    Ok(())
}

async fn mark_pushed_as_synced(
    pool: &PgPool,
    user_id: Uuid,
    media_item_ids: &[Uuid],
) -> Result<(), TraktError> {
    if media_item_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO trakt_sync_state (id, user_id, media_item_id, trakt_id, is_watched, synced_at) \
         SELECT uuidv7(), $1, t.media_item_id, mi.trakt_id, true, now() \
         FROM unnest($2::uuid[]) AS t(media_item_id) \
         JOIN media_items mi ON mi.id = t.media_item_id \
         ON CONFLICT (user_id, media_item_id) DO UPDATE SET \
            is_watched = true, \
            synced_at = now()",
    )
    .bind(user_id)
    .bind(media_item_ids)
    .execute(pool)
    .await
    .map_err(TraktError::Database)?;
    Ok(())
}

fn row_ids_to_value(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    let ids = TraktIds {
        trakt: row.try_get::<Option<i64>, _>("trakt_id").unwrap_or(None),
        slug: None,
        imdb: row.try_get::<Option<String>, _>("imdb_id").unwrap_or(None),
        tmdb: row.try_get::<Option<i64>, _>("tmdb_id").unwrap_or(None),
        tvdb: row.try_get::<Option<i64>, _>("tvdb_id").unwrap_or(None),
    };
    ids.to_id_object()
}

fn has_not_found_items(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Array(items) => !items.is_empty(),
        serde_json::Value::Object(items) => items.values().any(has_not_found_items),
        serde_json::Value::Null => false,
        _ => true,
    }
}

fn row_to_history_item(row: &sqlx::postgres::PgRow) -> TraktHistoryItem {
    TraktHistoryItem {
        media_item_id: row.get("media_item_id"),
        trakt_id: row.try_get::<Option<i64>, _>("trakt_id").unwrap_or(None),
        is_watched: row.get("is_watched"),
        watched_at: row.get("watched_at"),
        plays: row.get("plays"),
        is_in_watchlist: row.get("is_in_watchlist"),
        is_in_collection: row.get("is_in_collection"),
        rating: row.get("rating"),
        rated_at: row.get("rated_at"),
        synced_at: row.get("synced_at"),
    }
}

fn try_acquire_sync_lock(locks: &dashmap::DashMap<Uuid, Instant>, user_id: Uuid) -> bool {
    let now = Instant::now();
    match locks.entry(user_id) {
        Entry::Occupied(mut entry) => {
            if now.duration_since(*entry.get()) < SYNC_LOCK_TTL {
                return false;
            }
            *entry.get_mut() = now;
            true
        }
        Entry::Vacant(entry) => {
            entry.insert(now);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use serde_json::json;

    use super::*;

    #[test]
    fn token_refreshes_before_the_expiry_buffer_elapses() {
        let now = Utc::now();

        assert!(needs_token_refresh(now - Duration::seconds(1), now));
        assert!(needs_token_refresh(now + TOKEN_REFRESH_BUFFER, now));
        assert!(needs_token_refresh(
            now + TOKEN_REFRESH_BUFFER - Duration::seconds(1),
            now
        ));
        assert!(!needs_token_refresh(
            now + TOKEN_REFRESH_BUFFER + Duration::seconds(1),
            now
        ));
    }

    #[test]
    fn token_pair_encryption_round_trips_both_tokens() {
        let (key, _) = crate::services::encryption::EncryptionKey::generate();
        let (access, refresh) = encrypt_token_pair(&key, "access-token", "refresh-token")
            .expect("token encryption should succeed");

        assert!(access.starts_with(crate::services::encryption::ENCRYPTED_PREFIX));
        assert!(refresh.starts_with(crate::services::encryption::ENCRYPTED_PREFIX));
        assert_eq!(key.decrypt(&access).unwrap(), "access-token");
        assert_eq!(key.decrypt(&refresh).unwrap(), "refresh-token");
    }

    #[test]
    fn not_found_detection_requires_an_unconfirmed_item() {
        assert!(!has_not_found_items(&json!(null)));
        assert!(!has_not_found_items(&json!({"movies": [], "shows": []})));
        assert!(has_not_found_items(
            &json!({"movies": [{"title": "Missing"}]})
        ));
    }

    #[test]
    fn sync_error_codes_are_safe_to_persist() {
        assert_eq!(sync_error_code(&TraktError::SyncIncomplete), "TRAKT_006");
        assert_eq!(sync_error_code(&TraktError::TokenStorage), "TRAKT_007");
        assert_eq!(
            sync_error_code(&TraktError::Database(sqlx::Error::PoolClosed)),
            "DATABASE_ERROR"
        );
    }

    #[test]
    fn sync_metric_outcomes_remain_low_cardinality() {
        assert_eq!(sync_metric_outcome(&Ok(SyncSummary::default())), "success");
        assert_eq!(
            sync_metric_outcome(&Err(TraktError::SyncInProgress)),
            "skipped"
        );
        assert_eq!(
            sync_metric_outcome(&Err(TraktError::RateLimited {
                retry_after_secs: None
            })),
            "failure"
        );
    }
}
