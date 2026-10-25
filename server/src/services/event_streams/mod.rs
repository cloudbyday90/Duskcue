// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

mod forwarding;
mod lifecycle;
mod output;
mod wire;

pub use lifecycle::EventStreams;
pub use output::Output;
pub use wire::HEARTBEAT;

#[derive(Debug, thiserror::Error)]
#[error("event stream service is unavailable")]
pub struct Unavailable;

#[cfg(test)]
mod tests;
