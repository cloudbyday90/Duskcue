// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    authority, destinations, error::AssignmentError, policy, preconditions::Condition, storage,
    types::*,
};
use crate::{
    domains::household_security::{transactions, types::Scope},
    error::AppError,
    state::AppState,
};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
use validator::Validate;

async fn current(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    view: View,
) -> Result<AssignmentResponse, AssignmentError> {
    Ok(policy::response(
        storage::load(tx, user)
            .await?
            .ok_or(AssignmentError::NotFound)?,
        view,
    ))
}

pub(super) async fn read(
    state: &AppState,
    actor: Scope,
    target: Option<Uuid>,
) -> Result<AssignmentResponse, AppError> {
    let (mut tx, user, view) = if let Some(target) = target {
        (
            authority::administrator(&state.pool, actor, target, false).await?,
            target,
            View::Administrator,
        )
    } else {
        (
            authority::owner(&state.pool, actor).await?,
            actor.user,
            View::HouseholdOwner,
        )
    };
    let result = current(&mut tx, user, view).await?;
    tx.commit().await.map_err(AssignmentError::from)?;
    Ok(result)
}

pub(super) async fn create(
    state: &AppState,
    actor: Scope,
    user: Uuid,
    condition: Condition,
    request: CreateRequest,
) -> Result<AssignmentResponse, AppError> {
    let mut tx = authority::administrator(&state.pool, actor, user, true).await?;
    condition.check(
        storage::load(&mut tx, user)
            .await?
            .as_ref()
            .map(|_| "existing"),
    )?;
    request
        .validate()
        .map_err(|_| AssignmentError::InvalidPolicy)?;
    let policy = request.policy();
    let id = Uuid::now_v7();
    let credentials = storage::credentials(
        &state.encryption_key,
        user,
        id,
        request.playlist_url,
        request.guide_url,
    )?;
    storage::insert(&mut tx, user, id, policy, credentials).await?;
    transactions::revoke_management(&mut tx, user).await?;
    authority::audit(&mut tx, actor, id, user, "assignment_created").await?;
    let result = current(&mut tx, user, View::Administrator).await?;
    tx.commit().await.map_err(AssignmentError::from)?;
    Ok(result)
}

pub(super) async fn admin_patch(
    state: &AppState,
    actor: Scope,
    user: Uuid,
    condition: Condition,
    request: AdminPatch,
) -> Result<AssignmentResponse, AppError> {
    let mut tx = authority::administrator(&state.pool, actor, user, true).await?;
    let row = storage::load(&mut tx, user)
        .await?
        .ok_or(AssignmentError::NotFound)?;
    let old_policy = row.policy();
    let old_id = row.id;
    condition.check(Some(&policy::etag(&policy::response(
        row,
        View::Administrator,
    ))?))?;
    request
        .validate()
        .map_err(|_| AssignmentError::InvalidPolicy)?;
    let (id, event) = match request {
        AdminPatch::Configure {
            enabled,
            max_live_streams,
            allow_owner_profile_access,
            allow_owner_live_limit,
        } => {
            storage::configure(
                &mut tx,
                old_id,
                AdminPolicy {
                    enabled,
                    max_live_streams,
                    allow_owner_profile_access,
                    allow_owner_live_limit,
                },
            )
            .await?;
            (old_id, "assignment_policy_changed")
        }
        AdminPatch::RotateCredentials {
            playlist_url,
            guide_url,
        } => {
            let credentials =
                storage::credentials(&state.encryption_key, user, old_id, playlist_url, guide_url)?;
            storage::rotate(&mut tx, &state.encryption_key, user, old_id, credentials).await?;
            (old_id, "assignment_credentials_rotated")
        }
        AdminPatch::ReplaceAssignment {
            playlist_url,
            guide_url,
        } => {
            let id = Uuid::now_v7();
            let credentials =
                storage::credentials(&state.encryption_key, user, id, playlist_url, guide_url)?;
            storage::remove(&mut tx, old_id).await?;
            storage::insert(&mut tx, user, id, old_policy, credentials).await?;
            (id, "assignment_replaced")
        }
        AdminPatch::ApproveDestination {
            feed,
            expected_origin,
            addresses,
        } => {
            destinations::approve(
                &mut tx,
                &state.encryption_key,
                user,
                old_id,
                feed,
                &expected_origin,
                &addresses,
            )
            .await?;
            (old_id, "destination_approved")
        }
        AdminPatch::RevokeDestination { feed } => {
            destinations::save(&mut tx, old_id, feed, None, 0).await?;
            (old_id, "destination_revoked")
        }
    };
    transactions::revoke_management(&mut tx, user).await?;
    authority::audit(&mut tx, actor, id, user, event).await?;
    let result = current(&mut tx, user, View::Administrator).await?;
    tx.commit().await.map_err(AssignmentError::from)?;
    Ok(result)
}

