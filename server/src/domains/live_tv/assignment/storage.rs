// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    error::AssignmentError,
    secrets::{self, FeedField},
    types::{AdminPolicy, AssignmentRow},
};
use crate::{error::AppError, services::encryption::EncryptionKey};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(super) struct Credentials {
    playlist: String,
    guide: String,
    playlist_transport: &'static str,
    guide_transport: &'static str,
    playlist_origin: String,
    guide_origin: String,
}

pub(super) fn credentials(
    key: &EncryptionKey,
    user: Uuid,
    id: Uuid,
    playlist: String,
    guide: String,
) -> Result<Credentials, AppError> {
    let validation = super::super::service::validate_connection(
        super::super::types::ConnectionValidationRequest {
            playlist_url: playlist.clone(),
            guide_url: guide.clone(),
        },
    )
    .map_err(|error| AppError::Validation {
        errors: error.fields().to_vec(),
        instance: None,
    })?;
    let transport = |v| match v {
        super::super::types::FeedTransport::Http => "http",
        super::super::types::FeedTransport::Https => "https",
    };
    Ok(Credentials {
        playlist_origin: url::Url::parse(&playlist)
            .map_err(|_| AssignmentError::InvalidPolicy)?
            .origin()
            .ascii_serialization(),
        guide_origin: url::Url::parse(&guide)
            .map_err(|_| AssignmentError::InvalidPolicy)?
            .origin()
            .ascii_serialization(),
        playlist: secrets::seal(key, user, id, FeedField::Playlist, &playlist)
            .map_err(|_| AssignmentError::Unavailable)?,
        guide: secrets::seal(key, user, id, FeedField::Guide, &guide)
            .map_err(|_| AssignmentError::Unavailable)?,
        playlist_transport: transport(validation.playlist.transport),
        guide_transport: transport(validation.guide.transport),
    })
}

pub(super) async fn load(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
) -> Result<Option<AssignmentRow>, AssignmentError> {
    Ok(sqlx::query_as("SELECT id,household_user_id,revision,source_generation,playlist_address_count,guide_address_count,playlist_transport,guide_transport,enabled,max_live_streams,allow_owner_profile_access,allow_owner_live_limit,owner_max_live_streams FROM live_tv_assignments WHERE household_user_id=$1 FOR UPDATE")
        .bind(user).fetch_optional(&mut **tx).await?)
}

pub(super) async fn insert(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    id: Uuid,
    policy: AdminPolicy,
    secrets: Credentials,
) -> Result<(), AssignmentError> {
    sqlx::query("INSERT INTO live_tv_assignments(id,household_user_id,playlist_secret,guide_secret,playlist_transport,guide_transport,enabled,max_live_streams,allow_owner_profile_access,allow_owner_live_limit) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(id).bind(user).bind(secrets.playlist).bind(secrets.guide).bind(secrets.playlist_transport).bind(secrets.guide_transport)
        .bind(policy.enabled).bind(policy.max_live_streams).bind(policy.allow_owner_profile_access).bind(policy.allow_owner_live_limit).execute(&mut **tx).await?;
    Ok(())
}

pub(super) async fn configure(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    policy: AdminPolicy,
) -> Result<(), AssignmentError> {
    sqlx::query("UPDATE live_tv_assignments SET enabled=$2,max_live_streams=$3,allow_owner_profile_access=$4,allow_owner_live_limit=$5,revision=revision+1,source_generation=source_generation+1,updated_at=clock_timestamp() WHERE id=$1")
        .bind(id).bind(policy.enabled).bind(policy.max_live_streams).bind(policy.allow_owner_profile_access).bind(policy.allow_owner_live_limit).execute(&mut **tx).await?;
    Ok(())
}

pub(super) async fn rotate(
    tx: &mut Transaction<'_, Postgres>,
    key: &EncryptionKey,
    user: Uuid,
    id: Uuid,
    secrets: Credentials,
) -> Result<(), AssignmentError> {
    let (playlist, guide) = super::destinations::retain(
        tx,
        key,
        user,
        id,
        &secrets.playlist_origin,
        &secrets.guide_origin,
    )
    .await?;
    sqlx::query("UPDATE live_tv_assignments SET playlist_secret=$2,guide_secret=$3,playlist_transport=$4,guide_transport=$5,playlist_destination=CASE WHEN $6 THEN playlist_destination ELSE NULL END,playlist_address_count=CASE WHEN $6 THEN playlist_address_count ELSE 0 END,guide_destination=CASE WHEN $7 THEN guide_destination ELSE NULL END,guide_address_count=CASE WHEN $7 THEN guide_address_count ELSE 0 END,revision=revision+1,source_generation=source_generation+1,updated_at=clock_timestamp() WHERE id=$1")
        .bind(id).bind(secrets.playlist).bind(secrets.guide).bind(secrets.playlist_transport).bind(secrets.guide_transport).bind(playlist).bind(guide).execute(&mut **tx).await?;
    Ok(())
}

pub(super) async fn remove(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<(), AssignmentError> {
    sqlx::query("DELETE FROM live_tv_assignments WHERE id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
