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

use std::path::Path;

pub struct SandboxConfig<'a> {
    pub media_path: &'a Path,
    pub transcode_dir: &'a Path,
}

#[cfg(target_os = "linux")]
pub fn apply_sandbox(config: &SandboxConfig<'_>) -> Result<(), std::io::Error> {
    apply_landlock(config)?;
    apply_seccomp()?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn apply_sandbox(_config: &SandboxConfig<'_>) -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_landlock(config: &SandboxConfig<'_>) -> Result<(), std::io::Error> {
    use landlock::{
        ABI, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
        RulesetStatus,
    };

    let abi = ABI::V3;
    let access_ro = AccessFs::from_read(abi);
    let access_rw = AccessFs::from_all(abi);

    let ruleset = Ruleset::default()
        .handle_access(access_rw)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?
        .create()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    let add_ro_rule = |rs: landlock::RulesetCreated, path: &Path| -> Result<landlock::RulesetCreated, std::io::Error> {
        let fd = PathFd::new(path).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("landlock open {}: {e}", path.display()))
        })?;
        rs.add_rule(PathBeneath::new(fd, access_ro)).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("landlock rule {}: {e}", path.display()))
        })
    };

    let add_rw_rule = |rs: landlock::RulesetCreated, path: &Path| -> Result<landlock::RulesetCreated, std::io::Error> {
        let fd = PathFd::new(path).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("landlock open {}: {e}", path.display()))
        })?;
        rs.add_rule(PathBeneath::new(fd, access_rw)).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("landlock rule {}: {e}", path.display()))
        })
    };

    let mut rs = ruleset;

    for path in [Path::new("/usr"), Path::new("/lib"), Path::new("/etc"), Path::new("/dev/dri")] {
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

    let status = rs.restrict_self().map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, format!("landlock restrict_self: {e}"))
    })?;

    match status.ruleset {
        RulesetStatus::FullyEnforced | RulesetStatus::PartiallyEnforced => Ok(()),
        RulesetStatus::NotEnforced => Ok(()),
    }
}

#[cfg(target_os = "linux")]
fn apply_seccomp() -> Result<(), std::io::Error> {
    let filter = build_ffmpeg_filter().map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, format!("seccomp build: {e}"))
    })?;

    seccompiler::apply_filter(&filter).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, format!("seccomp apply: {e}"))
    })
}

#[cfg(target_os = "linux")]
fn build_ffmpeg_filter() -> Result<seccompiler::BpfProgram, seccompiler::Error> {
    use seccompiler::{SeccompAction, SeccompFilter, SeccompRule};
    use std::collections::BTreeMap;

    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = vec![
        (libc::SYS_read, vec![]),
        (libc::SYS_write, vec![]),
        (libc::SYS_close, vec![]),
        (libc::SYS_lseek, vec![]),
        (libc::SYS_pread64, vec![]),
        (libc::SYS_pwrite64, vec![]),
        (libc::SYS_openat, vec![]),
        (libc::SYS_fstat, vec![]),
        (libc::SYS_newfstatat, vec![]),
        (libc::SYS_fstatfs, vec![]),
        (libc::SYS_statx, vec![]),
        (libc::SYS_mmap, vec![]),
        (libc::SYS_munmap, vec![]),
        (libc::SYS_mprotect, vec![]),
        (libc::SYS_madvise, vec![]),
        (libc::SYS_brk, vec![]),
        (libc::SYS_mremap, vec![]),
        (libc::SYS_poll, vec![]),
        (libc::SYS_ppoll, vec![]),
        (libc::SYS_epoll_create1, vec![]),
        (libc::SYS_epoll_ctl, vec![]),
        (libc::SYS_epoll_wait, vec![]),
        (libc::SYS_epoll_pwait, vec![]),
        (libc::SYS_futex, vec![]),
        (libc::SYS_clock_gettime, vec![]),
        (libc::SYS_clock_nanosleep, vec![]),
        (libc::SYS_nanosleep, vec![]),
        (libc::SYS_gettimeofday, vec![]),
        (libc::SYS_ioctl, vec![]),
        (libc::SYS_dup, vec![]),
        (libc::SYS_dup2, vec![]),
        (libc::SYS_dup3, vec![]),
        (libc::SYS_pipe2, vec![]),
        (libc::SYS_fcntl, vec![]),
        (libc::SYS_getdents64, vec![]),
        (libc::SYS_faccessat2, vec![]),
        (libc::SYS_faccessat, vec![]),
        (libc::SYS_readlink, vec![]),
        (libc::SYS_readlinkat, vec![]),
        (libc::SYS_uname, vec![]),
        (libc::SYS_sysinfo, vec![]),
        (libc::SYS_getrandom, vec![]),
        (libc::SYS_rt_sigaction, vec![]),
        (libc::SYS_rt_sigprocmask, vec![]),
        (libc::SYS_rt_sigreturn, vec![]),
        (libc::SYS_exit_group, vec![]),
        (libc::SYS_clone, vec![]),
        (libc::SYS_set_tid_address, vec![]),
        (libc::SYS_fadvise64, vec![]),
        (libc::SYS_rseq, vec![]),
        (libc::SYS_prctl, vec![]),
        (libc::SYS_sched_getaffinity, vec![]),
        (libc::SYS_sched_yield, vec![]),
        (libc::SYS_getpid, vec![]),
        (libc::SYS_gettid, vec![]),
        (libc::SYS_writev, vec![]),
        (libc::SYS_readv, vec![]),
        (libc::SYS_pwritev, vec![]),
        (libc::SYS_preadv, vec![]),
        (libc::SYS_getrlimit, vec![]),
        (libc::SYS_prlimit64, vec![]),
    ]
    .into_iter()
    .collect();

    #[cfg(target_arch = "x86_64")]
    {
        rules.insert(libc::SYS_arch_prctl, vec![]);
    }

    let arch = target_arch();

    SeccompFilter::new(rules, SeccompAction::KillProcess, SeccompAction::Allow, arch)?
        .try_into()
}

#[cfg(target_os = "linux")]
fn target_arch() -> seccompiler::TargetArch {
    #[cfg(target_arch = "x86_64")]
    {
        seccompiler::TargetArch::x86_64
    }

    #[cfg(target_arch = "aarch64")]
    {
        seccompiler::TargetArch::aarch64
    }
}
