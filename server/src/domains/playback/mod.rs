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

pub mod error;
pub mod handlers;
pub mod service;
pub mod types;

pub use error::PlaybackError;

use axum::Router;
use axum::routing::{delete, get, post};

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/playback/start", post(handlers::start_playback))
        .route("/api/v1/playback/heartbeat", post(handlers::heartbeat))
        .route("/api/v1/playback/stop", post(handlers::stop_playback))
        .route("/api/v1/playback/seek", post(handlers::seek))
        .route(
            "/api/v1/playback/info/{session_id}",
            get(handlers::get_playback_info),
        )
        .route(
            "/api/v1/items/{item_id}/watch-data",
            get(handlers::get_watch_data).put(handlers::update_watch_data),
        )
        .route(
            "/api/v1/items/{item_id}/bookmarks",
            get(handlers::list_bookmarks).post(handlers::create_bookmark),
        )
        .route(
            "/api/v1/items/{item_id}/bookmarks/{bookmark_id}",
            delete(handlers::delete_bookmark),
        )
        .route("/api/v1/stream/{media_file_id}", get(handlers::stream_file))
        .route(
            "/api/v1/transcode/{session_id}/manifest.m3u8",
            get(handlers::get_transcode_manifest),
        )
        .route(
            "/api/v1/transcode/{session_id}/{rendition}/index.m3u8",
            get(handlers::get_transcode_playlist),
        )
        .route(
            "/api/v1/transcode/{session_id}/{rendition}/{segment}",
            get(handlers::get_transcode_segment),
        )
        .route(
            "/api/v1/playlists",
            get(handlers::list_playlists).post(handlers::create_playlist),
        )
        .route(
            "/api/v1/playlists/{playlist_id}",
            get(handlers::get_playlist)
                .patch(handlers::update_playlist)
                .delete(handlers::delete_playlist),
        )
        .route(
            "/api/v1/playlists/{playlist_id}/items",
            get(handlers::list_playlist_items).post(handlers::add_playlist_item),
        )
        .route(
            "/api/v1/playlists/{playlist_id}/items/{item_id}",
            delete(handlers::remove_playlist_item),
        )
        .route(
            "/api/v1/streaming-policies",
            get(handlers::list_streaming_policies).post(handlers::create_streaming_policy),
        )
        .route(
            "/api/v1/streaming-policies/{policy_id}",
            get(handlers::get_streaming_policy)
                .patch(handlers::update_streaming_policy)
                .delete(handlers::delete_streaming_policy),
        )
        .route(
            "/api/v1/users/{user_id}/streaming-limits",
            get(handlers::get_effective_streaming_limits),
        )
        .with_state(state)
}
