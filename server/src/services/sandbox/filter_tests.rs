// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;

fn evaluate(filter: &seccompiler::BpfProgram, syscall: i64, arch: u32) -> u32 {
    evaluate_arguments(filter, syscall, arch, [0; 6])
}

fn evaluate_arguments(
    filter: &seccompiler::BpfProgram,
    syscall: i64,
    arch: u32,
    arguments: [u64; 6],
) -> u32 {
    let mut index = 0;
    let mut accumulator = 0;
    for _ in 0..filter.len() * 2 {
        let instruction = &filter[index];
        let jump = match instruction.code {
            0x20 => {
                accumulator = match instruction.k {
                    0 => syscall as u32,
                    4 => arch,
                    offset if (16..=60).contains(&offset) && offset % 4 == 0 => {
                        let offset = (offset - 16) as usize;
                        (arguments[offset / 8] >> ((offset % 8) * 8)) as u32
                    }
                    _ => panic!("unexpected input word"),
                };
                0
            }
            0x15 | 0x25 | 0x35 => {
                let condition = match instruction.code {
                    0x15 => accumulator == instruction.k,
                    0x25 => accumulator > instruction.k,
                    _ => accumulator >= instruction.k,
                };
                usize::from(if condition {
                    instruction.jt
                } else {
                    instruction.jf
                })
            }
            0x05 => instruction.k as usize,
            0x06 => return instruction.k,
            _ => panic!("unexpected filter opcode {}", instruction.code),
        };
        index += jump + 1;
    }
    panic!("filter failed to return");
}

