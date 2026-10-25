// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::ImportError;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

pub(super) const MIB: usize = 1024 * 1024;
pub(super) const PLAYLIST: usize = 8 * MIB;
pub(super) const GUIDE: usize = 64 * MIB;
pub(super) const XML: usize = 128 * MIB;
pub(super) const RECORD: usize = 4096;
pub(super) const CHANNELS: usize = 10_000;
pub(super) const PROGRAMMES: usize = 250_000;
pub(super) const EVENTS: usize = 8_000_000;

pub(super) struct Work {
    pub cancel: CancellationToken,
    pub deadline: Instant,
    pub plaintext: usize,
    pub ciphertext: usize,
}

impl Work {
    pub fn check(&self) -> Result<(), ImportError> {
        if self.cancel.is_cancelled() {
            return Err(ImportError::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(ImportError::Deadline);
        }
        Ok(())
    }

    pub fn payload(&mut self, plain: usize, cipher: usize) -> Result<(), ImportError> {
        self.check()?;
        self.plaintext = self
            .plaintext
            .checked_add(plain)
            .ok_or(ImportError::Limit)?;
        self.ciphertext = self
            .ciphertext
            .checked_add(cipher)
            .ok_or(ImportError::Limit)?;
        if self.plaintext > 96 * MIB || self.ciphertext > 144 * MIB {
            return Err(ImportError::Limit);
        }
        Ok(())
    }
}

pub(super) fn text(value: &str, max: usize) -> Result<(), ImportError> {
    if value.len() > max || value.chars().any(|c| !legal(c)) {
        return Err(ImportError::Limit);
    }
    Ok(())
}

pub(super) fn legal(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

pub(super) fn json(value: &serde_json::Value) -> Result<String, ImportError> {
    let value = serde_json::to_string(value).map_err(|_| ImportError::Format)?;
    if value.len() > RECORD {
        return Err(ImportError::Limit);
    }
    Ok(value)
}
