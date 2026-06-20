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

use crate::domains::segments::error::SegmentError;
use crate::domains::segments::types::*;

pub async fn list_segments(
    pool: &PgPool,
    media_item_id: Uuid,
    segment_type_filter: Option<&str>,
    can_edit: bool,
) -> Result<SegmentListResponse, SegmentError> {
    todo!("Task 2 — query media_segments with optional type filter, map rows to SegmentResponse with can_edit computed from caller capabilities")
}

pub async fn create_segment(
    pool: &PgPool,
    media_item_id: Uuid,
    req: &CreateSegmentRequest,
) -> Result<SegmentResponse, SegmentError> {
    todo!("Task 2 — validate timestamps, insert with source='manual' is_manual=true confidence=req.confidence.unwrap_or(1.0), compute skip_to_ms defaulting to end_ms, map unique violation to ManualSegmentExists")
}

pub async fn update_segment(
    pool: &PgPool,
    media_item_id: Uuid,
    segment_id: Uuid,
    req: &UpdateSegmentRequest,
) -> Result<SegmentResponse, SegmentError> {
    todo!("Task 2 — COALESCE partial update on start_ms/end_ms/skip_to_ms/confidence, revalidate end_ms > start_ms and skip_to_ms in [start_ms, end_ms]")
}

pub async fn delete_segment(
    pool: &PgPool,
    media_item_id: Uuid,
    segment_id: Uuid,
) -> Result<(), SegmentError> {
    todo!("Task 2 — DELETE FROM media_segments WHERE id=$1 AND media_item_id=$2; not-found -> SegmentNotFound")
}

pub async fn trigger_library_analysis(
    pool: &PgPool,
    library_id: Uuid,
) -> Result<AnalyzeSegmentsResponse, SegmentError> {
    todo!("Task 5 — verify library exists, enqueue segment_analysis scheduled task (or run synchronously for Task 5), return queued status")
}
