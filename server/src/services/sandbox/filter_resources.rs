// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use seccompiler::{BackendError, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompRule};

pub(super) fn self_usage_rule() -> Result<SeccompRule, BackendError> {
    SeccompRule::new(vec![SeccompCondition::new(
        0,
        SeccompCmpArgLen::Qword,
        SeccompCmpOp::Eq,
        libc::RUSAGE_SELF as u64,
    )?])
}
