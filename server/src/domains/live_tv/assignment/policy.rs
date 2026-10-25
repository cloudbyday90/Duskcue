// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    error::AssignmentError,
    types::{AssignmentResponse, AssignmentRow, View},
};

pub(super) fn response(row: AssignmentRow, view: View) -> AssignmentResponse {
    let cap = match (row.max_live_streams, row.owner_max_live_streams) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    AssignmentResponse {
        assignment_id: row.id,
        household_user_id: row.household_user_id,
        revision: row.revision,
        source_generation: row.source_generation,
        playlist_destination_approved: row.playlist_address_count > 0,
        guide_destination_approved: row.guide_address_count > 0,
        playlist_address_count: row.playlist_address_count,
        guide_address_count: row.guide_address_count,
        configured: true,
        playlist_transport: row.playlist_transport,
        guide_transport: row.guide_transport,
        enabled: row.enabled,
        max_live_streams: row.max_live_streams,
        owner_max_live_streams: row.owner_max_live_streams,
        configured_live_tv_cap: cap,
        allow_owner_profile_access: row.allow_owner_profile_access,
        allow_owner_live_limit: row.allow_owner_live_limit,
        network_checked: false,
        playback_available: false,
        management_scope: view,
    }
}

pub(super) fn etag(response: &AssignmentResponse) -> Result<String, AssignmentError> {
    let bytes = serde_json::to_vec(response).map_err(|_| AssignmentError::Unavailable)?;
    Ok(crate::cache::sha256_etag(&bytes))
}
