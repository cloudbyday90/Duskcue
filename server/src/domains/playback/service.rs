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

use uuid::Uuid;

use crate::domains::playback::error::PlaybackError;

pub async fn start_playback(
    _user_id: Uuid,
    _media_item_id: Uuid,
    _media_file_id: Option<Uuid>,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn heartbeat(
    _session_id: Uuid,
    _position_ms: Option<i32>,
    _state: Option<&str>,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn stop_playback(_session_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn seek(_session_id: Uuid, _position_ms: i32) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn get_playback_info(_session_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn get_user_item_data(
    _user_id: Uuid,
    _media_item_id: Uuid,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn list_bookmarks(
    _user_id: Uuid,
    _media_item_id: Uuid,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn create_bookmark(
    _user_id: Uuid,
    _media_item_id: Uuid,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn delete_bookmark(
    _user_id: Uuid,
    _bookmark_id: Uuid,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn list_playlists(_user_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn get_playlist(_user_id: Uuid, _playlist_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn create_playlist(_user_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn update_playlist(_user_id: Uuid, _playlist_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn delete_playlist(_user_id: Uuid, _playlist_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn list_playlist_items(
    _user_id: Uuid,
    _playlist_id: Uuid,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn add_playlist_item(_user_id: Uuid, _playlist_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn remove_playlist_item(
    _user_id: Uuid,
    _playlist_id: Uuid,
    _item_id: Uuid,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn stream_file(
    _user_id: Uuid,
    _media_file_id: Uuid,
    _range_header: Option<String>,
) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn get_transcode_manifest(_session_id: Uuid) -> Result<(), PlaybackError> {
    todo!()
}

pub async fn get_transcode_segment(_session_id: Uuid, _rendition: &str, _segment: &str) -> Result<(), PlaybackError> {
    todo!()
}
