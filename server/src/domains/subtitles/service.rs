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

#![allow(unused_variables)]

use sqlx::PgPool;
use uuid::Uuid;

use crate::domains::subtitles::error::SubtitleError;
use crate::domains::subtitles::types::*;

pub async fn list_subtitles(
    pool: &PgPool,
    media_item_id: Uuid,
) -> Result<SubtitleListResponse, SubtitleError> {
    todo!()
}

pub async fn get_subtitle(
    pool: &PgPool,
    media_item_id: Uuid,
    subtitle_id: Uuid,
) -> Result<SubtitleFileResponse, SubtitleError> {
    todo!()
}

pub async fn get_subtitle_content(
    pool: &PgPool,
    media_item_id: Uuid,
    subtitle_id: Uuid,
    delivery_format: Option<&str>,
    user_offset_ms: Option<i32>,
) -> Result<(String, &'static str), SubtitleError> {
    todo!()
}

pub async fn fetch_subtitles(
    pool: &PgPool,
    media_item_id: Uuid,
    req: &FetchSubtitlesRequest,
) -> Result<FetchSubtitlesResponse, SubtitleError> {
    todo!()
}

pub async fn set_subtitle_offset(
    pool: &PgPool,
    user_id: Uuid,
    media_item_id: Uuid,
    subtitle_id: Uuid,
    offset_ms: i32,
) -> Result<SubtitleOffsetResponse, SubtitleError> {
    todo!()
}

pub async fn trigger_ocr(
    pool: &PgPool,
    media_item_id: Uuid,
    subtitle_id: Uuid,
    engine_override: Option<&str>,
) -> Result<SubtitleOcrResult, SubtitleError> {
    todo!()
}

pub async fn get_subtitle_sync_data(
    pool: &PgPool,
    media_item_id: Uuid,
    subtitle_id: Uuid,
) -> Result<SubtitleSyncDataResponse, SubtitleError> {
    todo!()
}

pub async fn delete_subtitle(
    pool: &PgPool,
    media_item_id: Uuid,
    subtitle_id: Uuid,
) -> Result<(), SubtitleError> {
    todo!()
}

pub fn validate_language_code(code: &str) -> bool {
    code.len() >= 2 && code.len() <= 10 && code.chars().all(|c| c.is_ascii_alphabetic())
}
