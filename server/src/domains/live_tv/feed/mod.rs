// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

mod body;
mod connection;
mod error;
mod head_limit;
pub mod imports;
#[cfg(feature = "playback-qualification")]
mod operation;
#[cfg(feature = "playback-qualification")]
mod owner;
pub(crate) mod source;
pub(crate) mod tls;

pub use error::FeedError;
#[cfg(feature = "playback-qualification")]
pub use owner::{FeedOwner, Fetch};
