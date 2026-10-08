// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use seccompiler::{BpfProgram, SeccompAction, sock_filter};

pub(super) fn with_private_registration_denied(
    program: BpfProgram,
    architecture: u32,
    barrier_syscall: u32,
) -> BpfProgram {
    let words = [
        (4, architecture),
        (0, barrier_syscall),
        (16, 16),
        (20, 0),
        (24, 0),
        (28, 0),
    ];
    let mut prefixed = Vec::with_capacity(program.len() + words.len() * 2 + 1);
    for (index, (offset, expected)) in words.into_iter().enumerate() {
        prefixed.push(sock_filter {
            code: 0x20,
            jt: 0,
            jf: 0,
            k: offset,
        });
        prefixed.push(sock_filter {
            code: 0x15,
            jt: 0,
            jf: ((words.len() - index - 1) * 2 + 1) as u8,
            k: expected,
        });
    }
    prefixed.push(sock_filter {
        code: 0x06,
        jt: 0,
        jf: 0,
        k: SeccompAction::Errno(libc::EPERM as u32).into(),
    });
    prefixed.extend(program);
    prefixed
}
