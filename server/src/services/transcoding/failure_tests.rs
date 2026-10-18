// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{FailureClass, FailureHints};

#[test]
fn failure_hints_return_only_safe_classes_with_deterministic_priority() {
    let mut hints = FailureHints::default();
    assert_eq!(hints.classify(), FailureClass::Unknown);
    hints.inspect("Error while decoding stream #0:4: Invalid data");
    assert_eq!(hints.classify(), FailureClass::Decode);
    hints.inspect("moov atom not found");
    assert_eq!(hints.classify(), FailureClass::Container);
    hints.inspect("[hevc_nvenc] Cannot load libcuda.so.1");
    assert_eq!(hints.classify(), FailureClass::Hardware);
}

#[test]
fn bounded_hint_inspection_does_not_retain_or_classify_an_unbounded_suffix() {
    let mut hints = FailureHints::default();
    hints.inspect(&format!("{}cannot load libcuda", "x".repeat(4096)));
    assert_eq!(hints.classify(), FailureClass::Unknown);
    hints.inspect("libx264 successfully initialized");
    assert_eq!(hints.classify(), FailureClass::Unknown);
}

#[test]
fn security_output_and_resource_failures_override_retryable_hints() {
    for unsafe_error in [
        "Permission denied",
        "Operation not permitted",
        "No space left on device",
        "Cannot allocate memory",
        "Read-only file system",
        "sandbox bootstrap failed",
        "Error opening output file",
        "Error writing trailer",
    ] {
        let mut hints = FailureHints::default();
        hints.inspect("Cannot load libcuda.so.1; Error while decoding stream");
        hints.inspect(unsafe_error);
        assert_eq!(hints.classify(), FailureClass::Unknown, "{unsafe_error}");
    }
}
