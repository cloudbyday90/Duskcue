// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use seccompiler::{BpfProgram, SeccompAction, sock_filter};

pub(super) fn with_cache_permission_change_denied(
    program: BpfProgram,
    architecture: u32,
    chmod_at: u32,
    legacy_chmod: Option<u32>,
) -> BpfProgram {
    let directory = libc::AT_FDCWD as i64 as u64;
    let program = deny_matching(
        program,
        &[
            (4, architecture),
            (0, chmod_at),
            (16, directory as u32),
            (20, (directory >> 32) as u32),
            (32, 0o755),
            (36, 0),
        ],
    );
    match legacy_chmod {
        Some(syscall) => deny_matching(
            program,
            &[(4, architecture), (0, syscall), (24, 0o755), (28, 0)],
        ),
        None => program,
    }
}

fn deny_matching(program: BpfProgram, words: &[(u32, u32)]) -> BpfProgram {
    let mut prefixed = Vec::with_capacity(program.len() + words.len() * 2 + 1);
    for (index, &(offset, expected)) in words.iter().enumerate() {
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
