// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::service::{
    canonical_content_rating, hash_parent_pin, profile_response,
    replace_profile_libraries_in_transaction, row_to_profile, validate_profile_type,
};
use super::{error::ProfilesError, types::*};
use crate::domains::household_security::{self as owner, types::Scope};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub async fn create_profile(
    pool: &PgPool,
    user: Uuid,
    req: CreateProfileRequest,
) -> Result<ProfileResponse, ProfilesError> {
    create_profile_for_session(pool, user, None, req).await
}

async fn get_profile(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    id: Uuid,
) -> Result<ProfileRow, ProfilesError> {
    let row=sqlx::query("SELECT id,owner_user_id,name,avatar,profile_type,is_default,max_content_rating,allow_search,allow_downloads,allow_external_links,allow_ambient_channels,parent_pin_hash,parent_pin_failed_attempts,parent_pin_locked_until,created_at,updated_at FROM user_profiles WHERE id=$1 AND owner_user_id=$2 FOR NO KEY UPDATE")
        .bind(id).bind(user).fetch_optional(&mut **tx).await?.ok_or(ProfilesError::NotFound)?;
    Ok(row_to_profile(&row))
}

pub async fn create_profile_for_session(
    pool: &PgPool,
    owner_user_id: Uuid,
    actor: Option<Scope>,
    req: CreateProfileRequest,
) -> Result<ProfileResponse, ProfilesError> {
    let mut transaction = owner::transactions::begin(pool, owner_user_id).await?;
    owner::transactions::authorize(&mut transaction, owner_user_id, actor).await?;
    let profile_type = req.profile_type.unwrap_or_else(|| "standard".to_string());
    validate_profile_type(&profile_type)?;
    let is_kids_profile = profile_type == "kids";
    let max_content_rating = canonical_content_rating(
        req.max_content_rating
            .as_deref()
            .unwrap_or(if is_kids_profile { "TV-Y7" } else { "NC-17" }),
    )?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(ProfilesError::InvalidProfileType(
            "name is required".to_string(),
        ));
    }
    let parent_pin_hash = match req.parent_pin.as_deref() {
        Some(pin) if is_kids_profile => Some(hash_parent_pin(pin)?),
        Some(_) => return Err(ProfilesError::ParentPinNotAllowed),
        None if is_kids_profile => return Err(ProfilesError::ParentPinRequired),
        None => None,
    };

    let row = sqlx::query(
        "INSERT INTO user_profiles (owner_user_id, name, avatar, profile_type, max_content_rating, \
         allow_search, allow_downloads, allow_external_links, allow_ambient_channels, parent_pin_hash) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         RETURNING id, owner_user_id, name, avatar, profile_type, is_default, max_content_rating, \
         allow_search, allow_downloads, allow_external_links, allow_ambient_channels, parent_pin_hash, \
         parent_pin_failed_attempts, parent_pin_locked_until, created_at, updated_at",
    )
    .bind(owner_user_id)
    .bind(name)
    .bind(req.avatar.as_deref().map(str::trim).filter(|v| !v.is_empty()))
    .bind(&profile_type)
    .bind(&max_content_rating)
    .bind(req.allow_search.unwrap_or(!is_kids_profile))
    .bind(req.allow_downloads.unwrap_or(!is_kids_profile))
    .bind(req.allow_external_links.unwrap_or(!is_kids_profile))
    .bind(req.allow_ambient_channels.unwrap_or(true))
    .bind(parent_pin_hash)
    .fetch_one(&mut *transaction)
    .await?;

    let profile = row_to_profile(&row);
    replace_profile_libraries_in_transaction(
        &mut transaction,
        profile.id,
        req.library_ids.unwrap_or_default(),
    )
    .await?;
    transaction.commit().await?;
    profile_response(pool, profile).await
}

