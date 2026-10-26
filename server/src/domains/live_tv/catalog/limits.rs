// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::error::CatalogError;
use std::{io::Write, time::Duration};
use tokio::time::Instant;

pub(crate) const MIB: usize = 1024 * 1024;
pub(crate) const CURSOR: usize = 2048;
pub(crate) const CURSOR_PLAIN: usize = 1024;
pub(crate) const QUERY: usize = 8192;
pub(crate) const WORK: Duration = Duration::from_secs(3);
pub(crate) const RELEASE: Duration = Duration::from_secs(2);

pub(crate) struct Budget {
    pub deadline: Instant,
    cipher: usize,
    plain: usize,
}

impl Budget {
    pub fn new() -> Self {
        Self {
            deadline: Instant::now() + WORK,
            cipher: 0,
            plain: 0,
        }
    }

    pub fn check(&self) -> Result<(), CatalogError> {
        if Instant::now() >= self.deadline {
            Err(CatalogError::Unavailable)
        } else {
            Ok(())
        }
    }

    pub fn cipher(&mut self, bytes: usize) -> Result<(), CatalogError> {
        self.check()?;
        self.cipher = self
            .cipher
            .checked_add(bytes)
            .filter(|n| *n <= 2 * MIB)
            .ok_or(CatalogError::Unavailable)?;
        Ok(())
    }

    pub fn plain(&mut self, bytes: usize) -> Result<(), CatalogError> {
        self.check()?;
        self.plain = self
            .plain
            .checked_add(bytes)
            .filter(|n| *n <= MIB)
            .ok_or(CatalogError::Unavailable)?;
        Ok(())
    }

    pub fn serialize<T: serde::Serialize>(&self, value: &T) -> Result<Vec<u8>, CatalogError> {
        let mut output = Bounded {
            bytes: Vec::new(),
            budget: self,
        };
        serde_json::to_writer(&mut output, value).map_err(|_| CatalogError::Unavailable)?;
        self.check()?;
        Ok(output.bytes)
    }
}

struct Bounded<'a> {
    bytes: Vec<u8>,
    budget: &'a Budget,
}

impl Write for Bounded<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.budget.check().is_err()
            || self
                .bytes
                .len()
                .checked_add(bytes.len())
                .is_none_or(|n| n > MIB)
        {
            return Err(std::io::Error::other("catalog response unavailable"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
