// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;

fn publication_fds() -> [u64; 7] {
    let directory = libc::AT_FDCWD as i64 as u64;
    [
        directory,
        0,
        1,
        u64::MAX,
        directory.wrapping_add(1),
        directory as u32 as u64,
        directory ^ 1 << 32,
    ]
}

#[test]
fn file_publication_requires_both_full_native_directory_selectors() {
    let filter = build_ffmpeg_filter().unwrap();
    let architecture = super::super::super::wire::audit_arch();
    for source in publication_fds() {
        for destination in publication_fds() {
            let arguments = [source, u64::MAX, destination, u64::MAX, 0, 0];
            let matched = source == libc::AT_FDCWD as i64 as u64 && source == destination;
            assert_eq!(
                evaluate_arguments(&filter, libc::SYS_renameat, architecture, arguments),
                if matched { 0x7fff_0000 } else { 0x8000_0000 }
            );
            assert_eq!(
                evaluate_arguments(&filter, libc::SYS_renameat, architecture ^ 1, arguments),
                0x8000_0000
            );
            for denied in [
                libc::SYS_renameat2,
                libc::SYS_renameat | 0x4000_0000,
                libc::SYS_execve,
                libc::SYS_execveat,
                libc::SYS_socket,
                libc::SYS_connect,
            ] {
                assert_eq!(
                    evaluate_arguments(&filter, denied, architecture, arguments),
                    0x8000_0000
                );
            }
        }
    }
}

#[test]
fn file_publication_rule_compiles_for_both_native_architectures() {
    for (architecture, target, rename_at) in [
        (0xc000_003e, seccompiler::TargetArch::x86_64, 264),
        (0xc000_00b7, seccompiler::TargetArch::aarch64, 38),
    ] {
        let filter: seccompiler::BpfProgram = seccompiler::SeccompFilter::new(
            std::collections::BTreeMap::from([(
                rename_at,
                vec![files::file_publication_rule().unwrap()],
            )]),
            seccompiler::SeccompAction::KillProcess,
            seccompiler::SeccompAction::Allow,
            target,
        )
        .unwrap()
        .try_into()
        .unwrap();
        for source in publication_fds() {
            for destination in publication_fds() {
                let arguments = [source, 0, destination, 0, u64::MAX, u64::MAX];
                for syscall in 0..1024 {
                    let allowed = syscall == rename_at
                        && source == libc::AT_FDCWD as i64 as u64
                        && source == destination;
                    assert_eq!(
                        evaluate_arguments(&filter, syscall, architecture, arguments),
                        if allowed { 0x7fff_0000 } else { 0x8000_0000 }
                    );
                    assert_eq!(
                        evaluate_arguments(&filter, syscall, architecture ^ 1, arguments),
                        0x8000_0000
                    );
                }
                assert_eq!(
                    evaluate_arguments(&filter, rename_at | 0x4000_0000, architecture, arguments),
                    0x8000_0000
                );
            }
        }
    }
}
