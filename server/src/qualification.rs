// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[path = "../tests/support/household_database.rs"]
#[cfg(feature = "playback-qualification")]
pub(crate) mod database;
#[path = "../tests/support/guarded_docker.rs"]
pub(crate) mod docker;
