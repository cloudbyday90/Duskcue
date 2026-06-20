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

//! Storyboard domain service — DB access for the `storyboards` table plus
//! file-path resolution for serving the WebVTT index and WebP sprite sheets.
//!
//! The generation pipeline itself (FFmpeg frame extraction, sprite assembly,
//! WebVTT index authoring) lives in
//! [`crate::services::storyboards`](../../services/storyboards/index.html)
//! (Task 4). The background worker that orchestrates per-library generation
//! lives in
//! [`crate::workers::storyboard_generator`](../../workers/storyboard_generator/index.html)
//! (Task 6). The trigger functions here enqueue work on the scheduler
//! (mirroring `subtitle_auto_fetch` from Phase 9 Task 7 and
//! `segment_analysis` from Phase 10 Task 5).
//!
//! Storyboard files are stored under `{data_dir}/cache/storyboards/{media_file_id}/`
//! per STORYBOARDS.md — `data_dir` comes from `BootstrapConfig` via `AppState`.
//!
//! All queries use runtime `sqlx::query` (not compile-time `query!`)
//! consistent with the auth/users/segments/etc. domain convention — no
//! `DATABASE_URL` is required at build time.

use std::path::Path;

use sqlx::PgPool;
use uuid::Uuid;

use crate::domains::storyboards::error::StoryboardError;
use crate::domains::storyboards::types::*;

/// Get storyboard metadata for a media item.
///
/// Resolves the primary `media_files` row for the item, then loads the
/// matching `storyboards` row and builds the response with sprite URLs.
/// Returns `StoryboardNotFound` (MEDIA_007) when no storyboard has been
/// generated yet for this item.
pub async fn get_storyboard(
    pool: &PgPool,
    media_item_id: Uuid,
) -> Result<StoryboardResponse, StoryboardError> {
    todo!("Task 4 — resolve media_file, load storyboards row, build sprite URLs")
}

/// Read the WebVTT index file content for a media item's storyboard.
///
/// Returns the raw `index.vtt` text (served with `text/vtt` content type).
/// Returns `StoryboardNotFound` when no storyboard exists yet.
pub async fn get_storyboard_index(
    pool: &PgPool,
    media_item_id: Uuid,
    cache_dir: &Path,
) -> Result<String, StoryboardError> {
    todo!("Task 4 — resolve media_file, load storyboards row, read index.vtt from disk")
}

/// Read a WebP sprite sheet image for a media item's storyboard.
///
/// Validates the sprite filename against the expected pattern
/// (`sprite_NNN.webp`) to prevent path traversal, then reads the file bytes.
/// Returns `InvalidSpriteFilename` for malformed names, `StoryboardNotFound`
/// when no storyboard exists yet.
pub async fn get_storyboard_sprite(
    pool: &PgPool,
    media_item_id: Uuid,
    sprite_filename: &str,
    cache_dir: &Path,
) -> Result<Vec<u8>, StoryboardError> {
    todo!("Task 4 — validate filename, resolve media_file, read sprite from disk")
}

/// Trigger storyboard generation for all missing items in a library.
///
/// Verifies the library exists, checks that no generation is already running
/// for it (SYS_002 on conflict), then enqueues the `storyboard_generation`
/// scheduled task. Returns a queued acknowledgement — actual generation runs
/// in the background worker (Task 6).
pub async fn trigger_library_generation(
    pool: &PgPool,
    library_id: Uuid,
) -> Result<GenerateStoryboardsResponse, StoryboardError> {
    todo!("Task 6 — verify library, check in-progress, enqueue storyboard_generation task")
}

/// Trigger storyboard generation for a specific media item (force regen).
///
/// Resolves the item's primary media file and enqueues a single-file
/// generation job. Unlike the library endpoint this forces regeneration even
/// if a storyboard already exists (the worker deletes and regenerates).
pub async fn trigger_item_generation(
    pool: &PgPool,
    media_item_id: Uuid,
) -> Result<GenerateStoryboardsResponse, StoryboardError> {
    todo!("Task 6 — verify item, enqueue single-file storyboard generation")
}

/// Delete cached storyboard data for a media item.
///
/// Removes the on-disk sprite files and the `storyboards` DB row. The
/// storyboard can be regenerated at any time — it is derived data stored in
/// the cache layer (per STORYBOARDS.md design principle 1).
pub async fn delete_storyboard(
    pool: &PgPool,
    media_item_id: Uuid,
    cache_dir: &Path,
) -> Result<(), StoryboardError> {
    todo!("Task 4 — resolve media_file, delete on-disk files, delete storyboards row")
}
