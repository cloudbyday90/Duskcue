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

use argon2::password_hash::{
    PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng,
};
use argon2::{Algorithm, Argon2, Params, Version};
use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use super::error::ProfilesError;
use super::types::*;

const PARENT_PIN_MAX_ATTEMPTS: i16 = 5;
const PARENT_PIN_LOCKOUT_MINUTES: i64 = 15;
const PARENT_UNLOCK_MINUTES: i64 = 10;

pub async fn list_profiles(
    pool: &PgPool,
    owner_user_id: Uuid,
    session_id: Uuid,
    active_profile_id: Uuid,
    profile_selection_required: bool,
    device_id: Option<&str>,
) -> Result<ProfileListResponse, ProfilesError> {
    let rows = sqlx::query(
        "SELECT id, owner_user_id, name, avatar, profile_type, is_default, max_content_rating, \
         allow_search, allow_downloads, allow_external_links, allow_ambient_channels, parent_pin_hash, \
         parent_pin_failed_attempts, parent_pin_locked_until, created_at, updated_at \
         FROM user_profiles WHERE owner_user_id = $1 ORDER BY is_default DESC, created_at ASC",
    )
    .bind(owner_user_id)
    .fetch_all(pool)
    .await?;

    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        items.push(profile_response(pool, row_to_profile(row)).await?);
    }

    let parent_unlock =
        parent_unlock_state(pool, owner_user_id, session_id, active_profile_id).await?;

    Ok(ProfileListResponse {
        active_profile_id,
        profile_selection_required,
        remembered_profile_id: remembered_profile_id(pool, owner_user_id, device_id).await?,
        device_can_remember_profile: normalized_device_id(device_id).is_some(),
        parent_unlock_required: parent_unlock.required,
        parent_unlock_expires_at: parent_unlock.expires_at,
        items,
    })
}

pub async fn create_profile(
    pool: &PgPool,
    owner_user_id: Uuid,
    req: CreateProfileRequest,
) -> Result<ProfileResponse, ProfilesError> {
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
    .fetch_one(pool)
    .await?;

    let profile = row_to_profile(&row);
    replace_profile_libraries(pool, profile.id, req.library_ids.unwrap_or_default()).await?;
    profile_response(pool, profile).await
}

