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

pub use error::SubtitleError;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/items/{item_id}/subtitles",
            get(handlers::list_subtitles).post(handlers::fetch_subtitles),
        )
        .route(
            "/api/v1/items/{item_id}/subtitles/{subtitle_id}",
            get(handlers::get_subtitle).delete(handlers::delete_subtitle),
        )
        .route(
            "/api/v1/items/{item_id}/subtitles/{subtitle_id}/content",
            get(handlers::get_subtitle_content),
        )
        .route(
            "/api/v1/items/{item_id}/subtitles/{subtitle_id}/offset",
            axum::routing::put(handlers::set_subtitle_offset),
        )
        .route(
            "/api/v1/items/{item_id}/subtitles/{subtitle_id}/ocr",
            post(handlers::trigger_ocr),
        )
        .route(
            "/api/v1/items/{item_id}/subtitles/{subtitle_id}/sync",
            get(handlers::get_subtitle_sync_data),
        )
        .with_state(state)
}
