// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[path = "filter_barrier.rs"]
mod barrier;
#[path = "filter_denial.rs"]
mod denial;
#[path = "filter_files.rs"]
mod files;
#[path = "filter_permissions.rs"]
mod permissions;
#[path = "filter_resources.rs"]
mod resources;

pub(super) fn build_ffmpeg_filter() -> Result<seccompiler::BpfProgram, std::io::Error> {
    let program = denial::with_numa_query_denied(
        build_allowlist_filter()?,
        super::wire::audit_arch(),
        libc::SYS_get_mempolicy as u32,
    );
    let program = barrier::with_private_registration_denied(
        program,
        super::wire::audit_arch(),
        libc::SYS_membarrier as u32,
    );
    #[cfg(target_arch = "x86_64")]
    let legacy_chmod = Some(libc::SYS_chmod as u32);
    #[cfg(target_arch = "aarch64")]
    let legacy_chmod = None;
    Ok(permissions::with_cache_permission_change_denied(
        program,
        super::wire::audit_arch(),
        libc::SYS_fchmodat as u32,
        legacy_chmod,
    ))
}

fn build_allowlist_filter() -> Result<seccompiler::BpfProgram, std::io::Error> {
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
        (libc::SYS_ppoll, vec![]),
        (libc::SYS_epoll_create1, vec![]),
        (libc::SYS_epoll_ctl, vec![]),
        (libc::SYS_epoll_pwait, vec![]),
        (libc::SYS_futex, vec![]),
        (libc::SYS_clock_gettime, vec![]),
        (libc::SYS_clock_nanosleep, vec![]),
        (libc::SYS_nanosleep, vec![]),
        (libc::SYS_gettimeofday, vec![]),
        (libc::SYS_ioctl, vec![]),
        (libc::SYS_dup, vec![]),
        (libc::SYS_dup3, vec![]),
        (libc::SYS_pipe2, vec![]),
        (libc::SYS_fcntl, vec![]),
        (libc::SYS_getdents64, vec![]),
        (libc::SYS_faccessat2, vec![]),
        (libc::SYS_faccessat, vec![]),
        (libc::SYS_readlinkat, vec![]),
        (libc::SYS_uname, vec![]),
        (libc::SYS_sysinfo, vec![]),
        (libc::SYS_getrandom, vec![]),
        (libc::SYS_rt_sigaction, vec![]),
        (libc::SYS_rt_sigprocmask, vec![]),
        (libc::SYS_rt_sigreturn, vec![]),
        (libc::SYS_exit, vec![]),
        (libc::SYS_exit_group, vec![]),
        (libc::SYS_clone, vec![]),
        (libc::SYS_set_tid_address, vec![]),
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

    rules.insert(
        libc::SYS_getrusage,
        vec![
            resources::self_usage_rule()
                .map_err(|error| std::io::Error::other(format!("self resource rule: {error}")))?,
        ],
    );
    rules.insert(
        libc::SYS_unlinkat,
        vec![
            files::file_unlink_rule()
                .map_err(|error| std::io::Error::other(format!("file unlink rule: {error}")))?,
        ],
    );
    rules.insert(
        libc::SYS_renameat,
        vec![
            files::file_publication_rule().map_err(|error| {
                std::io::Error::other(format!("file publication rule: {error}"))
            })?,
        ],
    );

    #[cfg(target_arch = "x86_64")]
    {
        rules.insert(libc::SYS_access, vec![]);
        rules.insert(libc::SYS_open, vec![]);
        rules.insert(libc::SYS_stat, vec![]);
        rules.insert(libc::SYS_unlink, vec![]);
        rules.insert(libc::SYS_rename, vec![]);
        rules.insert(libc::SYS_arch_prctl, vec![]);
        rules.insert(libc::SYS_poll, vec![]);
        rules.insert(libc::SYS_epoll_wait, vec![]);
        rules.insert(libc::SYS_dup2, vec![]);
        rules.insert(libc::SYS_readlink, vec![]);
        rules.insert(libc::SYS_fadvise64, vec![]);
    }

    #[cfg(target_arch = "aarch64")]
    {
        const AARCH64_FADVISE64: i64 = 223;
        rules.insert(AARCH64_FADVISE64, vec![]);
    }

    let arch = target_arch();

    SeccompFilter::new(
        rules,
        SeccompAction::KillProcess,
        SeccompAction::Allow,
        arch,
    )
    .map_err(|e| std::io::Error::other(format!("seccomp filter: {e}")))?
    .try_into()
    .map_err(|e| std::io::Error::other(format!("seccomp bpf: {e}")))
}

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

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