pub async fn update_profile(
    pool: &PgPool,
    owner_user_id: Uuid,
    profile_id: Uuid,
    req: UpdateProfileRequest,
) -> Result<ProfileResponse, ProfilesError> {
    let current = get_owned_profile(pool, owner_user_id, profile_id).await?;
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

    let mut transaction = pool.begin().await?;
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
) -> Result<(), ProfilesError> {
    let profile = get_owned_profile(pool, owner_user_id, profile_id).await?;
    if profile.is_default {
        return Err(ProfilesError::CannotDelete);
    }

    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_sessions WHERE user_id = $1 AND active_profile_id = $2)",
    )
    .bind(owner_user_id)
    .bind(profile_id)
    .fetch_one(pool)
    .await?;
    if active {
        return Err(ProfilesError::CannotDelete);
    }

    sqlx::query("DELETE FROM user_profiles WHERE id = $1 AND owner_user_id = $2")
        .bind(profile_id)
        .bind(owner_user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn switch_profile(
    pool: &PgPool,
    owner_user_id: Uuid,
    session_id: Uuid,
    device_id: Option<&str>,
    profile_id: Uuid,
    remember_on_device: Option<bool>,
) -> Result<SwitchProfileResponse, ProfilesError> {
    let profile = get_owned_profile(pool, owner_user_id, profile_id).await?;
    let mut transaction = pool.begin().await?;
    let session = sqlx::query(
        "SELECT s.active_profile_id, s.parent_unlock_profile_id, s.parent_unlock_expires_at, \
         p.profile_type AS active_profile_type, p.parent_pin_hash IS NOT NULL AS active_profile_has_parent_pin \
         FROM user_sessions s JOIN user_profiles p ON p.id = s.active_profile_id \
         WHERE s.id = $1 AND s.user_id = $2 FOR UPDATE OF s",
    )
    .bind(session_id)
    .bind(owner_user_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(ProfilesError::AccessDenied)?;
    let active_profile_id: Uuid = session.get("active_profile_id");
    let active_profile_type: String = session.get("active_profile_type");
    let active_profile_has_parent_pin: bool = session.get("active_profile_has_parent_pin");
    let unlocked_profile_id: Option<Uuid> = session.try_get("parent_unlock_profile_id").ok();
    let unlocked_until: Option<DateTime<Utc>> = session.try_get("parent_unlock_expires_at").ok();
    if active_profile_id != profile_id
        && active_profile_type == "kids"
        && profile.profile_type == "standard"
        && active_profile_has_parent_pin
        && (unlocked_profile_id != Some(active_profile_id)
            || unlocked_until.is_none_or(|expires_at| expires_at <= Utc::now()))
    {
        return Err(ProfilesError::ParentUnlockRequired);
    }
    if let Some(remember) = remember_on_device {
        let device_id = normalized_device_id(device_id);
        if remember {
            let device_id = device_id.ok_or(ProfilesError::DeviceIdentityRequired)?;
            sqlx::query(
                "INSERT INTO profile_device_preferences (owner_user_id, device_id, profile_id) \
                 VALUES ($1, $2, $3) \
                 ON CONFLICT (owner_user_id, device_id) \
                 DO UPDATE SET profile_id = EXCLUDED.profile_id, updated_at = now()",
            )
            .bind(owner_user_id)
            .bind(device_id)
            .bind(profile_id)
            .execute(&mut *transaction)
            .await?;
        } else if let Some(device_id) = device_id {
            sqlx::query(
                "DELETE FROM profile_device_preferences WHERE owner_user_id = $1 AND device_id = $2",
            )
            .bind(owner_user_id)
            .bind(device_id)
            .execute(&mut *transaction)
            .await?;
        }
    }

    let update = if active_profile_id == profile_id {
        "UPDATE user_sessions SET profile_selection_required = false, last_active_at = now() WHERE id = $1 AND user_id = $2"
    } else {
        "UPDATE user_sessions SET active_profile_id = $3, profile_selection_required = false, \
         parent_unlock_profile_id = NULL, parent_unlock_expires_at = NULL, last_active_at = now() \
         WHERE id = $1 AND user_id = $2"
    };
    let mut update_query = sqlx::query(update).bind(session_id).bind(owner_user_id);
    if active_profile_id != profile_id {
        update_query = update_query.bind(profile_id);
    }
    let changed = update_query.execute(&mut *transaction).await?;
    if changed.rows_affected() == 0 {
        return Err(ProfilesError::AccessDenied);
    }
    transaction.commit().await?;
    let parent_unlock = parent_unlock_state(pool, owner_user_id, session_id, profile_id).await?;
    Ok(SwitchProfileResponse {
        active_profile: profile_response(pool, profile).await?,
        profile_selection_required: false,
        remembered_profile_id: remembered_profile_id(pool, owner_user_id, device_id).await?,
        device_can_remember_profile: normalized_device_id(device_id).is_some(),
        parent_unlock_required: parent_unlock.required,
        parent_unlock_expires_at: parent_unlock.expires_at,
    })
}

pub async fn parent_unlock(
    pool: &PgPool,
    owner_user_id: Uuid,
    session_id: Uuid,
    pin: String,
) -> Result<ParentUnlockResponse, ProfilesError> {
    validate_parent_pin(&pin)?;
    let mut transaction = pool.begin().await?;
    let active_session =
        sqlx::query("SELECT active_profile_id FROM user_sessions WHERE id = $1 AND user_id = $2")
            .bind(session_id)
            .bind(owner_user_id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(ProfilesError::AccessDenied)?;
    let profile_id: Uuid = active_session.get("active_profile_id");
    let profile = sqlx::query(
        "SELECT profile_type, parent_pin_hash, parent_pin_failed_attempts, parent_pin_locked_until \
         FROM user_profiles WHERE id = $1 AND owner_user_id = $2 FOR UPDATE",
    )
    .bind(profile_id)
    .bind(owner_user_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(ProfilesError::NotFound)?;
    let profile_type: String = profile.get("profile_type");
    if profile_type != "kids" {
        return Err(ProfilesError::ParentUnlockUnavailable);
    }
    let locked_session = sqlx::query(
        "SELECT active_profile_id FROM user_sessions WHERE id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(session_id)
    .bind(owner_user_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(ProfilesError::AccessDenied)?;
    let locked_active_profile_id: Uuid = locked_session.get("active_profile_id");
    if locked_active_profile_id != profile_id {
        return Err(ProfilesError::ParentUnlockUnavailable);
    }
    let parent_pin_hash: Option<String> = profile.try_get("parent_pin_hash").ok().flatten();
    let parent_pin_hash = parent_pin_hash.ok_or(ProfilesError::ParentUnlockUnavailable)?;
    let existing_parent_pin_lock: Option<DateTime<Utc>> =
        profile.try_get("parent_pin_locked_until").ok().flatten();
    if existing_parent_pin_lock.is_some_and(|locked_until| locked_until > Utc::now()) {
        return Err(ProfilesError::ParentPinLocked);
    }
    if !verify_parent_pin(&parent_pin_hash, &pin)? {
        let failed_attempts: i16 = profile.get("parent_pin_failed_attempts");
        let next_failed_attempts = failed_attempts.saturating_add(1);
        let locked_until = parent_pin_locked_until(next_failed_attempts, Utc::now());
        sqlx::query(
            "UPDATE user_profiles SET parent_pin_failed_attempts = $2, parent_pin_locked_until = $3, updated_at = now() \
             WHERE id = $1",
        )
        .bind(profile_id)
        .bind(next_failed_attempts)
        .bind(locked_until)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        return Err(ProfilesError::ParentPinInvalid);
    }
    let unlocked_until = Utc::now() + Duration::minutes(PARENT_UNLOCK_MINUTES);
    sqlx::query(
        "UPDATE user_profiles SET parent_pin_failed_attempts = 0, parent_pin_locked_until = NULL, updated_at = now() \
         WHERE id = $1",
    )
    .bind(profile_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE user_sessions SET parent_unlock_profile_id = $3, parent_unlock_expires_at = $4, last_active_at = now() \
         WHERE id = $1 AND user_id = $2",
    )
    .bind(session_id)
    .bind(owner_user_id)
    .bind(profile_id)
    .bind(unlocked_until)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(ParentUnlockResponse { unlocked_until })
}

pub fn normalized_device_id(device_id: Option<&str>) -> Option<&str> {
    device_id
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.chars().count() <= 200)
}

pub async fn remembered_profile_id(
    pool: &PgPool,
    owner_user_id: Uuid,
    device_id: Option<&str>,
) -> Result<Option<Uuid>, ProfilesError> {
    let Some(device_id) = normalized_device_id(device_id) else {
        return Ok(None);
    };

    Ok(sqlx::query_scalar::<_, Uuid>(
        "SELECT p.id FROM profile_device_preferences preference \
         JOIN user_profiles p ON p.id = preference.profile_id AND p.owner_user_id = preference.owner_user_id \
         WHERE preference.owner_user_id = $1 AND preference.device_id = $2",
    )
    .bind(owner_user_id)
    .bind(device_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn load_profile_scope(
    pool: &PgPool,
    owner_user_id: Uuid,
    profile_id: Uuid,
    has_all_library_access: bool,
) -> Result<ProfileScope, ProfilesError> {
    let profile = get_owned_profile(pool, owner_user_id, profile_id).await?;
    let library_ids = list_profile_library_ids(pool, profile_id).await?;
    let user_library_ids = if has_all_library_access {
        Vec::new()
    } else {
        sqlx::query("SELECT library_id FROM user_library_access WHERE user_id = $1")
            .bind(owner_user_id)
            .fetch_all(pool)
            .await?
            .iter()
            .map(|row| row.try_get("library_id"))
            .collect::<Result<Vec<Uuid>, sqlx::Error>>()?
    };

    Ok(ProfileScope {
        profile_id,
        owner_user_id,
        profile_type: profile.profile_type,
        max_content_rating: profile.max_content_rating,
        allow_search: profile.allow_search,
        allow_downloads: profile.allow_downloads,
        allow_external_links: profile.allow_external_links,
        allow_ambient_channels: profile.allow_ambient_channels,
        library_ids,
        user_library_ids,
        has_all_library_access,
    })
}

pub fn is_kids(scope: &ProfileScope) -> bool {
    scope.profile_type == "kids"
}

pub fn is_media_allowed(
    scope: &ProfileScope,
    library_id: Uuid,
    content_rating: Option<&str>,
) -> bool {
    let user_allows_library =
        scope.has_all_library_access || scope.user_library_ids.contains(&library_id);
    if !user_allows_library {
        return false;
    }
    if !is_kids(scope) {
        return true;
    }
    if !scope.library_ids.contains(&library_id) {
        return false;
    }
    let Some(actual_rank) = content_rating.and_then(content_rating_rank) else {
        return false;
    };
    let Some(max_rank) = content_rating_rank(&scope.max_content_rating) else {
        return false;
    };
    actual_rank <= max_rank
}

pub async fn assert_media_access(
    pool: &PgPool,
    scope: &ProfileScope,
    media_item_id: Uuid,
) -> Result<(), ProfilesError> {
    let row = sqlx::query(
        "SELECT mi.library_id, mi.content_rating FROM media_items mi \
         JOIN libraries l ON l.id = mi.library_id \
         WHERE mi.id = $1 AND l.deleted_at IS NULL",
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await?
    .ok_or(ProfilesError::ContentNotAllowed)?;
    let library_id: Uuid = row.try_get("library_id")?;
    let content_rating: Option<String> = row.try_get("content_rating").ok().flatten();
    if is_media_allowed(scope, library_id, content_rating.as_deref()) {
        Ok(())
    } else {
        Err(ProfilesError::ContentNotAllowed)
    }
}

pub async fn list_ambient_channels(
    pool: &PgPool,
    scope: &ProfileScope,
    include_all: bool,
) -> Result<AmbientChannelListResponse, ProfilesError> {
    let rows = if include_all {
        sqlx::query(
            "SELECT c.id, c.owner_user_id, c.name, c.description, c.audience, c.is_enabled, c.created_at, c.updated_at, \
             (SELECT count(*) FROM ambient_channel_items i WHERE i.channel_id = c.id) AS item_count \
             FROM ambient_channels c WHERE c.owner_user_id = $1 ORDER BY c.audience, c.name",
        )
        .bind(scope.owner_user_id)
        .fetch_all(pool)
        .await?
    } else {
        let audience = if is_kids(scope) { "kids" } else { "standard" };
        sqlx::query(
            "SELECT c.id, c.owner_user_id, c.name, c.description, c.audience, c.is_enabled, c.created_at, c.updated_at, \
             (SELECT count(*) FROM ambient_channel_items i WHERE i.channel_id = c.id) AS item_count \
             FROM ambient_channels c WHERE c.owner_user_id = $1 AND c.audience = $2 AND c.is_enabled = true \
             ORDER BY c.name",
        )
        .bind(scope.owner_user_id)
        .bind(audience)
        .fetch_all(pool)
        .await?
    };

    Ok(AmbientChannelListResponse {
        items: rows.iter().map(row_to_channel_response).collect(),
    })
}

pub async fn create_ambient_channel(
    pool: &PgPool,
    owner_user_id: Uuid,
    req: CreateAmbientChannelRequest,
) -> Result<AmbientChannelResponse, ProfilesError> {
    validate_channel_audience(&req.audience)?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(ProfilesError::ChannelUnavailable);
    }
    let row = sqlx::query(
        "INSERT INTO ambient_channels (owner_user_id, name, description, audience, is_enabled) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING id, owner_user_id, name, description, audience, is_enabled, created_at, updated_at",
    )
    .bind(owner_user_id)
    .bind(name)
    .bind(req.description.as_deref().map(str::trim).filter(|v| !v.is_empty()))
    .bind(&req.audience)
    .bind(req.is_enabled.unwrap_or(true))
    .fetch_one(pool)
    .await?;
    let channel = row_to_channel(&row);
    replace_channel_items(
        pool,
        owner_user_id,
        channel.id,
        req.media_item_ids.unwrap_or_default(),
    )
    .await?;
    channel_response(pool, channel).await
}

pub async fn update_ambient_channel(
    pool: &PgPool,
    owner_user_id: Uuid,
    channel_id: Uuid,
    req: UpdateAmbientChannelRequest,
) -> Result<AmbientChannelResponse, ProfilesError> {
    if let Some(audience) = req.audience.as_deref() {
        validate_channel_audience(audience)?;
    }
    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if req.name.is_some() && name.is_none() {
        return Err(ProfilesError::ChannelUnavailable);
    }
    let row = sqlx::query(
        "UPDATE ambient_channels SET name = COALESCE($3, name), description = COALESCE($4, description), \
         audience = COALESCE($5, audience), is_enabled = COALESCE($6, is_enabled), updated_at = now() \
         WHERE id = $1 AND owner_user_id = $2 \
         RETURNING id, owner_user_id, name, description, audience, is_enabled, created_at, updated_at",
    )
    .bind(channel_id)
    .bind(owner_user_id)
    .bind(name)
    .bind(req.description.as_deref().map(str::trim).filter(|v| !v.is_empty()))
    .bind(req.audience)
    .bind(req.is_enabled)
    .fetch_optional(pool)
    .await?
    .ok_or(ProfilesError::ChannelNotFound)?;
    channel_response(pool, row_to_channel(&row)).await
}

pub async fn delete_ambient_channel(
    pool: &PgPool,
    owner_user_id: Uuid,
    channel_id: Uuid,
) -> Result<(), ProfilesError> {
    let result = sqlx::query("DELETE FROM ambient_channels WHERE id = $1 AND owner_user_id = $2")
        .bind(channel_id)
        .bind(owner_user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ProfilesError::ChannelNotFound);
    }
    Ok(())
}

pub async fn list_ambient_channel_items(
    pool: &PgPool,
    owner_user_id: Uuid,
    channel_id: Uuid,
) -> Result<AmbientChannelItemsResponse, ProfilesError> {
    assert_channel_owned(pool, owner_user_id, channel_id).await?;
    let media_item_ids = sqlx::query(
        "SELECT media_item_id FROM ambient_channel_items WHERE channel_id = $1 ORDER BY position",
    )
    .bind(channel_id)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|row| row.try_get("media_item_id"))
    .collect::<Result<Vec<Uuid>, sqlx::Error>>()?;
    Ok(AmbientChannelItemsResponse {
        channel_id,
        media_item_ids,
    })
}

pub async fn replace_channel_items(
    pool: &PgPool,
    owner_user_id: Uuid,
    channel_id: Uuid,
    media_item_ids: Vec<Uuid>,
) -> Result<(), ProfilesError> {
    assert_channel_owned(pool, owner_user_id, channel_id).await?;
    let mut unique = Vec::new();
    for media_item_id in media_item_ids {
        if !unique.contains(&media_item_id) {
            unique.push(media_item_id);
        }
    }
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM ambient_channel_items WHERE channel_id = $1")
        .bind(channel_id)
        .execute(&mut *tx)
        .await?;
    for (position, media_item_id) in unique.iter().enumerate() {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM media_items WHERE id = $1)")
                .bind(media_item_id)
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(ProfilesError::ContentNotAllowed);
        }
        sqlx::query(
            "INSERT INTO ambient_channel_items (channel_id, media_item_id, position) VALUES ($1, $2, $3)",
        )
        .bind(channel_id)
        .bind(media_item_id)
        .bind(position as i32)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn next_ambient_channel_item(
    pool: &PgPool,
    scope: &ProfileScope,
    channel_id: Uuid,
    after_media_item_id: Option<Uuid>,
) -> Result<AmbientChannelNextResponse, ProfilesError> {
    let channel = get_available_channel(pool, scope, channel_id).await?;
    let items = sqlx::query(
        "SELECT media_item_id FROM ambient_channel_items WHERE channel_id = $1 ORDER BY position",
    )
    .bind(channel_id)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|row| row.try_get("media_item_id"))
    .collect::<Result<Vec<Uuid>, sqlx::Error>>()?;

    if items.is_empty() {
        return Err(ProfilesError::ChannelEmpty);
    }
    let start = after_media_item_id
        .and_then(|id| items.iter().position(|candidate| *candidate == id))
        .map(|index| (index + 1) % items.len())
        .unwrap_or(0);
    for offset in 0..items.len() {
        let media_item_id = items[(start + offset) % items.len()];
        if assert_media_access(pool, scope, media_item_id)
            .await
            .is_ok()
        {
            return Ok(AmbientChannelNextResponse {
                channel_id,
                channel_name: channel.name,
                media_item_id,
                playback_mode: "ambient".to_string(),
            });
        }
    }
    Err(ProfilesError::ChannelEmpty)
}

pub async fn assert_ambient_playback_allowed(
    pool: &PgPool,
    scope: &ProfileScope,
    channel_id: Uuid,
    media_item_id: Uuid,
) -> Result<(), ProfilesError> {
    get_available_channel(pool, scope, channel_id).await?;
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM ambient_channel_items WHERE channel_id = $1 AND media_item_id = $2)",
    )
    .bind(channel_id)
    .bind(media_item_id)
    .fetch_one(pool)
    .await?;
    if !is_member {
        return Err(ProfilesError::ChannelUnavailable);
    }
    assert_media_access(pool, scope, media_item_id).await
}

pub fn canonical_content_rating(value: &str) -> Result<String, ProfilesError> {
    let normalized = value.trim().to_ascii_uppercase();
    let rating = match normalized.as_str() {
        "TVY" | "TV-Y" => "TV-Y",
        "TVY7" | "TV-Y7" | "TV-Y7-FV" => "TV-Y7",
        "G" | "TVG" | "TV-G" => {
            if normalized.starts_with("TV") {
                "TV-G"
            } else {
                "G"
            }
        }
        "PG" => "PG",
        "TVPG" | "TV-PG" => "TV-PG",
        "PG13" | "PG-13" => "PG-13",
        "TV14" | "TV-14" => "TV-14",
        "R" => "R",
        "TVMA" | "TV-MA" => "TV-MA",
        "NC17" | "NC-17" => "NC-17",
        _ => return Err(ProfilesError::InvalidContentRating(value.to_string())),
    };
    Ok(rating.to_string())
}

pub fn content_rating_rank(value: &str) -> Option<u8> {
    let normalized = value.trim().to_ascii_uppercase();
    let base = normalized.split([' ', ':']).next().unwrap_or("");
    canonical_content_rating(base).ok().and_then(|rating| {
        CONTENT_RATINGS
            .iter()
            .position(|candidate| *candidate == rating)
            .map(|index| index as u8)
    })
}

fn validate_profile_type(value: &str) -> Result<(), ProfilesError> {
    if PROFILE_TYPES.contains(&value) {
        Ok(())
    } else {
        Err(ProfilesError::InvalidProfileType(value.to_string()))
    }
}

fn validate_channel_audience(value: &str) -> Result<(), ProfilesError> {
    if CHANNEL_AUDIENCES.contains(&value) {
        Ok(())
    } else {
        Err(ProfilesError::ChannelUnavailable)
    }
}

async fn get_owned_profile(
    pool: &PgPool,
    owner_user_id: Uuid,
    profile_id: Uuid,
) -> Result<ProfileRow, ProfilesError> {
    let row = sqlx::query(
        "SELECT id, owner_user_id, name, avatar, profile_type, is_default, max_content_rating, \
         allow_search, allow_downloads, allow_external_links, allow_ambient_channels, parent_pin_hash, \
         parent_pin_failed_attempts, parent_pin_locked_until, created_at, updated_at \
         FROM user_profiles WHERE id = $1 AND owner_user_id = $2",
    )
    .bind(profile_id)
    .bind(owner_user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(ProfilesError::NotFound)?;
    Ok(row_to_profile(&row))
}

async fn profile_response(
    pool: &PgPool,
    profile: ProfileRow,
) -> Result<ProfileResponse, ProfilesError> {
    let library_ids = list_profile_library_ids(pool, profile.id).await?;
    Ok(ProfileResponse {
        id: profile.id,
        name: profile.name,
        avatar: profile.avatar,
        profile_type: profile.profile_type,
        is_default: profile.is_default,
        max_content_rating: profile.max_content_rating,
        library_ids,
        allow_search: profile.allow_search,
        allow_downloads: profile.allow_downloads,
        allow_external_links: profile.allow_external_links,
        allow_ambient_channels: profile.allow_ambient_channels,
        parent_pin_configured: profile.parent_pin_hash.is_some(),
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    })
}

async fn list_profile_library_ids(
    pool: &PgPool,
    profile_id: Uuid,
) -> Result<Vec<Uuid>, ProfilesError> {
    Ok(sqlx::query(
        "SELECT library_id FROM profile_library_access WHERE profile_id = $1 ORDER BY library_id",
    )
    .bind(profile_id)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|row| row.try_get("library_id"))
    .collect::<Result<Vec<Uuid>, sqlx::Error>>()?)
}

async fn replace_profile_libraries(
    pool: &PgPool,
    profile_id: Uuid,
    library_ids: Vec<Uuid>,
) -> Result<(), ProfilesError> {
    let mut tx = pool.begin().await?;
    replace_profile_libraries_in_transaction(&mut tx, profile_id, library_ids).await?;
    tx.commit().await?;
    Ok(())
}

async fn replace_profile_libraries_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    profile_id: Uuid,
    library_ids: Vec<Uuid>,
) -> Result<(), ProfilesError> {
    let mut unique = Vec::new();
    for library_id in library_ids {
        if !unique.contains(&library_id) {
            unique.push(library_id);
        }
    }
    sqlx::query("DELETE FROM profile_library_access WHERE profile_id = $1")
        .bind(profile_id)
        .execute(&mut **transaction)
        .await?;
    for library_id in unique {
        sqlx::query(
            "INSERT INTO profile_library_access (profile_id, library_id) SELECT $1, id FROM libraries WHERE id = $2 AND deleted_at IS NULL",
        )
        .bind(profile_id)
        .bind(library_id)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

struct ParentUnlockState {
    required: bool,
    expires_at: Option<DateTime<Utc>>,
}

async fn parent_unlock_state(
    pool: &PgPool,
    owner_user_id: Uuid,
    session_id: Uuid,
    active_profile_id: Uuid,
) -> Result<ParentUnlockState, ProfilesError> {
    let row = sqlx::query(
        "SELECT p.profile_type = 'kids' AND p.parent_pin_hash IS NOT NULL \
         AND (s.parent_unlock_profile_id IS DISTINCT FROM p.id OR s.parent_unlock_expires_at IS NULL \
         OR s.parent_unlock_expires_at <= now()) AS parent_unlock_required, \
         CASE WHEN s.parent_unlock_profile_id = p.id AND s.parent_unlock_expires_at > now() \
         THEN s.parent_unlock_expires_at ELSE NULL END AS parent_unlock_expires_at \
         FROM user_sessions s JOIN user_profiles p ON p.id = s.active_profile_id \
         WHERE s.id = $1 AND s.user_id = $2 AND p.id = $3",
    )
    .bind(session_id)
    .bind(owner_user_id)
    .bind(active_profile_id)
    .fetch_optional(pool)
    .await?
    .ok_or(ProfilesError::AccessDenied)?;
    Ok(ParentUnlockState {
        required: row.get("parent_unlock_required"),
        expires_at: row.try_get("parent_unlock_expires_at").ok().flatten(),
    })
}

fn validate_parent_pin(pin: &str) -> Result<(), ProfilesError> {
    if !(4..=12).contains(&pin.len()) || !pin.bytes().all(|value| value.is_ascii_digit()) {
        return Err(ProfilesError::ParentPinRequired);
    }
    Ok(())
}

fn parent_pin_hasher() -> Result<Argon2<'static>, ProfilesError> {
    let params = Params::new(19 * 1024, 2, 1, Some(32))
        .map_err(|_| ProfilesError::ParentPinHashingFailed)?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

fn hash_parent_pin(pin: &str) -> Result<String, ProfilesError> {
    validate_parent_pin(pin)?;
    let salt = SaltString::generate(&mut OsRng);
    parent_pin_hasher()?
        .hash_password(pin.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| ProfilesError::ParentPinHashingFailed)
}

fn verify_parent_pin(hash: &str, pin: &str) -> Result<bool, ProfilesError> {
    let parsed = match PasswordHash::new(hash) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(false),
    };
    Ok(parent_pin_hasher()?
        .verify_password(pin.as_bytes(), &parsed)
        .is_ok())
}

fn parent_pin_locked_until(failed_attempts: i16, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    (failed_attempts >= PARENT_PIN_MAX_ATTEMPTS)
        .then(|| now + Duration::minutes(PARENT_PIN_LOCKOUT_MINUTES))
}

async fn assert_channel_owned(
    pool: &PgPool,
    owner_user_id: Uuid,
    channel_id: Uuid,
) -> Result<(), ProfilesError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM ambient_channels WHERE id = $1 AND owner_user_id = $2)",
    )
    .bind(channel_id)
    .bind(owner_user_id)
    .fetch_one(pool)
    .await?;
    if exists {
        Ok(())
    } else {
        Err(ProfilesError::ChannelNotFound)
    }
}