pub(super) async fn remove(
    state: &AppState,
    actor: Scope,
    user: Uuid,
    condition: Condition,
) -> Result<(), AppError> {
    let mut tx = authority::administrator(&state.pool, actor, user, true).await?;
    let response = current(&mut tx, user, View::Administrator).await?;
    condition.check(Some(&policy::etag(&response)?))?;
    storage::remove(&mut tx, response.assignment_id).await?;
    transactions::revoke_management(&mut tx, user).await?;
    authority::audit(
        &mut tx,
        actor,
        response.assignment_id,
        user,
        "assignment_deleted",
    )
    .await?;
    tx.commit().await.map_err(AssignmentError::from)?;
    Ok(())
}

pub(super) async fn owner_patch(
    state: &AppState,
    actor: Scope,
    condition: Condition,
    request: OwnerPatch,
) -> Result<AssignmentResponse, AppError> {
    let mut tx = authority::owner(&state.pool, actor).await?;
    let row = storage::load(&mut tx, actor.user)
        .await?
        .ok_or(AssignmentError::NotFound)?;
    let response = policy::response(row, View::HouseholdOwner);
    condition.check(Some(&policy::etag(&response)?))?;
    if !response.enabled {
        return Err(AssignmentError::Forbidden.into());
    }
    request
        .validate()
        .map_err(|_| AssignmentError::InvalidPolicy)?;
    let event = match request {
        OwnerPatch::SetLiveLimit { max_live_streams } => {
            if !response.allow_owner_live_limit {
                return Err(AssignmentError::Forbidden.into());
            }
            if max_live_streams
                .zip(response.max_live_streams)
                .is_some_and(|(owner, admin)| owner > admin)
            {
                return Err(AssignmentError::InvalidPolicy.into());
            }
            sqlx::query("UPDATE live_tv_assignments SET owner_max_live_streams=$2 WHERE id=$1")
                .bind(response.assignment_id)
                .bind(max_live_streams)
                .execute(&mut *tx)
                .await
                .map_err(AssignmentError::from)?;
            "owner_limit_changed"
        }
        OwnerPatch::SetProfileAccess {
            profile_id,
            enabled,
        } => {
            if !response.allow_owner_profile_access {
                return Err(AssignmentError::Forbidden.into());
            }
            let profile_type: Option<String> = sqlx::query_scalar("SELECT profile_type FROM user_profiles WHERE id=$1 AND owner_user_id=$2 FOR NO KEY UPDATE")
                .bind(profile_id).bind(actor.user).fetch_optional(&mut *tx).await.map_err(AssignmentError::from)?;
            match profile_type.as_deref() {
                None => return Err(AssignmentError::NotFound.into()),
                Some("standard") => {}
                _ => return Err(AssignmentError::InvalidPolicy.into()),
            }
            sqlx::query("INSERT INTO live_tv_profile_access(household_user_id,assignment_id,profile_id,enabled) VALUES ($1,$2,$3,$4) ON CONFLICT (assignment_id,profile_id) DO UPDATE SET enabled=EXCLUDED.enabled")
                .bind(actor.user).bind(response.assignment_id).bind(profile_id).bind(enabled).execute(&mut *tx).await.map_err(AssignmentError::from)?;
            "owner_profile_access_changed"
        }
    };
    sqlx::query("UPDATE live_tv_assignments SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1").bind(response.assignment_id).execute(&mut *tx).await.map_err(AssignmentError::from)?;
    authority::audit(&mut tx, actor, response.assignment_id, actor.user, event).await?;
    let result = current(&mut tx, actor.user, View::HouseholdOwner).await?;
    tx.commit().await.map_err(AssignmentError::from)?;
    Ok(result)
}

pub(super) async fn profiles(
    state: &AppState,
    actor: Scope,
    query: ProfileQuery,
) -> Result<ProfilePage, AppError> {
    query
        .validate()
        .map_err(|_| AssignmentError::InvalidRequest)?;
    let mut tx = authority::owner(&state.pool, actor).await?;
    let assignment = current(&mut tx, actor.user, View::HouseholdOwner).await?;
    let limit = query.limit.unwrap_or(50) as usize;
    let rows = sqlx::query("SELECT p.id,p.name,p.profile_type,COALESCE(a.enabled,false) AND p.profile_type='standard' AS enabled FROM user_profiles p LEFT JOIN live_tv_profile_access a ON a.profile_id=p.id AND a.assignment_id=$2 WHERE p.owner_user_id=$1 AND ($3::uuid IS NULL OR p.id>$3) ORDER BY p.id LIMIT $4")
        .bind(actor.user).bind(assignment.assignment_id).bind(query.after).bind((limit+1) as i64).fetch_all(&mut *tx).await.map_err(AssignmentError::from)?;
    let has_more = rows.len() > limit;
    let items: Vec<ProfileResponse> = rows
        .into_iter()
        .take(limit)
        .map(|row| {
            let profile_type: String = row.get("profile_type");
            ProfileResponse {
                id: row.get("id"),
                name: row.get("name"),
                eligible: profile_type == "standard",
                profile_type,
                enabled: row.get("enabled"),
            }
        })
        .collect();
    let next_cursor = has_more.then(|| items.last().map(|p| p.id)).flatten();
    tx.commit().await.map_err(AssignmentError::from)?;
    Ok(ProfilePage {
        assignment_revision: assignment.revision,
        items,
        next_cursor,
    })
}
