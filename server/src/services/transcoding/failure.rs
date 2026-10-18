// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    Hardware,
    Decode,
    Container,
    Stalled,
    Unknown,
}

#[derive(Default)]
pub(super) struct FailureHints {
    unsafe_to_retry: bool,
    hardware: bool,
    decode: bool,
    container: bool,
}

impl FailureHints {
    pub(super) fn inspect(&mut self, line: &str) {
        self.unsafe_to_retry |= line.len() >= 4096;
        let line = line
            .chars()
            .take(4096)
            .collect::<String>()
            .to_ascii_lowercase();
        self.unsafe_to_retry |= [
            "permission denied",
            "operation not permitted",
            "no space left",
            "cannot allocate memory",
            "read-only file system",
            "read-only filesystem",
            "sandbox",
            "bootstrap",
            "error opening output",
            "error writing",
        ]
        .iter()
        .any(|hint| line.contains(hint));
        self.hardware |= [
            "cannot load libcuda",
            "no nvenc capable devices",
            "openencodesessionex failed",
            "failed to initialise vaapi",
            "failed to initialize vaapi",
            "device creation failed",
            "no device available for decoder",
            "error initializing an internal mfx session",
            "cannot init cuda",
            "could not dynamically load cuda",
            "hwaccel initialisation returned error",
        ]
        .iter()
        .any(|hint| line.contains(hint));
        self.container |= [
            "moov atom not found",
            "ebml header parsing failed",
            "invalid packet size",
            "could not find codec parameters",
        ]
        .iter()
        .any(|hint| line.contains(hint));
        self.decode |= [
            "error while decoding",
            "invalid nal unit",
            "corrupt decoded frame",
            "invalid data found when processing input",
        ]
        .iter()
        .any(|hint| line.contains(hint));
    }

    pub(super) fn classify(&self) -> FailureClass {
        if self.unsafe_to_retry {
            FailureClass::Unknown
        } else if self.hardware {
            FailureClass::Hardware
        } else if self.container {
            FailureClass::Container
        } else if self.decode {
            FailureClass::Decode
        } else {
            FailureClass::Unknown
        }
    }

    pub(super) fn blocks_recovery(&self) -> bool {
        self.unsafe_to_retry
    }
}

#[cfg(test)]
#[path = "failure_tests.rs"]
mod tests;