async fn get_available_channel(
    pool: &PgPool,
    scope: &ProfileScope,
    channel_id: Uuid,
) -> Result<AmbientChannelRow, ProfilesError> {
    if !scope.allow_ambient_channels {
        return Err(ProfilesError::FeatureDisabled);
    }
    let audience = if is_kids(scope) { "kids" } else { "standard" };
    let row = sqlx::query(
        "SELECT id, owner_user_id, name, description, audience, is_enabled, created_at, updated_at \
         FROM ambient_channels WHERE id = $1 AND owner_user_id = $2 AND audience = $3 AND is_enabled = true",
    )
    .bind(channel_id)
    .bind(scope.owner_user_id)
    .bind(audience)
    .fetch_optional(pool)
    .await?
    .ok_or(ProfilesError::ChannelUnavailable)?;
    Ok(row_to_channel(&row))
}

async fn channel_response(
    pool: &PgPool,
    channel: AmbientChannelRow,
) -> Result<AmbientChannelResponse, ProfilesError> {
    let item_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM ambient_channel_items WHERE channel_id = $1")
            .bind(channel.id)
            .fetch_one(pool)
            .await?;
    Ok(AmbientChannelResponse {
        id: channel.id,
        name: channel.name,
        description: channel.description,
        audience: channel.audience,
        is_enabled: channel.is_enabled,
        item_count,
        created_at: channel.created_at,
        updated_at: channel.updated_at,
    })
}

