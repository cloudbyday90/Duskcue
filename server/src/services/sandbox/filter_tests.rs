// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;

fn evaluate(filter: &seccompiler::BpfProgram, syscall: i64, arch: u32) -> u32 {
    let mut index = 0;
    let mut accumulator = 0;
    for _ in 0..filter.len() * 2 {
        let instruction = &filter[index];
        let jump = match instruction.code {
            0x20 => {
                accumulator = match instruction.k {
                    0 => syscall as u32,
                    4 => arch,
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
fn native_file_open_alias_uses_the_existing_path_boundary() {
    let filter = build_ffmpeg_filter().unwrap();
    let arch = super::super::wire::audit_arch();
    for syscall in [libc::SYS_open, libc::SYS_openat] {
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
    assert_eq!(filter.len(), baseline.len() + 5);
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
