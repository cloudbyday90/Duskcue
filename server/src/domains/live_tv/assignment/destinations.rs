// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    error::AssignmentError,
    secrets::{self, FeedField},
};
use crate::{
    domains::live_tv::destination::{self, Approval},
    services::encryption::EncryptionKey,
};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

pub(super) async fn approve(
    tx: &mut Transaction<'_, Postgres>,
    key: &EncryptionKey,
    user: Uuid,
    id: Uuid,
    feed: FeedField,
    expected: &str,
    pins: &[String],
) -> Result<(), AssignmentError> {
    let query = match feed {
        FeedField::Playlist => {
            "SELECT playlist_secret FROM live_tv_assignments WHERE id=$1 AND household_user_id=$2"
        }
        FeedField::Guide => {
            "SELECT guide_secret FROM live_tv_assignments WHERE id=$1 AND household_user_id=$2"
        }
    };
    let secret: String = sqlx::query_scalar(query)
        .bind(id)
        .bind(user)
        .fetch_one(&mut **tx)
        .await?;
    let raw =
        secrets::open(key, user, id, feed, &secret).map_err(|_| AssignmentError::Unavailable)?;
    let approval = Approval::new(expected, pins, &destination::source(&raw)?)?;
    let sealed = approval.seal(key, user, id, feed)?;
    save(tx, id, feed, Some(&sealed), approval.addresses.len() as i16).await
}

pub(super) async fn save(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    feed: FeedField,
    secret: Option<&str>,
    count: i16,
) -> Result<(), AssignmentError> {
    let query = match feed {
        FeedField::Playlist => {
            "UPDATE live_tv_assignments SET playlist_destination=$2,playlist_address_count=$3,revision=revision+1,source_generation=source_generation+1,updated_at=clock_timestamp() WHERE id=$1"
        }
        FeedField::Guide => {
            "UPDATE live_tv_assignments SET guide_destination=$2,guide_address_count=$3,revision=revision+1,source_generation=source_generation+1,updated_at=clock_timestamp() WHERE id=$1"
        }
    };
    sqlx::query(query)
        .bind(id)
        .bind(secret)
        .bind(count)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(super) async fn retain(
    tx: &mut Transaction<'_, Postgres>,
    key: &EncryptionKey,
    user: Uuid,
    id: Uuid,
    playlist: &str,
    guide: &str,
) -> Result<(bool, bool), AssignmentError> {
    let row = sqlx::query("SELECT playlist_secret,guide_secret,playlist_destination,guide_destination FROM live_tv_assignments WHERE id=$1 AND household_user_id=$2").bind(id).bind(user).fetch_one(&mut **tx).await?;
    let valid = |feed, field, destination_field, expected: &str| {
        let destination: Option<String> = row.get(destination_field);
        let secret: String = row.get(field);
        let old = secrets::open(key, user, id, feed, &secret)
            .ok()
            .and_then(|raw| destination::source(&raw).ok());
        match (old, destination) {
            (Some(url), Some(envelope)) if url.origin().ascii_serialization() == expected => {
                Approval::open(key, user, id, feed, &envelope, &url).is_ok()
            }
            _ => false,
        }
    };
    Ok((
        valid(
            FeedField::Playlist,
            "playlist_secret",
            "playlist_destination",
            playlist,
        ),
        valid(FeedField::Guide, "guide_secret", "guide_destination", guide),
    ))
}
