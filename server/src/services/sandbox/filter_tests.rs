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