#[test]
fn final_filter_denies_exec_network_and_wrong_architecture() {
    let filter = build_ffmpeg_filter().unwrap();
    let arch = super::super::wire::audit_arch();
    for syscall in [
        libc::SYS_execve,
        libc::SYS_execveat,
        libc::SYS_socket,
        libc::SYS_socketpair,
        libc::SYS_connect,
        libc::SYS_ptrace,
    ] {
        assert_eq!(
            evaluate(&filter, syscall, arch),
            0x8000_0000,
            "syscall {syscall}"
        );
    }
    for syscall in [
        libc::SYS_read,
        libc::SYS_write,
        libc::SYS_openat,
        libc::SYS_futex,
        libc::SYS_prctl,
    ] {
        assert_eq!(
            evaluate(&filter, syscall, arch),
            0x7fff_0000,
            "syscall {syscall}"
        );
    }
    assert_eq!(evaluate(&filter, libc::SYS_read, arch ^ 1), 0x8000_0000);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_file_operation_aliases_preserve_architecture_and_denials() {
    let filter = build_ffmpeg_filter().unwrap();
    let arch = super::super::wire::audit_arch();
    for syscall in [
        libc::SYS_open,
        libc::SYS_openat,
        libc::SYS_access,
        libc::SYS_unlink,
    ] {
        assert_eq!(evaluate(&filter, syscall, arch), 0x7fff_0000);
        assert_eq!(evaluate(&filter, syscall, arch ^ 1), 0x8000_0000);
    }
    for syscall in [libc::SYS_execve, libc::SYS_execveat, libc::SYS_socket] {
        assert_eq!(evaluate(&filter, syscall, arch), 0x8000_0000);
    }
}

#[test]
fn numa_query_fallback_changes_one_denial_and_preserves_all_other_native_actions() {
    let baseline = build_allowlist_filter().unwrap();
    let filter = build_ffmpeg_filter().unwrap();
    let arch = super::super::wire::audit_arch();
    assert_eq!(filter.len(), baseline.len() + 18);
    for syscall in 0..1024 {
        let expected = if syscall == libc::SYS_get_mempolicy {
            u32::from(seccompiler::SeccompAction::Errno(libc::EPERM as u32))
        } else {
            evaluate(&baseline, syscall, arch)
        };
        assert_eq!(
            evaluate(&filter, syscall, arch),
            expected,
            "syscall {syscall}"
        );
        assert_eq!(evaluate(&filter, syscall, arch ^ 1), 0x8000_0000);
    }
    for syscall in [
        libc::SYS_set_mempolicy,
        libc::SYS_mbind,
        libc::SYS_execve,
        libc::SYS_execveat,
        libc::SYS_socket,
        libc::SYS_ptrace,
        libc::SYS_get_mempolicy | 0x4000_0000,
        -1,
    ] {
        assert_eq!(evaluate(&filter, syscall, arch), 0x8000_0000);
    }
}

#[test]
fn numa_denial_prefix_uses_each_compiled_native_architecture_and_syscall_number() {
    for (architecture, target, query, read) in [
        (0xc000_003e, seccompiler::TargetArch::x86_64, 239, 0),
        (0xc000_00b7, seccompiler::TargetArch::aarch64, 236, 63),
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
        let filter = denial::with_numa_query_denied(baseline.clone(), architecture, query);
        for syscall in 0..1024 {
            let expected = if syscall == i64::from(query) {
                u32::from(seccompiler::SeccompAction::Errno(libc::EPERM as u32))
            } else {
                evaluate(&baseline, syscall, architecture)
            };
            assert_eq!(evaluate(&filter, syscall, architecture), expected);
            assert_eq!(evaluate(&filter, syscall, architecture ^ 1), 0x8000_0000);
        }
    }
}

#[test]
fn private_registration_fallback_checks_command_and_flags_preserving_other_actions() {
    let baseline = build_allowlist_filter().unwrap();
    let filter = build_ffmpeg_filter().unwrap();
    let arch = super::super::wire::audit_arch();
    for arguments in registration_arguments() {
        for syscall in 0..1024 {
            let expected = if syscall == libc::SYS_get_mempolicy
                || syscall == libc::SYS_membarrier && arguments[..2] == [16, 0]
            {
                u32::from(seccompiler::SeccompAction::Errno(libc::EPERM as u32))
            } else {
                evaluate_arguments(&baseline, syscall, arch, arguments)
            };
            assert_eq!(
                evaluate_arguments(&filter, syscall, arch, arguments),
                expected
            );
            assert_eq!(
                evaluate_arguments(&filter, syscall, arch ^ 1, arguments),
                0x8000_0000
            );
        }
        assert_eq!(
            evaluate_arguments(&filter, libc::SYS_membarrier | 0x4000_0000, arch, arguments),
            0x8000_0000
        );
    }
}

#[test]
fn private_registration_denial_uses_each_compiled_native_architecture() {
    for (architecture, target, query, registration, read) in [
        (0xc000_003e, seccompiler::TargetArch::x86_64, 239, 324, 0),
        (0xc000_00b7, seccompiler::TargetArch::aarch64, 236, 283, 63),
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
        let numa = denial::with_numa_query_denied(baseline.clone(), architecture, query);
        let filter = barrier::with_private_registration_denied(numa, architecture, registration);
        for arguments in registration_arguments() {
            for syscall in 0..1024 {
                let expected = if syscall == i64::from(query)
                    || syscall == i64::from(registration) && arguments[..2] == [16, 0]
                {
                    u32::from(seccompiler::SeccompAction::Errno(libc::EPERM as u32))
                } else {
                    evaluate_arguments(&baseline, syscall, architecture, arguments)
                };
                assert_eq!(
                    evaluate_arguments(&filter, syscall, architecture, arguments),
                    expected
                );
                assert_eq!(
                    evaluate_arguments(&filter, syscall, architecture ^ 1, arguments),
                    0x8000_0000
                );
            }
        }
    }
}

fn registration_arguments() -> [[u64; 6]; 12] {
    [
        [16, 0, 0, 0, 0, 0],
        [16, 0, 0, u64::MAX, u64::MAX, u64::MAX],
        [0, 0, 0, 0, 0, 0],
        [8, 0, 0, 0, 0, 0],
        [64, 0, 0, 0, 0, 0],
        [16 | 1 << 32, 0, 0, 0, 0, 0],
        [16, 1, 0, 0, 0, 0],
        [16, 1 << 32, 0, 0, 0, 0],
        [16, 0, 1, 0, 0, 0],
        [16, 0, 1 << 32, 0, 0, 0],
        [16, 0, u64::MAX, 0, 0, 0],
        [u64::MAX, u64::MAX, u64::MAX, 0, 0, 0],
    ]
}

#[test]
fn self_resource_getter_requires_the_full_native_selector_and_preserves_denials() {
    let filter = build_ffmpeg_filter().unwrap();
    let architecture = super::super::wire::audit_arch();
    for who in [0, 1, u64::MAX, 1 << 32, 1 << 63] {
        let arguments = [who, u64::MAX, 0, 0, 0, 0];
        assert_eq!(
            evaluate_arguments(&filter, libc::SYS_getrusage, architecture, arguments),
            if who == 0 { 0x7fff_0000 } else { 0x8000_0000 },
        );
        assert_eq!(
            evaluate_arguments(&filter, libc::SYS_getrusage, architecture ^ 1, arguments),
            0x8000_0000
        );
        assert_eq!(
            evaluate_arguments(
                &filter,
                libc::SYS_getrusage | 0x4000_0000,
                architecture,
                arguments
            ),
            0x8000_0000
        );
        for denied in [
            libc::SYS_execve,
            libc::SYS_execveat,
            libc::SYS_socket,
            libc::SYS_ptrace,
        ] {
            assert_eq!(
                evaluate_arguments(&filter, denied, architecture, arguments),
                0x8000_0000
            );
        }
    }
}

#[test]
fn conditioned_self_resource_rule_compiles_for_each_native_architecture() {
    for (architecture, target, usage_syscall, read) in [
        (0xc000_003e, seccompiler::TargetArch::x86_64, 98, 0),
        (0xc000_00b7, seccompiler::TargetArch::aarch64, 165, 63),
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
        let filter: seccompiler::BpfProgram = seccompiler::SeccompFilter::new(
            std::collections::BTreeMap::from([
                (read, Vec::new()),
                (usage_syscall, vec![resources::self_usage_rule().unwrap()]),
            ]),
            seccompiler::SeccompAction::KillProcess,
            seccompiler::SeccompAction::Allow,
            target,
        )
        .unwrap()
        .try_into()
        .unwrap();
        for who in [0, 1, u64::MAX, 1 << 32, 1 << 63] {
            let arguments = [who, u64::MAX, u64::MAX, 0, 0, 0];
            for syscall in 0..1024 {
                let expected = if syscall == usage_syscall && who == 0 {
                    0x7fff_0000
                } else {
                    evaluate_arguments(&baseline, syscall, architecture, arguments)
                };
                assert_eq!(
                    evaluate_arguments(&filter, syscall, architecture, arguments),
                    expected
                );
                assert_eq!(
                    evaluate_arguments(&filter, syscall, architecture ^ 1, arguments),
                    0x8000_0000
                );
            }
        }
    }
}

#[test]
fn owned_thread_exit_preserves_native_architecture_and_other_denials() {
    let filter = build_ffmpeg_filter().unwrap();
    let architecture = super::super::wire::audit_arch();
    for status in [0, 1, 255, u64::MAX, 1 << 32] {
        let arguments = [status, 0, 0, 0, 0, 0];
        for syscall in [libc::SYS_exit, libc::SYS_exit_group] {
            assert_eq!(
                evaluate_arguments(&filter, syscall, architecture, arguments),
                0x7fff_0000
            );
            assert_eq!(
                evaluate_arguments(&filter, syscall, architecture ^ 1, arguments),
                0x8000_0000
            );
            assert_eq!(
                evaluate_arguments(&filter, syscall | 0x4000_0000, architecture, arguments),
                0x8000_0000
            );
        }
        for syscall in [
            libc::SYS_execve,
            libc::SYS_execveat,
            libc::SYS_socket,
            libc::SYS_ptrace,
            libc::SYS_kill,
            libc::SYS_tkill,
        ] {
            assert_eq!(
                evaluate_arguments(&filter, syscall, architecture, arguments),
                0x8000_0000
            );
        }
    }
}

#[test]
fn file_unlink_requires_zero_full_flags_and_preserves_directory_and_other_denials() {
    let filter = build_ffmpeg_filter().unwrap();
    let architecture = super::super::wire::audit_arch();
    for flags in [0, 1, libc::AT_REMOVEDIR as u64, 1 << 32, u64::MAX] {
        let arguments = [libc::AT_FDCWD as u64, u64::MAX, flags, 0, 0, 0];
        assert_eq!(
            evaluate_arguments(&filter, libc::SYS_unlinkat, architecture, arguments),
            if flags == 0 { 0x7fff_0000 } else { 0x8000_0000 }
        );
        assert_eq!(
            evaluate_arguments(&filter, libc::SYS_unlinkat, architecture ^ 1, arguments),
            0x8000_0000
        );
        assert_eq!(
            evaluate_arguments(
                &filter,
                libc::SYS_unlinkat | 0x4000_0000,
                architecture,
                arguments
            ),
            0x8000_0000
        );
        for denied in [
            libc::SYS_execve,
            libc::SYS_execveat,
            libc::SYS_socket,
            libc::SYS_renameat,
        ] {
            assert_eq!(
                evaluate_arguments(&filter, denied, architecture, arguments),
                0x8000_0000
            );
        }
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            evaluate_arguments(&filter, libc::SYS_rmdir, architecture, arguments),
            0x8000_0000
        );
    }
}
