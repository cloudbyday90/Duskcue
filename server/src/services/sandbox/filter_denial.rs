// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use seccompiler::{BpfProgram, SeccompAction, sock_filter};

pub(super) fn with_numa_query_denied(
    program: BpfProgram,
    architecture: u32,
    query_syscall: u32,
) -> BpfProgram {
    let mut prefixed = vec![
        sock_filter {
            code: 0x20,
            jt: 0,
            jf: 0,
            k: 4,
        },
        sock_filter {
            code: 0x15,
            jt: 0,
            jf: 3,
            k: architecture,
        },
        sock_filter {
            code: 0x20,
            jt: 0,
            jf: 0,
            k: 0,
        },
        sock_filter {
            code: 0x15,
            jt: 0,
            jf: 1,
            k: query_syscall,
        },
        sock_filter {
            code: 0x06,
            jt: 0,
            jf: 0,
            k: SeccompAction::Errno(libc::EPERM as u32).into(),
        },
    ];
    prefixed.extend(program);
    prefixed
}
