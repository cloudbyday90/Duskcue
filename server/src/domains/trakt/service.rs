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

use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::services::trakt_client::{TraktClient, TraktTokenResponse, TraktUserSettings};
use crate::state::AppState;

use crate::domains::trakt::error::TraktError;
use crate::domains::trakt::types::*;

const TOKEN_REFRESH_BUFFER: Duration = Duration::minutes(5);

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

    let account = upsert_account(&state.pool, user_id, &token, &settings).await?;

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
    let account =
        load_account(&state.pool, user_id).await?.ok_or(TraktError::AccountNotLinked)?;

    let buffer_ago = Utc::now() - TOKEN_REFRESH_BUFFER;
    if account.token_expires_at > buffer_ago {
        return Ok((account.access_token.clone(), account));
    }

    let client = trakt_client(state)?;
    let refreshed = client.refresh_token_pair(&account.refresh_token).await?;

    let new_expires_at = token_expires_at(&refreshed);
    update_tokens(&state.pool, user_id, &refreshed, new_expires_at).await?;

    let updated = load_account(&state.pool, user_id)
        .await?
        .ok_or(TraktError::AccountNotLinked)?;
    Ok((updated.access_token.clone(), updated))
}

pub async fn get_sync_settings(
    _pool: &PgPool,
    _user_id: Uuid,
) -> Result<SyncSettingsResponse, TraktError> {
    todo!("Phase 11 Task 5")
}

pub async fn update_sync_settings(
    _pool: &PgPool,
    _user_id: Uuid,
    _settings: &UpdateSyncSettingsRequest,
) -> Result<SyncSettingsResponse, TraktError> {
    todo!("Phase 11 Task 5")
}

pub async fn trigger_sync(
    _pool: &PgPool,
    _user_id: Uuid,
) -> Result<SyncTriggerResponse, TraktError> {
    todo!("Phase 11 Task 6")
}

pub async fn get_sync_status(
    _pool: &PgPool,
    _user_id: Uuid,
) -> Result<SyncStatusResponse, TraktError> {
    todo!("Phase 11 Task 5")
}

pub async fn list_history(
    _pool: &PgPool,
    _user_id: Uuid,
    _query: &HistoryQuery,
) -> Result<TraktHistoryResponse, TraktError> {
    todo!("Phase 11 Task 5")
}

pub async fn list_ratings(
    _pool: &PgPool,
    _user_id: Uuid,
    _query: &HistoryQuery,
) -> Result<TraktHistoryResponse, TraktError> {
    todo!("Phase 11 Task 5")
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

async fn load_account(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<TraktAccountRow>, TraktError> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, trakt_username, trakt_user_id,
               access_token, refresh_token, token_expires_at, token_scope,
               last_full_sync_at, sync_enabled, sync_watched, sync_watchlist,
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
    pool: &PgPool,
    user_id: Uuid,
    token: &TraktTokenResponse,
    settings: &TraktUserSettings,
) -> Result<TraktAccountRow, TraktError> {
    let expires_at = token_expires_at(token);

    let row = sqlx::query(
        r#"
        INSERT INTO trakt_accounts (
            user_id, trakt_username, trakt_user_id,
            access_token, refresh_token, token_expires_at, token_scope,
            sync_enabled, sync_watched, sync_watchlist, sync_collection, sync_ratings
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, true, true, true, true, true)
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
                  last_full_sync_at, sync_enabled, sync_watched, sync_watchlist,
                  sync_collection, sync_ratings, created_at, updated_at
        "#,
    )
    .bind(user_id)
    .bind(&settings.user.username)
    .bind(settings.account.id)
    .bind(&token.access_token)
    .bind(&token.refresh_token)
    .bind(expires_at)
    .bind(token.scope.as_deref().filter(|s| !s.is_empty()))
    .fetch_one(pool)
    .await
    .map_err(TraktError::Database)?;

    Ok(row_to_account(&row))
}

async fn update_tokens(
    pool: &PgPool,
    user_id: Uuid,
    token: &TraktTokenResponse,
    expires_at: DateTime<Utc>,
) -> Result<(), TraktError> {
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
    .bind(&token.access_token)
    .bind(&token.refresh_token)
    .bind(expires_at)
    .bind(token.scope.as_deref().filter(|s| !s.is_empty()))
    .execute(pool)
    .await
    .map_err(TraktError::Database)?;
    Ok(())
}

fn token_expires_at(token: &TraktTokenResponse) -> DateTime<Utc> {
    Utc::now() + Duration::seconds(token.expires_in.max(0))
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
        sync_enabled: row.get("sync_enabled"),
        sync_watched: row.get("sync_watched"),
        sync_watchlist: row.get("sync_watchlist"),
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
            sync_watchlist: a.sync_watchlist,
            sync_collection: a.sync_collection,
            sync_ratings: a.sync_ratings,
            last_full_sync_at: a.last_full_sync_at,
        },
        None => TraktAccountResponse {
            linked: false,
            trakt_username: None,
            trakt_user_id: None,
            token_expires_at: None,
            sync_enabled: false,
            sync_watched: false,
            sync_watchlist: false,
            sync_collection: false,
            sync_ratings: false,
            last_full_sync_at: None,
        },
    }
}
