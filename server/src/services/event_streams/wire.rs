// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use crate::services::event_bus::ServerEvent;
use axum::response::sse::Event;
use std::time::Duration;

pub(super) const CHANNEL_DEPTH: usize = 32;
pub const HEARTBEAT: Duration = Duration::from_secs(15);
pub(super) fn retry() -> Event {
    Event::default().retry(Duration::from_millis(5000))
}
pub(super) fn encode(event: &ServerEvent) -> Event {
    Event::default()
        .event(&event.event_type)
        .data(event.payload.to_string())
        .id(event.id.to_string())
}
