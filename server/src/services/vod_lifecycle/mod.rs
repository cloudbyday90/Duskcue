// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

mod body;
mod generation;
mod io;
mod owner;
mod queue;
mod read;
mod types;

pub(crate) use generation::Generation;
pub(crate) use io::remove_cache;
pub(crate) use owner::VodLifecycle;
pub(crate) use read::ReadContext;
pub(crate) use types::{Content, Prepared};

pub(crate) const CHUNK: usize = 64 * 1024;
pub(crate) const PLAYLIST: usize = 1024 * 1024;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod root_tests;
