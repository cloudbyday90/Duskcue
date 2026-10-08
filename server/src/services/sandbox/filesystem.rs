// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use super::SandboxConfig;

pub(super) fn prepare(
    config: &SandboxConfig<'_>,
) -> Result<landlock::RulesetCreated, std::io::Error> {
    use landlock::{
        ABI, Access, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
    };

    let abi = ABI::V3;
    let access_ro = AccessFs::from_read(abi);
    let access_rw = AccessFs::from_all(abi);
    let mut access_files = access_rw;
    access_files.remove(AccessFs::RemoveDir);

    let ruleset = Ruleset::default()
        .handle_access(access_rw)
        .map_err(|e| std::io::Error::other(e.to_string()))?
        .create()
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let add_ro_rule = |rs: landlock::RulesetCreated,
                       path: &Path|
     -> Result<landlock::RulesetCreated, std::io::Error> {
        let fd = PathFd::new(path)
            .map_err(|e| std::io::Error::other(format!("landlock open {}: {e}", path.display())))?;
        rs.add_rule(PathBeneath::new(fd, access_ro))
            .map_err(|e| std::io::Error::other(format!("landlock rule {}: {e}", path.display())))
    };

    let add_rw_rule = |rs: landlock::RulesetCreated,
                       path: &Path|
     -> Result<landlock::RulesetCreated, std::io::Error> {
        let fd = PathFd::new(path)
            .map_err(|e| std::io::Error::other(format!("landlock open {}: {e}", path.display())))?;
        rs.add_rule(PathBeneath::new(fd, access_files))
            .map_err(|e| std::io::Error::other(format!("landlock rule {}: {e}", path.display())))
    };

    let mut rs = ruleset;

    for path in [
        Path::new("/usr"),
        Path::new("/lib"),
        Path::new("/etc"),
        Path::new("/dev/dri"),
    ] {
        if path.exists() {
            rs = add_ro_rule(rs, path)?;
        }
    }

    if config.media_path.exists() {
        rs = add_ro_rule(rs, config.media_path)?;
    }

    if config.transcode_dir.exists() {
        rs = add_rw_rule(rs, config.transcode_dir)?;
    }

    if Path::new("/tmp").exists() {
        rs = add_rw_rule(rs, Path::new("/tmp"))?;
    }

    Ok(rs)
}
