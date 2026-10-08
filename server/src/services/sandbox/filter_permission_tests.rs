// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;

fn permission_arguments() -> Vec<[u64; 6]> {
    let directory = libc::AT_FDCWD as i64 as u64;
    let mut cases = vec![
        [directory, 0, 0o755, 0, 0, 0],
        [directory, u64::MAX, 0o755, u64::MAX, u64::MAX, u64::MAX],
        [directory, 1, 0o644, 0, 0, 0],
        [directory, 1, 0o4755, 0, 0, 0],
        [directory, 1, 0o755 | 1 << 32, 0, 0, 0],
        [directory, 1, u64::MAX, 0, 0, 0],
        [0, 1, 0o755, 0, 0, 0],
        [directory.wrapping_add(1), 1, 0o755, 0, 0, 0],
        [directory as u32 as u64, 1, 0o755, 0, 0, 0],
        [directory ^ 1 << 32, 1, 0o755, 0, 0, 0],
    ];
    for mode in [0o755, 0o644, 0o4755, 0o755 | 1 << 32, u64::MAX] {
        cases.push([0, mode, 0, 0, 0, 0]);
        cases.push([u64::MAX, mode, u64::MAX, u64::MAX, u64::MAX, u64::MAX]);
    }
    cases
}

fn matches_permission_denial(
    syscall: i64,
    chmod_at: u32,
    legacy_chmod: Option<u32>,
    arguments: [u64; 6],
) -> bool {
    syscall == i64::from(chmod_at)
        && arguments[0] == libc::AT_FDCWD as i64 as u64
        && arguments[2] == 0o755
        || legacy_chmod.is_some_and(|number| syscall == i64::from(number)) && arguments[1] == 0o755
}

#[test]
fn optional_permission_maintenance_changes_only_exact_native_denials() {
    let baseline = barrier::with_private_registration_denied(
        denial::with_numa_query_denied(
            build_allowlist_filter().unwrap(),
            super::super::super::wire::audit_arch(),
            libc::SYS_get_mempolicy as u32,
        ),
        super::super::super::wire::audit_arch(),
        libc::SYS_membarrier as u32,
    );
    let filter = build_ffmpeg_filter().unwrap();
    let architecture = super::super::super::wire::audit_arch();
    #[cfg(target_arch = "x86_64")]
    let legacy_chmod = Some(libc::SYS_chmod as u32);
    #[cfg(target_arch = "aarch64")]
    let legacy_chmod = None;
    assert_native_actions(
        &baseline,
        &filter,
        architecture,
        libc::SYS_fchmodat as u32,
        legacy_chmod,
    );
}

#[test]
fn permission_denial_prefix_uses_both_compiled_native_abis() {
    for (architecture, target, chmod_at, legacy_chmod, read) in [
        (
            0xc000_003e,
            seccompiler::TargetArch::x86_64,
            268,
            Some(90),
            0,
        ),
        (0xc000_00b7, seccompiler::TargetArch::aarch64, 53, None, 63),
    ] {
        let baseline: seccompiler::BpfProgram = seccompiler::SeccompFilter::new(
            std::collections::BTreeMap::from([(read, Vec::new())]),
            seccompiler::SeccompAction::KillProcess,
            seccompiler::SeccompAction::Allow,
            target,
        )
        .unwrap()
        .try_into()
        .unwrap();
        let filter = super::super::permissions::with_cache_permission_change_denied(
            baseline.clone(),
            architecture,
            chmod_at,
            legacy_chmod,
        );
        assert_eq!(
            filter.len(),
            baseline.len() + 13 + if legacy_chmod.is_some() { 9 } else { 0 }
        );
        assert_native_actions(&baseline, &filter, architecture, chmod_at, legacy_chmod);
    }
}

fn assert_native_actions(
    baseline: &seccompiler::BpfProgram,
    filter: &seccompiler::BpfProgram,
    architecture: u32,
    chmod_at: u32,
    legacy_chmod: Option<u32>,
) {
    for arguments in permission_arguments() {
        for syscall in 0..1024 {
            let expected = if matches_permission_denial(syscall, chmod_at, legacy_chmod, arguments)
            {
                u32::from(seccompiler::SeccompAction::Errno(libc::EPERM as u32))
            } else {
                evaluate_arguments(baseline, syscall, architecture, arguments)
            };
            assert_eq!(
                evaluate_arguments(filter, syscall, architecture, arguments),
                expected,
                "syscall {syscall}, arguments {arguments:?}"
            );
            assert_eq!(
                evaluate_arguments(filter, syscall, architecture ^ 1, arguments),
                0x8000_0000
            );
        }
        for syscall in [chmod_at, legacy_chmod.unwrap_or(chmod_at)] {
            assert_eq!(
                evaluate_arguments(
                    filter,
                    i64::from(syscall | 0x4000_0000),
                    architecture,
                    arguments
                ),
                0x8000_0000
            );
        }
    }
}