pub async fn update_profile(
    pool: &PgPool,
    owner_user_id: Uuid,
    profile_id: Uuid,
    actor: Scope,
    req: UpdateProfileRequest,
) -> Result<ProfileResponse, ProfilesError> {
    let mut transaction = owner::transactions::begin(pool, owner_user_id).await?;
    owner::transactions::authorize(&mut transaction, owner_user_id, Some(actor)).await?;
    let current = get_profile(&mut transaction, owner_user_id, profile_id).await?;
    let max_content_rating = match req.max_content_rating.as_deref() {
        Some(value) => Some(canonical_content_rating(value)?),
        None => None,
    };
    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if req.name.is_some() && name.is_none() {
        return Err(ProfilesError::InvalidProfileType(
            "name is required".to_string(),
        ));
    }
    let parent_pin_hash = match req.parent_pin.as_deref() {
        Some(pin) if current.profile_type == "kids" => Some(hash_parent_pin(pin)?),
        Some(_) => return Err(ProfilesError::ParentPinNotAllowed),
        None => None,
    };

    let row = sqlx::query(
        "UPDATE user_profiles SET name = COALESCE($3, name), avatar = COALESCE($4, avatar), \
         max_content_rating = COALESCE($5, max_content_rating), allow_search = COALESCE($6, allow_search), \
         allow_downloads = COALESCE($7, allow_downloads), allow_external_links = COALESCE($8, allow_external_links), \
         allow_ambient_channels = COALESCE($9, allow_ambient_channels), parent_pin_hash = COALESCE($10, parent_pin_hash), \
         parent_pin_failed_attempts = CASE WHEN $10 IS NULL THEN parent_pin_failed_attempts ELSE 0 END, \
         parent_pin_locked_until = CASE WHEN $10 IS NULL THEN parent_pin_locked_until ELSE NULL END, updated_at = now() \
         WHERE id = $1 AND owner_user_id = $2 \
         RETURNING id, owner_user_id, name, avatar, profile_type, is_default, max_content_rating, \
         allow_search, allow_downloads, allow_external_links, allow_ambient_channels, parent_pin_hash, \
         parent_pin_failed_attempts, parent_pin_locked_until, created_at, updated_at",
    )
    .bind(profile_id)
    .bind(owner_user_id)
    .bind(name)
    .bind(req.avatar.as_deref().map(str::trim).filter(|v| !v.is_empty()))
    .bind(max_content_rating)
    .bind(req.allow_search)
    .bind(req.allow_downloads)
    .bind(req.allow_external_links)
    .bind(req.allow_ambient_channels)
    .bind(&parent_pin_hash)
    .fetch_one(&mut *transaction)
    .await?;

    if let Some(library_ids) = req.library_ids {
        replace_profile_libraries_in_transaction(&mut transaction, current.id, library_ids).await?;
    }
    if parent_pin_hash.is_some() {
        sqlx::query(
            "UPDATE user_sessions SET parent_unlock_profile_id = NULL, parent_unlock_expires_at = NULL \
             WHERE parent_unlock_profile_id = $1",
        )
        .bind(profile_id)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;

    profile_response(pool, row_to_profile(&row)).await
}

pub async fn delete_profile(
    pool: &PgPool,
    owner_user_id: Uuid,
    profile_id: Uuid,
    actor: Scope,
) -> Result<(), ProfilesError> {
    let mut transaction = owner::transactions::begin(pool, owner_user_id).await?;
    owner::transactions::authorize(&mut transaction, owner_user_id, Some(actor)).await?;
    let profile = get_profile(&mut transaction, owner_user_id, profile_id).await?;
    if profile.is_default {
        return Err(ProfilesError::CannotDelete);
    }

    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_sessions WHERE user_id = $1 AND active_profile_id = $2)",
    )
    .bind(owner_user_id)
    .bind(profile_id)
    .fetch_one(&mut *transaction)
    .await?;
    if active {
        return Err(ProfilesError::CannotDelete);
    }

    sqlx::query("DELETE FROM user_profiles WHERE id = $1 AND owner_user_id = $2")
        .bind(profile_id)
        .bind(owner_user_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}
