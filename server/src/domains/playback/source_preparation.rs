// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

use super::PlaybackError;
use super::media_source::{
    MediaFileDetails, SelectedTracks, build_media_file_info,
    fetch_media_file_details_on_connection, select_best_media_file_on_connection,
};
use super::types::{PlaybackActor, StartPlaybackRequest, VALID_PLAYBACK_MODES};
use crate::domains::profiles::{service as profiles, types::AmbientChannelRow};
use crate::services::decision_engine::MediaFileInfo;

pub(super) struct ResolvedSource {
    pub media_item_id: Uuid,
    pub library_id: Uuid,
    pub playback_mode: String,
    pub ambient_channel: Option<AmbientChannelRow>,
    pub details: MediaFileDetails,
    pub media: MediaFileInfo,
    pub tracks: SelectedTracks,
}

pub(super) async fn resolve(
    pool: &PgPool,
    actor: PlaybackActor,
    request: &StartPlaybackRequest,
) -> Result<ResolvedSource, PlaybackError> {
    let mut conn = pool.acquire().await?;
    resolve_on_connection(&mut conn, actor, request).await
}

pub(super) async fn resolve_on_connection(
    conn: &mut PgConnection,
    actor: PlaybackActor,
    request: &StartPlaybackRequest,
) -> Result<ResolvedSource, PlaybackError> {
    let media_item_id = request.media_item_id.ok_or(PlaybackError::MediaNotFound)?;
    let playback_mode = request.playback_mode.as_deref().unwrap_or("interactive");
    if !VALID_PLAYBACK_MODES.contains(&playback_mode) {
        return Err(PlaybackError::InvalidPlaybackMode(playback_mode.into()));
    }
    let scope = profiles::load_profile_scope_on_connection(
        conn,
        actor.user_id,
        actor.profile_id,
        actor.has_all_library_access,
    )
    .await
    .map_err(|_| PlaybackError::AccessDenied)?;
    profiles::assert_media_access_on_connection(conn, &scope, media_item_id)
        .await
        .map_err(|_| PlaybackError::AccessDenied)?;
    let ambient_channel = if playback_mode == "ambient" {
        let id = request.ambient_channel_id.ok_or_else(|| {
            PlaybackError::InvalidPlaybackMode(
                "ambient playback requires ambient_channel_id".into(),
            )
        })?;
        let updated = request.ambient_channel_updated_at.ok_or_else(|| {
            PlaybackError::InvalidPlaybackMode(
                "ambient playback requires ambient_channel_updated_at".into(),
            )
        })?;
        let channel = profiles::assert_ambient_playback_allowed_on_connection(
            conn,
            &scope,
            id,
            media_item_id,
        )
        .await
        .map_err(|_| PlaybackError::AccessDenied)?;
        if channel.updated_at != updated {
            return Err(PlaybackError::AmbientChannelStale);
        }
        Some(channel)
    } else {
        if request.ambient_channel_id.is_some() || request.ambient_channel_updated_at.is_some() {
            return Err(PlaybackError::InvalidPlaybackMode(
                "interactive playback cannot include ambient channel fields".into(),
            ));
        }
        None
    };
    let row = sqlx::query("SELECT library_id FROM media_items WHERE id=$1")
        .bind(media_item_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(PlaybackError::MediaNotFound)?;
    let library_id = row.try_get("library_id").unwrap_or_default();
    let details = match request.media_file_id {
        Some(id) => fetch_media_file_details_on_connection(conn, id, media_item_id).await?,
        None => select_best_media_file_on_connection(conn, media_item_id).await?,
    };
    let (media, tracks) = build_media_file_info(
        &details,
        request.audio_stream_index,
        request.subtitle_stream_index,
    )?;
    Ok(ResolvedSource {
        media_item_id,
        library_id,
        playback_mode: playback_mode.into(),
        ambient_channel,
        details,
        media,
        tracks,
    })
}