fn row_to_profile(row: &sqlx::postgres::PgRow) -> ProfileRow {
    ProfileRow {
        id: row.get("id"),
        owner_user_id: row.get("owner_user_id"),
        name: row.get("name"),
        avatar: row.try_get("avatar").ok().flatten(),
        profile_type: row.get("profile_type"),
        is_default: row.get("is_default"),
        max_content_rating: row.get("max_content_rating"),
        allow_search: row.get("allow_search"),
        allow_downloads: row.get("allow_downloads"),
        allow_external_links: row.get("allow_external_links"),
        allow_ambient_channels: row.get("allow_ambient_channels"),
        parent_pin_hash: row.try_get("parent_pin_hash").ok().flatten(),
        parent_pin_failed_attempts: row.get("parent_pin_failed_attempts"),
        parent_pin_locked_until: row.try_get("parent_pin_locked_until").ok().flatten(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_channel(row: &sqlx::postgres::PgRow) -> AmbientChannelRow {
    AmbientChannelRow {
        id: row.get("id"),
        owner_user_id: row.get("owner_user_id"),
        name: row.get("name"),
        description: row.try_get("description").ok().flatten(),
        audience: row.get("audience"),
        is_enabled: row.get("is_enabled"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_channel_response(row: &sqlx::postgres::PgRow) -> AmbientChannelResponse {
    AmbientChannelResponse {
        id: row.get("id"),
        name: row.get("name"),
        description: row.try_get("description").ok().flatten(),
        audience: row.get("audience"),
        is_enabled: row.get("is_enabled"),
        item_count: row.get("item_count"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kids_scope(max_content_rating: &str, library_id: Uuid) -> ProfileScope {
        ProfileScope {
            profile_id: Uuid::now_v7(),
            owner_user_id: Uuid::now_v7(),
            profile_type: "kids".to_string(),
            max_content_rating: max_content_rating.to_string(),
            allow_search: true,
            allow_downloads: false,
            allow_external_links: false,
            allow_ambient_channels: true,
            library_ids: vec![library_id],
            user_library_ids: vec![library_id],
            has_all_library_access: false,
        }
    }

    #[test]
    fn content_ratings_normalize_known_provider_aliases() {
        assert_eq!(canonical_content_rating("tv-y7-fv").unwrap(), "TV-Y7");
        assert_eq!(canonical_content_rating("pg13").unwrap(), "PG-13");
        assert!(canonical_content_rating("unrated").is_err());
    }

    #[test]
    fn kids_scope_denies_unknown_and_over_limit_ratings() {
        let library_id = Uuid::now_v7();
        let scope = kids_scope("PG", library_id);

        assert!(is_media_allowed(&scope, library_id, Some("TV-G")));
        assert!(!is_media_allowed(&scope, library_id, Some("PG-13")));
        assert!(!is_media_allowed(&scope, library_id, None));
    }

    #[test]
    fn kids_scope_requires_a_parent_allowed_library() {
        let allowed_library = Uuid::now_v7();
        let scope = kids_scope("TV-Y7", allowed_library);

        assert!(is_media_allowed(&scope, allowed_library, Some("TV-Y7")));
        assert!(!is_media_allowed(&scope, Uuid::now_v7(), Some("TV-Y7")));
    }

    #[test]
    fn device_id_normalization_accepts_only_bounded_nonempty_values() {
        assert_eq!(
            normalized_device_id(Some("  living-room-tv  ")),
            Some("living-room-tv")
        );
        assert_eq!(normalized_device_id(Some("   ")), None);
        assert_eq!(normalized_device_id(Some(&"a".repeat(201))), None);
    }

    #[test]
    fn parent_pins_require_a_bounded_numeric_value() {
        assert!(validate_parent_pin("1234").is_ok());
        assert!(validate_parent_pin("123456789012").is_ok());
        assert!(validate_parent_pin("123").is_err());
        assert!(validate_parent_pin("1234567890123").is_err());
        assert!(validate_parent_pin("12a4").is_err());
    }

    #[test]
    fn parent_pin_hashes_are_argon2id_and_verify_without_exposure() {
        let hash = hash_parent_pin("482916").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_parent_pin(&hash, "482916").unwrap());
        assert!(!verify_parent_pin(&hash, "482917").unwrap());
    }

    #[test]
    fn fifth_parent_pin_failure_starts_the_durable_lockout_window() {
        let now = Utc::now();
        assert!(parent_pin_locked_until(4, now).is_none());
        assert_eq!(
            parent_pin_locked_until(5, now),
            Some(now + Duration::minutes(PARENT_PIN_LOCKOUT_MINUTES))
        );
    }
}
