// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

#[cfg(all(
    target_os = "linux",
    not(any(target_arch = "x86_64", target_arch = "aarch64"))
))]
compile_error!("managed FFmpeg requires Linux x86_64 or aarch64");

#[cfg(target_os = "linux")]
mod descriptors;
#[cfg(target_os = "linux")]
mod filesystem;
#[cfg(target_os = "linux")]
mod filter;
#[cfg(target_os = "linux")]
pub(crate) mod launch;
#[cfg(target_os = "linux")]
mod managed_elf;
#[cfg(target_os = "linux")]
pub(crate) mod owned_output;
#[cfg(target_os = "linux")]
mod wire;

pub struct SandboxConfig<'a> {
    pub media_path: &'a Path,
    pub transcode_dir: &'a Path,
}
